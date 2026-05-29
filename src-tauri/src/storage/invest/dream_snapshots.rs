use crate::storage::invest::{with_conn, with_conn_mut};
use rusqlite::Connection;

const CREATE_TABLE_SQL: &str = "
CREATE TABLE IF NOT EXISTS dream_snapshots (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    dream_type      TEXT NOT NULL,
    trigger_type    TEXT NOT NULL,
    before_json     TEXT NOT NULL,
    after_json      TEXT,
    status          TEXT NOT NULL DEFAULT 'pending',
    summary         TEXT,
    rollback_ready  INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ds_type ON dream_snapshots(dream_type);
CREATE INDEX IF NOT EXISTS idx_ds_status ON dream_snapshots(status);";

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DreamSnapshot {
    pub id: i64,
    pub dream_type: String,
    pub trigger_type: String,
    pub before_json: String,
    pub after_json: Option<String>,
    pub status: String,
    pub summary: Option<String>,
    pub rollback_ready: bool,
    pub created_at: String,
}

/// Create table using a local connection (for use during init_db before static DB is set).
pub fn create_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(CREATE_TABLE_SQL)
        .map_err(|e| format!("create dream_snapshots table: {e}"))
}

/// Create table using the static DB connection (for use after init_db).
pub fn create_table_if_not_exists() -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute_batch(CREATE_TABLE_SQL)
            .map_err(|e| format!("create dream_snapshots table: {e}"))
    })
}

/// Insert a fully-completed snapshot in a single atomic INSERT.
/// Used by save_snapshot where all fields are known upfront.
pub fn insert_complete(
    dream_type: &str,
    trigger_type: &str,
    before_json: &str,
    after_json: &str,
    summary: &str,
) -> Result<i64, String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO dream_snapshots (dream_type, trigger_type, before_json, after_json, status, summary, rollback_ready, created_at)
             VALUES (?1, ?2, ?3, ?4, 'completed', ?5, 1, datetime('now'))",
            rusqlite::params![dream_type, trigger_type, before_json, after_json, summary],
        )
        .map_err(|e| format!("insert complete snapshot: {e}"))?;
        Ok(conn.last_insert_rowid())
    })
}

/// Get a single snapshot by ID.
pub fn get_by_id(id: i64) -> Result<Option<DreamSnapshot>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, dream_type, trigger_type, before_json, after_json, status, summary, rollback_ready, created_at
                 FROM dream_snapshots WHERE id = ?1",
            )
            .map_err(|e| format!("prepare get_by_id: {e}"))?;
        let mut rows = stmt
            .query_map([id], |row| {
                Ok(DreamSnapshot {
                    id: row.get(0)?,
                    dream_type: row.get(1)?,
                    trigger_type: row.get(2)?,
                    before_json: row.get(3)?,
                    after_json: row.get(4)?,
                    status: row.get(5)?,
                    summary: row.get(6)?,
                    rollback_ready: row.get::<_, i32>(7)? != 0,
                    created_at: row.get(8)?,
                })
            })
            .map_err(|e| format!("query get_by_id: {e}"))?;
        rows.next()
            .transpose()
            .map_err(|e| format!("read get_by_id row: {e}"))
    })
}

pub fn list_snapshots(
    dream_type: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<DreamSnapshot>, String> {
    with_conn(|conn| {
        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match dream_type {
            Some(t) => (
                "SELECT id, dream_type, trigger_type, before_json, after_json, status, summary, rollback_ready, created_at
                 FROM dream_snapshots WHERE dream_type = ?1 ORDER BY created_at DESC LIMIT ?2"
                    .into(),
                vec![Box::new(t.to_string()), Box::new(limit.unwrap_or(20))],
            ),
            None => (
                "SELECT id, dream_type, trigger_type, before_json, after_json, status, summary, rollback_ready, created_at
                 FROM dream_snapshots ORDER BY created_at DESC LIMIT ?1"
                    .into(),
                vec![Box::new(limit.unwrap_or(20))],
            ),
        };
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("prepare dream_snapshots query: {e}"))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                Ok(DreamSnapshot {
                    id: row.get(0)?,
                    dream_type: row.get(1)?,
                    trigger_type: row.get(2)?,
                    before_json: row.get(3)?,
                    after_json: row.get(4)?,
                    status: row.get(5)?,
                    summary: row.get(6)?,
                    rollback_ready: row.get::<_, i32>(7)? != 0,
                    created_at: row.get(8)?,
                })
            })
            .map_err(|e| format!("query dream_snapshots: {e}"))?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r.map_err(|e| format!("read dream_snapshot row: {e}"))?);
        }
        Ok(result)
    })
}

pub fn mark_rolled_back(id: i64) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "UPDATE dream_snapshots SET status = 'rolled_back', rollback_ready = 0 WHERE id = ?1",
            [id],
        )
        .map_err(|e| format!("mark rolled back: {e}"))?;
        Ok(())
    })
}
