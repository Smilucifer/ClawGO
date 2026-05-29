use super::with_conn;
use super::with_conn_mut;
use rusqlite::params;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictReviewEntry {
    pub id: i64,
    pub verdict_id: String,
    pub symbol: String,
    pub verdict_type: String,
    pub verdict_date: String,
    pub window_days: i64,
    pub price_at_verdict: Option<f64>,
    pub price_after: Option<f64>,
    pub return_pct: Option<f64>,
    pub hit: bool,
    pub flat_threshold: Option<f64>,
    pub created_at: String,
}

pub fn create_table_if_not_exists() -> Result<(), String> {
    with_conn(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS verdict_reviews (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                verdict_id      TEXT NOT NULL,
                symbol          TEXT NOT NULL,
                verdict_type    TEXT NOT NULL,
                verdict_date    TEXT NOT NULL,
                window_days     INTEGER NOT NULL,
                price_at_verdict REAL,
                price_after     REAL,
                return_pct      REAL,
                hit             INTEGER NOT NULL,
                flat_threshold  REAL,
                created_at      TEXT NOT NULL,
                UNIQUE(verdict_id, window_days)
            );
            CREATE INDEX IF NOT EXISTS idx_vr_verdict ON verdict_reviews(verdict_id);
            CREATE INDEX IF NOT EXISTS idx_vr_symbol ON verdict_reviews(symbol);
            CREATE INDEX IF NOT EXISTS idx_vr_date ON verdict_reviews(verdict_date);",
        )
        .map_err(|e| format!("create verdict_reviews table: {}", e))?;
        Ok(())
    })
}

pub fn upsert_review(entry: &VerdictReviewEntry) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO verdict_reviews (verdict_id, symbol, verdict_type, verdict_date, window_days, price_at_verdict, price_after, return_pct, hit, flat_threshold, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(verdict_id, window_days) DO UPDATE SET
                price_at_verdict = excluded.price_at_verdict,
                price_after = excluded.price_after,
                return_pct = excluded.return_pct,
                hit = excluded.hit,
                flat_threshold = excluded.flat_threshold,
                created_at = excluded.created_at",
            params![
                entry.verdict_id,
                entry.symbol,
                entry.verdict_type,
                entry.verdict_date,
                entry.window_days,
                entry.price_at_verdict,
                entry.price_after,
                entry.return_pct,
                entry.hit as i64,
                entry.flat_threshold,
                entry.created_at,
            ],
        )
        .map_err(|e| format!("upsert verdict review: {}", e))?;
        Ok(())
    })
}

pub fn list_reviews(symbol: Option<&str>, limit: Option<i64>) -> Result<Vec<VerdictReviewEntry>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(100);
        let (sql, query_params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match symbol {
            Some(s) => (
                "SELECT id, verdict_id, symbol, verdict_type, verdict_date, window_days, price_at_verdict, price_after, return_pct, hit, flat_threshold, created_at
                 FROM verdict_reviews WHERE symbol = ?1 ORDER BY verdict_date DESC, window_days ASC LIMIT ?2",
                vec![Box::new(s.to_string()), Box::new(limit_val)],
            ),
            None => (
                "SELECT id, verdict_id, symbol, verdict_type, verdict_date, window_days, price_at_verdict, price_after, return_pct, hit, flat_threshold, created_at
                 FROM verdict_reviews ORDER BY verdict_date DESC, window_days ASC LIMIT ?1",
                vec![Box::new(limit_val)],
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(query_params.iter()), |row| {
                Ok(VerdictReviewEntry {
                    id: row.get(0)?,
                    verdict_id: row.get(1)?,
                    symbol: row.get(2)?,
                    verdict_type: row.get(3)?,
                    verdict_date: row.get(4)?,
                    window_days: row.get(5)?,
                    price_at_verdict: row.get(6)?,
                    price_after: row.get(7)?,
                    return_pct: row.get(8)?,
                    hit: row.get::<_, i64>(9)? != 0,
                    flat_threshold: row.get(10)?,
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

pub fn clear_reviews() -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute("DELETE FROM verdict_reviews", [])
            .map_err(|e| format!("clear verdict reviews: {}", e))?;
        Ok(())
    })
}
