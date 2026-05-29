use super::with_conn;
use rusqlite::params;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub source: String,
    pub event_type: String,
    pub title: String,
    pub body: Option<String>,
    pub symbols: Option<String>,
    pub severity: String,
    pub stance: String,
    pub triggered: bool,
    pub trigger_verdict_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventSource {
    pub id: String,
    pub name: String,
    pub source_type: String,
    pub config: Option<String>,
    pub enabled: bool,
    pub last_poll_at: Option<String>,
    pub created_at: String,
}

pub fn save_event(e: &Event) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO events (id, source, event_type, title, body, symbols, severity, stance, triggered, trigger_verdict_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![e.id, e.source, e.event_type, e.title, e.body, e.symbols, e.severity, e.stance, e.triggered as i32, e.trigger_verdict_id, e.created_at],
        )
        .map_err(|e| format!("save event: {}", e))?;
        Ok(())
    })
}

pub fn list_events(source: Option<&str>, limit: Option<i64>) -> Result<Vec<Event>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(100);
        let (sql, query_params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match source {
            Some(s) => (
                "SELECT id, source, event_type, title, body, symbols, severity, stance, triggered, trigger_verdict_id, created_at FROM events WHERE source = ?1 ORDER BY created_at DESC LIMIT ?2",
                vec![Box::new(s.to_string()), Box::new(limit_val)],
            ),
            None => (
                "SELECT id, source, event_type, title, body, symbols, severity, stance, triggered, trigger_verdict_id, created_at FROM events ORDER BY created_at DESC LIMIT ?1",
                vec![Box::new(limit_val)],
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(query_params.iter()), |row| {
                Ok(Event {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    event_type: row.get(2)?,
                    title: row.get(3)?,
                    body: row.get(4)?,
                    symbols: row.get(5)?,
                    severity: row.get(6)?,
                    stance: row.get(7)?,
                    triggered: row.get::<_, i32>(8)? != 0,
                    trigger_verdict_id: row.get(9)?,
                    created_at: row.get(10)?,
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

pub fn mark_event_triggered(event_id: &str, verdict_id: &str) -> Result<(), String> {
    with_conn(|conn| {
        let changed = conn
            .execute(
                "UPDATE events SET triggered = 1, trigger_verdict_id = ?1 WHERE id = ?2",
                params![verdict_id, event_id],
            )
            .map_err(|e| format!("mark triggered: {}", e))?;
        if changed == 0 {
            Err("Event not found".to_string())
        } else {
            Ok(())
        }
    })
}

pub fn save_event_source(s: &EventSource) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT INTO event_sources (id, name, source_type, config, enabled, last_poll_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET name=?2, source_type=?3, config=?4, enabled=?5, last_poll_at=?6",
            params![s.id, s.name, s.source_type, s.config, s.enabled as i32, s.last_poll_at, s.created_at],
        )
        .map_err(|e| format!("save source: {}", e))?;
        Ok(())
    })
}

pub fn list_event_sources() -> Result<Vec<EventSource>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare("SELECT id, name, source_type, config, enabled, last_poll_at, created_at FROM event_sources ORDER BY name")
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(EventSource {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    source_type: row.get(2)?,
                    config: row.get(3)?,
                    enabled: row.get::<_, i32>(4)? != 0,
                    last_poll_at: row.get(5)?,
                    created_at: row.get(6)?,
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
