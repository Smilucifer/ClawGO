//! 盘后 SABC 全市场缓存构建。批量拉全市场 → 粗筛 ≤200 → 逐候选四因子 → 落 premarket_factor_cache。
//! 粗筛+打分逻辑供 cache job 与生成兜底共享,避免两份实现漂移。

use std::collections::HashSet;
use crate::storage::invest::premarket_cache::CachedFactor;
use crate::tushare::client::{DailyBar, MoneyflowDc, TushareClient};

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
