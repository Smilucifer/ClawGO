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
}

pub fn default_jobs() -> Vec<CronJob> {
    vec![
        CronJob {
            id: "pnl_snapshot".into(),
            name: "PnL 快照".into(),
            cron_expr: "30 9,11 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "定时记录持仓市值快照".into(),
        },
        CronJob {
            id: "verdict_review".into(),
            name: "Verdict Review".into(),
            cron_expr: "0 17 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "回溯验证裁决命中率".into(),
        },
        CronJob {
            id: "event_scan".into(),
            name: "Event Watch 扫描".into(),
            cron_expr: "*/30 8-22 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "扫描财经新闻和公告".into(),
        },
        CronJob {
            id: "dream_user".into(),
            name: "Dreaming (用户记忆)".into(),
            cron_expr: String::new(),
            interval_min: Some(120),
            enabled: false,
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "用户记忆衰减与归档".into(),
        },
        CronJob {
            id: "dream_invest".into(),
            name: "Dreaming (投资)".into(),
            cron_expr: "0 3 * * *".into(),
            interval_min: None,
            enabled: false,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "投资洞察管线: Light→REM→Deep".into(),
        },
        CronJob {
            id: "daily_report".into(),
            name: "每日报告".into(),
            cron_expr: "0 22 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "生成每日投资报告并存档".into(),
        },
        CronJob {
            id: "macro_refresh".into(),
            name: "宏观指标缓存刷新".into(),
            cron_expr: "*/15 8-22 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "每15分钟刷新12个宏观指标到macro_cache表".into(),
        },
    ]
}
