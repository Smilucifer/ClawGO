use std::path::{Path, PathBuf};

use crate::models::MemoryNode;
use crate::storage::memory_store;

/// Migrate per-character JSONL memory logs to the global SQLite memory store.
/// Idempotent: writes a marker file after migration, skips if already done.
pub fn migrate_jsonl_to_sqlite(data_dir: &Path) -> Result<usize, String> {
    let marker = data_dir.join(".memory_migrated_v2");
    if marker.exists() {
        return Ok(0);
    }

    let chars_dir = data_dir.join("characters");
    if !chars_dir.exists() {
        // No characters dir — nothing to migrate
        std::fs::write(&marker, b"done").map_err(|e| format!("write marker: {}", e))?;
        return Ok(0);
    }

    let mut total_imported = 0;

    let entries = std::fs::read_dir(&chars_dir).map_err(|e| format!("read chars dir: {}", e))?;
    for entry in entries.flatten() {
        let jsonl_path = entry.path().join("memory-log.jsonl");
        if !jsonl_path.exists() {
            continue;
        }

        match migrate_single_jsonl(&jsonl_path) {
            Ok(n) => {
                if n > 0 {
                    log::info!(
                        "Migrated {} memories from {}",
                        n,
                        entry.file_name().to_string_lossy()
                    );
                }
                total_imported += n;
            }
            Err(e) => {
                log::warn!(
                    "Failed to migrate {}: {}",
                    entry.file_name().to_string_lossy(),
                    e
                );
            }
        }
    }

    // Write marker even if 0 imported (no data = migration is "done")
    std::fs::write(&marker, b"done").map_err(|e| format!("write marker: {}", e))?;

    Ok(total_imported)
}

fn migrate_single_jsonl(path: &PathBuf) -> Result<usize, String> {
    use std::io::{BufRead, BufReader};

    let file = std::fs::File::open(path).map_err(|e| format!("open jsonl: {}", e))?;
    let reader = BufReader::new(file);
    let mut imported = 0;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("read line: {}", e))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse as legacy MemoryNode (with character_id)
        let node: MemoryNode = match serde_json::from_str(line) {
            Ok(n) => n,
            Err(e) => {
                log::warn!("skip invalid jsonl line: {}", e);
                continue;
            }
        };

        // Skip if already exists in SQLite
        if memory_store::get_memory(&node.id)?.is_some() {
            continue;
        }

        // Skip non-approved memories
        if node.status != "approved" {
            continue;
        }

        // Insert into SQLite (character_id is ignored by the store)
        memory_store::insert_memory(&node)?;
        imported += 1;
    }

    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MemoryNode, MemorySource};

    #[test]
    fn test_migration_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();

        // Create a character dir with a memory-log.jsonl
        let char_dir = data_dir.join("characters").join("char-1");
        std::fs::create_dir_all(&char_dir).unwrap();

        let node = MemoryNode {
            id: "m1".to_string(),
            character_id: "char-1".to_string(),
            content: "test memory".to_string(),
            memory_type: "fact".to_string(),
            confidence: 0.9,
            source: MemorySource {
                kind: "extraction".to_string(),
                run_id: None,
                group_chat_id: None,
            },
            tags: vec!["test".to_string()],
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            status: "approved".to_string(),
        };

        let jsonl_content = serde_json::to_string(&node).unwrap();
        std::fs::write(char_dir.join("memory-log.jsonl"), &jsonl_content).unwrap();

        // Init DB and migrate
        memory_store::init_db(&data_dir).unwrap();
        let count = migrate_jsonl_to_sqlite(&data_dir).unwrap();
        assert_eq!(count, 1);

        // Verify data
        let m = memory_store::get_memory("m1").unwrap().unwrap();
        assert_eq!(m.content, "test memory");

        // Run again — should be idempotent (0 imported)
        let count2 = migrate_jsonl_to_sqlite(&data_dir).unwrap();
        assert_eq!(count2, 0);
    }
}
