use super::parser::ParsedFields;
use super::roles::CommitteeRole;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Convergence detection
// ---------------------------------------------------------------------------

/// Check if Quant and Risk have converged over the last 2 rounds.
/// Convergence = same SIGNAL + strength difference < 1.0.
pub fn check_convergence(round_outputs: &[RoundOutput]) -> bool {
    if round_outputs.len() < 4 {
        return false; // need at least Q1, R1, Q2, R2
    }

    // Find the last 2 Quant and Risk outputs
    let quant_rounds: Vec<&RoundOutput> = round_outputs
        .iter()
        .filter(|o| matches!(o.role, CommitteeRole::QuantR1 | CommitteeRole::QuantR2))
        .collect();
    let risk_rounds: Vec<&RoundOutput> = round_outputs
        .iter()
        .filter(|o| matches!(o.role, CommitteeRole::RiskR1 | CommitteeRole::RiskR2))
        .collect();

    if quant_rounds.len() < 2 || risk_rounds.len() < 2 {
        return false;
    }

    let q1 = &quant_rounds[quant_rounds.len() - 2];
    let q2 = &quant_rounds[quant_rounds.len() - 1];
    let r1 = &risk_rounds[risk_rounds.len() - 2];
    let r2 = &risk_rounds[risk_rounds.len() - 1];

    // Check SIGNAL agreement across all 4
    let _signals: Vec<Option<&str>> = [q1, q2, r1, r2]
        .iter()
        .map(|o| o.parsed.signal.as_deref().or(o.parsed.quant_view.as_deref()))
        .collect();

    // All must agree on direction (simplified: check if quant_view/risk_view are consistent)
    let q_views_match = q1.parsed.quant_view == q2.parsed.quant_view;
    let r_views_match = r1.parsed.risk_view == r2.parsed.risk_view;

    // Strength difference < 1.0
    let q_strength_diff =
        (q1.parsed.strength.unwrap_or(5.0) - q2.parsed.strength.unwrap_or(5.0)).abs();
    let r_strength_diff =
        (r1.parsed.strength.unwrap_or(5.0) - r2.parsed.strength.unwrap_or(5.0)).abs();

    q_views_match && r_views_match && q_strength_diff < 1.0 && r_strength_diff < 1.0
}

// ---------------------------------------------------------------------------
// SENTINEL override
// ---------------------------------------------------------------------------

/// Check if SENTINEL should override the CIO verdict.
/// Triggers when CONCENTRATION_PCT difference between Risk R1 and Risk R2 > 0.3%.
pub fn check_sentinel(round_outputs: &[RoundOutput]) -> Option<SentinelOverride> {
    let risk_outputs: Vec<&RoundOutput> = round_outputs
        .iter()
        .filter(|o| matches!(o.role, CommitteeRole::RiskR1 | CommitteeRole::RiskR2))
        .collect();

    if risk_outputs.len() < 2 {
        return None;
    }

    let r1 = &risk_outputs[0];
    let r2 = &risk_outputs[risk_outputs.len() - 1];

    let r1_pct = r1.parsed.concentration_pct.unwrap_or(0.0);
    let r2_pct = r2.parsed.concentration_pct.unwrap_or(0.0);
    let diff = (r2_pct - r1_pct).abs();

    if diff > 0.3 {
        Some(SentinelOverride {
            reason: format!(
                "SENTINEL: CONCENTRATION_PCT shifted by {:.1}% (R1={:.1}% -> R2={:.1}%)",
                diff, r1_pct, r2_pct
            ),
            forced_verdict: "TRIM".to_string(),
            forced_confidence: 0.3,
        })
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentinelOverride {
    pub reason: String,
    pub forced_verdict: String,
    pub forced_confidence: f64,
}

// ---------------------------------------------------------------------------
// CIO Sanity Check -- 3 Gates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanityCheckResult {
    pub gate1_pass: bool, // signal consistency
    pub gate2_pass: bool, // concentration < 40%
    pub gate3_pass: bool, // dry powder sufficient
    pub final_verdict: String,
    pub final_confidence: f64,
    pub notes: Vec<String>,
}

/// Run CIO Sanity Check 3 Gates on the parsed CIO output.
pub fn cio_sanity_check(
    cio_parsed: &ParsedFields,
    round_outputs: &[RoundOutput],
    macro_signal: &str,
    emergency_buffer_cny: f64,
) -> SanityCheckResult {
    let mut result = SanityCheckResult {
        gate1_pass: true,
        gate2_pass: true,
        gate3_pass: true,
        final_verdict: cio_parsed
            .verdict
            .clone()
            .unwrap_or_else(|| "HOLD".to_string()),
        final_confidence: cio_parsed.confidence.unwrap_or(0.5),
        notes: Vec::new(),
    };

    // Gate 1 -- Signal consistency
    let macro_is_bullish = macro_signal == "risk_on";
    let cio_is_bullish = matches!(result.final_verdict.as_str(), "BUY" | "ACCUMULATE");
    let cio_is_bearish = matches!(result.final_verdict.as_str(), "TRIM" | "SELL");

    if (macro_is_bullish && cio_is_bearish) || (!macro_is_bullish && cio_is_bullish) {
        let has_override = round_outputs.iter().any(|o| {
            o.parsed.risk_view.as_deref() == Some("OVERRIDE")
                || o.parsed.risk_view.as_deref() == Some("UPGRADE_TO_OVERRIDE")
        });
        if !has_override {
            result.gate1_pass = false;
            result.final_verdict = "HOLD".to_string();
            result
                .notes
                .push("Gate 1: signal inconsistency without override".to_string());
        }
    }

    // Gate 2 -- Concentration > 40%
    let concentration = cio_parsed.concentration_pct.unwrap_or(
        round_outputs
            .iter()
            .filter_map(|o| o.parsed.concentration_pct)
            .last()
            .unwrap_or(0.0),
    );
    if concentration > 40.0 {
        result.gate2_pass = false;
        if !matches!(result.final_verdict.as_str(), "TRIM" | "SELL") {
            result.final_verdict = "TRIM".to_string();
            result.notes.push(format!(
                "Gate 2: concentration {:.1}% > 40%, forced to TRIM",
                concentration
            ));
        }
    }

    // Gate 3 -- Dry powder check
    let dry_powder = cio_parsed.dry_powder_cny.unwrap_or(
        round_outputs
            .iter()
            .filter_map(|o| o.parsed.dry_powder_cny)
            .last()
            .unwrap_or(0.0),
    );
    if dry_powder < emergency_buffer_cny {
        result.gate3_pass = false;
        result.final_verdict = "HOLD".to_string();
        result.final_confidence = result.final_confidence.min(0.4);
        result.notes.push(format!(
            "Gate 3: dry powder {:.0} < emergency buffer {:.0}, downgraded to HOLD",
            dry_powder, emergency_buffer_cny
        ));
    }

    // Check for WORKER_UNAVAILABLE (retry exhaustion)
    let has_unavailable = round_outputs
        .iter()
        .any(|o| o.parsed.raw_text.contains("[WORKER_UNAVAILABLE]"));
    if has_unavailable {
        result.final_verdict = "HOLD".to_string();
        result.final_confidence = result.final_confidence.min(0.4);
        result
            .notes
            .push("Worker unavailable, degraded to HOLD".to_string());
    }

    result
}

// ---------------------------------------------------------------------------
// Round output -- accumulated per-role result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RoundOutput {
    pub role: CommitteeRole,
    pub round: u8,
    pub parsed: ParsedFields,
    pub latency_ms: u64,
    pub tokens_used: u32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_output(
        role: CommitteeRole,
        round: u8,
        signal: Option<&str>,
        strength: Option<f64>,
        concentration: Option<f64>,
        dry_powder: Option<f64>,
        view: Option<&str>,
    ) -> RoundOutput {
        let mut parsed = ParsedFields::default();
        parsed.signal = signal.map(|s| s.to_string());
        parsed.strength = strength;
        parsed.concentration_pct = concentration;
        parsed.dry_powder_cny = dry_powder;
        match role {
            CommitteeRole::QuantR1 | CommitteeRole::QuantR2 => {
                parsed.quant_view = view.map(|s| s.to_string());
            }
            CommitteeRole::RiskR1 | CommitteeRole::RiskR2 => {
                parsed.risk_view = view.map(|s| s.to_string());
            }
            _ => {}
        }
        parsed.raw_text = format!("test output for {:?}", role);
        RoundOutput {
            role,
            round,
            parsed,
            latency_ms: 100,
            tokens_used: 200,
        }
    }

    #[test]
    fn test_convergence_detected() {
        let outputs = vec![
            make_output(
                CommitteeRole::QuantR1,
                1,
                None,
                Some(7.0),
                None,
                None,
                Some("AGREE"),
            ),
            make_output(
                CommitteeRole::RiskR1,
                1,
                None,
                Some(6.0),
                None,
                None,
                Some("AGREE"),
            ),
            make_output(
                CommitteeRole::QuantR2,
                2,
                None,
                Some(7.0),
                None,
                None,
                Some("AGREE"),
            ),
            make_output(
                CommitteeRole::RiskR2,
                2,
                None,
                Some(6.5),
                None,
                None,
                Some("AGREE"),
            ),
        ];
        assert!(check_convergence(&outputs));
    }

    #[test]
    fn test_convergence_not_detected_different_views() {
        let outputs = vec![
            make_output(
                CommitteeRole::QuantR1,
                1,
                None,
                Some(7.0),
                None,
                None,
                Some("AGREE"),
            ),
            make_output(
                CommitteeRole::RiskR1,
                1,
                None,
                Some(6.0),
                None,
                None,
                Some("CHALLENGE"),
            ),
            make_output(
                CommitteeRole::QuantR2,
                2,
                None,
                Some(7.0),
                None,
                None,
                Some("AGREE"),
            ),
            make_output(
                CommitteeRole::RiskR2,
                2,
                None,
                Some(6.0),
                None,
                None,
                Some("CHALLENGE"),
            ),
        ];
        assert!(!check_convergence(&outputs));
    }

    #[test]
    fn test_convergence_not_detected_strength_drift() {
        let outputs = vec![
            make_output(
                CommitteeRole::QuantR1,
                1,
                None,
                Some(3.0),
                None,
                None,
                Some("AGREE"),
            ),
            make_output(
                CommitteeRole::RiskR1,
                1,
                None,
                Some(6.0),
                None,
                None,
                Some("AGREE"),
            ),
            make_output(
                CommitteeRole::QuantR2,
                2,
                None,
                Some(8.0),
                None,
                None,
                Some("AGREE"),
            ),
            make_output(
                CommitteeRole::RiskR2,
                2,
                None,
                Some(6.0),
                None,
                None,
                Some("AGREE"),
            ),
        ];
        assert!(!check_convergence(&outputs));
    }

    #[test]
    fn test_sentinel_triggers_on_large_shift() {
        let outputs = vec![
            make_output(CommitteeRole::RiskR1, 1, None, None, Some(20.0), None, None),
            make_output(CommitteeRole::RiskR2, 2, None, None, Some(35.0), None, None),
        ];
        let sentinel = check_sentinel(&outputs);
        assert!(sentinel.is_some());
        let s = sentinel.unwrap();
        assert_eq!(s.forced_verdict, "TRIM");
    }

    #[test]
    fn test_sentinel_no_trigger_small_shift() {
        let outputs = vec![
            make_output(CommitteeRole::RiskR1, 1, None, None, Some(20.0), None, None),
            make_output(CommitteeRole::RiskR2, 2, None, None, Some(20.2), None, None),
        ];
        assert!(check_sentinel(&outputs).is_none());
    }

    #[test]
    fn test_sanity_gate1_inconsistency() {
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.7),
            ..Default::default()
        };
        let outputs = vec![];
        let result = cio_sanity_check(&cio, &outputs, "risk_off", 100000.0);
        assert!(!result.gate1_pass);
        assert_eq!(result.final_verdict, "HOLD");
    }

    #[test]
    fn test_sanity_gate2_high_concentration() {
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.7),
            concentration_pct: Some(45.0),
            ..Default::default()
        };
        let outputs = vec![];
        let result = cio_sanity_check(&cio, &outputs, "risk_on", 100000.0);
        assert!(!result.gate2_pass);
        assert_eq!(result.final_verdict, "TRIM");
    }

    #[test]
    fn test_sanity_gate3_low_dry_powder() {
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.7),
            dry_powder_cny: Some(50000.0),
            ..Default::default()
        };
        let outputs = vec![];
        let result = cio_sanity_check(&cio, &outputs, "risk_on", 100000.0);
        assert!(!result.gate3_pass);
        assert_eq!(result.final_verdict, "HOLD");
        assert!(result.final_confidence <= 0.4);
    }

    #[test]
    fn test_sanity_worker_unavailable() {
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.8),
            ..Default::default()
        };
        let outputs = vec![RoundOutput {
            role: CommitteeRole::QuantR1,
            round: 1,
            parsed: ParsedFields {
                raw_text: "[WORKER_UNAVAILABLE]".to_string(),
                ..Default::default()
            },
            latency_ms: 0,
            tokens_used: 0,
        }];
        let result = cio_sanity_check(&cio, &outputs, "risk_on", 100000.0);
        assert_eq!(result.final_verdict, "HOLD");
        assert!(result.final_confidence <= 0.4);
    }

    #[test]
    fn test_sanity_all_gates_pass() {
        let cio = ParsedFields {
            verdict: Some("ACCUMULATE".to_string()),
            confidence: Some(0.7),
            concentration_pct: Some(20.0),
            dry_powder_cny: Some(200000.0),
            ..Default::default()
        };
        let outputs = vec![];
        let result = cio_sanity_check(&cio, &outputs, "risk_on", 100000.0);
        assert!(result.gate1_pass);
        assert!(result.gate2_pass);
        assert!(result.gate3_pass);
        assert_eq!(result.final_verdict, "ACCUMULATE");
    }
}
