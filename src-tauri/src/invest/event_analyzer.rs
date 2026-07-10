//! Event analyzer: batch-processes unanalyzed events via LLM.
//!
//! Scheduled on trading-day market hours (see `event_analyzer` cron in
//! `scheduler::default_jobs`), queries events with `analyzed=false`,
//! normalizes them via LLM, and updates severity/stance/symbols fields.

use crate::invest::event_scanner::{
    NormalizedEvent, RawEvent, Severity, build_normalizer_prompt_with_sectors,
    fallback_normalize_from, normalize_events, parse_normalized_response, short, wrap_csv,
};
use crate::storage::invest::events::{Event, list_unanalyzed_events, update_event_analysis};
use crate::storage::invest::sentiment::{list_unanalyzed_sentiment, update_sentiment_analysis};
use crate::storage::invest::stock_industry::all_industries;

/// Max events to analyze in a single batch.
const MAX_BATCH_SIZE: i64 = 50;

/// Run a single text-completion via the committee CLI executor.
///
/// Used by event scanner/analyzer normalization in place of `OpenAiCompatClient`.
/// Routes through the committee-configured provider (`committee_tuning.json` →
/// `selected_provider`/`model`) so scheduled normalization uses the same LLM the
/// user picked for the committee (e.g. MiMo Plan) rather than silently burning
/// the default Claude subscription. When the committee provider is `"default"`
/// (native CC), `resolve_settings_path` returns `None` and this falls back to the
/// ambient `~/.claude` settings — same as before for default users.
pub async fn cli_complete(
    system_prompt: &str,
    user_message: &str,
) -> Result<String, String> {
    let settings = crate::invest::macro_verdict::resolve_settings_path();
    let exec = crate::invest::committee::cli_executor::CliCommitteeExecutor::global()
        .ok_or("claude CLI not available")?;
    exec.run_role(system_prompt, user_message, 0, settings.as_deref(), None).await
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

/// Which pending table to normalize.
#[derive(Debug, Clone, Copy)]
pub enum AnalyzeTable {
    /// `events` table (Jin10 flash / announcements / AkShare per-stock news).
    Events,
    /// `sentiment_items` table (Xueqiu / EastMoney / self-media).
    Sentiment,
}


/// Generic normalization entry point. Builds a closed-set sectors prompt
/// from `stock_industry.all_industries()` and dispatches to the
/// table-specific analyzer.
pub async fn analyze_pending(
    table: AnalyzeTable,
    language: &str,
) -> Result<AnalyzerResult, String> {
    let industries = all_industries().unwrap_or_default();
    let prompt = build_normalizer_prompt_with_sectors(language, &industries);

    match table {
        AnalyzeTable::Events => analyze_events_table(&prompt).await,
        AnalyzeTable::Sentiment => analyze_sentiment_table(&prompt).await,
    }
}

/// Run analysis on unanalyzed events.
///
/// Events classified as LOW by the LLM keep their original severity
/// (preserving jin10 collector events that haven't been keyword-filtered).
async fn analyze_events_table(prompt: &str) -> Result<AnalyzerResult, String> {
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

    let normalized = normalize_events_batch(&pending, prompt).await;

    for (event, norm) in pending.iter().zip(normalized.iter()) {
        if norm.severity == Severity::Low {
            // Keep original severity — preserves jin10 events not keyword-filtered
            log::debug!(
                "  [keep] '{}' — LLM classified as LOW, preserving original '{}'",
                short(&event.title),
                event.severity
            );
            skipped += 1;
            let _ = update_event_analysis(
                &event.id,
                &event.severity,
                &norm.stance,
                None,
                norm.summary_opt(),
                wrap_csv(&norm.sectors).as_deref(),
                wrap_csv(&norm.topics).as_deref(),
            );
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
            norm.summary_opt(),
            wrap_csv(&norm.sectors).as_deref(),
            wrap_csv(&norm.topics).as_deref(),
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

/// Run analysis on unanalyzed sentiment_items.
///
/// Converts each SentimentItem to a RawEvent, reuses `normalize_events`
/// (the same LLM pipeline as event_scanner), then writes normalized
/// fields back through `update_sentiment_analysis`.
async fn analyze_sentiment_table(prompt: &str) -> Result<AnalyzerResult, String> {
    let pending = list_unanalyzed_sentiment(Some(MAX_BATCH_SIZE))?;
    if pending.is_empty() {
        return Ok(AnalyzerResult {
            total_pending: 0,
            analyzed: 0,
            skipped: 0,
            errors: vec![],
        });
    }
    let total_pending = pending.len();
    log::info!(
        "Found {} unanalyzed sentiment items, starting analysis",
        total_pending
    );

    let raws: Vec<RawEvent> = pending
        .iter()
        .map(|it| RawEvent {
            source: format!("sentiment:{}", it.provider),
            event_type: it.source_type.clone(),
            title: it.title.clone(),
            body: it.summary.clone().unwrap_or_else(|| it.title.clone()),
            url: it.url.clone(),
            created_at: it.created_at.clone(),
        })
        .collect();
    let normalized = normalize_events(&raws, prompt).await;

    let mut analyzed = 0usize;
    let mut errors: Vec<String> = Vec::new();
    for (item, norm) in pending.iter().zip(normalized.iter()) {
        match update_sentiment_analysis(
            &item.id,
            norm.summary_opt(),
            norm.severity.as_str(),
            &norm.stance,
            wrap_csv(&norm.affected_symbols).as_deref(),
            wrap_csv(&norm.sectors).as_deref(),
            wrap_csv(&norm.topics).as_deref(),
        ) {
            Ok(()) => analyzed += 1,
            Err(e) => errors.push(format!("update {}: {}", item.id, e)),
        }
    }

    log::info!(
        "Sentiment analysis complete: {} analyzed, {} errors",
        analyzed,
        errors.len()
    );

    Ok(AnalyzerResult {
        total_pending,
        analyzed,
        skipped: 0,
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
            summary: None,
            sectors: None,
            topics: None,
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
