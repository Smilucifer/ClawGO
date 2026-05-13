use crate::commands::embedding;
use crate::commands::vectorstore;
use crate::group_chat::memory_graph::graph_expand;
use crate::models::MemoryNode;
use crate::storage::characters;
use once_cell::sync::Lazy;
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
            return degraded_keyword_search(&entries, query, top_k);
        }
    };

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
                scored.push((node.clone(), 0.3 + keyword_boost * 0.3));
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

/// Format memories for system prompt injection, respecting token budget.
pub fn format_memory_injection(
    memories: &[MemoryNode],
    max_tokens: usize,
    max_tokens_per_memory: usize,
) -> String {
    if memories.is_empty() {
        return String::new();
    }

    let mut lines = vec!["[Character Memory — 相关记忆]".to_string()];
    let mut token_count = 0;
    let chars_per_token_approx = 4;

    for (i, mem) in memories.iter().enumerate() {
        let truncated: String = mem.content.chars()
            .take(max_tokens_per_memory * chars_per_token_approx)
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
        let line_tokens = line.len() / chars_per_token_approx;
        if token_count + line_tokens > max_tokens {
            break;
        }
        token_count += line_tokens;
        lines.push(line);
    }

    lines.join("\n")
}
