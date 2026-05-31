use serde::{Deserialize, Serialize};

use crate::invest::international::InternationalClient;
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

/// Minimum Tushare events before skipping Yahoo Finance fallback.
/// Below this threshold the yield is too sparse for reliable keyword filtering.
const YAHOO_FALLBACK_MIN_EVENTS: usize = 3;

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
    if HIGH_KEYWORDS.iter().any(|k| title.contains(k) || body.contains(k)) {
        Some(Severity::High)
    } else if MEDIUM_KEYWORDS.iter().any(|k| title.contains(k) || body.contains(k)) {
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
            "\n[{}] source={} type={} title={}\n{}\n",
            i + 1,
            ev.source,
            ev.event_type,
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

/// Result of a scan run.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub fetched: usize,
    pub filtered: usize,
    pub saved: usize,
    pub sources_scanned: Vec<String>,
    /// Errors encountered during scan (Tushare / Yahoo failures).
    pub errors: Vec<String>,
}

/// Run a full event scan: fetch from Tushare, filter by keywords, normalize via LLM, save to DB.
pub async fn scan_events(
    tushare: &crate::tushare::TushareClient,
    llm_client: &dyn InvestLlmClient,
    llm_config: &LlmConfig,
    normalizer_prompt: Option<&str>,
) -> Result<ScanResult, String> {
    use chrono::{Local, Duration as ChronoDuration};

    let now = Local::now();
    let today = now.format("%Y%m%d").to_string();
    let one_day_ago = (now - ChronoDuration::days(1)).format("%Y%m%d").to_string();

    let mut raw_events: Vec<RawEvent> = Vec::new();
    let mut sources_scanned: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // 1. Fetch major_news (sina + cls)
    for src in &["sina", "cls"] {
        sources_scanned.push(format!("tushare_major_news:{}", src));
        match tushare.major_news(src, &one_day_ago, &today).await {
            Ok(items) => {
                log::info!(
                    "tushare_major_news({}) returned {} items",
                    src,
                    items.len()
                );
                for item in items {
                    let created_at = if item.datetime.is_empty() {
                        now.format("%Y-%m-%dT%H:%M:%S").to_string()
                    } else {
                        item.datetime
                    };
                    raw_events.push(RawEvent {
                        source: format!("tushare_major_news:{}", src),
                        event_type: "news".to_string(),
                        title: item.title,
                        body: item.content,
                        url: None,
                        created_at,
                    });
                }
            }
            Err(e) => {
                let msg = format!("tushare_major_news({}): {}", src, e);
                log::warn!("{}", msg);
                errors.push(msg);
            }
        }
    }

    // 2. Fetch announcements for HOLD + WATCH holdings
    let holdings = crate::storage::invest::portfolio::list_holdings()
        .unwrap_or_default();
    let active_symbols: Vec<&str> = holdings
        .iter()
        .filter(|h| h.kind == "hold" || h.kind == "watch")
        .map(|h| h.symbol.as_str())
        .collect();

    log::info!(
        "Active holdings for announcement scan: {:?}",
        active_symbols
    );

    // Fetch announcements in parallel for all active symbols
    let ann_futures: Vec<_> = active_symbols
        .iter()
        .map(|symbol| {
            let start = one_day_ago.clone();
            let end = today.clone();
            async move {
                let result = tushare.anns_d(symbol, &start, &end).await;
                (symbol, result)
            }
        })
        .collect();
    let ann_results = futures_util::future::join_all(ann_futures).await;
    for (symbol, result) in ann_results {
        sources_scanned.push(format!("tushare_anns_d:{}", symbol));
        match result {
            Ok(items) => {
                log::info!(
                    "tushare_anns_d({}) returned {} items",
                    symbol,
                    items.len()
                );
                for item in items {
                    let created_at = if item.ann_date.is_empty() {
                        now.format("%Y-%m-%dT%H:%M:%S").to_string()
                    } else {
                        item.ann_date.clone()
                    };
                    raw_events.push(RawEvent {
                        source: "tushare_anns_d".to_string(),
                        event_type: "announcement".to_string(),
                        title: item.title,
                        body: String::new(),
                        url: Some(item.url),
                        created_at,
                    });
                }
            }
            Err(e) => {
                let msg = format!("tushare_anns_d({}): {}", symbol, e);
                log::warn!("{}", msg);
                errors.push(msg);
            }
        }
    }

    log::info!(
        "Tushare sources yielded {} raw events total",
        raw_events.len()
    );

    // 3. Fetch Yahoo Finance news as fallback when Tushare yields few results
    if raw_events.len() < YAHOO_FALLBACK_MIN_EVENTS {
        log::info!(
            "Tushare yielded only {} events, fetching Yahoo Finance news as fallback",
            raw_events.len()
        );
        sources_scanned.push("yahoo_finance_news".to_string());
        let yahoo_client = InternationalClient::new();
        let yahoo_items = yahoo_client.fetch_china_finance_news(15).await;
        if yahoo_items.is_empty() {
            log::info!("Yahoo Finance news returned 0 items");
            errors.push("yahoo_finance: returned 0 items (possible 429 rate limit)".into());
        } else {
            log::info!("Yahoo Finance news returned {} items", yahoo_items.len());
            for item in yahoo_items {
                let created_at = if item.provider_publish_time > 0 {
                    chrono::DateTime::from_timestamp(item.provider_publish_time, 0)
                        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S").to_string())
                        .unwrap_or_else(|| now.format("%Y-%m-%dT%H:%M:%S").to_string())
                } else {
                    now.format("%Y-%m-%dT%H:%M:%S").to_string()
                };
                raw_events.push(RawEvent {
                    source: "yahoo_finance".to_string(),
                    event_type: "news".to_string(),
                    title: item.title,
                    body: format!("Publisher: {}", item.publisher),
                    url: Some(item.link),
                    created_at,
                });
            }
        }
    }

    let fetched = raw_events.len();
    log::info!("Total raw events before filtering: {}", fetched);

    // 4. Filter by keyword severity (drop LOW)
    let filtered_events: Vec<RawEvent> = raw_events
        .into_iter()
        .filter(|ev| classify_severity(&ev.title, &ev.body).is_some())
        .collect();
    let filtered = filtered_events.len();

    log::info!(
        "After keyword filtering: {} events remain (dropped {})",
        filtered,
        fetched.saturating_sub(filtered)
    );

    if filtered_events.is_empty() {
        return Ok(ScanResult {
            fetched,
            filtered: 0,
            saved: 0,
            sources_scanned,
            errors,
        });
    }

    // 5. Normalize via LLM
    let normalized = normalize_events(llm_client, llm_config, &filtered_events, normalizer_prompt).await;

    // 6. Save to DB (dedup by source+title via INSERT OR IGNORE)
    let mut saved = 0usize;
    for (ev, norm) in filtered_events.iter().zip(normalized.iter()) {
        // Skip events the LLM reclassified as LOW (pre-filter only keeps HIGH/MEDIUM)
        if norm.severity == Severity::Low {
            continue;
        }
        let symbols_str = norm.affected_symbols.join(",");
        let body = if norm.one_line_claim.is_empty() {
            Some(ev.title.clone())
        } else {
            Some(norm.one_line_claim.clone())
        };
        let event = crate::storage::invest::events::Event {
            id: uuid::Uuid::new_v4().to_string(),
            source: ev.source.clone(),
            event_type: ev.event_type.clone(),
            title: ev.title.clone(),
            body,
            symbols: if symbols_str.is_empty() {
                None
            } else {
                Some(symbols_str)
            },
            severity: match norm.severity {
                Severity::High => "high",
                Severity::Medium => "medium",
                Severity::Low => "low",
            }
            .to_string(),
            stance: norm.stance.clone(),
            triggered: false,
            trigger_verdict_id: None,
            created_at: ev.created_at.clone(),
        };
        match crate::storage::invest::events::save_event(&event) {
            Ok(()) => saved += 1,
            Err(e) => {
                // Duplicate key errors are expected (dedup)
                if !e.to_string().contains("UNIQUE") {
                    log::warn!("Failed to save event '{}': {}", ev.title, e);
                }
            }
        }
    }

    log::info!("Scan complete: {} fetched, {} filtered, {} saved", fetched, filtered, saved);

    Ok(ScanResult {
        fetched,
        filtered,
        saved,
        sources_scanned,
        errors,
    })
}

/// Parse LLM JSON response, matching results to raw events by index.
fn parse_normalized_response(content: &str, raw_events: &[RawEvent]) -> Vec<NormalizedEvent> {
    // Extract JSON array from response (handle markdown code blocks)
    let json_str = content.trim();
    let json_str = if json_str.starts_with("```") {
        json_str
            .lines()
            .skip(1)
            .filter(|l| l.trim() != "```")
            .collect::<Vec<_>>()
            .join("\n")
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

    #[test]
    fn test_parse_markdown_wrapped_json() {
        let raw = vec![
            RawEvent {
                source: "test".into(),
                event_type: "news".into(),
                title: "Event 1".into(),
                body: "body 1".into(),
                url: None,
                created_at: "2026-01-01".into(),
            },
            RawEvent {
                source: "test".into(),
                event_type: "news".into(),
                title: "Event 2".into(),
                body: "body 2".into(),
                url: None,
                created_at: "2026-01-02".into(),
            },
        ];
        let wrapped = "```json\n[{\"one_line_claim\":\"claim 1\",\"stance\":\"neutral\",\"severity\":\"low\",\"affected_symbols\":[]}]\n```";
        let result = parse_normalized_response(wrapped, &raw);
        assert_eq!(result.len(), 2); // 1 parsed + 1 fallback for missing
        assert_eq!(result[0].stance, "neutral");
        // Second event falls back
        assert_eq!(result[1].stance, "neutral");
    }
}
