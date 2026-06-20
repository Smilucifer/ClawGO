//! User-driven cleanup commands for the invest subsystem.
//!
//! Design: scan → confirm → apply.
//! - `invest_cleanup_scan` is READ-ONLY: it only counts rows / checks dirs and
//!   reports the totals to the frontend.
//! - `invest_cleanup_apply` deletes ONLY the targets the user explicitly
//!   confirmed (via the frontend checkboxes + dialog). Nothing auto-deletes
//!   on startup or on scan.
//!
//! The dirty `verdicts` / `pnl_snapshots` heuristic deletes from spec C5 are
//! intentionally NOT included here — their characterization is too ambiguous
//! to safely codify a destructive WHERE.
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupReport {
    pub daily_reports_rows: i64,
    pub event_sources_rows: i64,
    pub domain_insights_rows: i64,
    /// Domain-insight rows considered "empty / idle dreaming" output.
    /// `domain_insights.content` is `NOT NULL` per schema, so the IS NULL
    /// arm is harmless/never matches; the empty-or-very-short check is what
    /// actually gates this count.
    pub domain_insights_empty_rows: i64,
    pub rooms_dir_exists: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupTargets {
    pub daily_reports: bool,
    pub event_sources: bool,
    pub domain_insights_empty: bool,
    pub rooms_dir: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult {
    /// Human-readable summary of what was deleted, one entry per applied
    /// target (e.g. `"daily_reports: 42 rows"`).
    pub deleted: Vec<String>,
}

/// Match expression for "empty / idle dreaming" insight rows.
///
/// `content` is `NOT NULL` in the schema (`CREATE_TABLES_SQL`), so the
/// `IS NULL` arm is vacuous but kept for safety in case an older DB had a
/// nullable column. The substantive filter is empty/whitespace or very
/// short content (< 10 trimmed chars), which is what idle dreaming output
/// looks like.
const DOMAIN_INSIGHTS_EMPTY_WHERE: &str =
    "content IS NULL OR TRIM(content) = '' OR LENGTH(TRIM(content)) < 10";

/// READ-ONLY scan of cleanup targets. Returns counts/existence; deletes
/// nothing.
#[tauri::command]
pub async fn invest_cleanup_scan() -> Result<CleanupReport, String> {
    let (dr, es, di, di_empty) = crate::storage::invest::with_conn(|conn| {
        let dr: i64 = conn
            .query_row("SELECT COUNT(*) FROM daily_reports", [], |r| r.get(0))
            .unwrap_or(0);
        let es: i64 = conn
            .query_row("SELECT COUNT(*) FROM event_sources", [], |r| r.get(0))
            .unwrap_or(0);
        let di: i64 = conn
            .query_row("SELECT COUNT(*) FROM domain_insights", [], |r| r.get(0))
            .unwrap_or(0);
        let di_empty: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM domain_insights WHERE {DOMAIN_INSIGHTS_EMPTY_WHERE}"),
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        Ok((dr, es, di, di_empty))
    })?;

    let rooms = crate::storage::data_dir().join("rooms");
    Ok(CleanupReport {
        daily_reports_rows: dr,
        event_sources_rows: es,
        domain_insights_rows: di,
        domain_insights_empty_rows: di_empty,
        rooms_dir_exists: rooms.exists(),
    })
}

/// Apply the user-confirmed cleanup targets. Each `true` flag in
/// `targets` triggers exactly one destructive operation.
#[tauri::command]
pub async fn invest_cleanup_apply(targets: CleanupTargets) -> Result<CleanupResult, String> {
    let mut deleted: Vec<String> = Vec::new();

    crate::storage::invest::with_conn(|conn| {
        if targets.daily_reports {
            let n = conn
                .execute("DELETE FROM daily_reports", [])
                .map_err(|e| format!("delete daily_reports: {e}"))?;
            log::info!("[invest_cleanup] deleted {n} rows from daily_reports");
            deleted.push(format!("daily_reports: {n} rows"));
        }
        if targets.event_sources {
            let n = conn
                .execute("DELETE FROM event_sources", [])
                .map_err(|e| format!("delete event_sources: {e}"))?;
            log::info!("[invest_cleanup] deleted {n} rows from event_sources");
            deleted.push(format!("event_sources: {n} rows"));
        }
        if targets.domain_insights_empty {
            let n = conn
                .execute(
                    &format!("DELETE FROM domain_insights WHERE {DOMAIN_INSIGHTS_EMPTY_WHERE}"),
                    [],
                )
                .map_err(|e| format!("delete empty domain_insights: {e}"))?;
            log::info!(
                "[invest_cleanup] deleted {n} rows from domain_insights (empty/idle dreaming)"
            );
            deleted.push(format!("domain_insights(empty): {n} rows"));
        }
        Ok(())
    })?;

    if targets.rooms_dir {
        let rooms = crate::storage::data_dir().join("rooms");
        if rooms.exists() {
            std::fs::remove_dir_all(&rooms)
                .map_err(|e| format!("remove rooms dir {}: {e}", rooms.display()))?;
            log::info!("[invest_cleanup] removed legacy rooms dir at {}", rooms.display());
            deleted.push("rooms/ dir".into());
        } else {
            log::info!("[invest_cleanup] rooms dir not present, nothing to remove");
        }
    }

    Ok(CleanupResult { deleted })
}
