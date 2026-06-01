pub mod agent;
pub mod commands;
pub mod hooks;
pub mod models;
pub mod pricing;
pub mod process_ext;
pub mod group_chat;
pub mod storage;
pub mod tushare;
pub mod invest;
pub mod python;
pub mod web_server;

use agent::adapter::new_actor_session_map;
use agent::control::CliInfoCache;
use agent::spawn_locks::SpawnLocks;
use agent::stream::new_process_map;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::Arc;
use storage::events::EventWriter;
use tauri::tray::TrayIconEvent;
use tauri::Manager;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

/// Effective web server port (may differ from configured port if busy)
pub type EffectiveWebPort = Arc<AtomicU16>;
/// Web-server-specific cancel token for restart support
pub type WebServerCancel = Arc<tokio::sync::Mutex<CancellationToken>>;
/// Token version — shared between IPC and web server for rotation detection
pub type SharedTokenVersion = Arc<AtomicU64>;
/// WS shutdown broadcast — token rotation triggers disconnect of all WS clients
pub type WsShutdownSender = Arc<broadcast::Sender<()>>;
/// Live token — hot-swappable via RwLock for immediate login/logout on rotation
pub type SharedLiveToken = Arc<tokio::sync::RwLock<String>>;
/// Mutex to serialize web server start/stop operations
pub type WebServerLock = Arc<tokio::sync::Mutex<()>>;
/// JoinHandle for the serve task — await during stop to ensure port release
pub type WebServerHandle = Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>;
/// Generation counter — each spawn_server increments; stale tasks check before cleanup.
/// Newtype to avoid Tauri manage() collision with SharedTokenVersion (both Arc<AtomicU64>).
#[derive(Clone)]
pub struct WebServerGeneration(pub Arc<AtomicU64>);
/// Effective bind address — reflects actual running state (not settings).
/// Newtype to avoid Tauri manage() collision with SharedLiveToken (both Arc<RwLock<String>>).
#[derive(Clone)]
pub struct EffectiveWebBind(pub Arc<tokio::sync::RwLock<String>>);
/// Startup warning — populated when origins are degraded or other non-fatal startup issues.
#[derive(Clone)]
pub struct WebServerWarning(pub Arc<tokio::sync::RwLock<Option<String>>>);

/// One-shot gate to prevent concurrent shutdown tasks.
/// CAS ensures only the first caller proceeds; subsequent quit/close events are no-ops.
pub struct ShutdownGate(AtomicBool);

impl Default for ShutdownGate {
    fn default() -> Self {
        Self::new()
    }
}

impl ShutdownGate {
    pub fn new() -> Self {
        Self(AtomicBool::new(false))
    }
    /// Returns `true` if this call entered the gate (first caller wins).
    pub fn try_enter(&self) -> bool {
        self.0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

/// Run a PnL snapshot: get current holdings, calculate total value, save snapshot.
pub async fn run_pnl_snapshot() -> Result<String, String> {
    use crate::storage::invest::{portfolio, verdicts};

    let settings = crate::storage::settings::get_user_settings();
    let token = settings
        .tushare_token
        .ok_or("no tushare_token configured")?;

    let holdings = portfolio::list_holdings()?;
    let hold_items: Vec<_> = holdings.iter().filter(|h| h.kind == "hold").collect();
    if hold_items.is_empty() {
        return Ok("no holdings, skipping".to_string());
    }

    let cash = portfolio::get_cash()?;

    let client = crate::tushare::TushareClient::with_token_and_proxy(token, settings.tushare_proxy_url);
    let mut holdings_value = 0.0;
    for h in &hold_items {
        if let (Some(shares), Some(_cost)) = (h.shares, h.avg_cost) {
            match client.get_latest_price(&h.symbol).await {
                Ok(p) => holdings_value += p * shares,
                Err(e) => log::warn!("[invest-pnl] price fetch failed for {}: {}", h.symbol, e),
            }
        }
    }

    let total_value = cash + holdings_value;

    let prev = verdicts::list_pnl_snapshots(Some(1))?;
    let (daily_pnl, daily_pnl_pct) = if let Some(last) = prev.first() {
        let pnl = total_value - last.total_value;
        let pct = if last.total_value > 0.0 {
            (pnl / last.total_value) * 100.0
        } else {
            0.0
        };
        (Some(pnl), Some(pct))
    } else {
        (None, None)
    };

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let snapshot = verdicts::PnlSnapshot {
        id: 0,
        snapshot_date: today,
        total_value,
        cash,
        holdings_value,
        daily_pnl,
        daily_pnl_pct,
        created_at: String::new(),
    };
    let id = verdicts::save_pnl_snapshot(&snapshot)?;
    Ok(format!("saved snapshot #{}: total={:.2}", id, total_value))
}

/// Spawn the event scanner background cron job.
/// Runs every 30 minutes, fetching Tushare news/announcements, filtering by
/// keyword severity, normalizing via LLM, and saving new events to the DB.
fn spawn_event_scanner_cron() {
    tauri::async_runtime::spawn(async {
        // Initial delay before first scan (let app finish startup)
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        loop {
            log::info!("[invest-scanner] cron: starting scan");
            match run_event_scan_once().await {
                Ok(result) => {
                    log::info!(
                        "[invest-scanner] cron: scan complete — fetched={}, filtered={}, saved={}",
                        result.fetched, result.filtered, result.saved
                    );
                }
                Err(e) => {
                    // Config errors (missing token/provider) are expected when not set up
                    log::debug!("[invest-scanner] cron: scan skipped/failed: {}", e);
                }
            }

            // Sleep 30 minutes between scans
            tokio::time::sleep(std::time::Duration::from_secs(30 * 60)).await;
        }
    });
}

/// Run a single event scan: create clients and call the scanner.
async fn run_event_scan_once() -> Result<crate::invest::event_scanner::ScanResult, String> {
    let (tushare, client, llm_config) = crate::commands::invest::build_scan_clients()?;

    crate::invest::event_scanner::scan_events(&tushare, &client, &llm_config, None).await
}

pub fn run() {
    // Initialize logging — our crate at debug level by default
    // Override with RUST_LOG env var, e.g. RUST_LOG=warn cargo tauri dev
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("claw_go_lib=debug,warn"),
    )
    .format_timestamp_millis()
    .init();

    log::info!("ClawGO Desktop starting");

    // Set up Windows Job Object so child processes are killed on crash/force-quit.
    // No-op on non-Windows.
    process_ext::setup_job_kill_on_close();

    // Reconcile orphaned runs on startup
    storage::runs::reconcile_orphaned_runs();

    // Clean up legacy hook-bridge (removed: was redundant with stream-json mode)
    // Must run BEFORE migrate_native_hooks — strips stale bridge entries so only real user hooks are imported.
    hooks::setup::cleanup_hook_bridge();

    // One-time migration: import native hooks into Claw GO managed settings
    hooks::setup::migrate_native_hooks();

    // Global cancellation token — shared with all session actors for graceful shutdown
    let cancel_token = CancellationToken::new();
    let cancel_for_exit = cancel_token.clone();

    // Shared flag: true if system tray was successfully created
    let tray_ok = Arc::new(AtomicBool::new(false));
    let tray_ok_for_event = tray_ok.clone();

    // Web server shared state
    let ws_shutdown_sender: WsShutdownSender = Arc::new(broadcast::channel::<()>(1).0);
    let shared_token_version: SharedTokenVersion = Arc::new(AtomicU64::new(0));
    let shared_live_token: SharedLiveToken = {
        use rand::Rng;
        let token: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        log::debug!("[app] ephemeral web token generated (masked)");
        Arc::new(tokio::sync::RwLock::new(token))
    };
    let effective_web_port: EffectiveWebPort = Arc::new(AtomicU16::new(0));
    let ws_cancel: WebServerCancel = Arc::new(tokio::sync::Mutex::new(CancellationToken::new()));
    let ws_lock: WebServerLock = Arc::new(tokio::sync::Mutex::new(()));
    let ws_handle: WebServerHandle = Arc::new(tokio::sync::Mutex::new(None));
    let ws_generation = WebServerGeneration(Arc::new(AtomicU64::new(0)));
    let ws_effective_bind = EffectiveWebBind(Arc::new(tokio::sync::RwLock::new(String::new())));
    let ws_warning = WebServerWarning(Arc::new(tokio::sync::RwLock::new(None)));

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .manage(new_process_map())
        .manage(new_actor_session_map())
        .manage(CliInfoCache::new())
        .manage(Arc::new(EventWriter::new()))
        .manage(SpawnLocks::new())
        .manage(ShutdownGate::new())
        .manage(cancel_token)
        .manage(ws_shutdown_sender)
        .manage(shared_token_version)
        .manage(shared_live_token)
        .manage(effective_web_port)
        .manage(ws_cancel)
        .manage(ws_lock)
        .manage(ws_handle)
        .manage(ws_generation)
        .manage(ws_effective_bind)
        .manage(ws_warning)
        // NOTE: Currently ~60 IPC commands. If approaching 80+, consider grouping
        // into Tauri command modules or using a single dispatch command with typed payloads.
        .invoke_handler(tauri::generate_handler![
            commands::runs::list_runs,
            commands::runs::get_run,
            commands::runs::start_run,
            commands::runs::stop_run,
            commands::runs::update_run_model,
            commands::runs::rename_run,
            commands::runs::soft_delete_runs,
            commands::runs::search_prompts,
            commands::history::search_runs,
            commands::history::get_run_files,
            commands::runs::add_prompt_favorite,
            commands::runs::remove_prompt_favorite,
            commands::runs::update_prompt_favorite_tags,
            commands::runs::update_prompt_favorite_note,
            commands::runs::list_prompt_favorites,
            commands::runs::list_prompt_tags,
            commands::group_chat::list_group_chats,
            commands::group_chat::get_group_chat,
            commands::group_chat::create_group_chat,
            commands::group_chat::attach_group_chat_run,
            commands::group_chat::create_group_chat_participant,
            commands::group_chat::create_group_chat_claude_participant,
            commands::group_chat::update_group_chat_memo,
            commands::group_chat::send_group_chat_message,
            commands::group_chat::delete_group_chat,
            commands::group_chat::list_group_chat_run_index,
            commands::group_chat::get_group_chat_turn_snapshot,
            commands::group_chat::cancel_group_chat_turn,
            commands::group_chat::remove_group_chat_participant,
            commands::characters::list_characters,
            commands::characters::create_character,
            commands::characters::update_character,
            commands::characters::delete_character,
            commands::characters::list_character_memories,
            commands::characters::get_character_memory,
            commands::characters::create_character_memory,
            commands::characters::update_character_memory,
            commands::characters::delete_character_memory,
            commands::characters::search_character_memories,
            commands::characters::list_pending_memories,
            commands::characters::approve_memory,
            commands::characters::reject_memory,
            commands::avatar::upload_character_avatar,
            commands::memos::list_memos,
            commands::memos::add_memo,
            commands::memos::update_memo,
            commands::memos::delete_memo,
            commands::memos::clear_memos,
            commands::memos::list_memories,
            commands::memos::save_memory,
            commands::chat::send_chat_message,
            commands::events::get_run_events,
            commands::artifacts::get_run_artifacts,
            commands::balance::refresh_balance_status,
            commands::settings::get_user_settings,
            commands::settings::update_user_settings,
            commands::settings::validate_platform_credentials,
            commands::settings::get_agent_settings,
            commands::settings::update_agent_settings,
            commands::settings::list_managed_mcp_servers,
            commands::settings::add_managed_mcp_server,
            commands::settings::remove_managed_mcp_server,
            commands::settings::list_managed_hooks,
            commands::settings::add_managed_hook,
            commands::settings::remove_managed_hook,
            commands::settings::list_managed_plugins,
            commands::settings::set_managed_plugin,
            commands::settings::remove_managed_plugin,
            commands::fs::list_directory,
            commands::fs::check_is_directory,
            commands::fs::read_file_base64,
            commands::git::get_git_summary,
            commands::git::get_git_branch,
            commands::git::get_git_diff,
            commands::git::get_git_status,
            commands::export::export_conversation,
            commands::export::write_html_export,
            commands::files::read_text_file,
            commands::files::write_text_file,
            commands::files::read_task_output,
            commands::files::list_memory_files,
            commands::stats::get_usage_overview,
            commands::stats::get_global_usage_overview,
            commands::stats::clear_usage_cache,
            commands::stats::get_heatmap_daily,
            commands::stats::get_changelog,
            commands::diagnostics::check_agent_cli,
            commands::diagnostics::test_remote_host,
            commands::diagnostics::get_cli_dist_tags,
            commands::diagnostics::check_project_init,
            commands::diagnostics::check_ssh_key,
            commands::diagnostics::generate_ssh_key,
            commands::diagnostics::run_diagnostics,
            commands::diagnostics::detect_local_proxy,
            commands::diagnostics::test_api_connectivity,
            commands::diagnostics::get_windows_msvc_env_status,
            commands::session::start_session,
            commands::session::send_session_message,
            commands::session::stop_session,
            commands::session::send_session_control,
            commands::session::broadcast_mcp_toggle,
            commands::session::get_bus_events,
            commands::session::fork_session,
            commands::session::side_question,
            commands::session::start_ralph_loop,
            commands::session::cancel_ralph_loop,
            commands::session::approve_session_tool,
            commands::session::cancel_control_request,
            commands::session::respond_permission,
            commands::session::respond_hook_callback,
            commands::session::respond_elicitation,
            commands::control::get_cli_info,
            commands::teams::list_teams,
            commands::teams::get_team_config,
            commands::teams::list_team_tasks,
            commands::teams::get_team_task,
            commands::teams::get_team_inbox,
            commands::teams::get_all_team_inboxes,
            commands::teams::delete_team,
            commands::plugins::list_marketplaces,
            commands::plugins::list_marketplace_plugins,
            commands::plugins::list_standalone_skills,
            commands::plugins::list_project_commands,
            commands::plugins::get_skill_content,
            commands::plugins::list_installed_plugins,
            commands::plugins::install_plugin,
            commands::plugins::uninstall_plugin,
            commands::plugins::enable_plugin,
            commands::plugins::disable_plugin,
            commands::plugins::update_plugin,
            commands::plugins::add_marketplace,
            commands::plugins::remove_marketplace,
            commands::plugins::update_marketplace,
            commands::plugins::create_skill,
            commands::plugins::update_skill,
            commands::plugins::delete_skill,
            commands::plugins::check_community_health,
            commands::plugins::search_community_skills,
            commands::plugins::get_community_skill_detail,
            commands::plugins::install_community_skill,
            commands::agents::list_agents,
            commands::agents::read_agent_file,
            commands::agents::create_agent_file,
            commands::agents::update_agent_file,
            commands::agents::delete_agent_file,
            commands::clipboard::get_clipboard_files,
            commands::clipboard::read_clipboard_file,
            commands::clipboard::save_temp_attachment,
            commands::mcp::list_configured_mcp_servers,
            commands::mcp::add_mcp_server,
            commands::mcp::remove_mcp_server,
            commands::mcp::toggle_mcp_server_config,
            commands::mcp::get_disabled_mcp_servers,
            commands::mcp::check_mcp_registry_health,
            commands::mcp::search_mcp_registry,
            commands::cli_config::get_cli_config,
            commands::cli_config::get_project_cli_config,
            commands::cli_config::update_cli_config,
            commands::cli_settings::get_cli_permissions,
            commands::cli_settings::update_cli_permissions,
            commands::onboarding::check_auth_status,
            commands::onboarding::detect_install_methods,
            commands::onboarding::run_claude_login,
            commands::onboarding::get_auth_overview,
            commands::onboarding::set_cli_api_key,
            commands::onboarding::remove_cli_api_key,
            commands::screenshot::capture_screenshot,
            commands::screenshot::update_screenshot_hotkey,
            commands::cli_sync::discover_cli_sessions,
            commands::cli_sync::import_cli_session,
            commands::cli_sync::sync_cli_session,
            commands::updates::check_for_updates,
            commands::web_server::get_web_server_status,
            commands::web_server::get_web_server_token,
            commands::web_server::regenerate_web_server_token,
            commands::web_server::restart_web_server,
            commands::web_server::get_local_ip,
            commands::preview::open_preview_window,
            commands::preview::close_preview_window,
            commands::invest::get_holdings,
            commands::invest::add_holding,
            commands::invest::update_holding,
            commands::invest::delete_holding,
            commands::invest::record_trade,
            commands::invest::get_trades,
            commands::invest::delete_trade,
            commands::invest::update_trade,
            commands::invest::get_cash,
            commands::invest::get_initial_cash,
            commands::invest::set_initial_cash,
            commands::invest::update_cash,
            commands::invest::get_verdicts,
            commands::invest::save_verdict,
            commands::invest::get_pnl_snapshots,
            commands::invest::save_pnl_snapshot,
            commands::invest::delete_pnl_snapshot,
            commands::invest::get_events,
            commands::invest::save_event,
            commands::invest::mark_event_triggered,
            commands::invest::get_event_sources,
            commands::invest::save_event_source,
            commands::invest::is_trading_day,
            commands::invest::get_scheduler_logs,
            commands::invest::get_strategy,
            commands::invest::list_strategies,
            commands::invest::save_strategy,
            commands::invest::delete_strategy,
            commands::invest::search_stocks,
            commands::invest::search_etfs,
            commands::invest::get_latest_price,
            commands::invest::get_daily_bars,
            commands::invest::sync_trade_calendar,
            commands::invest::migrate_legacy_portfolio,
            commands::invest::get_llm_config,
            commands::invest::save_llm_config,
            commands::invest::run_committee,
            commands::invest::run_committee_stream,
            commands::invest::load_committee_archive,
            commands::invest::get_role_prompts,
            commands::invest::save_role_prompt,
            commands::invest::scan_events,
            commands::invest::get_scan_status,
            commands::invest::list_cron_jobs,
            commands::invest::toggle_cron_job,
            commands::invest::update_cron_schedule,
            commands::invest::get_cron_job_logs,
            commands::invest::trigger_cron_job,
            commands::invest::run_verdict_review_cmd,
            commands::invest::get_tracking_status,
            commands::invest::list_all_tracking,
            commands::invest::get_verdict_review_summary,
            commands::invest::get_verdict_review_detail,
            commands::invest::trigger_dream,
            commands::invest::get_dream_config,
            commands::invest::save_dream_config,
            commands::invest::list_dream_traces,
            commands::invest::rollback_dream,
            commands::invest::list_insights,
            commands::invest::archive_insight,
            commands::invest::unarchive_insight,
            commands::invest::search_domain_insights,
            commands::invest::get_user_profile,
            commands::invest::save_user_profile,
            commands::invest::generate_daily_report,
            commands::invest::list_daily_reports,
            commands::invest::get_regime_classification,
            commands::invest::get_datasource_health,
            commands::python_status::get_python_status,
            commands::python_status::restart_python_runtime,
        ])
        .setup(move |app| {
            // Set up broadcast emitter (requires AppHandle, so must be in setup)
            let broadcaster = web_server::broadcaster::EventBroadcaster::new();
            let writer = app.state::<Arc<EventWriter>>().inner().clone();
            let emitter = Arc::new(web_server::broadcaster::BroadcastEmitter::new(
                writer,
                app.handle().clone(),
                broadcaster.clone(),
            ));
            app.manage(broadcaster);
            app.manage(emitter);

            // Start web server (non-blocking, spawns async task)
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                match web_server::start_server(&app_handle).await {
                    Ok(true) => log::debug!("[app] web server started"),
                    Ok(false) => log::debug!("[app] web server disabled"),
                    Err(e) => log::error!("[app] web server failed to start: {}", e),
                }
            });

            // Initialize Python runtime (embedded data server for Yahoo Finance etc.)
            // init() handles everything: path resolution, subprocess spawn, health check,
            // version query, and frontend progress events — no separate verify step needed.
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = python::init(&app_handle).await {
                        log::warn!("[app] Python runtime init failed: {}", e);
                    }
                });
            }

            // Start team file watcher for ~/.claude/teams/ and ~/.claude/tasks/
            let cancel = app.state::<CancellationToken>().inner().clone();
            hooks::team_watcher::start_team_watcher(app.handle().clone(), cancel);

            // Start invest scheduler runner (background cron loop)
            invest::scheduler::runner::start(|job_id| async move {
                invest::scheduler::runner::dispatch_job(&job_id).await
            });

            // System tray — hide-to-tray on close, left-click to show
            // Non-fatal: if tray library is unavailable (e.g. some Linux desktops),
            // the app still works but window close = quit instead of hide-to-tray.
            match setup_tray(app) {
                Ok(_) => {
                    tray_ok.store(true, Ordering::Relaxed);
                }
                Err(e) => {
                    log::warn!("[app] tray unavailable: {e}, window close = quit");
                }
            }

            // Global shortcut plugin — must be registered inside setup() with a handler
            // so the event dispatch loop is properly initialized
            {
                use tauri_plugin_global_shortcut::ShortcutState;
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(|app, _shortcut, event| {
                            if event.state == ShortcutState::Pressed {
                                commands::screenshot::handle_global_shortcut(app);
                            }
                        })
                        .build(),
                )?;
            }

            // Register screenshot hotkey from settings (must come after plugin init)
            commands::screenshot::init_screenshot_hotkey(app.handle());

            Ok(())
        })
        .on_window_event(move |window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // Only intercept close for the main window
                    if window.label() != "main" {
                        return;
                    }
                    api.prevent_close(); // always prevent default close
                    if tray_ok_for_event.load(Ordering::Relaxed) {
                        // Hide to tray instead of quitting
                        let _ = window.hide();
                        log::debug!("[app] window hidden to tray");
                    } else {
                        // No tray — graceful shutdown
                        log::debug!("[app] tray unavailable, starting graceful shutdown");
                        let app = window.app_handle().clone();
                        if let Some(gate) = app.try_state::<ShutdownGate>() {
                            if !gate.try_enter() {
                                return; // shutdown already in progress
                            }
                        }
                        if let Some(ct) = app.try_state::<CancellationToken>() {
                            ct.cancel();
                        }
                        tauri::async_runtime::spawn(async move {
                            graceful_shutdown_actors(&app).await;
                            app.exit(0);
                        });
                    }
                }
                tauri::WindowEvent::Destroyed if window.label() == "main" => {
                    // Safety fallback: cancel actors if main window is truly destroyed (e.g. app.exit()).
                    // Skip for secondary windows (e.g. preview) — destroying them must not shut down the app.
                    cancel_for_exit.cancel();
                }
                _ => {}
            }
        });

    // Run character_id migration synchronously before Tauri binds IPC handlers.
    // Must happen before .build() to avoid racing with concurrent IPC handlers.
    match crate::group_chat::migration::migrate_participant_character_ids() {
        Ok(n) if n > 0 => log::info!("Migrated {} group chats to character_id linkage", n),
        Err(e) => log::warn!("Character ID migration failed: {}", e),
        _ => {}
    }

    // Initialize the user-centric memory database (SQLite + FTS5).
    let data_dir = crate::storage::data_dir();
    if let Err(e) = crate::storage::memory_store::init_db(&data_dir) {
        log::warn!("Failed to init memory DB: {}", e);
    }

    // Initialize invest database (holdings, trades, verdicts, events, scheduler).
    if let Err(e) = crate::storage::invest::init_db(&data_dir) {
        log::warn!("Failed to init invest DB: {}", e);
    }

    // Sync trade calendar on startup (non-blocking).
    {
        tauri::async_runtime::spawn(async move {
            let settings = crate::storage::settings::get_user_settings();
            if let Some(token) = settings.tushare_token {
                let client = crate::tushare::TushareClient::with_token_and_proxy(token, settings.tushare_proxy_url);
                let today = chrono::Local::now();
                let start = today.format("%Y%m%d").to_string();
                let end = (today + chrono::Duration::days(730)).format("%Y%m%d").to_string();
                match client.trade_cal("SSE", &start, &end).await {
                    Ok(cals) => {
                        let mut count = 0;
                        for cal in &cals {
                            if crate::storage::invest::scheduler::upsert_trade_calendar(
                                &cal.cal_date,
                                cal.is_open != 0,
                                Some(&cal.pretrade_date),
                            )
                            .is_ok()
                            {
                                count += 1;
                            }
                        }
                        log::info!("[invest] synced {} trade calendar entries", count);
                    }
                    Err(e) => log::warn!("[invest] trade calendar sync failed: {}", e),
                }
            } else {
                log::debug!("[invest] no tushare_token, skipping calendar sync");
            }
        });
    }

    // Start PnL snapshot cron job.
    // Runs at 9:30, 11:00, 13:00, 15:00 Beijing time on weekdays.
    {
        tauri::async_runtime::spawn(async {
            use chrono::TimeZone;

            // Target times: 9:30, 11:00, 13:00, 15:00
            let target_times: [(u32, u32); 4] = [(9, 30), (11, 0), (13, 0), (15, 0)];

            loop {
                let now = chrono::Local::now();
                let mut next_run = None;

                for &(hour, minute) in &target_times {
                    let candidate = now
                        .date_naive()
                        .and_hms_opt(hour, minute, 0)
                        .unwrap();
                    let candidate = chrono::Local.from_local_datetime(&candidate).unwrap();
                    if candidate > now {
                        next_run = Some(candidate);
                        break;
                    }
                }

                let next = next_run.unwrap_or_else(|| {
                    let tomorrow = now.date_naive() + chrono::Duration::days(1);
                    let t = tomorrow.and_hms_opt(9, 30, 0).unwrap();
                    chrono::Local.from_local_datetime(&t).unwrap()
                });

                let sleep_duration = (next - now).to_std().unwrap_or(std::time::Duration::from_secs(3600));
                log::info!("[invest-pnl] next snapshot at {:?}", next);
                tokio::time::sleep(sleep_duration).await;

                // Guard: only run on trading days
                let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
                match crate::storage::invest::scheduler::is_trading_day(&date_str) {
                    Ok(true) => {}
                    _ => {
                        log::debug!("[invest-pnl] skipping non-trading day {}", date_str);
                        continue;
                    }
                }

                let task_id = match crate::storage::invest::scheduler::log_task_start("pnl_snapshot") {
                    Ok(id) => id,
                    Err(e) => {
                        log::warn!("[invest-pnl] failed to log start: {}", e);
                        continue;
                    }
                };

                let result = run_pnl_snapshot().await;
                match &result {
                    Ok(msg) => {
                        let _ = crate::storage::invest::scheduler::log_task_end(task_id, "success", Some(msg));
                        log::info!("[invest-pnl] snapshot: {}", msg);
                    }
                    Err(e) => {
                        let _ = crate::storage::invest::scheduler::log_task_end(task_id, "error", Some(e));
                        log::warn!("[invest-pnl] failed: {}", e);
                    }
                }
            }
        });
    }

    // Start event scanner cron job.
    // Runs every 30 minutes, scanning Tushare news and announcements for
    // HOLD/WATCH portfolio items, normalizing via LLM, saving new events.
    spawn_event_scanner_cron();

    // Run memory migration from per-character JSONL to SQLite (idempotent).
    match crate::group_chat::memory_migration::migrate_jsonl_to_sqlite(&data_dir) {
        Ok(n) if n > 0 => log::info!("Migrated {} memories from JSONL to SQLite", n),
        Err(e) => log::warn!("Memory migration failed: {}", e),
        _ => {}
    }

    // Start background dream cycle task (memory consolidation).
    // Controlled by UserSettings.memory_dream_enabled (default: true).
    let dream_data_dir = data_dir.clone();
    tauri::async_runtime::spawn(async move {
        // Initial delay to let the app finish startup
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        loop {
            // Check if dream cycle is enabled in settings
            let enabled = crate::storage::settings::get_user_settings().memory_dream_enabled;
            if enabled {
                let data_dir = dream_data_dir.clone();
                match tokio::task::spawn_blocking(move || {
                    crate::group_chat::memory_dream::run_dream_cycle(&data_dir)
                })
                .await
                {
                    Ok(Ok(crate::group_chat::memory_dream::DreamCycleResult::Completed {
                        merged,
                        decayed,
                        ..
                    })) => {
                        log::info!(
                            "[dream] cycle completed: merged={}, decayed={}",
                            merged,
                            decayed
                        );
                    }
                    Ok(Ok(crate::group_chat::memory_dream::DreamCycleResult::Skipped)) => {
                        log::debug!("[dream] cycle skipped (too soon)");
                    }
                    Ok(Err(e)) => {
                        log::warn!("[dream] cycle failed: {}", e);
                    }
                    Err(e) => {
                        log::warn!("[dream] task panicked: {}", e);
                    }
                }
            } else {
                log::debug!("[dream] cycle skipped (disabled in settings)");
            }
            // Sleep for the dream interval before next check
            tokio::time::sleep(std::time::Duration::from_secs(
                crate::group_chat::memory_dream::DREAM_INTERVAL_SECS,
            ))
            .await;
        }
    });

    let app = builder.build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        // macOS: clicking the dock icon when all windows are hidden should reopen the window
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen {
            has_visible_windows,
            ..
        } = event
        {
            if !has_visible_windows {
                show_main_window(app_handle);
                log::debug!("[app] reopened window from dock click");
            }
        }

        let _ = (app_handle, event); // suppress unused warnings on non-macOS
    });
}

/// Restore the main window: unminimize if needed, then show and focus.
fn show_main_window(handle: &impl tauri::Manager<tauri::Wry>) {
    if let Some(w) = handle.get_webview_window("main") {
        if w.is_minimized().unwrap_or(false) {
            let _ = w.unminimize();
        }
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Create system tray with Show/Quit menu. Left-click shows the window.
fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder};

    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &separator, &quit])?;

    let tray_icon_bytes = include_bytes!("../icons/tray-icon.png");
    let tray_img =
        tauri::image::Image::from_bytes(tray_icon_bytes).expect("failed to load tray icon");

    TrayIconBuilder::new()
        .icon(tray_img)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => {
                show_main_window(app);
            }
            "quit" => {
                if let Some(gate) = app.try_state::<ShutdownGate>() {
                    if !gate.try_enter() {
                        return; // shutdown already in progress
                    }
                }
                if let Some(ct) = app.try_state::<CancellationToken>() {
                    ct.cancel();
                }
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    graceful_shutdown_actors(&app).await;
                    app.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    log::debug!("[app] system tray created");
    Ok(())
}

/// Graceful shutdown: wait for actors to self-clean, then force-kill remaining processes.
///
/// Two-phase approach:
/// - Phase 1: Wait up to 3s for actors to exit (cancel token already fired → handle_stop → kill+wait).
/// - Phase 2: Drain remaining actors, try_send Stop, join with 2s timeout, abort if stuck.
/// - Then drain ProcessMap (stream processes).
async fn graceful_shutdown_actors(app: &tauri::AppHandle) {
    use crate::agent::adapter::ActorSessionMap;
    use crate::agent::session_actor::ActorCommand;
    use crate::agent::stream::ProcessMap;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);

    // ── Phase 1: Wait for actors to self-cleanup (cancel already fired) ──
    if let Some(sessions) = app.try_state::<ActorSessionMap>() {
        loop {
            let count = sessions.lock().await.len();
            if count == 0 {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                log::warn!(
                    "[app] graceful shutdown: {} actors still alive, force stopping",
                    count
                );
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }

        // ── Phase 2: Force-stop remaining actors ──
        let remaining: Vec<_> = {
            let mut map = sessions.lock().await;
            map.drain().collect()
        };
        for (run_id, handle) in remaining {
            log::debug!("[app] force stopping actor: {}", run_id);
            // try_send avoids blocking if mailbox is full (bounded channel, 64 slots)
            let (reply_tx, _reply_rx) = tokio::sync::oneshot::channel();
            let _ = handle
                .cmd_tx
                .try_send(ActorCommand::Stop { reply: reply_tx });
            // Get AbortHandle before consuming JoinHandle in timeout
            let abort = handle.join_handle.abort_handle();
            match tokio::time::timeout(std::time::Duration::from_secs(2), handle.join_handle).await
            {
                Ok(Ok(())) => {
                    log::debug!("[app] actor {} exited cleanly", run_id);
                }
                Ok(Err(e)) => {
                    log::warn!("[app] actor {} join error: {}", run_id, e);
                }
                Err(_) => {
                    log::warn!("[app] actor {} did not exit in 2s, aborting task", run_id);
                    abort.abort();
                }
            }
        }
    }

    // ── Kill remaining stream processes ──
    // ProcessMap lock is only held briefly (run_agent/stop_process do remove-then-await),
    // but we keep a timeout as a defensive fallback.
    if let Some(process_map) = app.try_state::<ProcessMap>() {
        let to_kill = match tokio::time::timeout(std::time::Duration::from_secs(1), async {
            let mut map = process_map.lock().await;
            map.drain().collect::<Vec<_>>()
        })
        .await
        {
            Ok(vec) => vec,
            Err(_) => {
                log::warn!(
                    "[app] graceful shutdown: ProcessMap lock timeout, \
                     skipping (kill_on_drop / Job Object may handle)"
                );
                Vec::new()
            }
        };
        for (run_id, mut child) in to_kill {
            log::debug!("[app] graceful shutdown: killing stream process {}", run_id);
            let _ = child.kill().await;
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await;
        }
    }

    log::debug!("[app] graceful shutdown complete");
}
