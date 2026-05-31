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

/// Try extracting a field by multiple keys (e.g., English then Chinese).
/// Returns the first match found.
fn extract_field_any(text: &str, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(val) = extract_field(text, key) {
            return Some(val);
        }
    }
    None
}

fn extract_f64(text: &str, key: &str) -> Option<f64> {
    extract_field(text, key).and_then(|v| v.parse::<f64>().ok())
}

/// Try extracting an f64 field by multiple keys.
fn extract_f64_any(text: &str, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(val) = extract_f64(text, key) {
            return Some(val);
        }
    }
    None
}

/// Extract a boolean field ("true"/"false", case-insensitive).
fn extract_bool(text: &str, key: &str) -> Option<bool> {
    extract_field(text, key).and_then(|v| match v.to_lowercase().as_str() {
        "true" | "yes" | "1" => Some(true),
        "false" | "no" | "0" => Some(false),
        _ => None,
    })
}

/// Try extracting a boolean field by multiple keys.
fn extract_bool_any(text: &str, keys: &[&str]) -> Option<bool> {
    for key in keys {
        if let Some(val) = extract_bool(text, key) {
            return Some(val);
        }
    }
    None
}

/// Extract a list field: finds the key line, then collects subsequent ` - item` lines.
/// Returns `None` if the key is not found (empty list is distinguishable from missing).
/// Apply R2 ADJUSTED_SIGNAL/ADJUSTED_STRENGTH override to parsed fields.
fn apply_r2_signal_override(parsed: &mut ParsedFields, text: &str) {
    // English keys: ADJUSTED_SIGNAL / 调整信号
    if let Some(adjusted) = extract_field_any(text, &["ADJUSTED_SIGNAL", "调整信号"]) {
        parsed.signal = Some(adjusted);
    }
    // English keys: ADJUSTED_STRENGTH / 调整强度
    if let Some(strength) = extract_f64_any(text, &["ADJUSTED_STRENGTH", "调整强度"]) {
        parsed.strength = Some(strength);
    }
}

/// Extract a list field, trying multiple keys (bilingual support).
fn extract_list_field_any(text: &str, keys: &[&str]) -> Option<Vec<String>> {
    let mut items = Vec::new();
    let mut found_key = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !found_key {
            for key in keys {
                if trimmed.strip_prefix(&format!("{}:", key)).is_some()
                    || trimmed.strip_prefix(&format!("{}：", key)).is_some()
                {
                    found_key = true;
                    break;
                }
            }
            if found_key {
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
    // English: SIGNAL / 信号
    parsed.signal = extract_field_any(text, &["SIGNAL", "信号"]).map(|s| {
        let s = s.to_lowercase();
        if s.contains("risk_on") || s.contains("risk on") {
            "risk_on".to_string()
        } else if s.contains("risk_off") || s.contains("risk off") {
            "risk_off".to_string()
        } else {
            "neutral".to_string()
        }
    });
    // English: STRENGTH / 强度
    parsed.strength = extract_f64_any(text, &["STRENGTH", "强度"]);
}

fn parse_quant(text: &str, parsed: &mut ParsedFields) {
    // R1 fields
    parsed.regime = extract_field_any(text, &["REGIME", "市场状态"]);
    parsed.signal = extract_field_any(text, &["SIGNAL", "信号"]);
    parsed.strength = extract_f64_any(text, &["STRENGTH", "强度"]);
    parsed.key_data = extract_list_field_any(text, &["KEY_DATA", "关键数据"]);
    parsed.one_liner = extract_field_any(text, &["ONE_LINER", "一句话"]);
    // R2 fields — R2 overrides R1 where applicable
    apply_r2_signal_override(parsed, text);
    parsed.regime_protection_triggered = extract_bool_any(text, &["REGIME_PROTECTION_TRIGGERED", "保护触发"]);
    parsed.reasoning = extract_field_any(text, &["REASONING", "推理"]);
}

fn parse_risk(text: &str, parsed: &mut ParsedFields) {
    // R1 fields
    parsed.signal = extract_field_any(text, &["SIGNAL", "信号"]);
    parsed.strength = extract_f64_any(text, &["STRENGTH", "强度"]);
    parsed.pnl_pct = extract_f64_any(text, &["PNL_PCT", "盈亏比"]);
    parsed.worst_case_loss_pct = extract_f64_any(text, &["WORST_CASE_LOSS_PCT_AT_-20", "最大回撤"]);
    parsed.one_liner = extract_field_any(text, &["ONE_LINER", "一句话"]);
    parsed.concentration_pct = extract_f64_any(text, &["CONCENTRATION_PCT", "集中度"]);
    parsed.dry_powder_cny = extract_f64_any(text, &["DRY_POWDER_CNY", "可用子弹"]);
    // R2 fields — R2 overrides R1 where applicable
    apply_r2_signal_override(parsed, text);
    parsed.adjusted_stop_loss = extract_field_any(text, &["ADJUSTED_STOP_LOSS", "调整止损"]);
    parsed.reasoning = extract_field_any(text, &["REASONING", "推理"]);
    // CONCENTRATION_PCT and DRY_POWDER_CNY may also appear in R2
}

fn parse_cio(text: &str, parsed: &mut ParsedFields) {
    // English: VERDICT / 裁决
    parsed.verdict = extract_field_any(text, &["VERDICT", "裁决"]).map(|v| {
        // Uppercase normalizes English variants; Chinese chars are unaffected by to_uppercase()
        let v = v.to_uppercase();
        if v.contains("BUY") || v.contains("买入") {
            "BUY".to_string()
        } else if v.contains("ACCUMULATE") || v.contains("加仓") {
            "ACCUMULATE".to_string()
        } else if v.contains("HOLD") || v.contains("持有") {
            "HOLD".to_string()
        } else if v.contains("TRIM") || v.contains("减仓") {
            "TRIM".to_string()
        } else if v.contains("SELL") || v.contains("卖出") {
            "SELL".to_string()
        } else {
            v
        }
    });
    parsed.confidence = extract_f64_any(text, &["CONFIDENCE", "置信度"]);
    parsed.concentration_pct = extract_f64_any(text, &["CONCENTRATION_PCT", "集中度"]);
    parsed.dry_powder_cny = extract_f64_any(text, &["DRY_POWDER_CNY", "可用子弹"]);
    parsed.dominant_view = extract_field_any(text, &["DOMINANT_VIEW", "主流观点"]);
    parsed.suggested_alloc_cny = extract_f64_any(text, &["SUGGESTED_ALLOC_CNY", "建议配置"]);
    parsed.reasoning = extract_field_any(text, &["REASONING", "推理"]);
    parsed.personal_note = extract_field_any(text, &["PERSONAL_NOTE", "个人备注"]);
    parsed.execution_plan = extract_field_any(text, &["EXECUTION_PLAN", "执行计划"]);
    parsed.risk_plan = extract_field_any(text, &["RISK_PLAN", "风控计划"]);
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
    fn test_parse_cio_concentration_and_dry_powder() {
        let text = "VERDICT: ACCUMULATE\nCONFIDENCE: 0.75\nCONCENTRATION_PCT: 18.5\nDRY_POWDER_CNY: 350000\nDOMINANT_VIEW: quant\nSUGGESTED_ALLOC_CNY: 100000\nREASONING: 低位分批建仓\nPERSONAL_NOTE: 子弹充足\nEXECUTION_PLAN: pyramid\nRISK_PLAN: stop loss at -8%";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.verdict.as_deref(), Some("ACCUMULATE"));
        assert_eq!(parsed.concentration_pct, Some(18.5));
        assert_eq!(parsed.dry_powder_cny, Some(350000.0));
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

    // ── Bilingual Chinese field name tests ──────────────────────────────

    #[test]
    fn test_parse_macro_chinese_fields() {
        let text = "市场处于上升趋势\n信号: risk_on\n强度: 7";
        let parsed = parse_role_output(CommitteeRole::Macro, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("risk_on"));
        assert_eq!(parsed.strength, Some(7.0));
    }

    #[test]
    fn test_parse_quant_r1_chinese_fields() {
        let text = "市场状态: bull\n信号: risk_on\n强度: 8\n关键数据:\n - PE=13.5\n - 北向资金净流入120亿\n一句话: 技术面偏多";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.regime.as_deref(), Some("bull"));
        assert_eq!(parsed.signal.as_deref(), Some("risk_on"));
        assert_eq!(parsed.strength, Some(8.0));
        assert_eq!(
            parsed.key_data.as_deref(),
            Some(&["PE=13.5".to_string(), "北向资金净流入120亿".to_string()][..])
        );
        assert_eq!(parsed.one_liner.as_deref(), Some("技术面偏多"));
    }

    #[test]
    fn test_parse_quant_r2_chinese_fields() {
        let text = "调整信号: risk_off\n调整强度: 4\n保护触发: true\n推理: 回调信号增强";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("risk_off"));
        assert_eq!(parsed.strength, Some(4.0));
        assert_eq!(parsed.regime_protection_triggered, Some(true));
        assert_eq!(parsed.reasoning.as_deref(), Some("回调信号增强"));
    }

    #[test]
    fn test_parse_risk_r1_chinese_fields() {
        let text = "信号: risk_on\n强度: 6\n盈亏比: 3.5\n最大回撤: -12\n一句话: 风险可控\n集中度: 35\n可用子弹: 50000";
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
    fn test_parse_risk_r2_chinese_fields() {
        let text = "调整信号: risk_off\n调整止损: 0.92\n推理: 下行保护触发\n集中度: 30\n可用子弹: 60000";
        let parsed = parse_role_output(CommitteeRole::Risk, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("risk_off"));
        assert_eq!(parsed.adjusted_stop_loss.as_deref(), Some("0.92"));
        assert_eq!(parsed.reasoning.as_deref(), Some("下行保护触发"));
        assert_eq!(parsed.concentration_pct, Some(30.0));
        assert_eq!(parsed.dry_powder_cny, Some(60000.0));
    }

    #[test]
    fn test_parse_cio_chinese_fields() {
        let text = "裁决: 持有\n置信度: 0.6\n集中度: 25\n可用子弹: 400000\n主流观点: 震荡市观望\n建议配置: 200000\n推理: 等待数据确认\n个人备注: 等待确认\n执行计划: 无操作\n风控计划: 维持现有仓位";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.verdict.as_deref(), Some("HOLD"));
        assert_eq!(parsed.confidence, Some(0.6));
        assert_eq!(parsed.concentration_pct, Some(25.0));
        assert_eq!(parsed.dry_powder_cny, Some(400000.0));
        assert_eq!(parsed.dominant_view.as_deref(), Some("震荡市观望"));
        assert_eq!(parsed.suggested_alloc_cny, Some(200000.0));
        assert_eq!(parsed.reasoning.as_deref(), Some("等待数据确认"));
        assert_eq!(parsed.personal_note.as_deref(), Some("等待确认"));
        assert_eq!(parsed.execution_plan.as_deref(), Some("无操作"));
        assert_eq!(parsed.risk_plan.as_deref(), Some("维持现有仓位"));
    }

    #[test]
    fn test_parse_cio_chinese_verdict_variants() {
        // 买入
        let text = "裁决: 买入\n置信度: 0.9";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.verdict.as_deref(), Some("BUY"));

        // 加仓
        let text = "裁决: 加仓\n置信度: 0.8";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.verdict.as_deref(), Some("ACCUMULATE"));

        // 减仓
        let text = "裁决: 减仓\n置信度: 0.7";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.verdict.as_deref(), Some("TRIM"));

        // 卖出
        let text = "裁决: 卖出\n置信度: 0.6";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.verdict.as_deref(), Some("SELL"));
    }

    #[test]
    fn test_parse_mixed_english_chinese_fields() {
        // Mix of English and Chinese keys in the same text
        let text = "SIGNAL: risk_on\n强度: 5\nKEY_DATA:\n - test\n一句话: mixed test";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("risk_on"));
        assert_eq!(parsed.strength, Some(5.0));
        assert_eq!(parsed.key_data.as_deref(), Some(&["test".to_string()][..]));
        assert_eq!(parsed.one_liner.as_deref(), Some("mixed test"));
    }
}
