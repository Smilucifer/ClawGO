pub mod config;
pub mod runner;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub cron_expr: String,
    #[serde(default)]
    pub interval_min: Option<i64>,
    pub enabled: bool,
    #[serde(default)]
    pub requires_trading_day: bool,
    #[serde(default)]
    pub last_run: Option<String>,
    #[serde(default)]
    pub next_run: Option<String>,
    #[serde(default)]
    pub last_status: Option<String>,
    #[serde(default)]
    pub description: String,
    /// If true, this job runs on its own dedicated timer loop with a fixed
    /// interval (ignoring cron_expr). The main scheduler loop skips it.
    #[serde(default)]
    pub dedicated: bool,
}

pub fn default_jobs() -> Vec<CronJob> {
    vec![
        CronJob {
            id: "pnl_snapshot".into(),
            name: "PnL 快照".into(),
            cron_expr: "0 30 9,11,13,15 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "定时记录持仓市值快照".into(),
            dedicated: false,
        },
        CronJob {
            id: "verdict_review".into(),
            name: "Verdict Review".into(),
            cron_expr: "0 0 17 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "回溯验证裁决命中率".into(),
            dedicated: false,
        },
        CronJob {
            id: "event_scan".into(),
            name: "Event Watch 扫描".into(),
            cron_expr: "0 */30 8-22 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "扫描财经新闻和公告".into(),
            dedicated: false,
        },
        CronJob {
            id: "jin10_collector".into(),
            name: "金十快讯采集".into(),
            cron_expr: "*/15 * * * * *".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "每15秒采集金十A股快讯".into(),
            dedicated: true,
        },
        CronJob {
            id: "event_analyzer".into(),
            name: "事件分析器".into(),
            cron_expr: "0 */10 * * * *".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "每10分钟分析未处理事件".into(),
            dedicated: true,
        },
        CronJob {
            id: "dream_invest".into(),
            name: "Dreaming (投资)".into(),
            cron_expr: "0 0 3 * * *".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "投资洞察管线: Light→REM→Deep".into(),
            dedicated: false,
        },
        CronJob {
            id: "daily_report".into(),
            name: "每日报告".into(),
            cron_expr: "0 0 22 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "生成每日投资报告并存档".into(),
            dedicated: false,
        },
        CronJob {
            id: "macro_refresh".into(),
            name: "宏观指标缓存刷新".into(),
            cron_expr: "0 */15 8-22 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "每15分钟刷新12个宏观指标到macro_cache表".into(),
            dedicated: false,
        },
        CronJob {
            id: "clearance_convert".into(),
            name: "清仓持仓转换".into(),
            cron_expr: "0 5 5 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            // 非交易日也可能有未处理的清仓,确保跨周末/假期也能转换
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "每日 05:05 将昨日清仓的持仓转换为关注".into(),
            dedicated: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pnl_snapshot_default_cron_covers_four_intraday_slots() {
        let jobs = default_jobs();
        let pnl = jobs
            .iter()
            .find(|j| j.id == "pnl_snapshot")
            .expect("pnl_snapshot job present");
        assert_eq!(pnl.cron_expr, "0 30 9,11,13,15 * * 1-5");
        assert!(pnl.enabled);
        assert!(pnl.requires_trading_day);
        assert!(!pnl.dedicated);
    }
}
