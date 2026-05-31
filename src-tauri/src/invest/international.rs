//! HTTP client for international market indicators via Yahoo Finance v8 API.

use serde::Deserialize;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single real-time quote from Yahoo Finance.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YahooQuote {
    pub symbol: String,
    pub name: String,
    pub price: f64,
    pub change: f64,
    pub change_pct: f64,
    pub previous_close: f64,
    pub timestamp: i64,
}

/// A single daily bar from Yahoo Finance historical data.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YahooBar {
    pub symbol: String,
    pub date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u64,
}

// ---------------------------------------------------------------------------
// Yahoo Finance v8 API response types (internal)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct YahooChartResponse {
    chart: YahooChartResult,
}

#[derive(Debug, Deserialize)]
struct YahooChartResult {
    result: Vec<serde_json::Value>,
    #[serde(rename = "error")]
    error: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Yahoo Finance v8 chart API base URL.
const YAHOO_CHART_API: &str = "https://query1.finance.yahoo.com/v8/finance/chart";

/// Yahoo Finance search API base URL (for news).
const YAHOO_SEARCH_API: &str = "https://query1.finance.yahoo.com/v1/finance/search";

/// Delay between consecutive Yahoo Finance API requests to avoid 429 rate limiting.
pub const YAHOO_REQUEST_INTERVAL_MS: u64 = 500;

/// Well-known international indicator symbols.
pub const INTERNATIONAL_SYMBOLS: &[(&str, &str)] = &[
    ("^VIX", "VIX 恐慌指数"),
    ("^TNX", "美10Y国债收益率"),
    ("DX-Y.NYB", "美元指数"),
    ("GC=F", "国际金价"),
    ("CL=F", "国际油价"),
    ("USDCNY=X", "USD/CNY 汇率"),
];

/// A single news item from Yahoo Finance search API.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YahooNewsItem {
    pub uuid: String,
    pub title: String,
    pub publisher: String,
    pub link: String,
    pub provider_publish_time: i64,
    pub related_tickers: Vec<String>,
}

// Yahoo Finance search response types (internal)
#[derive(Debug, Deserialize)]
struct YahooSearchResponse {
    news: Vec<YahooSearchNewsItem>,
}

#[derive(Debug, Deserialize)]
struct YahooSearchNewsItem {
    uuid: Option<String>,
    title: Option<String>,
    publisher: Option<String>,
    link: Option<String>,
    #[serde(default)]
    provider_publish_time: Option<i64>,
    #[serde(default)]
    related_tickers: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// HTTP client for Yahoo Finance international indicators.
pub struct InternationalClient {
    client: reqwest::Client,
}

impl InternationalClient {
    /// Create a new client with a 15-second request timeout.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to build reqwest client");
        Self { client }
    }

    // -- low-level -----------------------------------------------------------

    /// Fetch raw chart JSON from Yahoo Finance v8 API.
    ///
    /// Retries up to 3 times with exponential back-off (1s, 2s, 4s) on 429 / 5xx.
    async fn fetch_chart_raw(
        &self,
        symbol: &str,
        interval: &str,
        range: &str,
    ) -> Result<serde_json::Value, String> {
        let url = format!("{YAHOO_CHART_API}/{symbol}");
        let max_retries = 3u32;
        let mut last_err: Option<String> = None;

        for attempt in 0..max_retries {
            let resp = self
                .client
                .get(&url)
                .query(&[("interval", interval), ("range", range)])
                .send()
                .await
                .map_err(|e| format!("Yahoo request failed for {symbol}: {e}"))?;

            let status = resp.status();

            // Retry on 429 or 5xx with exponential back-off
            if status.as_u16() == 429 || status.is_server_error() {
                let backoff_ms = 1000 * (1u64 << attempt);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                last_err = Some(format!(
                    "Yahoo HTTP {status} for {symbol} (attempt {}/{max_retries})",
                    attempt + 1
                ));
                continue;
            }

            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("Yahoo HTTP {status} for {symbol}: {body}"));
            }

            let text = resp
                .text()
                .await
                .map_err(|e| format!("failed to read Yahoo response: {e}"))?;

            let parsed: YahooChartResponse =
                serde_json::from_str(&text).map_err(|e| format!("Yahoo json parse error: {e}"))?;

            if let Some(err) = &parsed.chart.error {
                if !err.is_null() {
                    return Err(format!("Yahoo API error for {symbol}: {err}"));
                }
            }

            return parsed
                .chart
                .result
                .into_iter()
                .next()
                .ok_or_else(|| format!("no chart result for {symbol}"));
        }

        Err(last_err.unwrap_or_else(|| format!("Yahoo request failed for {symbol} after {max_retries} attempts")))
    }

    // -- high-level helpers --------------------------------------------------

    /// Fetch real-time quote for a Yahoo symbol.
    ///
    /// Uses `interval=1d&range=1d` to get the most recent market data.
    pub async fn fetch_yahoo_quote(&self, symbol: &str) -> Result<YahooQuote, String> {
        let item = self.fetch_chart_raw(symbol, "1d", "1d").await?;

        let meta = item
            .get("meta")
            .ok_or_else(|| format!("no meta in Yahoo response for {symbol}"))?;

        let response_symbol = meta
            .get("symbol")
            .and_then(|v| v.as_str())
            .unwrap_or(symbol)
            .to_string();

        let price = meta
            .get("regularMarketPrice")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| format!("no market price for {symbol}"))?;

        let prev_close = meta
            .get("chartPreviousClose")
            .and_then(|v| v.as_f64())
            .unwrap_or(price);

        let change = price - prev_close;
        let change_pct = if prev_close != 0.0 {
            change / prev_close * 100.0
        } else {
            0.0
        };

        let timestamp = meta
            .get("regularMarketTime")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        Ok(YahooQuote {
            symbol: response_symbol,
            name: resolve_symbol_name(symbol),
            price,
            change,
            change_pct,
            previous_close: prev_close,
            timestamp,
        })
    }

    /// Fetch historical daily bars for a Yahoo symbol.
    ///
    /// `days` controls the lookback window (e.g. 30 for 1 month).
    /// Returns bars in chronological order (oldest first).
    pub async fn fetch_yahoo_history(
        &self,
        symbol: &str,
        days: u32,
    ) -> Result<Vec<YahooBar>, String> {
        // Map day count to Yahoo range string
        let range = match days {
            0 => return Ok(Vec::new()),
            1 => "1d",
            5 => "5d",
            30 => "1mo",
            90 => "3mo",
            180 => "6mo",
            365 => "1y",
            _ => {
                if days <= 7 {
                    "5d"
                } else if days <= 30 {
                    "1mo"
                } else if days <= 90 {
                    "3mo"
                } else if days <= 180 {
                    "6mo"
                } else {
                    "1y"
                }
            }
        };

        let item = self.fetch_chart_raw(symbol, "1d", range).await?;

        let meta_symbol = item
            .get("meta")
            .and_then(|m| m.get("symbol"))
            .and_then(|v| v.as_str())
            .unwrap_or(symbol)
            .to_string();

        // Extract timestamps
        let timestamps: Vec<i64> = item["timestamp"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default();

        // Extract OHLCV from indicators.quote[0]
        let quote = item
            .get("indicators")
            .and_then(|i| i.get("quote"))
            .and_then(|q| q.as_array())
            .and_then(|arr| arr.first())
            .ok_or("missing indicators.quote in Yahoo response")?;

        let extract_f64_array = |key: &str| -> Vec<f64> {
            quote
                .get(key)
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
                .unwrap_or_default()
        };

        let opens = extract_f64_array("open");
        let highs = extract_f64_array("high");
        let lows = extract_f64_array("low");
        let closes = extract_f64_array("close");
        let volumes: Vec<f64> = extract_f64_array("volume");

        let mut bars = Vec::with_capacity(timestamps.len());

        for (i, &ts) in timestamps.iter().enumerate() {
            let date = chrono::DateTime::from_timestamp(ts, 0)
                .map(|dt| dt.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| format!("ts_{ts}"));

            bars.push(YahooBar {
                symbol: meta_symbol.clone(),
                date,
                open: opens.get(i).copied().unwrap_or(0.0),
                high: highs.get(i).copied().unwrap_or(0.0),
                low: lows.get(i).copied().unwrap_or(0.0),
                close: closes.get(i).copied().unwrap_or(0.0),
                volume: volumes.get(i).copied().unwrap_or(0.0) as u64,
            });
        }

        // Sort chronologically (oldest first)
        bars.sort_by(|a, b| a.date.cmp(&b.date));

        Ok(bars)
    }

    /// Fetch quotes for all well-known international symbols sequentially.
    ///
    /// Requests are spaced 500ms apart to avoid triggering Yahoo's rate limiter.
    pub async fn fetch_all_quotes(&self) -> Vec<Result<YahooQuote, String>> {
        let mut results = Vec::with_capacity(INTERNATIONAL_SYMBOLS.len());
        for (i, (sym, _)) in INTERNATIONAL_SYMBOLS.iter().enumerate() {
            if i > 0 {
                tokio::time::sleep(Duration::from_millis(YAHOO_REQUEST_INTERVAL_MS)).await;
            }
            results.push(self.fetch_yahoo_quote(sym).await);
        }
        results
    }

    /// Search for news articles via Yahoo Finance search API.
    ///
    /// `query` is the search string (e.g. "A股 央行", "中国股市").
    /// `count` is the maximum number of news items to return (default 10).
    /// Retries up to 3 times with exponential back-off (1s, 2s, 4s) on 429 / 5xx.
    pub async fn fetch_yahoo_news(
        &self,
        query: &str,
        count: u32,
    ) -> Result<Vec<YahooNewsItem>, String> {
        let url = YAHOO_SEARCH_API;
        let max_retries = 3u32;
        let mut last_err: Option<String> = None;

        for attempt in 0..max_retries {
            let resp = self
                .client
                .get(url)
                .query(&[
                    ("q", query),
                    ("quotesCount", "0"),
                    ("newsCount", &count.to_string()),
                ])
                .send()
                .await
                .map_err(|e| format!("Yahoo news search request failed: {e}"))?;

            let status = resp.status();

            // Retry on 429 or 5xx with exponential back-off
            if status.as_u16() == 429 || status.is_server_error() {
                let backoff_ms = 1000 * (1u64 << attempt);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                last_err = Some(format!(
                    "Yahoo news search HTTP {status} (attempt {}/{max_retries})",
                    attempt + 1
                ));
                continue;
            }

            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("Yahoo news search HTTP {status}: {body}"));
            }

            let text = resp
                .text()
                .await
                .map_err(|e| format!("failed to read Yahoo news response: {e}"))?;

            let parsed: YahooSearchResponse = serde_json::from_str(&text)
                .map_err(|e| format!("Yahoo news json parse error: {e}"))?;

            let items = parsed
                .news
                .into_iter()
                .filter_map(|item| {
                    Some(YahooNewsItem {
                        uuid: item.uuid.unwrap_or_default(),
                        title: item.title.unwrap_or_default(),
                        publisher: item.publisher.unwrap_or_default(),
                        link: item.link.unwrap_or_default(),
                        provider_publish_time: item.provider_publish_time.unwrap_or(0),
                        related_tickers: item.related_tickers.unwrap_or_default(),
                    })
                })
                .filter(|item| !item.title.is_empty())
                .collect();

            return Ok(items);
        }

        Err(last_err.unwrap_or_else(|| {
            format!("Yahoo news search failed after {max_retries} attempts")
        }))
    }

    /// Fetch Chinese financial news from Yahoo Finance using multiple search queries.
    /// Returns deduplicated news items sorted by publish time (newest first).
    ///
    /// Queries are spaced 500ms apart to avoid triggering Yahoo's rate limiter.
    pub async fn fetch_china_finance_news(&self, max_items: usize) -> Vec<YahooNewsItem> {
        let queries = ["A股 中国", "中国股市", "央行 中国", "A股 市场"];
        let per_query = ((max_items as u32) / queries.len() as u32).max(3);

        let mut all: Vec<YahooNewsItem> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for (i, q) in queries.iter().enumerate() {
            if i > 0 {
                tokio::time::sleep(Duration::from_millis(YAHOO_REQUEST_INTERVAL_MS)).await;
            }
            if let Ok(items) = self.fetch_yahoo_news(q, per_query).await {
                for item in items {
                    if item.uuid.is_empty() || seen.insert(item.uuid.clone()) {
                        all.push(item);
                    }
                }
            }
        }

        all.sort_by(|a, b| b.provider_publish_time.cmp(&a.provider_publish_time));
        all.truncate(max_items);
        all
    }
}

// ---------------------------------------------------------------------------
// Symbol name resolution
// ---------------------------------------------------------------------------

/// Resolve a Yahoo symbol to a human-readable Chinese name.
pub fn resolve_symbol_name(symbol: &str) -> String {
    INTERNATIONAL_SYMBOLS
        .iter()
        .find(|(s, _)| *s == symbol)
        .map(|(_, name)| name.to_string())
        .unwrap_or_else(|| symbol.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_symbol_name_known() {
        assert_eq!(resolve_symbol_name("^VIX"), "VIX 恐慌指数");
        assert_eq!(resolve_symbol_name("GC=F"), "国际金价");
        assert_eq!(resolve_symbol_name("USDCNY=X"), "USD/CNY 汇率");
    }

    #[test]
    fn resolve_symbol_name_unknown() {
        assert_eq!(resolve_symbol_name("AAPL"), "AAPL");
    }

    #[test]
    fn international_symbols_count() {
        assert_eq!(INTERNATIONAL_SYMBOLS.len(), 6);
    }
}
