//! 拥挤度雷达：换手率分位 + 成交占比 + 龙头背离 → 健康/偏热/过热。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CrowdLevel {
    Healthy,
    Warm,
    Hot,
}

/// 三指标合成拥挤度。各指标已归一到 0-1。
/// `turnover_pct`: 换手率历史分位；`volume_share`: 成交占全市场比；`divergence`: 龙头/板块背离度。
pub fn crowd_level(turnover_pct: f64, volume_share: f64, divergence: f64) -> CrowdLevel {
    // 加权合成：换手分位 0.4 + 成交占比 0.35 + 背离 0.25。
    // volume_share 以 0.30 为满档（单一标的占全市场 30%），封顶避免异常值放大权重。
    let score = turnover_pct * 0.4
        + (volume_share / 0.30).min(1.0) * 0.35
        + divergence * 0.25;
    if score >= 0.75 {
        CrowdLevel::Hot
    } else if score >= 0.55 {
        CrowdLevel::Warm
    } else {
        CrowdLevel::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crowd_healthy() {
        // 低换手分位 + 低成交占比 + 无背离 → 健康
        assert!(matches!(crowd_level(0.3, 0.05, 0.0), CrowdLevel::Healthy));
    }

    #[test]
    fn test_crowd_hot() {
        // 高换手分位 + 高成交占比 + 强背离 → 过热
        assert!(matches!(crowd_level(0.95, 0.30, 0.8), CrowdLevel::Hot));
    }

    #[test]
    fn test_crowd_warm() {
        assert!(matches!(crowd_level(0.75, 0.15, 0.3), CrowdLevel::Warm));
    }
}
