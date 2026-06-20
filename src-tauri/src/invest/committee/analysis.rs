use super::orchestrator::Mode;
use super::parser::ParsedFields;
use super::roles::CommitteeRole;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Convergence detection
// ---------------------------------------------------------------------------

/// Check if Quant and Risk have converged across rounds.
/// Convergence = same signal + strength difference < 1.0 for both roles.
pub fn check_convergence(round_outputs: &[RoundOutput]) -> bool {
    // Collect Quant and Risk outputs
    let quant_rounds: Vec<&RoundOutput> = round_outputs
        .iter()
        .filter(|o| o.role == CommitteeRole::Quant)
        .collect();
    let risk_rounds: Vec<&RoundOutput> = round_outputs
        .iter()
        .filter(|o| o.role == CommitteeRole::Risk)
        .collect();

    if quant_rounds.len() < 2 || risk_rounds.len() < 2 {
        return false; // need at least Q_R1, R_R1, Q_R2, R_R2
    }

    let q1 = &quant_rounds[quant_rounds.len() - 2];
    let q2 = &quant_rounds[quant_rounds.len() - 1];
    let r1 = &risk_rounds[risk_rounds.len() - 2];
    let r2 = &risk_rounds[risk_rounds.len() - 1];

    // All must agree on signal direction — require both signals to be present and match.
    // None == None is NOT agreement (missing data != consensus).
    let q_signals_match = match (&q1.parsed.signal, &q2.parsed.signal) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };
    let r_signals_match = match (&r1.parsed.signal, &r2.parsed.signal) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };

    // Strength difference < 1.0 — require both values present.
    let q_strength_diff = match (q1.parsed.strength, q2.parsed.strength) {
        (Some(a), Some(b)) => (a - b).abs(),
        _ => f64::MAX, // missing strength = no convergence
    };
    let r_strength_diff = match (r1.parsed.strength, r2.parsed.strength) {
        (Some(a), Some(b)) => (a - b).abs(),
        _ => f64::MAX,
    };

    q_signals_match && r_signals_match && q_strength_diff < 1.0 && r_strength_diff < 1.0
}

// ---------------------------------------------------------------------------
// SENTINEL override
// ---------------------------------------------------------------------------

/// Check if SENTINEL should override the CIO verdict.
/// Triggers when CONCENTRATION_PCT difference between Risk rounds > 0.3%.
pub fn check_sentinel(round_outputs: &[RoundOutput]) -> Option<SentinelOverride> {
    let risk_outputs: Vec<&RoundOutput> = round_outputs
        .iter()
        .filter(|o| o.role == CommitteeRole::Risk)
        .collect();

    if risk_outputs.len() < 2 {
        return None;
    }

    // Guard: need actual concentration data from first Risk output to assess
    let r1_pct = match risk_outputs[0].parsed.concentration_pct {
        Some(v) => v,
        None => return None, // can't assess without baseline
    };

    // Track max concentration shift across ALL risk outputs (not just first vs last)
    let mut max_diff: f64 = 0.0;
    let mut max_pct = r1_pct;
    for ro in risk_outputs.iter().skip(1) {
        if let Some(pct) = ro.parsed.concentration_pct {
            let diff = (pct - r1_pct).abs();
            if diff > max_diff {
                max_diff = diff;
                max_pct = pct;
            }
        }
    }

    if max_diff > 0.3 {
        Some(SentinelOverride {
            reason: format!(
                "SENTINEL: CONCENTRATION_PCT shifted by {:.1}% (R1={:.1}% -> peak {:.1}%)",
                max_diff, r1_pct, max_pct
            ),
            forced_verdict: "TRIM".to_string(),
            forced_confidence: 0.3,
        })
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentinelOverride {
    pub reason: String,
    pub forced_verdict: String,
    pub forced_confidence: f64,
}

// ---------------------------------------------------------------------------
// CIO Sanity Check -- 2 Gates + fallback
// ---------------------------------------------------------------------------

/// 硬 fallback = 工作节点真不可用/输出真空，必须降级 HOLD。
/// 软 fallback（missing_critical_fields 等）= 有原文仅缺结构化字段，不全局降级。
/// 口径须与前端 CommitteeLiveTab.svelte 的 HARD_FALLBACKS 一致。
fn is_hard_fallback(reason: &str) -> bool {
    matches!(reason, "worker_unavailable" | "empty_text" | "cli_executor_none")
        || reason.starts_with("cli_error")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SanityCheckResult {
    pub gate1_pass: bool, // signal consistency
    pub gate2_pass: bool, // triple deterioration guard clause
    pub final_verdict: String,
    pub final_confidence: f64,
    pub notes: Vec<String>,
}

/// Run CIO Sanity Check on the parsed CIO output.
///
/// Gates:
/// - G1: Signal consistency (macro vs CIO direction)
/// - G2: Triple deterioration guard clause (macro risk_off + quant bearish + deep loss → SELL)
/// - Fallback: WORKER_UNAVAILABLE / fallback_reason → HOLD
pub fn cio_sanity_check(
    cio_parsed: &ParsedFields,
    round_outputs: &[RoundOutput],
    macro_signal: &str,
    macro_strength: Option<f64>,
    mode: Mode,
) -> SanityCheckResult {
    let mut result = SanityCheckResult {
        gate1_pass: true,
        gate2_pass: true,
        final_verdict: cio_parsed
            .verdict
            .clone()
            .unwrap_or_else(|| "HOLD".to_string()),
        final_confidence: cio_parsed.confidence.unwrap_or(0.5),
        notes: Vec::new(),
    };

    // Gate 1 -- Signal consistency
    let macro_is_bullish = macro_signal == "risk_on";
    let macro_is_risk_off = macro_signal == "risk_off";
    let cio_is_bullish = matches!(result.final_verdict.as_str(), "BUY" | "ACCUMULATE");
    let cio_is_bearish = matches!(result.final_verdict.as_str(), "TRIM" | "SELL");

    if (macro_is_bullish && cio_is_bearish) || (macro_is_risk_off && cio_is_bullish) {
        result.gate1_pass = false;
        result.final_verdict = "HOLD".to_string();
        // 被否决的 HOLD 是低信念观望，压低 confidence 与 Fallback 口径一致，
        // 避免“HOLD 配 0.7”污染 verdict_reviews 命中率统计。
        result.final_confidence = result.final_confidence.min(0.4);
        result
            .notes
            .push("G1: 宏观信号与CIO裁决不一致，降级为HOLD".to_string());
    }

    // Gate 2 -- Triple deterioration guard clause (from L4 Officer)
    // macro=risk_off + strength ≥ 7 + quant=bearish + strength ≥ 7 + loss ≥ 15% → SELL
    let macro_guard = macro_is_risk_off
        && macro_strength.map_or(false, |s| s >= 7.0);

    // Find latest Quant output for signal + strength
    let quant_output = round_outputs
        .iter()
        .filter(|o| o.role == CommitteeRole::Quant)
        .last();
    let quant_guard = quant_output.map_or(false, |o| {
        o.parsed.signal.as_deref() == Some("bearish")
            && o.parsed.strength.map_or(false, |s| s >= 7.0)
    });

    // Find latest Risk output for loss percentage (pnl_pct < -15 = 15% loss)
    let loss_guard = round_outputs
        .iter()
        .filter(|o| o.role == CommitteeRole::Risk)
        .last()
        .and_then(|o| o.parsed.pnl_pct)
        .map_or(false, |pnl| pnl <= -15.0);

    // research 模式无真实持仓，loss_guard 无意义，跳过 Gate2
    if mode != Mode::Research && macro_guard && quant_guard && loss_guard {
        result.gate2_pass = false;
        result.final_verdict = "SELL".to_string();
        result.final_confidence = 0.2;
        result.notes.push(
            "G2: 三重恶化（宏观risk_off≥7 + 技术bearish≥7 + 浮亏≥15%），强制清仓".to_string(),
        );
    }

    // 仅硬 fallback 触发全局 HOLD；软 fallback（仅缺结构化字段，原文完整）不降级。
    // [WORKER_UNAVAILABLE] marker 始终视为硬不可用。
    let has_unavailable = round_outputs.iter().any(|o| {
        o.parsed.raw_text.contains("[WORKER_UNAVAILABLE]")
            || o.parsed
                .fallback_reason
                .as_deref()
                .is_some_and(is_hard_fallback)
    });
    if has_unavailable {
        result.final_verdict = "HOLD".to_string();
        result.final_confidence = result.final_confidence.min(0.4);
        result
            .notes
            .push("工作节点不可用或输出异常，降级为HOLD".to_string());
    }

    // ── 高信念主动裁决通道 ──────────────────────────────────────────────
    // 纠正"信息不足即躺平"。仅在以下全部满足时把 HOLD 升级为方向性裁决：
    //   - 无任何角色 fallback/不可用(数据缺失绝不伪造信念)
    //   - Gate1 & Gate2 都过(未被宏观矛盾/三重恶化否决)
    //   - 当前是 HOLD(只兜底躺平，不翻已有方向)
    //   - Quant 与 Macro 同向且强度都 ≥ 6
    //   - Risk 信号 != high_risk
    if !has_unavailable
        && result.gate1_pass
        && result.gate2_pass
        && result.final_verdict == "HOLD"
    {
        let quant = round_outputs
            .iter()
            .filter(|o| o.role == CommitteeRole::Quant)
            .last();
        let quant_signal = quant.and_then(|o| o.parsed.signal.clone());
        let quant_strength = quant.and_then(|o| o.parsed.strength).unwrap_or(0.0);

        let risk_signal = round_outputs
            .iter()
            .filter(|o| o.role == CommitteeRole::Risk)
            .last()
            .and_then(|o| o.parsed.signal.clone());
        let risk_ok = risk_signal.as_deref() != Some("high_risk");

        // macro 方向:risk_on=看多, risk_off=看空
        let macro_bull = macro_signal == "risk_on";
        let macro_bear = macro_signal == "risk_off";
        let macro_strong = macro_strength.map_or(false, |s| s >= 6.0);

        // quant 方向
        let quant_bull = quant_signal.as_deref() == Some("bullish");
        let quant_bear = quant_signal.as_deref() == Some("bearish");
        let quant_strong = quant_strength >= 6.0;

        if risk_ok && macro_strong && quant_strong {
            let upgraded = if macro_bull && quant_bull {
                Some("ACCUMULATE")
            } else if macro_bear && quant_bear {
                Some("TRIM")
            } else {
                None
            };
            if let Some(v) = upgraded {
                result.final_verdict = v.to_string();
                // 设下限 0.65,避免"方向/0.3"自相矛盾,又刻意 ≤0.95 避开 hard rule A
                result.final_confidence = result.final_confidence.max(0.65);
                result
                    .notes
                    .push("[HIGH_CONVICTION] Quant与Macro同向强信号，HOLD升级为方向性裁决".to_string());
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Hard rule A/B clamp (CIO_PROMPT 承诺的"系统自动降级/clamp")
// ---------------------------------------------------------------------------

/// Hard rule A/B clamp 结果。
pub struct HardClamp {
    pub verdict: String,
    pub alloc_cny: Option<f64>,
    pub first_tranche_cny: Option<f64>,
}

/// 应用两条 hard rule(对应 CIO_PROMPT 承诺):
/// - rule A: confidence >= 0.95 且 verdict==BUY → 降级 ACCUMULATE。
/// - rule B: |alloc| > 100000 → clamp 到 ±100000;first_tranche 同步 clamp 到 [-cap, cap]。
pub fn apply_hard_rules(
    verdict: &str,
    confidence: f64,
    alloc_cny: Option<f64>,
    first_tranche_cny: Option<f64>,
) -> HardClamp {
    // rule A
    let verdict = if confidence >= 0.95 && verdict == "BUY" {
        "ACCUMULATE".to_string()
    } else {
        verdict.to_string()
    };

    // rule B
    let alloc_cny = alloc_cny.map(|a| a.clamp(-100_000.0, 100_000.0));
    let first_tranche_cny = match (first_tranche_cny, alloc_cny) {
        (Some(ft), Some(a)) => {
            let cap = a.abs();
            // first_tranche 绝对值不超过 clamp 后 alloc 的绝对值(cap);符号保持原值
            Some(ft.clamp(-cap, cap))
        }
        (ft, _) => ft,
    };

    HardClamp {
        verdict,
        alloc_cny,
        first_tranche_cny,
    }
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
    ) -> RoundOutput {
        let mut parsed = ParsedFields::default();
        parsed.signal = signal.map(|s| s.to_string());
        parsed.strength = strength;
        parsed.concentration_pct = concentration;
        parsed.dry_powder_cny = dry_powder;
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
            make_output(CommitteeRole::Quant, 1, Some("risk_on"), Some(7.0), None, None),
            make_output(CommitteeRole::Risk, 1, Some("risk_on"), Some(6.0), None, None),
            make_output(CommitteeRole::Quant, 2, Some("risk_on"), Some(7.0), None, None),
            make_output(CommitteeRole::Risk, 2, Some("risk_on"), Some(6.5), None, None),
        ];
        assert!(check_convergence(&outputs));
    }

    #[test]
    fn test_convergence_not_detected_different_signals() {
        let outputs = vec![
            make_output(CommitteeRole::Quant, 1, Some("risk_on"), Some(7.0), None, None),
            make_output(CommitteeRole::Risk, 1, Some("risk_on"), Some(6.0), None, None),
            make_output(CommitteeRole::Quant, 2, Some("risk_on"), Some(7.0), None, None),
            make_output(CommitteeRole::Risk, 2, Some("risk_off"), Some(6.0), None, None),
        ];
        assert!(!check_convergence(&outputs));
    }

    #[test]
    fn test_convergence_not_detected_strength_drift() {
        let outputs = vec![
            make_output(CommitteeRole::Quant, 1, Some("risk_on"), Some(3.0), None, None),
            make_output(CommitteeRole::Risk, 1, Some("risk_on"), Some(6.0), None, None),
            make_output(CommitteeRole::Quant, 2, Some("risk_on"), Some(8.0), None, None),
            make_output(CommitteeRole::Risk, 2, Some("risk_on"), Some(6.0), None, None),
        ];
        assert!(!check_convergence(&outputs));
    }

    #[test]
    fn test_convergence_none_signals_not_agreement() {
        let outputs = vec![
            make_output(CommitteeRole::Quant, 1, None, Some(7.0), None, None),
            make_output(CommitteeRole::Risk, 1, None, Some(6.0), None, None),
            make_output(CommitteeRole::Quant, 2, None, Some(7.0), None, None),
            make_output(CommitteeRole::Risk, 2, None, Some(6.0), None, None),
        ];
        assert!(!check_convergence(&outputs));
    }

    #[test]
    fn test_sentinel_no_trigger_when_data_missing() {
        let outputs = vec![
            make_output(CommitteeRole::Risk, 1, None, None, None, None),
            make_output(CommitteeRole::Risk, 2, None, None, None, None),
        ];
        assert!(check_sentinel(&outputs).is_none());
    }

    #[test]
    fn test_sentinel_triggers_on_large_shift() {
        let outputs = vec![
            make_output(CommitteeRole::Risk, 1, None, None, Some(20.0), None),
            make_output(CommitteeRole::Risk, 2, None, None, Some(35.0), None),
        ];
        let sentinel = check_sentinel(&outputs);
        assert!(sentinel.is_some());
        let s = sentinel.unwrap();
        assert_eq!(s.forced_verdict, "TRIM");
    }

    #[test]
    fn test_sentinel_no_trigger_small_shift() {
        let outputs = vec![
            make_output(CommitteeRole::Risk, 1, None, None, Some(20.0), None),
            make_output(CommitteeRole::Risk, 2, None, None, Some(20.2), None),
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
        let result = cio_sanity_check(&cio, &outputs, "risk_off", None, Mode::Holding);
        assert!(!result.gate1_pass);
        assert_eq!(result.final_verdict, "HOLD");
    }

    #[test]
    fn test_sanity_gate1_lowers_confidence() {
        // CIO 高信念看多(0.8)，但 macro=risk_off → Gate1 降级 HOLD 并压低 confidence
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.8),
            ..Default::default()
        };
        let outputs = vec![];
        let result = cio_sanity_check(&cio, &outputs, "risk_off", None, Mode::Holding);
        assert!(!result.gate1_pass);
        assert_eq!(result.final_verdict, "HOLD");
        assert_eq!(result.final_confidence, 0.4); // min(0.8, 0.4)
    }

    #[test]
    fn test_sanity_gate1_keeps_lower_confidence() {
        // 原 confidence 已低于 0.4 时,保持原值(min 语义)
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.3),
            ..Default::default()
        };
        let result = cio_sanity_check(&cio, &[], "risk_off", None, Mode::Holding);
        assert_eq!(result.final_confidence, 0.3); // min(0.3, 0.4)
    }

    #[test]
    fn test_sanity_gate2_triple_deterioration() {
        // macro=risk_off + strength=8, quant=bearish + strength=7, loss=20%
        let mut macro_parsed = ParsedFields::default();
        macro_parsed.signal = Some("risk_off".to_string());
        macro_parsed.strength = Some(8.0);
        macro_parsed.raw_text = "macro output".to_string();

        let mut quant_parsed = ParsedFields::default();
        quant_parsed.signal = Some("bearish".to_string());
        quant_parsed.strength = Some(7.0);
        quant_parsed.raw_text = "quant output".to_string();

        let mut risk_parsed = ParsedFields::default();
        risk_parsed.signal = Some("high_risk".to_string());
        risk_parsed.pnl_pct = Some(-20.0);
        risk_parsed.raw_text = "risk output".to_string();

        let outputs = vec![
            RoundOutput { role: CommitteeRole::Macro, round: 1, parsed: macro_parsed, latency_ms: 0, tokens_used: 0 },
            RoundOutput { role: CommitteeRole::Quant, round: 1, parsed: quant_parsed, latency_ms: 0, tokens_used: 0 },
            RoundOutput { role: CommitteeRole::Risk, round: 1, parsed: risk_parsed, latency_ms: 0, tokens_used: 0 },
        ];
        let cio = ParsedFields {
            verdict: Some("HOLD".to_string()),
            confidence: Some(0.5),
            ..Default::default()
        };
        let result = cio_sanity_check(&cio, &outputs, "risk_off", Some(8.0), Mode::Holding);
        assert!(!result.gate2_pass);
        assert_eq!(result.final_verdict, "SELL");
        assert_eq!(result.final_confidence, 0.2);
    }

    #[test]
    fn test_sanity_gate2_no_trigger_weak_macro() {
        // macro=risk_off but strength=5 (below threshold) → G2 should pass
        let quant = ParsedFields {
            signal: Some("bearish".to_string()),
            strength: Some(8.0),
            raw_text: "quant".to_string(),
            ..Default::default()
        };
        let risk = ParsedFields {
            pnl_pct: Some(-20.0),
            raw_text: "risk".to_string(),
            ..Default::default()
        };
        let outputs = vec![
            RoundOutput { role: CommitteeRole::Quant, round: 1, parsed: quant, latency_ms: 0, tokens_used: 0 },
            RoundOutput { role: CommitteeRole::Risk, round: 1, parsed: risk, latency_ms: 0, tokens_used: 0 },
        ];
        let cio = ParsedFields {
            verdict: Some("HOLD".to_string()),
            confidence: Some(0.5),
            ..Default::default()
        };
        let result = cio_sanity_check(&cio, &outputs, "risk_off", Some(5.0), Mode::Holding);
        assert!(result.gate2_pass);
    }

    #[test]
    fn test_sanity_worker_unavailable() {
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.8),
            ..Default::default()
        };
        let outputs = vec![RoundOutput {
            role: CommitteeRole::Quant,
            round: 1,
            parsed: ParsedFields {
                raw_text: "[WORKER_UNAVAILABLE]".to_string(),
                ..Default::default()
            },
            latency_ms: 0,
            tokens_used: 0,
        }];
        let result = cio_sanity_check(&cio, &outputs, "risk_on", None, Mode::Holding);
        assert_eq!(result.final_verdict, "HOLD");
        assert!(result.final_confidence <= 0.4);
    }

    #[test]
    fn test_soft_fallback_does_not_force_hold() {
        // missing_critical_fields 是软 fallback：有原文仅缺结构化字段，不应把整盘压成 HOLD
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.8),
            ..Default::default()
        };
        let outputs = vec![RoundOutput {
            role: CommitteeRole::Quant,
            round: 2,
            parsed: ParsedFields {
                raw_text: "保护触发: yes".to_string(),
                fallback_reason: Some("missing_critical_fields".to_string()),
                ..Default::default()
            },
            latency_ms: 0,
            tokens_used: 0,
        }];
        let result = cio_sanity_check(&cio, &outputs, "risk_on", None, Mode::Holding);
        assert_eq!(result.final_verdict, "BUY"); // 保留 CIO 原裁决，不再降级
    }

    #[test]
    fn test_hard_fallback_still_forces_hold() {
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.8),
            ..Default::default()
        };
        let outputs = vec![RoundOutput {
            role: CommitteeRole::Risk,
            round: 1,
            parsed: ParsedFields {
                raw_text: "[WORKER_UNAVAILABLE] cli failed".to_string(),
                fallback_reason: Some("cli_error: timeout".to_string()),
                ..Default::default()
            },
            latency_ms: 0,
            tokens_used: 0,
        }];
        let result = cio_sanity_check(&cio, &outputs, "risk_on", None, Mode::Holding);
        assert_eq!(result.final_verdict, "HOLD");
        assert!(result.final_confidence <= 0.4);
    }

    #[test]
    fn test_cli_error_without_marker_forces_hold() {
        // 隔离 is_hard_fallback 的 cli_error 前缀分支：
        // raw_text 中没有 [WORKER_UNAVAILABLE]，fallback_reason 也不在 matches!() 三元组里，
        // 唯一能让 has_unavailable=true 的路径就是 starts_with("cli_error")。
        // 若有人把 starts_with 改成 == "cli_error"，本用例会 RED：final_verdict 会停在 BUY/0.8。
        // macro=risk_on + cio=BUY 双多头 → G1 不冲突；macro 非 risk_off → G2 macro_guard=false。
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.8),
            ..Default::default()
        };
        let outputs = vec![RoundOutput {
            role: CommitteeRole::Quant,
            round: 1,
            parsed: ParsedFields {
                raw_text: "分析超时".to_string(),
                fallback_reason: Some("cli_error: deadline exceeded".to_string()),
                ..Default::default()
            },
            latency_ms: 0,
            tokens_used: 0,
        }];
        let result = cio_sanity_check(&cio, &outputs, "risk_on", None, Mode::Holding);
        assert_eq!(result.final_verdict, "HOLD");
        assert!(result.final_confidence <= 0.4);
    }

    #[test]
    fn test_sanity_all_gates_pass() {
        let cio = ParsedFields {
            verdict: Some("ACCUMULATE".to_string()),
            confidence: Some(0.7),
            ..Default::default()
        };
        let outputs = vec![];
        let result = cio_sanity_check(&cio, &outputs, "risk_on", None, Mode::Holding);
        assert!(result.gate1_pass);
        assert!(result.gate2_pass);
        assert_eq!(result.final_verdict, "ACCUMULATE");
    }

    // ---- Hard rule A/B clamp ----

    #[test]
    fn test_hard_rule_a_downgrades_high_conf_buy() {
        let r = apply_hard_rules("BUY", 0.97, Some(50_000.0), Some(20_000.0));
        assert_eq!(r.verdict, "ACCUMULATE");
    }

    #[test]
    fn test_hard_rule_a_not_triggered_below_threshold() {
        // 高信念通道升级用 0.65，刻意不触发 rule A
        let r = apply_hard_rules("BUY", 0.65, None, None);
        assert_eq!(r.verdict, "BUY");
    }

    #[test]
    fn test_hard_rule_b_clamps_alloc_and_first_tranche() {
        let r = apply_hard_rules("BUY", 0.7, Some(150_000.0), Some(160_000.0));
        assert_eq!(r.alloc_cny, Some(100_000.0));
        assert_eq!(r.first_tranche_cny, Some(100_000.0)); // 同步 clamp 到 cap
    }

    #[test]
    fn test_hard_rule_b_preserves_within_limit() {
        let r = apply_hard_rules("ACCUMULATE", 0.6, Some(80_000.0), Some(30_000.0));
        assert_eq!(r.alloc_cny, Some(80_000.0));
        assert_eq!(r.first_tranche_cny, Some(30_000.0));
    }

    // ---- High-conviction upgrade channel ----

    // 构造高信念升级所需的 round_outputs：Quant bullish≥6 + Macro risk_on≥6 + Risk ok
    fn high_conviction_outputs(quant_signal: &str, quant_str: f64, risk_signal: &str) -> Vec<RoundOutput> {
        let mut q = ParsedFields::default();
        q.signal = Some(quant_signal.to_string());
        q.strength = Some(quant_str);
        q.raw_text = "quant".to_string();
        let mut r = ParsedFields::default();
        r.signal = Some(risk_signal.to_string());
        r.raw_text = "risk".to_string();
        vec![
            RoundOutput { role: CommitteeRole::Quant, round: 1, parsed: q, latency_ms: 0, tokens_used: 0 },
            RoundOutput { role: CommitteeRole::Risk, round: 1, parsed: r, latency_ms: 0, tokens_used: 0 },
        ]
    }

    #[test]
    fn test_high_conviction_upgrades_hold() {
        // HOLD + Quant bullish 7 + Macro risk_on 7 + Risk ok → 升级到 BUY/ACCUMULATE，conf≥0.65
        let cio = ParsedFields { verdict: Some("HOLD".to_string()), confidence: Some(0.35), ..Default::default() };
        let outputs = high_conviction_outputs("bullish", 7.0, "ok");
        let result = cio_sanity_check(&cio, &outputs, "risk_on", Some(7.0), Mode::Holding);
        assert!(matches!(result.final_verdict.as_str(), "BUY" | "ACCUMULATE"));
        assert!(result.final_confidence >= 0.65);
        assert!(result.notes.iter().any(|n| n.contains("HIGH_CONVICTION")));
    }

    #[test]
    fn test_high_conviction_skipped_when_risk_high() {
        // Risk=high_risk → 不升级
        let cio = ParsedFields { verdict: Some("HOLD".to_string()), confidence: Some(0.35), ..Default::default() };
        let outputs = high_conviction_outputs("bullish", 7.0, "high_risk");
        let result = cio_sanity_check(&cio, &outputs, "risk_on", Some(7.0), Mode::Holding);
        assert_eq!(result.final_verdict, "HOLD");
    }

    #[test]
    fn test_high_conviction_skipped_when_strength_low() {
        // Quant strength=5 (<6) → 不升级
        let cio = ParsedFields { verdict: Some("HOLD".to_string()), confidence: Some(0.35), ..Default::default() };
        let outputs = high_conviction_outputs("bullish", 5.0, "ok");
        let result = cio_sanity_check(&cio, &outputs, "risk_on", Some(7.0), Mode::Holding);
        assert_eq!(result.final_verdict, "HOLD");
    }

    #[test]
    fn test_high_conviction_skipped_on_hard_fallback() {
        // 硬 fallback → Fallback 先把 verdict 压成 HOLD/≤0.4，高信念不得翻案
        let cio = ParsedFields { verdict: Some("HOLD".to_string()), confidence: Some(0.35), ..Default::default() };
        let mut outputs = high_conviction_outputs("bullish", 7.0, "ok");
        outputs[0].parsed.raw_text = "[WORKER_UNAVAILABLE]".to_string();
        outputs[0].parsed.fallback_reason = Some("worker_unavailable".to_string());
        let result = cio_sanity_check(&cio, &outputs, "risk_on", Some(7.0), Mode::Holding);
        assert_eq!(result.final_verdict, "HOLD");
    }

    #[test]
    fn test_high_conviction_not_blocked_by_soft_fallback() {
        // 软 fallback 不阻止高信念升级
        let cio = ParsedFields { verdict: Some("HOLD".to_string()), confidence: Some(0.35), ..Default::default() };
        let mut outputs = high_conviction_outputs("bullish", 7.0, "ok");
        outputs[0].parsed.fallback_reason = Some("missing_critical_fields".to_string());
        let result = cio_sanity_check(&cio, &outputs, "risk_on", Some(7.0), Mode::Holding);
        assert!(matches!(result.final_verdict.as_str(), "ACCUMULATE" | "BUY"));
    }

    #[test]
    fn test_gate2_skipped_in_research_mode() {
        // 即使三重恶化条件全满足,research 模式也不触发 Gate2 强制 SELL
        let mut macro_parsed = ParsedFields::default();
        macro_parsed.signal = Some("risk_off".to_string());
        macro_parsed.strength = Some(8.0);
        macro_parsed.raw_text = "macro".to_string();
        let mut quant_parsed = ParsedFields::default();
        quant_parsed.signal = Some("bearish".to_string());
        quant_parsed.strength = Some(7.0);
        quant_parsed.raw_text = "quant".to_string();
        let mut risk_parsed = ParsedFields::default();
        risk_parsed.pnl_pct = Some(-20.0);
        risk_parsed.raw_text = "risk".to_string();
        let outputs = vec![
            RoundOutput { role: CommitteeRole::Macro, round: 1, parsed: macro_parsed, latency_ms: 0, tokens_used: 0 },
            RoundOutput { role: CommitteeRole::Quant, round: 1, parsed: quant_parsed, latency_ms: 0, tokens_used: 0 },
            RoundOutput { role: CommitteeRole::Risk, round: 1, parsed: risk_parsed, latency_ms: 0, tokens_used: 0 },
        ];
        let cio = ParsedFields { verdict: Some("HOLD".to_string()), confidence: Some(0.5), ..Default::default() };
        let result = cio_sanity_check(&cio, &outputs, "risk_off", Some(8.0), Mode::Research);
        assert!(result.gate2_pass); // research 模式 Gate2 不触发
        assert_ne!(result.final_verdict, "SELL");
    }
}
