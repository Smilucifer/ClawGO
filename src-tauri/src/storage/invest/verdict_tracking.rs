use super::with_conn;
use super::with_conn_mut;
use rusqlite::params;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackedVerdict {
    pub id: i64,
    pub verdict_id: String,
    pub symbol: String,
    pub verdict_type: String,
    pub verdict_date: String,
    pub status: String,
    pub created_at: String,
    pub stopped_at: Option<String>,
}

const CREATE_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS verdict_tracking (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    verdict_id  TEXT NOT NULL UNIQUE,
    symbol      TEXT NOT NULL,
    verdict_type TEXT NOT NULL,
    verdict_date TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'active',
    created_at  TEXT NOT NULL,
    stopped_at  TEXT
);
CREATE INDEX IF NOT EXISTS idx_vt_status ON verdict_tracking(status);
CREATE INDEX IF NOT EXISTS idx_vt_symbol ON verdict_tracking(symbol);";

/// Create the verdict_tracking table using a provided connection.
pub fn create_table(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(CREATE_TABLE_SQL)
        .map_err(|e| format!("create verdict_tracking table: {}", e))?;
    Ok(())
}

/// Create the verdict_tracking table using the global connection.
pub fn create_table_if_not_exists() -> Result<(), String> {
    with_conn(|conn| {
        conn.execute_batch(CREATE_TABLE_SQL)
            .map_err(|e| format!("create verdict_tracking table: {}", e))?;
        Ok(())
    })
}

/// Register a verdict for automatic daily price tracking.
/// Called when a verdict is archived by the committee.
/// Deduplicates: same (symbol, verdict_date) keeps only one active tracking entry.
pub fn start_tracking(
    verdict_id: &str,
    symbol: &str,
    verdict_type: &str,
    verdict_date: &str,
) -> Result<(), String> {
    with_conn_mut(|conn| {
        // Check if there is already an active tracking for this (symbol, date)
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM verdict_tracking WHERE symbol = ?1 AND verdict_date = ?2 AND status = 'active' LIMIT 1",
                params![symbol, verdict_date],
                |row| row.get(0),
            )
            .ok();

        if existing.is_some() {
            // Already tracking this symbol for this date — skip duplicate
            log::info!(
                "Skipping duplicate tracking: symbol={} date={} verdict_id={}",
                symbol, verdict_date, verdict_id
            );
            return Ok(());
        }

        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "INSERT INTO verdict_tracking (verdict_id, symbol, verdict_type, verdict_date, status, created_at)
             VALUES (?1, ?2, ?3, ?4, 'active', ?5)
             ON CONFLICT(verdict_id) DO NOTHING",
            params![verdict_id, symbol, verdict_type, verdict_date, now],
        )
        .map_err(|e| format!("start tracking: {}", e))?;
        Ok(())
    })
}

/// Stop tracking a verdict. Called when a position is sold or removed from watch.
pub fn stop_tracking(verdict_id: &str) -> Result<(), String> {
    with_conn_mut(|conn| {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "UPDATE verdict_tracking SET status = 'completed', stopped_at = ?1 WHERE verdict_id = ?2 AND status = 'active'",
            params![now, verdict_id],
        )
        .map_err(|e| format!("stop tracking: {}", e))?;
        Ok(())
    })
}

/// Stop tracking all verdicts for a given symbol.
/// Called when a symbol's holdings are fully cleared.
pub fn stop_tracking_by_symbol(symbol: &str) -> Result<usize, String> {
    with_conn_mut(|conn| {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let changed = conn.execute(
            "UPDATE verdict_tracking SET status = 'completed', stopped_at = ?1 WHERE symbol = ?2 AND status = 'active'",
            params![now, symbol],
        )
        .map_err(|e| format!("stop tracking by symbol: {}", e))?;
        Ok(changed)
    })
}

/// List all verdicts currently being tracked (status = 'active').
pub fn list_active_tracking() -> Result<Vec<TrackedVerdict>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, verdict_id, symbol, verdict_type, verdict_date, status, created_at, stopped_at
                 FROM verdict_tracking WHERE status = 'active' ORDER BY verdict_date ASC",
            )
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(TrackedVerdict {
                    id: row.get(0)?,
                    verdict_id: row.get(1)?,
                    symbol: row.get(2)?,
                    verdict_type: row.get(3)?,
                    verdict_date: row.get(4)?,
                    status: row.get(5)?,
                    created_at: row.get(6)?,
                    stopped_at: row.get(7)?,
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

/// List all tracking entries (active + completed), for UI display.
pub fn list_all_tracking(limit: Option<i64>) -> Result<Vec<TrackedVerdict>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(100);
        let mut stmt = conn
            .prepare(
                "SELECT id, verdict_id, symbol, verdict_type, verdict_date, status, created_at, stopped_at
                 FROM verdict_tracking ORDER BY verdict_date DESC LIMIT ?1",
            )
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(params![limit_val], |row| {
                Ok(TrackedVerdict {
                    id: row.get(0)?,
                    verdict_id: row.get(1)?,
                    symbol: row.get(2)?,
                    verdict_type: row.get(3)?,
                    verdict_date: row.get(4)?,
                    status: row.get(5)?,
                    created_at: row.get(6)?,
                    stopped_at: row.get(7)?,
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

/// Check if a symbol has any active tracked verdicts.
pub fn has_active_tracking(symbol: &str) -> Result<bool, String> {
    with_conn(|conn| {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM verdict_tracking WHERE symbol = ?1 AND status = 'active'",
                params![symbol],
                |row| row.get(0),
            )
            .map_err(|e| format!("count active tracking: {}", e))?;
        Ok(count > 0)
    })
}
