use super::config;
use crate::storage::invest::scheduler::{is_trading_day, log_task_end, log_task_start};
use crate::tushare::client::TushareClient;
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
                crate::invest::event_scanner::DEFAULT_LANGUAGE,
            )
            .await?;
            Ok(format!(
                "Scanned: {} fetched, {} saved",
                result.fetched, result.saved
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
            let today = crate::invest::date_utils::get_invest_date();

            for job in jobs {
                if !job.enabled {
                    continue;
                }

                // Check if it's time to fire using pre-computed next_run
                let should_fire = match &job.next_run {
                    Some(next) => chrono::NaiveDateTime::parse_from_str(next, "%Y-%m-%dT%H:%M:%S")
                        .map(|dt| chrono::Local::now().naive_local() >= dt)
                        .unwrap_or(true), // parse failure -> fire
                    None => true, // never run -> fire now
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

                // Update last_run/last_status and persist (single load + save)
                let now = chrono::Local::now()
                    .format("%Y-%m-%dT%H:%M:%S")
                    .to_string();
                let mut jobs_mut = config::load_jobs();
                if let Some(j) = jobs_mut.iter_mut().find(|j| j.id == job.id) {
                    j.last_run = Some(now);
                    j.last_status = Some(
                        if result.is_ok() { "ok" } else { "error" }.into(),
                    );
                    // Recompute next_run in-memory (load_jobs already does this,
                    // but we mutated last_run so recompute for the saved copy)
                    j.next_run = config::compute_next_run_for_job(j);
                    if let Err(e) = config::save_jobs(&jobs_mut) {
                        log::error!("Failed to save job state: {e}");
                    }
                }
            }

            sleep(Duration::from_secs(60)).await;
        }
    });
}
