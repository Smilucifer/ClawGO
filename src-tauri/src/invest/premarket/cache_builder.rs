//! 盘后 SABC 全市场缓存构建。批量拉全市场 → 粗筛 ≤200 → 逐候选四因子 → 落 premarket_factor_cache。
//! 粗筛+打分逻辑供 cache job 与生成兜底共享,避免两份实现漂移。

use std::collections::HashSet;
use crate::storage::invest::premarket_cache::{save_factor_cache, CachedFactor};
use crate::tushare::client::{DailyBar, MoneyflowDc, TushareClient};
use crate::invest::premarket::report::{compute_sentiment_and_catalyst, compute_technical};
use futures_util::StreamExt;

/// 候选粗筛(纯函数):
/// - 舆情命中股(在 sentiment_symbols 内)优先全保留;
/// - 剩余名额按 pct_chg 降序补齐到 cap;
/// - 返回 (ts_code, pct_chg, amount)。
pub fn select_candidates(
    daily: &[DailyBar],
    sentiment_symbols: &HashSet<String>,
    cap: usize,
) -> Vec<(String, f64, f64)> {
    let code6 = |ts: &str| ts.split('.').next().unwrap_or(ts).to_string();
    let mut hit: Vec<&DailyBar> = Vec::new();
    let mut rest: Vec<&DailyBar> = Vec::new();
    for b in daily {
        if sentiment_symbols.contains(&code6(&b.ts_code)) {
            hit.push(b);
        } else {
            rest.push(b);
        }
    }
    rest.sort_by(|a, b| b.pct_chg.partial_cmp(&a.pct_chg).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(String, f64, f64)> = Vec::with_capacity(cap);
    for b in hit.iter().chain(rest.iter()) {
        if out.len() >= cap {
            break;
        }
        out.push((b.ts_code.clone(), b.pct_chg, b.amount));
    }
    out
}

/// capital 因子(纯函数):**单日** net_amount(万元)tanh 归一到 0-100。
/// ⚠️ 与旧 compute_capital(近5日求和 /5e5)不同,此处是单日值,分母重标为 1e5
/// (=10 亿单日主力净流入,对个股已属很强);tanh(1)=0.76 → ~88 分。None → 50(中性)。
/// 注:旧版还混 30% 北向(moneyflow_hsgt,市场级),批量版按个股摊无意义,已去掉。
pub fn capital_score_from_net(net_amount_wan: Option<f64>) -> f64 {
    match net_amount_wan {
        Some(v) => ((v / 1.0e5).tanh() + 1.0) / 2.0 * 100.0,
        None => 50.0,
    }
}

const CANDIDATE_CAP: usize = 200;
const TECH_CONCURRENCY: usize = 3;
const MAX_LOOKBACK_DAYS: i64 = 7;

/// 从 invest 基准日往前逐日回退,返回首个 daily_market 非空的 (compact日, dash日, daily行)。
/// 盘前/节后今天无数据时靠此回退到最近已收盘交易日。全部为空 → Err。
async fn resolve_recent_trade_date(
    client: &TushareClient,
) -> Result<(String, String, Vec<DailyBar>), String> {
    let base = crate::invest::date_utils::get_invest_naive_date();
    for back in 0..=MAX_LOOKBACK_DAYS {
        let day = base - chrono::Duration::days(back);
        let compact = day.format("%Y%m%d").to_string();
        let daily = client.daily_market(&compact).await?;
        if !daily.is_empty() {
            let dash = day.format("%Y-%m-%d").to_string();
            return Ok((compact, dash, daily));
        }
    }
    Err(format!(
        "resolve_recent_trade_date: 近 {MAX_LOOKBACK_DAYS} 日均无 daily 数据"
    ))
}

/// 盘后缓存构建:批量拉全市场 → 粗筛 ≤200 → 逐候选四因子 → 落表。
/// 返回 (trade_date, 写入行数)。交易日经 resolve_recent_trade_date 回退确定。
pub async fn build_cache() -> Result<(String, usize), String> {
    let client = TushareClient::from_settings()?;

    // 1. 确定最近有数据的交易日 + 全市场 daily(带回退)
    let (td_compact, td_dash, daily) = resolve_recent_trade_date(&client).await?;

    // 2. 批量拉全市场 moneyflow_dc(同一交易日)→ 按 6 位裸码建 net_amount map
    let flow: Vec<MoneyflowDc> = client
        .moneyflow_dc_market(&td_compact)
        .await
        .unwrap_or_else(|e| {
            log::warn!("[cache_builder] moneyflow_dc_market failed: {e}; capital 全缺省");
            vec![]
        });
    let code6 = |ts: &str| ts.split('.').next().unwrap_or(ts).to_string();
    let net_map: std::collections::HashMap<String, Option<f64>> =
        flow.iter().map(|f| (code6(&f.ts_code), f.net_amount)).collect();

    // 3. 近 3 日舆情命中股集合(6 位裸码)
    let since = (chrono::Local::now() - chrono::Duration::days(3))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let sent_items = crate::storage::invest::sentiment::list_recent_sentiment(&since, 500)
        .unwrap_or_default();
    let mut sentiment_symbols: HashSet<String> = HashSet::new();
    for it in &sent_items {
        if let Some(sym) = &it.symbol {
            sentiment_symbols.insert(code6(sym));
        }
    }

    // 4. 粗筛候选
    let candidates = select_candidates(&daily, &sentiment_symbols, CANDIDATE_CAP);
    log::info!("[cache_builder] {} 候选(全市场 {} 行, 舆情命中 {})",
        candidates.len(), daily.len(), sentiment_symbols.len());

    // 5. 批量查名(6 位裸码 → 中文名;查不到回退代码)
    let code_list: Vec<String> = candidates.iter().map(|(ts, _, _)| code6(ts)).collect();
    let name_map = crate::storage::invest::stock_industry::names_of(&code_list)
        .unwrap_or_default();

    // 6. technical(K线)限速慢拉;sentiment/catalyst(本地库)+ capital(查 net_map)同步
    let tech_results: Vec<(String, Option<f64>)> = futures_util::stream::iter(
        candidates.iter().map(|(ts, _, _)| {
            let ts = ts.clone();
            async move { (ts.clone(), compute_technical(&ts).await) }
        }),
    )
    .buffer_unordered(TECH_CONCURRENCY)
    .collect()
    .await;
    let tech_map: std::collections::HashMap<String, Option<f64>> =
        tech_results.into_iter().collect();

    let mut rows: Vec<CachedFactor> = Vec::with_capacity(candidates.len());
    for (ts, pct, amount) in &candidates {
        let c6 = code6(ts);
        let (sent_opt, cat_opt) = compute_sentiment_and_catalyst(&c6);
        let mut missing = Vec::new();
        let sentiment = sent_opt.unwrap_or_else(|| { missing.push("sentiment".into()); 50.0 });
        let catalyst = cat_opt.unwrap_or_else(|| { missing.push("catalyst".into()); 50.0 });
        let capital = match net_map.get(&c6) {
            Some(net) => capital_score_from_net(*net),
            None => { missing.push("capital".into()); 50.0 }
        };
        let technical = match tech_map.get(ts).and_then(|o| *o) {
            Some(t) => t,
            None => { missing.push("technical".into()); 50.0 }
        };
        // 名称:stock_industry 查表,查不到回退代码
        let name = name_map.get(&c6).cloned().unwrap_or_else(|| ts.clone());
        rows.push(CachedFactor {
            symbol: ts.clone(),
            name,
            change_pct: *pct,
            amount: *amount,
            sentiment, capital, technical, catalyst,
            missing,
        });
    }

    // 7. 落表(缓存 key = td_dash,与生成侧读取口径一致)
    save_factor_cache(&td_dash, &rows)?;
    Ok((td_dash, rows.len()))
}

/// 生成兜底:缓存缺失/过期时现场构建一次。忽略返回值,失败不阻断报告。
pub async fn build_cache_for_generation() -> Result<(), String> {
    build_cache().await.map(|(td, n)| {
        log::info!("[cache_builder] 兜底构建完成 {td}: {n} 行");
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(ts: &str, pct: f64) -> DailyBar {
        DailyBar {
            ts_code: ts.into(), trade_date: "20260708".into(),
            open: 0.0, high: 0.0, low: 0.0, close: 0.0, pre_close: 0.0,
            change: 0.0, pct_chg: pct, vol: 0.0, amount: 0.0,
        }
    }

    #[test]
    fn select_prioritizes_sentiment_hits_then_fills_by_pct() {
        let daily = vec![bar("600000.SH", 1.0), bar("600001.SH", 9.0), bar("600002.SH", 5.0)];
        let mut hits = HashSet::new();
        hits.insert("600000".to_string()); // 低涨幅但命中舆情,须保留
        let out = select_candidates(&daily, &hits, 2);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "600000.SH");     // 命中优先
        assert_eq!(out[1].0, "600001.SH");     // 剩余按涨幅降序 → 9.0 先
    }

    #[test]
    fn capital_score_maps_net_to_range() {
        assert_eq!(capital_score_from_net(None), 50.0);
        assert!((capital_score_from_net(Some(0.0)) - 50.0).abs() < 0.01);
        assert!(capital_score_from_net(Some(1.0e5)) > 85.0);   // 10亿单日 → ~88
        assert!(capital_score_from_net(Some(-1.0e5)) < 15.0);  // 对称
    }
}
