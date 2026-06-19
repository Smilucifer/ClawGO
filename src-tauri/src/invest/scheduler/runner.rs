use super::config;
use crate::storage::invest::scheduler::{is_trading_day, log_task_end, log_task_start};
use crate::tushare::client::TushareClient;
use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration, Instant};
use tokio_util::sync::CancellationToken;

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Set of currently-executing job ids. Used so the main loop, dedicated loops
/// and the manual `trigger_cron_job` command never concurrently run the same id.
static RUNNING_JOBS: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));

/// Try to claim exclusive execution of `id`. Returns `true` iff this caller
/// won the race and is now responsible for calling `release_job` (typically
/// via the `JobGuard` RAII helper).
pub fn try_acquire_job(id: &str) -> bool {
    let mut set = match RUNNING_JOBS.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    set.insert(id.to_string())
}

/// Release a job slot. Idempotent: removing an absent id is a no-op.
pub fn release_job(id: &str) {
    let mut set = match RUNNING_JOBS.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    set.remove(id);
}

/// RAII guard that releases a job slot on drop, including unwind from a panic.
pub struct JobGuard(pub String);

impl Drop for JobGuard {
    fn drop(&mut self) {
        release_job(&self.0);
    }
}

/// Shared dispatch for scheduler jobs. Called from both the background runner
/// loop and the manual `trigger_cron_job` Tauri command.
pub async fn dispatch_job(id: &str) -> Result<String, String> {
    match id {
        "pnl_snapshot" => {
            let result = crate::run_pnl_snapshot().await?;
            Ok(result)
        }
        "event_scan" => {
            let (tushare, llm_client, llm_config) = crate::commands::invest::build_scan_clients()?;
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
            let (_, llm_client, llm_config) = crate::commands::invest::build_scan_clients()?;
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
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
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

/// Run a dispatch future under a tokio task so any panic is captured by the
/// JoinHandle rather than aborting the surrounding loop. Outcome is funneled
/// through `execute_and_log` so success / failure / panic all leave a row in
/// `scheduler_logs` and update `last_run`/`last_status` consistently.
async fn run_dispatch_with_panic_catch<Fut>(job_id: &str, fut: Fut, compute_next: bool)
where
    Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    let id_owned = job_id.to_string();
    let handle = tokio::spawn(fut);
    match handle.await {
        Ok(result) => execute_and_log(&id_owned, result, compute_next).await,
        Err(join_err) => {
            log::error!("[scheduler] job {id_owned} panicked: {join_err}");
            execute_and_log(&id_owned, Err(format!("panic: {join_err}")), compute_next).await;
        }
    }
}

/// Acquire the per-job mutex, build the dispatch future, run it under panic
/// protection. If the slot is already held (manual trigger or a still-running
/// previous tick), this tick is skipped with a warn log — never blocked.
///
/// Returns `true` iff this caller actually executed the dispatch (success,
/// error, or panic), and `false` iff it was skipped because another caller
/// already held the slot. The caller should only advance `last_run` /
/// `next_run` when this returns `true`; otherwise a contended cron tick gets
/// silently rolled forward and the fire is permanently lost.
async fn run_job_guarded<F, Fut>(dispatch: Arc<F>, job_id: String, compute_next: bool) -> bool
where
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    if !try_acquire_job(&job_id) {
        log::warn!("[scheduler] job {job_id} already running, skipping this tick");
        return false;
    }
    let _guard = JobGuard(job_id.clone());
    let fut = (dispatch)(job_id.clone());
    run_dispatch_with_panic_catch(&job_id, fut, compute_next).await;
    true
}

/// Start the scheduler loop and dedicated timers. Call once from lib.rs setup.
///
/// `cancel` is the app-wide shutdown token. When tripped, every loop exits
/// cleanly and `RUNNING` is reset so a future `start()` can re-arm.
///
/// Jobs with `dedicated: true` run on their own precise timer loops;
/// all other jobs go through the main cron loop.
pub fn start<F, Fut>(dispatch: F, cancel: CancellationToken)
where
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    if RUNNING.swap(true, Ordering::SeqCst) {
        return; // already running
    }

    // Spawn dedicated loops for high-frequency jobs
    start_dedicated_loop(
        "jin10_collector",
        Duration::from_secs(10),
        Duration::from_secs(15),
        cancel.clone(),
    );
    start_dedicated_loop(
        "event_analyzer",
        Duration::from_secs(30),
        Duration::from_secs(10 * 60),
        cancel.clone(),
    );

    // Main scheduler loop for all non-dedicated jobs
    let dispatch = Arc::new(dispatch);
    let cancel_main = cancel.clone();
    tauri::async_runtime::spawn(async move {
        // Initial delay to let app finish setup
        tokio::select! {
            _ = sleep(Duration::from_secs(10)) => {}
            _ = cancel_main.cancelled() => {
                RUNNING.store(false, Ordering::SeqCst);
                log::info!("[scheduler] main loop cancelled before startup");
                return;
            }
        }

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

            // Sequential execution: shared LLM + Tushare quotas would burst
            // under parallel fan-out. Per-job mutex + panic catch isolate failures.
            for job_id in to_fire {
                let ran = run_job_guarded(dispatch.clone(), job_id.clone(), true).await;

                // Only advance last_run/next_run when this tick actually executed.
                // If another caller (manual trigger / dedicated loop / still-running
                // previous tick) held the slot, leave next_run alone so this fire
                // gets retried on the next tick — otherwise a contended fire is
                // silently lost and the UI shows a fake "just ran" status.
                if !ran {
                    continue;
                }

                let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
                if let Some(j) = jobs.iter_mut().find(|j| j.id == job_id) {
                    j.last_run = Some(now);
                    j.next_run = config::compute_next_run_for_job(j);
                    if let Err(e) = config::save_jobs(&jobs) {
                        log::error!("Failed to save job state: {e}");
                    }
                }
            }

            tokio::select! {
                _ = sleep(Duration::from_secs(60)) => {}
                _ = cancel_main.cancelled() => break,
            }
        }

        RUNNING.store(false, Ordering::SeqCst);
        log::info!("[scheduler] main loop exited (cancelled)");
    });
}

/// Spawn a dedicated timer loop for a high-frequency job.
/// Uses `dispatch_job` for the actual work, maintaining a single source of truth.
/// `cancel` lets app shutdown break the loop without leaving an orphaned task.
fn start_dedicated_loop(
    job_id: &'static str,
    initial_delay: Duration,
    interval: Duration,
    cancel: CancellationToken,
) {
    tauri::async_runtime::spawn(async move {
        tokio::select! {
            _ = sleep(initial_delay) => {}
            _ = cancel.cancelled() => {
                log::info!("[{job_id}] dedicated timer cancelled before start");
                return;
            }
        }
        log::info!(
            "[{job_id}] dedicated timer started ({}s interval)",
            interval.as_secs()
        );

        loop {
            let start = Instant::now();

            if try_acquire_job(job_id) {
                let _guard = JobGuard(job_id.to_string());
                let fut = dispatch_job(job_id);
                run_dispatch_with_panic_catch(job_id, fut, false).await;
            } else {
                log::warn!("[scheduler] dedicated job {job_id} already running, skipping tick");
            }

            // Account for execution time to maintain precise cadence
            let elapsed = start.elapsed();
            let to_sleep = interval.saturating_sub(elapsed);
            tokio::select! {
                _ = sleep(to_sleep) => {}
                _ = cancel.cancelled() => break,
            }
        }

        log::info!("[{job_id}] dedicated timer exited (cancelled)");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_acquire_release_cycle_is_exclusive() {
        let id = "scheduler_runner_test_acquire_release";
        release_job(id); // defensive cleanup from any leaked prior run

        assert!(try_acquire_job(id), "first acquire should succeed");
        assert!(
            !try_acquire_job(id),
            "second acquire while held should fail"
        );

        release_job(id);
        assert!(
            try_acquire_job(id),
            "acquire after release should succeed again"
        );
        release_job(id);
    }

    #[test]
    fn job_guard_releases_on_drop() {
        let id = "scheduler_runner_test_guard_drop";
        release_job(id); // ensure clean slot

        // First acquire succeeds and is bound to a guard.
        {
            assert!(try_acquire_job(id), "fresh slot, acquire must succeed");
            let _guard = JobGuard(id.to_string());
            // While guard is alive, slot is held.
            assert!(!try_acquire_job(id), "slot must be held while guard alive");
        }
        // After guard scope ends, Drop should have released the slot,
        // so a fresh acquire must succeed.
        assert!(
            try_acquire_job(id),
            "guard drop should have released the slot"
        );
        release_job(id);
    }
}
