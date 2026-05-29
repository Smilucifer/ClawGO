use super::CronJob;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchedulerConfig {
    jobs: Vec<JobOverride>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobOverride {
    id: String,
    #[serde(default)]
    cron_expr: Option<String>,
    #[serde(default)]
    interval_min: Option<i64>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    requires_trading_day: Option<bool>,
}

fn config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".claw-go").join("invest").join("scheduler.json")
}

/// Load jobs: start from defaults, overlay user overrides from scheduler.json.
pub fn load_jobs() -> Vec<CronJob> {
    let mut jobs = super::default_jobs();
    let path = config_path();
    if !path.exists() {
        return jobs;
    }
    let Ok(content) = std::fs::read_to_string(&path) else {
        return jobs;
    };
    let Ok(config) = serde_json::from_str::<SchedulerConfig>(&content) else {
        return jobs;
    };
    for ov in config.jobs {
        if let Some(job) = jobs.iter_mut().find(|j| j.id == ov.id) {
            if let Some(c) = ov.cron_expr {
                job.cron_expr = c;
            }
            if let Some(i) = ov.interval_min {
                job.interval_min = Some(i);
            }
            if let Some(e) = ov.enabled {
                job.enabled = e;
            }
            if let Some(r) = ov.requires_trading_day {
                job.requires_trading_day = r;
            }
        }
    }
    jobs
}

/// Save user overrides (only changed fields) to scheduler.json.
pub fn save_jobs(jobs: &[CronJob]) -> Result<(), String> {
    let defaults = super::default_jobs();
    let overrides: Vec<JobOverride> = jobs
        .iter()
        .filter_map(|job| {
            let def = defaults.iter().find(|d| d.id == job.id);
            let changed = def.map_or(true, |d| {
                d.cron_expr != job.cron_expr
                    || d.interval_min != job.interval_min
                    || d.enabled != job.enabled
                    || d.requires_trading_day != job.requires_trading_day
            });
            if changed {
                Some(JobOverride {
                    id: job.id.clone(),
                    cron_expr: Some(job.cron_expr.clone()),
                    interval_min: job.interval_min,
                    enabled: Some(job.enabled),
                    requires_trading_day: Some(job.requires_trading_day),
                })
            } else {
                None
            }
        })
        .collect();
    let config = SchedulerConfig { jobs: overrides };
    let json = serde_json::to_string_pretty(&config).map_err(|e| format!("{e}"))?;
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{e}"))?;
    }
    std::fs::write(&path, json).map_err(|e| format!("{e}"))
}

/// Toggle a single job's enabled state and persist.
pub fn toggle_job(id: &str, enabled: bool) -> Result<(), String> {
    let mut jobs = load_jobs();
    let job = jobs
        .iter_mut()
        .find(|j| j.id == id)
        .ok_or_else(|| format!("Job '{}' not found", id))?;
    job.enabled = enabled;
    save_jobs(&jobs)
}

/// Update a single job's cron expression and persist.
pub fn update_cron(id: &str, cron_expr: &str) -> Result<(), String> {
    // Validate cron expression
    cron::Schedule::from_str(cron_expr).map_err(|e| format!("Invalid cron: {e}"))?;
    let mut jobs = load_jobs();
    let job = jobs
        .iter_mut()
        .find(|j| j.id == id)
        .ok_or_else(|| format!("Job '{}' not found", id))?;
    job.cron_expr = cron_expr.to_string();
    save_jobs(&jobs)
}

/// Load dream-specific config (lookback_days, min_score, min_count) from dream_config.json,
/// merged with scheduler job state (enabled, cron/interval).
pub fn load_dream_config() -> super::super::dreaming::DreamConfig {
    let mut config = super::super::dreaming::DreamConfig::default();

    // Overlay scheduler job state
    let jobs = load_jobs();
    if let Some(j) = jobs.iter().find(|j| j.id == "dream_invest") {
        config.invest_enabled = j.enabled;
        config.invest_cron = j.cron_expr.clone();
    }
    if let Some(j) = jobs.iter().find(|j| j.id == "dream_user") {
        config.user_memory_enabled = j.enabled;
        config.user_memory_interval_min = j.interval_min.unwrap_or(120);
    }

    // Overlay dream_config.json for pipeline params
    let path = dream_config_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(override_cfg) = serde_json::from_str::<DreamConfigOverride>(&content) {
                if let Some(v) = override_cfg.lookback_days {
                    config.lookback_days = v;
                }
                if let Some(v) = override_cfg.min_score {
                    config.min_score = v;
                }
                if let Some(v) = override_cfg.min_count {
                    config.min_count = v;
                }
            }
        }
    }

    config
}

/// Save dream config: pipeline params to dream_config.json, scheduler state via save_jobs.
pub fn save_dream_config(config: &super::super::dreaming::DreamConfig) -> Result<(), String> {
    // Validate cron expression
    if !config.invest_cron.is_empty() {
        cron::Schedule::from_str(&config.invest_cron)
            .map_err(|e| format!("Invalid cron expression: {e}"))?;
    }

    // Save pipeline params to dream_config.json
    let override_cfg = DreamConfigOverride {
        lookback_days: Some(config.lookback_days),
        min_score: Some(config.min_score),
        min_count: Some(config.min_count),
    };
    let json = serde_json::to_string_pretty(&override_cfg).map_err(|e| format!("{e}"))?;
    let path = dream_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{e}"))?;
    }
    std::fs::write(&path, json).map_err(|e| format!("write dream_config.json: {e}"))?;

    // Save scheduler state
    let mut jobs = load_jobs();
    if let Some(j) = jobs.iter_mut().find(|j| j.id == "dream_user") {
        j.enabled = config.user_memory_enabled;
        j.interval_min = Some(config.user_memory_interval_min);
    }
    if let Some(j) = jobs.iter_mut().find(|j| j.id == "dream_invest") {
        j.enabled = config.invest_enabled;
        j.cron_expr = config.invest_cron.clone();
    }
    save_jobs(&jobs)
}

fn dream_config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".claw-go").join("invest").join("dream_config.json")
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DreamConfigOverride {
    #[serde(default)]
    lookback_days: Option<i64>,
    #[serde(default)]
    min_score: Option<f64>,
    #[serde(default)]
    min_count: Option<i64>,
}
