//! 行业板块资金流采集：调 Python 桥 `akshare_sector.sector_fund_flow`。
//!
//! 主数据源：同花顺 `stock_board_industry_summary_ths()`（一次返回 90 个行业板块，
//! 字段最全 —— 覆盖盘前 02 段板块资金流入榜 + 拥挤度雷达三指标合成的全部输入）。
//! 兜底：东财 `stock_sector_fund_flow_rank` → 同花顺 `stock_fund_flow_industry`。
//!
//! 单位由 Python 侧统一到 **亿元 / % / 万手 / 家**，Rust 直接透传。
//!
//! 上层（拥挤度合成 / 报告 02 / 03 段）拿到空 Vec 就当"当日无数据"处理。

use serde::{Deserialize, Serialize};

/// 单个行业板块当日快照。
///
/// 字段来源分层：
/// - **主源（ths_summary）** 全字段可用：`total_turnover / total_volume /
///   advance_count / decline_count / lead_stock / lead_change_pct`。
/// - **兜底源（eastmoney / ths）** 附加字段为 None，仅保证 `net_inflow + change_pct`。
///
/// - `net_inflow`      : 主力净流入 (亿元, 正=流入)
/// - `change_pct`      : 板块涨跌幅 (%)
/// - `turnover_rate`   : 换手率 (%)。当前上游均不返回，保留字段供未来接入。
/// - `main_inflow_pct` : 主力净占比 (%)。仅东财兜底通道有值。
/// - `total_turnover`  : 板块当日总成交额 (亿元) —— 拥挤度成交占比分母。
/// - `total_volume`    : 板块当日总成交量 (万手) —— 保留供未来指标使用。
/// - `advance_count`   : 板块内上涨家数 —— 拥挤度龙头背离用。
/// - `decline_count`   : 板块内下跌家数
/// - `lead_stock`      : 领涨股名
/// - `lead_change_pct` : 领涨股涨跌幅 (%) —— 拥挤度龙头背离用。
/// - `source`          : `"ths_summary" / "eastmoney" / "ths"`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorFlow {
    pub name: String,
    pub net_inflow: f64,
    pub change_pct: Option<f64>,
    pub turnover_rate: Option<f64>,
    pub main_inflow_pct: Option<f64>,
    #[serde(default)]
    pub total_turnover: Option<f64>,
    #[serde(default)]
    pub total_volume: Option<f64>,
    #[serde(default)]
    pub advance_count: Option<i64>,
    #[serde(default)]
    pub decline_count: Option<i64>,
    #[serde(default)]
    pub lead_stock: Option<String>,
    #[serde(default)]
    pub lead_change_pct: Option<f64>,
    pub source: String,
}

/// 拉取全部行业板块当日快照（按 net_inflow 降序，Python 侧已排）。
///
/// 上游任意异常（代理断连 / 接口空 / 解析失败）都会由 Python 侧吞掉并返回 `[]`，
/// 因此这里返回 `Ok(vec![])` 表示"跑通了但当日无数据"，Err 只在桥本身挂掉时出现。
pub async fn fetch_sector_flow() -> Result<Vec<SectorFlow>, String> {
    let runtime = crate::python::require()?;
    let value = runtime
        .call("akshare_sector.sector_fund_flow", serde_json::json!({}))
        .await?;
    serde_json::from_value::<Vec<SectorFlow>>(value)
        .map_err(|e| format!("parse akshare_sector.sector_fund_flow: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_ths_summary_full() {
        // ths_summary 主源：所有附加字段可用
        let raw = serde_json::json!([{
            "name": "半导体",
            "net_inflow": 92.6,
            "change_pct": -0.66,
            "turnover_rate": null,
            "main_inflow_pct": null,
            "total_turnover": 4989.75,
            "total_volume": 5322.0,
            "advance_count": 67,
            "decline_count": 112,
            "lead_stock": "芯原微",
            "lead_change_pct": 16.8,
            "source": "ths_summary"
        }]);
        let v: Vec<SectorFlow> = serde_json::from_value(raw).unwrap();
        assert_eq!(v.len(), 1);
        let s = &v[0];
        assert_eq!(s.name, "半导体");
        assert!((s.net_inflow - 92.6).abs() < 1e-6);
        assert_eq!(s.total_turnover, Some(4989.75));
        assert_eq!(s.advance_count, Some(67));
        assert_eq!(s.decline_count, Some(112));
        assert_eq!(s.lead_stock.as_deref(), Some("芯原微"));
        assert_eq!(s.lead_change_pct, Some(16.8));
        assert_eq!(s.source, "ths_summary");
    }

    #[test]
    fn deserialize_em_fallback_missing_extras() {
        // 兜底源缺附加字段：serde(default) 让它们回落到 None
        let raw = serde_json::json!([{
            "name": "半导体",
            "net_inflow": 12.34,
            "change_pct": 1.5,
            "turnover_rate": null,
            "main_inflow_pct": 3.21,
            "source": "eastmoney"
        }]);
        let v: Vec<SectorFlow> = serde_json::from_value(raw).unwrap();
        assert_eq!(v[0].source, "eastmoney");
        assert_eq!(v[0].main_inflow_pct, Some(3.21));
        assert!(v[0].total_turnover.is_none());
        assert!(v[0].advance_count.is_none());
        assert!(v[0].lead_stock.is_none());
    }

    #[test]
    fn deserialize_ths_old_shape_still_works() {
        let raw = serde_json::json!([{
            "name": "半导体",
            "net_inflow": 66.62,
            "change_pct": -0.66,
            "turnover_rate": null,
            "main_inflow_pct": null,
            "source": "ths"
        }]);
        let v: Vec<SectorFlow> = serde_json::from_value(raw).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].source, "ths");
        assert!(v[0].total_turnover.is_none());
    }
}
