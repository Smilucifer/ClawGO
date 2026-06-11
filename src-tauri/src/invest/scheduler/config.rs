use super::CronJob;
use chrono::{Local, TimeZone};
use cron::Schedule;
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
    #[serde(default)]
    last_run: Option<String>,
    #[serde(default)]
    last_status: Option<String>,
}

fn config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".claw-go").join("invest").join("scheduler.json")
}

/// Load jobs: start from defaults, overlay user overrides, compute next_run.
pub fn load_jobs() -> Vec<CronJob> {
    let mut jobs = load_jobs_base();
    for job in &mut jobs {
        job.next_run = compute_next_run_for_job(job);
    }
    jobs
}

/// Shared base: defaults + disk overlay. No derived-field computation.
pub(crate) fn load_jobs_base() -> Vec<CronJob> {
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
            if let Some(lr) = ov.last_run {
                job.last_run = Some(lr);
            }
            if let Some(ls) = ov.last_status {
                job.last_status = Some(ls);
            }
        }
    }
    jobs
}

/// Compute the next fire time for a job based on its schedule.
pub fn compute_next_run_for_job(job: &CronJob) -> Option<String> {
    if !job.enabled {
        return None;
    }
    let now = Local::now();
    if let Some(interval) = job.interval_min {
        // Interval-based: next = last_run + interval, or now if never run
        let next = match &job.last_run {
            Some(last) => {
                let last_dt = chrono::NaiveDateTime::parse_from_str(last, "%Y-%m-%dT%H:%M:%S").ok()?;
                let next_naive = last_dt + chrono::Duration::minutes(interval);
                if next_naive <= now.naive_local() {
                    now
                } else {
                    Local.from_local_datetime(&next_naive).single()?
                }
            }
            None => now,
        };
        return Some(next.format("%Y-%m-%dT%H:%M:%S").to_string());
    }
    // Cron-based
    let schedule = Schedule::from_str(&job.cron_expr).ok()?;
    let next = schedule.after(&now).next()?;
    Some(next.format("%Y-%m-%dT%H:%M:%S").to_string())
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
                    || d.last_run != job.last_run
                    || d.last_status != job.last_status
            });
            if changed {
                Some(JobOverride {
                    id: job.id.clone(),
                    cron_expr: Some(job.cron_expr.clone()),
                    interval_min: job.interval_min,
                    enabled: Some(job.enabled),
                    requires_trading_day: Some(job.requires_trading_day),
                    last_run: job.last_run.clone(),
                    last_status: job.last_status.clone(),
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

/// Normalize a cron expression to 6-field format (second minute hour dom month dow).
/// If the expression has exactly 5 fields, prepend "0 " for the seconds field.
fn normalize_cron_6field(expr: &str) -> String {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() == 5 {
        format!("0 {}", expr)
    } else {
        expr.to_string()
    }
}

/// Update a single job's cron expression and persist.
pub fn update_cron(id: &str, cron_expr: &str) -> Result<(), String> {
    // Validate cron expression (trim whitespace and stray characters)
    let cleaned = cron_expr.trim();
    let cleaned: String = cleaned.chars().filter(|c| c.is_ascii_alphanumeric() || " */,-".contains(*c)).collect();
    let cleaned = normalize_cron_6field(&cleaned);
    cron::Schedule::from_str(&cleaned).map_err(|e| format!("Invalid cron: {e}"))?;
    let mut jobs = load_jobs();
    let job = jobs
        .iter_mut()
        .find(|j| j.id == id)
        .ok_or_else(|| format!("Job '{}' not found", id))?;
    job.cron_expr = cleaned;
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
    // Validate cron expression (trim and strip stray characters)
    let cleaned_cron: String = if config.invest_cron.is_empty() {
        String::new()
    } else {
        let filtered: String = config.invest_cron.trim().chars()
            .filter(|c| c.is_ascii_alphanumeric() || " */,-".contains(*c))
            .collect();
        normalize_cron_6field(&filtered)
    };
    if !cleaned_cron.is_empty() {
        cron::Schedule::from_str(&cleaned_cron)
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
    if let Some(j) = jobs.iter_mut().find(|j| j.id == "dream_invest") {
        j.enabled = config.invest_enabled;
        j.cron_expr = cleaned_cron;
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
