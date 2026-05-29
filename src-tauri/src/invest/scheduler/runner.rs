use super::config;
use crate::storage::invest::scheduler::{is_trading_day, log_task_end, log_task_start};
use chrono::TimeZone;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

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
            )
            .await?;
            Ok(format!(
                "Scanned: {} fetched, {} saved",
                result.fetched, result.saved
            ))
        }
        "verdict_review" => {
            // TODO: implement in Task 3 — verdict_review module not yet available
            Err("verdict_review not yet implemented (Task 3)".into())
        }
        "dream_invest" => {
            // TODO: implement in Task 5 — dreaming module not yet available
            Err("dream_invest not yet implemented (Task 5)".into())
        }
        "dream_user" => {
            // TODO: implement in Task 5 — dreaming module not yet available
            Err("dream_user not yet implemented (Task 5)".into())
        }
        _ => Err(format!("Unknown job: {}", id)),
    }
}

/// Start the scheduler loop. Call once from lib.rs setup.
/// The `dispatch` callback maps job_id to the async function to execute.
pub fn start<F, Fut>(dispatch: F)
where
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    if RUNNING.swap(true, Ordering::SeqCst) {
        return; // already running
    }
    let dispatch = Arc::new(dispatch);
    tauri::async_runtime::spawn(async move {
        // Initial delay to let app finish setup
        sleep(Duration::from_secs(10)).await;
        loop {
            let jobs = config::load_jobs();
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();

            for job in jobs {
                if !job.enabled {
                    continue;
                }

                // Check if it's time to fire
                let should_fire = if let Some(interval) = job.interval_min {
                    // Interval-based: next_fire = last_run + interval
                    match &job.last_run {
                        Some(last) => {
                            let Ok(last_dt) = chrono::NaiveDateTime::parse_from_str(
                                last, "%Y-%m-%dT%H:%M:%S",
                            ) else {
                                continue;
                            };
                            let next = last_dt + chrono::Duration::minutes(interval);
                            chrono::Local::now().naive_local() >= next
                        }
                        None => true, // never run -> fire now
                    }
                } else {
                    // Cron-based
                    let Ok(schedule) = cron::Schedule::from_str(&job.cron_expr) else {
                        continue;
                    };
                    let after = match &job.last_run {
                        Some(last) => chrono::NaiveDateTime::parse_from_str(
                            last,
                            "%Y-%m-%dT%H:%M:%S",
                        )
                        .ok(),
                        None => None,
                    };
                    match after {
                        Some(after) => {
                            // Convert naive back to local for cron schedule comparison
                            let after_local = chrono::Local
                                .from_local_datetime(&after)
                                .single();
                            match after_local {
                                Some(after_local) => {
                                    if let Some(next) = schedule.after(&after_local).next() {
                                        chrono::Local::now() >= next
                                    } else {
                                        false
                                    }
                                }
                                None => true, // ambiguous/invalid local time -> fire
                            }
                        }
                        None => true, // never run -> fire now
                    }
                };

                if !should_fire {
                    continue;
                }

                // Trading day guard
                if job.requires_trading_day && !is_trading_day(&today).unwrap_or(false) {
                    if let Ok(id) = log_task_start(&job.id) {
                        let _ = log_task_end(id, "skipped", Some("non-trading day"));
                    }
                    continue;
                }

                // Execute
                let log_id = log_task_start(&job.id).ok();
                let result = (dispatch)(job.id.clone()).await;

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

                // Update last_run in config
                let now = chrono::Local::now()
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string();
                let mut jobs_mut = config::load_jobs();
                if let Some(j) = jobs_mut.iter_mut().find(|j| j.id == job.id) {
                    j.last_run = Some(now);
                    j.last_status = Some(
                        if result.is_ok() {
                            "ok"
                        } else {
                            "error"
                        }
                        .into(),
                    );
                    let _ = config::save_jobs(&jobs_mut);
                }
            }

            sleep(Duration::from_secs(60)).await;
        }
    });
}
