use crate::models::MemoryNode;
use crate::storage::memory_store;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DREAM_INTERVAL_SECS: u64 = 6 * 3600; // 6 hours
const DECAY_FACTOR: f64 = 0.98; // confidence decay per cycle
const MIN_CONFIDENCE: f64 = 30.0; // floor for decay
const MERGE_THRESHOLD: f64 = 0.85; // text similarity threshold for merge candidates

#[derive(Serialize, Deserialize)]
struct DreamSnapshot {
    timestamp: u64,
    memories: Vec<MemoryNode>,
}

/// Check if enough time has passed since the last dream cycle.
pub fn should_run_dream(data_dir: &Path) -> bool {
    let marker = data_dir.join("memory_dream_last");
    match std::fs::read_to_string(&marker) {
        Ok(ts_str) => {
            let last: u64 = ts_str.trim().parse().unwrap_or(0);
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            now.saturating_sub(last) >= DREAM_INTERVAL_SECS
        }
        Err(_) => true, // no marker → run immediately
    }
}

/// Update the dream timestamp marker.
fn mark_dream_time(data_dir: &Path) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let marker = data_dir.join("memory_dream_last");
    let _ = std::fs::write(&marker, now.to_string());
}

/// List archived (low-confidence) memories for review.
pub fn list_archived_memories(limit: usize, offset: usize) -> Result<Vec<MemoryNode>, String> {
    memory_store::list_memories(Some("archived"), None, limit, offset)
}

/// Restore an archived memory to approved status.
pub fn restore_archived_memory(id: &str) -> Result<MemoryNode, String> {
    let mut node = memory_store::get_memory(id)?
        .ok_or_else(|| format!("memory {} not found", id))?;
    if node.status != "archived" {
        return Err(format!("memory {} is not archived (status={})", id, node.status));
    }
    node.status = "approved".to_string();
    node.confidence = 60.0; // restore with moderate confidence
    node.updated_at = chrono::Utc::now().to_rfc3339();
    memory_store::update_memory(&node)?;
    Ok(node)
}

/// Export all approved memories to a JSON snapshot file.
pub fn snapshot_memories(data_dir: &Path) -> Result<PathBuf, String> {
    let memories = memory_store::list_memories(Some("approved"), None, 10000, 0)
        .map_err(|e| format!("Failed to list memories: {}", e))?;
    let snapshot = DreamSnapshot {
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        memories,
    };
    let snapshots_dir = data_dir.join("memory_snapshots");
    std::fs::create_dir_all(&snapshots_dir)
        .map_err(|e| format!("Failed to create snapshots dir: {}", e))?;
    let filename = format!("snapshot_{}.json", snapshot.timestamp);
    let path = snapshots_dir.join(&filename);
    let json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| format!("Failed to serialize snapshot: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write snapshot: {}", e))?;

    // Keep only last 10 snapshots
    cleanup_old_snapshots(&snapshots_dir, 10);

    Ok(path)
}

/// Restore memories from a snapshot file, replacing current store.
pub fn rollback_to_snapshot(snapshot_path: &Path, data_dir: &Path) -> Result<usize, String> {
    let json = std::fs::read_to_string(snapshot_path)
        .map_err(|e| format!("Failed to read snapshot: {}", e))?;
    let snapshot: DreamSnapshot =
        serde_json::from_str(&json).map_err(|e| format!("Failed to parse snapshot: {}", e))?;

    // Clear existing memories
    let existing = memory_store::list_memories(None, None, 10000, 0)
        .map_err(|e| format!("Failed to list memories: {}", e))?;
    for mem in &existing {
        let _ = memory_store::delete_memory(&mem.id);
    }

    // Import from snapshot
    let count = snapshot.memories.len();
    for mem in snapshot.memories {
        let _ = memory_store::insert_memory(&mem);
    }

    log::info!(
        "[dream] rolled back to snapshot from {}, restored {} memories",
        snapshot.timestamp,
        count
    );

    // Update marker to prevent immediate re-run
    mark_dream_time(data_dir);

    Ok(count)
}

/// Run a dream cycle: snapshot → find duplicates → merge → decay confidence.
///
/// This is a synchronous function intended to be called from a tokio spawn_blocking.
pub fn run_dream_cycle(data_dir: &Path) -> Result<DreamCycleResult, String> {
    if !should_run_dream(data_dir) {
        return Ok(DreamCycleResult::Skipped);
    }

    log::info!("[dream] starting dream cycle");

    // 1. Snapshot
    let snapshot_path = snapshot_memories(data_dir)?;

    // 2. Find and merge duplicates
    let merged = merge_duplicates()?;

    // 3. Decay confidence on old memories
    let decayed = decay_confidence()?;

    // 4. Mark completion
    mark_dream_time(data_dir);

    log::info!(
        "[dream] cycle complete: merged={}, decayed={}, snapshot={}",
        merged,
        decayed,
        snapshot_path.display()
    );

    Ok(DreamCycleResult::Completed {
        merged,
        decayed,
        snapshot_path,
    })
}

#[derive(Debug)]
pub enum DreamCycleResult {
    Skipped,
    Completed {
        merged: usize,
        decayed: usize,
        snapshot_path: PathBuf,
    },
}

/// Find memories with similar content and merge them.
/// Uses simple text similarity (Jaccard on character n-grams).
fn merge_duplicates() -> Result<usize, String> {
    let memories = memory_store::list_memories(Some("approved"), None, 10000, 0)
        .map_err(|e| format!("Failed to list memories: {}", e))?;

    let mut merged_count = 0;
    let mut skip_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for i in 0..memories.len() {
        if skip_ids.contains(&memories[i].id) {
            continue;
        }
        for j in (i + 1)..memories.len() {
            if skip_ids.contains(&memories[j].id) {
                continue;
            }
            let sim = text_similarity(&memories[i].content, &memories[j].content);
            if sim >= MERGE_THRESHOLD {
                // Keep the one with higher confidence, merge tags
                let (keep, remove) = if memories[i].confidence >= memories[j].confidence {
                    (&memories[i], &memories[j])
                } else {
                    (&memories[j], &memories[i])
                };

                // Merge tags
                let mut merged_tags = keep.tags.clone();
                for tag in &remove.tags {
                    if !merged_tags.contains(tag) {
                        merged_tags.push(tag.clone());
                    }
                }

                // Update the keeper with merged tags and boosted confidence
                let new_confidence = (keep.confidence + 5.0).min(100.0);
                let now = chrono::Utc::now().to_rfc3339();
                let updated_node = MemoryNode {
                    id: keep.id.clone(),
                    character_id: keep.character_id.clone(),
                    content: keep.content.clone(),
                    memory_type: keep.memory_type.clone(),
                    confidence: new_confidence,
                    source: keep.source.clone(),
                    tags: merged_tags,
                    created_at: keep.created_at.clone(),
                    updated_at: now,
                    status: keep.status.clone(),
                };
                let _ = memory_store::update_memory(&updated_node);

                // Delete the duplicate
                let _ = memory_store::delete_memory(&remove.id);
                skip_ids.insert(remove.id.clone());
                merged_count += 1;

                log::debug!(
                    "[dream] merged '{}' into '{}' (sim={:.2})",
                    truncate_str(&remove.content, 40),
                    truncate_str(&keep.content, 40),
                    sim
                );
            }
        }
    }

    Ok(merged_count)
}

/// Decay confidence of all approved memories slightly.
/// Memories below MIN_CONFIDENCE are marked as archived.
fn decay_confidence() -> Result<usize, String> {
    let memories = memory_store::list_memories(Some("approved"), None, 10000, 0)
        .map_err(|e| format!("Failed to list memories: {}", e))?;

    let mut decayed = 0;
    for mem in &memories {
        let new_confidence = mem.confidence * DECAY_FACTOR;
        if new_confidence < MIN_CONFIDENCE {
            // Archive low-confidence memories
            let now = chrono::Utc::now().to_rfc3339();
            let updated_node = MemoryNode {
                id: mem.id.clone(),
                character_id: mem.character_id.clone(),
                content: mem.content.clone(),
                memory_type: mem.memory_type.clone(),
                confidence: new_confidence,
                source: mem.source.clone(),
                tags: mem.tags.clone(),
                created_at: mem.created_at.clone(),
                updated_at: now,
                status: "archived".to_string(),
            };
            let _ = memory_store::update_memory(&updated_node);
            log::debug!(
                "[dream] archived low-confidence memory: {} (was {:.1}%)",
                truncate_str(&mem.content, 40),
                mem.confidence
            );
        } else if (new_confidence - mem.confidence).abs() > 0.01 {
            let now = chrono::Utc::now().to_rfc3339();
            let updated_node = MemoryNode {
                id: mem.id.clone(),
                character_id: mem.character_id.clone(),
                content: mem.content.clone(),
                memory_type: mem.memory_type.clone(),
                confidence: new_confidence,
                source: mem.source.clone(),
                tags: mem.tags.clone(),
                created_at: mem.created_at.clone(),
                updated_at: now,
                status: mem.status.clone(),
            };
            let _ = memory_store::update_memory(&updated_node);
        }
        decayed += 1;
    }

    Ok(decayed)
}

/// Compute text similarity using Jaccard index on word tokens.
/// Consistent with memory_store::find_duplicates for uniform dedup behavior.
fn text_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }
    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Remove old snapshot files, keeping only the N most recent.
fn cleanup_old_snapshots(dir: &Path, keep: usize) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "json")
                .unwrap_or(false)
        })
        .filter_map(|e| {
            let time = e.metadata().ok()?.modified().ok()?;
            Some((time, e.path()))
        })
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0)); // newest first
    for (_, path) in files.into_iter().skip(keep) {
        let _ = std::fs::remove_file(path);
    }
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect::<String>() + if s.chars().count() > max_chars { "..." } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_similarity_identical() {
        let sim = text_similarity("hello world", "hello world");
        assert!((sim - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_text_similarity_different() {
        let sim = text_similarity("hello world", "foo bar baz");
        assert!(sim < 0.3);
    }

    #[test]
    fn test_text_similarity_similar() {
        let sim = text_similarity(
            "user prefers dark mode in all applications",
            "user prefers dark mode in all apps",
        );
        assert!(sim > 0.5);
    }

    #[test]
    fn test_text_similarity_cjk() {
        let sim = text_similarity("用户喜欢用 Go 写后端", "用户喜欢用 Rust 写后端");
        assert!(sim > 0.5, "CJK word-overlap should work: got {}", sim);
    }
}
