use crate::storage::invest::{with_conn, with_conn_mut};
use rusqlite::{params, Connection};

const CREATE_TABLE_SQL: &str = "
CREATE TABLE IF NOT EXISTS macro_cache (
    indicator   TEXT PRIMARY KEY,
    value       REAL,
    extra_json  TEXT,
    source      TEXT NOT NULL,
    fetched_at  TEXT NOT NULL DEFAULT (datetime('now'))
);";

/// The 12 canonical macro indicators stored in this table.
pub const ALL_INDICATORS: &[&str] = &[
    "csi300_close",
    "csi300_vol20",
    "northbound_net",
    "margin_balance",
    "shibor_on",
    "cgb_10y",
    "vix",
    "tnx",
    "dxy",
    "gold",
    "oil",
    "usdcny",
];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroCacheEntry {
    pub indicator: String,
    pub value: Option<f64>,
    pub extra_json: Option<String>,
    pub source: String,
    pub fetched_at: String,
}

/// Create table using a local connection (for use during init_db before static DB is set).
pub fn create_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(CREATE_TABLE_SQL)
        .map_err(|e| format!("create macro_cache table: {e}"))
}

/// Create table using the static DB connection (for use after init_db).
pub fn create_table_if_not_exists() -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute_batch(CREATE_TABLE_SQL)
            .map_err(|e| format!("create macro_cache table: {e}"))
    })
}

/// UPSERT a single macro indicator value.
pub fn save_macro_cache(
    indicator: &str,
    value: Option<f64>,
    extra_json: Option<&str>,
    source: &str,
) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO macro_cache (indicator, value, extra_json, source, fetched_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(indicator) DO UPDATE SET
                value = excluded.value,
                extra_json = excluded.extra_json,
                source = excluded.source,
                fetched_at = excluded.fetched_at",
            params![indicator, value, extra_json, source],
        )
        .map_err(|e| format!("upsert macro_cache {indicator}: {e}"))?;
        Ok(())
    })
}

/// Read a single macro indicator by name.
pub fn load_macro_cache(indicator: &str) -> Result<Option<MacroCacheEntry>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT indicator, value, extra_json, source, fetched_at
                 FROM macro_cache WHERE indicator = ?1",
            )
            .map_err(|e| format!("prepare load_macro_cache: {e}"))?;
        let mut rows = stmt
            .query_map([indicator], |row| {
                Ok(MacroCacheEntry {
                    indicator: row.get(0)?,
                    value: row.get(1)?,
                    extra_json: row.get(2)?,
                    source: row.get(3)?,
                    fetched_at: row.get(4)?,
                })
            })
            .map_err(|e| format!("query load_macro_cache: {e}"))?;
        rows.next()
            .transpose()
            .map_err(|e| format!("read load_macro_cache row: {e}"))
    })
}

/// Read all macro indicators.
pub fn load_all_macro_cache() -> Result<Vec<MacroCacheEntry>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT indicator, value, extra_json, source, fetched_at
                 FROM macro_cache ORDER BY indicator",
            )
            .map_err(|e| format!("prepare load_all_macro_cache: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(MacroCacheEntry {
                    indicator: row.get(0)?,
                    value: row.get(1)?,
                    extra_json: row.get(2)?,
                    source: row.get(3)?,
                    fetched_at: row.get(4)?,
                })
            })
            .map_err(|e| format!("query load_all_macro_cache: {e}"))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("read load_all_macro_cache row: {e}"))?);
        }
        Ok(items)
    })
}

/// Check whether a cache entry is older than `max_age_minutes`.
///
/// Compares `fetched_at` (stored as UTC datetime string) against the current time.
/// Returns `true` if the entry is stale or if the timestamp cannot be parsed.
pub fn is_stale(entry: &MacroCacheEntry, max_age_minutes: u32) -> bool {
    use chrono::{NaiveDateTime, Utc};

    let Ok(fetched) = NaiveDateTime::parse_from_str(&entry.fetched_at, "%Y-%m-%d %H:%M:%S") else {
        // Unparseable timestamp => treat as stale
        return true;
    };
    let fetched_utc = fetched.and_utc();
    let now = Utc::now();
    let age = now.signed_duration_since(fetched_utc);
    age.num_minutes() > max_age_minutes as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_stale_parsing() {
        // An entry with a clearly old timestamp should be stale.
        let entry = MacroCacheEntry {
            indicator: "gold".into(),
            value: Some(3200.0),
            extra_json: None,
            source: "tushare".into(),
            fetched_at: "2020-01-01 00:00:00".into(),
        };
        assert!(is_stale(&entry, 60));
    }

    #[test]
    fn test_is_stale_unparseable() {
        let entry = MacroCacheEntry {
            indicator: "gold".into(),
            value: Some(3200.0),
            extra_json: None,
            source: "tushare".into(),
            fetched_at: "not-a-date".into(),
        };
        assert!(is_stale(&entry, 60));
    }
}
