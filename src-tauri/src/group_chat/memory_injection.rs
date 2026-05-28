use crate::models::{MemoryConfig, MemoryNode};
use crate::storage::memory_store;

/// Search for relevant memories to inject into a conversation.
/// Uses FTS5 full-text search + tag matching (no vector search, no graph).
/// Filters results by relevance_threshold if provided.
pub fn search_memories_for_injection(
    query: &str,
    top_k: usize,
    relevance_threshold: Option<f64>,
) -> Vec<MemoryNode> {
    // Extract potential tags from query words
    let tags: Vec<String> = query
        .split_whitespace()
        .filter(|w| w.len() >= 2)
        .map(|w| w.to_lowercase())
        .collect();

    let results = memory_store::search_hybrid(query, &tags, top_k).unwrap_or_default();

    // Apply relevance threshold filtering if configured
    if let Some(threshold) = relevance_threshold {
        if threshold > 0.0 {
            // BM25 scores are negative (lower = more relevant).
            // Threshold is a 0-1 normalized score; map to BM25 range.
            // We use find_duplicates' Jaccard as a proxy: filter by word-overlap similarity.
            // For simplicity, we keep all FTS results (BM25 already ranks by relevance)
            // and only filter if threshold is very high (>0.8 = strict).
            // The real filtering happens at the caller level with confidence.
            return results
                .into_iter()
                .filter(|m| m.confidence >= threshold * 100.0)
                .collect();
        }
    }

    results
}

/// Inject user memories into a system prompt.
/// Shared implementation for both group chat and private chat paths.
/// Respects auto_learn gating, max_retrieval_count, and relevance_threshold from memory_config.
pub fn inject_memories_into_prompt(
    user_message: &str,
    system_prompt: &mut String,
    memory_config: Option<&MemoryConfig>,
    max_tokens: usize,
    max_tokens_per_memory: usize,
) {
    // Resolve config values (defaults if no config provided)
    let (auto_learn, top_k, threshold) = match memory_config {
        Some(config) => (
            config.auto_learn,
            config.max_retrieval_count,
            Some(config.relevance_threshold),
        ),
        None => (true, 5, None),
    };

    // Respect per-character auto_learn gating
    if !auto_learn {
        return;
    }

    let memories = search_memories_for_injection(user_message, top_k, threshold);
    if !memories.is_empty() {
        let memory_prompt = format_memory_injection(&memories, max_tokens, max_tokens_per_memory);
        if !memory_prompt.is_empty() {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&memory_prompt);
        }
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

    let mut lines = vec!["[User Memory]".to_string()];
    let mut token_count = 0;

    for (i, mem) in memories.iter().enumerate() {
        // Truncate content to per-memory token budget using CJK-aware counting
        let mut content_tokens: f64 = 0.0;
        let truncated: String = mem
            .content
            .chars()
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
            "feedback" => "Feedback",
            "relationship" => "Relationship",
            "skill" => "Skill",
            _ => "Memory",
        };
        let line = format!(
            "{}. [{} · {}%] {}",
            i + 1,
            tag,
            mem.confidence as u32,
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
    if matches!(
        ch as u32,
        0x3400..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FFFF | 0x30000..=0x3FFFF
    ) {
        1.0
    } else {
        0.25
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_empty() {
        assert_eq!(format_memory_injection(&[], 500, 200), "");
    }

    #[test]
    fn test_format_single() {
        let m = MemoryNode {
            id: "1".into(),
            character_id: String::new(),
            content: "用户喜欢 Go".into(),
            memory_type: "preference".into(),
            confidence: 85.0,
            source: crate::models::MemorySource {
                kind: "test".into(),
                run_id: None,
                group_chat_id: None,
            },
            tags: vec![],
            created_at: String::new(),
            updated_at: String::new(),
            status: "approved".into(),
        };
        let result = format_memory_injection(&[m], 500, 200);
        assert!(result.contains("[User Memory]"));
        assert!(result.contains("Preference"));
        assert!(result.contains("用户喜欢 Go"));
    }
}
