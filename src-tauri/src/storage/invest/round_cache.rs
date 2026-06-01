use crate::invest::committee::parser::ParsedFields;
use crate::invest::committee::roles::CommitteeRole;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Round cache — persists per-symbol, per-round parsed outputs for replay
// ---------------------------------------------------------------------------

/// A cached round output for a specific symbol, role, and round.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedRoundOutput {
    pub symbol: String,
    pub role: CommitteeRole,
    pub round: u8,
    pub parsed: ParsedFields,
    pub latency_ms: u64,
    pub tokens_used: u32,
}

/// Get the cache root directory: `~/.claw-go/invest/round-cache/`
fn cache_root() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".claw-go").join("invest").join("round-cache")
}

/// Save a round output to the cache.
pub fn save_round_cache(entry: &CachedRoundOutput) -> Result<(), String> {
    let dir = cache_root().join(&entry.symbol);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("create cache dir: {e}"))?;

    let filename = format!("{}_r{}.json", role_key(entry.role), entry.round);
    let path = dir.join(filename);
    let json = serde_json::to_string_pretty(entry)
        .map_err(|e| format!("serialize cache: {e}"))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("write cache: {e}"))
}

/// Load all cached round outputs for a symbol (across all roles and rounds).
pub fn load_round_cache(symbol: &str) -> Result<Vec<CachedRoundOutput>, String> {
    let dir = cache_root().join(symbol);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();
    let entries = std::fs::read_dir(&dir)
        .map_err(|e| format!("read cache dir: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(cached) = serde_json::from_str::<CachedRoundOutput>(&content) {
                    results.push(cached);
                }
            }
        }
    }

    // Sort by round then role order
    results.sort_by(|a, b| a.round.cmp(&b.round).then(role_sort_key(a.role).cmp(&role_sort_key(b.role))));
    Ok(results)
}

/// Clear the cache for a symbol.
pub fn clear_round_cache(symbol: &str) -> Result<(), String> {
    let dir = cache_root().join(symbol);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| format!("remove cache dir: {e}"))?;
    }
    Ok(())
}

/// Map role to a filesystem-safe key.
fn role_key(role: CommitteeRole) -> &'static str {
    match role {
        CommitteeRole::Macro => "macro",
        CommitteeRole::Quant => "quant",
        CommitteeRole::Risk => "risk",
        CommitteeRole::Cio => "cio",
        CommitteeRole::L4Officer => "l4_officer",
    }
}

/// Sort key for roles (Macro < Quant < Risk < CIO < L4).
fn role_sort_key(role: CommitteeRole) -> u8 {
    match role {
        CommitteeRole::Macro => 0,
        CommitteeRole::Quant => 1,
        CommitteeRole::Risk => 2,
        CommitteeRole::Cio => 3,
        CommitteeRole::L4Officer => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_key_matches() {
        assert_eq!(role_key(CommitteeRole::Macro), "macro");
        assert_eq!(role_key(CommitteeRole::Quant), "quant");
        assert_eq!(role_key(CommitteeRole::Risk), "risk");
        assert_eq!(role_key(CommitteeRole::Cio), "cio");
        assert_eq!(role_key(CommitteeRole::L4Officer), "l4_officer");
    }
}
