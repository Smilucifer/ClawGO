use chrono::Local;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use super::orchestrator::CommitteeResult;

// ---------------------------------------------------------------------------
// ArchivedDecision — query result for load_archive
// ---------------------------------------------------------------------------

/// A previously archived committee decision returned by [`load_archive`].
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedDecision {
    pub date: String,
    pub symbol: String,
    pub content: String,
}

// ---------------------------------------------------------------------------
// archive_decision_full (markdown + events.jsonl)
// ---------------------------------------------------------------------------
//
// NOTE: The old `archive_decision()` function that persisted to the verdicts
// table has been removed (Finding 11). The orchestrator now uses
// `committees::archive_verdict()` directly. Only `archive_decision_full()`
// remains — it writes the markdown report and events.jsonl entry.

/// Get the archive root directory: `~/.claw-go/invest/committee/`
fn archive_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claw-go")
        .join("invest")
        .join("committee")
}

/// Get the date-scoped archive directory: `~/.claw-go/invest/committee/{YYYY-MM-DD}/`
fn archive_date_dir() -> PathBuf {
    let today = Local::now().format("%Y-%m-%d").to_string();
    archive_root().join(today)
}

/// Validate that a symbol contains only safe filesystem characters.
fn validate_symbol(symbol: &str) -> Result<(), String> {
    if symbol.is_empty() {
        return Err("symbol must not be empty".into());
    }
    if !symbol
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return Err(format!("symbol contains unsafe characters: {symbol:?}"));
    }
    if symbol.contains("..") {
        return Err(format!("symbol must not contain '..': {symbol:?}"));
    }
    Ok(())
}

/// Archive a full committee decision: writes markdown report and appends to
/// `events.jsonl`. This is the high-fidelity archive path called from the
/// orchestrator after verdict persistence via `committees::archive_verdict()`.
pub fn archive_decision_full(
    symbol: &str,
    result: &CommitteeResult,
) -> Result<(), String> {
    validate_symbol(symbol)?;
    let dir = archive_date_dir();
    fs::create_dir_all(&dir)
        .map_err(|e| format!("create archive dir: {e}"))?;

    // ── Markdown report ──────────────────────────────────────────────────
    let md = format_decision_markdown(symbol, result);
    let md_path = dir.join(format!("{symbol}.md"));
    let mut md_file = fs::File::create(&md_path)
        .map_err(|e| format!("create md file: {e}"))?;
    md_file.write_all(md.as_bytes())
        .map_err(|e| format!("write md file: {e}"))?;

    // ── events.jsonl (daily-overwrite semantics, Finding 12 fix) ──────────
    // Read existing entries, filter out same symbol+date, then write back
    // filtered content + new entry. This matches the DB's daily-overwrite pattern.
    let jsonl_path = archive_root().join("events.jsonl");
    let today_str = Local::now().format("%Y-%m-%d").to_string();
    let event = serde_json::json!({
        "ts": Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
        "symbol": symbol,
        "verdict": result.final_verdict,
        "confidence": result.final_confidence,
        "macro_signal": result.macro_signal,
        "converged": result.converged,
        "rounds": result.rounds.len(),
        "tokens": result.total_tokens,
        "latency_ms": result.total_latency_ms,
        "has_sentinel_override": result.sentinel_override.is_some(),
        "sanity_gate1": result.sanity_check.gate1_pass,
        "sanity_gate2": result.sanity_check.gate2_pass,
        "sanity_gate3": result.sanity_check.gate3_pass,
    });

    // Read existing lines and filter out entries for the same symbol+date
    let mut filtered_lines: Vec<String> = Vec::new();
    if jsonl_path.exists() {
        if let Ok(content) = fs::read_to_string(&jsonl_path) {
            for line in content.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                    let entry_symbol = val.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                    let entry_ts = val.get("ts").and_then(|v| v.as_str()).unwrap_or("");
                    let entry_date = entry_ts.get(..10).unwrap_or("");
                    // Keep entries that don't match both symbol AND today's date
                    if !(entry_symbol == symbol && entry_date == today_str) {
                        filtered_lines.push(line.to_string());
                    }
                }
            }
        }
    }

    // Append the new entry
    let new_line = serde_json::to_string(&event)
        .map_err(|e| format!("serialize event: {e}"))?;
    filtered_lines.push(new_line);

    // Write all lines back
    let mut jsonl_file = fs::File::create(&jsonl_path)
        .map_err(|e| format!("create events.jsonl: {e}"))?;
    for line in &filtered_lines {
        jsonl_file
            .write_all(line.as_bytes())
            .map_err(|e| format!("write events.jsonl: {e}"))?;
        jsonl_file
            .write_all(b"\n")
            .map_err(|e| format!("write newline: {e}"))?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// format_decision_markdown
// ---------------------------------------------------------------------------

/// Format a [`CommitteeResult`] as a human-readable markdown decision report.
pub fn format_decision_markdown(
    symbol: &str,
    result: &CommitteeResult,
) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S");
    let mut md = String::new();

    // ── Title ────────────────────────────────────────────────────────────
    md.push_str(&format!("# {} Committee Decision Report\n\n", symbol));
    md.push_str(&format!("**Date:** {}\n\n", now));

    // ── Final Verdict & Confidence ───────────────────────────────────────
    md.push_str("## Final Verdict\n\n");
    md.push_str(&format!("| Field | Value |\n"));
    md.push_str(&format!("|-------|-------|\n"));
    md.push_str(&format!("| Verdict | **{}** |\n", result.final_verdict));
    md.push_str(&format!("| Confidence | **{:.1}%** |\n", result.final_confidence * 100.0));
    md.push_str(&format!("| Macro Signal | {} |\n", result.macro_signal));
    if let Some(strength) = result.macro_strength {
        md.push_str(&format!("| Macro Strength | {:.0}/10 |\n", strength));
    }
    md.push('\n');

    // ── Sanity Check Gates ───────────────────────────────────────────────
    md.push_str("## Sanity Check (3 Gates)\n\n");
    md.push_str(&format!("| Gate | Status |\n"));
    md.push_str(&format!("|------|--------|\n"));
    md.push_str(&format!(
        "| Gate 1 (Signal Consistency) | {} |\n",
        gate_label(result.sanity_check.gate1_pass)
    ));
    md.push_str(&format!(
        "| Gate 2 (Concentration) | {} |\n",
        gate_label(result.sanity_check.gate2_pass)
    ));
    md.push_str(&format!(
        "| Gate 3 (Dry Powder) | {} |\n",
        gate_label(result.sanity_check.gate3_pass)
    ));
    if !result.sanity_check.notes.is_empty() {
        md.push_str("\n**Notes:**\n");
        for note in &result.sanity_check.notes {
            md.push_str(&format!("- {}\n", note));
        }
    }
    md.push('\n');

    // ── Sentinel Override ────────────────────────────────────────────────
    if let Some(ref sentinel) = result.sentinel_override {
        md.push_str("## Sentinel Override\n\n");
        md.push_str(&format!("| Field | Value |\n"));
        md.push_str(&format!("|-------|-------|\n"));
        md.push_str(&format!("| Forced Verdict | **{}** |\n", sentinel.forced_verdict));
        md.push_str(&format!("| Forced Confidence | {:.1}% |\n", sentinel.forced_confidence * 100.0));
        md.push_str(&format!("| Reason | {} |\n", sentinel.reason));
        md.push('\n');
    }

    // ── Convergence Status ───────────────────────────────────────────────
    md.push_str("## Convergence\n\n");
    if result.converged {
        md.push_str("The committee **converged** on the final verdict.\n\n");
    } else {
        md.push_str("The committee **did not converge**; the verdict reflects the CIO's final judgment.\n\n");
    }

    // ── Round Outputs ────────────────────────────────────────────────────
    md.push_str("## Round Outputs\n\n");
    for ro in &result.rounds {
        md.push_str(&format!("### {} (Round {})\n\n", ro.label, ro.round));
        md.push_str(&format!("**Role:** {} | **Tokens:** {} | **Latency:** {}ms\n\n",
            ro.role.label(), ro.tokens_used, ro.latency_ms));
        md.push_str("```\n");
        md.push_str(&ro.parsed.raw_text);
        md.push_str("\n```\n\n");
    }

    // ── CIO Reasoning ────────────────────────────────────────────────────
    md.push_str("## CIO Reasoning\n\n");
    md.push_str(&result.reasoning);
    md.push_str("\n\n");

    // ── Footer ───────────────────────────────────────────────────────────
    md.push_str("---\n\n");
    md.push_str(&format!(
        "**Total Tokens:** {} | **Total Latency:** {}ms\n",
        result.total_tokens, result.total_latency_ms
    ));

    md
}

/// Render a gate pass/fail label.
fn gate_label(pass: bool) -> &'static str {
    if pass { "PASS" } else { "FAIL" }
}

// ---------------------------------------------------------------------------
// load_archive
// ---------------------------------------------------------------------------

/// Load archived committee decisions for `symbol` from the last `days` days.
/// Returns decisions in reverse chronological order (newest first).
pub fn load_archive(
    symbol: &str,
    days: i64,
) -> Result<Vec<ArchivedDecision>, String> {
    validate_symbol(symbol)?;
    let root = archive_root();
    if !root.exists() {
        return Ok(Vec::new());
    }

    let today = Local::now();
    let mut results: Vec<ArchivedDecision> = Vec::new();

    for offset in 0..days {
        let date = (today - chrono::Duration::days(offset))
            .format("%Y-%m-%d")
            .to_string();
        let dir = root.join(&date);
        let md_path = dir.join(format!("{symbol}.md"));

        if md_path.exists() {
            let content = fs::read_to_string(&md_path)
                .map_err(|e| format!("read {}: {e}", md_path.display()))?;
            results.push(ArchivedDecision {
                date,
                symbol: symbol.to_string(),
                content,
            });
        }
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::orchestrator::RoundOutputSummary;
    use super::super::analysis::{SanityCheckResult, SentinelOverride};
    use super::super::parser::ParsedFields;
    use super::super::roles::CommitteeRole;

    /// Helper to build a minimal CommitteeResult for testing.
    fn make_test_result() -> CommitteeResult {
        CommitteeResult {
            symbol: "TEST".to_string(),
            final_verdict: "HOLD".to_string(),
            final_confidence: 0.75,
            macro_signal: "risk_on".to_string(),
            macro_strength: None,
            reasoning: "CIO reasoning goes here.".to_string(),
            rounds: vec![
                RoundOutputSummary {
                    role: CommitteeRole::Macro,
                    round: 1,
                    label: "宏观分析师 R1".to_string(),
                    parsed: ParsedFields {
                        signal: Some("risk_on".to_string()),
                        strength: Some(7.0),
                        truncated: false,
                        raw_text: "Macro analysis text".to_string(),
                        ..Default::default()
                    },
                    latency_ms: 1200,
                    tokens_used: 350,
                },
            ],
            total_tokens: 1500,
            total_latency_ms: 8500,
            converged: true,
            sentinel_override: None,
            sanity_check: SanityCheckResult {
                gate1_pass: true,
                gate2_pass: true,
                gate3_pass: false,
                gate4_pass: true,
                final_verdict: "HOLD".to_string(),
                final_confidence: 0.75,
                notes: vec!["Gate 3 triggered: low dry powder".to_string()],
            },
        }
    }

    // archive_decision was removed (Finding 11); only archive_decision_full remains.

    #[test]
    fn test_format_markdown() {
        let result = make_test_result();
        let md = format_decision_markdown("TEST", &result);

        assert!(md.contains("# TEST Committee Decision Report"));
        assert!(md.contains("HOLD"));
        assert!(md.contains("75.0%"));
        assert!(md.contains("risk_on"));
        assert!(md.contains("PASS"));
        assert!(md.contains("FAIL")); // gate3
        assert!(md.contains("Gate 3 triggered"));
        assert!(md.contains("converged"));
        assert!(md.contains("宏观分析师 R1"));
        assert!(md.contains("Macro analysis text"));
        assert!(md.contains("CIO reasoning goes here."));
        assert!(md.contains("1500"));
        assert!(md.contains("8500ms"));
    }

    #[test]
    fn test_format_markdown_with_sentinel() {
        let mut result = make_test_result();
        result.sentinel_override = Some(SentinelOverride {
            reason: "Emergency risk event detected".to_string(),
            forced_verdict: "SELL".to_string(),
            forced_confidence: 0.95,
        });
        let md = format_decision_markdown("TEST", &result);
        assert!(md.contains("Sentinel Override"));
        assert!(md.contains("SELL"));
        assert!(md.contains("95.0%"));
        assert!(md.contains("Emergency risk event"));
    }

    #[test]
    fn test_format_markdown_not_converged() {
        let mut result = make_test_result();
        result.converged = false;
        let md = format_decision_markdown("TEST", &result);
        assert!(md.contains("did not converge"));
    }

    #[test]
    fn test_archive_dir_is_absolute() {
        let root = archive_root();
        assert!(root.is_absolute(), "archive root should be absolute: {root:?}");
        let date_dir = archive_date_dir();
        assert!(date_dir.is_absolute(), "archive date dir should be absolute: {date_dir:?}");
        // Should contain the expected path components
        let root_str = root.to_string_lossy();
        assert!(root_str.contains(".claw-go"), "should contain .claw-go: {root_str}");
        assert!(root_str.contains("committee"), "should contain committee: {root_str}");
    }

    #[test]
    fn test_load_archive_empty_dir() {
        // load_archive on a nonexistent dir should return empty, not error
        let result = load_archive("NONEXISTENT", 1);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
