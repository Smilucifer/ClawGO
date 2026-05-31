pub mod pipeline;
pub mod snapshot;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DreamConfig {
    pub invest_enabled: bool,
    pub invest_cron: String,
    pub lookback_days: i64,
    pub min_score: f64,
    pub min_count: i64,
}

impl Default for DreamConfig {
    fn default() -> Self {
        Self {
            invest_enabled: false,
            invest_cron: "0 3 * * *".into(),
            lookback_days: 30,
            min_score: 0.8,
            min_count: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DreamResult {
    pub insights_written: usize,
    pub insights_updated: usize,
    pub insights_archived: usize,
    pub pipeline_duration_ms: i64,
    pub stages: Vec<StageResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageResult {
    pub stage: String,
    pub duration_ms: i64,
    pub items_processed: usize,
    pub items_output: usize,
}

/// Trigger the invest dreaming pipeline.
pub async fn trigger_dream(mode: &str, tushare_token: &str) -> Result<DreamResult, String> {
    match mode {
        "invest" => pipeline::run_invest_pipeline(tushare_token).await,
        _ => Err(format!("Unknown dream mode: {mode}")),
    }
}
