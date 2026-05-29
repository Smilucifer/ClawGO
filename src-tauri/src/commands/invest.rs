use crate::storage::invest::{
    events::{self, Event, EventSource},
    portfolio::{self, Holding, Trade},
    scheduler::{self, SchedulerLog},
    strategy,
    verdicts::{self, PnlSnapshot, Verdict},
};
use tauri::Emitter;

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
    };
    portfolio::record_trade(&t)
}

#[tauri::command]
pub fn get_trades(symbol: Option<String>, limit: Option<i64>) -> Result<Vec<Trade>, String> {
    portfolio::list_trades(symbol.as_deref(), limit)
}

// ── Cash ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_cash() -> Result<f64, String> {
    portfolio::get_cash()
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
    let client = crate::tushare::TushareClient::new(token);
    client.stock_basic(Some(&name)).await
}

#[tauri::command]
pub async fn get_latest_price(
    ts_code: String,
    token: String,
) -> Result<f64, String> {
    let client = crate::tushare::TushareClient::new(token);
    client.get_latest_price(&ts_code).await
}

#[tauri::command]
pub async fn get_daily_bars(
    ts_code: String,
    start_date: String,
    end_date: String,
    token: String,
) -> Result<Vec<crate::tushare::client::DailyBar>, String> {
    let client = crate::tushare::TushareClient::new(token);
    client.daily(&ts_code, &start_date, &end_date).await
}

// ── Trade Calendar Sync ──────────────────────────────────────────────────

#[tauri::command]
pub async fn sync_trade_calendar(token: String) -> Result<usize, String> {
    let client = crate::tushare::TushareClient::new(token);
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

    let mut providers = Vec::new();
    for pid in &["deepseek", "mimo_plan", "mimo_api"] {
        let obj = data.get(*pid).cloned().unwrap_or(serde_json::Value::Null);
        let display_id = match *pid {
            "deepseek" => "deepseek",
            "mimo_plan" => "mimo-plan",
            "mimo_api" => "mimo-api",
            _ => pid,
        };
        providers.push(InvestLlmProviderConfig {
            provider_id: display_id.to_string(),
            api_key: obj["api_key"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            base_url: obj["base_url"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            default_model: obj["default_model"]
                .as_str()
                .unwrap_or("")
                .to_string(),
        });
    }

    Ok(InvestLlmConfig {
        providers,
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

#[tauri::command]
pub async fn run_committee(
    symbols: Vec<String>,
    debate_rounds: Option<u8>,
) -> Result<Vec<crate::invest::committee::orchestrator::CommitteeResult>, String> {
    let config_data = get_llm_config()?;
    let client =
        crate::invest::llm::client::OpenAiCompatClient::new().map_err(|e| format!("init LLM client: {}", e))?;
    let client: std::sync::Arc<dyn crate::invest::llm::types::InvestLlmClient> =
        std::sync::Arc::new(client);

    let mut committee_config =
        crate::invest::committee::orchestrator::CommitteeConfig::default();
    // Explicit override > user config > default
    committee_config.debate_rounds = debate_rounds.unwrap_or(config_data.debate_rounds);
    committee_config.emergency_buffer_cny = config_data.emergency_buffer_cny;
    committee_config.timeout_secs = config_data.timeout_secs;

    let results = crate::invest::committee::orchestrator::run_committee_batch(
        client,
        &symbols,
        &committee_config,
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
) -> Result<Vec<crate::invest::committee::orchestrator::CommitteeResult>, String> {
    let config_data = get_llm_config()?;
    let client =
        crate::invest::llm::client::OpenAiCompatClient::new().map_err(|e| format!("init LLM client: {}", e))?;
    let client: std::sync::Arc<dyn crate::invest::llm::types::InvestLlmClient> =
        std::sync::Arc::new(client);

    let mut committee_config =
        crate::invest::committee::orchestrator::CommitteeConfig::default();
    committee_config.debate_rounds = debate_rounds.unwrap_or(config_data.debate_rounds);
    committee_config.emergency_buffer_cny = config_data.emergency_buffer_cny;
    committee_config.timeout_secs = config_data.timeout_secs;

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
    use crate::invest::committee::roles::{load_prompt, CommitteeRole};

    let mut map = std::collections::HashMap::new();
    for role in CommitteeRole::all() {
        let key = serde_json::to_value(role)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("{:?}", role));
        map.insert(key, load_prompt(*role));
    }
    Ok(map)
}

#[tauri::command]
pub fn save_role_prompt(role: String, content: String) -> Result<(), String> {
    use crate::invest::committee::roles::{save_prompt, CommitteeRole};

    let role_enum: CommitteeRole = match role.as_str() {
        "macro" => CommitteeRole::Macro,
        "quant_r1" => CommitteeRole::QuantR1,
        "risk_r1" => CommitteeRole::RiskR1,
        "wealth" => CommitteeRole::Wealth,
        "quant_r2" => CommitteeRole::QuantR2,
        "risk_r2" => CommitteeRole::RiskR2,
        "cio" => CommitteeRole::Cio,
        other => return Err(format!("unknown role: {}", other)),
    };
    save_prompt(role_enum, &content)
}

// ── Event Scanner ─────────────────────────────────────────────────────────

/// Build TushareClient + LLM client + config for event scanning.
/// Used by both the `scan_events` Tauri command and the background cron.
pub fn build_scan_clients() -> Result<(crate::tushare::TushareClient, crate::invest::llm::client::OpenAiCompatClient, crate::invest::llm::types::LlmConfig), String> {
    let settings = crate::storage::settings::get_user_settings();
    let token = settings
        .tushare_token
        .ok_or("no tushare_token configured")?;
    let tushare = crate::tushare::TushareClient::new(token);

    let config_data = get_llm_config()?;
    let client =
        crate::invest::llm::client::OpenAiCompatClient::new().map_err(|e| format!("init LLM client: {}", e))?;

    // Use the first provider that has an API key configured
    let provider_cfg = config_data
        .providers
        .iter()
        .find(|p| !p.api_key.is_empty())
        .ok_or("no LLM provider with an API key configured")?;

    let provider_id = match provider_cfg.provider_id.as_str() {
        "deepseek" => crate::invest::llm::types::ProviderId::DeepSeek,
        "mimo-plan" => crate::invest::llm::types::ProviderId::MiMoPlan,
        "mimo-api" => crate::invest::llm::types::ProviderId::MiMoApi,
        other => return Err(format!("unknown provider: {}", other)),
    };

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
) -> Result<crate::invest::event_scanner::ScanResult, String> {
    let (tushare, client, llm_config) = build_scan_clients()?;

    crate::invest::event_scanner::scan_events(
        &tushare,
        &client,
        &llm_config,
        normalizer_prompt.as_deref(),
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
