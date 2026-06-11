use serde::{Deserialize, Serialize};

use crate::invest::international::InternationalClient;
use crate::invest::llm::{InvestLlmClient, LlmConfig, Message, collect_stream};

/// Truncate string at char boundary for logging (≤40 chars).
pub fn short(s: &str) -> &str {
    &s[..s.floor_char_boundary(40.min(s.len()))]
}

/// Convert Unix timestamp to local-time ISO 8601 string (UTC+8), with a fallback for zero/invalid timestamps.
pub fn format_provider_timestamp(ts: i64, fallback: &str) -> String {
    if ts > 0 {
        chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.with_timezone(&chrono::Local).format("%Y-%m-%dT%H:%M:%S").to_string())
            .unwrap_or_else(|| fallback.to_string())
    } else {
        fallback.to_string()
    }
}

/// Normalize Tushare date strings to ISO 8601 format.
/// Handles "20260609" → "2026-06-09T00:00:00" and passes through valid ISO strings.
fn normalize_tushare_date(raw: &str, fallback: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return fallback.to_string();
    }
    // "20260609" 8-digit pure numeric format
    if trimmed.len() == 8 && trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return format!(
            "{}-{}-{}T00:00:00",
            &trimmed[0..4],
            &trimmed[4..6],
            &trimmed[6..8]
        );
    }
    // "2026-06-09" 10-char date-only format
    if trimmed.len() == 10 && trimmed.as_bytes()[4] == b'-' && trimmed.as_bytes()[7] == b'-' {
        return format!("{}T00:00:00", trimmed);
    }
    // Already has time component or other format — pass through
    trimmed.to_string()
}

/// Severity classification from rule-based keyword filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    High,
    Medium,
    Low,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
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

// ── Source count limits ──

/// Max Jin10 flash news items per scan.
pub const JIN10_COUNT: usize = 30;
/// Max AkShare per-stock news items per symbol.
const AKSHARE_PER_STOCK_COUNT: u32 = 8;

// ── Rule-based keyword filtering ──

const HIGH_KEYWORDS: &[&str] = &[
    "央行", "降准", "降息", "加息", "MLF", "LPR", "逆回购",
    "暴跌", "熔断", "ST", "退市", "暂停上市", "重大违法",
    "关税", "制裁", "禁令", "反垄断", "行业整顿",
    // English equivalents for global events
    "tariff", "sanctions", "interest rate", "federal reserve",
    "cpi", "gdp", "unemployment", "yield curve", "inflation",
    "default", "bankruptcy", "trade war", "trade tension",
];

const MEDIUM_KEYWORDS: &[&str] = &[
    "财报", "业绩预告", "净利润", "营收",
    "增持", "减持", "回购", "定增", "分红",
    "产能", "订单", "并购", "重组",
    // English equivalents
    "earnings", "revenue", "dividend", "buyback",
    "merger", "acquisition", "downgrade", "debt", "credit",
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

/// Default language for backend-initiated scans (no frontend context).
pub const DEFAULT_LANGUAGE: &str = "zh-CN";

/// Default system prompt for event normalization (Chinese).
const DEFAULT_NORMALIZER_PROMPT_ZH: &str = r#"你是一个A股财经新闻分析师。对以下新闻/公告进行结构化提取。

对每条新闻输出一个JSON数组，每个元素包含：
- one_line_claim: 一句话摘要（≤30字）
- stance: bullish / bearish / neutral
- severity: high / medium / low
- affected_symbols: 涉及的A股代码数组（6位数字格式，如 "600519"）

只输出JSON数组，不要其他文字。"#;

/// Default system prompt for event normalization (English).
const DEFAULT_NORMALIZER_PROMPT_EN: &str = r#"You are an A-share financial news analyst. Extract structured data from the following news/announcements.

Output a JSON array, each element containing:
- one_line_claim: one-line summary (≤50 chars)
- stance: bullish / bearish / neutral
- severity: high / medium / low
- affected_symbols: array of A-share stock codes (6-digit format, e.g. "600519")

Output only the JSON array, no other text."#;

/// Get the default normalizer prompt for the given language.
pub fn default_normalizer_prompt(language: &str) -> &'static str {
    if language.starts_with("en") {
        DEFAULT_NORMALIZER_PROMPT_EN
    } else {
        DEFAULT_NORMALIZER_PROMPT_ZH
    }
}

/// Normalize a batch of raw events using LLM.
/// Returns normalized results in the same order as input.
/// Falls back to rule-based severity on parse failure.
pub async fn normalize_events(
    client: &dyn InvestLlmClient,
    config: &LlmConfig,
    raw_events: &[RawEvent],
    system_prompt: &str,
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

    let system = system_prompt;
    let messages = vec![Message::user(items)];

    // Call LLM
    let stream = match client.chat_stream(system, &messages, None, config).await {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Event normalizer LLM call failed: {}, falling back to rule-based", e);
            return raw_events.iter().map(|ev| fallback_normalize_from(&ev.title, &ev.body)).collect();
        }
    };

    let collected = collect_stream(stream).await;
    let content = collected.content;

    // Parse JSON response
    parse_normalized_response(&content, raw_events, |ev| fallback_normalize_from(&ev.title, &ev.body))
}

/// Result of a scan run.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub fetched: usize,
    pub filtered: usize,
    pub saved: usize,
    pub sources_scanned: Vec<String>,
    /// Errors encountered during scan (Tushare / AkShare failures).
    pub errors: Vec<String>,
}

/// Run a full event scan: fetch from Tushare announcements + Jin10 flash + AkShare per-stock,
/// filter by keywords, normalize via LLM, save to DB.
pub async fn scan_events(
    tushare: &crate::tushare::TushareClient,
    llm_client: &dyn InvestLlmClient,
    llm_config: &LlmConfig,
    normalizer_prompt: Option<&str>,
    language: &str,
) -> Result<ScanResult, String> {
    use chrono::{Local, Duration as ChronoDuration};

    let now = Local::now();
    let today = now.format("%Y%m%d").to_string();
    let one_day_ago = (now - ChronoDuration::days(1)).format("%Y%m%d").to_string();

    let mut raw_events: Vec<RawEvent> = Vec::new();
    let mut sources_scanned: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // 1. Fetch Tushare announcements for HOLD + WATCH holdings
    let holdings = crate::storage::invest::portfolio::list_holdings()
        .unwrap_or_default();
    // Explicit filter: old DBs may lack the CHECK constraint on kind
    let active_symbols: Vec<&str> = holdings
        .iter()
        .filter(|h| h.kind == crate::storage::invest::portfolio::HoldingKind::Hold || h.kind == crate::storage::invest::portfolio::HoldingKind::Watch)
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
                    let fallback = now.format("%Y-%m-%dT%H:%M:%S").to_string();
                    let created_at = normalize_tushare_date(&item.ann_date, &fallback);
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
        "Tushare announcements yielded {} raw events total",
        raw_events.len()
    );

    // 2. Fetch Jin10 flash news (macro + international) + AkShare per-stock news concurrently
    {
        let client = InternationalClient::from_settings();
        let fallback_time = now.format("%Y-%m-%dT%H:%M:%S").to_string();

        // Helper: convert NewsItem to RawEvent
        let mut add_news_items = |items: &[crate::invest::international::NewsItem], source_name: &str| {
            if items.is_empty() {
                log::info!("{source_name} returned 0 items");
                return;
            }
            log::info!("{source_name} returned {} items", items.len());
            for item in items {
                let created_at = format_provider_timestamp(item.provider_publish_time, &fallback_time);
                raw_events.push(RawEvent {
                    source: source_name.to_string(),
                    event_type: "news".to_string(),
                    title: item.title.clone(),
                    body: format!("Publisher: {}", item.publisher),
                    url: Some(item.link.clone()),
                    created_at,
                });
            }
        };

        // Build per-stock futures for AkShare
        let stock_futures: Vec<_> = active_symbols
            .iter()
            .map(|symbol| {
                let c = client.clone();
                async move {
                    let result = c.fetch_akshare_stock_news(symbol, AKSHARE_PER_STOCK_COUNT).await;
                    (symbol, result)
                }
            })
            .collect();

        // Run Jin10 + all per-stock fetches concurrently
        // Use A-share channel filter to get only A-share related news
        sources_scanned.push("jinshi_flash".to_string());
        let jin10_client = client.clone();
        let jin10_future = async {
            jin10_client.fetch_jinshi_a_share_news(JIN10_COUNT).await
        };

        let (jin10_result, stock_results) = tokio::join!(
            jin10_future,
            futures_util::future::join_all(stock_futures)
        );

        add_news_items(&jin10_result, "jinshi_flash");

        for (symbol, result) in stock_results {
            let source = format!("akshare:{}", symbol);
            sources_scanned.push(source.clone());
            match result {
                Ok(items) => add_news_items(&items, &source),
                Err(e) => {
                    log::warn!("akshare_stock_news({}): {}", symbol, e);
                    errors.push(format!("akshare:{}: {}", symbol, e));
                }
            }
        }
    }

    let fetched = raw_events.len();
    log::info!("Total raw events before filtering: {}", fetched);

    // 3. Deduplicate raw_events by (source, title) to avoid duplicate LLM calls
    {
        use std::collections::HashSet;
        let mut seen: HashSet<(String, String)> = HashSet::new();
        raw_events.retain(|ev| seen.insert((ev.source.clone(), ev.title.clone())));
        let dedup_count = fetched - raw_events.len();
        if dedup_count > 0 {
            log::info!("Deduplicated {} duplicate events by (source, title)", dedup_count);
        }
    }

    // 4. Filter by keyword severity (drop LOW)
    let filtered_events: Vec<RawEvent> = raw_events
        .into_iter()
        .filter(|ev| classify_severity(&ev.title, &ev.body).is_some())
        .collect();
    let dropped = fetched.saturating_sub(filtered_events.len());

    log::info!(
        "After keyword filtering: {} events remain (dropped {})",
        filtered_events.len(),
        dropped
    );

    if filtered_events.is_empty() {
        return Ok(ScanResult {
            fetched,
            filtered: dropped,
            saved: 0,
            sources_scanned,
            errors,
        });
    }

    // 5. Normalize via LLM
    let effective_prompt = normalizer_prompt.unwrap_or_else(|| default_normalizer_prompt(language));
    let normalized = normalize_events(llm_client, llm_config, &filtered_events, effective_prompt).await;

    // 6. Save to DB (dedup by source+title via INSERT OR IGNORE)
    let mut saved = 0usize;
    for (ev, norm) in filtered_events.iter().zip(normalized.iter()) {

        // Log LLM classification for diagnostics
        log::debug!(
            "  [normalize] '{}' => severity={}, stance={}, claim='{}'",
            short(&ev.title),
            norm.severity.as_str().to_ascii_uppercase(),
            norm.stance,
            norm.one_line_claim
        );

        // Skip events the LLM reclassified as LOW (pre-filter only keeps HIGH/MEDIUM)
        if norm.severity == Severity::Low {
            log::debug!("  [skip] '{}' — LLM classified as LOW", short(&ev.title));
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
            severity: norm.severity.as_str().to_string(),
            stance: norm.stance.clone(),
            triggered: false,
            trigger_verdict_id: None,
            created_at: ev.created_at.clone(),
            analyzed: true,
            analyzed_at: Some(chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()),
            channels: "[]".to_string(),
        };
        match crate::storage::invest::events::save_event(&event) {
            Ok(()) => saved += 1,
            Err(e) => {
                // Duplicate key errors are expected (dedup)
                if e.contains("UNIQUE") {
                    log::debug!("  [dedup] '{}' — already exists", short(&ev.title));
                } else {
                    log::warn!("Failed to save event '{}': {}", ev.title, e);
                }
            }
        }
    }

    log::info!("Scan complete: {} fetched, {} dropped, {} saved", fetched, dropped, saved);

    Ok(ScanResult {
        fetched,
        filtered: dropped,
        saved,
        sources_scanned,
        errors,
    })
}

/// Parse LLM JSON response, matching results to items by index.
/// Generic over input type — callers provide a fallback closure.
pub fn parse_normalized_response<T>(
    content: &str,
    items: &[T],
    fallback: impl Fn(&T) -> NormalizedEvent,
) -> Vec<NormalizedEvent> {
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
            normalized.truncate(items.len());
            while normalized.len() < items.len() {
                let idx = normalized.len();
                normalized.push(fallback(&items[idx]));
            }
            normalized
        }
        Err(e) => {
            log::warn!("Failed to parse normalizer response: {}, falling back to rule-based", e);
            items.iter().map(fallback).collect()
        }
    }
}

/// Fallback normalization from title + body text.
/// Used by both RawEvent and Event via `fallback_normalize_for`.
pub fn fallback_normalize_from(title: &str, body: &str) -> NormalizedEvent {
    let severity = classify_severity(title, body)
        .unwrap_or(Severity::Low);

    NormalizedEvent {
        one_line_claim: title.chars().take(30).collect(),
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
        let result = parse_normalized_response(json, &raw, |ev| fallback_normalize_from(&ev.title, &ev.body));
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
        let result = parse_normalized_response(wrapped, &raw, |ev| fallback_normalize_from(&ev.title, &ev.body));
        assert_eq!(result.len(), 2); // 1 parsed + 1 fallback for missing
        assert_eq!(result[0].stance, "neutral");
        // Second event falls back
        assert_eq!(result[1].stance, "neutral");
    }
}
