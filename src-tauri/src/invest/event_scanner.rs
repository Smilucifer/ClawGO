use serde::{Deserialize, Serialize};

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
#[serde(rename_all = "camelCase")]
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
}
