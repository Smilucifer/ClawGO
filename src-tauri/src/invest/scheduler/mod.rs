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
            cron_expr: "0 */30 8-22 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "交易日8-22点每30分钟分析未处理事件(其他时间手动触发)".into(),
            dedicated: false,
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
        CronJob {
            id: "premarket_report".into(),
            name: "盘前观察报告".into(),
            cron_expr: "0 0 21 * * 0-4".into(),
            interval_min: None,
            enabled: true,
            // A6: runs Sun-Thu 21:00 (evening before); evening is NOT a trading day
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "交易日前一晚 21:00 生成观察报告（舆情+SABC+拥挤度+AI点评）".into(),
            dedicated: false,
        },
        CronJob {
            id: "macro_verdict".into(),
            name: "全局宏观判断".into(),
            // 开盘→收盘约每 20-30 分钟(错峰 macro_refresh 的 */15),含 11:05 上午读数与 14:55 收盘定版。
            // 门禁(is_trading_session 9:30-11:30/13:00-15:00)挡掉 9:05、11:35、11:55 的废触发。
            cron_expr: "0 5,35,55 9,10,11,13,14 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "开盘时段每30分钟产出全局宏观判断(赚钱效应/市场阶段/signal)".into(),
            dedicated: false,
        },
        CronJob {
            id: "premarket_cache".into(),
            name: "盘后SABC缓存".into(),
            // 盘后 16:30 工作日:收盘后拉全市场,粗筛≤200,算四因子落表
            cron_expr: "0 30 16 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "盘后批量拉全市场→粗筛→四因子→premarket_factor_cache".into(),
            dedicated: false,
        },
        CronJob {
            id: "sentiment_collector".into(),
            name: "舆情定时采集".into(),
            // 每小时采集外部舆情 + 串联归一化打标(非交易日也采,覆盖周末消息)
            cron_expr: "0 0 * * * *".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "每小时采集外部舆情并归一化打标(全市场口径)".into(),
            dedicated: false,
        },
        CronJob {
            id: "news_cleanup".into(),
            name: "快讯/新闻清理".into(),
            cron_expr: "0 30 3 * * *".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "每日03:30清理7天前的快讯和舆情数据".into(),
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

    #[test]
    fn test_macro_verdict_job_registered() {
        assert!(default_jobs().iter().any(|j| j.id == "macro_verdict" && j.requires_trading_day));
    }
}
