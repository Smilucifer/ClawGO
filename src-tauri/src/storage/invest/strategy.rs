use super::with_conn;
use rusqlite::params;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Strategy {
    pub id: String,
    pub name: String,
    /// JSON array of StrategyTarget objects. Stored as TEXT in SQLite;
    /// deserialized from JSON string automatically via custom deserialize.
    #[serde(deserialize_with = "deserialize_json_string_or_array")]
    pub targets: Vec<serde_json::Value>,
    pub max_single_pct: Option<f64>,
    pub min_cash_pct: Option<f64>,
    pub updated_at: String,
}

/// Deserialize a field that may be a JSON string (from SQLite) or an array (from IPC).
fn deserialize_json_string_or_array<'de, D>(deserializer: D) -> Result<Vec<serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    use serde::Deserialize as _;
    let val = serde_json::Value::deserialize(deserializer)?;
    match val {
        serde_json::Value::Array(arr) => Ok(arr),
        serde_json::Value::String(s) => serde_json::from_str(&s).map_err(de::Error::custom),
        _ => Ok(vec![]),
    }
}

/// Parse a JSON string from SQLite into a Vec of Values.
fn parse_targets(s: String) -> Vec<serde_json::Value> {
    serde_json::from_str(&s).unwrap_or_default()
}

pub fn get_strategy(id: &str) -> Result<Option<Strategy>, String> {
    with_conn(|conn| {
        let result = conn.query_row(
            "SELECT id, name, targets, max_single_pct, min_cash_pct, updated_at FROM strategy WHERE id = ?1",
            params![id],
            |row| {
                Ok(Strategy {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    targets: parse_targets(row.get(2)?),
                    max_single_pct: row.get(3)?,
                    min_cash_pct: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        );
        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("get strategy: {}", e)),
        }
    })
}

pub fn list_strategies() -> Result<Vec<Strategy>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare("SELECT id, name, targets, max_single_pct, min_cash_pct, updated_at FROM strategy ORDER BY name")
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Strategy {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    targets: parse_targets(row.get(2)?),
                    max_single_pct: row.get(3)?,
                    min_cash_pct: row.get(4)?,
                    updated_at: row.get(5)?,
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

pub fn save_strategy(s: &Strategy) -> Result<(), String> {
    with_conn(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        let updated = if s.updated_at.is_empty() { &now } else { &s.updated_at };
        let targets_json =
            serde_json::to_string(&s.targets).map_err(|e| format!("serialize targets: {e}"))?;
        conn.execute(
            "INSERT INTO strategy (id, name, targets, max_single_pct, min_cash_pct, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET name=?2, targets=?3, max_single_pct=?4, min_cash_pct=?5, updated_at=?6",
            params![s.id, s.name, targets_json, s.max_single_pct, s.min_cash_pct, updated],
        )
        .map_err(|e| format!("save strategy: {}", e))?;
        Ok(())
    })
}

pub fn delete_strategy(id: &str) -> Result<(), String> {
    with_conn(|conn| {
        let changed = conn
            .execute("DELETE FROM strategy WHERE id = ?1", params![id])
            .map_err(|e| format!("delete strategy: {}", e))?;
        if changed == 0 {
            Err("Strategy not found".to_string())
        } else {
            Ok(())
        }
    })
}
