use super::CronJob;
use chrono::{Local, TimeZone};
use cron::Schedule;
use once_cell::sync::Lazy;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    #[serde(default)]
    next_run: Option<String>,
}

/// Serialize concurrent reads/writes to scheduler.json so callers never see
/// half-written content. `load_jobs_base` and `save_jobs` each take this
/// lock independently; toggle_job / update_cron / save_dream_config call
/// load → save sequentially with the lock released in between, so no
/// re-entrance occurs on the same thread.
static SCHEDULER_FILE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn config_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".claw-go").join("invest").join("scheduler.json")
}

/// Load jobs: start from defaults, overlay user overrides, compute next_run.
pub fn load_jobs() -> Vec<CronJob> {
    let mut jobs = load_jobs_base();
    for job in &mut jobs {
        // Only compute next_run if not already persisted from disk.
        // Previously, this always recomputed, which produced a strictly-future
        // time that caused should_fire() to always return false.
        if job.next_run.is_none() {
            job.next_run = compute_next_run_for_job(job);
        }
    }
    jobs
}

/// Shared base: defaults + disk overlay. No derived-field computation.
pub(crate) fn load_jobs_base() -> Vec<CronJob> {
    let _guard = SCHEDULER_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
                job.cron_expr = normalize_cron_6field(&c);
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
            if let Some(nr) = ov.next_run {
                job.next_run = Some(nr);
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
    let schedule = match Schedule::from_str(&job.cron_expr) {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "compute_next_run_for_job: failed to parse cron '{}' for job '{}': {e}",
                job.cron_expr,
                job.id
            );
            return None;
        }
    };
    let next = schedule.after(&now).next()?;
    Some(next.format("%Y-%m-%dT%H:%M:%S").to_string())
}

/// Pure predicate: should the main scheduler loop fire `job` at `now`?
///
/// Semantics:
/// - `enabled == false` or `dedicated == true` → never fire from the main loop.
/// - `next_run == None` (never scheduled) → fire once; the caller MUST write
///   back `next_run` after firing or skipping so subsequent ticks fall into
///   the parsed branch.
/// - `next_run == Some(parseable)` → fire when `now >= parsed`.
/// - `next_run == Some(garbage)` → fire (self-heals on next write-back).
pub(crate) fn should_fire(job: &CronJob, now: chrono::NaiveDateTime) -> bool {
    if !job.enabled || job.dedicated {
        return false;
    }
    match &job.next_run {
        Some(next) => chrono::NaiveDateTime::parse_from_str(next, "%Y-%m-%dT%H:%M:%S")
            .map(|dt| now >= dt)
            .unwrap_or(true),
        None => true,
    }
}

/// Save user overrides (only changed fields) to scheduler.json.
///
/// Atomic write (tmp + rename), retrying up to 3 times on PermissionDenied.
/// Mirrors `crate::invest::committee::queue::save_queue`.
pub fn save_jobs(jobs: &[CronJob]) -> Result<(), String> {
    let _guard = SCHEDULER_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

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
                    || d.next_run != job.next_run
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
                    next_run: job.next_run.clone(),
                })
            } else {
                None
            }
        })
        .collect();

    let config = SchedulerConfig { jobs: overrides };
    let json = serde_json::to_string_pretty(&config).map_err(|e| format!("{e}"))?;

    let path = config_path();
    let dir = path
        .parent()
        .ok_or_else(|| "scheduler.json path has no parent".to_string())?;
    fs::create_dir_all(dir).map_err(|e| format!("create_dir_all: {e}"))?;

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp = dir.join(format!("scheduler.json.{}.{}.tmp", std::process::id(), nanos));

    fs::write(&tmp, &json).map_err(|e| format!("write tmp: {e}"))?;

    for attempt in 0..3u8 {
        match fs::rename(&tmp, &path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && attempt < 2 => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return Err(format!("rename: {e}"));
            }
        }
    }
    let _ = fs::remove_file(&tmp);
    Err("rename: PermissionDenied after 3 retries".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_job(cron_expr: impl Into<String>) -> CronJob {
        CronJob {
            id: "test_job".into(),
            name: "Test Job".into(),
            cron_expr: cron_expr.into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: String::new(),
            dedicated: false,
        }
    }

    #[test]
    fn normalize_5field_prepends_seconds() {
        assert_eq!(normalize_cron_6field("0 3 * * *"), "0 0 3 * * *");
        assert_eq!(normalize_cron_6field("*/15 * * * *"), "0 */15 * * * *");
    }

    #[test]
    fn normalize_6field_unchanged() {
        assert_eq!(normalize_cron_6field("0 0 3 * * *"), "0 0 3 * * *");
        assert_eq!(
            normalize_cron_6field("0 30 9,11,13,15 * * 1-5"),
            "0 30 9,11,13,15 * * 1-5"
        );
    }

    #[test]
    fn compute_next_run_returns_none_for_unnormalized_5field_cron() {
        // Documents the bug: a raw 5-field cron is unparseable by the
        // `cron` crate, so without normalization on load we silently get None.
        let job = make_job("0 3 * * *");
        assert!(compute_next_run_for_job(&job).is_none());
    }

    #[test]
    fn compute_next_run_returns_some_for_5field_cron_after_normalize() {
        // Simulates what load_jobs_base now writes when a user override
        // contains a 5-field cron string.
        let mut job = make_job("");
        job.cron_expr = normalize_cron_6field("0 3 * * *");
        assert_eq!(job.cron_expr, "0 0 3 * * *");
        assert!(
            compute_next_run_for_job(&job).is_some(),
            "expected Some next_run for normalized cron '{}'",
            job.cron_expr
        );
    }

    #[test]
    fn compute_next_run_some_for_6field_cron() {
        let job = make_job("0 30 9,11,13,15 * * 1-5");
        assert!(compute_next_run_for_job(&job).is_some());
    }

    fn at(s: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").unwrap()
    }

    #[test]
    fn should_fire_disabled_returns_false() {
        let mut job = make_job("0 0 3 * * *");
        job.enabled = false;
        job.next_run = None;
        assert!(!should_fire(&job, at("2026-06-19T12:00:00")));
    }

    #[test]
    fn should_fire_dedicated_returns_false() {
        let mut job = make_job("0 0 3 * * *");
        job.dedicated = true;
        job.next_run = None;
        assert!(!should_fire(&job, at("2026-06-19T12:00:00")));
    }

    #[test]
    fn should_fire_none_next_run_returns_true() {
        let job = make_job("0 0 3 * * *");
        assert!(should_fire(&job, at("2026-06-19T12:00:00")));
    }

    #[test]
    fn should_fire_past_next_run_returns_true() {
        let mut job = make_job("0 0 3 * * *");
        job.next_run = Some("2000-01-01T00:00:00".into());
        assert!(should_fire(&job, at("2026-06-19T12:00:00")));
    }

    #[test]
    fn should_fire_future_next_run_returns_false() {
        let mut job = make_job("0 0 3 * * *");
        job.next_run = Some("2099-01-01T00:00:00".into());
        assert!(!should_fire(&job, at("2026-06-19T12:00:00")));
    }

    #[test]
    fn should_fire_unparseable_next_run_returns_true() {
        let mut job = make_job("0 0 3 * * *");
        job.next_run = Some("not-a-timestamp".into());
        assert!(should_fire(&job, at("2026-06-19T12:00:00")));
    }

    use std::sync::Mutex as StdMutex;
    static TEST_ENV_LOCK: StdMutex<()> = StdMutex::new(());

    /// RAII guard that restores USERPROFILE/HOME on drop, even on panic.
    /// Required because `dirs::home_dir()` on Windows resolves via
    /// SHGetKnownFolderPath(FOLDERID_Profile) and ignores env vars — so the
    /// pre-`save_jobs` assertion below may panic when isolation fails, and we
    /// must restore env regardless to avoid leaking test state into the
    /// process for subsequent tests.
    struct EnvGuard {
        userprofile: Option<std::ffi::OsString>,
        home: Option<std::ffi::OsString>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.userprofile {
                Some(v) => std::env::set_var("USERPROFILE", v),
                None => std::env::remove_var("USERPROFILE"),
            }
            match &self.home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    #[test]
    fn save_then_load_base_roundtrips_cron_override() {
        let _t = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().expect("tempdir");
        let _env = EnvGuard {
            userprofile: std::env::var_os("USERPROFILE"),
            home: std::env::var_os("HOME"),
        };
        std::env::set_var("USERPROFILE", tmp.path());
        std::env::set_var("HOME", tmp.path());

        // Defensive isolation check: dirs 6.0 on Windows resolves home via
        // SHGetKnownFolderPath, not env vars, so USERPROFILE/HOME overrides
        // are no-ops there. If config_path() is not under tmp, abort BEFORE
        // any disk write so we never clobber the developer's real
        // ~/.claw-go/invest/scheduler.json.
        assert!(
            config_path().starts_with(tmp.path()),
            "test isolation failed: config_path()={:?} is not under tmp dir {:?}; aborting before write to avoid polluting real ~/.claw-go config",
            config_path(),
            tmp.path()
        );

        let mut jobs = super::super::default_jobs();
        let pnl = jobs.iter_mut().find(|j| j.id == "pnl_snapshot").expect("pnl present");
        pnl.cron_expr = "0 0 10,14 * * 1-5".to_string();
        pnl.enabled = false;
        save_jobs(&jobs).expect("save_jobs ok");

        let reloaded = load_jobs_base();
        let rp = reloaded.iter().find(|j| j.id == "pnl_snapshot").expect("pnl after reload");
        assert_eq!(rp.cron_expr, "0 0 10,14 * * 1-5");
        assert!(!rp.enabled);
    }

    #[test]
    fn next_run_persists_through_save_load_cycle() {
        let _t = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().expect("tempdir");
        let _env = EnvGuard {
            userprofile: std::env::var_os("USERPROFILE"),
            home: std::env::var_os("HOME"),
        };
        std::env::set_var("USERPROFILE", tmp.path());
        std::env::set_var("HOME", tmp.path());

        assert!(
            config_path().starts_with(tmp.path()),
            "test isolation failed"
        );

        let mut jobs = super::super::default_jobs();
        let pnl = jobs.iter_mut().find(|j| j.id == "pnl_snapshot").unwrap();
        pnl.next_run = Some("2026-06-25T09:30:00".into());
        save_jobs(&jobs).expect("save_jobs ok");

        let reloaded = load_jobs();
        let rp = reloaded.iter().find(|j| j.id == "pnl_snapshot").unwrap();
        assert_eq!(
            rp.next_run.as_deref(),
            Some("2026-06-25T09:30:00"),
            "next_run should survive save→load round-trip"
        );
    }
}
