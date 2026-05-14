use crate::storage::characters;

/// Compact memory log under char_lock (prevents write-loss race).
/// LanceDB vector index is cleared inside the lock by compact_memory_log_locked.
/// Returns true if compaction was performed.
pub fn compact_memory_log_if_needed(character_id: &str) -> Result<bool, String> {
    characters::compact_memory_log_locked(character_id)
}

/// Apply retention policy under char_lock: atomically removes entries older
/// than retention_days, preventing write-loss races during the read-write cycle.
/// Returns count of removed entries.
pub fn apply_retention_policy(character_id: &str, retention_days: u32) -> Result<usize, String> {
    characters::apply_retention_policy_locked(character_id, retention_days)
}
