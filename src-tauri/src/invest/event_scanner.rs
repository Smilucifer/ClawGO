use serde::{Deserialize, Serialize};

use crate::invest::llm::{InvestLlmClient, LlmConfig, Message, collect_stream};

/// Severity classification from rule-based keyword filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    High,
    Medium,
    Low,
}

/// Result of LLM normalization for a single event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NormalizedEvent {
    pub one_line_claim: String,
    pub stance: String,
    pub severity: Severity,
    pub affected_symbols: Vec<String>,
}

/// Raw event before normalization.
#[derive(Debug, Clone)]
pub struct RawEvent {
    pub source: String,
    pub event_type: String,
    pub title: String,
    pub body: String,
    pub url: Option<String>,
    pub created_at: String,
}

// ── Rule-based keyword filtering ──

const HIGH_KEYWORDS: &[&str] = &[
    "央行", "降准", "降息", "加息", "MLF", "LPR", "逆回购",
    "暴跌", "熔断", "ST", "退市", "暂停上市", "重大违法",
    "关税", "制裁", "禁令", "反垄断", "行业整顿",
];

const MEDIUM_KEYWORDS: &[&str] = &[
    "财报", "业绩预告", "净利润", "营收",
    "增持", "减持", "回购", "定增", "分红",
    "产能", "订单", "并购", "重组",
];

/// Classify severity by keyword matching.
/// Returns None for LOW (irrelevant) events that should be filtered out.
pub fn classify_severity(title: &str, body: &str) -> Option<Severity> {
    let text = format!("{} {}", title, body);
    if HIGH_KEYWORDS.iter().any(|k| text.contains(k)) {
        Some(Severity::High)
    } else if MEDIUM_KEYWORDS.iter().any(|k| text.contains(k)) {
        Some(Severity::Medium)
    } else {
        None
    }
}

// ── LLM normalization ──

/// Default system prompt for event normalization.
const DEFAULT_NORMALIZER_PROMPT: &str = r#"你是一个A股财经新闻分析师。对以下新闻/公告进行结构化提取。

对每条新闻输出一个JSON数组，每个元素包含：
- one_line_claim: 一句话摘要（≤30字）
- stance: bullish / bearish / neutral
- severity: high / medium / low
- affected_symbols: 涉及的A股代码数组（6位数字格式，如 "600519"）

只输出JSON数组，不要其他文字。"#;

/// Normalize a batch of raw events using LLM.
/// Returns normalized results in the same order as input.
/// Falls back to rule-based severity on parse failure.
pub async fn normalize_events(
    client: &dyn InvestLlmClient,
    config: &LlmConfig,
    raw_events: &[RawEvent],
    system_prompt: Option<&str>,
) -> Vec<NormalizedEvent> {
    if raw_events.is_empty() {
        return vec![];
    }

    // Build batch prompt
    let mut items = String::new();
    for (i, ev) in raw_events.iter().enumerate() {
        items.push_str(&format!(
            "\n[{}] source={} title={}\n{}\n",
            i + 1,
            ev.source,
            ev.title,
            if ev.body.is_empty() { &ev.title } else { &ev.body }
        ));
    }

    let system = system_prompt.unwrap_or(DEFAULT_NORMALIZER_PROMPT);
    let messages = vec![Message::user(items)];

    // Call LLM
    let stream = match client.chat_stream(system, &messages, None, config).await {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Event normalizer LLM call failed: {}, falling back to rule-based", e);
            return raw_events.iter().map(|ev| fallback_normalize(ev)).collect();
        }
    };

    let collected = collect_stream(stream).await;
    let content = collected.content;

    // Parse JSON response
    parse_normalized_response(&content, raw_events)
}

/// Parse LLM JSON response, matching results to raw events by index.
fn parse_normalized_response(content: &str, raw_events: &[RawEvent]) -> Vec<NormalizedEvent> {
    // Extract JSON array from response (handle markdown code blocks)
    let json_str = content.trim();
    let json_str = if json_str.starts_with("```") {
        json_str
            .lines()
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end_matches("```")
            .trim()
            .to_string()
    } else {
        json_str.to_string()
    };

    match serde_json::from_str::<Vec<NormalizedEvent>>(&json_str) {
        Ok(results) => {
            // Pad or truncate to match input length
            let mut normalized = results;
            normalized.truncate(raw_events.len());
            while normalized.len() < raw_events.len() {
                let idx = normalized.len();
                normalized.push(fallback_normalize(&raw_events[idx]));
            }
            normalized
        }
        Err(e) => {
            log::warn!("Failed to parse normalizer response: {}, falling back to rule-based", e);
            raw_events.iter().map(|ev| fallback_normalize(ev)).collect()
        }
    }
}

/// Fallback: use rule-based severity, neutral stance, no symbols.
fn fallback_normalize(ev: &RawEvent) -> NormalizedEvent {
    let severity = classify_severity(&ev.title, &ev.body)
        .map(|s| match s {
            Severity::High => Severity::High,
            Severity::Medium => Severity::Medium,
            Severity::Low => Severity::Low,
        })
        .unwrap_or(Severity::Low);

    NormalizedEvent {
        one_line_claim: ev.title.chars().take(30).collect(),
        stance: "neutral".to_string(),
        severity,
        affected_symbols: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_high() {
        assert_eq!(classify_severity("央行宣布降准50个基点", ""), Some(Severity::High));
        assert_eq!(classify_severity("某股暴跌触发熔断", ""), Some(Severity::High));
    }

    #[test]
    fn test_classify_medium() {
        assert_eq!(classify_severity("公司发布财报", ""), Some(Severity::Medium));
        assert_eq!(classify_severity("大股东减持公告", ""), Some(Severity::Medium));
    }

    #[test]
    fn test_classify_low_filtered() {
        assert_eq!(classify_severity("今日天气晴朗", ""), None);
        assert_eq!(classify_severity("体育新闻", ""), None);
    }

    #[test]
    fn test_parse_normalized_response() {
        let raw = vec![RawEvent {
            source: "test".into(),
            event_type: "news".into(),
            title: "央行降准".into(),
            body: "央行宣布降准50个基点".into(),
            url: None,
            created_at: "2026-05-29T10:00:00Z".into(),
        }];
        let json = r#"[{"one_line_claim":"央行降准50基点","stance":"bullish","severity":"high","affected_symbols":["600519"]}]"#;
        let result = parse_normalized_response(json, &raw);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].stance, "bullish");
        assert_eq!(result[0].affected_symbols, vec!["600519"]);
    }
}
