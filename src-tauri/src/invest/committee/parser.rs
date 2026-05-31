use super::roles::CommitteeRole;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Parsed fields from LLM output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedFields {
    // -- Shared directional fields --
    /// Unified directional signal: Macro SIGNAL, Quant R1 SIGNAL / R2 ADJUSTED_SIGNAL,
    /// Risk R1 SIGNAL / R2 ADJUSTED_SIGNAL
    pub signal: Option<String>,
    /// Strength 1-10 (Macro, Quant, Risk) — R1 STRENGTH / R2 ADJUSTED_STRENGTH
    pub strength: Option<f64>,

    // -- Quant-specific --
    /// Quant R1: REGIME 回填
    pub regime: Option<String>,
    /// Quant R1: KEY_DATA 行列表项
    pub key_data: Option<Vec<String>>,
    /// Quant R1 / Risk R1: ONE_LINER 一句话摘要
    pub one_liner: Option<String>,
    /// Quant R2: REGIME_PROTECTION_TRIGGERED (true/false)
    pub regime_protection_triggered: Option<bool>,

    // -- Risk-specific --
    /// Risk Officer: current max concentration %
    pub concentration_pct: Option<f64>,
    /// Risk Officer: available dry powder in CNY
    pub dry_powder_cny: Option<f64>,
    /// Risk R1: PNL_PCT 当期盈亏百分比
    pub pnl_pct: Option<f64>,
    /// Risk R1: WORST_CASE_LOSS_PCT_AT_-20
    pub worst_case_loss_pct: Option<f64>,
    /// Risk R2: ADJUSTED_STOP_LOSS
    pub adjusted_stop_loss: Option<String>,

    // -- CIO-specific --
    /// CIO: BUY / ACCUMULATE / HOLD / TRIM / SELL
    pub verdict: Option<String>,
    /// CIO: 0.0-1.0
    pub confidence: Option<f64>,
    /// Quant R2 / Risk R2 / CIO: REASONING
    pub reasoning: Option<String>,
    /// CIO: DOMINANT_VIEW
    pub dominant_view: Option<String>,
    /// CIO: SUGGESTED_ALLOC_CNY
    pub suggested_alloc_cny: Option<f64>,
    /// CIO: personal note
    pub personal_note: Option<String>,
    /// CIO: execution plan
    pub execution_plan: Option<String>,
    /// CIO: risk plan
    pub risk_plan: Option<String>,

    // -- Meta --
    /// Whether output was truncated by hard limit
    pub truncated: bool,
    /// Raw text (preserved for archiving)
    pub raw_text: String,
}

// ---------------------------------------------------------------------------
// Parser functions
// ---------------------------------------------------------------------------

/// Parse LLM output for any role into structured fields.
pub fn parse_role_output(role: CommitteeRole, text: &str, truncated: bool) -> ParsedFields {
    let mut parsed = ParsedFields {
        raw_text: text.to_string(),
        truncated,
        ..Default::default()
    };

    match role {
        CommitteeRole::Macro => parse_macro(text, &mut parsed),
        CommitteeRole::Quant => parse_quant(text, &mut parsed),
        CommitteeRole::Risk => parse_risk(text, &mut parsed),
        CommitteeRole::Cio => parse_cio(text, &mut parsed),
    }

    parsed
}

fn extract_field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&format!("{}:", key)) {
            return Some(rest.trim().to_string());
        }
        if let Some(rest) = line.strip_prefix(&format!("{}：", key)) {
            // Chinese colon variant
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn extract_f64(text: &str, key: &str) -> Option<f64> {
    extract_field(text, key).and_then(|v| v.parse::<f64>().ok())
}

/// Extract a boolean field ("true"/"false", case-insensitive).
fn extract_bool(text: &str, key: &str) -> Option<bool> {
    extract_field(text, key).and_then(|v| match v.to_lowercase().as_str() {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    })
}

/// Extract a list field: finds the key line, then collects subsequent ` - item` lines.
/// Returns `None` if the key is not found (empty list is distinguishable from missing).
/// Apply R2 ADJUSTED_SIGNAL/ADJUSTED_STRENGTH override to parsed fields.
fn apply_r2_signal_override(parsed: &mut ParsedFields, text: &str) {
    if let Some(adjusted) = extract_field(text, "ADJUSTED_SIGNAL") {
        parsed.signal = Some(adjusted);
    }
    if let Some(strength) = extract_f64(text, "ADJUSTED_STRENGTH") {
        parsed.strength = Some(strength);
    }
}

fn extract_list_field(text: &str, key: &str) -> Option<Vec<String>> {
    let mut items = Vec::new();
    let mut found_key = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !found_key {
            if trimmed.strip_prefix(&format!("{}:", key)).is_some()
                || trimmed.strip_prefix(&format!("{}：", key)).is_some()
            {
                found_key = true;
                // The key line may also have an inline value after the colon — skip it
                // (e.g. "KEY_DATA: - item1" would not start with ` - `)
                continue;
            }
        } else {
            // Collect ` - item` or `- item` lines until a non-list line or empty line
            if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("— ")) {
                let item = item.trim().to_string();
                if !item.is_empty() {
                    items.push(item);
                }
            } else if trimmed.is_empty() || trimmed.contains(':') || trimmed.contains('：') {
                // Reached next section — stop collecting
                break;
            }
        }
    }
    if found_key {
        Some(items)
    } else {
        None
    }
}

fn parse_macro(text: &str, parsed: &mut ParsedFields) {
    parsed.signal = extract_field(text, "SIGNAL").map(|s| {
        let s = s.to_lowercase();
        if s.contains("risk_on") || s.contains("risk on") {
            "risk_on".to_string()
        } else if s.contains("risk_off") || s.contains("risk off") {
            "risk_off".to_string()
        } else {
            "neutral".to_string()
        }
    });
    parsed.strength = extract_f64(text, "STRENGTH");
}

fn parse_quant(text: &str, parsed: &mut ParsedFields) {
    // R1 fields
    parsed.regime = extract_field(text, "REGIME");
    parsed.signal = extract_field(text, "SIGNAL");
    parsed.strength = extract_f64(text, "STRENGTH");
    parsed.key_data = extract_list_field(text, "KEY_DATA");
    parsed.one_liner = extract_field(text, "ONE_LINER");
    // R2 fields — R2 overrides R1 where applicable
    apply_r2_signal_override(parsed, text);
    parsed.regime_protection_triggered = extract_bool(text, "REGIME_PROTECTION_TRIGGERED");
    parsed.reasoning = extract_field(text, "REASONING");
}

fn parse_risk(text: &str, parsed: &mut ParsedFields) {
    // R1 fields
    parsed.signal = extract_field(text, "SIGNAL");
    parsed.strength = extract_f64(text, "STRENGTH");
    parsed.pnl_pct = extract_f64(text, "PNL_PCT");
    parsed.worst_case_loss_pct = extract_f64(text, "WORST_CASE_LOSS_PCT_AT_-20");
    parsed.one_liner = extract_field(text, "ONE_LINER");
    parsed.concentration_pct = extract_f64(text, "CONCENTRATION_PCT");
    parsed.dry_powder_cny = extract_f64(text, "DRY_POWDER_CNY");
    // R2 fields — R2 overrides R1 where applicable
    apply_r2_signal_override(parsed, text);
    parsed.adjusted_stop_loss = extract_field(text, "ADJUSTED_STOP_LOSS");
    parsed.reasoning = extract_field(text, "REASONING");
    // CONCENTRATION_PCT and DRY_POWDER_CNY may also appear in R2
}

fn parse_cio(text: &str, parsed: &mut ParsedFields) {
    parsed.verdict = extract_field(text, "VERDICT").map(|v| {
        let v = v.to_uppercase();
        if v.contains("BUY") {
            "BUY".to_string()
        } else if v.contains("ACCUMULATE") {
            "ACCUMULATE".to_string()
        } else if v.contains("HOLD") {
            "HOLD".to_string()
        } else if v.contains("TRIM") {
            "TRIM".to_string()
        } else if v.contains("SELL") {
            "SELL".to_string()
        } else {
            v
        }
    });
    parsed.confidence = extract_f64(text, "CONFIDENCE");
    parsed.concentration_pct = extract_f64(text, "CONCENTRATION_PCT");
    parsed.dominant_view = extract_field(text, "DOMINANT_VIEW");
    parsed.suggested_alloc_cny = extract_f64(text, "SUGGESTED_ALLOC_CNY");
    parsed.reasoning = extract_field(text, "REASONING");
    parsed.personal_note = extract_field(text, "PERSONAL_NOTE");
    parsed.execution_plan = extract_field(text, "EXECUTION_PLAN");
    parsed.risk_plan = extract_field(text, "RISK_PLAN");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_macro_risk_on() {
        let text = "当前市场处于上升趋势,沪深300 60日分位75%,北向资金持续流入。\n\nSIGNAL: risk_on";
        let parsed = parse_role_output(CommitteeRole::Macro, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("risk_on"));
    }

    #[test]
    fn test_parse_macro_risk_off_with_strength() {
        let text = "市场恐慌,连续5日跌幅超8%。\nSIGNAL: risk_off\nSTRENGTH: 8";
        let parsed = parse_role_output(CommitteeRole::Macro, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("risk_off"));
        assert_eq!(parsed.strength, Some(8.0));
    }

    #[test]
    fn test_parse_quant_r1() {
        let text = "REGIME: bull\nSIGNAL: risk_on\nSTRENGTH: 7\nKEY_DATA:\n - 沪深300 PE=13.5\n - 北向资金净流入120亿\nONE_LINER: 技术面偏多";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.regime.as_deref(), Some("bull"));
        assert_eq!(parsed.signal.as_deref(), Some("risk_on"));
        assert_eq!(parsed.strength, Some(7.0));
        assert_eq!(
            parsed.key_data.as_deref(),
            Some(&["沪深300 PE=13.5".to_string(), "北向资金净流入120亿".to_string()][..])
        );
        assert_eq!(parsed.one_liner.as_deref(), Some("技术面偏多"));
    }

    #[test]
    fn test_parse_quant_r2_with_adjusted() {
        let text = "ADJUSTED_SIGNAL: risk_off\nADJUSTED_STRENGTH: 5\nREGIME_PROTECTION_TRIGGERED: true\nREASONING: 短期回调信号增强";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("risk_off"));
        assert_eq!(parsed.strength, Some(5.0));
        assert_eq!(parsed.regime_protection_triggered, Some(true));
        assert_eq!(parsed.reasoning.as_deref(), Some("短期回调信号增强"));
    }

    #[test]
    fn test_parse_risk_r1() {
        let text = "SIGNAL: risk_on\nSTRENGTH: 6\nPNL_PCT: 3.5\nWORST_CASE_LOSS_PCT_AT_-20: -12\nONE_LINER: 风险可控\nCONCENTRATION_PCT: 35\nDRY_POWDER_CNY: 50000";
        let parsed = parse_role_output(CommitteeRole::Risk, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("risk_on"));
        assert_eq!(parsed.strength, Some(6.0));
        assert_eq!(parsed.pnl_pct, Some(3.5));
        assert_eq!(parsed.worst_case_loss_pct, Some(-12.0));
        assert_eq!(parsed.one_liner.as_deref(), Some("风险可控"));
        assert_eq!(parsed.concentration_pct, Some(35.0));
        assert_eq!(parsed.dry_powder_cny, Some(50000.0));
    }

    #[test]
    fn test_parse_risk_r2_with_adjusted() {
        let text = "ADJUSTED_SIGNAL: risk_off\nADJUSTED_STOP_LOSS: 0.92\nREASONING: 下行保护触发\nCONCENTRATION_PCT: 30\nDRY_POWDER_CNY: 60000";
        let parsed = parse_role_output(CommitteeRole::Risk, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("risk_off"));
        assert_eq!(parsed.adjusted_stop_loss.as_deref(), Some("0.92"));
        assert_eq!(parsed.reasoning.as_deref(), Some("下行保护触发"));
        assert_eq!(parsed.concentration_pct, Some(30.0));
        assert_eq!(parsed.dry_powder_cny, Some(60000.0));
    }

    #[test]
    fn test_parse_cio_verdict() {
        let text = "VERDICT: HOLD\nCONFIDENCE: 0.6\nDOMINANT_VIEW: 震荡市观望\nSUGGESTED_ALLOC_CNY: 200000\nREASONING: 等待数据确认\nCONCENTRATION_PCT: 25\nPERSONAL_NOTE: 等待确认\nEXECUTION_PLAN: 无操作\nRISK_PLAN: 维持现有仓位";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.verdict.as_deref(), Some("HOLD"));
        assert_eq!(parsed.confidence, Some(0.6));
        assert_eq!(parsed.dominant_view.as_deref(), Some("震荡市观望"));
        assert_eq!(parsed.suggested_alloc_cny, Some(200000.0));
        assert_eq!(parsed.reasoning.as_deref(), Some("等待数据确认"));
        assert_eq!(parsed.personal_note.as_deref(), Some("等待确认"));
    }

    #[test]
    fn test_parse_with_chinese_colon() {
        let text = "VERDICT：BUY\nCONFIDENCE：0.8";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.verdict.as_deref(), Some("BUY"));
        assert_eq!(parsed.confidence, Some(0.8));
    }

    #[test]
    fn test_parse_empty_text() {
        let parsed = parse_role_output(CommitteeRole::Macro, "", false);
        assert!(parsed.signal.is_none());
        assert!(parsed.strength.is_none());
    }

    #[test]
    fn test_truncated_flag() {
        let parsed = parse_role_output(CommitteeRole::Macro, "SIGNAL: risk_on", true);
        assert!(parsed.truncated);
    }

    #[test]
    fn test_hard_truncate_noop() {
        let short = "short text";
        let (result, was_truncated) = super::super::roles::hard_truncate(short, CommitteeRole::Macro, 1);
        assert_eq!(result, short);
        assert!(!was_truncated);
    }

    #[test]
    fn test_hard_truncate_actual() {
        let long = "这是一段超过250个汉字的测试文本".repeat(50);
        let (result, was_truncated) =
            super::super::roles::hard_truncate(&long, CommitteeRole::Quant, 1);
        assert!(was_truncated);
        assert!(result.chars().count() <= 250);
    }
}
