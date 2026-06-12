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

    // -- Macro-specific (new fields) --
    /// 市场阶段: "主升" | "分歧" | "退潮" | "冰点" | "混沌"
    pub market_phase: Option<String>,
    /// 宏观敏感度: "positive" | "negative" | "neutral"
    pub sensitivity: Option<String>,
    /// 敏感度原因
    pub sensitivity_reason: Option<String>,
    /// 情绪温度: "乐观" | "中性" | "谨慎" | "恐慌"
    pub emotion_temperature: Option<String>,

    // -- Quant-specific (new fields) --
    /// 资金流向原始文本
    pub money_flow: Option<String>,
    /// 买点评估: "低吸" | "突破" | "回踩" | "追高" | "不可交易"
    pub buy_point_assessment: Option<String>,
    /// 估值评估原始文本
    pub valuation_assessment: Option<String>,

    // -- Risk-specific (new fields) --
    /// L4 否决: true=卫语句触发
    pub l4_veto: Option<bool>,
    /// 否决原因
    pub l4_veto_reason: Option<String>,
    /// 情绪状态: "stable" | "warning" | "danger"
    pub emotional_state: Option<String>,
    /// 标的风险综合
    pub stock_risk_summary: Option<String>,
    /// Risk R2: L4 否决复检结果
    pub l4_veto_r2: Option<bool>,
    /// Risk R2: 情绪重校准
    pub emotion_recalibrated: Option<String>,

    // -- L4 Officer-specific (新角色) --
    /// 卫语句判定
    pub l4_guard_clause: Option<bool>,
    /// 卫语句原因
    pub l4_guard_reason: Option<String>,
    /// L4 情绪评估: "stable" | "warning" | "danger"
    pub l4_emotion_assessment: Option<String>,
    /// 行为红灯: "green" | "yellow" | "red"
    pub l4_red_light: Option<String>,
    /// 买点合理性
    pub l4_buy_point_ok: Option<bool>,

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
    /// 催化剂层级: "Tier1" | "Tier2" | "Tier3" | "无"
    pub catalyst_tier: Option<String>,
    /// 一句话催化剂摘要
    pub catalyst_summary: Option<String>,
    /// L4 检查: 止损明确
    pub l4_check_stop_loss: Option<bool>,
    /// L4 检查: 仓位合理
    pub l4_check_position: Option<bool>,
    /// L4 检查: 情绪稳定
    pub l4_check_emotion: Option<bool>,
    /// L4 检查: 买点合理
    pub l4_check_buy_point: Option<bool>,
    /// L4 执行检查通过数 (0-4)，Rust 端计算
    pub l4_execution_checks_passed: Option<f64>,
    /// 是否 Tier1 催化剂
    pub is_tier1: Option<bool>,
    /// Tier1 观察时长（小时）
    pub tier1_watch_hours: Option<f64>,

    // -- L4 行为红灯 (Rust 端计算) --
    /// 行为红灯评分 (0-30)
    pub execution_red_light_score: Option<f64>,
    /// 行为红灯: score >= 20 为 true
    pub red_light: Option<bool>,

    // -- Meta --
    /// Whether output was truncated by hard limit
    pub truncated: bool,
    /// Reason for fallback if parsing detected missing critical fields
    pub fallback_reason: Option<String>,
    /// Raw text (preserved for archiving)
    pub raw_text: String,
}

impl ParsedFields {
    /// 行为红灯是否触发（从评分派生）
    pub fn is_red_light(&self) -> Option<bool> {
        self.execution_red_light_score.map(|s| s >= 20.0)
    }
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
        CommitteeRole::L4Officer => parse_l4_officer(text, truncated, &mut parsed),
    }

    parsed
}

/// Detect fallback reason based on role-specific critical fields.
/// Returns Some(reason) if the parsed output is missing essential data.
pub fn detect_fallback_reason(role: CommitteeRole, parsed: &ParsedFields) -> Option<String> {
    // Check for WORKER_UNAVAILABLE marker in raw text
    if parsed.raw_text.contains("[WORKER_UNAVAILABLE]") {
        return Some("worker_unavailable".to_string());
    }

    // Check for empty text
    if parsed.raw_text.trim().is_empty() {
        return Some("empty_text".to_string());
    }

    // Role-specific critical field checks
    let missing = match role {
        CommitteeRole::Macro => parsed.signal.is_none(),
        CommitteeRole::Quant => parsed.signal.is_none() && parsed.regime.is_none(),
        CommitteeRole::Risk => parsed.signal.is_none(),
        CommitteeRole::Cio => parsed.verdict.is_none(),
        CommitteeRole::L4Officer => parsed.l4_guard_clause.is_none(),
    };

    if missing {
        Some("missing_critical_fields".to_string())
    } else {
        None
    }
}

fn extract_field(text: &str, key: &str) -> Option<String> {
    let colon_fmt = format!("{}:", key);
    let cn_colon_fmt = format!("{}：", key);
    let bold_colon_fmt = format!("**{}**:", key);
    let bold_cn_colon_fmt = format!("**{}**：", key);
    let equals_fmt = format!("{}=", key);
    let bold_equals_fmt = format!("**{}**=", key);

    for line in text.lines() {
        let line = line.trim();
        // 1. **KEY**: value or **KEY**：value (bold + colon variants)
        if let Some(rest) = line.strip_prefix(&bold_colon_fmt) {
            return Some(rest.trim().to_string());
        }
        if let Some(rest) = line.strip_prefix(&bold_cn_colon_fmt) {
            return Some(rest.trim().to_string());
        }
        // 2. **KEY**=value (bold + equals)
        if let Some(rest) = line.strip_prefix(&bold_equals_fmt) {
            return Some(rest.trim().to_string());
        }
        // 3. KEY: value (English colon)
        if let Some(rest) = line.strip_prefix(&colon_fmt) {
            return Some(rest.trim().to_string());
        }
        // 4. KEY：value (Chinese colon)
        if let Some(rest) = line.strip_prefix(&cn_colon_fmt) {
            return Some(rest.trim().to_string());
        }
        // 5. KEY=value (equals, no colon)
        if let Some(rest) = line.strip_prefix(&equals_fmt) {
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
    // Use extract_field to check if any key exists (handles all 6 format variants)
    let found = keys.iter().any(|key| extract_field(text, key).is_some());
    if !found {
        return None;
    }

    // Find the key line and collect subsequent list items
    let mut items = Vec::new();
    let mut found_key_line = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !found_key_line {
            if keys.iter().any(|key| {
                // Check all format variants that extract_field supports
                let colon_fmt = format!("{}:", key);
                let cn_colon_fmt = format!("{}：", key);
                let bold_colon_fmt = format!("**{}**:", key);
                let bold_cn_colon_fmt = format!("**{}**：", key);
                let equals_fmt = format!("{}=", key);
                let bold_equals_fmt = format!("**{}**=", key);
                trimmed.strip_prefix(&bold_colon_fmt).is_some()
                    || trimmed.strip_prefix(&bold_cn_colon_fmt).is_some()
                    || trimmed.strip_prefix(&bold_equals_fmt).is_some()
                    || trimmed.strip_prefix(&colon_fmt).is_some()
                    || trimmed.strip_prefix(&cn_colon_fmt).is_some()
                    || trimmed.strip_prefix(&equals_fmt).is_some()
            }) {
                found_key_line = true;
                continue;
            }
        } else {
            if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("— ")) {
                let item = item.trim().to_string();
                if !item.is_empty() {
                    items.push(item);
                }
            } else if trimmed.is_empty() || trimmed.contains(':') || trimmed.contains('：') {
                break;
            }
        }
    }
    Some(items)
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
    // 市场阶段: "主升" | "分歧" | "退潮" | "冰点" | "混沌"
    parsed.market_phase = extract_field_any(text, &["MARKET_PHASE", "市场阶段"]);
    // 宏观敏感度: "positive" | "negative" | "neutral"
    parsed.sensitivity = extract_field_any(text, &["SENSITIVITY", "敏感度"]);
    // 敏感度原因
    parsed.sensitivity_reason = extract_field_any(text, &["SENSITIVITY_REASON", "敏感度原因"]);
    // 情绪温度: "乐观" | "中性" | "谨慎" | "恐慌"
    parsed.emotion_temperature =
        extract_field_any(text, &["EMOTION_TEMPERATURE", "情绪温度"]);
}

fn parse_quant(text: &str, parsed: &mut ParsedFields) {
    // R1 fields
    parsed.regime = extract_field_any(text, &["REGIME", "市场状态"]);
    parsed.signal = extract_field_any(text, &["SIGNAL", "信号"]);
    parsed.strength = extract_f64_any(text, &["STRENGTH", "强度"]);
    parsed.key_data = extract_list_field_any(text, &["KEY_DATA", "关键数据"]);
    parsed.one_liner = extract_field_any(text, &["ONE_LINER", "一句话"]);
    // 资金流向原始文本
    parsed.money_flow = extract_field_any(text, &["MONEY_FLOW", "资金流向"]);
    // 买点评估: "低吸" | "突破" | "回踩" | "追高" | "不可交易"
    parsed.buy_point_assessment =
        extract_field_any(text, &["BUY_POINT_ASSESSMENT", "买点评估"]);
    // 估值评估原始文本
    parsed.valuation_assessment =
        extract_field_any(text, &["VALUATION_ASSESSMENT", "估值评估"]);
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
    // L4 否决: true=卫语句触发
    parsed.l4_veto = extract_bool_any(text, &["L4否决", "L4_VETO"]);
    // 否决原因
    parsed.l4_veto_reason = extract_field_any(text, &["否决原因", "L4_VETO_REASON"]);
    // 情绪状态: "stable" | "warning" | "danger"
    parsed.emotional_state = extract_field_any(text, &["情绪状态", "EMOTIONAL_STATE"]);
    // 标的风险综合
    parsed.stock_risk_summary = extract_field_any(text, &["标的风险", "STOCK_RISK"]);
    // R2 fields — R2 overrides R1 where applicable
    apply_r2_signal_override(parsed, text);
    parsed.adjusted_stop_loss = extract_field_any(text, &["ADJUSTED_STOP_LOSS", "调整止损"]);
    parsed.reasoning = extract_field_any(text, &["REASONING", "推理"]);
    // R2: L4 否决复检结果
    parsed.l4_veto_r2 = extract_bool_any(text, &["L4否决复检", "L4_VETO_R2"]);
    // R2: 情绪重校准
    parsed.emotion_recalibrated =
        extract_field_any(text, &["情绪重校准", "EMOTION_RECALIBRATED"]);
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
    // 催化剂层级: "Tier1" | "Tier2" | "Tier3" | "无"
    parsed.catalyst_tier = extract_field_any(text, &["CATALYST_TIER", "催化剂层级"]);
    // 一句话催化剂摘要
    parsed.catalyst_summary = extract_field_any(text, &["CATALYST_SUMMARY", "催化剂摘要"]);
    // L4 检查: 止损明确
    parsed.l4_check_stop_loss =
        extract_bool_any(text, &["STOP_LOSS_CLEAR", "止损明确"]);
    // L4 检查: 仓位合理
    parsed.l4_check_position = extract_bool_any(text, &["POSITION_OK", "仓位合理"]);
    // L4 检查: 情绪稳定
    parsed.l4_check_emotion =
        extract_bool_any(text, &["EMOTION_STABLE", "情绪稳定"]);
    // L4 检查: 买点合理
    parsed.l4_check_buy_point =
        extract_bool_any(text, &["BUY_POINT_OK", "买点合理"]);
    // 是否 Tier1 催化剂
    parsed.is_tier1 = extract_bool_any(text, &["IS_TIER1"]);
    // Tier1 观察时长（小时）
    parsed.tier1_watch_hours = extract_f64_any(text, &["TIER1_WATCH_HOURS"]);
    // L4 执行检查通过数 (0-4)，Rust 端计算
    let checks = [
        parsed.l4_check_stop_loss.unwrap_or(false),
        parsed.l4_check_position.unwrap_or(false),
        parsed.l4_check_emotion.unwrap_or(false),
        parsed.l4_check_buy_point.unwrap_or(false),
    ];
    let passed = checks.iter().filter(|&&b| b).count() as f64;
    parsed.l4_execution_checks_passed = Some(passed);
}

/// 解析 L4 行为官输出，提取行为健康度字段。
fn parse_l4_officer(text: &str, _truncated: bool, parsed: &mut ParsedFields) {
    // 卫语句判定
    parsed.l4_guard_clause = extract_bool_any(text, &["卫语句", "GUARD_CLAUSE"]);
    // 卫语句原因
    parsed.l4_guard_reason = extract_field_any(text, &["卫语句原因", "GUARD_REASON"]);
    // 情绪评估: "stable" | "warning" | "danger"
    parsed.l4_emotion_assessment =
        extract_field_any(text, &["情绪评估", "EMOTION"]);
    // 行为红灯: "green" | "yellow" | "red"
    parsed.l4_red_light = extract_field_any(text, &["行为红灯", "RED_LIGHT"]);
    // 买点合理性
    parsed.l4_buy_point_ok = extract_bool_any(text, &["买点合理", "BUY_POINT_OK"]);
    // 推理
    parsed.reasoning = extract_field_any(text, &["推理", "REASONING"]);
    // L4 Officer 也输出信号和强度（兼容）
    parsed.signal = extract_field_any(text, &["SIGNAL", "信号"]);
    parsed.strength = extract_f64_any(text, &["STRENGTH", "强度"]);
}

/// 计算 L4 行为红灯评分（Rust 端确定性计算）
///
/// 评分规则：
/// - c_score: emotional_state → stable=0, warning=5, danger=10, 其他=3
/// - k_score: 集中度/子弹 → concentration>60%=10, >40%=6, dry_powder<1000=8, 其他=2
/// - l_score: 近7天交易次数 → >=5=10, >=3=5, <3=0
/// - score = c_score + k_score + l_score (0-30)
/// - green: 0-10, yellow: 11-20, red: 21-30
pub fn compute_red_light_score(
    emotional_state: &str,
    concentration_pct: f64,
    dry_powder_cny: f64,
    recent_trade_count_7d: i64,
) -> (f64, String) {
    let c_score = match emotional_state {
        "stable" => 0.0,
        "warning" => 5.0,
        "danger" => 10.0,
        _ => 3.0,
    };

    let k_score = if concentration_pct > 60.0 {
        10.0
    } else if concentration_pct > 40.0 {
        6.0
    } else if dry_powder_cny < 1000.0 {
        8.0
    } else {
        2.0
    };

    let l_score = if recent_trade_count_7d >= 5 {
        10.0
    } else if recent_trade_count_7d >= 3 {
        5.0
    } else {
        0.0
    };

    let score = c_score + k_score + l_score;
    let level = if score <= 10.0 {
        "green".to_string()
    } else if score <= 20.0 {
        "yellow".to_string()
    } else {
        "red".to_string()
    };

    (score, level)
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

    // ── 新增字段解析测试 ─────────────────────────────────────────────

    #[test]
    fn test_parse_macro_new_fields() {
        let text = "SIGNAL: risk_on\nSTRENGTH: 7\n市场阶段: 主升\n敏感度: positive\n敏感度原因: 北向资金持续流入\n情绪温度: 乐观";
        let parsed = parse_role_output(CommitteeRole::Macro, text, false);
        assert_eq!(parsed.market_phase.as_deref(), Some("主升"));
        assert_eq!(parsed.sensitivity.as_deref(), Some("positive"));
        assert_eq!(parsed.sensitivity_reason.as_deref(), Some("北向资金持续流入"));
        assert_eq!(parsed.emotion_temperature.as_deref(), Some("乐观"));
    }

    #[test]
    fn test_parse_macro_new_fields_english() {
        let text = "SIGNAL: risk_off\nSTRENGTH: 3\nMARKET_PHASE: 退潮\nSENSITIVITY: negative\nSENSITIVITY_REASON: trade war\nEMOTION_TEMPERATURE: 恐慌";
        let parsed = parse_role_output(CommitteeRole::Macro, text, false);
        assert_eq!(parsed.market_phase.as_deref(), Some("退潮"));
        assert_eq!(parsed.sensitivity.as_deref(), Some("negative"));
        assert_eq!(parsed.sensitivity_reason.as_deref(), Some("trade war"));
        assert_eq!(parsed.emotion_temperature.as_deref(), Some("恐慌"));
    }

    #[test]
    fn test_parse_quant_new_fields() {
        let text = "REGIME: bull\nSIGNAL: bullish\nSTRENGTH: 7\n资金流向: 北向净流入\n买点评估: 低吸\n估值评估: PE 13.5 偏低";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.money_flow.as_deref(), Some("北向净流入"));
        assert_eq!(parsed.buy_point_assessment.as_deref(), Some("低吸"));
        assert_eq!(parsed.valuation_assessment.as_deref(), Some("PE 13.5 偏低"));
    }

    #[test]
    fn test_parse_risk_new_fields() {
        let text = "SIGNAL: ok\nSTRENGTH: 5\nL4否决: true\n否决原因: 集中度过高\n情绪状态: warning\n标的风险: 短期波动加剧";
        let parsed = parse_role_output(CommitteeRole::Risk, text, false);
        assert_eq!(parsed.l4_veto, Some(true));
        assert_eq!(parsed.l4_veto_reason.as_deref(), Some("集中度过高"));
        assert_eq!(parsed.emotional_state.as_deref(), Some("warning"));
        assert_eq!(parsed.stock_risk_summary.as_deref(), Some("短期波动加剧"));
    }

    #[test]
    fn test_parse_risk_r2_new_fields() {
        let text = "ADJUSTED_SIGNAL: concerned\nADJUSTED_STOP_LOSS: 0.90\nL4否决复检: true\n情绪重校准: stable\nREASONING: 确认否决";
        let parsed = parse_role_output(CommitteeRole::Risk, text, false);
        assert_eq!(parsed.l4_veto_r2, Some(true));
        assert_eq!(parsed.emotion_recalibrated.as_deref(), Some("stable"));
    }

    #[test]
    fn test_parse_cio_new_fields() {
        let text = "VERDICT: ACCUMULATE\nCONFIDENCE: 0.75\nCATALYST_TIER: Tier1\nCATALYST_SUMMARY: 政策利好\nSTOP_LOSS_CLEAR: true\nPOSITION_OK: true\nEMOTION_STABLE: true\nBUY_POINT_OK: false\nIS_TIER1: true\nTIER1_WATCH_HOURS: 48";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.catalyst_tier.as_deref(), Some("Tier1"));
        assert_eq!(parsed.catalyst_summary.as_deref(), Some("政策利好"));
        assert_eq!(parsed.l4_check_stop_loss, Some(true));
        assert_eq!(parsed.l4_check_position, Some(true));
        assert_eq!(parsed.l4_check_emotion, Some(true));
        assert_eq!(parsed.l4_check_buy_point, Some(false));
        assert_eq!(parsed.l4_execution_checks_passed, Some(3.0));
        assert_eq!(parsed.is_tier1, Some(true));
        assert_eq!(parsed.tier1_watch_hours, Some(48.0));
    }

    #[test]
    fn test_parse_l4_officer() {
        let text = "卫语句: yes\n卫语句原因: 止损线触发\n情绪评估: warning\n行为红灯: yellow\n买点合理: yes\n推理: 行为基本健康但需关注";
        let parsed = parse_role_output(CommitteeRole::L4Officer, text, false);
        assert_eq!(parsed.l4_guard_clause, Some(true));
        assert_eq!(parsed.l4_guard_reason.as_deref(), Some("止损线触发"));
        assert_eq!(parsed.l4_emotion_assessment.as_deref(), Some("warning"));
        assert_eq!(parsed.l4_red_light.as_deref(), Some("yellow"));
        assert_eq!(parsed.l4_buy_point_ok, Some(true));
        assert_eq!(
            parsed.reasoning.as_deref(),
            Some("行为基本健康但需关注")
        );
    }

    #[test]
    fn test_parse_l4_officer_english() {
        let text = "GUARD_CLAUSE: false\nGUARD_REASON: N/A\nEMOTION: stable\nRED_LIGHT: green\nBUY_POINT_OK: true\nREASONING: all clear";
        let parsed = parse_role_output(CommitteeRole::L4Officer, text, false);
        assert_eq!(parsed.l4_guard_clause, Some(false));
        assert_eq!(parsed.l4_emotion_assessment.as_deref(), Some("stable"));
        assert_eq!(parsed.l4_red_light.as_deref(), Some("green"));
    }

    #[test]
    fn test_compute_red_light_score_green() {
        let (score, level) = super::compute_red_light_score("stable", 20.0, 50000.0, 1);
        assert_eq!(score, 2.0); // c=0 + k=2 + l=0
        assert_eq!(level, "green");
    }

    #[test]
    fn test_compute_red_light_score_yellow() {
        let (score, level) = super::compute_red_light_score("warning", 50.0, 5000.0, 3);
        assert_eq!(score, 16.0); // c=5 + k=6 + l=5
        assert_eq!(level, "yellow");
    }

    #[test]
    fn test_compute_red_light_score_red() {
        let (score, level) = super::compute_red_light_score("danger", 70.0, 500.0, 5);
        assert_eq!(score, 28.0); // c=10 + k=10 + l=10
        assert_eq!(level, "red");
    }

    #[test]
    fn test_compute_red_light_score_unknown_emotion() {
        let (score, level) = super::compute_red_light_score("unknown", 30.0, 20000.0, 0);
        assert_eq!(score, 5.0); // c=3 + k=2 + l=0
        assert_eq!(level, "green");
    }

    #[test]
    fn test_compute_red_light_score_high_concentration() {
        // concentration > 40% but <= 60%
        let (score, level) = super::compute_red_light_score("stable", 45.0, 10000.0, 0);
        assert_eq!(score, 6.0); // c=0 + k=6 + l=0
        assert_eq!(level, "green");
    }

    #[test]
    fn test_compute_red_light_score_low_dry_powder() {
        // concentration <= 40% but dry_powder < 1000
        let (score, level) = super::compute_red_light_score("stable", 30.0, 500.0, 0);
        assert_eq!(score, 8.0); // c=0 + k=8 + l=0
        assert_eq!(level, "green");
    }

    #[test]
    fn test_cio_execution_checks_all_pass() {
        let text = "VERDICT: BUY\nCONFIDENCE: 0.9\nSTOP_LOSS_CLEAR: yes\nPOSITION_OK: yes\nEMOTION_STABLE: yes\nBUY_POINT_OK: yes";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.l4_execution_checks_passed, Some(4.0));
    }

    #[test]
    fn test_cio_execution_checks_none_pass() {
        let text = "VERDICT: HOLD\nCONFIDENCE: 0.3\nSTOP_LOSS_CLEAR: no\nPOSITION_OK: no\nEMOTION_STABLE: no\nBUY_POINT_OK: no";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.l4_execution_checks_passed, Some(0.0));
    }

    // ── Task 1: Flexible format variant tests ──────────────────────────

    #[test]
    fn test_extract_field_equals_format() {
        // Format: KEY=value (no colon)
        let text = "SIGNAL=risk_on\nSTRENGTH=7";
        assert_eq!(extract_field(text, "SIGNAL"), Some("risk_on".to_string()));
        assert_eq!(extract_field(text, "STRENGTH"), Some("7".to_string()));
    }

    #[test]
    fn test_extract_field_colon_space_format() {
        // Format: KEY: value (colon + space)
        let text = "SIGNAL: risk_on";
        assert_eq!(extract_field(text, "SIGNAL"), Some("risk_on".to_string()));
    }

    #[test]
    fn test_extract_field_chinese_colon_space_format() {
        // Format: KEY：value (Chinese colon, no space before value)
        let text = "VERDICT：BUY";
        assert_eq!(extract_field(text, "VERDICT"), Some("BUY".to_string()));
    }

    #[test]
    fn test_extract_field_bold_asterisks_format() {
        // Format: **KEY**: value (Markdown bold)
        let text = "**SIGNAL**: risk_on\n**STRENGTH**: 8";
        assert_eq!(extract_field(text, "SIGNAL"), Some("risk_on".to_string()));
        assert_eq!(extract_field(text, "STRENGTH"), Some("8".to_string()));
    }

    #[test]
    fn test_extract_field_bold_asterisks_equals_format() {
        // Format: **KEY**=value (Markdown bold + equals)
        let text = "**SIGNAL**=risk_on";
        assert_eq!(extract_field(text, "SIGNAL"), Some("risk_on".to_string()));
    }

    #[test]
    fn test_extract_field_colon_no_space_format() {
        // Format: KEY:value (colon, no space)
        let text = "SIGNAL:risk_on";
        assert_eq!(extract_field(text, "SIGNAL"), Some("risk_on".to_string()));
    }

    // ── Task 2: List field format variants ─────────────────────────────

    #[test]
    fn test_extract_list_field_equals_format() {
        // KEY_DATA=value1;value2 won't work for lists, but the key detection should still work
        // The list items follow on subsequent lines with `- ` prefix
        let text = "KEY_DATA=\n- PE=13.5\n- 北向资金净流入120亿";
        let result = extract_list_field_any(text, &["KEY_DATA"]);
        assert_eq!(
            result,
            Some(vec!["PE=13.5".to_string(), "北向资金净流入120亿".to_string()])
        );
    }

    #[test]
    fn test_extract_list_field_bold_colon_format() {
        let text = "**KEY_DATA**:\n- PE=13.5\n- volume up";
        let result = extract_list_field_any(text, &["KEY_DATA"]);
        assert_eq!(
            result,
            Some(vec!["PE=13.5".to_string(), "volume up".to_string()])
        );
    }

    #[test]
    fn test_extract_list_field_bold_equals_format() {
        let text = "**KEY_DATA**=\n- item1\n- item2";
        let result = extract_list_field_any(text, &["KEY_DATA"]);
        assert_eq!(
            result,
            Some(vec!["item1".to_string(), "item2".to_string()])
        );
    }

    // ── Task 3: Fallback reason detection ──────────────────────────────

    #[test]
    fn test_detect_fallback_unavailable_marker() {
        let parsed = ParsedFields {
            raw_text: "[WORKER_UNAVAILABLE] LLM call failed".to_string(),
            ..Default::default()
        };
        assert_eq!(
            detect_fallback_reason(CommitteeRole::Macro, &parsed),
            Some("worker_unavailable".to_string())
        );
    }

    #[test]
    fn test_detect_fallback_macro_missing_signal() {
        let parsed = ParsedFields {
            signal: None,
            raw_text: "市场分析报告...".to_string(),
            ..Default::default()
        };
        assert_eq!(
            detect_fallback_reason(CommitteeRole::Macro, &parsed),
            Some("missing_critical_fields".to_string())
        );
    }

    #[test]
    fn test_detect_fallback_quant_missing_signal_regime() {
        let parsed = ParsedFields {
            signal: None,
            regime: None,
            raw_text: "some analysis".to_string(),
            ..Default::default()
        };
        assert_eq!(
            detect_fallback_reason(CommitteeRole::Quant, &parsed),
            Some("missing_critical_fields".to_string())
        );
    }

    #[test]
    fn test_detect_fallback_risk_missing_signal() {
        let parsed = ParsedFields {
            signal: None,
            raw_text: "risk analysis".to_string(),
            ..Default::default()
        };
        assert_eq!(
            detect_fallback_reason(CommitteeRole::Risk, &parsed),
            Some("missing_critical_fields".to_string())
        );
    }

    #[test]
    fn test_detect_fallback_cio_missing_verdict() {
        let parsed = ParsedFields {
            verdict: None,
            raw_text: "cio analysis".to_string(),
            ..Default::default()
        };
        assert_eq!(
            detect_fallback_reason(CommitteeRole::Cio, &parsed),
            Some("missing_critical_fields".to_string())
        );
    }

    #[test]
    fn test_detect_fallback_l4_missing_guard() {
        let parsed = ParsedFields {
            l4_guard_clause: None,
            raw_text: "l4 analysis".to_string(),
            ..Default::default()
        };
        assert_eq!(
            detect_fallback_reason(CommitteeRole::L4Officer, &parsed),
            Some("missing_critical_fields".to_string())
        );
    }

    #[test]
    fn test_detect_fallback_empty_text() {
        let parsed = ParsedFields {
            raw_text: "".to_string(),
            ..Default::default()
        };
        assert_eq!(
            detect_fallback_reason(CommitteeRole::Macro, &parsed),
            Some("empty_text".to_string())
        );
    }

    #[test]
    fn test_detect_fallback_none_when_ok() {
        let parsed = ParsedFields {
            signal: Some("risk_on".to_string()),
            raw_text: "SIGNAL: risk_on\nSTRENGTH: 7".to_string(),
            ..Default::default()
        };
        assert_eq!(detect_fallback_reason(CommitteeRole::Macro, &parsed), None);
    }
}
