use crate::storage::invest::{
    events::{self, Event, EventSource},
    portfolio::{self, Holding, Trade},
    scheduler::{self, SchedulerLog},
    strategy,
    verdicts::{self, PnlSnapshot, Verdict},
};

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
        triggered: false,
        trigger_verdict_id: None,
        created_at: now,
    };
    events::save_event(&e)
}

#[tauri::command]
pub fn mark_event_triggered(id: String, verdict_id: String) -> Result<(), String> {
    events::mark_event_triggered(&id, &verdict_id)
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
