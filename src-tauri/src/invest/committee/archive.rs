use chrono::Local;

/// Archive a committee decision to the local invest database.
/// This is fire-and-forget; errors are logged but not propagated by the caller.
pub async fn archive_decision(
    symbol: &str,
    verdict: &str,
    confidence: f64,
    macro_signal: Option<&str>,
    macro_strength: Option<f64>,
    reasoning: &str,
    model: &str,
    provider: &str,
    tokens_used: u32,
    latency_ms: u64,
) -> Result<(), String> {
    use crate::storage::invest::verdicts::{save_verdict, Verdict};

    let id = format!(
        "{}_{}",
        symbol,
        Local::now().format("%Y%m%d%H%M%S%.3f")
    );

    let v = Verdict {
        id,
        symbol: symbol.to_string(),
        verdict: verdict.to_string(),
        confidence: Some(confidence),
        macro_signal: macro_signal.map(|s| s.to_string()),
        macro_strength,
        reasoning: Some(reasoning.to_string()),
        model: Some(model.to_string()),
        provider: Some(provider.to_string()),
        tokens_used: Some(tokens_used as i64),
        latency_ms: Some(latency_ms as i64),
        created_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };

    save_verdict(&v)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_archive_decision_signature_compiles() {
        // Verify the function signature matches what the orchestrator expects.
        // Actual DB tests require a running invest.db — tested via integration.
        let _f = archive_decision;
        let _: fn(
            &str,
            &str,
            f64,
            Option<&str>,
            Option<f64>,
            &str,
            &str,
            &str,
            u32,
            u64,
        ) -> _ = archive_decision;
    }
}
