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
    let data = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    data.join("claw-go").join("invest").join("scheduler.json")
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
