//! 盘前报告生成器：采集 → 内联归一化 → 打分 → AI 点评 → 组装 md+json 存盘。
//!
//! 尽力而为：任一数据源失败标注"数据缺失"不中断。写盘路径与 `daily_report::generate_daily_report`
//! 保持一致：`{data_dir}/invest/reports/premarket_{date}.md(+.json)`。
//!
//! Task 6：四因子真实计算 + 结构化 AI 点评。
//! - 舆论/催化：`storage::invest::sentiment::list_sentiment_by_symbol`
//! - 资金：`tushare::moneyflow_dc`（主力净流入）+ `moneyflow_hsgt`（北向）
//! - 技术：`invest::regime::compute_regime_for_symbol`（RSI/MA/趋势）
//! - AI 点评：`invest::event_analyzer::cli_complete` 输出结构化 JSON
//!
//! 补丁 M3：grade **只**来自 `scoring::score()`；AI 输出仅进入 aiCommentary 字段，绝不参与打分。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::invest::premarket::crowding::{crowd_levels_for, CrowdLevel};
use crate::invest::premarket::scoring::{
    get_premarket_config, grade_of, score, AiReview, FactorBreakdown, Grade, PremarketConfig,
    SymbolScore,
};
use crate::invest::premarket::sector_flow::{fetch_sector_flow, SectorFlow};
use crate::storage::invest::macro_cache::{build_macro_snapshot, MacroSnapshot};
use crate::storage::invest::stock_industry;

// ---------------------------------------------------------------------------
// AI 精筛（B3b）：AI keep/drop + 熔断降级 + sections_status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
struct AiDecisions {
    #[serde(default)]
    decisions: Vec<AiDecision>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct AiDecision {
    symbol: String,
    action: String,
    #[serde(default)]
    reason: String,
    #[serde(default, rename = "risk_flag")]
    risk_flag: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiReviewStatus {
    Ok,
    Disabled,
    Failed,
    CircuitBroken,
}

impl AiReviewStatus {
    fn as_wire(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Disabled => "disabled",
            Self::Failed => "failed",
            Self::CircuitBroken => "circuit_broken",
        }
    }
}

/// 最终进池上限（AI 精筛后取前 20 做 rank-cut）。
const AI_REVIEW_FINAL_LIMIT: usize = 20;
/// 候选池上限：量化打分后取 top K 送 AI 精筛。
const AI_REVIEW_TOP_K: usize = 25;
/// 熔断阈值：AI drop 数 >= 此值时回退纯量化 top20。
const AI_REVIEW_DROP_CIRCUIT: usize = 13;
/// AI 精筛 CLI 超时（秒）。
const AI_REVIEW_TIMEOUT_SECS: u64 = 60;

/// 从盘后缓存读最新交易日整批 → 组装 SymbolScore。缓存缺失/过期时先兜底构建一次再读。
async fn collect_scores_from_cache(cfg: &PremarketConfig) -> Vec<SymbolScore> {
    use crate::storage::invest::premarket_cache::{is_fresh, load_latest_cache};
    let today = crate::invest::date_utils::get_invest_date();

    let fresh_cache = match load_latest_cache() {
        Ok(Some((td, rows))) if is_fresh(&td, &today, 4) && !rows.is_empty() => Some(rows),
        _ => None,
    };
    let rows = match fresh_cache {
        Some(r) => r,
        None => {
            log::warn!("[premarket] 缓存缺失/过期,兜底现场构建");
            let _ = crate::invest::premarket::cache_builder::build_cache_for_generation().await;
            match load_latest_cache() {
                Ok(Some((_, r))) => r,
                _ => {
                    log::warn!("[premarket] 兜底后仍无缓存,观察池为空");
                    return vec![];
                }
            }
        }
    };
    rows.into_iter()
        .map(|c| {
            let factors = FactorBreakdown {
                sentiment: c.sentiment,
                capital: c.capital,
                technical: c.technical,
                catalyst: c.catalyst,
                sector_strength: c.sector_strength,
            };
            score(&c.symbol, &c.name, factors, c.missing, cfg)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// AI 精筛：apply_ai_decisions（纯函数）
// ---------------------------------------------------------------------------

/// 把 AI 的 keep/drop 决策应用到量化 top-K 列表上。
///
/// 返回 `(kept, dropped, status)`：
/// - `kept`：被保留的 SymbolScore（ai_review 字段已填充）
/// - `dropped`：被 AI drop 的 SymbolScore
/// - `status`：本次精筛的状态
///
/// 熔断逻辑：drop_count >= `AI_REVIEW_DROP_CIRCUIT` 或全部被 drop → CircuitBroken，
/// 此时所有标的回退为 kept（ai_review=None），dropped 为空。
fn apply_ai_decisions(
    top_k: Vec<SymbolScore>,
    decisions: Vec<AiDecision>,
) -> (Vec<SymbolScore>, Vec<SymbolScore>, AiReviewStatus) {
    if top_k.is_empty() {
        return (vec![], vec![], AiReviewStatus::Ok);
    }

    let mut decision_map: std::collections::HashMap<&str, &AiDecision> =
        std::collections::HashMap::new();
    for d in &decisions {
        decision_map.insert(d.symbol.as_str(), d);
    }

    let mut kept = Vec::new();
    let mut dropped = Vec::new();

    for score in top_k {
        match decision_map.get(score.symbol.as_str()) {
            Some(decision) => {
                let action = decision.action.trim().to_lowercase();
                let risk_flag = if decision.risk_flag.trim().is_empty() {
                    "other".to_string()
                } else {
                    decision.risk_flag.trim().to_string()
                };
                if action == "drop" {
                    dropped.push(score);
                } else {
                    // "keep" or any bad action → keep
                    let mut s = score;
                    s.ai_review = Some(AiReview {
                        action: "keep".to_string(),
                        reason: decision.reason.clone(),
                        risk_flag,
                    });
                    kept.push(s);
                }
            }
            None => {
                // Missing ts_code in decisions → default keep, ai_review=None
                let mut s = score;
                s.ai_review = None;
                kept.push(s);
            }
        }
    }

    // 熔断检查：drop 数 >= 阈值，或 AI 对所有标的都下了 drop
    let drop_count = dropped.len();
    let total_decided = kept.len() + dropped.len();
    // "all drop" = every decision says drop, and at least half the candidates were dropped
    let all_drop = total_decided > 0
        && drop_count == total_decided
        && decisions.iter().all(|d| d.action.trim().to_lowercase() == "drop");
    if drop_count >= AI_REVIEW_DROP_CIRCUIT || all_drop {
        // CircuitBroken：全部回退，ai_review 置 None
        let mut all: Vec<SymbolScore> = kept.into_iter().chain(dropped).collect();
        for s in &mut all {
            s.ai_review = None;
        }
        return (all, vec![], AiReviewStatus::CircuitBroken);
    }

    (kept, dropped, AiReviewStatus::Ok)
}

/// 统计最近 3 天每只标的的舆情命中条数（供 AI 精筛 prompt 附加上下文）。
fn sentiment_hit_map(symbols: &[SymbolScore]) -> std::collections::HashMap<String, u32> {
    let since = (chrono::Local::now() - chrono::Duration::days(3))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let mut map: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for s in symbols {
        let code = s.symbol.split('.').next().unwrap_or(&s.symbol);
        let count = crate::storage::invest::sentiment::list_sentiment_by_symbol(code, &since, 50)
            .map(|v| v.len() as u32)
            .unwrap_or(0);
        map.insert(s.symbol.clone(), count);
    }
    map
}

/// 构建 AI 精筛 prompt：每只标的一行摘要，AI 输出 JSON keep/drop 决策。
fn build_ai_review_prompt(top_k: &[SymbolScore], hit_map: &std::collections::HashMap<String, u32>) -> String {
    let mut lines = Vec::new();
    for s in top_k {
        let hits = hit_map.get(&s.symbol).copied().unwrap_or(0);
        lines.push(format!(
            "{} ({}) | 总分 {:.1} | 情绪{:.0} 资金{:.0} 技术{:.0} 催化{:.0} | 舆情{}条{}",
            s.symbol,
            s.name,
            s.total,
            s.factors.sentiment,
            s.factors.capital,
            s.factors.technical,
            s.factors.catalyst,
            hits,
            if s.missing_factors.is_empty() {
                String::new()
            } else {
                format!(" | 缺:{}", s.missing_factors.join(","))
            }
        ));
    }
    let stock_list = lines.join("\n");
    format!(
        "你是A股盘前量化筛选的 AI 复核员。以下 {count} 只标的是量化打分 top-K 候选。\n\
         请逐只评估，判断是否值得保留进入最终观察池。\n\
         保留标准：基本面无重大利空、近期舆情正面或中性、技术面未处于极端超买。\n\
         剔除标准：存在监管处罚、退市风险、重大利空舆情、技术面极端超买（RSI>85）。\n\n\
         输出严格JSON格式：\n\
         {{\"decisions\":[{{\"symbol\":\"代码\",\"action\":\"keep或drop\",\"reason\":\"一句话\",\"risk_flag\":\"low/medium/high/other\"}}]}}\n\
         只输出JSON，不要解释。\n\n\
         标的列表：\n{stock_list}",
        count = top_k.len()
    )
}

/// 调用 CLI 执行 AI 精筛，解析 JSON 决策，应用到 top-K 列表。
async fn run_ai_review(top_k: Vec<SymbolScore>) -> (Vec<SymbolScore>, Vec<SymbolScore>, AiReviewStatus) {
    if top_k.is_empty() {
        return (vec![], vec![], AiReviewStatus::Ok);
    }

    let hit_map = sentiment_hit_map(&top_k);
    let prompt = build_ai_review_prompt(&top_k, &hit_map);

    let settings = crate::invest::macro_verdict::resolve_settings_path();
    let exec = match crate::invest::committee::cli_executor::CliCommitteeExecutor::global() {
        Some(e) => e,
        None => {
            log::warn!("[premarket] AI review: claude CLI not available, fallback");
            let cleared: Vec<SymbolScore> = top_k.into_iter().map(|mut s| { s.ai_review = None; s }).collect();
            return (cleared, vec![], AiReviewStatus::Failed);
        }
    };

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(AI_REVIEW_TIMEOUT_SECS),
        exec.run_role(
            "你是严谨的A股量化筛选复核员，只输出JSON。",
            &prompt,
            AI_REVIEW_TIMEOUT_SECS,
            settings.as_deref(),
            None,
        ),
    )
    .await;

    let resp = match result {
        Ok(Ok(text)) => text,
        Ok(Err(e)) => {
            log::warn!("[premarket] AI review CLI failed: {e}");
            let cleared: Vec<SymbolScore> = top_k.into_iter().map(|mut s| { s.ai_review = None; s }).collect();
            return (cleared, vec![], AiReviewStatus::Failed);
        }
        Err(_) => {
            log::warn!("[premarket] AI review timed out after {}s", AI_REVIEW_TIMEOUT_SECS);
            let cleared: Vec<SymbolScore> = top_k.into_iter().map(|mut s| { s.ai_review = None; s }).collect();
            return (cleared, vec![], AiReviewStatus::Failed);
        }
    };

    // 解析 JSON（容错：可能被 markdown 包裹）
    let cleaned = resp
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let decisions = match serde_json::from_str::<AiDecisions>(cleaned) {
        Ok(d) => d.decisions,
        Err(e) => {
            log::warn!("[premarket] AI review parse failed: {e}; raw len={}", cleaned.len());
            let cleared: Vec<SymbolScore> = top_k.into_iter().map(|mut s| { s.ai_review = None; s }).collect();
            return (cleared, vec![], AiReviewStatus::Failed);
        }
    };

    apply_ai_decisions(top_k, decisions)
}

/// 用真实 `MacroSnapshot` 字段渲染成中文 md 片段。缺失字段写"—"。
fn render_macro_md(snap: &MacroSnapshot) -> String {
    fn fmt_opt(v: Option<f64>) -> String {
        v.map(|x| format!("{x:.2}")).unwrap_or_else(|| "—".to_string())
    }

    let mut md = String::new();
    md.push_str(&format!(
        "- 上证指数：{}（20 日波动率 {}%）\n",
        fmt_opt(snap.sh_composite_close),
        fmt_opt(snap.sh_composite_vol20)
    ));
    md.push_str(&format!(
        "- 两市成交额：{} 亿\n",
        fmt_opt(snap.two_market_volume)
    ));
    md.push_str(&format!(
        "- 北向资金：{} 亿\n",
        fmt_opt(snap.northbound_net)
    ));
    md.push_str(&format!(
        "- 涨/跌家数：{} / {}\n",
        fmt_opt(snap.advance_count),
        fmt_opt(snap.decline_count)
    ));
    md.push_str(&format!(
        "- 涨停 / 跌停 / 涨幅>3% / 平盘：{} / {} / {} / {}\n",
        fmt_opt(snap.limit_up_count),
        fmt_opt(snap.limit_down_count),
        fmt_opt(snap.up_over_3pct_count),
        fmt_opt(snap.flat_count)
    ));
    md.push_str(&format!(
        "- VIX：{}  |  国际金价：{}\n",
        fmt_opt(snap.vix),
        fmt_opt(snap.gold)
    ));
    md
}

fn render_scores_md(scores: &[SymbolScore]) -> String {
    if scores.is_empty() {
        return "（观察池为空）\n".to_string();
    }
    let mut md = String::new();
    md.push_str("| 标的 | 名称 | 总分 | 评级 | 情绪 | 资金 | 技术 | 催化 | 缺失 |\n");
    md.push_str("|------|------|------|------|------|------|------|------|------|\n");
    for s in scores {
        let missing = if s.missing_factors.is_empty() {
            "—".to_string()
        } else {
            s.missing_factors.join(",")
        };
        md.push_str(&format!(
            "| {} | {} | {:.2} | {:?} | {:.0} | {:.0} | {:.0} | {:.0} | {} |\n",
            s.symbol,
            s.name,
            s.total,
            s.grade,
            s.factors.sentiment,
            s.factors.capital,
            s.factors.technical,
            s.factors.catalyst,
            missing,
        ));
    }
    md
}

// ---------------------------------------------------------------------------
// 四因子真实计算
// ---------------------------------------------------------------------------

/// 舆论 + 催化因子（同一批 sentiment_items 输入，一次查询）。
///
/// - **舆论**：sentiment_hint 均值（-1..1 → 0-100）。
/// - **催化**：3 日内关联条数 × 10，上限 100（10 条即封顶，避免噪声堆积）。
pub(crate) fn compute_sentiment_and_catalyst(code: &str) -> (Option<f64>, Option<f64>) {
    let since = (chrono::Local::now() - chrono::Duration::days(3))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let items = match crate::storage::invest::sentiment::list_sentiment_by_symbol(code, &since, 40)
    {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "[premarket] list_sentiment_by_symbol({code}) failed: {e}; treat as missing"
            );
            return (None, None);
        }
    };
    if items.is_empty() {
        return (None, None);
    }

    // 情绪：只取有 sentiment_hint 的条目均值；全为 None → 情绪缺失（催化仍算）。
    let hints: Vec<f64> = items.iter().filter_map(|i| i.sentiment_hint).collect();
    let sentiment = if hints.is_empty() {
        None
    } else {
        let avg = hints.iter().sum::<f64>() / hints.len() as f64;
        Some(((avg + 1.0) / 2.0 * 100.0).clamp(0.0, 100.0))
    };

    let catalyst = Some((items.len() as f64 * 10.0).min(100.0));
    (sentiment, catalyst)
}

/// 技术因子：从 `compute_regime_for_symbol` 的 regime 字符串 + RSI + 分位合成 0-100。
///
/// - regime 基分：uptrend=75 / range_bound=55 / downtrend=35 / crash=15 / unknown=50。
/// - RSI 修正：50±(rsi-50)×0.4（RSI 极端往中位靠拢，避免超买/超卖时过高分）。
/// - 2年分位加成：分位 <0.2 或 >0.8 时轻微扣分（过热/过冷）。
pub(crate) async fn compute_technical(symbol: &str) -> Option<f64> {
    let client = match crate::tushare::client::TushareClient::from_settings() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[premarket] tushare unavailable for technical({symbol}): {e}");
            return None;
        }
    };
    let result = match crate::invest::regime::compute_regime_for_symbol(&client, symbol).await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("[premarket] compute_regime_for_symbol({symbol}) failed: {e}");
            return None;
        }
    };

    // regime 为 "unknown" 说明数据不足 60 根 K，视为缺失让 grade 走 missing 分支
    if result.regime == "unknown" {
        return None;
    }

    let base = match result.regime {
        "uptrend" => 75.0,
        "range_bound" => 55.0,
        "downtrend" => 35.0,
        "crash" => 15.0,
        _ => 50.0,
    };

    // RSI 拉回中位：base + (rsi-50) * 0.4，但 RSI>70 或 <30 时轻微惩罚
    let rsi = result.metrics.rsi14;
    let rsi_adj = if rsi > 70.0 || rsi < 30.0 {
        -5.0
    } else {
        (rsi - 50.0) * 0.2
    };

    // 极端分位轻微扣分：<0.2 过冷（可能是反弹机会但风险大）,>0.8 过热
    let q = result.metrics.price_quantile_2y;
    let q_adj: f64 = if q > 0.8 {
        -5.0
    } else if q < 0.2 {
        -3.0
    } else {
        0.0
    };

    Some((base + rsi_adj + q_adj).clamp(0.0, 100.0))
}

// ---------------------------------------------------------------------------
// AI 点评（结构化 JSON，不改档）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSector {
    pub name: String,
    pub tag: String,
    pub count: u32,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCommentary {
    pub sectors: Vec<AiSector>,
    pub tone: String,
}

/// 采样最近舆情条目，拼一段供 AI 聚合板块 + 打标签 + 给基调。
///
/// AI 失败或 JSON 解析失败均返回 None；调用方在 md 中显示占位文案，SABC 分级不受影响。
async fn ai_commentary(news_block: &str) -> Option<AiCommentary> {
    if news_block.trim().is_empty() {
        return None;
    }
    let prompt = format!(
        "你是A股盘前分析师。把以下新闻聚合成3-5个板块，每个给：name、tag(只能选:新闻强/催化强/情绪强/分歧大/风险预警)、count、note(一句话)。\
         风险预警专收监管/政策转向/处罚退市/地缘扰动。输出JSON: {{\"sectors\":[...],\"tone\":\"基调总述\"}}。只输出JSON。\n\n{}",
        news_block
    );
    let resp = crate::invest::event_analyzer::cli_complete(
        "你是严谨的金融分析师，只输出JSON。",
        &prompt,
    )
    .await
    .ok()?;
    let cleaned = resp
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    match serde_json::from_str::<AiCommentary>(cleaned) {
        Ok(v) => Some(v),
        Err(e) => {
            log::warn!("[premarket] ai_commentary parse failed: {e}; raw len={}", cleaned.len());
            None
        }
    }
}

/// 从最近 sentiment_items 拼一段新闻文本喂给 AI。上限 120 条，正文截断到 80 字避免 prompt 超长。
fn build_news_block_for_ai() -> String {
    let since = (chrono::Local::now() - chrono::Duration::days(2))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let items = crate::storage::invest::sentiment::list_recent_sentiment(&since, 120)
        .unwrap_or_default();
    let mut buf = String::new();
    for it in items.iter().take(120) {
        let summary = it.summary.as_deref().unwrap_or("");
        let short = summary.chars().take(80).collect::<String>();
        buf.push_str(&format!(
            "- [{}] {} | {}\n",
            it.stance, it.title, short
        ));
    }
    buf
}

fn render_ai_commentary_md(ai: &AiCommentary) -> String {
    let mut md = String::new();
    md.push_str(&format!("**基调**：{}\n\n", ai.tone));
    for s in &ai.sectors {
        md.push_str(&format!(
            "- **{}**（{}，{} 条）：{}\n",
            s.name, s.tag, s.count, s.note
        ));
    }
    md
}

// ---------------------------------------------------------------------------
// 02 段：板块资金流入榜 + 拥挤度雷达
// ---------------------------------------------------------------------------

/// 02 段一条板块记录（写进 json / 前端消费）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectorFlowEntry {
    pub name: String,
    /// 主力净流入 (亿元)
    pub net_inflow: f64,
    /// 板块涨跌幅 (%)
    pub change_pct: Option<f64>,
    /// 板块总成交额 (亿元)
    pub total_turnover: Option<f64>,
    /// 板块内上涨家数
    pub advance_count: Option<i64>,
    /// 板块内下跌家数
    pub decline_count: Option<i64>,
    /// 领涨股名
    pub lead_stock: Option<String>,
    /// 领涨股涨跌幅 (%)
    pub lead_change_pct: Option<f64>,
    /// 拥挤度: "healthy" / "warm" / "hot"
    pub crowd_level: String,
    /// 拥挤度三个 0-1 输入（供前端调试展示）
    pub crowd_inputs: CrowdInputsOut,
    /// 净流入横向条宽度：0-100 相对 Top1 归一
    pub bar_width: f64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrowdInputsOut {
    pub turnover_pct: f64,
    pub volume_share: f64,
    pub divergence: f64,
}

fn crowd_level_str(l: &CrowdLevel) -> &'static str {
    match l {
        CrowdLevel::Healthy => "healthy",
        CrowdLevel::Warm => "warm",
        CrowdLevel::Hot => "hot",
    }
}

/// 板块列表 → 02 段结构。按 `net_inflow` 降序，取 `top_n`。
///
/// `bar_width` 归一：以列表首条（Top1 净流入）为满档 100。首条 <=0 时全部 0。
fn build_sector_flows(sectors: &[SectorFlow], top_n: usize) -> Vec<SectorFlowEntry> {
    if sectors.is_empty() {
        return vec![];
    }
    // sectors 已在 Python 侧按 net_inflow 降序；防御性再排一次
    let mut ordered: Vec<&SectorFlow> = sectors.iter().collect();
    ordered.sort_by(|a, b| {
        b.net_inflow
            .partial_cmp(&a.net_inflow)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // 计算拥挤度用完整分布（用整份 sectors 做分位/成交占比分母）
    let crowd = crowd_levels_for(sectors);
    // idx by name → crowd
    use std::collections::HashMap;
    let by_name: HashMap<&str, (&CrowdLevel, super::crowding::CrowdInputs)> = sectors
        .iter()
        .zip(crowd.iter())
        .map(|(s, (lvl, inp))| (s.name.as_str(), (lvl, *inp)))
        .collect();

    let top_net = ordered.first().map(|s| s.net_inflow).unwrap_or(0.0);

    ordered
        .into_iter()
        .take(top_n)
        .map(|s| {
            let (lvl, inp) = by_name
                .get(s.name.as_str())
                .copied()
                .unwrap_or((&CrowdLevel::Healthy, super::crowding::CrowdInputs {
                    turnover_pct: 0.5,
                    volume_share: 0.0,
                    divergence: 0.0,
                }));
            let bar_width = if top_net > 0.0 {
                (s.net_inflow / top_net * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            };
            SectorFlowEntry {
                name: s.name.clone(),
                net_inflow: s.net_inflow,
                change_pct: s.change_pct,
                total_turnover: s.total_turnover,
                advance_count: s.advance_count,
                decline_count: s.decline_count,
                lead_stock: s.lead_stock.clone(),
                lead_change_pct: s.lead_change_pct,
                crowd_level: crowd_level_str(lvl).to_string(),
                crowd_inputs: CrowdInputsOut {
                    turnover_pct: (inp.turnover_pct * 1000.0).round() / 1000.0,
                    volume_share: (inp.volume_share * 1000.0).round() / 1000.0,
                    divergence: (inp.divergence * 1000.0).round() / 1000.0,
                },
                bar_width: (bar_width * 100.0).round() / 100.0,
                source: s.source.clone(),
            }
        })
        .collect()
}

fn render_sector_flows_md(entries: &[SectorFlowEntry]) -> String {
    if entries.is_empty() {
        return "（当日板块资金流数据缺失）\n".to_string();
    }
    let mut md = String::new();
    md.push_str("| 板块 | 净流入(亿) | 涨跌幅 | 拥挤度 | 领涨股 | 领涨股涨幅 | 上涨/下跌 |\n");
    md.push_str("|------|-----------|--------|--------|--------|-----------|-----------|\n");
    for e in entries {
        let chg = e
            .change_pct
            .map(|v| format!("{v:+.2}%"))
            .unwrap_or_else(|| "—".to_string());
        let lead_chg = e
            .lead_change_pct
            .map(|v| format!("{v:+.2}%"))
            .unwrap_or_else(|| "—".to_string());
        let adv_dec = match (e.advance_count, e.decline_count) {
            (Some(a), Some(d)) => format!("{a}/{d}"),
            _ => "—".to_string(),
        };
        md.push_str(&format!(
            "| {} | {:+.2} | {} | {} | {} | {} | {} |\n",
            e.name,
            e.net_inflow,
            chg,
            e.crowd_level,
            e.lead_stock.as_deref().unwrap_or("—"),
            lead_chg,
            adv_dec,
        ));
    }
    md
}

// ---------------------------------------------------------------------------
// 03 段：主线（行业主题）排序 —— 纯 Rust 聚合
// ---------------------------------------------------------------------------

/// 03 段一条主线记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeEntry {
    pub rank: u32,
    pub name: String,
    /// 池内命中的股票数（同一行业下的观察池标的数）
    pub member_count: u32,
    /// 组内舆论均值 (0-100)
    pub sentiment: f64,
    /// 组内资金均值 (0-100)
    pub capital: f64,
    /// 组内催化均值 (0-100)
    pub catalyst: f64,
    /// 组内技术均值 (0-100) —— 一并给出，避免前端只看到 3 因子
    pub technical: f64,
    /// 板块总分：与 score() 相同的权重公式（sentiment/capital/technical/catalyst）
    pub total: f64,
    /// SABC 评级：直接复用 score() 的阈值
    pub grade: Grade,
    /// 模板化摘要
    pub reason: String,
}

/// 观察池 scores 按 industry 聚合出 03 段主线排序。
///
/// 组内因子取算数平均；板块总分 = 复用 cfg 权重加权。
/// `code_of` 传入函数：给 symbol → 6 位 code 的映射，用来查 stock_industry 表。
fn build_themes(scores: &[SymbolScore], cfg: &PremarketConfig, top_n: usize) -> Vec<ThemeEntry> {
    use std::collections::HashMap;

    // industry_name → 该组的因子累加 + 计数
    #[derive(Default)]
    struct Bucket {
        sent_sum: f64,
        cap_sum: f64,
        tech_sum: f64,
        cat_sum: f64,
        count: u32,
    }

    let mut buckets: HashMap<String, Bucket> = HashMap::new();
    for s in scores {
        // symbol 形如 "600519.SH" → 6 位 code
        let code = s.symbol.split('.').next().unwrap_or(&s.symbol);
        let industry = match stock_industry::industry_of(code) {
            Ok(Some(name)) if !name.trim().is_empty() => name,
            _ => continue, // 无行业映射 → 跳过（不落入"其他"，避免噪声）
        };
        let b = buckets.entry(industry).or_default();
        b.sent_sum += s.factors.sentiment;
        b.cap_sum += s.factors.capital;
        b.tech_sum += s.factors.technical;
        b.cat_sum += s.factors.catalyst;
        b.count += 1;
    }

    let mut entries: Vec<ThemeEntry> = buckets
        .into_iter()
        .map(|(name, b)| {
            let n = b.count as f64;
            let sentiment = b.sent_sum / n;
            let capital = b.cap_sum / n;
            let technical = b.tech_sum / n;
            let catalyst = b.cat_sum / n;
            let total = sentiment * cfg.weight_sentiment
                + capital * cfg.weight_capital
                + technical * cfg.weight_technical
                + catalyst * cfg.weight_catalyst;
            let grade = grade_of(total, cfg);
            let reason = format!(
                "舆论 {:.0} / 资金 {:.0} / 催化 {:.0}, 池内 {} 只",
                sentiment, capital, catalyst, b.count
            );
            ThemeEntry {
                rank: 0, // 后面按 total 降序赋值
                name,
                member_count: b.count,
                sentiment: (sentiment * 100.0).round() / 100.0,
                capital: (capital * 100.0).round() / 100.0,
                catalyst: (catalyst * 100.0).round() / 100.0,
                technical: (technical * 100.0).round() / 100.0,
                total: (total * 100.0).round() / 100.0,
                grade,
                reason,
            }
        })
        .collect();

    entries.sort_by(|a, b| {
        b.total
            .partial_cmp(&a.total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    entries.truncate(top_n);
    for (i, e) in entries.iter_mut().enumerate() {
        e.rank = (i + 1) as u32;
    }
    entries
}

fn render_themes_md(themes: &[ThemeEntry]) -> String {
    if themes.is_empty() {
        return "（观察池无行业映射，主线暂缺）\n".to_string();
    }
    let mut md = String::new();
    md.push_str("| 排名 | 主线 | 池内 | 总分 | 评级 | 舆论 | 资金 | 催化 | 技术 |\n");
    md.push_str("|------|------|------|------|------|------|------|------|------|\n");
    for t in themes {
        md.push_str(&format!(
            "| {} | {} | {} | {:.2} | {:?} | {:.0} | {:.0} | {:.0} | {:.0} |\n",
            t.rank,
            t.name,
            t.member_count,
            t.total,
            t.grade,
            t.sentiment,
            t.capital,
            t.catalyst,
            t.technical,
        ));
    }
    md
}

// ---------------------------------------------------------------------------
// 主入口
// ---------------------------------------------------------------------------

/// 生成盘前观察报告。返回 md 文件绝对路径。
///
/// 时序（CP2）：先跑 Plan A 四源归一化 → 再拉雪球独立通道 → 再读宏观快照 → 再打分。
/// 各段独立降级，绝不因单源失败中断。
///
/// **grade 隔离**：SABC 评级只在 `score()` 内根据总分算出；AI 点评仅进入
/// `aiCommentary`/md 展示层，任何 AI 失败都不影响 scores。
pub async fn generate_premarket_report(data_dir: &Path) -> Result<String, String> {
    // A6: report generated evening before, dated for next trading day
    let today = crate::storage::invest::scheduler::beijing_today();
    let date = crate::storage::invest::scheduler::next_trading_day(&today)?;

    // 1. 采集 + 内联归一化（CP2 时序保证）——含雪球独立通道
    let _ = crate::invest::sentiment::collect_all_sentiment(None, 20).await;
    let _ = crate::invest::sentiment::fetch_xueqiu_market(15).await; // 降级不阻断

    // 2. 宏观快照（真实字段渲染）
    let (macro_md, macro_snapshot) = match build_macro_snapshot() {
        Some(snap) => (render_macro_md(&snap), Some(snap)),
        None => (
            "宏观快照：数据缺失（macro_cache 未初始化）\n".to_string(),
            None,
        ),
    };

    // 3. 股票池 SABC 打分（读盘后缓存,兜底现场构建）
    let cfg: PremarketConfig = get_premarket_config();
    let full_pool: Vec<SymbolScore> = collect_scores_from_cache(&cfg).await;
    let mut sorted_pool = full_pool;
    sorted_pool.sort_by(|a, b| b.total.partial_cmp(&a.total).unwrap_or(std::cmp::Ordering::Equal));
    let top_k: Vec<SymbolScore> = sorted_pool.into_iter().take(AI_REVIEW_TOP_K).collect();

    // B3b: AI 精筛 pass — quant top 25 → AI keep/drop → kept top 20 → rank-cut
    let (kept_after_ai, ai_dropped, ai_status) = if cfg.enable_ai_review {
        run_ai_review(top_k).await
    } else {
        let cleared: Vec<SymbolScore> = top_k
            .into_iter()
            .map(|mut s| {
                s.ai_review = None;
                s
            })
            .collect();
        (cleared, vec![], AiReviewStatus::Disabled)
    };

    let scores: Vec<SymbolScore> = crate::invest::premarket::scoring::assign_grades_by_rank(
        kept_after_ai.into_iter().take(AI_REVIEW_FINAL_LIMIT).collect(),
    );

    // 3.5 板块资金流 + 拥挤度雷达（02 段）—— 尽力而为
    let sector_flows_entries: Vec<SectorFlowEntry> = match fetch_sector_flow().await {
        Ok(rows) if !rows.is_empty() => build_sector_flows(&rows, 10),
        Ok(_) => {
            log::warn!("[premarket] sector_flow returned empty; 02 段 fallback []");
            vec![]
        }
        Err(e) => {
            log::warn!("[premarket] fetch_sector_flow failed: {e}; 02 段 fallback []");
            vec![]
        }
    };
    let sector_flows_md = render_sector_flows_md(&sector_flows_entries);

    // 3.6 主线（行业主题）排序（03 段）—— 纯 Rust 聚合
    let themes: Vec<ThemeEntry> = build_themes(&scores, &cfg, 5);
    let themes_md = render_themes_md(&themes);

    // 4. AI 点评（结构化 JSON；失败 → None，不影响分数）
    let news_block = build_news_block_for_ai();
    let ai = ai_commentary(&news_block).await;
    let ai_md = ai
        .as_ref()
        .map(render_ai_commentary_md)
        .unwrap_or_else(|| "AI 点评生成失败（不影响 SABC 分级）。\n".to_string());

    // 5. 组装 md + json 落盘
    let reports_dir = data_dir.join("invest").join("reports");
    std::fs::create_dir_all(&reports_dir).map_err(|e| format!("mkdir reports: {e}"))?;

    let mut md = String::new();
    md.push_str(&format!("# 盘前观察 {date}\n\n"));
    md.push_str("## 宏观快照\n\n");
    md.push_str(&macro_md);
    md.push('\n');
    md.push_str("## 板块资金流入榜 (Top 10)\n\n");
    md.push_str(&sector_flows_md);
    md.push('\n');
    md.push_str("## 主线排序 (Top 5)\n\n");
    md.push_str(&themes_md);
    md.push('\n');
    md.push_str("## SABC 观察池\n\n");
    md.push_str(&format!("共 {} 标的\n\n", scores.len()));
    md.push_str(&render_scores_md(&scores));
    // B3b: AI 精筛状态通知
    if matches!(ai_status, AiReviewStatus::Failed) {
        md.push_str("\n> AI 精筛失败(不影响选池)。\n");
    } else if matches!(ai_status, AiReviewStatus::CircuitBroken) {
        md.push_str(&format!(
            "\n> AI 精筛熔断（drop 数 >= {}），已回退纯量化 top20。\n",
            AI_REVIEW_DROP_CIRCUIT
        ));
    }
    md.push_str("\n## AI 点评\n\n");
    md.push_str(&ai_md);
    md.push('\n');

    let md_path = reports_dir.join(format!("premarket_{date}.md"));
    std::fs::write(&md_path, &md).map_err(|e| format!("write md: {e}"))?;

    let json_path = reports_dir.join(format!("premarket_{date}.json"));
    let json = serde_json::json!({
        "date": date,
        "macro": macro_snapshot,
        "sectorFlows": sector_flows_entries,
        "themes": themes,
        "scores": scores,
        "config": cfg,
        "aiCommentary": ai,
        "aiDropped": ai_dropped,
        "sectionsStatus": {
            "capitalFlow": if sector_flows_entries.is_empty() { "unavailable" } else { "ok" },
            "aiReview": ai_status.as_wire(),
        },
    });
    std::fs::write(
        &json_path,
        serde_json::to_string_pretty(&json).map_err(|e| format!("serialize json: {e}"))?,
    )
    .map_err(|e| format!("write json: {e}"))?;

    Ok(md_path.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invest::premarket::scoring::FactorBreakdown;

    fn mk(symbol: &str, total: f64) -> SymbolScore {
        SymbolScore {
            symbol: symbol.to_string(),
            name: symbol.to_string(),
            total,
            grade: Grade::C,
            factors: FactorBreakdown {
                sentiment: 50.0,
                capital: 50.0,
                technical: 50.0,
                catalyst: 50.0,
                sector_strength: 50.0,
            },
            missing_factors: vec![],
            ai_review: None,
        }
    }

    #[test]
    fn apply_normal_drop_produces_kept_and_dropped_lists() {
        let top_k = vec![mk("600519.SH", 90.0), mk("000858.SZ", 85.0), mk("601318.SH", 80.0)];
        let decisions = vec![
            AiDecision { symbol: "600519.SH".into(), action: "keep".into(), reason: "ok".into(), risk_flag: "low".into() },
            AiDecision { symbol: "000858.SZ".into(), action: "drop".into(), reason: "overbought".into(), risk_flag: "high".into() },
            AiDecision { symbol: "601318.SH".into(), action: "keep".into(), reason: "fine".into(), risk_flag: "medium".into() },
        ];
        let (kept, dropped, status) = apply_ai_decisions(top_k, decisions);
        assert_eq!(kept.len(), 2);
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].symbol, "000858.SZ");
        assert_eq!(status, AiReviewStatus::Ok);
        // kept items should have ai_review populated
        for s in &kept {
            assert!(s.ai_review.is_some());
            assert_eq!(s.ai_review.as_ref().unwrap().action, "keep");
        }
    }

    #[test]
    fn apply_circuit_breaker_when_drop_ge_13_returns_empty_ai() {
        // 13 drops → circuit breaker
        let top_k: Vec<SymbolScore> = (0..15).map(|i| mk(&format!("S{i:04}"), 100.0 - i as f64)).collect();
        let decisions: Vec<AiDecision> = (0..13)
            .map(|i| AiDecision {
                symbol: format!("S{i:04}"),
                action: "drop".into(),
                reason: "risk".into(),
                risk_flag: "high".into(),
            })
            .collect();
        let (kept, dropped, status) = apply_ai_decisions(top_k, decisions);
        assert_eq!(status, AiReviewStatus::CircuitBroken);
        assert_eq!(dropped.len(), 0);
        assert_eq!(kept.len(), 15);
        // All ai_review should be None on circuit break
        for s in &kept {
            assert!(s.ai_review.is_none());
        }
    }

    #[test]
    fn apply_partial_return_missing_symbols_default_to_keep() {
        let top_k = vec![mk("600519.SH", 90.0), mk("000858.SZ", 85.0)];
        // Only one decision — the other symbol is missing from AI output
        let decisions = vec![AiDecision {
            symbol: "600519.SH".into(),
            action: "drop".into(),
            reason: "bad".into(),
            risk_flag: "high".into(),
        }];
        let (kept, dropped, status) = apply_ai_decisions(top_k, decisions);
        assert_eq!(status, AiReviewStatus::Ok);
        assert_eq!(kept.len(), 1);
        assert_eq!(dropped.len(), 1);
        assert_eq!(kept[0].symbol, "000858.SZ");
        assert!(kept[0].ai_review.is_none()); // missing → default keep, no review
    }

    #[test]
    fn apply_unknown_ts_code_is_ignored() {
        let top_k = vec![mk("600519.SH", 90.0)];
        let decisions = vec![AiDecision {
            symbol: "NONEXIST.XX".into(),
            action: "drop".into(),
            reason: "nope".into(),
            risk_flag: "high".into(),
        }];
        let (kept, dropped, status) = apply_ai_decisions(top_k, decisions);
        assert_eq!(status, AiReviewStatus::Ok);
        assert_eq!(kept.len(), 1);
        assert_eq!(dropped.len(), 0);
        assert_eq!(kept[0].symbol, "600519.SH");
    }

    #[test]
    fn apply_bad_risk_flag_and_bad_action_are_coerced() {
        let top_k = vec![mk("600519.SH", 90.0), mk("000858.SZ", 85.0)];
        let decisions = vec![
            AiDecision {
                symbol: "600519.SH".into(),
                action: "INVALID".into(),
                reason: "test".into(),
                risk_flag: "".into(), // empty → "other"
            },
            AiDecision {
                symbol: "000858.SZ".into(),
                action: "keep".into(),
                reason: "ok".into(),
                risk_flag: "invalid_value".into(), // unknown → kept as-is
            },
        ];
        let (kept, dropped, status) = apply_ai_decisions(top_k, decisions);
        assert_eq!(status, AiReviewStatus::Ok);
        assert_eq!(kept.len(), 2);
        assert_eq!(dropped.len(), 0);
        // bad action → coerced to "keep"
        let r0 = kept[0].ai_review.as_ref().unwrap();
        assert_eq!(r0.action, "keep");
        assert_eq!(r0.risk_flag, "other"); // empty → "other"
        // bad risk_flag → kept as-is (not in the low/medium/high enum)
        let r1 = kept[1].ai_review.as_ref().unwrap();
        assert_eq!(r1.risk_flag, "invalid_value");
    }
}
