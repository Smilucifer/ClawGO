//! 行业板块资金流采集：调 Python 桥 `akshare_sector.sector_fund_flow`。
//!
//! 数据源优先东财 (含主力净占比)，兜底同花顺 (只有净额+涨跌幅)。
//! 单位由 Python 侧统一到 **亿元 / %**，Rust 直接透传。
//!
//! 上层 (拥挤度合成 / 报告 02 段) 拿到空 Vec 就当"当日无数据"处理。

use serde::{Deserialize, Serialize};

/// 单个行业板块当日资金流。
///
/// - `net_inflow`     : 主力净流入 (亿元, 正=流入)
/// - `change_pct`     : 板块涨跌幅 (%)
/// - `turnover_rate`  : 换手率 (%)。当前 EM/THS 两条通道都不返回, 保留字段供未来接入
///                       东财 `stock_board_industry_spot_em` 或 miniQMT 广度补齐。
/// - `main_inflow_pct`: 主力净占比 (%)。仅东财通道有值。
/// - `source`         : `"eastmoney"` / `"ths"`，便于上层判定字段完整度。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorFlow {
    pub name: String,
    pub net_inflow: f64,
    pub change_pct: Option<f64>,
    pub turnover_rate: Option<f64>,
    pub main_inflow_pct: Option<f64>,
    pub source: String,
}

/// 拉取全部行业板块当日主力资金流 (按 net_inflow 降序, Python 侧已排)。
///
/// 上游任意异常 (代理断连 / 接口空 / 解析失败) 都会由 Python 侧吞掉并返回 `[]`，
/// 因此这里返回 `Ok(vec![])` 表示 "跑通了但当日无数据"，Err 只在桥本身挂掉时出现。
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
    fn deserialize_ths_shape() {
        // THS 兜底: turnover/main_pct 为 null
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
        assert_eq!(v[0].name, "半导体");
        assert!((v[0].net_inflow - 66.62).abs() < 1e-6);
        assert!(v[0].turnover_rate.is_none());
        assert!(v[0].main_inflow_pct.is_none());
        assert_eq!(v[0].source, "ths");
    }

    #[test]
    fn deserialize_em_shape() {
        // EM 通道: 有主力净占比
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
    }
}
