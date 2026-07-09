use crate::storage::invest::{
    events::{self, Event},
    portfolio::{self, Holding, Trade},
    scheduler,
    strategy,
    verdicts::{self, PnlSnapshot, Verdict},
};
use tauri::Emitter;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// Per-symbol cancellation tokens for in-flight committee pipelines.
/// Key = symbol. Managed as Tauri state.
pub type CommitteeCancelRegistry = Arc<Mutex<HashMap<String, CancellationToken>>>;

/// Build an empty cancel registry for `App::manage`.
pub fn new_committee_cancel_registry() -> CommitteeCancelRegistry {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Maximum number of detail entries returned by `get_verdict_review_detail`.
const MAX_DETAIL_ENTRIES: i64 = 200;

// ── Holdings ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_holdings() -> Result<Vec<Holding>, String> {
    portfolio::list_holdings()
}

#[tauri::command]
pub fn convert_watch_to_hold(
    symbol: String,
    currency: String,
    name: Option<String>,
    shares: f64,
    price: f64,
    asset_type: Option<String>,
) -> Result<(), String> {
    portfolio::convert_watch_to_hold(&symbol, &currency, name, shares, price, asset_type)
}

// ── Trades ──────────────────────────────────────────────────────────────────

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn record_trade(
    id: Option<String>,
    symbol: String,
    currency: String,
    kind: String,
    action: String,
    shares: Option<f64>,
    price: Option<f64>,
    amount: Option<f64>,
    notes: Option<String>,
    name: Option<String>,
    trade_date: Option<String>,
    asset_type: Option<String>,
    commission: Option<f64>,
    stamp_duty: Option<f64>,
) -> Result<(), String> {
    let trade_id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    // Normalize trade_date to YYYY-MM-DD: if the caller didn't supply one (e.g. sells
    // and system actions), derive it from created_at so the column is never NULL and
    // the UI never has to fall back to a locale-formatted (slash) date.
    let trade_date = trade_date
        .filter(|d| !d.trim().is_empty())
        .or_else(|| now.get(..10).map(|s| s.to_string()));
    let t = Trade {
        id: trade_id,
        symbol,
        currency,
        kind: kind.parse().unwrap_or_default(),
        action: action.parse().unwrap_or_default(),
        shares,
        price,
        amount,
        notes,
        created_at: now,
        name,
        trade_date,
        asset_type,
        commission,
        stamp_duty,
    };
    portfolio::record_trade(&t)
}

#[tauri::command]
pub fn get_trades(symbol: Option<String>, limit: Option<i64>) -> Result<Vec<Trade>, String> {
    portfolio::list_trades(symbol.as_deref(), limit)
}

#[tauri::command]
pub fn delete_trade(id: String) -> Result<(), String> {
    portfolio::delete_trade(&id)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn update_trade(
    id: String,
    symbol: String,
    currency: String,
    kind: String,
    action: String,
    shares: Option<f64>,
    price: Option<f64>,
    amount: Option<f64>,
    notes: Option<String>,
    name: Option<String>,
    trade_date: Option<String>,
    asset_type: Option<String>,
    commission: Option<f64>,
    stamp_duty: Option<f64>,
) -> Result<(), String> {
    let t = Trade {
        id,
        symbol,
        currency,
        kind: kind.parse().unwrap_or_default(),
        action: action.parse().unwrap_or_default(),
        shares,
        price,
        amount,
        notes,
        created_at: String::new(), // unused — UPDATE SQL preserves original
        name,
        trade_date,
        asset_type,
        commission,
        stamp_duty,
    };
    portfolio::update_trade(&t)
}

// ── Cash ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_cash() -> Result<f64, String> {
    portfolio::get_cash()
}

#[tauri::command]
pub fn get_initial_cash() -> Result<f64, String> {
    portfolio::get_initial_cash()
}

// ── Verdicts ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_verdicts(symbol: Option<String>, limit: Option<i64>) -> Result<Vec<Verdict>, String> {
    verdicts::list_verdicts(symbol.as_deref(), limit)
}

// ── PnL Snapshots ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_pnl_snapshots(limit: Option<i64>) -> Result<Vec<PnlSnapshot>, String> {
    verdicts::list_pnl_snapshots(limit)
}

#[tauri::command]
pub fn delete_pnl_snapshot(id: i64) -> Result<(), String> {
    verdicts::delete_pnl_snapshot(id)
}

// ── Events ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_events(source: Option<String>, limit: Option<i64>) -> Result<Vec<Event>, String> {
    events::list_events(source.as_deref(), limit)
}

#[tauri::command]
pub fn mark_event_triggered(id: String, verdict_id: Option<String>) -> Result<(), String> {
    events::mark_event_triggered(&id, verdict_id.as_deref())
}

// ── Strategy ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_strategies() -> Result<Vec<strategy::Strategy>, String> {
    strategy::list_strategies()
}

#[tauri::command]
pub fn save_strategy(
    id: Option<String>,
    name: String,
    targets: String,
    max_single_pct: Option<f64>,
    min_cash_pct: Option<f64>,
) -> Result<(), String> {
    let sid = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let parsed_targets: Vec<serde_json::Value> =
        serde_json::from_str(&targets).map_err(|e| format!("invalid targets JSON: {e}"))?;
    let s = strategy::Strategy {
        id: sid,
        name,
        targets: parsed_targets,
        max_single_pct,
        min_cash_pct,
        updated_at: now,
    };
    strategy::save_strategy(&s)
}

#[tauri::command]
pub fn delete_strategy(id: String) -> Result<(), String> {
    strategy::delete_strategy(&id)
}

// ── Tushare Market Data ───────────────────────────────────────────────────

#[tauri::command]
pub async fn search_stocks(
    name: String,
    token: String,
) -> Result<Vec<crate::tushare::client::StockBasic>, String> {
    let client = crate::tushare::TushareClient::with_token(token);
    client.stock_basic(Some(&name)).await
}

#[tauri::command]
pub async fn search_etfs(
    name: String,
    token: String,
) -> Result<Vec<crate::tushare::client::FundBasic>, String> {
    let client = crate::tushare::TushareClient::with_token(token);
    client.fund_basic(Some(&name)).await
}

#[tauri::command]
pub async fn get_latest_price(
    ts_code: String,
    token: String,
) -> Result<f64, String> {
    let client = crate::tushare::TushareClient::with_token(token);
    client.get_latest_price(&ts_code).await
}

/// 批量获取实时行情。股票用 `rt_k`（盘中最新价），ETF 降级到 `fund_daily`。
#[tauri::command]
pub async fn get_realtime_quotes(
    ts_codes: Vec<String>,
    token: String,
) -> Result<Vec<crate::tushare::client::RealtimeQuote>, String> {
    let client = crate::tushare::TushareClient::with_token(token);
    let refs: Vec<&str> = ts_codes.iter().map(|s| s.as_str()).collect();
    client.realtime_quotes(&refs).await
}

// ── Trade Calendar Sync ──────────────────────────────────────────────────

pub async fn sync_trade_calendar(token: String) -> Result<usize, String> {
    let client = crate::tushare::TushareClient::with_token(token);
    let today = chrono::Local::now();
    let start = today.format("%Y%m%d").to_string();
    let end = (today + chrono::Duration::days(730)).format("%Y%m%d").to_string();

    let cals = client.trade_cal("SSE", &start, &end).await?;
    let count = cals.len();
    for cal in &cals {
        scheduler::upsert_trade_calendar(
            &cal.cal_date,
            cal.is_open != 0,
            Some(&cal.pretrade_date),
        )?;
    }
    Ok(count)
}

// ── Legacy Migration ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn migrate_legacy_portfolio() -> Result<String, String> {
    let home = crate::storage::home_dir().ok_or("cannot find home dir")?;
    let home = std::path::PathBuf::from(home);
    let legacy_path = home.join(".claw-go").join("invest").join("portfolio.json");
    if !legacy_path.exists() {
        return Ok("no_legacy".to_string());
    }

    let content = std::fs::read_to_string(&legacy_path)
        .map_err(|e| format!("read legacy: {}", e))?;
    let data: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("parse legacy: {}", e))?;

    let mut migrated = 0;

    if let Some(cash) = data.get("cash").and_then(|v| v.as_f64()) {
        portfolio::set_cash(cash)?;
        migrated += 1;
    }

    if let Some(holdings) = data.get("holdings").and_then(|v| v.as_array()) {
        for h in holdings {
            let symbol = h.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
            let name = h.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
            let shares = h.get("shares").and_then(|v| v.as_f64());
            let avg_cost = h.get("avg_cost").or(h.get("cost")).and_then(|v| v.as_f64());
            let kind = h.get("kind").and_then(|v| v.as_str()).unwrap_or("hold");

            if symbol.is_empty() {
                continue;
            }

            let holding = portfolio::Holding {
                symbol: symbol.to_string(),
                currency: "CNY".to_string(),
                kind: kind.parse().unwrap_or_default(),
                name,
                notional: 0.0,
                avg_cost,
                shares,
                frozen_shares: None,
                entry_date: None,
                linked_verdict_id: None,
                notes: Some("migrated from legacy".to_string()),
                asset_type: Some("stock".to_string()),
                cleared_date: None,
                created_at: String::new(),
                updated_at: String::new(),
            };
            portfolio::upsert_holding(&holding)?;
            migrated += 1;
        }
    }

    let backup = legacy_path.with_extension("legacy");
    let _ = std::fs::rename(&legacy_path, &backup);

    Ok(format!("migrated {} items", migrated))
}

// ── Data Initialization ──────────────────────────────────────────────────────

/// Initialize invest data: clear all tables, sync trade calendar, set initial balance.
/// Returns a summary of what was initialized.
#[tauri::command]
pub async fn init_invest_data(
    token: String,
    initial_balance: Option<f64>,
) -> Result<String, String> {
    let mut steps = Vec::new();

    // 1. Clear all invest tables
    crate::storage::invest::clear_all_invest_data()?;
    steps.push("cleared all tables".to_string());

    // 2. Sync trade calendar
    match sync_trade_calendar(token).await {
        Ok(count) => steps.push(format!("trade_calendar: {} days", count)),
        Err(e) => steps.push(format!("trade_calendar: failed ({})", e)),
    }

    // 3. Set initial balance if provided
    if let Some(balance) = initial_balance {
        portfolio::set_cash(balance)?;
        portfolio::set_initial_cash(balance)?;
        steps.push(format!("initial_balance: ¥{:.2}", balance));
    }

    Ok(steps.join("; "))
}

// ── Committee Tuning ────────────────────────────────────────────────────────

/// Lightweight knobs for the committee pipeline, persisted to
/// `~/.claw-go/invest/committee_tuning.json`. Replaces the legacy
/// `llm_config.json` (which carried HTTP-side `api_key`/`base_url` for the
/// removed `OpenAiCompatClient`). Provider routing now goes through the CLI
/// executor + `write_committee_settings_json`, so all this struct needs is the
/// platform_id + model_override + a few scheduling knobs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitteeTuning {
    /// Platform credential id (matches `UserSettings.platform_credentials[].platform_id`)
    /// or `"default"` for the ambient `~/.claude` config.
    pub selected_provider: String,
    /// Optional model override; empty = use the CLI's native default.
    #[serde(default)]
    pub model: String,
    pub debate_rounds: u8,
    pub timeout_secs: u64,
    /// Maximum number of concurrent symbol pipelines (default 5).
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_symbols: usize,
}

fn default_max_concurrent() -> usize {
    5
}

fn committee_tuning_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".claw-go")
        .join("invest")
        .join("committee_tuning.json")
}

fn default_committee_tuning() -> CommitteeTuning {
    CommitteeTuning {
        selected_provider: "default".to_string(),
        model: String::new(),
        debate_rounds: 4,
        timeout_secs: 120,
        max_concurrent_symbols: 5,
    }
}

#[tauri::command]
pub fn get_committee_tuning() -> Result<CommitteeTuning, String> {
    let path = committee_tuning_path();
    if !path.exists() {
        return Ok(default_committee_tuning());
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("read committee_tuning: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("parse committee_tuning: {}", e))
}

#[tauri::command]
pub fn save_committee_tuning(tuning: CommitteeTuning) -> Result<(), String> {
    let path = committee_tuning_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {}", e))?;
    }
    let json = serde_json::to_string_pretty(&tuning).map_err(|e| format!("serialize: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("write committee_tuning: {}", e))
}

// ── Committee Mode Overrides ─────────────────────────────────────────────────

/// 每标的的分析模式覆盖表，持久化到 `~/.claw-go/invest/committee_mode_overrides.json`。
/// key = symbol，value = "research" | "holding"。只记录被用户手动改过、偏离默认推导的票
/// （默认：watch→research / hold→holding，由前端 store 推导）。后端只负责整表存/取，
/// 不参与默认值判定（persist 层不知道 symbol 的 kind）。
fn committee_mode_overrides_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".claw-go")
        .join("invest")
        .join("committee_mode_overrides.json")
}

#[tauri::command]
pub fn get_committee_mode_overrides() -> Result<std::collections::HashMap<String, String>, String> {
    let path = committee_mode_overrides_path();
    if !path.exists() {
        return Ok(std::collections::HashMap::new());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("read committee_mode_overrides: {}", e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("parse committee_mode_overrides: {}", e))
}

#[tauri::command]
pub fn save_committee_mode_overrides(
    overrides: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let path = committee_mode_overrides_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {}", e))?;
    }
    let json = serde_json::to_string_pretty(&overrides)
        .map_err(|e| format!("serialize mode overrides: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("write committee_mode_overrides: {}", e))
}

// ── Committee ───────────────────────────────────────────────────────────────

/// Parse a provider ID string into the `ProviderId` enum used by the
/// committee orchestrator (for archival labels at line 2029 only). Unknown IDs
/// (`"default"`, `custom-*`, ...) bubble up as `Err` — the caller treats that
/// as "no enum-mapped provider" and routes through `--settings` instead.
fn try_parse_provider_id(id: &str) -> Result<crate::invest::llm::types::ProviderId, String> {
    match id {
        "deepseek" => Ok(crate::invest::llm::types::ProviderId::DeepSeek),
        "mimo-plan" => Ok(crate::invest::llm::types::ProviderId::MiMoPlan),
        "mimo-api" => Ok(crate::invest::llm::types::ProviderId::MiMoApi),
        _ => Err(format!("unknown provider: {}", id)),
    }
}

/// Build a `CommitteeConfig` from the persisted `CommitteeTuning`, generating
/// a `--settings` JSON for CLI-based third-party provider routing. The
/// settings path is stored in `CommitteeConfig.settings_path` for the CLI
/// executor.
fn build_committee_config(
    tuning: &CommitteeTuning,
    debate_rounds: Option<u8>,
) -> Result<crate::invest::committee::orchestrator::CommitteeConfig, String> {
    // ProviderId enum is best-effort: only the three first-party platforms
    // (deepseek/mimo-plan/mimo-api) map cleanly. For "default" / custom-*
    // selections we fall back to DeepSeek for `role_providers`, but the actual
    // CLI invocation is routed via `settings_path` (write_committee_settings_json).
    let provider_for_archive = try_parse_provider_id(&tuning.selected_provider)
        .unwrap_or(crate::invest::llm::types::ProviderId::DeepSeek);

    let model_override = if tuning.model.is_empty() {
        None
    } else {
        Some(tuning.model.clone())
    };

    let mut committee_config = crate::invest::committee::orchestrator::CommitteeConfig {
        debate_rounds: debate_rounds.unwrap_or(tuning.debate_rounds),
        timeout_secs: tuning.timeout_secs,
        max_concurrent_symbols: tuning.max_concurrent_symbols.max(1),
        model_override: model_override.clone(),
        ..Default::default()
    };

    for role in crate::invest::committee::roles::CommitteeRole::all() {
        committee_config
            .role_providers
            .insert(*role, provider_for_archive);
    }

    // Generate --settings JSON for CLI executor (third-party provider routing).
    let platform_id = &tuning.selected_provider;
    let settings_result = crate::invest::committee::cli_executor::write_committee_settings_json(
        platform_id,
        model_override.as_deref(),
    );
    match settings_result {
        Ok(Some(path)) => {
            committee_config.settings_path = Some(path);
        }
        Ok(None) => {
            log::info!("build_committee_config: no --settings needed (default provider)");
        }
        Err(e) => {
            // Non-default provider selected but settings generation failed —
            // CLI would silently fall back to CC native config (wrong provider).
            // Surface the error so the user knows their provider selection didn't take effect.
            if platform_id != "default" && !platform_id.is_empty() {
                return Err(format!(
                    "无法为供应商 '{}' 生成 CLI 配置: {}。请检查 API Key 是否已配置。",
                    platform_id, e
                ));
            }
            log::warn!("build_committee_config: settings generation note: {e}");
        }
    }
    Ok(committee_config)
}

/// 把前端传来的 symbol→mode 字符串 map 解析成 Mode 枚举 map。
/// 未知/缺失值回退 Holding(向后兼容)。
fn parse_mode_map(
    modes: Option<std::collections::HashMap<String, String>>,
) -> std::collections::HashMap<String, crate::invest::committee::orchestrator::Mode> {
    use crate::invest::committee::orchestrator::Mode;
    modes
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| {
            let m = match v.as_str() {
                "research" => Mode::Research,
                _ => Mode::Holding,
            };
            (k, m)
        })
        .collect()
}

#[tauri::command]
pub async fn run_committee_stream(
    app: tauri::AppHandle,
    symbols: Vec<String>,
    debate_rounds: Option<u8>,
    dry_run: Option<bool>,
    modes: Option<std::collections::HashMap<String, String>>,
    cancel_registry: tauri::State<'_, CommitteeCancelRegistry>,
) -> Result<Vec<crate::invest::committee::orchestrator::CommitteeResult>, String> {
    let tuning = get_committee_tuning()?;
    let committee_config = build_committee_config(&tuning, debate_rounds)?;

    let emitter: crate::invest::committee::orchestrator::EventEmitter = {
        let app = app.clone();
        std::sync::Arc::new(move |event: crate::invest::committee::events::CommitteeEvent| {
            let _ = app.emit("committee-event", &event);
        })
    };

    // Register a fresh cancellation token per symbol. If a symbol is already in flight
    // (registry has a live token for it), skip it rather than overwriting — overwriting
    // would orphan the in-flight run's token (making it un-cancellable) and the later
    // remove() would also clobber the wrong batch's entry.
    let mut tokens: HashMap<String, CancellationToken> = HashMap::new();
    let mut skipped: Vec<String> = Vec::new();
    {
        let mut reg = cancel_registry
            .lock()
            .map_err(|e| format!("cancel registry poisoned: {e}"))?;
        for s in &symbols {
            if reg.contains_key(s) {
                log::warn!("[invest] committee run for {} already in flight, skipping duplicate", s);
                skipped.push(s.clone());
                continue;
            }
            let tok = CancellationToken::new();
            reg.insert(s.clone(), tok.clone());
            tokens.insert(s.clone(), tok);
        }
    }

    // Only run the symbols we actually registered (drop duplicates already in flight).
    let symbols: Vec<String> = symbols
        .into_iter()
        .filter(|s| !skipped.contains(s))
        .collect();

    let mode_map = parse_mode_map(modes);
    let results = crate::invest::committee::orchestrator::run_committee_batch_stream(
        &symbols,
        &committee_config,
        emitter,
        dry_run.unwrap_or(false),
        tokens,
        mode_map,
    )
    .await;

    // Clean up registry entries for this batch — only those we registered.
    {
        let mut reg = cancel_registry
            .lock()
            .map_err(|e| format!("cancel registry poisoned: {e}"))?;
        for s in &symbols {
            reg.remove(s);
        }
    }

    let mut out = Vec::with_capacity(results.len());
    let mut first_err: Option<String> = None;
    for r in results {
        match r {
            Ok(v) => out.push(v),
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
    }
    if out.is_empty() {
        Err(first_err.unwrap_or_else(|| "all symbols failed".to_string()))
    } else {
        Ok(out)
    }
}

// ── Abort commands ───────────────────────────────────────────────────────────

/// Cancel one in-flight committee symbol pipeline.
#[tauri::command]
pub fn abort_committee_symbol(
    cancel_registry: tauri::State<'_, CommitteeCancelRegistry>,
    symbol: String,
) -> Result<(), String> {
    let reg = cancel_registry
        .lock()
        .map_err(|e| format!("cancel registry poisoned: {e}"))?;
    if let Some(token) = reg.get(&symbol) {
        token.cancel();
    }
    Ok(())
}

/// Cancel all in-flight committee pipelines.
#[tauri::command]
pub fn abort_committee_all(
    cancel_registry: tauri::State<'_, CommitteeCancelRegistry>,
) -> Result<(), String> {
    let reg = cancel_registry
        .lock()
        .map_err(|e| format!("cancel registry poisoned: {e}"))?;
    for token in reg.values() {
        token.cancel();
    }
    Ok(())
}

/// Load the persisted committee live-queue state.
#[tauri::command]
pub fn load_committee_queue() -> Result<crate::invest::committee::queue::CommitteeQueueState, String>
{
    Ok(crate::invest::committee::queue::load_queue())
}

/// Persist the committee live-queue state.
#[tauri::command]
pub fn save_committee_queue(
    state: crate::invest::committee::queue::CommitteeQueueState,
) -> Result<(), String> {
    crate::invest::committee::queue::save_queue(&state)
}

// ── Role Prompts ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn load_committee_archive(
    symbol: String,
    days: Option<i64>,
) -> Result<Vec<crate::invest::committee::archive::ArchivedDecision>, String> {
    crate::invest::committee::archive::load_archive(&symbol, days.unwrap_or(7))
}

#[tauri::command]
pub fn get_role_prompts() -> Result<std::collections::HashMap<String, String>, String> {
    use crate::invest::committee::roles::{get_prompt_dir, CommitteeRole, Round};

    let mut map = std::collections::HashMap::new();
    for role in CommitteeRole::all() {
        let key = serde_json::to_value(role)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("{:?}", role));
        // R1 prompt — for Quant/Risk, read from the round-specific filename
        // (e.g. quant_r1.txt) which matches what save_prompt() writes to,
        // falling back to the legacy role-level filename (e.g. quant.txt).
        let r1_path = match role {
            CommitteeRole::Quant | CommitteeRole::Risk => {
                let round_path = get_prompt_dir().join(Round::R1.prompt_filename(*role));
                if round_path.exists() {
                    round_path
                } else {
                    get_prompt_dir().join(role.prompt_filename())
                }
            }
            _ => get_prompt_dir().join(role.prompt_filename()),
        };
        let r1 = std::fs::read_to_string(&r1_path)
            .unwrap_or_else(|_| role.default_prompt().to_string());
        map.insert(key.clone(), r1);

        // R2 prompt (quant and risk only)
        if matches!(role, CommitteeRole::Quant | CommitteeRole::Risk) {
            let r2_key = format!("{key}_r2");
            let r2_filename = match role {
                CommitteeRole::Quant => "quant_r2.txt",
                CommitteeRole::Risk => "risk_r2.txt",
                _ => unreachable!(),
            };
            let r2_path = get_prompt_dir().join(r2_filename);
            let r2 = std::fs::read_to_string(&r2_path)
                .unwrap_or_else(|_| role.default_r2_prompt().to_string());
            map.insert(r2_key, r2);
        }
    }
    Ok(map)
}

#[tauri::command]
pub fn save_role_prompt(role: String, content: String, round: Option<u8>) -> Result<(), String> {
    use crate::invest::committee::roles::{save_prompt, CommitteeRole};

    // Handle composite keys like "quant_r2", "risk_r2"
    let (base_role, effective_round) = if role.ends_with("_r2") {
        let base = role.strip_suffix("_r2").unwrap();
        let role_enum = match base {
            "quant" => CommitteeRole::Quant,
            "risk" => CommitteeRole::Risk,
            other => return Err(format!("unknown role: {}", other)),
        };
        (role_enum, 2u8)
    } else {
        let role_enum = match role.as_str() {
            "macro" => CommitteeRole::Macro,
            "quant" => CommitteeRole::Quant,
            "risk" => CommitteeRole::Risk,
            "cio" => CommitteeRole::Cio,
            other => return Err(format!("unknown role: {}", other)),
        };
        (role_enum, round.unwrap_or(1))
    };
    save_prompt(base_role, effective_round, &content)
}

// ── Event Scanner ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn scan_events(
    normalizer_prompt: Option<String>,
    language: Option<String>,
) -> Result<crate::invest::event_scanner::ScanResult, String> {
    let tushare = crate::tushare::TushareClient::from_settings()?;
    let lang = language.as_deref().unwrap_or(crate::invest::event_scanner::DEFAULT_LANGUAGE);

    crate::invest::event_scanner::scan_events(
        &tushare,
        normalizer_prompt.as_deref(),
        lang,
    )
    .await
}

#[tauri::command]
pub fn get_scan_status() -> Result<ScanStatus, String> {
    let (total_events, high_count, untriggered_high, last_event_at) = events::get_event_stats()?;

    Ok(ScanStatus {
        total_events,
        high_count,
        untriggered_high,
        last_event_at,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub total_events: usize,
    pub high_count: usize,
    pub untriggered_high: usize,
    pub last_event_at: Option<String>,
}

// ── Domain Insights ────────────────────────────────────────────────────

#[tauri::command]
pub fn list_insights(
    status: Option<String>,
    insight_type: Option<String>,
    symbol: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<crate::storage::invest::domain_insights::DomainInsight>, String> {
    crate::storage::invest::domain_insights::list_insights(
        status.as_deref(),
        insight_type.as_deref(),
        symbol.as_deref(),
        limit,
    )
}

#[tauri::command]
pub fn archive_insight(id: String) -> Result<(), String> {
    crate::storage::invest::domain_insights::archive_insight(&id)
}

#[tauri::command]
pub fn unarchive_insight(id: String) -> Result<(), String> {
    crate::storage::invest::domain_insights::unarchive_insight(&id)
}

#[tauri::command]
pub fn search_domain_insights(
    query: String,
    limit: Option<i64>,
) -> Result<Vec<crate::storage::invest::domain_insights::DomainInsight>, String> {
    crate::storage::invest::domain_insights::search_insights(&query, limit)
}

// ── Scheduler commands ──────────────────────────────────────────────

#[tauri::command]
pub fn list_cron_jobs() -> Result<Vec<crate::invest::scheduler::CronJob>, String> {
    Ok(crate::invest::scheduler::config::load_jobs())
}

#[tauri::command]
pub fn toggle_cron_job(id: String, enabled: bool) -> Result<(), String> {
    crate::invest::scheduler::config::toggle_job(&id, enabled)
}

#[tauri::command]
pub fn update_cron_schedule(id: String, cron_expr: String) -> Result<(), String> {
    crate::invest::scheduler::config::update_cron(&id, &cron_expr)
}

#[tauri::command]
pub fn get_cron_job_logs(
    task_name: String,
    limit: Option<i64>,
) -> Result<Vec<crate::storage::invest::scheduler::SchedulerLog>, String> {
    crate::storage::invest::scheduler::get_task_logs(&task_name, limit)
}

#[tauri::command]
pub async fn trigger_cron_job(id: String) -> Result<String, String> {
    use crate::invest::scheduler::runner::{try_acquire_job, JobGuard};
    use crate::storage::invest::scheduler::{log_task_end, log_task_start};

    if !try_acquire_job(&id) {
        return Err(format!("job {id} already running"));
    }
    let _guard = JobGuard(id.clone());

    let log_id = log_task_start(&id)?;
    let result = crate::invest::scheduler::runner::dispatch_job(&id).await;

    let status = if result.is_ok() { "ok" } else { "error" };
    let msg = match &result {
        Ok(m) => Some(m.as_str()),
        Err(e) => Some(e.as_str()),
    };
    let _ = log_task_end(log_id, status, msg);
    result
}

// ── Verdict Review commands ──────────────────────────────────────────

#[tauri::command]
pub async fn run_verdict_review_cmd(
    tushare_token: String,
) -> Result<crate::invest::verdict_review::VerdictReviewSummary, String> {
    crate::invest::verdict_review::run_verdict_review(&tushare_token).await
}

#[tauri::command]
pub fn get_verdict_review_summary(
) -> Result<crate::invest::verdict_review::VerdictReviewSummary, String> {
    use crate::storage::invest::verdict_reviews;
    let reviews = verdict_reviews::list_reviews(None, None)?;
    Ok(crate::invest::verdict_review::aggregate_from_stored(&reviews))
}

#[tauri::command]
pub fn get_verdict_review_detail(
    symbol: Option<String>,
) -> Result<Vec<crate::storage::invest::verdict_reviews::VerdictReviewEntry>, String> {
    crate::storage::invest::verdict_reviews::list_reviews(symbol.as_deref(), Some(MAX_DETAIL_ENTRIES))
}

// ── Dreaming commands ────────────────────────────────────────────────

#[tauri::command]
pub async fn trigger_dream(mode: String) -> Result<crate::invest::dreaming::DreamResult, String> {
    let settings = crate::storage::settings::get_user_settings();
    let tushare_token = settings
        .tushare_token
        .ok_or("No Tushare token configured")?;
    crate::invest::dreaming::trigger_dream(&mode, &tushare_token).await
}

#[tauri::command]
pub fn get_dream_config() -> Result<crate::invest::dreaming::DreamConfig, String> {
    Ok(crate::invest::scheduler::config::load_dream_config())
}

#[tauri::command]
pub fn save_dream_config(config: crate::invest::dreaming::DreamConfig) -> Result<(), String> {
    crate::invest::scheduler::config::save_dream_config(&config)
}

#[tauri::command]
pub fn list_dream_traces(dream_type: Option<String>, limit: Option<i64>) -> Result<Vec<crate::storage::invest::dream_snapshots::DreamSnapshot>, String> {
    crate::storage::invest::dream_snapshots::list_snapshots(dream_type.as_deref(), limit)
}

#[tauri::command]
pub fn rollback_dream(snapshot_id: i64) -> Result<(), String> {
    crate::invest::dreaming::snapshot::rollback_snapshot(snapshot_id)
}

// ── User Profile ──────────────────────────────────────────────────────

#[tauri::command]
pub fn get_user_profile() -> Result<crate::storage::invest::user_profile::UserProfile, String> {
    crate::storage::invest::user_profile::get_profile()
        .map(|opt| opt.unwrap_or_default())
}

#[tauri::command]
pub fn save_user_profile(profile: crate::storage::invest::user_profile::UserProfile) -> Result<(), String> {
    crate::storage::invest::user_profile::save_profile(&profile)
}

// ── Data Source Health ──────────────────────────────────────────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSourceStatus {
    pub name: String,
    pub ok: bool,
    pub last_success: Option<String>,
    pub sample_value: Option<String>,
}

#[tauri::command]
pub async fn get_datasource_health() -> Vec<DataSourceStatus> {
    use chrono::Local;
    use crate::tushare::client::TushareClient;

    let now_str = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut sources = Vec::new();

    // Check Tushare — stock API and news API separately (different permission tiers)
    let settings = crate::storage::settings::get_user_settings();
    if let Some(ref token) = settings.tushare_token {
        let client = TushareClient::with_token(token.clone());

        // Tushare 行情 (stock API)
        match client.get_latest_price("000001.SZ").await {
            Ok(price) => {
                sources.push(DataSourceStatus {
                    name: "Tushare 行情".into(),
                    ok: true,
                    last_success: Some(now_str.clone()),
                    sample_value: Some(format!("000001.SZ = {:.2}", price)),
                });
            }
            Err(e) => {
                sources.push(DataSourceStatus {
                    name: "Tushare 行情".into(),
                    ok: false,
                    last_success: None,
                    sample_value: Some(format!("{e}")),
                });
            }
        }


    } else {
        sources.push(DataSourceStatus {
            name: "Tushare 行情".into(),
            ok: false,
            last_success: None,
            sample_value: Some("no token configured".into()),
        });
    }

    // Tencent realtime quote (does not require a token; HTTP qt.gtimg.cn)
    let tencent_http = reqwest::Client::new();
    match crate::tencent_quotes::fetch_quotes(&tencent_http, &["000001.SZ"]).await {
        Ok(quotes) => {
            let sample = quotes
                .first()
                .map(|q| format!("{} = {:.2}", q.ts_code, q.close))
                .unwrap_or_else(|| "(empty)".into());
            sources.push(DataSourceStatus {
                name: "腾讯 实时行情".into(),
                ok: !quotes.is_empty(),
                last_success: if quotes.is_empty() { None } else { Some(now_str.clone()) },
                sample_value: Some(sample),
            });
        }
        Err(e) => {
            log::warn!("[datasource] 腾讯 实时行情 probe failed: {}", e);
            sources.push(DataSourceStatus {
                name: "腾讯 实时行情".into(),
                ok: false,
                last_success: None,
                sample_value: Some(e),
            });
        }
    }

    // Tencent Shanghai Composite K-line (used by macro_refresh fallback)
    match crate::tencent_quotes::fetch_index_kline(&tencent_http, "sh000001", 25).await {
        Ok(kline) => {
            sources.push(DataSourceStatus {
                name: "腾讯 上证指数 K线".into(),
                ok: true,
                last_success: Some(now_str.clone()),
                sample_value: Some(format!(
                    "close = {:.2}, vol20 = {}",
                    kline.close,
                    kline.vol20.map(|v| format!("{:.2}", v)).unwrap_or_else(|| "n/a".into())
                )),
            });
        }
        Err(e) => {
            log::warn!("[datasource] 腾讯 上证指数 K线 probe failed: {}", e);
            sources.push(DataSourceStatus {
                name: "腾讯 上证指数 K线".into(),
                ok: false,
                last_success: None,
                sample_value: Some(e),
            });
        }
    }

    // Check invest.db (light schema check — confirms the holdings table exists)
    let db_probe: Result<(), String> = crate::storage::invest::with_conn(|conn| {
        conn.query_row("SELECT 1 FROM holdings LIMIT 1", [], |_| Ok(()))
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(()),
                other => Err(format!("{other}")),
            })
    });
    match db_probe {
        Ok(()) => sources.push(DataSourceStatus {
            name: "invest.db".into(),
            ok: true,
            last_success: Some(now_str.clone()),
            sample_value: Some("connected, schema ok".into()),
        }),
        Err(e) => sources.push(DataSourceStatus {
            name: "invest.db".into(),
            ok: false,
            last_success: None,
            sample_value: Some(format!("schema check failed: {e}")),
        }),
    }

    // Python runtime — root dependency for AkShare / Jin10 / 东财海外
    match crate::python::require() {
        Ok(_) => sources.push(DataSourceStatus {
            name: "Python 运行时".into(),
            ok: true,
            last_success: Some(now_str.clone()),
            sample_value: Some("ready".into()),
        }),
        Err(e) => sources.push(DataSourceStatus {
            name: "Python 运行时".into(),
            ok: false,
            last_success: None,
            sample_value: Some(format!("{e}")),
        }),
    }

    // Check international data sources (AkShare + Jin10)
    let intl_client = crate::invest::international::InternationalClient::from_settings();

    // miniQMT (xtdata) — depends on Python runtime + QMT client
    match intl_client.fetch_xtdata_health().await {
        Ok(health) => {
            let sample = if health.available {
                "QMT 客户端在线".into()
            } else if health.reason.is_empty() {
                "QMT 客户端离线".into()
            } else {
                health.reason
            };
            sources.push(DataSourceStatus {
                name: "miniQMT 行情".into(),
                ok: health.available,
                last_success: if health.available { Some(now_str.clone()) } else { None },
                sample_value: Some(sample),
            });
        }
        Err(e) => {
            log::warn!("[datasource] miniQMT health probe failed: {}", e);
            sources.push(DataSourceStatus {
                name: "miniQMT 行情".into(),
                ok: false,
                last_success: None,
                sample_value: Some(format!("{e}")),
            });
        }
    }

    async fn probe_news(
        name: &str,
        now: &str,
        result: Result<Vec<crate::invest::international::NewsItem>, String>,
    ) -> DataSourceStatus {
        match result {
            Ok(items) => {
                if items.is_empty() {
                    // Empty result but no error — likely missing dependency or no data
                    DataSourceStatus {
                        name: name.into(),
                        ok: true,
                        last_success: Some(now.into()),
                        sample_value: Some("(empty)".into()),
                    }
                } else {
                    let sample = items.first().map(|i| i.title.chars().take(20).collect::<String>());
                    DataSourceStatus {
                        name: name.into(),
                        ok: true,
                        last_success: Some(now.into()),
                        sample_value: sample,
                    }
                }
            }
            Err(e) => {
                log::warn!("[datasource] {} probe failed: {}", name, e);
                DataSourceStatus {
                    name: name.into(),
                    ok: false,
                    last_success: None,
                    sample_value: Some(e),
                }
            }
        }
    }

    sources.push(probe_news("AkShare 个股", &now_str, intl_client.fetch_akshare_stock_news("000001", 1).await).await);
    sources.push(probe_news("金十数据", &now_str, intl_client.fetch_jinshi_news("", 1, None).await).await);

    // AkShare 10Y bond yield (used by macro_refresh fallback)
    match intl_client.fetch_akshare_bond_yield().await {
        Ok(b) => {
            sources.push(DataSourceStatus {
                name: "AkShare 10Y国债".into(),
                ok: true,
                last_success: Some(now_str.clone()),
                sample_value: Some(format!("yield = {:.3} ({})", b.yield_10y, b.date)),
            });
        }
        Err(e) => {
            log::warn!("[datasource] AkShare 10Y国债 probe failed: {}", e);
            sources.push(DataSourceStatus {
                name: "AkShare 10Y国债".into(),
                ok: false,
                last_success: None,
                sample_value: Some(e),
            });
        }
    }

    // AkShare market stats (limit-up / limit-down counts)
    let today_compact = Local::now().format("%Y%m%d").to_string();
    match intl_client.fetch_akshare_market_stats(&today_compact).await {
        Ok(stats) => {
            sources.push(DataSourceStatus {
                name: "AkShare 涨跌停".into(),
                ok: true,
                last_success: Some(now_str.clone()),
                sample_value: Some(format!(
                    "up = {}, down = {} ({})",
                    stats.limit_up_count, stats.limit_down_count, stats.date
                )),
            });
        }
        Err(e) => {
            log::warn!("[datasource] AkShare 涨跌停 probe failed: {}", e);
            sources.push(DataSourceStatus {
                name: "AkShare 涨跌停".into(),
                ok: false,
                last_success: None,
                sample_value: Some(e),
            });
        }
    }

    // 东财海外指标 (DXY / 美10Y — f59 解码直连)
    match intl_client.fetch_eastmoney_overseas("100.UDI").await {
        Ok(q) => sources.push(DataSourceStatus {
            name: "东财海外指标".into(),
            ok: true,
            last_success: Some(now_str.clone()),
            sample_value: Some(format!("DXY = {:.2}", q.value)),
        }),
        Err(e) => {
            log::warn!("[datasource] 东财海外指标 probe failed: {}", e);
            sources.push(DataSourceStatus {
                name: "东财海外指标".into(),
                ok: false,
                last_success: None,
                sample_value: Some(e),
            });
        }
    }

    // AkShare 海外标量 (VIX)
    match intl_client.fetch_akshare_overseas("overseas_vix").await {
        Ok(v) => sources.push(DataSourceStatus {
            name: "AkShare 海外指标".into(),
            ok: true,
            last_success: Some(now_str.clone()),
            sample_value: Some(format!("VIX = {:.2}", v.value)),
        }),
        Err(e) => {
            log::warn!("[datasource] AkShare 海外指标 probe failed: {}", e);
            sources.push(DataSourceStatus {
                name: "AkShare 海外指标".into(),
                ok: false,
                last_success: None,
                sample_value: Some(e),
            });
        }
    }

    sources
}

// ─── 宏观判断命令 ───────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroVerdictView {
    pub verdict: Option<crate::storage::invest::macro_verdict::MacroVerdict>,
    pub is_current: bool,
}

/// 读全局宏观判断 + 新鲜度(是否对应当前数据版本)。
#[tauri::command]
pub fn get_macro_verdict() -> Result<MacroVerdictView, String> {
    use crate::storage::invest::{macro_cache, macro_verdict};
    let verdict = macro_verdict::load_verdict()?;
    let is_current = match &verdict {
        Some(v) => macro_verdict::is_current(v, &macro_cache::current_data_version()?),
        None => false,
    };
    Ok(MacroVerdictView { verdict, is_current })
}

/// 手动刷新全局宏观判断(非交易时段内部会跳过真跑,复用收盘定版)。
#[tauri::command]
pub async fn refresh_macro_verdict() -> Result<String, String> {
    crate::invest::macro_verdict::run_macro_verdict(true).await
}

/// 读 macro_cache 的宏观快照(纯数据,前端全局卡片用)。
#[tauri::command]
pub fn get_macro_snapshot() -> Result<Option<crate::storage::invest::macro_cache::MacroSnapshot>, String> {
    Ok(crate::storage::invest::macro_cache::build_macro_snapshot())
}

// ─── 舆情采集命令 ───────────────────────────────────────────────────────────

/// 单源舆情抓取（写入 sentiment_items，不做归一化）。
///
/// `provider = "all"` 时 Python 端会聚合所有已注册 provider。返回写入条数。
#[tauri::command]
pub async fn fetch_sentiment(
    provider: String,
    symbol: Option<String>,
    limit: u32,
) -> Result<usize, String> {
    crate::invest::sentiment::fetch_and_store(&provider, symbol.as_deref(), limit).await
}

/// 从 tushare `stock_basic` 全量刷新 `stock_industry` 表（每周一次即可）。
#[tauri::command]
pub async fn refresh_stock_industry_cmd() -> Result<usize, String> {
    crate::invest::sentiment::refresh_stock_industry().await
}

/// 盘前采集编排：抓取四源 → 内联归一化到清零，一次返回归一化聚合结果。
#[tauri::command]
pub async fn collect_sentiment(
    symbol: Option<String>,
    limit: u32,
) -> Result<crate::invest::event_analyzer::AnalyzerResult, String> {
    crate::invest::sentiment::collect_all_sentiment(symbol.as_deref(), limit).await
}

// ─── 盘前观察报告命令 ───────────────────────────────────────────────────────

/// 手动生成一次盘前观察报告，返回 md 文件绝对路径。
///
/// 内部会跑 Plan A 四源归一化 + 雪球独立通道 + 宏观快照 + 四因子 SABC 打分 + AI 点评。
#[tauri::command]
pub async fn generate_premarket_report_cmd() -> Result<String, String> {
    let data_dir = crate::storage::data_dir();
    crate::invest::premarket::report::generate_premarket_report(&data_dir).await
}

/// 列出最近 N 份盘前报告的日期（倒序）。扫描 `{data_dir}/invest/reports/premarket_*.md`。
#[tauri::command]
pub fn list_premarket_reports(limit: usize) -> Result<Vec<String>, String> {
    let dir = crate::storage::data_dir()
        .join("invest")
        .join("reports");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut dates: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|e| format!("read reports dir: {e}"))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".md")?;
            let date = stem.strip_prefix("premarket_")?;
            Some(date.to_string())
        })
        .collect();
    dates.sort();
    dates.reverse();
    dates.truncate(limit);
    Ok(dates)
}

/// 读取指定日期盘前报告的 md + json 内容。
///
/// 返回 `{ "date", "markdown", "json" }`；任一文件缺失字段为 null。
#[tauri::command]
pub fn read_premarket_report(date: String) -> Result<serde_json::Value, String> {
    let dir = crate::storage::data_dir()
        .join("invest")
        .join("reports");
    let md_path = dir.join(format!("premarket_{date}.md"));
    let json_path = dir.join(format!("premarket_{date}.json"));
    let markdown = std::fs::read_to_string(&md_path).ok();
    let json_value: Option<serde_json::Value> = std::fs::read_to_string(&json_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    if markdown.is_none() && json_value.is_none() {
        return Err(format!("report not found for {date}"));
    }
    Ok(serde_json::json!({
        "date": date,
        "markdown": markdown,
        "json": json_value,
    }))
}

/// 读取盘前四因子权重 + 阈值配置。缺失走 `PremarketConfig::default()`。
#[tauri::command]
pub fn get_premarket_config_cmd() -> Result<crate::invest::premarket::scoring::PremarketConfig, String>
{
    Ok(crate::invest::premarket::scoring::get_premarket_config())
}

/// 保存盘前配置。校验：4 个权重和 ≈ 1.0（±0.001），阈值 S > A > B。
#[tauri::command]
pub fn save_premarket_config_cmd(
    config: crate::invest::premarket::scoring::PremarketConfig,
) -> Result<(), String> {
    let sum = config.weight_sentiment
        + config.weight_capital
        + config.weight_technical
        + config.weight_catalyst;
    if (sum - 1.0).abs() > 0.001 {
        return Err(format!("权重和必须为1.0，当前{:.3}", sum));
    }
    if !(config.threshold_s > config.threshold_a && config.threshold_a > config.threshold_b) {
        return Err("阈值须满足 S > A > B".to_string());
    }
    crate::invest::premarket::scoring::save_premarket_config(config)
}
