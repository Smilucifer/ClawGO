//! Jin10 (金十数据) high-frequency news collector.
//!
//! Collects A-share related flash news from Jin10 API every 15 seconds,
//! writes directly to events table with `analyzed=false`.
//! The event_analyzer task later updates severity/stance via LLM.

use std::collections::HashSet;
use std::sync::Mutex;

use crate::invest::event_scanner::{JIN10_COUNT, format_provider_timestamp};
use crate::invest::international::InternationalClient;
use crate::storage::invest::events::{Event, save_event};

/// In-memory dedup set to avoid duplicate inserts within a session.
static SEEN_IDS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Initialize the seen IDs set (call once at startup).
fn ensure_seen_ids() -> std::sync::MutexGuard<'static, Option<HashSet<String>>> {
    let mut guard = SEEN_IDS.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(HashSet::new());
    }
    guard
}

/// Clear old seen IDs to prevent memory leak (keep last 2000 when over 5000).
fn cleanup_seen_ids(guard: &mut HashSet<String>) {
    if guard.len() > 5000 {
        let drain_count = guard.len() - 2000;
        let to_remove: Vec<String> = guard.iter().take(drain_count).cloned().collect();
        for id in to_remove {
            guard.remove(&id);
        }
    }
}

/// Result of a single collection run.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorResult {
    pub fetched: usize,
    pub new_saved: usize,
    pub duplicates_skipped: usize,
    pub errors: Vec<String>,
}

/// Run a single collection cycle: fetch Jin10 A-share news, dedup, save to events table.
pub async fn collect_jin10_news() -> Result<CollectorResult, String> {
    let client = InternationalClient::from_settings();

    // Fetch A-share channel news
    let items = client.fetch_jinshi_a_share_news(JIN10_COUNT).await;

    let fetched = items.len();
    let mut new_saved = 0usize;
    let mut duplicates_skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();

    // Pre-compute fallback timestamp once (not per-item)
    let now_fallback = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();

    // Get or init seen IDs
    let mut guard = ensure_seen_ids();
    let seen = guard.as_mut().unwrap();

    for item in items {
        // In-memory dedup
        if seen.contains(&item.uuid) {
            duplicates_skipped += 1;
            continue;
        }
        seen.insert(item.uuid.clone());

        // Create event with analyzed=false
        let event = Event {
            id: item.uuid.clone(),
            source: "jinshi_flash".to_string(),
            event_type: "news".to_string(),
            title: item.title.clone(),
            body: Some(format!("Publisher: {}", item.publisher)),
            symbols: None,
            severity: "pending".to_string(),
            stance: "pending".to_string(),
            triggered: false,
            trigger_verdict_id: None,
            created_at: format_provider_timestamp(item.provider_publish_time, &now_fallback),
            analyzed: false,
            analyzed_at: None,
            channels: "[]".to_string(),
        };

        match save_event(&event) {
            Ok(()) => {
                new_saved += 1;
            }
            Err(e) => {
                // UNIQUE constraint violation is expected (dedup from DB)
                if e.contains("UNIQUE") {
                    duplicates_skipped += 1;
                } else {
                    errors.push(format!("save event '{}': {}", item.title, e));
                }
            }
        }
    }

    // Cleanup old seen IDs
    cleanup_seen_ids(seen);

    Ok(CollectorResult {
        fetched,
        new_saved,
        duplicates_skipped,
        errors,
    })
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_provider_timestamp() {
        use crate::invest::event_scanner::format_provider_timestamp;
        // Valid timestamp
        let ts = 1781109420;
        let result = format_provider_timestamp(ts, "fallback");
        assert!(result.contains("2026"));

        // Zero timestamp → fallback
        let result = format_provider_timestamp(0, "fallback");
        assert_eq!(result, "fallback");
    }

    #[test]
    fn test_cleanup_seen_ids() {
        let mut set = HashSet::new();
        for i in 0..6000 {
            set.insert(format!("id_{}", i));
        }
        assert_eq!(set.len(), 6000);

        cleanup_seen_ids(&mut set);
        assert!(set.len() <= 3000);
    }
}
