use chrono::Local;
use rusqlite::params;

use super::with_conn;
use super::with_conn_mut;
use super::verdicts::Verdict;
use super::verdicts::row_to_verdict;

// ---------------------------------------------------------------------------
// archive_verdict — daily-overwrite archiving
// ---------------------------------------------------------------------------

/// Archive a committee verdict with daily-overwrite semantics.
///
/// For each symbol, only the latest verdict per calendar day is retained.
/// Previous verdicts for the same symbol on the same date are deleted and
/// their tracking entries superseded. This runs inside a single transaction
/// to guarantee atomicity.
///
/// This is the primary archival entry point called by the committee
/// orchestrator after a successful (non-dry-run) pipeline.
pub fn archive_verdict(
    symbol: &str,
    name: Option<&str>,
    verdict: &str,
    confidence: f64,
    macro_signal: Option<&str>,
    macro_strength: Option<f64>,
    reasoning: &str,
    model: &str,
    provider: &str,
    tokens_used: u32,
    latency_ms: u64,
) -> Result<(), String> {
    let now = Local::now();
    let id = format!(
        "{}_{}",
        symbol,
        now.format("%Y%m%d%H%M%S%.3f")
    );
    let created_at = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let date = extract_date(&created_at);
    let verdict_date = now.format("%Y%m%d").to_string();

    with_conn_mut(|conn| {
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("begin tx: {e}"))?;

        // 1. Supersede old tracking entries for this symbol on the same date
        tx.execute(
            "UPDATE verdict_tracking SET status = 'superseded', stopped_at = ?1
             WHERE symbol = ?2 AND verdict_date = ?3 AND status = 'active'",
            params![created_at, symbol, verdict_date],
        )
        .map_err(|e| format!("supersede tracking: {e}"))?;

        // 2. Delete older verdicts for this symbol on the same date
        tx.execute(
            "DELETE FROM verdicts WHERE symbol = ?1 AND date(created_at) = ?2",
            params![symbol, date],
        )
        .map_err(|e| format!("delete old verdicts: {e}"))?;

        // 3. Insert the new verdict
        tx.execute(
            "INSERT INTO verdicts (id, symbol, name, verdict, confidence, macro_signal, macro_strength, reasoning, model, provider, tokens_used, latency_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                id,
                symbol,
                name,
                verdict,
                confidence,
                macro_signal,
                macro_strength,
                reasoning,
                model,
                provider,
                tokens_used as i64,
                latency_ms as i64,
                created_at,
            ],
        )
        .map_err(|e| format!("insert verdict: {e}"))?;

        // 4. Register new tracking entry
        tx.execute(
            "INSERT INTO verdict_tracking (verdict_id, symbol, verdict_type, verdict_date, status, created_at)
             VALUES (?1, ?2, ?3, ?4, 'active', ?5)
             ON CONFLICT(verdict_id) DO NOTHING",
            params![id, symbol, verdict, verdict_date, created_at],
        )
        .map_err(|e| format!("start tracking: {e}"))?;

        tx.commit()
            .map_err(|e| format!("commit: {e}"))?;

        log::info!(
            "archived verdict for {} (date={}, id={})",
            symbol,
            date,
            id,
        );
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Query functions
// ---------------------------------------------------------------------------

/// Get the latest verdict for a symbol (most recent across all dates).
pub fn get_latest_verdict(symbol: &str) -> Result<Option<Verdict>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, symbol, name, verdict, confidence, macro_signal, macro_strength, reasoning, model, provider, tokens_used, latency_ms, created_at
                 FROM verdicts WHERE symbol = ?1 ORDER BY created_at DESC LIMIT 1",
            )
            .map_err(|e| format!("prepare: {e}"))?;

        let mut rows = stmt
            .query_map(params![symbol], row_to_verdict)
            .map_err(|e| format!("query: {e}"))?;

        match rows.next() {
            Some(Ok(v)) => Ok(Some(v)),
            Some(Err(e)) => Err(format!("row: {e}")),
            None => Ok(None),
        }
    })
}

/// List daily-latest verdicts for a given date (`YYYY-MM-DD`).
/// With the daily-overwrite invariant, at most one verdict per symbol exists
/// per date.
pub fn get_daily_verdicts(date: &str) -> Result<Vec<Verdict>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, symbol, name, verdict, confidence, macro_signal, macro_strength, reasoning, model, provider, tokens_used, latency_ms, created_at
                 FROM verdicts WHERE date(created_at) = ?1 ORDER BY symbol ASC",
            )
            .map_err(|e| format!("prepare: {e}"))?;

        let rows = stmt
            .query_map(params![date], row_to_verdict)
            .map_err(|e| format!("query: {e}"))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {e}"))?);
        }
        Ok(items)
    })
}

/// List verdict history for a symbol over the last N days.
/// Returns one verdict per day (the latest for that day), in reverse
/// chronological order.
pub fn list_verdict_history(symbol: &str, days: i64) -> Result<Vec<Verdict>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, symbol, name, verdict, confidence, macro_signal, macro_strength, reasoning, model, provider, tokens_used, latency_ms, created_at
                 FROM verdicts WHERE symbol = ?1 AND created_at >= date('now', ?2)
                 ORDER BY created_at DESC",
            )
            .map_err(|e| format!("prepare: {e}"))?;

        let offset = format!("-{} days", days);
        let rows = stmt
            .query_map(params![symbol, offset], row_to_verdict)
            .map_err(|e| format!("query: {e}"))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {e}"))?);
        }
        Ok(items)
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the date portion (`YYYY-MM-DD`) from a datetime string.
/// Handles both `"YYYY-MM-DD HH:MM:SS"` and `"YYYY-MM-DDTHH:MM:SS"`.
fn extract_date(created_at: &str) -> String {
    if let Some(pos) = created_at.find('T') {
        created_at[..pos].to_string()
    } else if let Some(pos) = created_at.find(' ') {
        created_at[..pos].to_string()
    } else {
        created_at.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_date_standard() {
        assert_eq!(extract_date("2026-05-31 14:30:00"), "2026-05-31");
    }

    #[test]
    fn test_extract_date_iso() {
        assert_eq!(extract_date("2026-05-31T14:30:00"), "2026-05-31");
    }

    #[test]
    fn test_extract_date_bare() {
        assert_eq!(extract_date("2026-05-31"), "2026-05-31");
    }
}
