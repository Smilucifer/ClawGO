use super::config;
use crate::storage::invest::scheduler::{is_trading_day, log_task_end, log_task_start};
use crate::tushare::client::TushareClient;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration, Instant};

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Shared dispatch for scheduler jobs. Called from both the background runner
/// loop and the manual `trigger_cron_job` Tauri command.
pub async fn dispatch_job(id: &str) -> Result<String, String> {
    match id {
        "pnl_snapshot" => {
            let result = crate::run_pnl_snapshot().await?;
            Ok(result)
        }
        "event_scan" => {
            let (tushare, llm_client, llm_config) =
                crate::commands::invest::build_scan_clients()?;
            let result = crate::invest::event_scanner::scan_events(
                &tushare,
                &llm_client,
                &llm_config,
                None,
                crate::invest::event_scanner::DEFAULT_LANGUAGE,
            )
            .await?;
            Ok(format!(
                "Scanned: {} fetched, {} saved",
                result.fetched, result.saved
            ))
        }
        "jin10_collector" => {
            let result = crate::invest::jin10_collector::collect_jin10_news().await?;
            Ok(format!(
                "Collected: {} fetched, {} new, {} duplicates",
                result.fetched, result.new_saved, result.duplicates_skipped
            ))
        }
        "event_analyzer" => {
            let (_, llm_client, llm_config) =
                crate::commands::invest::build_scan_clients()?;
            let result = crate::invest::event_analyzer::analyze_pending_events(
                &llm_client,
                &llm_config,
                None,
                crate::invest::event_scanner::DEFAULT_LANGUAGE,
            )
            .await?;
            Ok(format!(
                "Analyzed: {} pending, {} analyzed, {} skipped",
                result.total_pending, result.analyzed, result.skipped
            ))
        }
        "verdict_review" => {
            let settings = crate::storage::settings::get_user_settings();
            let tushare_token = settings
                .tushare_token
                .ok_or("no tushare_token configured for verdict_review")?;
            let summary = crate::invest::verdict_review::run_verdict_review(&tushare_token).await?;
            Ok(format!(
                "Verdict review complete: {} verdicts, {:.1}% hit rate",
                summary.total_verdicts,
                summary.overall_hit_rate * 100.0
            ))
        }
        "dream_invest" => {
            let settings = crate::storage::settings::get_user_settings();
            let tushare_token = settings
                .tushare_token
                .ok_or("no tushare_token configured for dream_invest")?;
            let result = crate::invest::dreaming::trigger_dream("invest", &tushare_token).await?;
            Ok(format!("Dream invest complete: {:?}", result))
        }
        "daily_report" => {
            let data_dir = crate::storage::data_dir();
            let result = crate::invest::daily_report::generate_daily_report(&data_dir)?;
            Ok(result)
        }
        "macro_refresh" => {
            let settings = crate::storage::settings::get_user_settings();
            let tushare_token = settings
                .tushare_token
                .ok_or("no tushare_token configured for macro_refresh")?;
            let client = TushareClient::with_token(tushare_token);
            crate::invest::macro_refresh::refresh_macro_cache(&client).await
        }
        _ => Err(format!("Unknown job: {}", id)),
    }
}

/// Persist job run status to scheduler config (shared by main loop and dedicated loops).
///
/// When `compute_next` is true, `next_run` is recalculated from the cron schedule.
/// Dedicated-loop jobs pass `false` since their timing is not cron-driven.
fn persist_job_status(job_id: &str, ok: bool, compute_next: bool) {
    let now = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();
    // Use load_jobs_base to avoid computing next_run for every job on each tick.
    // We only compute next_run for the specific job if requested.
    let mut jobs = config::load_jobs_base();
    if let Some(j) = jobs.iter_mut().find(|j| j.id == job_id) {
        j.last_run = Some(now);
        j.last_status = Some(if ok { "ok".into() } else { "error".into() });
        if compute_next {
            j.next_run = config::compute_next_run_for_job(j);
        } else {
            j.next_run = None;
        }
        if let Err(e) = config::save_jobs(&jobs) {
            log::error!("Failed to save job state: {e}");
        }
    }
}

/// Execute a job with logging and status persistence.
/// Shared by both the main cron loop and dedicated timer loops.
async fn execute_and_log(job_id: &str, result: Result<String, String>, compute_next: bool) {
    let log_id = log_task_start(job_id).ok();
    match &result {
        Ok(msg) => {
            if let Some(id) = log_id {
                let _ = log_task_end(id, "ok", Some(msg));
            }
        }
        Err(err) => {
            if let Some(id) = log_id {
                let _ = log_task_end(id, "error", Some(err));
            }
        }
    }
    persist_job_status(job_id, result.is_ok(), compute_next);
}

/// Start the scheduler loop and dedicated timers. Call once from lib.rs setup.
///
/// Jobs with `dedicated: true` run on their own precise timer loops;
/// all other jobs go through the main cron loop.
pub fn start<F, Fut>(dispatch: F)
where
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    if RUNNING.swap(true, Ordering::SeqCst) {
        return; // already running
    }

    // Spawn dedicated loops for high-frequency jobs
    start_dedicated_loop("jin10_collector", Duration::from_secs(10), Duration::from_secs(15));
    start_dedicated_loop("event_analyzer", Duration::from_secs(30), Duration::from_secs(10 * 60));

    // Main scheduler loop for all non-dedicated jobs
    let dispatch = Arc::new(dispatch);
    tauri::async_runtime::spawn(async move {
        // Initial delay to let app finish setup
        sleep(Duration::from_secs(10)).await;
        loop {
            let mut jobs = config::load_jobs();
            let today = crate::invest::date_utils::get_invest_date();
            let now_naive = chrono::Local::now().naive_local();

            // First pass: pure due-check + non-trading-day skip handling.
            // Skip side-effects (log + advance next_run) live here, NOT inside
            // a .filter() closure, so we can persist once at the end and so
            // the predicate stays a pure function (`config::should_fire`).
            let mut to_fire: Vec<String> = Vec::new();
            let mut dirty = false;
            for job in jobs.iter_mut() {
                if !config::should_fire(job, now_naive) {
                    continue;
                }
                if job.requires_trading_day && !is_trading_day(&today).unwrap_or(false) {
                    if let Ok(log_id) = log_task_start(&job.id) {
                        let _ = log_task_end(log_id, "skipped", Some("non-trading day"));
                    }
                    // BUG #14 fix: advance next_run so we don't re-skip every
                    // tick for the rest of the non-trading day.
                    job.next_run = config::compute_next_run_for_job(job);
                    dirty = true;
                    continue;
                }
                to_fire.push(job.id.clone());
            }

            if dirty {
                if let Err(e) = config::save_jobs(&jobs) {
                    log::error!("Failed to persist skipped-job next_run: {e}");
                }
            }

            for job_id in to_fire {
                let result = (dispatch)(job_id.clone()).await;
                execute_and_log(&job_id, result, true).await;

                // Update last_run/last_status in the local jobs vec for next_run computation
                let now = chrono::Local::now()
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string();
                if let Some(j) = jobs.iter_mut().find(|j| j.id == job_id) {
                    j.last_run = Some(now);
                    j.next_run = config::compute_next_run_for_job(j);
                    if let Err(e) = config::save_jobs(&jobs) {
                        log::error!("Failed to save job state: {e}");
                    }
                }
            }

            sleep(Duration::from_secs(60)).await;
        }
    });
}

/// Spawn a dedicated timer loop for a high-frequency job.
/// Uses `dispatch_job` for the actual work, maintaining a single source of truth.
fn start_dedicated_loop(job_id: &'static str, initial_delay: Duration, interval: Duration) {
    tauri::async_runtime::spawn(async move {
        sleep(initial_delay).await;
        log::info!("[{job_id}] dedicated timer started ({}s interval)", interval.as_secs());

        loop {
            let start = Instant::now();
            let result = dispatch_job(job_id).await;
            execute_and_log(job_id, result, false).await;

            // Account for execution time to maintain precise cadence
            let elapsed = start.elapsed();
            if elapsed < interval {
                sleep(interval - elapsed).await;
            }
        }
    });
}
