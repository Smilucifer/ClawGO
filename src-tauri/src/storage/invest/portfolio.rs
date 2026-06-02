use super::{is_etf_symbol, with_conn};
use rusqlite::{params, Connection};
use std::collections::HashMap;

/// 根据 symbol 推导 asset_type：优先使用 trade 提供的值，兜底从 symbol 前缀推导。
fn resolve_asset_type(t: &Trade) -> String {
    t.asset_type
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| if is_etf_symbol(&t.symbol) { "etf" } else { "stock" }.to_string())
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holding {
    pub symbol: String,
    pub currency: String,
    pub kind: String,
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
    pub kind: String,
    pub action: String,
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
        kind: row.get(3)?,
        action: row.get(4)?,
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
        let now = chrono::Utc::now().to_rfc3339();
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

pub fn delete_holding(symbol: &str, currency: &str, kind: &str) -> Result<(), String> {
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

pub fn record_trade(t: &Trade) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT INTO trades (id, symbol, currency, kind, action, shares, price, amount, notes, created_at, name, trade_date, asset_type) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![t.id, t.symbol, t.currency, t.kind, t.action, t.shares, t.price, t.amount, t.notes, t.created_at, t.name, t.trade_date, t.asset_type],
        )
        .map_err(|e| format!("record trade: {}", e))?;
        // Recalculate holdings after inserting trade (consistent with delete_trade/update_trade)
        recalculate_holdings_inner(conn)?;
        Ok(())
    })
}

/// Replay all trades from scratch to rebuild the holdings table.
/// This is an internal helper that must be called within an active connection.
/// Wrapped in a transaction to ensure atomicity; notional values are preserved
/// from existing holdings so that recalculation does not corrupt user-edited data.
fn recalculate_holdings_inner(conn: &Connection) -> Result<(), String> {
    // Preserve existing notional and asset_type values before DELETE
    // (Finding 2: notional=0 corruption fix; asset_type NOT NULL constraint fix)
    let mut notional_map: HashMap<(String, String, String), f64> = HashMap::new();
    let mut asset_type_map: HashMap<(String, String, String), String> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT symbol, currency, kind, notional, asset_type FROM holdings")
            .map_err(|e| format!("prepare notional query: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| format!("query notional: {}", e))?;
        for row in rows {
            let (sym, cur, kind, notional, at) = row.map_err(|e| format!("notional row: {}", e))?;
            notional_map.insert((sym.clone(), cur.clone(), kind.clone()), notional);
            asset_type_map.insert((sym, cur, kind), at);
        }
    }

    // Wrap DELETE + INSERTs in a transaction for atomicity (Finding 3: no transaction fix)
    conn.execute_batch("BEGIN").map_err(|e| format!("begin transaction: {}", e))?;

    let result = recalculate_holdings_inner_body(conn, &notional_map, &asset_type_map);

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
}

/// Inner body of recalculate_holdings, separated so the transaction wrapper
/// can handle commit/rollback around it.
fn recalculate_holdings_inner_body(
    conn: &Connection,
    notional_map: &HashMap<(String, String, String), f64>,
    asset_type_map: &HashMap<(String, String, String), String>,
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
    }

    let mut map: HashMap<(String, String, String), MemHolding> = HashMap::new();

    // Restore preserved notional and asset_type values into the in-memory map
    for ((sym, cur, kind), notional) in notional_map {
        let entry = map.entry((sym.clone(), cur.clone(), kind.clone())).or_default();
        entry.notional = *notional;
    }
    for ((sym, cur, kind), at) in asset_type_map {
        let entry = map.entry((sym.clone(), cur.clone(), kind.clone())).or_default();
        entry.asset_type = Some(at.clone());
    }

    for t in &trades {
        let key = (t.symbol.clone(), t.currency.clone(), t.kind.clone());
        match t.action.as_str() {
            "buy" => {
                let shares = t.shares.unwrap_or(0.0);
                let price = t.price.unwrap_or(0.0);
                let _amount = t.amount.unwrap_or(shares * price);
                let entry = map.entry(key).or_default();
                if entry.asset_type.is_none() {
                    entry.asset_type = Some(resolve_asset_type(t));
                }
                // Preserve name from trade (set on first buy)
                if entry.name.is_none() {
                    entry.name = t.name.clone();
                }
                if entry.shares > 0.0 && entry.avg_cost > 0.0 {
                    let new_shares = entry.shares + shares;
                    entry.avg_cost =
                        (entry.avg_cost * entry.shares + price * shares) / new_shares;
                    entry.shares = new_shares;
                } else {
                    entry.avg_cost = if shares > 0.0 { price } else { 0.0 };
                    entry.shares = shares;
                }
                // Compute notional as cost basis (avg_cost * shares).
                // Will be updated to current market value in PortfolioData::load.
                entry.recompute_notional();
            }
            "sell" => {
                let shares = t.shares.unwrap_or(0.0);
                if let Some(entry) = map.get_mut(&key) {
                    entry.shares = (entry.shares - shares).max(0.0);
                    entry.recompute_notional();
                }
            }
            "cost_edit" => {
                if let Some(price) = t.price {
                    if let Some(entry) = map.get_mut(&key) {
                        entry.avg_cost = price;
                        entry.recompute_notional();
                    }
                }
            }
            "convert_watch_to_hold" => {
                // Remove from watch, add to hold
                let watch_key = (t.symbol.clone(), t.currency.clone(), "watch".to_string());
                let hold_key = (t.symbol.clone(), t.currency.clone(), "hold".to_string());
                if let Some(watch_entry) = map.remove(&watch_key) {
                    let hold_entry = map.entry(hold_key).or_default();
                    hold_entry.name = watch_entry.name;
                    hold_entry.shares = watch_entry.shares;
                    hold_entry.avg_cost = watch_entry.avg_cost;
                    hold_entry.notional = watch_entry.notional; // preserve notional across convert
                    hold_entry.entry_date = watch_entry.entry_date;
                    hold_entry.notes = Some("converted from watchlist".to_string());
                    hold_entry.asset_type = watch_entry.asset_type;
                }
            }
            "convert_hold_to_watch" => {
                // Remove from hold, add to watch
                let hold_key = (t.symbol.clone(), t.currency.clone(), "hold".to_string());
                let watch_key = (t.symbol.clone(), t.currency.clone(), "watch".to_string());
                if let Some(hold_entry) = map.remove(&hold_key) {
                    let watch_entry = map.entry(watch_key).or_default();
                    watch_entry.name = hold_entry.name;
                    watch_entry.shares = hold_entry.shares;
                    watch_entry.avg_cost = hold_entry.avg_cost;
                    watch_entry.notional = hold_entry.notional; // preserve notional across convert
                    watch_entry.entry_date = hold_entry.entry_date;
                    watch_entry.notes = Some("converted from hold".to_string());
                    watch_entry.asset_type = hold_entry.asset_type;
                    watch_entry.is_watch = true;
                }
            }
            "add_watch" => {
                // Create or preserve the watch entry in the map so it survives recalculation
                let entry = map.entry(key).or_default();
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
                entry.entry_date = entry.entry_date.clone().or_else(|| {
                    Some(t.created_at[..10].to_string())
                });
            }
            "delete_watch" => {
                // Remove the watch entry from the map
                map.remove(&key);
            }
            // cash_adjust is trade-log-only; holdings are managed separately
            _ => {}
        }
    }

    // 4. Write rebuilt holdings to database
    let now = chrono::Utc::now().to_rfc3339();
    for ((symbol, currency, kind), h) in &map {
        // Skip zero-share holdings (but preserve watch items which have no shares)
        if h.shares <= 0.0001 && !h.is_watch {
            continue;
        }
        // Watch items have no shares; write null to DB
        let shares_val: Option<f64> = if h.is_watch { None } else { Some(h.shares) };
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
                now,
                now,
            ],
        )
        .map_err(|e| format!("insert rebuilt holding: {}", e))?;
    }

    Ok(())
}

/// Replay all trades from scratch to rebuild the holdings table.
pub fn recalculate_holdings() -> Result<(), String> {
    with_conn(|conn| recalculate_holdings_inner(conn))
}

pub fn delete_trade(id: &str) -> Result<(), String> {
    with_conn(|conn| {
        let changed = conn
            .execute("DELETE FROM trades WHERE id = ?1", params![id])
            .map_err(|e| format!("delete trade: {}", e))?;
        if changed == 0 {
            Err("Trade not found".to_string())
        } else {
            recalculate_holdings_inner(conn)?;
            Ok(())
        }
    })
}

pub fn update_trade(t: &Trade) -> Result<(), String> {
    with_conn(|conn| {
        let changed = conn
            .execute(
                "UPDATE trades SET symbol=?2, currency=?3, kind=?4, action=?5, shares=?6, price=?7, amount=?8, notes=?9, name=?10, trade_date=?11, asset_type=?12 WHERE id=?1",
                params![t.id, t.symbol, t.currency, t.kind, t.action, t.shares, t.price, t.amount, t.notes, t.name, t.trade_date, t.asset_type],
            )
            .map_err(|e| format!("update trade: {}", e))?;
        if changed == 0 {
            Err("Trade not found".to_string())
        } else {
            recalculate_holdings_inner(conn)?;
            Ok(())
        }
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

pub fn get_cash() -> Result<f64, String> {
    with_conn(|conn| {
        let result = conn
            .query_row("SELECT available FROM cash WHERE id = 1", [], |row| row.get::<_, f64>(0));
        match result {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0.0),
            Err(e) => Err(format!("get cash: {}", e)),
        }
    })
}

/// Get the initial cash balance (starting capital). Returns 0 if not set.
pub fn get_initial_cash() -> Result<f64, String> {
    with_conn(|conn| {
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
    })
}

pub fn set_cash(amount: f64) -> Result<(), String> {
    with_conn(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        // On first insert, also set initial_balance to the starting amount
        conn.execute(
            "INSERT INTO cash (id, available, initial_balance, updated_at) VALUES (1, ?1, ?1, ?2) \
             ON CONFLICT(id) DO UPDATE SET available=?1, updated_at=?2",
            params![amount, now],
        )
        .map_err(|e| format!("set cash: {}", e))?;
        Ok(())
    })
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
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO cash (id, available, initial_balance, updated_at) VALUES (1, 0, ?1, ?2)",
                params![amount, now],
            )
            .map_err(|e| format!("set initial cash (insert): {}", e))?;
        }
        Ok(())
    })
}
