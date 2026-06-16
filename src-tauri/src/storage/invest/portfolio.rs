use super::{is_etf_symbol, with_conn, with_conn_mut};
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};

// ── Macro: Display + FromSql + ToSql for string-backed enums ─────────────

macro_rules! sql_string_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident {
        $( $(#[$vmeta:meta])* $variant:ident => $str:expr ),+ $(,)?
    }) => {
        $(#[$meta])*
        $vis enum $name {
            $( $(#[$vmeta])* $variant ),+
        }

        impl $name {
            pub fn as_str(&self) -> &'static str {
                match self { $(Self::$variant => $str),+ }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl rusqlite::types::FromSql for $name {
            fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
                let s = value.as_str()?;
                Ok(s.parse::<$name>().unwrap_or_default())
            }
        }

        impl rusqlite::types::ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(rusqlite::types::ToSqlOutput::from(self.as_str()))
            }
        }
    };
}

// ── Type-safe enums ────────────────────────────────────────────────────────

// Trade action type — replaces raw strings for type safety.
// `Unknown` is a fallback for legacy DB values that survive migration
// (e.g. `convert_hold_to_watch`). Such trades are ignored during replay.
sql_string_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum TradeAction {
        Buy => "buy",
        Sell => "sell",
        CostEdit => "cost_edit",
        CashAdjust => "cash_adjust",
        TransferIn => "transfer_in",
        TransferOut => "transfer_out",
        AddWatch => "add_watch",
        DeleteWatch => "delete_watch",
        EditHolding => "edit_holding",
        Unknown => "unknown",
    }
}

impl Default for TradeAction {
    fn default() -> Self { Self::Unknown }
}

impl std::str::FromStr for TradeAction {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "buy" => Self::Buy,
            "sell" => Self::Sell,
            "cost_edit" => Self::CostEdit,
            "cash_adjust" => Self::CashAdjust,
            "transfer_in" => Self::TransferIn,
            "transfer_out" => Self::TransferOut,
            "add_watch" => Self::AddWatch,
            "delete_watch" => Self::DeleteWatch,
            "edit_holding" => Self::EditHolding,
            _ => {
                log::warn!("TradeAction::from_str: unrecognized action '{s}', falling back to Unknown");
                Self::Unknown
            }
        })
    }
}

// Holding kind — "hold" (position with shares), "watch" (tracked without shares),
// or "cash" (cash transfer / adjustment, no holding created).
sql_string_enum! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum HoldingKind {
        Hold => "hold",
        Watch => "watch",
        Cash => "cash",
    }
}

impl Default for HoldingKind {
    fn default() -> Self { Self::Hold }
}

impl std::str::FromStr for HoldingKind {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "hold" => Self::Hold,
            "watch" => Self::Watch,
            "cash" => Self::Cash,
            _ => {
                log::warn!("HoldingKind::from_str: unrecognized kind '{s}', falling back to Hold");
                Self::Hold
            }
        })
    }
}

/// 根据 symbol 推导 asset_type：优先使用 trade 提供的值，兜底从 symbol 前缀推导。
fn resolve_asset_type(t: &Trade) -> String {
    t.asset_type
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| if is_etf_symbol(&t.symbol) { "etf" } else { "stock" }.to_string())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holding {
    pub symbol: String,
    pub currency: String,
    pub kind: HoldingKind,
    pub name: Option<String>,
    pub notional: f64,
    pub avg_cost: Option<f64>,
    pub shares: Option<f64>,
    pub entry_date: Option<String>,
    pub linked_verdict_id: Option<String>,
    pub notes: Option<String>,
    pub asset_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Holding {
    /// Recompute notional from cost basis: avg_cost * shares.
    /// Used after buy/sell/cost_edit to keep the invariant consistent.
    pub fn recompute_notional(&mut self) {
        self.notional = self.avg_cost.unwrap_or(0.0) * self.shares.unwrap_or(0.0);
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub id: String,
    pub symbol: String,
    pub currency: String,
    pub kind: HoldingKind,
    pub action: TradeAction,
    pub shares: Option<f64>,
    pub price: Option<f64>,
    pub amount: Option<f64>,
    pub notes: Option<String>,
    pub created_at: String,
    /// Stock/ETF Chinese name — persisted so sold positions keep their name.
    pub name: Option<String>,
    /// User-specified trade date (YYYY-MM-DD). Falls back to created_at.
    pub trade_date: Option<String>,
    /// Asset type: "stock" or "etf". Propagated to holdings during recalculation.
    pub asset_type: Option<String>,
}

/// Canonical column list for SELECT queries on the trades table.
const TRADE_COLUMNS: &str = "id, symbol, currency, kind, action, shares, price, amount, notes, created_at, name, trade_date, asset_type";

/// Map a DB row to a Trade struct. Used by all SELECT queries.
fn trade_from_row(row: &rusqlite::Row) -> rusqlite::Result<Trade> {
    Ok(Trade {
        id: row.get(0)?,
        symbol: row.get(1)?,
        currency: row.get(2)?,
        kind: row.get::<_, HoldingKind>(3)?,
        action: row.get::<_, TradeAction>(4)?,
        shares: row.get(5)?,
        price: row.get(6)?,
        amount: row.get(7)?,
        notes: row.get(8)?,
        created_at: row.get(9)?,
        name: row.get(10)?,
        trade_date: row.get(11)?,
        asset_type: row.get(12)?,
    })
}

pub fn list_holdings() -> Result<Vec<Holding>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare("SELECT symbol, currency, kind, name, notional, avg_cost, shares, entry_date, linked_verdict_id, notes, asset_type, created_at, updated_at FROM holdings ORDER BY symbol")
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Holding {
                    symbol: row.get(0)?,
                    currency: row.get(1)?,
                    kind: row.get(2)?,
                    name: row.get(3)?,
                    notional: row.get(4)?,
                    avg_cost: row.get(5)?,
                    shares: row.get(6)?,
                    entry_date: row.get(7)?,
                    linked_verdict_id: row.get(8)?,
                    notes: row.get(9)?,
                    asset_type: row.get(10)?,
                    created_at: row.get(11)?,
                    updated_at: row.get(12)?,
                })
            })
            .map_err(|e| format!("query: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })
}

pub fn upsert_holding(h: &Holding) -> Result<(), String> {
    with_conn(|conn| {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let created = if h.created_at.is_empty() { &now } else { &h.created_at };
        conn.execute(
            "INSERT INTO holdings (symbol, currency, kind, name, notional, avg_cost, shares, entry_date, linked_verdict_id, notes, asset_type, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(symbol, currency, kind) DO UPDATE SET
               name=?4, notional=?5, avg_cost=?6, shares=?7, entry_date=?8, linked_verdict_id=?9, notes=?10, asset_type=?11, updated_at=?13",
            params![h.symbol, h.currency, h.kind, h.name, h.notional, h.avg_cost, h.shares, h.entry_date, h.linked_verdict_id, h.notes, h.asset_type, created, now],
        )
        .map_err(|e| format!("upsert holding: {}", e))?;
        Ok(())
    })
}

/// Update only the notional (market value) of a holding.
/// This is an explicit exception to the "single entry point" principle —
/// used by the committee orchestrator to refresh market prices without
/// creating trade log entries. Does NOT modify avg_cost, shares, or other fields.
pub fn update_holding_notional(symbol: &str, currency: &str, kind: &HoldingKind, notional: f64) -> Result<(), String> {
    with_conn(|conn| {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let changed = conn.execute(
            "UPDATE holdings SET notional = ?1, updated_at = ?2 WHERE symbol = ?3 AND currency = ?4 AND kind = ?5",
            params![notional, now, symbol, currency, kind],
        )
        .map_err(|e| format!("update notional: {}", e))?;
        if changed == 0 {
            log::debug!("update_holding_notional: no holding found for {symbol}/{currency}/{kind}");
        }
        Ok(())
    })
}

pub fn delete_holding(symbol: &str, currency: &str, kind: &HoldingKind) -> Result<(), String> {
    with_conn(|conn| {
        let changed = conn
            .execute(
                "DELETE FROM holdings WHERE symbol=?1 AND currency=?2 AND kind=?3",
                params![symbol, currency, kind],
            )
            .map_err(|e| format!("delete holding: {}", e))?;
        if changed == 0 {
            Err("Holding not found".to_string())
        } else {
            Ok(())
        }
    })
}

/// Insert a single trade row and apply its cash delta.
/// Must be called within an active connection/transaction.
fn insert_trade_sql(conn: &Connection, t: &Trade) -> Result<(), String> {
    conn.execute(
        "INSERT INTO trades (id, symbol, currency, kind, action, shares, price, amount, notes, created_at, name, trade_date, asset_type) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![t.id, t.symbol, t.currency, t.kind, t.action, t.shares, t.price, t.amount, t.notes, t.created_at, t.name, t.trade_date, t.asset_type],
    )
    .map_err(|e| format!("insert trade: {}", e))?;
    apply_cash_delta_sql(conn, cash_delta_for_trade(t, false))?;
    Ok(())
}

pub fn record_trade(t: &Trade) -> Result<(), String> {
    with_conn(|conn| {
        insert_trade_sql(conn, t)?;
        // Recalculate holdings from trade history.
        // Using delta-based cash above avoids corrupting cash when
        // initial_balance doesn't match actual starting capital.
        recalculate_holdings_inner(conn, false)?;
        Ok(())
    })
}

/// Compute the cash delta for a single trade.
/// `reverse=true` negates the effect (for undoing a trade on delete/update).
fn cash_delta_for_trade(t: &Trade, reverse: bool) -> f64 {
    let amount = t.amount.unwrap_or(0.0);
    let sign = if reverse { -1.0 } else { 1.0 };
    match t.action {
        TradeAction::Buy => -amount * sign,
        TradeAction::Sell => amount * sign,
        TradeAction::CashAdjust => amount * sign,
        TradeAction::TransferIn => amount * sign,
        TradeAction::TransferOut => -amount * sign,
        _ => 0.0,
    }
}

/// Atomic watch→hold conversion: delete_watch + buy in a single transaction.
/// This replaces the previous two-step IPC pattern which had an atomicity defect
/// (first trade persisted, second fails → data loss).
pub fn convert_watch_to_hold(
    symbol: &str,
    currency: &str,
    name: Option<String>,
    shares: f64,
    price: f64,
    asset_type: Option<String>,
) -> Result<(), String> {
    with_conn_mut(|conn| {
        let tx = conn.transaction().map_err(|e| format!("begin transaction: {}", e))?;
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let delete_trade = Trade {
            id: uuid::Uuid::new_v4().to_string(),
            symbol: symbol.to_string(),
            currency: currency.to_string(),
            kind: HoldingKind::Watch,
            action: TradeAction::DeleteWatch,
            shares: None,
            price: None,
            amount: Some(0.0),
            notes: None,
            created_at: now.clone(),
            name: None,
            trade_date: None,
            asset_type: None,
        };
        let buy_trade = Trade {
            id: uuid::Uuid::new_v4().to_string(),
            symbol: symbol.to_string(),
            currency: currency.to_string(),
            kind: HoldingKind::Hold,
            action: TradeAction::Buy,
            shares: Some(shares),
            price: Some(price),
            amount: Some(shares * price),
            notes: None,
            created_at: now,
            name,
            trade_date: None,
            asset_type: asset_type.or_else(|| Some("stock".to_string())),
        };

        insert_trade_sql(&tx, &delete_trade)?;
        insert_trade_sql(&tx, &buy_trade)?;
        recalculate_holdings_inner_no_tx(&tx, false)?;
        tx.commit().map_err(|e| format!("commit transaction: {}", e))?;
        Ok(())
    })
}

/// Apply a cash delta atomically via a single UPDATE (no read-modify-write).
fn apply_cash_delta_sql(conn: &Connection, delta: f64) -> Result<(), String> {
    if delta == 0.0 {
        return Ok(());
    }
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    conn.execute(
        "UPDATE cash SET available = available + ?1, updated_at = ?2 WHERE id = 1",
        params![delta, now],
    )
    .map_err(|e| format!("update cash delta: {}", e))?;
    Ok(())
}

/// Recalculate cash balance from initial_balance + trade history.
/// Must be called within an active connection/transaction.
/// buy, sell, cash_adjust, transfer_in, and transfer_out trades affect the derived cash balance.
fn recalculate_cash_inner(conn: &Connection, trades: &[Trade]) -> Result<(), String> {
    let initial = get_initial_cash_inner(conn).unwrap_or(0.0);
    let mut cash = initial;
    for t in trades {
        cash += cash_delta_for_trade(t, false);
    }
    set_cash_inner(conn, cash)
}

/// Look up a single trade by ID. Returns error if not found.
fn get_trade_by_id(conn: &Connection, id: &str) -> Result<Trade, String> {
    let mut stmt = conn
        .prepare(&format!("SELECT {TRADE_COLUMNS} FROM trades WHERE id = ?1"))
        .map_err(|e| format!("prepare: {}", e))?;
    let mut rows = stmt
        .query_map(params![id], trade_from_row)
        .map_err(|e| format!("query: {}", e))?;
    match rows.next() {
        Some(row) => row.map_err(|e| format!("row: {}", e)),
        None => Err("Trade not found".to_string()),
    }
}

/// Replay all trades from scratch to rebuild the holdings table.
/// When `recalc_cash=true`, also recomputes cash from initial_balance + trades.
/// When `recalc_cash=false`, callers handle cash separately (delta-based).
/// Wrapped in a transaction for atomicity; notional values are preserved
/// from existing holdings so that recalculation does not corrupt user-edited data.
fn recalculate_holdings_inner(conn: &Connection, recalc_cash: bool) -> Result<(), String> {
    recalculate_holdings_inner_impl(conn, recalc_cash, true)
}

/// Same as `recalculate_holdings_inner` but skips BEGIN/COMMIT when the caller
/// already manages the transaction (e.g. `convert_watch_to_hold`).
fn recalculate_holdings_inner_no_tx(conn: &Connection, recalc_cash: bool) -> Result<(), String> {
    recalculate_holdings_inner_impl(conn, recalc_cash, false)
}

fn recalculate_holdings_inner_impl(conn: &Connection, recalc_cash: bool, manage_tx: bool) -> Result<(), String> {
    let mut notional_map: HashMap<(String, String, String), f64> = HashMap::new();
    let mut asset_type_map: HashMap<(String, String, String), String> = HashMap::new();
    let mut created_at_map: HashMap<(String, String, String), String> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT symbol, currency, kind, notional, asset_type, created_at FROM holdings")
            .map_err(|e| format!("prepare notional query: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| format!("query notional: {}", e))?;
        for row in rows {
            let (sym, cur, kind, notional, at, ca) = row.map_err(|e| format!("notional row: {}", e))?;
            notional_map.insert((sym.clone(), cur.clone(), kind.clone()), notional);
            asset_type_map.insert((sym.clone(), cur.clone(), kind.clone()), at);
            created_at_map.insert((sym, cur, kind), ca);
        }
    }

    if manage_tx {
        conn.execute_batch("BEGIN").map_err(|e| format!("begin transaction: {}", e))?;
    }
    let result = recalculate_holdings_inner_body(conn, &notional_map, &asset_type_map, &created_at_map, recalc_cash);
    if manage_tx {
        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT").map_err(|e| format!("commit transaction: {}", e))?;
                Ok(())
            }
            Err(e) => {
                if let Err(rb_err) = conn.execute_batch("ROLLBACK") {
                    log::warn!("rollback failed: {rb_err}");
                }
                Err(e)
            }
        }
    } else {
        result
    }
}

/// Check if P&L should expire based on date gap.
/// P&L expires if there's >= 2 calendar days between cleared_date and buy_date,
/// which approximates "at least 1 trading day without holding the symbol".
fn is_pnl_expired(cleared_date: &str, buy_date: &str) -> bool {
    use chrono::NaiveDate;
    if let (Ok(d1), Ok(d2)) = (
        NaiveDate::parse_from_str(cleared_date, "%Y-%m-%d"),
        NaiveDate::parse_from_str(buy_date, "%Y-%m-%d"),
    ) {
        let days_gap = (d2 - d1).num_days();
        // Gap >= 2 calendar days means at least 1 full day without position
        // (e.g., sold Monday, buy Wednesday = 2 day gap, P&L expires)
        days_gap >= 2
    } else {
        // If dates can't be parsed, conservatively expire P&L
        true
    }
}

/// Realized P&L tracker per symbol for day-trading (做T) cost adjustment.
/// P&L is amortized into new cost basis unless there's a gap of >= 1 trading day
/// without holding the symbol (i.e., cleared and not re-bought within the next trading day).
struct PnlTracker {
    realized_pnl: f64,
    cleared_date: Option<String>, // Date when position was fully closed
}

/// In-memory holding entry used during trade replay to rebuild holdings.
#[derive(Default)]
struct MemHolding {
    name: Option<String>,
    shares: f64,
    avg_cost: f64,
    notional: f64, // preserved from existing holdings (Finding 2 fix)
    entry_date: Option<String>,
    linked_verdict_id: Option<String>,
    notes: Option<String>,
    asset_type: Option<String>,
    is_watch: bool, // true for add_watch entries (no shares, preserved across recalc)
}

impl MemHolding {
    fn recompute_notional(&mut self) {
        self.notional = self.avg_cost * self.shares;
    }

    /// Copy core identity/valuation fields from another entry.
    /// Used by hold↔watch conversions to avoid repeating field lists.
    fn copy_core_fields_from(&mut self, src: &MemHolding) {
        self.name = src.name.clone();
        self.avg_cost = src.avg_cost;
        self.notional = src.notional;
        self.entry_date = src.entry_date.clone();
        self.asset_type = src.asset_type.clone();
    }
}

/// Shared mutable state passed to process_* functions during trade replay.
struct RecalcContext<'a> {
    map: &'a mut HashMap<(String, String, String), MemHolding>,
    pnl_tracker: &'a mut HashMap<String, PnlTracker>,
    watch_deleted: &'a mut HashSet<String>,
}

// ── process_* functions for each TradeAction ────────────────────────────────

fn process_buy(ctx: &mut RecalcContext, key: (String, String, String), t: &Trade) {
    let shares = t.shares.unwrap_or(0.0);
    let price = t.price.unwrap_or(0.0);
    let entry = ctx.map.entry(key).or_default();
    if entry.asset_type.is_none() {
        entry.asset_type = Some(resolve_asset_type(t));
    }
    // Preserve name from trade (set on first buy)
    if entry.name.is_none() {
        entry.name = t.name.clone();
    }

    // Day-trading (做T) cost adjustment:
    // P&L from previous sells is amortized into new cost basis,
    // UNLESS there was a gap of >= 1 trading day without holding
    // the symbol (cleared and not re-bought within next trading day).
    let realized_pnl = if let Some(tracker) = ctx.pnl_tracker.remove(&t.symbol) {
        // Check if P&L has expired (cleared date gap >= 2 calendar days ≈ 1 trading day)
        if let Some(ref cleared_date) = tracker.cleared_date {
            let buy_date = &t.created_at[..10];
            if is_pnl_expired(cleared_date, buy_date) {
                // P&L expired: trading day gap means no amortization
                0.0
            } else {
                tracker.realized_pnl
            }
        } else {
            // No clear date recorded (partial sell), use P&L
            tracker.realized_pnl
        }
    } else {
        0.0
    };

    // Unified cost basis calculation (works for both new and existing positions)
    let existing_cost_basis = entry.avg_cost * entry.shares;
    let buy_cost = price * shares;
    let new_shares = entry.shares + shares;
    let adjusted_cost_basis = existing_cost_basis + buy_cost - realized_pnl;
    entry.avg_cost = if new_shares > 0.0 {
        (adjusted_cost_basis / new_shares).max(0.0)
    } else {
        0.0
    };
    entry.shares = new_shares;
    // Compute notional as cost basis (avg_cost * shares).
    // Will be updated to current market value in PortfolioData::load.
    entry.recompute_notional();
}

fn process_sell(ctx: &mut RecalcContext, key: (String, String, String), t: &Trade) {
    let shares = t.shares.unwrap_or(0.0);
    let sell_price = t.price.unwrap_or(0.0);
    let should_convert = if let Some(entry) = ctx.map.get_mut(&key) {
        // Calculate realized P&L for this sell trade
        // realized_pnl = shares_sold * (sell_price - avg_cost)
        if entry.shares > 0.0 && entry.avg_cost > 0.0 {
            let pnl = shares * (sell_price - entry.avg_cost);
            let tracker = ctx.pnl_tracker.entry(t.symbol.clone()).or_insert(PnlTracker {
                realized_pnl: 0.0,
                cleared_date: None,
            });
            tracker.realized_pnl += pnl;
        }
        entry.shares = (entry.shares - shares).max(0.0);
        entry.recompute_notional();
        // Auto-convert to watch when all shares are sold,
        // but skip if the user explicitly deleted this symbol from watch
        let is_cleared = entry.shares <= 0.0001 && !entry.is_watch && !ctx.watch_deleted.contains(&t.symbol);
        // Record cleared date for P&L expiry tracking
        if is_cleared {
            let trade_date = &t.created_at[..10];
            let tracker = ctx.pnl_tracker.entry(t.symbol.clone()).or_insert(PnlTracker {
                realized_pnl: 0.0,
                cleared_date: None,
            });
            tracker.cleared_date = Some(trade_date.to_string());
        }
        is_cleared
    } else {
        false
    };
    if should_convert {
        let watch_key = (t.symbol.clone(), t.currency.clone(), HoldingKind::Watch.to_string());
        if let Some(hold_entry) = ctx.map.remove(&key) {
            let watch_entry = ctx.map.entry(watch_key).or_default();
            watch_entry.copy_core_fields_from(&hold_entry);
            watch_entry.is_watch = true;
        }
    }
}

fn process_cost_edit(ctx: &mut RecalcContext, key: (String, String, String), t: &Trade) {
    if let Some(price) = t.price {
        if let Some(entry) = ctx.map.get_mut(&key) {
            entry.avg_cost = price;
            entry.recompute_notional();
        }
    }
}

fn process_edit_holding(ctx: &mut RecalcContext, key: (String, String, String), t: &Trade) {
    // Metadata edit: update cost, shares, entry date, notes, name, asset_type.
    // price/shares here are target values, not trade execution values.
    // amount must be null; cash_delta_for_trade returns 0 for this action.
    if let Some(entry) = ctx.map.get_mut(&key) {
        let cost_changed = t.price.is_some_and(|p| (p - entry.avg_cost).abs() > f64::EPSILON);
        if let Some(p) = t.price { entry.avg_cost = p; }
        if let Some(s) = t.shares { entry.shares = s; }
        if let Some(ref d) = t.trade_date { entry.entry_date = Some(d.clone()); }
        if let Some(ref n) = t.notes { entry.notes = Some(n.clone()); }
        if t.name.is_some() { entry.name = t.name.clone(); }
        if t.asset_type.is_some() { entry.asset_type = t.asset_type.clone(); }
        entry.recompute_notional();
        // Only clear P&L tracker when cost basis actually changed.
        // Metadata-only edits (notes, entry_date) should preserve day-trading P&L.
        if cost_changed {
            ctx.pnl_tracker.remove(&t.symbol);
        }
    }
}

fn process_add_watch(ctx: &mut RecalcContext, key: (String, String, String), t: &Trade) {
    let entry = ctx.map.entry(key).or_default();
    // Create or preserve the watch entry in the map so it survives recalculation
    entry.is_watch = true;
    if entry.asset_type.is_none() {
        entry.asset_type = Some(resolve_asset_type(t));
    }
    // Preserve name from trade (preferred) or existing entry
    if entry.name.is_none() {
        entry.name = t.name.clone().or_else(|| Some(t.symbol.clone()));
    }
    if let Some(p) = t.price {
        entry.avg_cost = p;
    }
    entry.entry_date.get_or_insert_with(|| t.created_at[..10].to_string());
}

fn process_delete_watch(ctx: &mut RecalcContext, t: &Trade) {
    // Record the symbol so sell→watch auto-convert won't resurrect it
    ctx.watch_deleted.insert(t.symbol.clone());
    let key = (t.symbol.clone(), t.currency.clone(), t.kind.to_string());
    ctx.map.remove(&key);
}

/// Inner body of recalculate_holdings, separated so the transaction wrapper
/// can handle commit/rollback around it.
fn recalculate_holdings_inner_body(
    conn: &Connection,
    notional_map: &HashMap<(String, String, String), f64>,
    asset_type_map: &HashMap<(String, String, String), String>,
    created_at_map: &HashMap<(String, String, String), String>,
    recalc_cash: bool,
) -> Result<(), String> {
    // 1. Clear all existing holdings
    conn.execute("DELETE FROM holdings", [])
        .map_err(|e| format!("clear holdings: {}", e))?;

    // 2. Load all trades in chronological order
    let sql = format!("SELECT {TRADE_COLUMNS} FROM trades ORDER BY created_at ASC");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("prepare trades for recalc: {}", e))?;

    let trades: Vec<Trade> = stmt
        .query_map([], trade_from_row)
        .map_err(|e| format!("query trades for recalc: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("collect trades for recalc: {}", e))?;

    // 3. Replay trades into an in-memory holdings map
    //    Key: (symbol, currency, kind)
    let mut map: HashMap<(String, String, String), MemHolding> = HashMap::new();
    // Track symbols explicitly deleted from watchlist so that the sell→watch
    // auto-convert does not "resurrect" entries the user already removed.
    let mut watch_deleted: HashSet<String> = HashSet::new();
    let mut pnl_tracker_map: HashMap<String, PnlTracker> = HashMap::new();

    // Restore preserved notional and asset_type values into the in-memory map
    for ((sym, cur, kind), notional) in notional_map {
        let entry = map.entry((sym.clone(), cur.clone(), kind.clone())).or_default();
        entry.notional = *notional;
    }
    for ((sym, cur, kind), at) in asset_type_map {
        let entry = map.entry((sym.clone(), cur.clone(), kind.clone())).or_default();
        entry.asset_type = Some(at.clone());
    }

    let mut ctx = RecalcContext {
        map: &mut map,
        pnl_tracker: &mut pnl_tracker_map,
        watch_deleted: &mut watch_deleted,
    };

    for t in &trades {
        let key = (t.symbol.clone(), t.currency.clone(), t.kind.to_string());
        match t.action {
            TradeAction::Buy => process_buy(&mut ctx, key, t),
            TradeAction::Sell => process_sell(&mut ctx, key, t),
            TradeAction::CostEdit => process_cost_edit(&mut ctx, key, t),
            TradeAction::EditHolding => process_edit_holding(&mut ctx, key, t),
            TradeAction::AddWatch => process_add_watch(&mut ctx, key, t),
            TradeAction::DeleteWatch => process_delete_watch(&mut ctx, t),
            TradeAction::CashAdjust | TradeAction::TransferIn | TradeAction::TransferOut => {}
            TradeAction::Unknown => {
                log::warn!("replay: skipping trade {} with Unknown action (symbol={})", t.id, t.symbol);
            }
        }
    }

    // 4. Recalculate cash from initial_balance + trade history (if requested).
    //    When skip_cash=true, the caller handles cash separately (delta-based).
    if recalc_cash {
        recalculate_cash_inner(conn, &trades)?;
    }

    // 5. Write rebuilt holdings to database
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    for ((symbol, currency, kind), h) in &map {
        // Skip zero-share holdings (but preserve watch items which have no shares)
        if h.shares <= 0.0001 && !h.is_watch {
            continue;
        }
        // Watch items have no shares; write null to DB
        let shares_val: Option<f64> = if h.is_watch { None } else { Some(h.shares) };
        // Preserve original created_at if available, otherwise use now
        let created_at = created_at_map
            .get(&(symbol.clone(), currency.clone(), kind.clone()))
            .cloned()
            .unwrap_or_else(|| now.clone());
        conn.execute(
            "INSERT INTO holdings (symbol, currency, kind, name, notional, avg_cost, shares, entry_date, linked_verdict_id, notes, asset_type, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                symbol,
                currency,
                kind,
                h.name,
                h.notional,
                h.avg_cost,
                shares_val,
                h.entry_date,
                h.linked_verdict_id,
                h.notes,
                h.asset_type,
                created_at,
                now,
            ],
        )
        .map_err(|e| format!("insert rebuilt holding: {}", e))?;
    }

    Ok(())
}

/// Replay all trades from scratch to rebuild the holdings table.
pub fn recalculate_holdings() -> Result<(), String> {
    with_conn(|conn| recalculate_holdings_inner(conn, true))
}

pub fn delete_trade(id: &str) -> Result<(), String> {
    with_conn(|conn| {
        let old_trade = get_trade_by_id(conn, id)?;
        apply_cash_delta_sql(conn, cash_delta_for_trade(&old_trade, true))?;
        conn.execute("DELETE FROM trades WHERE id = ?1", params![id])
            .map_err(|e| format!("delete trade: {}", e))?;
        recalculate_holdings_inner(conn, false)?;
        Ok(())
    })
}

pub fn update_trade(t: &Trade) -> Result<(), String> {
    with_conn(|conn| {
        let old_trade = get_trade_by_id(conn, &t.id)?;
        let old_delta = cash_delta_for_trade(&old_trade, true);
        let new_delta = cash_delta_for_trade(t, false);
        let changed = conn
            .execute(
                "UPDATE trades SET symbol=?2, currency=?3, kind=?4, action=?5, shares=?6, price=?7, amount=?8, notes=?9, name=?10, trade_date=?11, asset_type=?12 WHERE id=?1",
                params![t.id, t.symbol, t.currency, t.kind, t.action, t.shares, t.price, t.amount, t.notes, t.name, t.trade_date, t.asset_type],
            )
            .map_err(|e| format!("update trade: {}", e))?;
        if changed == 0 {
            return Err("Trade not found".to_string());
        }
        apply_cash_delta_sql(conn, old_delta + new_delta)?;
        recalculate_holdings_inner(conn, false)?;
        Ok(())
    })
}

pub fn list_trades(symbol: Option<&str>, limit: Option<i64>) -> Result<Vec<Trade>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(100);
        let (sql, query_params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match symbol {
            Some(s) => (
                format!("SELECT {TRADE_COLUMNS} FROM trades WHERE symbol = ?1 ORDER BY created_at DESC LIMIT ?2"),
                vec![Box::new(s.to_string()), Box::new(limit_val)],
            ),
            None => (
                format!("SELECT {TRADE_COLUMNS} FROM trades ORDER BY created_at DESC LIMIT ?1"),
                vec![Box::new(limit_val)],
            ),
        };
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(query_params.iter()), trade_from_row)
            .map_err(|e| format!("query: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })
}

/// Get net cash transfer between two dates (exclusive start, inclusive end).
/// Returns sum(transfer_in amounts) - sum(transfer_out amounts).
/// Used by PnL snapshot to exclude transfers from daily P&L calculation.
pub fn get_net_transfer_between(from_date: &str, to_date: &str) -> Result<f64, String> {
    with_conn(|conn| {
        let result = conn.query_row(
            "SELECT COALESCE(SUM(CASE WHEN action = 'transfer_in' THEN COALESCE(amount, 0.0) ELSE -COALESCE(amount, 0.0) END), 0.0)
             FROM trades
             WHERE action IN ('transfer_in', 'transfer_out')
               AND trade_date > ?1
               AND trade_date <= ?2",
            params![from_date, to_date],
            |row| row.get::<_, f64>(0),
        );
        match result {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0.0),
            Err(e) => Err(format!("get_net_transfer_between: {}", e)),
        }
    })
}

pub fn get_cash() -> Result<f64, String> {
    with_conn(|conn| get_cash_inner(conn))
}

fn get_cash_inner(conn: &Connection) -> Result<f64, String> {
    let result = conn.query_row("SELECT available FROM cash WHERE id = 1", [], |row| row.get::<_, f64>(0));
    match result {
        Ok(v) => Ok(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0.0),
        Err(e) => Err(format!("get cash: {}", e)),
    }
}

/// Get the initial cash balance (starting capital). Returns 0 if not set.
pub fn get_initial_cash() -> Result<f64, String> {
    with_conn(|conn| get_initial_cash_inner(conn))
}

fn get_initial_cash_inner(conn: &Connection) -> Result<f64, String> {
    let result = conn.query_row(
        "SELECT COALESCE(initial_balance, 0.0) FROM cash WHERE id = 1",
        [],
        |row| row.get::<_, f64>(0),
    );
    match result {
        Ok(v) => Ok(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0.0),
        Err(e) => Err(format!("get initial cash: {}", e)),
    }
}

pub fn set_cash(amount: f64) -> Result<(), String> {
    with_conn(|conn| set_cash_inner(conn, amount))
}

fn set_cash_inner(conn: &Connection, amount: f64) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    // Only set `available`; leave `initial_balance` as NULL on INSERT so that
    // `set_initial_cash` is the sole writer of that column.  The previous
    // `VALUES (1, ?1, ?1, ?2)` would corrupt `initial_balance` whenever the
    // cash row was re-created (e.g. after `clear_all_invest_data`), causing
    // `recalculate_cash_inner` to compute negative cash from a zero base.
    conn.execute(
        "INSERT INTO cash (id, available, initial_balance, updated_at) VALUES (1, ?1, NULL, ?2) \
         ON CONFLICT(id) DO UPDATE SET available=?1, updated_at=?2",
        params![amount, now],
    )
    .map_err(|e| format!("set cash: {}", e))?;
    Ok(())
}

/// 统计近 N 天内的交易次数
/// symbol 为 Some 时按标的统计，为 None 时统计所有标的
pub fn count_recent_trades(symbol: Option<&str>, days: i64) -> Result<i64, String> {
    with_conn(|conn| {
        let days_arg = format!("-{} days", days);
        let result = if let Some(sym) = symbol {
            conn.query_row(
                "SELECT COUNT(*) FROM trades WHERE symbol = ?1 AND created_at >= datetime('now', ?2)",
                params![sym, days_arg],
                |row| row.get::<_, i64>(0),
            )
        } else {
            conn.query_row(
                "SELECT COUNT(*) FROM trades WHERE created_at >= datetime('now', ?1)",
                params![days_arg],
                |row| row.get::<_, i64>(0),
            )
        };
        match result {
            Ok(v) => Ok(v),
            Err(e) => Err(format!("count recent trades: {}", e)),
        }
    })
}

/// 预计算最大回撤：当前持仓的理论最大浮亏（假设价格跌 20%）
/// 返回值为负数，如 -0.15 表示 15% 的回撤
pub fn max_drawdown_for_symbol(
    _symbol: &str,
    current_price: f64,
    avg_cost: f64,
    shares: f64,
) -> f64 {
    if shares <= 0.0 || avg_cost <= 0.0 || current_price <= 0.0 {
        return 0.0;
    }
    // 假设价格跌 20%，计算该跌幅下的浮亏百分比
    let stress_price = current_price * 0.8;
    let stress_pnl_pct = (stress_price - avg_cost) / avg_cost;
    // 如果当前已经浮亏超过 20%，返回实际浮亏百分比
    let current_pnl_pct = (current_price - avg_cost) / avg_cost;
    if current_pnl_pct < stress_pnl_pct {
        current_pnl_pct
    } else {
        stress_pnl_pct
    }
}

/// Set the initial cash balance (starting capital). Used for return calculation.
pub fn set_initial_cash(amount: f64) -> Result<(), String> {
    with_conn(|conn| {
        let changed = conn
            .execute(
                "UPDATE cash SET initial_balance = ?1 WHERE id = 1",
                params![amount],
            )
            .map_err(|e| format!("set initial cash: {}", e))?;
        if changed == 0 {
            // No cash row yet — create it
            let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            conn.execute(
                "INSERT INTO cash (id, available, initial_balance, updated_at) VALUES (1, 0, ?1, ?2)",
                params![amount, now],
            )
            .map_err(|e| format!("set initial cash (insert): {}", e))?;
        }
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cash_delta_transfer_in() {
        let t = Trade {
            id: "t1".into(),
            symbol: "CASH".into(),
            currency: "CNY".into(),
            kind: HoldingKind::Cash,
            action: TradeAction::TransferIn,
            shares: None,
            price: None,
            amount: Some(10000.0),
            notes: None,
            name: None,
            trade_date: "2026-06-12".into(),
            created_at: "2026-06-12T10:00:00Z".into(),
            asset_type: None,
        };
        assert_eq!(cash_delta_for_trade(&t, false), 10000.0);
        assert_eq!(cash_delta_for_trade(&t, true), -10000.0);
    }

    #[test]
    fn test_cash_delta_transfer_out() {
        let t = Trade {
            id: "t2".into(),
            symbol: "CASH".into(),
            currency: "CNY".into(),
            kind: HoldingKind::Cash,
            action: TradeAction::TransferOut,
            shares: None,
            price: None,
            amount: Some(5000.0),
            notes: None,
            name: None,
            trade_date: "2026-06-12".into(),
            created_at: "2026-06-12T10:00:00Z".into(),
            asset_type: None,
        };
        assert_eq!(cash_delta_for_trade(&t, false), -5000.0);
        assert_eq!(cash_delta_for_trade(&t, true), 5000.0);
    }

    #[test]
    fn test_cash_delta_cash_adjust_negative() {
        let t = Trade {
            id: "t3".into(),
            symbol: "CASH".into(),
            currency: "CNY".into(),
            kind: HoldingKind::Cash,
            action: TradeAction::CashAdjust,
            shares: None,
            price: None,
            amount: Some(-200.0),
            notes: None,
            name: None,
            trade_date: "2026-06-12".into(),
            created_at: "2026-06-12T10:00:00Z".into(),
            asset_type: None,
        };
        assert_eq!(cash_delta_for_trade(&t, false), -200.0);
        assert_eq!(cash_delta_for_trade(&t, true), 200.0);
    }

    #[test]
    fn test_cash_delta_non_cash_actions_are_zero() {
        for action in [TradeAction::AddWatch, TradeAction::DeleteWatch, TradeAction::EditHolding, TradeAction::CostEdit] {
            let t = Trade {
                id: "t".into(),
                symbol: "TEST".into(),
                currency: "CNY".into(),
                kind: HoldingKind::Hold,
                action,
                shares: Some(100.0),
                price: Some(10.0),
                amount: Some(1000.0),
                notes: None,
                name: None,
                trade_date: "2026-06-12".into(),
                created_at: "2026-06-12T10:00:00Z".into(),
                asset_type: None,
            };
            assert_eq!(cash_delta_for_trade(&t, false), 0.0, "expected zero for {:?}", t.action);
        }
    }
}
