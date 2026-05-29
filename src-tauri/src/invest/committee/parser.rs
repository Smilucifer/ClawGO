use super::roles::CommitteeRole;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Parsed fields from LLM output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedFields {
    /// Macro: "risk_on" | "risk_off" | "neutral"
    pub signal: Option<String>,
    /// Strength 1-10 (Macro, Quant, Risk)
    pub strength: Option<f64>,
    /// Risk Officer: current max concentration %
    pub concentration_pct: Option<f64>,
    /// Risk Officer: available dry powder in CNY
    pub dry_powder_cny: Option<f64>,
    /// Quant R1/R2: AGREE / CHALLENGE / NEUTRAL / MAINTAIN / UPGRADE
    pub quant_view: Option<String>,
    /// Risk R1/R2: AGREE / CHALLENGE / OVERRIDE / MAINTAIN / UPGRADE_TO_OVERRIDE
    pub risk_view: Option<String>,
    /// Wealth: FAVORABLE / NEUTRAL / CAUTIOUS
    pub wealth_context: Option<String>,
    /// Wealth: HIGH / MEDIUM / LOW / CRITICAL
    pub solvency_buffer_level: Option<String>,
    /// CIO: BUY / ACCUMULATE / HOLD / TRIM / SELL
    pub verdict: Option<String>,
    /// CIO: 0.0-1.0
    pub confidence: Option<f64>,
    /// CIO: personal note
    pub personal_note: Option<String>,
    /// CIO: execution plan
    pub execution_plan: Option<String>,
    /// CIO: risk plan
    pub risk_plan: Option<String>,
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
        CommitteeRole::QuantR1 | CommitteeRole::QuantR2 => parse_quant(text, &mut parsed),
        CommitteeRole::RiskR1 | CommitteeRole::RiskR2 => parse_risk(text, &mut parsed),
        CommitteeRole::Wealth => parse_wealth(text, &mut parsed),
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
    parsed.quant_view = extract_field(text, "QUANT_VIEW")
        .or_else(|| extract_field(text, "QUANT_R2_VIEW"));
    parsed.strength = extract_f64(text, "STRENGTH");
}

fn parse_risk(text: &str, parsed: &mut ParsedFields) {
    parsed.risk_view = extract_field(text, "RISK_VIEW")
        .or_else(|| extract_field(text, "RISK_R2_VIEW"));
    parsed.concentration_pct = extract_f64(text, "CONCENTRATION_PCT");
    parsed.dry_powder_cny = extract_f64(text, "DRY_POWDER_CNY");
    parsed.strength = extract_f64(text, "STRENGTH");
}

fn parse_wealth(text: &str, parsed: &mut ParsedFields) {
    parsed.wealth_context = extract_field(text, "WEALTH_CONTEXT");
    parsed.solvency_buffer_level = extract_field(text, "SOLVENCY_BUFFER_LEVEL");
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
    fn test_parse_quant_agree() {
        let text = "QUANT_VIEW: AGREE_with_Macro\n技术指标支持看多。\nSTRENGTH: 7";
        let parsed = parse_role_output(CommitteeRole::QuantR1, text, false);
        assert_eq!(parsed.quant_view.as_deref(), Some("AGREE_with_Macro"));
        assert_eq!(parsed.strength, Some(7.0));
    }

    #[test]
    fn test_parse_risk_with_concentration() {
        let text = "RISK_VIEW: CHALLENGE\nCONCENTRATION_PCT: 35\nDRY_POWDER_CNY: 50000\nSTRENGTH: 6";
        let parsed = parse_role_output(CommitteeRole::RiskR1, text, false);
        assert_eq!(parsed.risk_view.as_deref(), Some("CHALLENGE"));
        assert_eq!(parsed.concentration_pct, Some(35.0));
        assert_eq!(parsed.dry_powder_cny, Some(50000.0));
    }

    #[test]
    fn test_parse_cio_verdict() {
        let text = "VERDICT: HOLD\nCONFIDENCE: 0.6\nCONCENTRATION_PCT: 25\nPERSONAL_NOTE: 等待确认\nEXECUTION_PLAN: 无操作\nRISK_PLAN: 维持现有仓位";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.verdict.as_deref(), Some("HOLD"));
        assert_eq!(parsed.confidence, Some(0.6));
        assert_eq!(parsed.personal_note.as_deref(), Some("等待确认"));
    }

    #[test]
    fn test_parse_wealth() {
        let text = "WEALTH_CONTEXT: CAUTIOUS\nSOLVENCY_BUFFER_LEVEL: LOW";
        let parsed = parse_role_output(CommitteeRole::Wealth, text, false);
        assert_eq!(parsed.wealth_context.as_deref(), Some("CAUTIOUS"));
        assert_eq!(parsed.solvency_buffer_level.as_deref(), Some("LOW"));
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
        let long = "这是一段超过200个汉字的测试文本".repeat(50);
        let (result, was_truncated) =
            super::super::roles::hard_truncate(&long, CommitteeRole::QuantR1, 1);
        assert!(was_truncated);
        assert!(result.chars().count() <= 200);
    }
}
