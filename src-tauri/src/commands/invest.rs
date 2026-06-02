use crate::storage::invest::{
    events::{self, Event, EventSource},
    portfolio::{self, Holding, Trade},
    scheduler::{self, SchedulerLog},
    strategy,
    verdicts::{self, PnlSnapshot, Verdict},
};
use tauri::Emitter;

/// Maximum number of detail entries returned by `get_verdict_review_detail`.
const MAX_DETAIL_ENTRIES: i64 = 200;

// ── Holdings ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_holdings() -> Result<Vec<Holding>, String> {
    portfolio::list_holdings()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn add_holding(
    symbol: String,
    currency: String,
    kind: String,
    name: Option<String>,
    notional: f64,
    avg_cost: Option<f64>,
    shares: Option<f64>,
    entry_date: Option<String>,
    linked_verdict_id: Option<String>,
    notes: Option<String>,
    asset_type: Option<String>,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let h = Holding {
        symbol,
        currency,
        kind,
        name,
        notional,
        avg_cost,
        shares,
        entry_date,
        linked_verdict_id,
        notes,
        asset_type: asset_type.or_else(|| Some("stock".to_string())),
        created_at: now.clone(),
        updated_at: now,
    };
    portfolio::upsert_holding(&h)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn update_holding(
    symbol: String,
    currency: String,
    kind: String,
    name: Option<String>,
    notional: f64,
    avg_cost: Option<f64>,
    shares: Option<f64>,
    entry_date: Option<String>,
    linked_verdict_id: Option<String>,
    notes: Option<String>,
    asset_type: Option<String>,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let h = Holding {
        symbol,
        currency,
        kind,
        name,
        notional,
        avg_cost,
        shares,
        entry_date,
        linked_verdict_id,
        notes,
        asset_type: asset_type.or_else(|| Some("stock".to_string())),
        created_at: now.clone(),
        updated_at: now,
    };
    portfolio::upsert_holding(&h)
}

#[tauri::command]
pub fn delete_holding(symbol: String, currency: String, kind: String) -> Result<(), String> {
    portfolio::delete_holding(&symbol, &currency, &kind)
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
) -> Result<(), String> {
    let trade_id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now = chrono::Utc::now().to_rfc3339();
    let t = Trade {
        id: trade_id,
        symbol,
        currency,
        kind,
        action,
        shares,
        price,
        amount,
        notes,
        created_at: now,
        name,
        trade_date,
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
) -> Result<(), String> {
    let t = Trade {
        id,
        symbol,
        currency,
        kind,
        action,
        shares,
        price,
        amount,
        notes,
        created_at: String::new(),
        name,
        trade_date,
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

#[tauri::command]
pub fn set_initial_cash(amount: f64) -> Result<(), String> {
    portfolio::set_initial_cash(amount)
}

#[tauri::command]
pub fn update_cash(available: f64) -> Result<(), String> {
    portfolio::set_cash(available)
}

// ── Verdicts ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_verdicts(symbol: Option<String>, limit: Option<i64>) -> Result<Vec<Verdict>, String> {
    verdicts::list_verdicts(symbol.as_deref(), limit)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn save_verdict(
    id: Option<String>,
    symbol: String,
    name: Option<String>,
    verdict: String,
    confidence: Option<f64>,
    macro_signal: Option<String>,
    macro_strength: Option<f64>,
    reasoning: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    tokens_used: Option<i64>,
    latency_ms: Option<i64>,
) -> Result<(), String> {
    let vid = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now = chrono::Utc::now().to_rfc3339();
    let v = Verdict {
        id: vid,
        symbol,
        name,
        verdict,
        confidence,
        macro_signal,
        macro_strength,
        reasoning,
        model,
        provider,
        tokens_used,
        latency_ms,
        created_at: now,
    };
    verdicts::save_verdict(&v)
}

// ── PnL Snapshots ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_pnl_snapshots(limit: Option<i64>) -> Result<Vec<PnlSnapshot>, String> {
    verdicts::list_pnl_snapshots(limit)
}

#[tauri::command]
pub fn save_pnl_snapshot(
    snapshot_date: String,
    total_value: f64,
    cash: f64,
    holdings_value: f64,
    daily_pnl: Option<f64>,
    daily_pnl_pct: Option<f64>,
) -> Result<i64, String> {
    let s = PnlSnapshot {
        id: 0, // auto-increment
        snapshot_date,
        total_value,
        cash,
        holdings_value,
        daily_pnl,
        daily_pnl_pct,
        created_at: String::new(),
    };
    verdicts::save_pnl_snapshot(&s)
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
pub fn save_event(
    id: Option<String>,
    source: String,
    event_type: String,
    title: String,
    body: Option<String>,
    symbols: Option<String>,
    severity: Option<String>,
    stance: Option<String>,
) -> Result<(), String> {
    let eid = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now = chrono::Utc::now().to_rfc3339();
    let e = Event {
        id: eid,
        source,
        event_type,
        title,
        body,
        symbols,
        severity: severity.unwrap_or_else(|| "info".to_string()),
        stance: stance.unwrap_or_else(|| "neutral".to_string()),
        triggered: false,
        trigger_verdict_id: None,
        created_at: now,
    };
    events::save_event(&e)
}

#[tauri::command]
pub fn mark_event_triggered(id: String, verdict_id: Option<String>) -> Result<(), String> {
    events::mark_event_triggered(&id, verdict_id.as_deref())
}

#[tauri::command]
pub fn get_event_sources() -> Result<Vec<EventSource>, String> {
    events::list_event_sources()
}

#[tauri::command]
pub fn save_event_source(
    id: Option<String>,
    name: String,
    source_type: String,
    config: Option<String>,
    enabled: bool,
) -> Result<(), String> {
    let sid = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now = chrono::Utc::now().to_rfc3339();
    let s = EventSource {
        id: sid,
        name,
        source_type,
        config,
        enabled,
        last_poll_at: None,
        created_at: now,
    };
    events::save_event_source(&s)
}

// ── Scheduler ───────────────────────────────────────────────────────────────

#[tauri::command]
pub fn is_trading_day(date: String) -> Result<bool, String> {
    scheduler::is_trading_day(&date)
}

#[tauri::command]
pub fn get_scheduler_logs(
    task_name: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<SchedulerLog>, String> {
    scheduler::list_scheduler_logs(task_name.as_deref(), limit)
}

// ── Strategy ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_strategy(id: String) -> Result<Option<strategy::Strategy>, String> {
    strategy::get_strategy(&id)
}

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
    let now = chrono::Utc::now().to_rfc3339();
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

#[tauri::command]
pub async fn get_daily_bars(
    ts_code: String,
    start_date: String,
    end_date: String,
    token: String,
) -> Result<Vec<crate::tushare::client::DailyBar>, String> {
    let client = crate::tushare::TushareClient::with_token(token);
    client.daily(&ts_code, &start_date, &end_date).await
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

#[tauri::command]
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
                kind: kind.to_string(),
                name,
                notional: 0.0,
                avg_cost,
                shares,
                entry_date: None,
                linked_verdict_id: None,
                notes: Some("migrated from legacy".to_string()),
                asset_type: Some("stock".to_string()),
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

// ── LLM Config ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestLlmProviderConfig {
    pub provider_id: String,
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestLlmConfig {
    pub providers: Vec<InvestLlmProviderConfig>,
    /// The currently selected provider ID (e.g. "deepseek", "mimo-plan", "mimo-api").
    pub selected_provider: String,
    pub debate_rounds: u8,
    pub emergency_buffer_cny: f64,
    pub timeout_secs: u64,
}

fn llm_config_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".claw-go").join("invest").join("llm_config.json")
}

fn default_llm_config() -> InvestLlmConfig {
    InvestLlmConfig {
        providers: vec![
            InvestLlmProviderConfig {
                provider_id: "deepseek".to_string(),
                api_key: String::new(),
                base_url: "https://api.deepseek.com/v1".to_string(),
                default_model: "deepseek-v4-pro".to_string(),
            },
            InvestLlmProviderConfig {
                provider_id: "mimo-plan".to_string(),
                api_key: String::new(),
                base_url: "https://token-plan-cn.xiaomimimo.com/v1".to_string(),
                default_model: "mimo-v2.5-pro".to_string(),
            },
            InvestLlmProviderConfig {
                provider_id: "mimo-api".to_string(),
                api_key: String::new(),
                base_url: "https://api.xiaomimimo.com/v1".to_string(),
                default_model: "mimo-v2.5-pro".to_string(),
            },
        ],
        selected_provider: "deepseek".to_string(),
        debate_rounds: 4,
        emergency_buffer_cny: 100_000.0,
        timeout_secs: 120,
    }
}

#[tauri::command]
pub fn get_llm_config() -> Result<InvestLlmConfig, String> {
    let path = llm_config_path();
    if !path.exists() {
        return Ok(default_llm_config());
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("read llm_config: {}", e))?;
    let data: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("parse llm_config: {}", e))?;

    // Known non-provider keys in the config JSON
    const META_KEYS: &[&str] = &["selected_provider", "debate_rounds", "emergency_buffer_cny", "timeout_secs"];

    let mut providers = Vec::new();
    if let Some(obj) = data.as_object() {
        for (key, val) in obj {
            if META_KEYS.contains(&key.as_str()) {
                continue;
            }
            // Only process keys that look like provider configs (objects with api_key)
            if val.get("api_key").is_some() || val.get("base_url").is_some() {
                // Internal keys use underscores (mimo_plan), display IDs use hyphens (mimo-plan)
                let display_id = key.replace('_', "-");
                providers.push(InvestLlmProviderConfig {
                    provider_id: display_id,
                    api_key: val["api_key"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    base_url: val["base_url"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    default_model: val["default_model"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                });
            }
        }
    }

    Ok(InvestLlmConfig {
        providers,
        selected_provider: data["selected_provider"]
            .as_str()
            .unwrap_or("deepseek")
            .to_string(),
        debate_rounds: data["debate_rounds"].as_u64().unwrap_or(4) as u8,
        emergency_buffer_cny: data["emergency_buffer_cny"]
            .as_f64()
            .unwrap_or(100_000.0),
        timeout_secs: data["timeout_secs"].as_u64().unwrap_or(120),
    })
}

#[tauri::command]
pub fn save_llm_config(config: InvestLlmConfig) -> Result<(), String> {
    let path = llm_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create dir: {}", e))?;
    }

    let mut data = serde_json::Map::new();

    // Write providers in the nested key format expected by resolve_api_key()
    for p in &config.providers {
        let internal_id = match p.provider_id.as_str() {
            "deepseek" => "deepseek",
            "mimo-plan" => "mimo_plan",
            "mimo-api" => "mimo_api",
            other => other,
        };
        let mut obj = serde_json::Map::new();
        obj.insert("api_key".into(), serde_json::Value::String(p.api_key.clone()));
        obj.insert(
            "base_url".into(),
            serde_json::Value::String(p.base_url.clone()),
        );
        obj.insert(
            "default_model".into(),
            serde_json::Value::String(p.default_model.clone()),
        );
        data.insert(
            internal_id.to_string(),
            serde_json::Value::Object(obj),
        );
    }

    data.insert(
        "selected_provider".into(),
        serde_json::Value::String(config.selected_provider.clone()),
    );

    data.insert(
        "debate_rounds".into(),
        serde_json::Value::Number(config.debate_rounds.into()),
    );
    data.insert(
        "emergency_buffer_cny".into(),
        serde_json::json!(config.emergency_buffer_cny),
    );
    data.insert(
        "timeout_secs".into(),
        serde_json::Value::Number(config.timeout_secs.into()),
    );

    let json = serde_json::to_string_pretty(&serde_json::Value::Object(data))
        .map_err(|e| format!("serialize: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("write llm_config: {}", e))
}

// ── Committee ───────────────────────────────────────────────────────────────

/// Parse the selected_provider string into a ProviderId enum.
/// Returns DeepSeek as fallback for unknown IDs.
fn parse_provider_id(id: &str) -> crate::invest::llm::types::ProviderId {
    try_parse_provider_id(id).unwrap_or(crate::invest::llm::types::ProviderId::DeepSeek)
}

/// Parse a provider ID string, returning Err for unrecognized IDs.
fn try_parse_provider_id(id: &str) -> Result<crate::invest::llm::types::ProviderId, String> {
    match id {
        "deepseek" => Ok(crate::invest::llm::types::ProviderId::DeepSeek),
        "mimo-plan" => Ok(crate::invest::llm::types::ProviderId::MiMoPlan),
        "mimo-api" => Ok(crate::invest::llm::types::ProviderId::MiMoApi),
        _ => Err(format!("unknown provider: {}", id)),
    }
}

/// Build a CommitteeConfig from the saved InvestLlmConfig, applying the
/// selected provider to all roles.
fn build_committee_config(config_data: &InvestLlmConfig, debate_rounds: Option<u8>) -> crate::invest::committee::orchestrator::CommitteeConfig {
    let provider = parse_provider_id(&config_data.selected_provider);
    let mut committee_config = crate::invest::committee::orchestrator::CommitteeConfig::default();
    committee_config.debate_rounds = debate_rounds.unwrap_or(config_data.debate_rounds);
    committee_config.emergency_buffer_cny = config_data.emergency_buffer_cny;
    committee_config.timeout_secs = config_data.timeout_secs;
    // Pass user-configured model for the selected provider
    if let Some(p) = config_data.providers.iter().find(|p| p.provider_id == config_data.selected_provider) {
        if !p.default_model.is_empty() {
            committee_config.model_override = Some(p.default_model.clone());
        }
    }
    // Apply selected provider to all roles
    for role in crate::invest::committee::roles::CommitteeRole::all() {
        committee_config.role_providers.insert(*role, provider);
    }
    committee_config
}

#[tauri::command]
pub async fn run_committee(
    symbols: Vec<String>,
    debate_rounds: Option<u8>,
    dry_run: Option<bool>,
) -> Result<Vec<crate::invest::committee::orchestrator::CommitteeResult>, String> {
    let config_data = get_llm_config()?;
    let client =
        crate::invest::llm::client::OpenAiCompatClient::new().map_err(|e| format!("init LLM client: {}", e))?;
    let client: std::sync::Arc<dyn crate::invest::llm::types::InvestLlmClient> =
        std::sync::Arc::new(client);

    let committee_config = build_committee_config(&config_data, debate_rounds);

    let results = crate::invest::committee::orchestrator::run_committee_batch(
        client,
        &symbols,
        &committee_config,
        dry_run.unwrap_or(false),
    )
    .await;

    // Collect successes; report first error but still return partial results
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
    // If ALL failed, return the first error. If partial success, return what we have.
    if out.is_empty() {
        Err(first_err.unwrap_or_else(|| "all symbols failed".to_string()))
    } else {
        Ok(out)
    }
}

/// Streaming variant of `run_committee` — emits real-time `CommitteeEvent`s
/// on the `"committee-event"` Tauri event channel as each role starts/completes.
/// Returns the same `Vec<CommitteeResult>` as the non-streaming version.
#[tauri::command]
pub async fn run_committee_stream(
    app: tauri::AppHandle,
    symbols: Vec<String>,
    debate_rounds: Option<u8>,
    dry_run: Option<bool>,
) -> Result<Vec<crate::invest::committee::orchestrator::CommitteeResult>, String> {
    let config_data = get_llm_config()?;
    let client =
        crate::invest::llm::client::OpenAiCompatClient::new().map_err(|e| format!("init LLM client: {}", e))?;
    let client: std::sync::Arc<dyn crate::invest::llm::types::InvestLlmClient> =
        std::sync::Arc::new(client);

    let committee_config = build_committee_config(&config_data, debate_rounds);

    // Build emitter closure that forwards events to the Tauri event channel
    let emitter: crate::invest::committee::orchestrator::EventEmitter = {
        let app = app.clone();
        std::sync::Arc::new(move |event: crate::invest::committee::events::CommitteeEvent| {
            let _ = app.emit("committee-event", &event);
        })
    };

    let results = crate::invest::committee::orchestrator::run_committee_batch_stream(
        client,
        &symbols,
        &committee_config,
        emitter,
        dry_run.unwrap_or(false),
    )
    .await;

    // Collect successes; report first error but still return partial results
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

/// Build TushareClient + LLM client + config for event scanning.
/// Used by both the `scan_events` Tauri command and the background cron.
pub fn build_scan_clients() -> Result<(crate::tushare::TushareClient, crate::invest::llm::client::OpenAiCompatClient, crate::invest::llm::types::LlmConfig), String> {
    let tushare = crate::tushare::TushareClient::from_settings()?;

    let config_data = get_llm_config()?;
    let client =
        crate::invest::llm::client::OpenAiCompatClient::new().map_err(|e| format!("init LLM client: {}", e))?;

    // Use the first provider that has an API key configured
    let provider_cfg = config_data
        .providers
        .iter()
        .find(|p| !p.api_key.is_empty())
        .ok_or("no LLM provider with an API key configured")?;

    let provider_id = try_parse_provider_id(&provider_cfg.provider_id)?;

    let llm_config = crate::invest::llm::types::LlmConfig {
        provider: provider_id,
        model: provider_cfg.default_model.clone(),
        temperature: 0.7,
        max_tokens: 4096,
        timeout_secs: config_data.timeout_secs,
    };

    Ok((tushare, client, llm_config))
}

#[tauri::command]
pub async fn scan_events(
    normalizer_prompt: Option<String>,
    language: Option<String>,
) -> Result<crate::invest::event_scanner::ScanResult, String> {
    let (tushare, client, llm_config) = build_scan_clients()?;
    let lang = language.as_deref().unwrap_or(crate::invest::event_scanner::DEFAULT_LANGUAGE);

    crate::invest::event_scanner::scan_events(
        &tushare,
        &client,
        &llm_config,
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
    use crate::storage::invest::scheduler::{log_task_end, log_task_start};

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
pub fn get_tracking_status() -> Result<Vec<crate::storage::invest::verdict_tracking::TrackedVerdict>, String> {
    crate::storage::invest::verdict_tracking::list_active_tracking()
}

#[tauri::command]
pub fn list_all_tracking(limit: Option<i64>) -> Result<Vec<crate::storage::invest::verdict_tracking::TrackedVerdict>, String> {
    crate::storage::invest::verdict_tracking::list_all_tracking(limit)
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

// ── Daily Report ──────────────────────────────────────────────────────

#[tauri::command]
pub fn generate_daily_report() -> Result<String, String> {
    let data_dir = crate::storage::data_dir();
    crate::invest::daily_report::generate_daily_report(&data_dir)
}

#[tauri::command]
pub fn list_daily_reports(limit: Option<i64>) -> Result<Vec<crate::invest::daily_report::DailyReportRecord>, String> {
    crate::invest::daily_report::list_daily_reports(limit.unwrap_or(30))
}

// ── Regime Classification ──────────────────────────────────────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegimeResult {
    pub ts_code: String,
    pub regime: String,
    pub brief: String,
    pub metrics: std::collections::HashMap<String, f64>,
    pub computed_at: String,
}

#[tauri::command]
pub async fn get_regime_classification(ts_code: String, tushare_token: String) -> Result<RegimeResult, String> {
    use chrono::Local;
    use crate::tushare::client::TushareClient;

    let client = TushareClient::with_token(tushare_token);
    let result = crate::invest::regime::compute_regime_for_symbol(&client, &ts_code).await?;

    let m = &result.metrics;
    let mut metrics = std::collections::HashMap::new();
    metrics.insert("latest".into(), m.latest);
    metrics.insert("ma20".into(), m.ma20);
    metrics.insert("ma60".into(), m.ma60);
    metrics.insert("rsi14".into(), m.rsi14);
    metrics.insert("volatility_ann".into(), m.volatility_ann);
    metrics.insert("price_quantile_2y".into(), m.price_quantile_2y);

    Ok(RegimeResult {
        ts_code,
        regime: result.regime.to_string(),
        brief: result.reason,
        metrics,
        computed_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    })
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

    // Check Tushare
    let settings = crate::storage::settings::get_user_settings();
    if let Some(ref token) = settings.tushare_token {
        let client = TushareClient::with_token(token.clone());
        match client.get_latest_price("000001.SZ").await {
            Ok(price) => {
                sources.push(DataSourceStatus {
                    name: "Tushare".into(),
                    ok: true,
                    last_success: Some(now_str.clone()),
                    sample_value: Some(format!("000001.SZ = {:.2}", price)),
                });
            }
            Err(_) => {
                sources.push(DataSourceStatus {
                    name: "Tushare".into(),
                    ok: false,
                    last_success: None,
                    sample_value: None,
                });
            }
        }
    } else {
        sources.push(DataSourceStatus {
            name: "Tushare".into(),
            ok: false,
            last_success: None,
            sample_value: Some("no token configured".into()),
        });
    }

    // Check invest.db
    let db_ok = crate::storage::invest::with_conn(|_| Ok(())).is_ok();
    sources.push(DataSourceStatus {
        name: "invest.db".into(),
        ok: db_ok,
        last_success: if db_ok { Some(now_str.clone()) } else { None },
        sample_value: if db_ok { Some("connected".into()) } else { Some("connection failed".into()) },
    });

    // Check LLM config — read from ~/.claw-go/invest/llm_config.json
    let llm_path = llm_config_path();
    if llm_path.exists() {
        match std::fs::read_to_string(&llm_path) {
            Ok(content) => {
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(&content);
                match parsed {
                    Ok(data) => {
                        // Check that at least one provider has a non-empty api_key
                        let has_key = ["deepseek", "mimo_plan", "mimo_api"]
                            .iter()
                            .any(|pid| {
                                data.get(*pid)
                                    .and_then(|v| v.get("api_key"))
                                    .and_then(|v| v.as_str())
                                    .map(|s| !s.is_empty())
                                    .unwrap_or(false)
                            });
                        sources.push(DataSourceStatus {
                            name: "LLM Config".into(),
                            ok: has_key,
                            last_success: if has_key { Some(now_str.clone()) } else { None },
                            sample_value: if has_key {
                                Some("loaded".into())
                            } else {
                                Some("no api_key set".into())
                            },
                        });
                    }
                    Err(e) => {
                        sources.push(DataSourceStatus {
                            name: "LLM Config".into(),
                            ok: false,
                            last_success: None,
                            sample_value: Some(format!("parse error: {e}")),
                        });
                    }
                }
            }
            Err(e) => {
                sources.push(DataSourceStatus {
                    name: "LLM Config".into(),
                    ok: false,
                    last_success: None,
                    sample_value: Some(format!("read error: {e}")),
                });
            }
        }
    } else {
        sources.push(DataSourceStatus {
            name: "LLM Config".into(),
            ok: false,
            last_success: None,
            sample_value: Some("file not found".into()),
        });
    }

    // Check Yahoo Finance — fetch ^VIX as a connectivity probe
    let yahoo_client = crate::invest::international::InternationalClient::from_settings();
    match yahoo_client.fetch_yahoo_quote("^VIX").await {
        Ok(quote) => {
            sources.push(DataSourceStatus {
                name: "Yahoo Finance".into(),
                ok: true,
                last_success: Some(now_str.clone()),
                sample_value: Some(format!("^VIX = {:.2}", quote.price)),
            });
        }
        Err(e) => {
            sources.push(DataSourceStatus {
                name: "Yahoo Finance".into(),
                ok: false,
                last_success: None,
                sample_value: Some(e),
            });
        }
    }

    sources
}
