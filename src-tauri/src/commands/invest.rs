use crate::storage::invest::{
    events::{self, Event, EventSource},
    portfolio::{self, Holding, Trade},
    scheduler::{self, SchedulerLog},
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
