use super::with_conn;
use rusqlite::params;

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
    pub created_at: String,
    pub updated_at: String,
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
}

pub fn list_holdings() -> Result<Vec<Holding>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare("SELECT symbol, currency, kind, name, notional, avg_cost, shares, entry_date, linked_verdict_id, notes, created_at, updated_at FROM holdings ORDER BY symbol")
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
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
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
        conn.execute(
            "INSERT INTO holdings (symbol, currency, kind, name, notional, avg_cost, shares, entry_date, linked_verdict_id, notes, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(symbol, currency, kind) DO UPDATE SET
               name=?4, notional=?5, avg_cost=?6, shares=?7, entry_date=?8, linked_verdict_id=?9, notes=?10, updated_at=?12",
            params![h.symbol, h.currency, h.kind, h.name, h.notional, h.avg_cost, h.shares, h.entry_date, h.linked_verdict_id, h.notes, now, now],
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
            "INSERT INTO trades (id, symbol, currency, kind, action, shares, price, amount, notes, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![t.id, t.symbol, t.currency, t.kind, t.action, t.shares, t.price, t.amount, t.notes, t.created_at],
        )
        .map_err(|e| format!("record trade: {}", e))?;
        Ok(())
    })
}

pub fn list_trades(symbol: Option<&str>, limit: Option<i64>) -> Result<Vec<Trade>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(100);
        let (sql, query_params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match symbol {
            Some(s) => (
                "SELECT id, symbol, currency, kind, action, shares, price, amount, notes, created_at FROM trades WHERE symbol = ?1 ORDER BY created_at DESC LIMIT ?2",
                vec![Box::new(s.to_string()), Box::new(limit_val)],
            ),
            None => (
                "SELECT id, symbol, currency, kind, action, shares, price, amount, notes, created_at FROM trades ORDER BY created_at DESC LIMIT ?1",
                vec![Box::new(limit_val)],
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(query_params.iter()), |row| {
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

pub fn set_cash(amount: f64) -> Result<(), String> {
    with_conn(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO cash (id, available, updated_at) VALUES (1, ?1, ?2) ON CONFLICT(id) DO UPDATE SET available=?1, updated_at=?2",
            params![amount, now],
        )
        .map_err(|e| format!("set cash: {}", e))?;
        Ok(())
    })
}
