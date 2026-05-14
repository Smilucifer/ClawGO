use crate::models::{AiCharacter, MemoryGraphData, MemoryNode};
use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::fs;

static CHAR_LOCKS: Lazy<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub(crate) fn validate_character_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(format!("Invalid character_id: {}", id));
    }
    // Reject Windows-unsafe filename characters
    if id.contains(|c: char| matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|')) {
        return Err(format!("Invalid character_id: {}", id));
    }
    Ok(())
}

pub(crate) fn char_dir(character_id: &str) -> PathBuf {
    super::data_dir().join("characters").join(character_id)
}

fn char_lock(character_id: &str) -> Arc<Mutex<()>> {
    let mut map = CHAR_LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    map.entry(character_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn ensure_char_dir(character_id: &str) -> Result<PathBuf, String> {
    let dir = char_dir(character_id);
    super::ensure_dir(&dir).map_err(|e| format!("ensure char dir: {e}"))?;
    Ok(dir)
}

// --- Atomic JSON write (sync variant, with UUID temp name) ---

fn write_atomic_json<T: Serialize>(path: &Path, data: &T) -> Result<(), String> {
    let tmp = path.with_extension(format!("json.{}.tmp", uuid::Uuid::new_v4()));
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    fs::write(&tmp, &json).map_err(|e| format!("write tmp: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }

    fs::rename(&tmp, path).map_err(|e| format!("rename: {e}"))?;
    Ok(())
}

// --- Memory Log (authoritative source) ---

pub(crate) fn memory_log_path(character_id: &str) -> PathBuf {
    char_dir(character_id).join("memory-log.jsonl")
}

pub fn append_memory_log(character_id: &str, node: &MemoryNode) -> Result<(), String> {
    validate_character_id(character_id)?;
    let lock = char_lock(character_id);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    ensure_char_dir(character_id)?;
    let path = memory_log_path(character_id);
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open log: {e}"))?;
    let line = serde_json::to_string(node).map_err(|e| e.to_string())? + "\n";
    file.write_all(line.as_bytes())
        .map_err(|e| format!("write log: {e}"))?;
    Ok(())
}

fn read_log_entries_unlocked(character_id: &str) -> Result<Vec<MemoryNode>, String> {
    let path = memory_log_path(character_id);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(&path).map_err(|e| format!("open memory log: {e}"))?;
    let reader = std::io::BufReader::new(file);
    let mut entries = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| format!("read line: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(node) = serde_json::from_str::<MemoryNode>(&line) {
            entries.push(node);
        } else {
            log::warn!("[characters] skipping unparseable memory log line in {}", character_id);
        }
    }
    Ok(entries)
}

/// Compact the memory log under char_lock to prevent write-loss races.
/// Deduplicates entries (keep latest version of each ID), writes compacted log atomically.
/// Also clears the LanceDB vector index while the lock is held so concurrent upserts
/// cannot land in the index between compaction and cleanup.
/// Returns true if compaction was performed, false if below threshold.
///
/// Triggered when log exceeds 10,000 lines OR more than 30 days since last compaction.

/// Clear the LanceDB vector index for a character and write the rebuild marker.
/// If `skip_if_marker_exists` is true, no-op when `.rebuild_pending` already exists
/// (used by retention to avoid redundant clears after compaction already cleared).
/// Returns true if the index was cleared.
fn clear_lancedb_index(character_id: &str, skip_if_marker_exists: bool) -> bool {
    if skip_if_marker_exists {
        let marker = char_dir(character_id).join(".rebuild_pending");
        if marker.exists() {
            return false;
        }
    }
    let lancedb_dir = char_dir(character_id).join("lancedb");
    if !lancedb_dir.exists() {
        return false;
    }
    match std::fs::remove_dir_all(&lancedb_dir) {
        Ok(()) => {
            let marker = char_dir(character_id).join(".rebuild_pending");
            let _ = std::fs::write(&marker, b"1");
            true
        }
        Err(e) => {
            log::warn!("clear_lancedb_index: failed for {}: {e}", character_id);
            false
        }
    }
}

pub fn compact_memory_log_locked(character_id: &str) -> Result<bool, String> {
    validate_character_id(character_id)?;
    let lock = char_lock(character_id);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());

    let entries = read_log_entries_unlocked(character_id)?;

    // Check time-based trigger: compact if > 30 days since last compaction
    let compaction_marker = char_dir(character_id).join(".last_compaction");
    let time_triggered = if let Ok(meta) = std::fs::metadata(&compaction_marker) {
        if let Ok(modified) = meta.modified() {
            modified.elapsed().map(|d| d.as_secs() > 30 * 24 * 3600).unwrap_or(false)
        } else {
            false
        }
    } else {
        // No marker yet — eligible for time-based trigger once the log has
        // at least 1,000 entries (first-ever compaction for this character).
        entries.len() > 1_000
    };

    if entries.len() < 10_000 && !time_triggered {
        return Ok(false);
    }

    let mut latest: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, entry) in entries.iter().enumerate() {
        latest.insert(entry.id.clone(), i);
    }
    let mut compacted: Vec<_> = latest.into_values().map(|i| entries[i].clone()).collect();
    compacted.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    let path = memory_log_path(character_id);
    let tmp_path = path.with_extension(format!("jsonl.{}.tmp", uuid::Uuid::new_v4()));
    {
        let mut file = fs::File::create(&tmp_path)
            .map_err(|e| format!("compact: create tmp: {e}"))?;
        for node in &compacted {
            let line = serde_json::to_string(node).map_err(|e| format!("compact: serialize: {e}"))? + "\n";
            file.write_all(line.as_bytes()).map_err(|e| format!("compact: write: {e}"))?;
        }
        file.flush().map_err(|e| format!("compact: flush: {e}"))?;
    }
    fs::rename(&tmp_path, &path).map_err(|e| format!("compact: rename: {e}"))?;

    // Clear LanceDB vector index inside the lock so concurrent upserts can't
    // land stale vectors in the index after compaction removed the source entries.
    if clear_lancedb_index(character_id, false) {
        log::info!(
            "compact: vector index cleared for {}. Run rebuild_vector_index to restore.",
            character_id
        );
    }

    // Record compaction timestamp
    let _ = std::fs::write(&compaction_marker, b"1");

    Ok(true)
}

/// Apply retention policy under char_lock: atomically removes entries older
/// than retention_days, preventing write-loss races during the read-write cycle.
/// Returns count of removed entries.
pub fn apply_retention_policy_locked(character_id: &str, retention_days: u32) -> Result<usize, String> {
    validate_character_id(character_id)?;
    let lock = char_lock(character_id);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());

    let entries = read_log_entries_unlocked(character_id)?;
    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);

    let (keep, removed): (Vec<_>, Vec<_>) = entries
        .into_iter()
        .partition(|e| {
            chrono::DateTime::parse_from_rfc3339(&e.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc) >= cutoff)
                .unwrap_or(true) // keep unparseable entries (defensive)
        });

    let path = memory_log_path(character_id);
    let tmp_path = path.with_extension(format!("jsonl.{}.tmp", uuid::Uuid::new_v4()));
    {
        let mut file = fs::File::create(&tmp_path)
            .map_err(|e| format!("retention: create tmp: {e}"))?;
        for node in &keep {
            let line = serde_json::to_string(node)
                .map_err(|e| format!("retention: serialize: {e}"))? + "\n";
            file.write_all(line.as_bytes())
                .map_err(|e| format!("retention: write: {e}"))?;
        }
        file.flush().map_err(|e| format!("retention: flush: {e}"))?;
    }
    fs::rename(&tmp_path, &path)
        .map_err(|e| format!("retention: rename: {e}"))?;

    // Clear LanceDB vector index so stale vectors for removed entries are purged.
    // Skip if compaction already cleared it (check .rebuild_pending marker).
    if !removed.is_empty() {
        clear_lancedb_index(character_id, true);
    }

    Ok(removed.len())
}

pub fn read_all_memory_log_entries(character_id: &str) -> Result<Vec<MemoryNode>, String> {
    validate_character_id(character_id)?;
    let _lk = char_lock(character_id);
    let _guard = _lk.lock().unwrap_or_else(|e| e.into_inner());
    read_log_entries_unlocked(character_id)
}

pub fn delete_memory_from_log(character_id: &str, memory_id: &str) -> Result<(), String> {
    validate_character_id(character_id)?;
    let lock = char_lock(character_id);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    let entries = read_log_entries_unlocked(character_id)?;
    let filtered: Vec<_> = entries.into_iter().filter(|n| n.id != memory_id).collect();
    let path = memory_log_path(character_id);

    // Atomic write: temp file + rename
    let tmp = path.with_extension(format!("jsonl.{}.tmp", uuid::Uuid::new_v4()));
    {
        let mut tmp_file =
            fs::File::create(&tmp).map_err(|e| format!("create tmp log: {e}"))?;
        for node in &filtered {
            let line = serde_json::to_string(node).map_err(|e| e.to_string())? + "\n";
            tmp_file
                .write_all(line.as_bytes())
                .map_err(|e| format!("write log: {e}"))?;
        }
    }
    fs::rename(&tmp, &path).map_err(|e| format!("rename log: {e}"))?;
    Ok(())
}

/// Atomically update a memory entry in the log under per-ID lock.
/// Returns the updated MemoryNode.
pub fn update_memory_in_log(
    character_id: &str,
    memory_id: &str,
    content: Option<String>,
    memory_type: Option<String>,
    confidence: Option<f64>,
    tags: Option<Vec<String>>,
) -> Result<MemoryNode, String> {
    validate_character_id(character_id)?;
    let lock = char_lock(character_id);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());

    let mut entries = read_log_entries_unlocked(character_id)?;
    let idx = entries
        .iter()
        .position(|n| n.id == memory_id)
        .ok_or_else(|| format!("Memory not found: {}", memory_id))?;

    let now = chrono::Utc::now().to_rfc3339();
    if let Some(c) = content {
        entries[idx].content = c;
    }
    if let Some(t) = memory_type {
        entries[idx].memory_type = t;
    }
    if let Some(c) = confidence {
        entries[idx].confidence = c;
    }
    if let Some(t) = tags {
        entries[idx].tags = t;
    }
    entries[idx].updated_at = now;

    let updated = entries[idx].clone();

    // Rewrite atomically via temp file + rename
    let path = memory_log_path(character_id);
    let tmp_path = path.with_extension(format!("jsonl.{}.tmp", uuid::Uuid::new_v4()));
    let mut file =
        std::fs::File::create(&tmp_path).map_err(|e| format!("update_memory_log: create tmp: {e}"))?;
    for node in &entries {
        let line = serde_json::to_string(node)
            .map_err(|e| format!("update_memory_log: serialize: {e}"))?
            + "\n";
        file.write_all(line.as_bytes())
            .map_err(|e| format!("update_memory_log: write: {e}"))?;
    }
    file.flush()
        .map_err(|e| format!("update_memory_log: flush: {e}"))?;
    std::fs::rename(&tmp_path, &path)
        .map_err(|e| format!("update_memory_log: rename: {e}"))?;

    Ok(updated)
}

// --- Memory Graph (derived) ---

fn memory_graph_path(character_id: &str) -> PathBuf {
    char_dir(character_id).join("memory-graph.json")
}

pub fn save_memory_graph(character_id: &str, graph: &MemoryGraphData) -> Result<(), String> {
    validate_character_id(character_id)?;
    let lock = char_lock(character_id);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    ensure_char_dir(character_id)?;
    let path = memory_graph_path(character_id);
    write_atomic_json(&path, graph)?;
    Ok(())
}

pub fn load_memory_graph(character_id: &str) -> Result<MemoryGraphData, String> {
    validate_character_id(character_id)?;
    let _lk = char_lock(character_id);
    let _lock = _lk.lock().unwrap_or_else(|e| e.into_inner());
    let path = memory_graph_path(character_id);
    if !path.exists() {
        return Ok(MemoryGraphData {
            nodes: Vec::new(),
            edges: Vec::new(),
        });
    }
    let content =
        fs::read_to_string(&path).map_err(|e| format!("read graph: {e}"))?;
    let graph: MemoryGraphData =
        serde_json::from_str(&content).map_err(|e| format!("parse graph: {e}"))?;
    Ok(graph)
}

// --- Character Metadata ---

fn character_json_path(character_id: &str) -> PathBuf {
    char_dir(character_id).join("character.json")
}

pub fn save_character_metadata(character: &AiCharacter) -> Result<(), String> {
    validate_character_id(&character.id)?;
    let lock = char_lock(&character.id);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    ensure_char_dir(&character.id)?;
    let path = character_json_path(&character.id);
    write_atomic_json(&path, character)?;
    Ok(())
}

pub fn load_character_metadata(character_id: &str) -> Result<Option<AiCharacter>, String> {
    validate_character_id(character_id)?;
    let _lk = char_lock(character_id);
    let _lock = _lk.lock().unwrap_or_else(|e| e.into_inner());
    let path = character_json_path(character_id);
    if !path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(&path).map_err(|e| format!("read metadata: {e}"))?;
    let character: AiCharacter =
        serde_json::from_str(&content).map_err(|e| format!("parse metadata: {e}"))?;
    Ok(Some(character))
}
