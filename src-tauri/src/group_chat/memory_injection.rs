use crate::commands::embedding;
use crate::commands::vectorstore;
use crate::group_chat::memory_graph::graph_expand;
use crate::models::MemoryNode;
use crate::storage::characters;
use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Instant;

// Embedding health state — cached for 60 seconds
static EMBEDDING_HEALTH: Lazy<Mutex<Option<(bool, Instant)>>> =
    Lazy::new(|| Mutex::new(None));

fn is_embedding_healthy() -> bool {
    if let Ok(guard) = EMBEDDING_HEALTH.lock() {
        if let Some((healthy, last_check)) = *guard {
            if last_check.elapsed().as_secs() < 60 {
                return healthy;
            }
        }
    }
    true // assume healthy until proven otherwise
}

fn set_embedding_healthy(healthy: bool) {
    if let Ok(mut guard) = EMBEDDING_HEALTH.lock() {
        *guard = Some((healthy, Instant::now()));
    }
}

// Prevent duplicate lazy rebuild spawns
static REBUILD_IN_FLIGHT: Lazy<Mutex<HashSet<String>>> =
    Lazy::new(|| Mutex::new(HashSet::new()));

/// Drop guard that removes a character_id from REBUILD_IN_FLIGHT on drop,
/// ensuring cleanup even if the spawned task panics.
struct InFlightGuard {
    character_id: String,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        REBUILD_IN_FLIGHT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.character_id);
    }
}

fn try_trigger_rebuild(character_id: &str) {
    let marker_path = characters::char_dir(character_id).join(".rebuild_pending");
    if !marker_path.exists() {
        return;
    }
    let mut inflight = REBUILD_IN_FLIGHT
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if inflight.contains(character_id) {
        return;
    }
    inflight.insert(character_id.to_string());
    drop(inflight);

    let cid = character_id.to_string();
    tokio::spawn(async move {
        let _guard = InFlightGuard { character_id: cid.clone() };
        // Remove marker only once the task is running, not before spawn
        let marker = characters::char_dir(&cid).join(".rebuild_pending");
        let _ = std::fs::remove_file(&marker);
        match vectorstore::rebuild_vector_index(cid.clone()).await {
            Ok(n) => log::info!("lazy rebuild: {n} entries for {cid}"),
            Err(e) => {
                log::warn!("lazy rebuild failed for {cid}: {e}");
                let _ = std::fs::write(&marker, b"1");
            }
        }
    });
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DegradationTier {
    Full,
    Degraded,
    Minimal,
    Skip,
}

/// Hybrid search for relevant memories to inject.
pub async fn search_memories_for_injection(
    character_id: &str,
    query: &str,
    top_k: usize,
    threshold: f64,
    graph_hops: usize,
) -> (Vec<MemoryNode>, DegradationTier) {
    // Load log entries
    let entries = match characters::read_all_memory_log_entries(character_id) {
        Ok(e) => e,
        Err(_) => return (Vec::new(), DegradationTier::Skip),
    };
    if entries.is_empty() {
        return (Vec::new(), DegradationTier::Skip);
    }

    if !is_embedding_healthy() {
        return degraded_keyword_search(&entries, query, top_k);
    }

    // Full: try vector search
    let query_vec = match embedding::fetch_embedding(query).await {
        Ok(v) => v,
        Err(_) => {
            set_embedding_healthy(false);
            return degraded_keyword_search(&entries, query, top_k);
        }
    };
    set_embedding_healthy(true);

    let vector_results = match vectorstore::vector_search(
        character_id.to_string(),
        query_vec,
        top_k as u32 * 2,
    ).await {
        Ok(r) => r,
        Err(_) => {
            set_embedding_healthy(false);
            return degraded_keyword_search(&entries, query, top_k);
        }
    };

    // Trigger lazy rebuild if vector index is empty but memory log has entries
    if vector_results.is_empty() && !entries.is_empty() {
        try_trigger_rebuild(character_id);
        return degraded_keyword_search(&entries, query, top_k);
    }

    // Graph expansion from vector results
    let graph = characters::load_memory_graph(character_id).unwrap_or_default();
    let seed_ids: Vec<String> = vector_results.iter().map(|r| r.page_id.clone()).collect();
    let expanded_ids = graph_expand(&graph.edges, &seed_ids, graph_hops);

    // Merge: vector + keyword + expanded
    let mut scored: Vec<(MemoryNode, f64)> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for vr in &vector_results {
        if let Some(node) = entries.iter().find(|n| n.id == vr.page_id) {
            if !seen.contains(&node.id) {
                let keyword_boost = keyword_match_score(&node.content, query);
                scored.push((node.clone(), vr.score * 0.6 + keyword_boost * 0.4));
                seen.insert(node.id.clone());
            }
        }
    }

    for eid in &expanded_ids {
        if !seen.contains(eid) {
            if let Some(node) = entries.iter().find(|n| &n.id == eid) {
                let keyword_boost = keyword_match_score(&node.content, query);
                scored.push((node.clone(), 0.45 + keyword_boost * 0.3));
                seen.insert(eid.clone());
            }
        }
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let results: Vec<MemoryNode> = scored
        .into_iter()
        .filter(|(_, s)| *s >= threshold)
        .take(top_k)
        .map(|(n, _)| n)
        .collect();

    (results, DegradationTier::Full)
}

fn degraded_keyword_search(
    entries: &[MemoryNode],
    query: &str,
    top_k: usize,
) -> (Vec<MemoryNode>, DegradationTier) {
    let mut scored: Vec<(MemoryNode, f64)> = entries
        .iter()
        .map(|node| {
            let keyword_score = keyword_match_score(&node.content, query);
            (node.clone(), keyword_score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let results: Vec<MemoryNode> = scored.into_iter().take(top_k).map(|(n, _)| n).collect();
    (results, DegradationTier::Degraded)
}

fn keyword_match_score(text: &str, query: &str) -> f64 {
    let text_lower = text.to_lowercase();
    let query_lower = query.to_lowercase();
    let query_terms: Vec<&str> = query_lower.split_whitespace().collect();
    let mut matches = 0;
    for term in &query_terms {
        if text_lower.contains(term) {
            matches += 1;
        }
    }
    if query_terms.is_empty() { 0.0 } else { matches as f64 / query_terms.len() as f64 }
}

/// Estimate token count for a string, counting CJK characters as ~1 token
/// and ASCII/Latin characters as ~0.25 tokens (4 chars per token).
fn approx_tokens(s: &str) -> usize {
    let mut tokens: f64 = 0.0;
    for ch in s.chars() {
        tokens += cjk_token_weight(ch);
    }
    tokens.ceil() as usize
}

fn cjk_token_weight(ch: char) -> f64 {
    if matches!(ch as u32, 0x3400..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FFFF | 0x30000..=0x3FFFF) {
        1.0
    } else {
        0.25
    }
}

/// Format memories for system prompt injection, respecting token budget.
pub fn format_memory_injection(
    memories: &[MemoryNode],
    max_tokens: usize,
    max_tokens_per_memory: usize,
) -> String {
    if memories.is_empty() {
        return String::new();
    }

    let mut lines = vec!["[Character Memory]".to_string()];
    let mut token_count = 0;

    for (i, mem) in memories.iter().enumerate() {
        // Truncate content to per-memory token budget using CJK-aware counting
        let mut content_tokens: f64 = 0.0;
        let truncated: String = mem.content.chars()
            .take_while(|ch| {
                let t = cjk_token_weight(*ch);
                if (content_tokens + t).ceil() as usize > max_tokens_per_memory {
                    return false;
                }
                content_tokens += t;
                true
            })
            .collect();
        let tag = match mem.memory_type.as_str() {
            "fact" => "Fact",
            "experience" => "Experience",
            "preference" => "Preference",
            "rule" => "Rule",
            "relationship" => "Relationship",
            _ => "Memory",
        };
        let line = format!(
            "{}. [{} · 置信度 {}%] {}",
            i + 1,
            tag,
            (mem.confidence * 100.0) as u32,
            truncated
        );
        let line_tokens = approx_tokens(&line);
        if token_count + line_tokens > max_tokens {
            break;
        }
        token_count += line_tokens;
        lines.push(line);
    }

    lines.join("\n")
}
