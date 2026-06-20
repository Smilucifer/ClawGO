//! Event analyzer: batch-processes unanalyzed events via LLM.
//!
//! Runs every 10 minutes, queries events with `analyzed=false`,
//! normalizes them via LLM, and updates severity/stance/symbols fields.

use crate::invest::event_scanner::{
    NormalizedEvent, Severity, default_normalizer_prompt,
    fallback_normalize_from, parse_normalized_response, short,
};
use crate::storage::invest::events::{Event, list_unanalyzed_events, update_event_analysis};

/// Max events to analyze in a single batch.
const MAX_BATCH_SIZE: i64 = 50;

/// Run a single text-completion via the committee CLI executor.
///
/// Used by event scanner/analyzer normalization in place of `OpenAiCompatClient`.
/// `settings_path: None` means use the ambient `~/.claude` settings — committee
/// roles use `write_committee_settings_json` for platform_credentials, but event
/// normalization is fine on the default Claude provider.
pub async fn cli_complete(
    system_prompt: &str,
    user_message: &str,
) -> Result<String, String> {
    let exec = crate::invest::committee::cli_executor::CliCommitteeExecutor::global()
        .ok_or("claude CLI not available")?;
    exec.run_role(system_prompt, user_message, 0, None).await
}

/// Result of an analysis run.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzerResult {
    pub total_pending: usize,
    pub analyzed: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

/// Run analysis on unanalyzed events.
/// Queries pending events, normalizes via LLM, updates DB.
///
/// Events classified as LOW by the LLM keep their original severity
/// (preserving jin10 collector events that haven't been keyword-filtered).
pub async fn analyze_pending_events(
    normalizer_prompt: Option<&str>,
    language: &str,
) -> Result<AnalyzerResult, String> {
    // Query unanalyzed events
    let pending = list_unanalyzed_events(Some(MAX_BATCH_SIZE))?;

    if pending.is_empty() {
        return Ok(AnalyzerResult {
            total_pending: 0,
            analyzed: 0,
            skipped: 0,
            errors: vec![],
        });
    }

    let total_pending = pending.len();
    log::info!("Found {} unanalyzed events, starting analysis", total_pending);

    let mut analyzed = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();

    // Build batch prompt for LLM
    let effective_prompt = normalizer_prompt.unwrap_or_else(|| default_normalizer_prompt(language));
    let normalized = normalize_events_batch(&pending, effective_prompt).await;

    // Update each event
    for (event, norm) in pending.iter().zip(normalized.iter()) {
        if norm.severity == Severity::Low {
            // Keep original severity — preserves jin10 events not keyword-filtered
            log::debug!("  [keep] '{}' — LLM classified as LOW, preserving original '{}'", short(&event.title), event.severity);
            skipped += 1;
            let _ = update_event_analysis(&event.id, &event.severity, &norm.stance, None);
            continue;
        }

        let symbols_str = if norm.affected_symbols.is_empty() {
            None
        } else {
            Some(norm.affected_symbols.join(","))
        };

        match update_event_analysis(
            &event.id,
            norm.severity.as_str(),
            &norm.stance,
            symbols_str.as_deref(),
        ) {
            Ok(()) => {
                analyzed += 1;
                log::debug!(
                    "  [analyzed] '{}' => severity={}, stance={}",
                    short(&event.title),
                    norm.severity.as_str(),
                    norm.stance
                );
            }
            Err(e) => {
                errors.push(format!("update '{}': {}", event.title, e));
            }
        }
    }

    log::info!(
        "Analysis complete: {} analyzed, {} skipped, {} errors",
        analyzed,
        skipped,
        errors.len()
    );

    Ok(AnalyzerResult {
        total_pending,
        analyzed,
        skipped,
        errors,
    })
}

/// Normalize a batch of events using the committee CLI executor.
async fn normalize_events_batch(
    events: &[Event],
    system_prompt: &str,
) -> Vec<NormalizedEvent> {
    if events.is_empty() {
        return vec![];
    }

    // Build batch prompt
    let mut items = String::new();
    for (i, ev) in events.iter().enumerate() {
        let body = ev.body.as_deref().unwrap_or(&ev.title);
        items.push_str(&format!(
            "\n[{}] source={} type={} title={}\n{}\n",
            i + 1,
            ev.source,
            ev.event_type,
            ev.title,
            body
        ));
    }

    // Call CLI
    let content = match cli_complete(system_prompt, &items).await {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Event analyzer CLI call failed: {}, falling back to rule-based", e);
            return events.iter().map(|ev| {
                let body = ev.body.as_deref().unwrap_or(&ev.title);
                fallback_normalize_from(&ev.title, body)
            }).collect();
        }
    };

    // Parse JSON response using shared generic parser
    parse_normalized_response(&content, events, |ev| {
        let body = ev.body.as_deref().unwrap_or(&ev.title);
        fallback_normalize_from(&ev.title, body)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_normalize() {
        let event = Event {
            id: "test".to_string(),
            source: "test".to_string(),
            event_type: "news".to_string(),
            title: "央行宣布降准50个基点".to_string(),
            body: Some("央行宣布降准50个基点".to_string()),
            symbols: None,
            severity: "pending".to_string(),
            stance: "pending".to_string(),
            triggered: false,
            trigger_verdict_id: None,
            created_at: "2026-06-11T00:00:00".to_string(),
            analyzed: false,
            analyzed_at: None,
            channels: "[]".to_string(),
        };

        let body = event.body.as_deref().unwrap_or(&event.title);
        let norm = fallback_normalize_from(&event.title, body);
        assert_eq!(norm.severity, Severity::High);
        assert_eq!(norm.stance, "neutral");
    }

    #[test]
    fn test_short() {
        let s = "这是一个测试标题，用于验证截断功能是否正常工作";
        let result = short(s);
        assert!(result.len() <= 40);
    }
}
