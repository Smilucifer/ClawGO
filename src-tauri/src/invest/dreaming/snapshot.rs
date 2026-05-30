use crate::storage::invest::dream_snapshots;

/// Save a dream snapshot record. Single atomic INSERT with all fields.
pub fn save_snapshot(
    dream_type: &str,
    trigger_type: &str,
    before_json: &str,
    after_json: &str,
    summary: &str,
) -> Result<i64, String> {
    dream_snapshots::insert_complete(dream_type, trigger_type, before_json, after_json, summary)
}

/// Rollback a dream snapshot: restore domain_insights to before state.
pub fn rollback_snapshot(snapshot_id: i64) -> Result<(), String> {
    let snapshot = dream_snapshots::get_by_id(snapshot_id)?
        .ok_or("Snapshot not found")?;

    if !snapshot.rollback_ready {
        return Err("Snapshot is not rollback-ready".into());
    }

    // Verify current state matches after_json (if available)
    if let Some(after) = &snapshot.after_json {
        let current = crate::storage::invest::domain_insights::get_active_insights_json()?;
        if &current != after {
            return Err(
                "Current domain_insights state has changed since this dream. Rollback aborted."
                    .into(),
            );
        }
    }

    // Restore
    crate::storage::invest::domain_insights::restore_insight_snapshot(&snapshot.before_json)?;
    dream_snapshots::mark_rolled_back(snapshot_id)?;
    Ok(())
}
