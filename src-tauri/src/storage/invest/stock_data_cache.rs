use crate::storage::invest::{with_conn, with_conn_mut};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

const CREATE_TABLE_SQL: &str = "
CREATE TABLE IF NOT EXISTS stock_data_cache (
    symbol      TEXT NOT NULL,
    data_type   TEXT NOT NULL,
    data_date   TEXT NOT NULL,
    value_json  TEXT NOT NULL,
    fetched_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (symbol, data_type, data_date)
);

CREATE INDEX IF NOT EXISTS idx_sdc_symbol_type
    ON stock_data_cache(symbol, data_type);
";

/// Canonical data types stored in this table.
pub const DATA_TYPES: &[&str] = &[
    "daily_basic",
    "fina_indicator",
    "report_rc",
    "moneyflow_dc",
    "industry",
];

/// A cached stock data entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockDataCacheEntry {
    pub symbol: String,
    pub data_type: String,
    pub data_date: String,
    pub value_json: String,
    pub fetched_at: String,
}

/// Create table using a local connection (for use during init_db before static DB is set).
pub fn create_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(CREATE_TABLE_SQL)
        .map_err(|e| format!("create stock_data_cache table: {e}"))
}

/// Create table using the static DB connection (for use after init_db).
#[allow(unused)]
pub fn create_table_if_not_exists() -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute_batch(CREATE_TABLE_SQL)
            .map_err(|e| format!("create stock_data_cache table: {e}"))
    })
}

/// UPSERT a single cache entry. Uses INSERT OR REPLACE on the composite primary key.
pub fn upsert_cache(
    symbol: &str,
    data_type: &str,
    data_date: &str,
    value_json: &str,
) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO stock_data_cache (symbol, data_type, data_date, value_json, fetched_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(symbol, data_type, data_date) DO UPDATE SET
                value_json = excluded.value_json,
                fetched_at = excluded.fetched_at",
            params![symbol, data_type, data_date, value_json],
        )
        .map_err(|e| format!("upsert stock_data_cache {symbol}/{data_type}/{data_date}: {e}"))?;
        Ok(())
    })
}

/// Batch UPSERT — all entries written inside a single transaction.
/// Reduces N lock acquisitions + N fsyncs to 1 each.
pub fn batch_upsert(entries: &[(&str, &str, &str, &str)]) -> Result<(), String> {
    if entries.is_empty() {
        return Ok(());
    }
    with_conn_mut(|conn| {
        let tx = conn.transaction().map_err(|e| format!("begin tx: {e}"))?;
        for &(symbol, data_type, data_date, value_json) in entries {
            tx.execute(
                "INSERT INTO stock_data_cache (symbol, data_type, data_date, value_json, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))
                 ON CONFLICT(symbol, data_type, data_date) DO UPDATE SET
                    value_json = excluded.value_json,
                    fetched_at = excluded.fetched_at",
                params![symbol, data_type, data_date, value_json],
            )
            .map_err(|e| format!("batch_upsert {symbol}/{data_type}: {e}"))?;
        }
        tx.commit().map_err(|e| format!("commit tx: {e}"))
    })
}

/// Load the latest cache entry for a (symbol, data_type) pair.
/// Returns `(data_date, value_json, fetched_at)`.
pub fn load_latest(symbol: &str, data_type: &str) -> Result<Option<(String, String, String)>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT data_date, value_json, fetched_at
                 FROM stock_data_cache
                 WHERE symbol = ?1 AND data_type = ?2
                 ORDER BY data_date DESC
                 LIMIT 1",
            )
            .map_err(|e| format!("prepare load_latest: {e}"))?;
        let mut rows = stmt
            .query_map(params![symbol, data_type], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("query load_latest: {e}"))?;
        rows.next()
            .transpose()
            .map_err(|e| format!("read load_latest row: {e}"))
    })
}

/// Load a cache entry for a specific (symbol, data_type, data_date).
pub fn load_on_date(
    symbol: &str,
    data_type: &str,
    data_date: &str,
) -> Result<Option<String>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT value_json FROM stock_data_cache
                 WHERE symbol = ?1 AND data_type = ?2 AND data_date = ?3",
            )
            .map_err(|e| format!("prepare load_on_date: {e}"))?;
        let mut rows = stmt
            .query_map(params![symbol, data_type, data_date], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| format!("query load_on_date: {e}"))?;
        rows.next()
            .transpose()
            .map_err(|e| format!("read load_on_date row: {e}"))
    })
}

/// Load all latest entries for a symbol across all data types.
/// Returns a Vec of (data_type, data_date, value_json).
pub fn load_all_latest_for_symbol(
    symbol: &str,
) -> Result<Vec<(String, String, String)>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT data_type, data_date, value_json
                 FROM stock_data_cache
                 WHERE symbol = ?1
                   AND rowid IN (
                     SELECT MAX(rowid) FROM stock_data_cache
                     WHERE symbol = ?1
                     GROUP BY data_type
                   )
                 ORDER BY data_type",
            )
            .map_err(|e| format!("prepare load_all_latest: {e}"))?;
        let rows = stmt
            .query_map([symbol], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("query load_all_latest: {e}"))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("read load_all_latest row: {e}"))?);
        }
        Ok(items)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_types_defined() {
        assert_eq!(DATA_TYPES.len(), 5);
        assert!(DATA_TYPES.contains(&"daily_basic"));
        assert!(DATA_TYPES.contains(&"fina_indicator"));
        assert!(DATA_TYPES.contains(&"report_rc"));
        assert!(DATA_TYPES.contains(&"moneyflow_dc"));
        assert!(DATA_TYPES.contains(&"industry"));
    }
}
