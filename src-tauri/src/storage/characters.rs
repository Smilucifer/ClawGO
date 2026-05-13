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

fn validate_character_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(format!("Invalid character_id: {}", id));
    }
    Ok(())
}

fn char_dir(character_id: &str) -> PathBuf {
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

fn memory_log_path(character_id: &str) -> PathBuf {
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

pub fn read_all_memory_log_entries(character_id: &str) -> Result<Vec<MemoryNode>, String> {
    validate_character_id(character_id)?;
    let _lk = char_lock(character_id);
    let _lock = _lk.lock().unwrap_or_else(|e| e.into_inner());
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
        }
    }
    Ok(entries)
}

pub fn delete_memory_from_log(character_id: &str, memory_id: &str) -> Result<(), String> {
    validate_character_id(character_id)?;
    let lock = char_lock(character_id);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    let entries = read_all_memory_log_entries(character_id)?;
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
