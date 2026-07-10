use super::with_conn_mut;
use chrono::{Duration, Utc};
use rusqlite::params;

/// Format a cutoff timestamp as `YYYY-MM-DD HH:MM:SS` (UTC, no T/Z).
pub fn cutoff_string(keep_days: i64) -> String {
    let cutoff = Utc::now() - Duration::days(keep_days);
    cutoff.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Delete old events that have not been triggered and are not high severity.
/// Returns the number of rows deleted.
pub fn prune_events_on(conn: &mut rusqlite::Connection, cutoff: &str) -> Result<usize, String> {
    let n = conn
        .execute(
            "DELETE FROM events WHERE created_at < ?1 AND triggered = 0 AND severity != 'high'",
            params![cutoff],
        )
        .map_err(|e| format!("prune events: {}", e))?;
    Ok(n)
}

/// Delete old sentiment items. Returns the number of rows deleted.
pub fn prune_sentiment_on(conn: &mut rusqlite::Connection, cutoff: &str) -> Result<usize, String> {
    let n = conn
        .execute(
            "DELETE FROM sentiment_items WHERE created_at < ?1",
            params![cutoff],
        )
        .map_err(|e| format!("prune sentiment: {}", e))?;
    Ok(n)
}

/// Delete events older than `keep_days` (safe: untriggered, non-high).
pub fn prune_events(keep_days: i64) -> Result<usize, String> {
    let cutoff = cutoff_string(keep_days);
    with_conn_mut(|conn| prune_events_on(conn, &cutoff))
}

/// Delete sentiment items older than `keep_days`.
pub fn prune_sentiment_items(keep_days: i64) -> Result<usize, String> {
    let cutoff = cutoff_string(keep_days);
    with_conn_mut(|conn| prune_sentiment_on(conn, &cutoff))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn prune_events_skips_triggered_and_high_severity() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE events (
                id TEXT PRIMARY KEY, source TEXT, event_type TEXT, title TEXT,
                body TEXT, symbols TEXT, severity TEXT DEFAULT 'info',
                stance TEXT DEFAULT 'neutral', triggered INTEGER DEFAULT 0,
                trigger_verdict_id TEXT, created_at TEXT,
                analyzed INTEGER DEFAULT 0, analyzed_at TEXT, channels TEXT DEFAULT '[]'
            );",
        )
        .unwrap();

        // Old, untriggered, non-high => should be deleted
        conn.execute(
            "INSERT INTO events (id, source, event_type, title, created_at, triggered, severity) VALUES ('e1', 'j10', 'news', 'old normal', '2026-01-01 00:00:00', 0, 'info')",
            [],
        )
        .unwrap();
        // Old, triggered => should NOT be deleted
        conn.execute(
            "INSERT INTO events (id, source, event_type, title, created_at, triggered, severity) VALUES ('e2', 'j10', 'news', 'old triggered', '2026-01-01 00:00:00', 1, 'info')",
            [],
        )
        .unwrap();
        // Old, high severity => should NOT be deleted
        conn.execute(
            "INSERT INTO events (id, source, event_type, title, created_at, triggered, severity) VALUES ('e3', 'j10', 'news', 'old high', '2026-01-01 00:00:00', 0, 'high')",
            [],
        )
        .unwrap();
        // Recent, untriggered, non-high => should NOT be deleted
        conn.execute(
            "INSERT INTO events (id, source, event_type, title, created_at, triggered, severity) VALUES ('e4', 'j10', 'news', 'recent normal', '2099-01-01 00:00:00', 0, 'info')",
            [],
        )
        .unwrap();

        let deleted = prune_events_on(&mut conn, "2026-07-03 00:00:00").unwrap();
        assert_eq!(deleted, 1, "only the old untriggered non-high event should be deleted");

        // Verify remaining rows
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn prune_sentiment_on_deletes_all_old_items() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE sentiment_items (
                id TEXT PRIMARY KEY, provider TEXT, symbol TEXT, title TEXT,
                summary TEXT, url TEXT, published_at TEXT, read_count INTEGER,
                comment_count INTEGER, source_type TEXT DEFAULT 'news',
                sentiment_hint REAL, affected_symbols TEXT, sectors TEXT,
                topics TEXT, stance TEXT DEFAULT 'neutral', severity TEXT DEFAULT 'pending',
                analyzed INTEGER DEFAULT 0, created_at TEXT
            );",
        )
        .unwrap();

        conn.execute(
            "INSERT INTO sentiment_items (id, provider, title, created_at) VALUES ('s1', 'xueqiu', 'old', '2026-01-01 00:00:00')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sentiment_items (id, provider, title, created_at) VALUES ('s2', 'xueqiu', 'recent', '2099-01-01 00:00:00')",
            [],
        )
        .unwrap();

        let deleted = prune_sentiment_on(&mut conn, "2026-07-03 00:00:00").unwrap();
        assert_eq!(deleted, 1);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sentiment_items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn cutoff_string_uses_space_not_t() {
        let c = cutoff_string(7);
        assert!(!c.contains('T'), "cutoff must not contain T separator: {}", c);
        assert!(!c.contains('Z'), "cutoff must not contain Z suffix: {}", c);
        // Should match pattern: YYYY-MM-DD HH:MM:SS
        assert_eq!(c.len(), 19, "cutoff length should be 19 chars: {}", c);
        assert!(c.contains(' '), "cutoff must use space separator: {}", c);
    }
}
