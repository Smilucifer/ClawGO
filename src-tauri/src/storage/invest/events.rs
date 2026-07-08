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
    pub analyzed: bool,
    pub analyzed_at: Option<String>,
    pub channels: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub sectors: Option<String>,
    #[serde(default)]
    pub topics: Option<String>,
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

/// Canonical SELECT column list for the events table.
const EVENT_COLS: &str = "id, source, event_type, title, body, symbols, severity, stance, triggered, trigger_verdict_id, created_at, analyzed, analyzed_at, channels, summary, sectors, topics";

/// Map a SQLite row to an Event struct.
fn row_to_event(row: &rusqlite::Row) -> rusqlite::Result<Event> {
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
        analyzed: row.get::<_, i32>(11)? != 0,
        analyzed_at: row.get(12)?,
        channels: row.get(13)?,
        summary: row.get(14).ok().flatten(),
        sectors: row.get(15).ok().flatten(),
        topics: row.get(16).ok().flatten(),
    })
}

pub fn save_event(e: &Event) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT OR IGNORE INTO events (id, source, event_type, title, body, symbols, severity, stance, triggered, trigger_verdict_id, created_at, analyzed, analyzed_at, channels, summary, sectors, topics)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![e.id, e.source, e.event_type, e.title, e.body, e.symbols, e.severity, e.stance, e.triggered as i32, e.trigger_verdict_id, e.created_at, e.analyzed as i32, e.analyzed_at, e.channels, e.summary, e.sectors, e.topics],
        )
        .map_err(|e| format!("save event: {}", e))?;
        Ok(())
    })
}

pub fn list_events(source: Option<&str>, limit: Option<i64>) -> Result<Vec<Event>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(100);
        let (sql, query_params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match source {
            Some(s) => (
                format!("SELECT {} FROM events WHERE source = ?1 ORDER BY created_at DESC LIMIT ?2", EVENT_COLS),
                vec![Box::new(s.to_string()), Box::new(limit_val)],
            ),
            None => (
                format!("SELECT {} FROM events ORDER BY created_at DESC LIMIT ?1", EVENT_COLS),
                vec![Box::new(limit_val)],
            ),
        };
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(query_params.iter()), row_to_event)
            .map_err(|e| format!("query: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })
}

pub fn mark_event_triggered(event_id: &str, verdict_id: Option<&str>) -> Result<(), String> {
    with_conn(|conn| {
        let changed = conn
            .execute(
                "UPDATE events SET triggered = 1, trigger_verdict_id = ?1 WHERE id = ?2 AND triggered = 0",
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

pub fn get_event_stats() -> Result<(usize, usize, usize, Option<String>), String> {
    with_conn(|conn| {
        let total: usize = conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .map_err(|e| format!("count events: {}", e))?;
        let high: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE severity = 'high'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| format!("count high: {}", e))?;
        let untriggered_high: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE severity = 'high' AND triggered = 0",
                [],
                |row| row.get(0),
            )
            .map_err(|e| format!("count untriggered: {}", e))?;
        let last_at: Option<String> = match conn.query_row(
            "SELECT created_at FROM events ORDER BY created_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        ) {
            Ok(v) => Some(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(format!("last event: {}", e)),
        };
        Ok((total, high, untriggered_high, last_at))
    })
}

/// Query unanalyzed events (analyzed = 0).
pub fn list_unanalyzed_events(limit: Option<i64>) -> Result<Vec<Event>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(100);
        let sql = format!(
            "SELECT {} FROM events WHERE analyzed = 0 ORDER BY created_at DESC LIMIT ?1",
            EVENT_COLS
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(params![limit_val], row_to_event)
            .map_err(|e| format!("query: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })
}

/// Update event analysis results (severity, stance, symbols, summary, sectors, topics, analyzed flag).
pub fn update_event_analysis(
    event_id: &str,
    severity: &str,
    stance: &str,
    symbols: Option<&str>,
    summary: Option<&str>,
    sectors: Option<&str>,
    topics: Option<&str>,
) -> Result<(), String> {
    with_conn(|conn| {
        let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let changed = conn
            .execute(
                "UPDATE events SET severity = ?1, stance = ?2, symbols = ?3, summary = ?4, sectors = ?5, topics = ?6, analyzed = 1, analyzed_at = ?7 WHERE id = ?8 AND analyzed = 0",
                params![severity, stance, symbols, summary, sectors, topics, now, event_id],
            )
            .map_err(|e| format!("update analysis: {}", e))?;
        if changed == 0 {
            Err("Event not found or already analyzed".to_string())
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
