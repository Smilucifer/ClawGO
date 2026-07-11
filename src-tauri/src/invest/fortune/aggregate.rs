//! 每日盈记 point-in-time 单趟聚合。
//!
//! 一趟 O(N) 同时算每日预测分/盘后分，产出分析、总览、数据摘要。
use std::collections::HashMap;
use chrono::Datelike;
use crate::invest::fortune::calendar::{ganzhi, ganzhi_index, STEMS, BRANCHES};
use crate::invest::fortune::stats::{LayerStat, layer_score, composite_from_layers, fortune_level, FortuneLevel};
use crate::storage::invest::fortune::{DailyReturn, list_returns};

/// 单个层级累加器（天干或地支的一个值）。
#[derive(Default, Clone, Copy)]
struct Acc { sum: f64, days: u32, wins: u32 }
impl Acc {
    fn push(&mut self, ret: f64) {
        self.sum += ret; self.days += 1;
        if ret > 0.0 { self.wins += 1; }   // 胜率口径：仅正收益算赢
    }
    fn to_layer_stat(self) -> LayerStat {
        if self.days == 0 { return LayerStat { avg_return_pct: 0.0, win_rate: 0.5, sample: 0 }; }
        LayerStat {
            avg_return_pct: self.sum / self.days as f64,
            win_rate: self.wins as f64 / self.days as f64,
            sample: self.days,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayScore {
    pub date: String,
    pub stem: String,
    pub branch: String,
    pub predict_score: f64,
    pub predict_level: FortuneLevel,
    pub actual_return: Option<f64>,       // None = 未录（预测态）
    pub post_score: Option<f64>,          // None = 未录
    pub post_level: Option<FortuneLevel>,
    pub is_trading_day: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerRow {
    pub name: String,                     // 干支中文名
    pub avg_return: f64, pub win_rate: f64, pub sample: u32, pub score: f64,
    pub level: FortuneLevel,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub today: Option<DayScore>,          // None = 无任何数据（空状态）
    pub tomorrow: Option<DayScore>,
    pub calendar: Vec<DayScore>,          // 全量历史 + 当月未来预告格
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastItem {
    pub label: String,          // "最强天干" 等
    pub date: String, pub weekday: String,
    pub ganzhi: String, pub score: f64, pub level: FortuneLevel,
    pub is_strong: bool,        // true 红左边，false 绿左边
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub stems: Vec<LayerRow>,   // 10 天干
    pub branches: Vec<LayerRow>,// 12 地支
    pub forecasts: Vec<ForecastItem>,  // 4 路（最强/最弱 天干/地支）
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthStat { pub month: String, pub avg_return: f64 }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSummary {
    pub total_days: u32, pub win_days: u32, pub win_rate: f64,
    pub cumulative_return: f64, pub avg_daily_return: f64,
    pub top_stems: Vec<LayerRow>,   // Top3 排行（分数降序）
    pub top_branches: Vec<LayerRow>,
    pub risk_stems: Vec<LayerRow>,  // Top3 风险（分数升序）
    pub risk_branches: Vec<LayerRow>,
    pub monthly: Vec<MonthStat>,
}

/// 全量聚合结果（扫完所有记录后的终态累加器 + 每日预测/盘后分）。
struct Aggregation {
    stem_final: [Acc; 10],
    branch_final: [Acc; 12],
    day_scores: Vec<DayScore>,   // 每条记录一天，含预测/盘后
}

fn parse_ymd(date: &str) -> Option<(i64, i64, i64)> {
    let p: Vec<&str> = date.split('-').collect();
    if p.len() != 3 { return None; }
    Some((p[0].parse().ok()?, p[1].parse().ok()?, p[2].parse().ok()?))
}

fn score_for(stems: &[Acc; 10], branches: &[Acc; 12], si: usize, bi: usize) -> f64 {
    let s = layer_score(&stems[si].to_layer_stat());
    let b = layer_score(&branches[bi].to_layer_stat());
    composite_from_layers(s, b)
}

/// 单趟 point-in-time：records 按 date 升序。扫到 D 先拍预测快照，再并入 D → 盘后。
fn aggregate_pit(records: &[DailyReturn]) -> Aggregation {
    let mut stems = [Acc::default(); 10];
    let mut branches = [Acc::default(); 12];
    let mut day_scores = Vec::with_capacity(records.len());
    for rec in records {
        let Some((y, m, d)) = parse_ymd(&rec.date) else { continue };
        let idx = ganzhi_index(y, m, d);
        let (si, bi) = (idx % 10, idx % 12);
        let (stem, branch) = ganzhi(y, m, d);
        let predict = score_for(&stems, &branches, si, bi);
        stems[si].push(rec.return_pct);
        branches[bi].push(rec.return_pct);
        let post = score_for(&stems, &branches, si, bi);
        day_scores.push(DayScore {
            date: rec.date.clone(), stem: stem.to_string(), branch: branch.to_string(),
            predict_score: predict, predict_level: fortune_level(predict),
            actual_return: Some(rec.return_pct),
            post_score: Some(post), post_level: Some(fortune_level(post)),
            is_trading_day: true,
        });
    }
    Aggregation { stem_final: stems, branch_final: branches, day_scores }
}

/// 用当前全量累加器给任意日期（含未来）算预测态 DayScore。
fn predict_day(agg: &Aggregation, date: &str) -> Option<DayScore> {
    let (y, m, d) = parse_ymd(date)?;
    let idx = ganzhi_index(y, m, d);
    let (stem, branch) = ganzhi(y, m, d);
    let sc = score_for(&agg.stem_final, &agg.branch_final, idx % 10, idx % 12);
    Some(DayScore {
        date: date.to_string(), stem: stem.to_string(), branch: branch.to_string(),
        predict_score: sc, predict_level: fortune_level(sc),
        actual_return: None, post_score: None, post_level: None,
        is_trading_day: crate::storage::invest::scheduler::is_trading_day(date).unwrap_or(true),
    })
}

/// 枚举 [start_ym, end_ym] 每个自然日，产出完整日历（三态齐全）。
fn build_calendar(agg: &Aggregation, first_date: &str, today: &str) -> Vec<DayScore> {
    let recorded: HashMap<&str, &DayScore> =
        agg.day_scores.iter().map(|d| (d.date.as_str(), d)).collect();
    let (fy, fm, _) = parse_ymd(first_date).unwrap_or_else(|| parse_ymd(today).unwrap());
    let (ty, tm, _) = parse_ymd(today).unwrap();
    let mut out = Vec::new();
    let (mut y, mut m) = (fy, fm);
    // 逐月推进到当月（含）
    while (y, m) <= (ty, tm) {
        let days_in = chrono::NaiveDate::from_ymd_opt((y + (m == 12) as i64) as i32,
            (m % 12 + 1) as u32, 1).unwrap()
            .pred_opt().unwrap().day();
        for d in 1..=days_in {
            let date = format!("{y:04}-{m:02}-{d:02}");
            if let Some(rec) = recorded.get(date.as_str()) {
                out.push((*rec).clone());               // 盘后态
            } else if let Some(p) = predict_day(agg, &date) {
                out.push(p);                            // 预测/休市态
            }
        }
        if m == 12 { y += 1; m = 1; } else { m += 1; }
    }
    out
}

use crate::storage::invest::scheduler::{beijing_today, next_trading_day, is_trading_day};

pub fn compute_analysis() -> Result<Analysis, String> {
    let returns = list_returns()?;
    if returns.is_empty() {
        return Ok(Analysis { today: None, tomorrow: None, calendar: vec![] });
    }
    let agg = aggregate_pit(&returns);
    let today = beijing_today();
    let today_card = agg.day_scores.iter().find(|d| d.date == today).cloned()
        .or_else(|| predict_day(&agg, &today));
    let tomorrow_card = next_trading_day(&today).ok().and_then(|nd| predict_day(&agg, &nd));
    // returns 已按 date 升序 → 首条即最早
    let first = returns.first().map(|r| r.date.as_str()).unwrap_or(today.as_str());
    let calendar = build_calendar(&agg, first, &today);
    Ok(Analysis { today: today_card, tomorrow: tomorrow_card, calendar })
}

fn layer_row(name: &str, acc: Acc) -> LayerRow {
    let stat = acc.to_layer_stat();
    let score = layer_score(&stat);
    LayerRow { name: name.to_string(), avg_return: stat.avg_return_pct,
        win_rate: stat.win_rate, sample: stat.sample, score, level: fortune_level(score) }
}

/// 从 today+1 起向后扫最多 60 个自然日，找首个 stem_idx(或 branch_idx) 命中且为交易日的日期。
fn next_date_with(is_stem: bool, target_idx: usize, from: &str) -> Option<(String, String)> {
    let (mut y, mut m, mut d) = parse_ymd(from)?;
    for _ in 0..60 {
        // 前进一天（借 chrono）
        let nd = chrono::NaiveDate::from_ymd_opt(y as i32, m as u32, d as u32)?
            .succ_opt()?;
        y = nd.year() as i64; m = nd.month() as i64; d = nd.day() as i64;
        let date = format!("{y:04}-{m:02}-{d:02}");
        let idx = ganzhi_index(y, m, d);
        let hit = if is_stem { idx % 10 == target_idx } else { idx % 12 == target_idx };
        if hit && is_trading_day(&date).unwrap_or(true) {
            let wd = ["周一","周二","周三","周四","周五","周六","周日"]
                [nd.weekday().num_days_from_monday() as usize];
            return Some((date, wd.to_string()));
        }
    }
    None
}

pub fn compute_overview() -> Result<Overview, String> {
    let returns = list_returns()?;
    let agg = aggregate_pit(&returns);
    let stems: Vec<LayerRow> = (0..10).map(|i| layer_row(STEMS[i], agg.stem_final[i])).collect();
    let branches: Vec<LayerRow> = (0..12).map(|i| layer_row(BRANCHES[i], agg.branch_final[i])).collect();

    let today = beijing_today();
    let mut forecasts = Vec::new();
    // 仅在有数据时给预告；用分数排名找最强/最弱 idx
    let pick = |rows: &[LayerRow]| -> (usize, usize) {
        let mut hi = 0; let mut lo = 0;
        for (i, r) in rows.iter().enumerate() {
            if r.score > rows[hi].score { hi = i; }
            if r.score < rows[lo].score { lo = i; }
        }
        (hi, lo)
    };
    if !returns.is_empty() {
        let (sh, sl) = pick(&stems);
        let (bh, bl) = pick(&branches);
        let specs: [(&str, bool, usize, bool); 4] = [
            ("最强天干", true, sh, true), ("最弱天干", true, sl, false),
            ("最强地支", false, bh, true), ("最弱地支", false, bl, false),
        ];
        for (label, is_stem, idx, strong) in specs {
            if let Some((date, wd)) = next_date_with(is_stem, idx, &today) {
                let (y, m, d) = parse_ymd(&date).unwrap();
                let gi = ganzhi_index(y, m, d);
                let sc = score_for(&agg.stem_final, &agg.branch_final, gi % 10, gi % 12);
                forecasts.push(ForecastItem {
                    label: label.into(), date, weekday: wd,
                    ganzhi: format!("{}{}", STEMS[gi % 10], BRANCHES[gi % 12]),
                    score: sc, level: fortune_level(sc), is_strong: strong,
                });
            }
        }
    }
    Ok(Overview { stems, branches, forecasts })
}

pub fn compute_data_summary() -> Result<DataSummary, String> {
    let returns = list_returns()?;
    let agg = aggregate_pit(&returns);
    let total_days = returns.len() as u32;
    let win_days = returns.iter().filter(|r| r.return_pct > 0.0).count() as u32;
    let win_rate = if total_days > 0 { win_days as f64 / total_days as f64 } else { 0.0 };
    let cumulative_return: f64 = returns.iter().map(|r| r.return_pct).sum();
    let avg_daily_return = if total_days > 0 { cumulative_return / total_days as f64 } else { 0.0 };

    let mut stems: Vec<LayerRow> = (0..10).map(|i| layer_row(STEMS[i], agg.stem_final[i]))
        .filter(|r| r.sample > 0).collect();
    let mut branches: Vec<LayerRow> = (0..12).map(|i| layer_row(BRANCHES[i], agg.branch_final[i]))
        .filter(|r| r.sample > 0).collect();
    stems.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    branches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    let top_stems = stems.iter().take(3).cloned().collect();
    let top_branches = branches.iter().take(3).cloned().collect();
    let risk_stems = stems.iter().rev().take(3).cloned().collect();
    let risk_branches = branches.iter().rev().take(3).cloned().collect();

    // 月度：按 "YYYY-MM" 分组平均
    let mut by_month: std::collections::BTreeMap<String, (f64, u32)> = Default::default();
    for r in &returns {
        if r.date.len() >= 7 {
            let e = by_month.entry(r.date[..7].to_string()).or_default();
            e.0 += r.return_pct; e.1 += 1;
        }
    }
    let monthly = by_month.into_iter()
        .map(|(month, (sum, n))| MonthStat { month, avg_return: sum / n as f64 })
        .collect();

    Ok(DataSummary { total_days, win_days, win_rate, cumulative_return, avg_daily_return,
        top_stems, top_branches, risk_stems, risk_branches, monthly })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dr(date: &str, ret: f64) -> DailyReturn {
        DailyReturn { date: date.into(), return_pct: ret, note: String::new(),
            created_at: String::new(), updated_at: String::new() }
    }

    #[test]
    fn pit_predict_then_post_for_same_ganzhi() {
        // 同一干支两天：第一天预测=中性锚（无历史），第二天预测=纳入第一天后的分。
        // 2026-07-01 与 2026-08-30 都是乙酉（间隔 60 天）。构造两条乙酉记录。
        let recs = vec![dr("2026-07-01", 1.0), dr("2026-08-30", -0.5)];
        let agg = aggregate_pit(&recs);
        // 第一条：预测用空累加器 → 综合分 = 中性锚 56.9（层分均 50）
        assert!((agg.day_scores[0].predict_score - 56.9).abs() < 0.5);
        // 第一条盘后并入 +1.0 后，同干支层分升高 → 第二条预测应高于中性
        assert!(agg.day_scores[1].predict_score > 56.9);
    }

    #[test]
    fn win_rate_excludes_flat() {
        // 平盘(=0)不算赢：一条 0.0 收益 → wins=0, days=1
        let mut a = Acc::default();
        a.push(0.0);
        assert_eq!(a.wins, 0);
        assert_eq!(a.days, 1);
        a.push(0.1);
        assert_eq!(a.wins, 1);
    }

    #[test]
    fn empty_returns_neutral() {
        let s = Acc::default().to_layer_stat();
        assert_eq!(s.win_rate, 0.5);
        assert_eq!(s.sample, 0);
    }
}
