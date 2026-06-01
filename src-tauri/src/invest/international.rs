//! International market indicators via Python yfinance backend.
//!
//! Delegates all Yahoo Finance API calls to the embedded Python data server
//! (`python-runtime/scripts/server.py`) via JSON-RPC over stdin/stdout.

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Data structures (unchanged — consumed by macro_refresh, event_scanner, etc.)
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

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Well-known international indicator symbols.
pub const INTERNATIONAL_SYMBOLS: &[(&str, &str)] = &[
    ("^VIX", "VIX 恐慌指数"),
    ("^TNX", "美10Y国债收益率"),
    ("DX-Y.NYB", "美元指数"),
    ("GC=F", "国际金价"),
    ("CL=F", "国际油价"),
    ("USDCNY=X", "USD/CNY 汇率"),
];

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// Thin wrapper around the Python data server for Yahoo Finance operations.
///
/// All actual HTTP requests are handled by `python-runtime/scripts/providers/yahoo.py`
/// via the JSON-RPC bridge.
pub struct InternationalClient;

impl InternationalClient {
    /// Build from user settings (kept for API compatibility).
    pub fn from_settings() -> Self {
        Self
    }

    /// Get the global Python runtime reference.
    fn runtime() -> Result<&'static std::sync::Arc<crate::python::PythonRuntime>, String> {
        crate::python::require()
    }

    // -- high-level helpers --------------------------------------------------

    /// Fetch real-time quote for a Yahoo symbol.
    pub async fn fetch_yahoo_quote(&self, symbol: &str) -> Result<YahooQuote, String> {
        let params = serde_json::json!({"symbol": symbol});
        let result = Self::runtime()?.call("yahoo.quote", params).await?;
        serde_json::from_value(result).map_err(|e| format!("Failed to parse YahooQuote: {e}"))
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
        let params = serde_json::json!({"symbol": symbol, "days": days});
        let result = Self::runtime()?.call("yahoo.history", params).await?;
        serde_json::from_value(result).map_err(|e| format!("Failed to parse YahooBar list: {e}"))
    }

    /// Search for news articles via Yahoo Finance search API.
    ///
    /// `query` is the search string (e.g. "A股 央行", "中国股市").
    /// `count` is the maximum number of news items to return (default 10).
    pub async fn fetch_yahoo_news(
        &self,
        query: &str,
        count: u32,
    ) -> Result<Vec<YahooNewsItem>, String> {
        let params = serde_json::json!({"query": query, "count": count});
        let result = Self::runtime()?.call("yahoo.news", params).await?;
        serde_json::from_value(result).map_err(|e| format!("Failed to parse YahooNewsItem list: {e}"))
    }

    /// Fetch Chinese financial news from Yahoo Finance using multiple search queries.
    /// Returns deduplicated news items sorted by publish time (newest first).
    ///
    /// The Python yfinance library handles its own rate limiting internally,
    /// so no artificial delays are needed on the Rust side.
    pub async fn fetch_china_finance_news(&self, max_items: usize) -> Vec<YahooNewsItem> {
        let queries = ["A股 中国", "中国股市", "央行 中国", "A股 市场"];
        let per_query = ((max_items as u32) / queries.len() as u32).max(3);

        let mut all: Vec<YahooNewsItem> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for q in queries.iter() {
            if let Ok(items) = self.fetch_yahoo_news(q, per_query).await {
                for item in items {
                    if item.uuid.is_empty() || seen.insert(item.uuid.clone()) {
                        all.push(item);
                    }
                }
            }
        }

        all.sort_by_key(|b| std::cmp::Reverse(b.provider_publish_time));
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
