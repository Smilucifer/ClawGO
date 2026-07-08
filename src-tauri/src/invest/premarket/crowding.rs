//! 拥挤度雷达：换手率分位 + 成交占比 + 龙头背离 → 健康/偏热/过热。
//!
//! # 一期尽力而为口径 (spec §3.2)
//!
//! 输入原本设想三条独立通道，但当前 THS `summary` 接口不返回板块换手率历史分位，
//! 因此一期只做**资金拥挤度**维度：
//!
//! - `volume_share` **真实**：该板块 total_turnover / Σ total_turnover。
//! - `divergence` **真实近似**：领涨股大涨但板块内上涨家数占比低 → 涨势由极少数龙头拉起。
//!   `divergence = clamp((lead_change_pct / 10) - advance_ratio, 0, 1)`，
//!   其中 `advance_ratio = advance / (advance + decline)`。
//!   领涨股 10% → 视作满档 "1"；若整体上涨面 40%，则 divergence=0.6。
//! - `turnover_pct` **降级**：THS summary 无板块换手率历史分位。用板块当日涨跌幅
//!   在 90 板块内的排名分位（`change_pct` 越靠前分位越高）作为替身：
//!   拥挤热点往往涨幅居前 + 成交占比高 + 龙头背离，三条信号互补。
//!   下调该维度权重（0.25，原 0.40），让真正扎实的 volume_share/divergence 主导。

use serde::{Deserialize, Serialize};

use super::sector_flow::SectorFlow;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CrowdLevel {
    Healthy,
    Warm,
    Hot,
}

/// 三指标合成拥挤度。各指标已归一到 0-1。
///
/// - `turnover_pct` : 换手率历史分位（一期降级为板块涨幅相对分位）
/// - `volume_share` : 板块成交占板块合计比
/// - `divergence`   : 龙头/板块背离度
///
/// 权重：volume_share 0.45 + divergence 0.30 + turnover_pct 0.25（换手降级，权重下调）。
pub fn crowd_level(turnover_pct: f64, volume_share: f64, divergence: f64) -> CrowdLevel {
    // volume_share 单个板块 15% 已经算相当集中 (90 板块理论均值 ~1.1%)，
    // 以 0.15 为满档避免半导体级异常值再压缩其它维度权重。
    let score = turnover_pct * 0.25
        + (volume_share / 0.15).min(1.0) * 0.45
        + divergence * 0.30;
    if score >= 0.70 {
        CrowdLevel::Hot
    } else if score >= 0.50 {
        CrowdLevel::Warm
    } else {
        CrowdLevel::Healthy
    }
}

/// 单板块拥挤度输入。0-1 归一。
#[derive(Debug, Clone, Copy)]
pub struct CrowdInputs {
    pub turnover_pct: f64,
    pub volume_share: f64,
    pub divergence: f64,
}

/// 从 SectorFlow 快照批量合成三指标输入。
///
/// - `total_market_turnover`：所有板块 `total_turnover` 之和（Σ Σ 亿元），volume_share 的分母。
///   若为 0（数据缺失）则该板块 volume_share = 0。
/// - `change_pct_rank_pct`：调用方预先算好的按 change_pct 排名分位（0-1，越高越热）。
///   None 时该板块 turnover_pct = 0.5 中性。
///
/// divergence 数据缺失（lead_change_pct / advance / decline 任一 None）→ 0。
pub fn compute_crowd_inputs(
    s: &SectorFlow,
    total_market_turnover: f64,
    change_pct_rank_pct: Option<f64>,
) -> CrowdInputs {
    // volume_share
    let volume_share = match s.total_turnover {
        Some(v) if total_market_turnover > 0.0 => (v / total_market_turnover).clamp(0.0, 1.0),
        _ => 0.0,
    };

    // divergence：领涨股大涨 vs 板块内上涨面
    let divergence = match (s.lead_change_pct, s.advance_count, s.decline_count) {
        (Some(lead), Some(adv), Some(dec)) if (adv + dec) > 0 => {
            let advance_ratio = adv as f64 / (adv + dec) as f64;
            let lead_norm = (lead / 10.0).clamp(0.0, 1.0); // 领涨股 10%+ 视作满档
            (lead_norm - advance_ratio).clamp(0.0, 1.0)
        }
        _ => 0.0,
    };

    // turnover_pct：降级为 change_pct 相对分位
    let turnover_pct = change_pct_rank_pct.unwrap_or(0.5).clamp(0.0, 1.0);

    CrowdInputs {
        turnover_pct,
        volume_share,
        divergence,
    }
}

/// 一步到位：给一批板块打拥挤度。返回 `Vec<(index, CrowdLevel, CrowdInputs)>`
/// 与入参顺序对齐，方便调用方直接 zip 回写。
///
/// change_pct 的相对分位由本函数一次算出（按升序 rank）。
pub fn crowd_levels_for(sectors: &[SectorFlow]) -> Vec<(CrowdLevel, CrowdInputs)> {
    if sectors.is_empty() {
        return vec![];
    }
    let total_market: f64 = sectors.iter().filter_map(|s| s.total_turnover).sum();

    // change_pct 升序排名 → 分位 (0..1)。缺失 change_pct 的板块给 0.5 中性。
    // 用 (index, change_pct) 双列排序稳定：涨幅小的分位低。
    let mut idx_with_chg: Vec<(usize, f64)> = sectors
        .iter()
        .enumerate()
        .map(|(i, s)| (i, s.change_pct.unwrap_or(f64::NEG_INFINITY)))
        .collect();
    idx_with_chg.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut rank_pct = vec![0.5f64; sectors.len()];
    let n = sectors.len() as f64;
    for (rank, (orig_idx, chg)) in idx_with_chg.into_iter().enumerate() {
        if chg == f64::NEG_INFINITY {
            rank_pct[orig_idx] = 0.5;
        } else if n > 1.0 {
            rank_pct[orig_idx] = rank as f64 / (n - 1.0);
        } else {
            rank_pct[orig_idx] = 0.5;
        }
    }

    sectors
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let inputs = compute_crowd_inputs(s, total_market, Some(rank_pct[i]));
            let level = crowd_level(inputs.turnover_pct, inputs.volume_share, inputs.divergence);
            (level, inputs)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(name: &str, net: f64, chg: f64, tt: f64, adv: i64, dec: i64, lead: f64) -> SectorFlow {
        SectorFlow {
            name: name.into(),
            net_inflow: net,
            change_pct: Some(chg),
            turnover_rate: None,
            main_inflow_pct: None,
            total_turnover: Some(tt),
            total_volume: None,
            advance_count: Some(adv),
            decline_count: Some(dec),
            lead_stock: Some("龙头".into()),
            lead_change_pct: Some(lead),
            source: "ths_summary".into(),
        }
    }

    #[test]
    fn test_crowd_healthy() {
        assert!(matches!(crowd_level(0.3, 0.05, 0.0), CrowdLevel::Healthy));
    }

    #[test]
    fn test_crowd_hot() {
        // 高排名分位 + 高成交占比 + 强背离 → 过热
        assert!(matches!(crowd_level(0.95, 0.20, 0.8), CrowdLevel::Hot));
    }

    #[test]
    fn test_divergence_pure() {
        // 领涨股 10%涨幅 + 上涨面 30% → divergence = 1 - 0.3 = 0.7
        let s = mk("x", 5.0, 2.0, 100.0, 30, 70, 10.0);
        let inp = compute_crowd_inputs(&s, 1000.0, Some(0.5));
        assert!((inp.divergence - 0.7).abs() < 1e-6);
        assert!((inp.volume_share - 0.1).abs() < 1e-6);
    }

    #[test]
    fn test_volume_share_capped() {
        let s = mk("x", 0.0, 0.0, 500.0, 50, 50, 0.0);
        let inp = compute_crowd_inputs(&s, 1000.0, Some(0.5));
        // 50% 直接接入
        assert!((inp.volume_share - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_missing_fields_safe() {
        let s = SectorFlow {
            name: "x".into(),
            net_inflow: 0.0,
            change_pct: None,
            turnover_rate: None,
            main_inflow_pct: None,
            total_turnover: None,
            total_volume: None,
            advance_count: None,
            decline_count: None,
            lead_stock: None,
            lead_change_pct: None,
            source: "eastmoney".into(),
        };
        let inp = compute_crowd_inputs(&s, 0.0, None);
        assert_eq!(inp.volume_share, 0.0);
        assert_eq!(inp.divergence, 0.0);
        assert_eq!(inp.turnover_pct, 0.5);
    }

    #[test]
    fn test_batch_rank() {
        let sectors = vec![
            mk("A", 10.0, -5.0, 100.0, 10, 90, 2.0),
            mk("B", 0.0, 0.0, 100.0, 50, 50, 0.0),
            mk("C", 20.0, 5.0, 100.0, 90, 10, 15.0),
        ];
        let out = crowd_levels_for(&sectors);
        assert_eq!(out.len(), 3);
        // C 涨幅最高 → 排名分位最高
        assert!(out[2].1.turnover_pct > out[0].1.turnover_pct);
        // divergence: C 领涨 15% - 上涨面 0.9 = ...
        // A 领涨 2% - 上涨面 0.1 = 0.1
        assert!(out[2].1.divergence > out[0].1.divergence);
    }
}
