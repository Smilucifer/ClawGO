use super::with_conn;
use super::with_conn_mut;
use rusqlite::params;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub id: String,
    pub symbol: String,
    pub verdict: String,
    pub confidence: Option<f64>,
    pub macro_signal: Option<String>,
    pub macro_strength: Option<f64>,
    pub reasoning: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub tokens_used: Option<i64>,
    pub latency_ms: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PnlSnapshot {
    pub id: i64,
    pub snapshot_date: String,
    pub total_value: f64,
    pub cash: f64,
    pub holdings_value: f64,
    pub daily_pnl: Option<f64>,
    pub daily_pnl_pct: Option<f64>,
    pub created_at: String,
}

pub fn save_verdict(v: &Verdict) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO verdicts (id, symbol, verdict, confidence, macro_signal, macro_strength, reasoning, model, provider, tokens_used, latency_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![v.id, v.symbol, v.verdict, v.confidence, v.macro_signal, v.macro_strength, v.reasoning, v.model, v.provider, v.tokens_used, v.latency_ms, v.created_at],
        )
        .map_err(|e| format!("save verdict: {}", e))?;
        Ok(())
    })
}

pub fn list_verdicts(symbol: Option<&str>, limit: Option<i64>) -> Result<Vec<Verdict>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(50);
        let (sql, query_params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match symbol {
            Some(s) => (
                "SELECT id, symbol, verdict, confidence, macro_signal, macro_strength, reasoning, model, provider, tokens_used, latency_ms, created_at FROM verdicts WHERE symbol = ?1 ORDER BY created_at DESC LIMIT ?2",
                vec![Box::new(s.to_string()), Box::new(limit_val)],
            ),
            None => (
                "SELECT id, symbol, verdict, confidence, macro_signal, macro_strength, reasoning, model, provider, tokens_used, latency_ms, created_at FROM verdicts ORDER BY created_at DESC LIMIT ?1",
                vec![Box::new(limit_val)],
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(query_params.iter()), |row| {
                Ok(Verdict {
                    id: row.get(0)?,
                    symbol: row.get(1)?,
                    verdict: row.get(2)?,
                    confidence: row.get(3)?,
                    macro_signal: row.get(4)?,
                    macro_strength: row.get(5)?,
                    reasoning: row.get(6)?,
                    model: row.get(7)?,
                    provider: row.get(8)?,
                    tokens_used: row.get(9)?,
                    latency_ms: row.get(10)?,
                    created_at: row.get(11)?,
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

pub fn save_pnl_snapshot(s: &PnlSnapshot) -> Result<i64, String> {
    with_conn_mut(|conn| {
        // Upsert: update existing snapshot for the same date, or insert new
        let updated = conn.execute(
            "UPDATE pnl_snapshots SET total_value=?1, cash=?2, holdings_value=?3, daily_pnl=?4, daily_pnl_pct=?5 WHERE snapshot_date=?6",
            params![s.total_value, s.cash, s.holdings_value, s.daily_pnl, s.daily_pnl_pct, s.snapshot_date],
        ).map_err(|e| format!("update pnl: {}", e))?;

        if updated == 0 {
            conn.execute(
                "INSERT INTO pnl_snapshots (snapshot_date, total_value, cash, holdings_value, daily_pnl, daily_pnl_pct) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![s.snapshot_date, s.total_value, s.cash, s.holdings_value, s.daily_pnl, s.daily_pnl_pct],
            )
            .map_err(|e| format!("save pnl: {}", e))?;
        }
        Ok(conn.last_insert_rowid())
    })
}

pub fn list_pnl_snapshots(limit: Option<i64>) -> Result<Vec<PnlSnapshot>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(100);
        let mut stmt = conn
            .prepare("SELECT id, snapshot_date, total_value, cash, holdings_value, daily_pnl, daily_pnl_pct, created_at FROM pnl_snapshots ORDER BY snapshot_date DESC LIMIT ?1")
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(params![limit_val], |row| {
                Ok(PnlSnapshot {
                    id: row.get(0)?,
                    snapshot_date: row.get(1)?,
                    total_value: row.get(2)?,
                    cash: row.get(3)?,
                    holdings_value: row.get(4)?,
                    daily_pnl: row.get(5)?,
                    daily_pnl_pct: row.get(6)?,
                    created_at: row.get(7)?,
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
