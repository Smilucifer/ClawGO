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
    /// Macro: 信号理由(一句话)
    pub signal_reason: Option<String>,
    /// Macro: 市场阶段理由(一句话)
    pub market_phase_reason: Option<String>,

    // -- Quant-specific (new fields) --
    /// 资金流向原始文本
    pub money_flow: Option<String>,
    /// 买点评估: "低吸" | "突破" | "回踩" | "追高" | "不可交易"
    pub buy_point_assessment: Option<String>,
    /// Quant R1: 进场价(结构化点位,供复盘)
    pub entry_price: Option<f64>,
    /// Quant R1: 目标价(结构化点位,供复盘)
    pub target_price: Option<f64>,
    /// 估值评估原始文本
    pub valuation_assessment: Option<String>,

    // -- Risk-specific (new fields) --
    /// 标的风险综合
    pub stock_risk_summary: Option<String>,

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
    /// CIO: 执行模式 lump-sum | pyramid | grid | none
    pub execution_mode: Option<String>,
    /// CIO: 首笔金额
    pub first_tranche_cny: Option<f64>,
    /// CIO: risk plan
    pub risk_plan: Option<String>,
    /// CIO: 止损价(结构化数字,与自由文本 adjusted_stop_loss 并存)
    pub stop_loss_price: Option<f64>,
    /// 催化剂层级: "Tier1" | "Tier2" | "Tier3" | "无"
    pub catalyst_tier: Option<String>,
    /// 一句话催化剂摘要
    pub catalyst_summary: Option<String>,
    /// 是否 Tier1 催化剂
    pub is_tier1: Option<bool>,
    /// Tier1 观察时长（小时）
    pub tier1_watch_hours: Option<f64>,

    // -- Meta --
    /// Whether output was truncated by hard limit
    pub truncated: bool,
    /// Reason for fallback if parsing detected missing critical fields
    pub fallback_reason: Option<String>,
    /// Raw text (preserved for archiving)
    pub raw_text: String,
}

// ---------------------------------------------------------------------------
// Parser functions
// ---------------------------------------------------------------------------

/// Parse LLM output for any role into structured fields.
pub fn parse_role_output(role: CommitteeRole, text: &str, truncated: bool) -> ParsedFields {
    // 预处理：合并多行续行，确保长值不被换行截断
    let merged = merge_continuation_lines(text);
    let mut parsed = ParsedFields {
        raw_text: text.to_string(),
        truncated,
        ..Default::default()
    };

    match role {
        CommitteeRole::Macro => parse_macro(&merged, &mut parsed),
        CommitteeRole::Quant => parse_quant(&merged, &mut parsed),
        CommitteeRole::Risk => parse_risk(&merged, &mut parsed),
        CommitteeRole::Cio => parse_cio(&merged, &mut parsed),
        CommitteeRole::L4Officer => { /* L4 removed — no-op */ },
    }

    parsed
}

/// Returns `true` if the option is `None` or contains only whitespace.
fn is_blank(opt: &Option<String>) -> bool {
    opt.as_deref().map_or(true, |v| v.trim().is_empty())
}

/// Detect fallback reason based on role-specific critical fields.
/// Returns Some(reason) if the parsed output is missing essential data.
///
/// `round` distinguishes R1 vs R2 validation requirements — e.g., Quant R2 does
/// not output REGIME, so checking `parsed.regime` for R2 would be a false positive.
pub fn detect_fallback_reason(role: CommitteeRole, round: u8, parsed: &ParsedFields) -> Option<String> {
    // Check for WORKER_UNAVAILABLE marker in raw text
    if parsed.raw_text.contains("[WORKER_UNAVAILABLE]") {
        return Some("worker_unavailable".to_string());
    }

    // Check for empty text
    if parsed.raw_text.trim().is_empty() {
        return Some("empty_text".to_string());
    }

    // Role-specific critical field checks (None or empty string = missing)
    let missing = match role {
        CommitteeRole::Macro => is_blank(&parsed.signal),
        CommitteeRole::Quant => {
            if round >= 2 {
                // R2 only outputs adjusted signal + protection trigger; REGIME is not required
                is_blank(&parsed.signal)
            } else {
                is_blank(&parsed.signal) || is_blank(&parsed.regime)
            }
        }
        CommitteeRole::Risk => is_blank(&parsed.signal),
        CommitteeRole::Cio => is_blank(&parsed.verdict),
        CommitteeRole::L4Officer => return Some("l4_removed".to_string()),
    };

    if missing {
        Some("missing_critical_fields".to_string())
    } else {
        None
    }
}

/// 归一化一行以便 key 检测:剥离行首列表/引用前缀,以及整体包裹的成对 `**`。
/// 不改变字段值本身的内部内容(strip_markdown_formatting 仍在值阶段处理)。
fn normalize_key_line(line: &str) -> String {
    let mut s = line.trim();
    // 剥列表/引用前缀(可能叠加,如 "- > "):循环剥一层
    loop {
        let stripped = s
            .strip_prefix("- ")
            .or_else(|| s.strip_prefix("* "))
            .or_else(|| s.strip_prefix("+ "))
            .or_else(|| s.strip_prefix("> "));
        match stripped {
            Some(rest) => s = rest.trim_start(),
            None => break,
        }
    }
    // 剥有序列表前缀 "N. " / "N、"
    if let Some(pos) = s.find(['.', '、']) {
        if pos > 0 && pos <= 3 && s[..pos].chars().all(|c| c.is_ascii_digit()) {
            s = s[pos + s[pos..].chars().next().map_or(1, |c| c.len_utf8())..].trim_start();
        }
    }
    // 若行首是 `**`,寻找第一个闭合 `**`,剥掉这对包裹,让 key/冒号暴露出来。
    // 同时覆盖两种 LLM 输出形态:
    //   `**KEY**: VALUE`   —— 闭合 ** 在 key 末尾、冒号外
    //   `**KEY:** VALUE`   —— 闭合 ** 在冒号之后(行中部)
    // 仅在行首存在 `**` 时触发;不会影响值内部的 `**bold**`。
    let mut out = s.to_string();
    if out.starts_with("**") {
        if let Some(close) = out[2..].find("**") {
            let close_abs = 2 + close;
            out = format!("{}{}", &out[2..close_abs], &out[close_abs + 2..]);
        }
    }
    out
}

/// Check if a trimmed line starts with one of the 6 supported key formats.
/// Returns the value portion after the key+delimiter if matched.
pub(crate) fn matches_key_line<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let colon_fmt = format!("{}:", key);
    let cn_colon_fmt = format!("{}：", key);
    let bold_colon_fmt = format!("**{}**:", key);
    let bold_cn_colon_fmt = format!("**{}**：", key);
    let equals_fmt = format!("{}=", key);
    let bold_equals_fmt = format!("**{}**=", key);

    line.strip_prefix(&bold_colon_fmt)
        .or_else(|| line.strip_prefix(&bold_cn_colon_fmt))
        .or_else(|| line.strip_prefix(&bold_equals_fmt))
        .or_else(|| line.strip_prefix(&colon_fmt))
        .or_else(|| line.strip_prefix(&cn_colon_fmt))
        .or_else(|| line.strip_prefix(&equals_fmt))
}

/// 清理 LLM 输出中的 markdown 格式标记。
/// 移除 `**bold**`、`` `code` `` 等包裹符号，保留纯文本。
fn strip_markdown_formatting(s: &str) -> String {
    let mut out = s.to_string();
    // **bold** → bold
    loop {
        let start = match out.find("**") {
            Some(i) => i,
            None => break,
        };
        if let Some(end) = out[start + 2..].find("**") {
            let inner = out[start + 2..start + 2 + end].to_string();
            out = format!("{}{}{}", &out[..start], inner, &out[start + 2 + end + 2..]);
        } else {
            break;
        }
    }
    // `code` → code
    loop {
        let start = match out.find('`') {
            Some(i) => i,
            None => break,
        };
        if let Some(end) = out[start + 1..].find('`') {
            let inner = out[start + 1..start + 1 + end].to_string();
            out = format!("{}{}{}", &out[..start], inner, &out[start + 1 + end + 1..]);
        } else {
            break;
        }
    }
    out
}

/// 预处理文本：将多行续行合并到上一个 KEY: 行。
/// 续行定义：不以 KEY: 格式开头的非空行，拼接到前一个字段值末尾。
/// 这样 LLM 输出的长值被换行时仍能被正确提取。
fn merge_continuation_lines(text: &str) -> String {
    let mut result = String::new();
    let mut prev_was_key = false;
    for line in text.lines() {
        let trimmed = line.trim();
        // 精确检查：是否有 KEY: VALUE 模式（至少有一个字母/中文字符后跟冒号/等号）
        let is_structured_key = is_structured_key_line(trimmed);
        // 列表项（以 - 或 * 开头）不应被合并到前一个 key 的值中
        let is_list_item = trimmed.starts_with("- ") || trimmed.starts_with("* ");

        if is_structured_key {
            // 新的 key 行，正常追加（带换行）
            if !result.is_empty() && !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str(line.trim_end());
            result.push('\n');
            prev_was_key = true;
        } else if !trimmed.is_empty() && prev_was_key && !is_list_item {
            // 续行：追加到前一个 key 的值末尾（用空格分隔）
            if result.ends_with('\n') {
                result.truncate(result.len() - 1); // 移除尾部换行
            }
            result.push(' ');
            result.push_str(trimmed);
            result.push('\n');
            // prev_was_key 保持 true，允许连续续行
        } else {
            // 空行、列表项等，正常追加
            if !result.is_empty() && !result.ends_with('\n') {
                result.push('\n');
            }
            result.push_str(line.trim_end());
            result.push('\n');
            prev_was_key = false;
        }
    }
    result
}

/// 检测一行是否为结构化 KEY: VALUE 格式。
/// key 部分应为 1-30 个非空格字符，后跟中英文冒号或等号。
/// 先归一化(剥列表/引用前缀 + 行首 `**...**` 包裹),再判定 — 这样
/// `- **KEY**: v` / `**KEY: v**` / `> **KEY**: v` 等被装饰的 key 行
/// 也能被识别,避免在 `merge_continuation_lines` 中被误并到上一字段。
fn is_structured_key_line(line: &str) -> bool {
    let line = normalize_key_line(line);
    if let Some(pos) = line.find(':').or_else(|| line.find('：')).or_else(|| line.find('=')) {
        pos > 0 && pos < 30 && !line[..pos].contains(' ')
    } else {
        false
    }
}

fn extract_field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        // 行内多字段:出现 | 且**每段**都像 KEY: VALUE 时,按 | 拆分逐段优先尝试。
        // 反例守门:任一段不是结构化 KEY: VALUE 就放弃拆分(避免破坏值中含 | 的正常字段)。
        if line.contains('|') || line.contains('｜') {
            let all_structured = line
                .split(['|', '｜'])
                .all(|seg| is_structured_key_line(&normalize_key_line(seg)));
            if all_structured {
                for seg in line.split(['|', '｜']) {
                    let seg_norm = normalize_key_line(seg);
                    if let Some(rest) = matches_key_line(&seg_norm, key) {
                        return Some(strip_markdown_formatting(rest.trim()));
                    }
                }
                // 该行已按段尝试过,key 不在此行——继续看下一行,
                // 不再走整行匹配(否则会把别的字段值串当作本 key 的值)。
                continue;
            }
        }
        // 整行归一化匹配(单字段或值中含 | 但不是多字段结构)
        let normalized = normalize_key_line(line);
        if let Some(rest) = matches_key_line(&normalized, key) {
            return Some(strip_markdown_formatting(rest.trim()));
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
    extract_field(text, key).and_then(|v| parse_leading_f64(&v))
}

/// 从值串中解析前导数字:支持 "6"、"6.5"、"6/10"(取分子)、"30.5%"(去尾符)。
/// 同时容忍全角百分号 `％`、全角括号 `（` 等常见 LLM 输出后缀。
fn parse_leading_f64(v: &str) -> Option<f64> {
    let v = v.trim();
    // 在 / % ％ 空格 ( （ 等分隔符处截断,取首段作为数值候选
    let head = v
        .split(['/', '%', '％', ' ', '（', '('])
        .next()
        .unwrap_or(v)
        .trim();
    head.parse::<f64>().ok()
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
    // R2 signal keys — include common LLM output variants beyond the prompt-specified "调整信号"
    // Note: "ADJUSTED SIGNAL" (with space) is intentionally excluded — is_structured_key_line
    // rejects keys with spaces, and merge_continuation_lines would merge it into the previous
    // key's value. "ADJUSTED_SIGNAL" (underscore) covers the English variant.
    if let Some(adjusted) = extract_field_any(text, &[
        "ADJUSTED_SIGNAL", "调整信号", "调整风险信号",
        "调整后信号", "信号调整",
    ]) {
        parsed.signal = Some(adjusted);
    }
    // R2 strength keys
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
            if keys
                .iter()
                .any(|key| matches_key_line(trimmed, key).is_some())
            {
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
    parsed.signal_reason = extract_field_any(text, &["SIGNAL_REASON", "信号理由"]);
    parsed.market_phase_reason = extract_field_any(text, &["MARKET_PHASE_REASON", "市场阶段理由"]);
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
    parsed.entry_price = extract_f64_any(text, &["进场价", "ENTRY_PRICE"]);
    parsed.target_price = extract_f64_any(text, &["目标价", "TARGET_PRICE"]);
    // 估值评估原始文本
    parsed.valuation_assessment =
        extract_field_any(text, &["VALUATION_ASSESSMENT", "估值评估"]);
    // R2 fields — R2 overrides R1 where applicable
    apply_r2_signal_override(parsed, text);
    // R2 buy point override (prompt uses "调整买点", R1 uses "买点评估")
    if parsed.buy_point_assessment.is_none() {
        parsed.buy_point_assessment =
            extract_field_any(text, &["调整买点", "ADJUSTED_BUY_POINT"]);
    }
    parsed.regime_protection_triggered = extract_bool_any(text, &["REGIME_PROTECTION_TRIGGERED", "保护触发"]);
    parsed.reasoning = extract_field_any(text, &["REASONING", "推理"]);
}

fn parse_risk(text: &str, parsed: &mut ParsedFields) {
    // R1 fields — "风险信号" is the preferred key to avoid confusion with Quant/Macro signal
    parsed.signal = extract_field_any(text, &["SIGNAL", "信号", "风险信号"]);
    parsed.strength = extract_f64_any(text, &["STRENGTH", "强度"]);
    parsed.pnl_pct = extract_f64_any(text, &["PNL_PCT", "盈亏比"]);
    parsed.worst_case_loss_pct = extract_f64_any(text, &["WORST_CASE_LOSS_PCT_AT_-20", "最大回撤"]);
    parsed.one_liner = extract_field_any(text, &["ONE_LINER", "一句话"]);
    parsed.concentration_pct = extract_f64_any(text, &["CONCENTRATION_PCT", "集中度"]);
    parsed.dry_powder_cny = extract_f64_any(text, &["DRY_POWDER_CNY", "可用子弹"]);
    // 标的风险综合
    parsed.stock_risk_summary = extract_field_any(text, &["标的风险", "STOCK_RISK"]);
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
    parsed.execution_mode = extract_field_any(text, &["EXECUTION_MODE", "执行模式"]);
    parsed.first_tranche_cny = extract_f64_any(text, &["FIRST_TRANCHE_CNY", "首笔金额"]);
    parsed.risk_plan = extract_field_any(text, &["RISK_PLAN", "风控计划"]);
    parsed.stop_loss_price = extract_f64_any(text, &["止损价", "STOP_LOSS_PRICE"]);
    // 催化剂层级: "Tier1" | "Tier2" | "Tier3" | "无"
    parsed.catalyst_tier = extract_field_any(text, &["CATALYST_TIER", "催化剂层级"]);
    // 一句话催化剂摘要
    parsed.catalyst_summary = extract_field_any(text, &["CATALYST_SUMMARY", "催化剂摘要"]);
    // 是否 Tier1 催化剂
    parsed.is_tier1 = extract_bool_any(text, &["IS_TIER1"]);
    // Tier1 观察时长（小时）
    parsed.tier1_watch_hours = extract_f64_any(text, &["TIER1_WATCH_HOURS"]);
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
    fn test_parse_quant_r1_prices() {
        let text = "SIGNAL: risk_on\nSTRENGTH: 7\n进场价: 12.50\n目标价: 15.80\nONE_LINER: 技术面偏多";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.entry_price, Some(12.50));
        assert_eq!(parsed.target_price, Some(15.80));
    }

    #[test]
    fn test_parse_cio_stop_loss_price() {
        let text = "VERDICT: BUY\nCONFIDENCE: 0.7\n止损价: 11.20\n止损条件: 跌破20日线";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.stop_loss_price, Some(11.20));
    }

    #[test]
    fn test_parse_prices_absent_is_none() {
        let text = "SIGNAL: risk_on\nSTRENGTH: 7";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.entry_price, None);
        assert_eq!(parsed.target_price, None);
        assert_eq!(parsed.stop_loss_price, None);
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
        let text = "SIGNAL: ok\nSTRENGTH: 5\n情绪状态: warning\n标的风险: 短期波动加剧";
        let parsed = parse_role_output(CommitteeRole::Risk, text, false);
        assert_eq!(parsed.stock_risk_summary.as_deref(), Some("短期波动加剧"));
    }

    #[test]
    fn test_parse_risk_r2_new_fields() {
        let text = "ADJUSTED_SIGNAL: concerned\nADJUSTED_STOP_LOSS: 0.90\nREASONING: 确认否决";
        let parsed = parse_role_output(CommitteeRole::Risk, text, false);
        assert_eq!(parsed.adjusted_stop_loss.as_deref(), Some("0.90"));
    }

    #[test]
    fn test_parse_cio_new_fields() {
        let text = "VERDICT: ACCUMULATE\nCONFIDENCE: 0.75\nCATALYST_TIER: Tier1\nCATALYST_SUMMARY: 政策利好\nIS_TIER1: true\nTIER1_WATCH_HOURS: 48";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.catalyst_tier.as_deref(), Some("Tier1"));
        assert_eq!(parsed.catalyst_summary.as_deref(), Some("政策利好"));
        assert_eq!(parsed.is_tier1, Some(true));
        assert_eq!(parsed.tier1_watch_hours, Some(48.0));
    }

    // ── Task 1: Flexible format variant tests ──────────────────────────

    #[test]
    fn test_matches_key_line_all_variants() {
        assert_eq!(matches_key_line("SIGNAL: risk_on", "SIGNAL"), Some(" risk_on"));
        assert_eq!(matches_key_line("SIGNAL：risk_on", "SIGNAL"), Some("risk_on"));
        assert_eq!(matches_key_line("**SIGNAL**: risk_on", "SIGNAL"), Some(" risk_on"));
        assert_eq!(matches_key_line("**SIGNAL**：risk_on", "SIGNAL"), Some("risk_on"));
        assert_eq!(matches_key_line("SIGNAL=risk_on", "SIGNAL"), Some("risk_on"));
        assert_eq!(matches_key_line("**SIGNAL**=risk_on", "SIGNAL"), Some("risk_on"));
        assert_eq!(matches_key_line("no match here", "SIGNAL"), None);
        assert_eq!(matches_key_line("SIGNALX: risk_on", "SIGNAL"), None);
    }

    #[test]
    fn test_matches_key_line_chinese_key() {
        assert_eq!(matches_key_line("信号: risk_on", "信号"), Some(" risk_on"));
        assert_eq!(matches_key_line("**信号**：risk_on", "信号"), Some("risk_on"));
    }

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
            detect_fallback_reason(CommitteeRole::Macro, 1, &parsed),
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
            detect_fallback_reason(CommitteeRole::Macro, 1, &parsed),
            Some("missing_critical_fields".to_string())
        );
    }

    #[test]
    fn test_detect_fallback_quant_r1_missing_signal_regime() {
        let parsed = ParsedFields {
            signal: None,
            regime: None,
            raw_text: "some analysis".to_string(),
            ..Default::default()
        };
        assert_eq!(
            detect_fallback_reason(CommitteeRole::Quant, 1, &parsed),
            Some("missing_critical_fields".to_string())
        );
    }

    #[test]
    fn test_detect_fallback_quant_r1_missing_signal_only() {
        let parsed = ParsedFields {
            signal: None,
            regime: Some("bull".to_string()),
            raw_text: "REGIME: bull".to_string(),
            ..Default::default()
        };
        assert_eq!(
            detect_fallback_reason(CommitteeRole::Quant, 1, &parsed),
            Some("missing_critical_fields".to_string())
        );
    }

    #[test]
    fn test_detect_fallback_quant_r1_missing_regime_only() {
        let parsed = ParsedFields {
            signal: Some("risk_on".to_string()),
            regime: None,
            raw_text: "SIGNAL: risk_on".to_string(),
            ..Default::default()
        };
        assert_eq!(
            detect_fallback_reason(CommitteeRole::Quant, 1, &parsed),
            Some("missing_critical_fields".to_string())
        );
    }

    /// Quant R2 does not require REGIME — only signal is critical.
    #[test]
    fn test_detect_fallback_quant_r2_no_regime_ok() {
        let parsed = ParsedFields {
            signal: Some("bullish".to_string()),
            regime: None,
            raw_text: "调整信号: bullish\n保护触发: yes".to_string(),
            ..Default::default()
        };
        // R2 with signal present but regime absent → no fallback
        assert_eq!(detect_fallback_reason(CommitteeRole::Quant, 2, &parsed), None);
    }

    /// Quant R2 still requires signal.
    #[test]
    fn test_detect_fallback_quant_r2_missing_signal() {
        let parsed = ParsedFields {
            signal: None,
            regime: None,
            raw_text: "保护触发: yes".to_string(),
            ..Default::default()
        };
        assert_eq!(
            detect_fallback_reason(CommitteeRole::Quant, 2, &parsed),
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
            detect_fallback_reason(CommitteeRole::Risk, 1, &parsed),
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
            detect_fallback_reason(CommitteeRole::Cio, 1, &parsed),
            Some("missing_critical_fields".to_string())
        );
    }

    #[test]
    fn test_detect_fallback_l4_removed() {
        let parsed = ParsedFields {
            raw_text: "l4 analysis".to_string(),
            ..Default::default()
        };
        assert_eq!(
            detect_fallback_reason(CommitteeRole::L4Officer, 1, &parsed),
            Some("l4_removed".to_string())
        );
    }

    #[test]
    fn test_detect_fallback_empty_text() {
        let parsed = ParsedFields {
            raw_text: "".to_string(),
            ..Default::default()
        };
        assert_eq!(
            detect_fallback_reason(CommitteeRole::Macro, 1, &parsed),
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
        assert_eq!(detect_fallback_reason(CommitteeRole::Macro, 1, &parsed), None);
    }

    // ── Markdown formatting strip tests ─────────────────────────────────

    #[test]
    fn test_strip_bold_asterisks() {
        assert_eq!(strip_markdown_formatting("**risk_on**"), "risk_on");
        assert_eq!(strip_markdown_formatting("**bull** market"), "bull market");
    }

    #[test]
    fn test_strip_inline_code() {
        assert_eq!(strip_markdown_formatting("`risk_on`"), "risk_on");
        assert_eq!(strip_markdown_formatting("value is `7`"), "value is 7");
    }

    #[test]
    fn test_strip_nested_formatting() {
        assert_eq!(strip_markdown_formatting("**`bold_code`**"), "bold_code");
    }

    #[test]
    fn test_strip_no_formatting() {
        assert_eq!(strip_markdown_formatting("plain text"), "plain text");
    }

    // ── Continuation line merge tests ────────────────────────────────────

    #[test]
    fn test_merge_continuation_simple() {
        let text = "SIGNAL: risk_on\nSTRENGTH: 7";
        let merged = merge_continuation_lines(text);
        assert!(merged.contains("SIGNAL: risk_on"));
        assert!(merged.contains("STRENGTH: 7"));
    }

    #[test]
    fn test_merge_continuation_wrapped_value() {
        // 模拟 LLM 换行输出长值
        let text = "REASONING: 北向资金持续流入\n同时宏观数据偏暖";
        let merged = merge_continuation_lines(text);
        // 续行应被合并到 REASONING 的值中
        assert!(merged.contains("北向资金持续流入 同时宏观数据偏暖"));
    }

    #[test]
    fn test_merge_continuation_list_items_untouched() {
        let text = "KEY_DATA:\n- PE=13.5\n- volume up";
        let merged = merge_continuation_lines(text);
        // 列表项不应被合并到前一个 key 的值中
        assert!(merged.contains("KEY_DATA:\n- PE=13.5\n- volume up"));
    }

    #[test]
    fn test_merge_continuation_chinese_colon() {
        let text = "推理：短期回调信号增强\n但中期偏多";
        let merged = merge_continuation_lines(text);
        assert!(merged.contains("短期回调信号增强 但中期偏多"));
    }

    #[test]
    fn test_extract_field_with_markdown_value() {
        let text = "SIGNAL: **risk_on**";
        let val = extract_field(text, "SIGNAL").unwrap();
        assert_eq!(val, "risk_on");
    }

    #[test]
    fn test_extract_field_with_code_value() {
        let text = "VERDICT: `BUY`";
        let val = extract_field(text, "VERDICT").unwrap();
        assert_eq!(val, "BUY");
    }

    #[test]
    fn test_parse_with_continuation_and_markdown() {
        // 综合测试：markdown 格式值 + 续行
        let text = "SIGNAL: **risk_on**\nREASONING: 北向资金持续流入\n同时宏观数据偏暖\nSTRENGTH: 7";
        let parsed = parse_role_output(CommitteeRole::Macro, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("risk_on"));
        // REASONING 不是 Macro 字段，但强度应正常解析
        assert_eq!(parsed.strength, Some(7.0));
    }

    #[test]
    fn test_merge_continuation_long_key_preserved() {
        // 超过 20 字符的英文 key 不应被当作续行
        let text = "SIGNAL: risk_on\nWORST_CASE_LOSS_PCT_AT_-20: -12.5\nSTRENGTH: 7";
        let merged = merge_continuation_lines(text);
        assert!(merged.contains("WORST_CASE_LOSS_PCT_AT_-20: -12.5"));
        // 确保该行独立存在，未被合并到 SIGNAL 的值中
        assert!(merged.contains("risk_on\nWORST_CASE_LOSS_PCT_AT_-20"));
    }

    // ── R2 signal variant tests ─────────────────────────────────────────

    #[test]
    fn test_quant_r2_signal_variant_adjusted_signal() {
        // "调整后信号" 是 prompt 未指定但 LLM 可能使用的变体
        let text = "调整后信号: bullish\n调整强度: 7\n保护触发: no";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("bullish"));
        assert_eq!(detect_fallback_reason(CommitteeRole::Quant, 2, &parsed), None);
    }

    #[test]
    fn test_quant_r2_signal_variant_english_adjusted() {
        // "ADJUSTED_SIGNAL" (underscore) is the valid English variant.
        // "ADJUSTED SIGNAL" (space) is excluded — is_structured_key_line rejects keys with spaces.
        let text = "ADJUSTED_SIGNAL: bearish\nADJUSTED_STRENGTH: 5";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("bearish"));
        assert_eq!(detect_fallback_reason(CommitteeRole::Quant, 2, &parsed), None);
    }

    #[test]
    fn test_quant_r2_signal_variant_bold() {
        let text = "**调整信号**: neutral\n**调整强度**: 3";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("neutral"));
        assert_eq!(detect_fallback_reason(CommitteeRole::Quant, 2, &parsed), None);
    }

    #[test]
    fn test_quant_r2_buy_point_assessment() {
        // R2 prompt 使用 "调整买点"，parser 应能提取
        let text = "调整信号: bullish\n调整强度: 7\n调整买点: 低吸\n保护触发: no";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.buy_point_assessment.as_deref(), Some("低吸"));
    }

    #[test]
    fn test_quant_r1_buy_point_not_overridden_by_r2() {
        // R1 的 "买点评估" 应优先于 R2 的 "调整买点"
        let text = "买点评估: 突破\n调整买点: 低吸";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.buy_point_assessment.as_deref(), Some("突破"));
    }

    #[test]
    fn test_parse_cio_execution_mode_and_first_tranche() {
        let text = "VERDICT: ACCUMULATE\nCONFIDENCE: 0.7\n执行模式: pyramid\n首笔金额: 30000";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.execution_mode.as_deref(), Some("pyramid"));
        assert_eq!(parsed.first_tranche_cny, Some(30000.0));
    }

    #[test]
    fn test_parse_macro_signal_reason() {
        let text = "SIGNAL: risk_on\n信号理由: 北向资金持续流入\n市场阶段: 主升\n市场阶段理由: 站上MA60";
        let parsed = parse_role_output(CommitteeRole::Macro, text, false);
        assert_eq!(parsed.signal_reason.as_deref(), Some("北向资金持续流入"));
        assert_eq!(parsed.market_phase_reason.as_deref(), Some("站上MA60"));
    }

    // ── Task 1: Markdown wrap & list-prefix tolerance ──────────────────

    #[test]
    fn test_matches_markdown_wrapped_signal() {
        // Risk R1 真实形态:整行被 ** 包裹,冒号在加粗内部
        let parsed = parse_role_output(CommitteeRole::Risk, "**SIGNAL: concerned**", false);
        assert_eq!(parsed.signal.as_deref(), Some("concerned"));
    }

    #[test]
    fn test_matches_list_prefixed_bold_key() {
        // 002735 真实形态:列表前缀 + 加粗 key
        let parsed = parse_role_output(CommitteeRole::Risk, "- **集中度**: 30.5%", false);
        assert_eq!(parsed.concentration_pct, Some(30.5));
    }

    #[test]
    fn test_matches_bold_key_colon_inside() {
        // 002384 真实形态:**集中度:** 30.5%
        let parsed = parse_role_output(CommitteeRole::Risk, "**集中度:** 30.5%", false);
        assert_eq!(parsed.concentration_pct, Some(30.5));
    }

    // ── Task 2: Inline `|` multi-field split + N/M、% numeric tolerance ──

    #[test]
    fn test_inline_pipe_multi_field() {
        // Risk R1 真实形态:同一行两个字段用 | 分隔,且整体加粗分段
        let text = "**SIGNAL: concerned** | **强度: 6/10**";
        let parsed = parse_role_output(CommitteeRole::Risk, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("concerned"));
        assert_eq!(parsed.strength, Some(6.0));
    }

    #[test]
    fn test_pipe_inside_value_not_split() {
        // 反例:值里含 | 但不是多字段(止损条件描述),不应被破坏
        let text = "调整止损: 跌破MA20 | 或浮盈归零即减仓";
        let parsed = parse_role_output(CommitteeRole::Risk, text, false);
        assert_eq!(parsed.adjusted_stop_loss.as_deref(), Some("跌破MA20 | 或浮盈归零即减仓"));
    }

    // ── Task 3: 续行合并识别被包裹的 key 行 ─────────────────────────────

    #[test]
    fn test_wrapped_key_not_merged_as_continuation() {
        // 多行:裸 key 行后跟一个被 ** 包裹的 key 行,后者不应被并入前者的值
        let text = "标的风险: 估值偏高\n**SIGNAL: concerned**\n强度: 6";
        let parsed = parse_role_output(CommitteeRole::Risk, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("concerned"));
        assert_eq!(parsed.stock_risk_summary.as_deref(), Some("估值偏高"));
        assert_eq!(parsed.strength, Some(6.0));
    }

    #[test]
    fn test_list_prefixed_bold_key_not_merged() {
        // 真正会触发 Task 3 修复的 RED 场景:
        // 老 is_structured_key_line 对 `+ **风险信号**: concerned` 返回 false
        // (`+ **风险信号**` 段含空格);而 merge_continuation_lines 的 is_list_item
        // 守门只识别 `- `/`* `,不挡 `+ `,所以该行会被并入上一字段的值,
        // 导致 signal 提不出、stock_risk_summary 串到下一段。
        // Task 3 在 is_structured_key_line 内先归一化(剥列表前缀 + `**…**` 包裹),
        // 该行被识别为独立 key 行,各字段干净分离。
        // 注:用户最初提议的 `- **风险信号**: ...` 形式实际不会 mis-merge ——
        // is_list_item 已为 `- ` 短路;故改用 `+ ` 作为真正 RED 的列表前缀。
        let text = "标的风险: 估值偏高\n+ **风险信号**: concerned\n强度: 6";
        let parsed = parse_role_output(CommitteeRole::Risk, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("concerned"));
        assert_eq!(parsed.stock_risk_summary.as_deref(), Some("估值偏高"));
        assert_eq!(parsed.strength, Some(6.0));
    }
}
