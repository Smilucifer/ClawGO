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
    #[serde(alias = "change_pct")]
    pub change_pct: f64,
    #[serde(alias = "previous_close")]
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

/// A single news item from various providers (Yahoo Finance, AkShare, Jin10, etc.).
///
/// All Python providers emit snake_case keys, so no `rename_all` is applied.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct NewsItem {
    pub uuid: String,
    pub title: String,
    pub publisher: String,
    pub link: String,
    pub provider_publish_time: i64,
    pub related_tickers: Vec<String>,
}

/// China 10Y government bond yield from AkShare.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct BondYield10y {
    pub yield_10y: f64,
    pub date: String,
}

/// A-share market statistics from AkShare.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct MarketStats {
    pub limit_up_count: u32,
    pub limit_down_count: u32,
    pub date: String,
}

/// A-share market-wide advance/decline counts from AkShare.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct AdvanceDecline {
    pub advance_count: u32,
    pub decline_count: u32,
    pub date: String,
}

/// miniQMT 客户端健康状态。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct XtdataHealth {
    pub available: bool,
    #[serde(default)]
    pub reason: String,
}

/// miniQMT 单根 K线（字段已由 xtdata.py 归一化为 tushare 风格）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct XtdataKlineBar {
    pub trade_date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub vol: f64,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct XtdataKlineResp {
    items: Vec<XtdataKlineBar>,
}

/// miniQMT 全 A 市场广度快照。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MarketBreadth {
    pub available: bool,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub up: u32,
    #[serde(default)]
    pub flat: u32,
    #[serde(default)]
    pub down: u32,
    #[serde(default)]
    pub limit_up: u32,
    #[serde(default)]
    pub limit_down: u32,
    #[serde(default)]
    pub up_over_3pct: u32,
    #[serde(default)]
    pub valid: u32,
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
#[derive(Clone)]
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

    /// Generic JSON-RPC call to the Python data server.
    /// Deserializes the result into the target type.
    async fn rpc_call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<T, String> {
        let result = Self::runtime()?.call(method, params).await?;
        serde_json::from_value(result)
            .map_err(|e| format!("Failed to parse {method} response: {e}"))
    }

    // -- Yahoo Finance (quote + history) --------------------------------------

    /// Fetch real-time quote for a Yahoo symbol.
    pub async fn fetch_yahoo_quote(&self, symbol: &str) -> Result<YahooQuote, String> {
        self.rpc_call("yahoo.quote", serde_json::json!({"symbol": symbol}))
            .await
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
        self.rpc_call(
            "yahoo.history",
            serde_json::json!({"symbol": symbol, "days": days}),
        )
        .await
    }

    // -- Jin10 provider (金十数据) --------------------------------------------

    /// Fetch flash news from Jin10 (金十数据).
    /// Returns items compatible with NewsItem schema.
    ///
    /// # Arguments
    /// * `query` - Optional keyword filter
    /// * `count` - Max items to return
    /// * `channel` - Channel filter (None=all, Some(2)=A-share, Some(3)=commodity, Some(4)=bond, Some(5)=international)
    pub async fn fetch_jinshi_news(
        &self,
        query: &str,
        count: u32,
        channel: Option<u32>,
    ) -> Result<Vec<NewsItem>, String> {
        let mut params = serde_json::json!({"query": query, "count": count});
        if let Some(ch) = channel {
            params["channel"] = serde_json::json!(ch);
        }
        self.rpc_call("jinshi.news", params).await
    }

    /// Fetch all flash news from Jin10 (金十数据) — all channels.
    /// No query filter, returns the full feed.
    pub async fn fetch_jinshi_all_news(&self, max_items: usize) -> Vec<NewsItem> {
        match self.fetch_jinshi_news("", max_items as u32, None).await {
            Ok(items) => items,
            Err(e) => {
                log::warn!("fetch_jinshi_all_news failed: {}", e);
                Vec::new()
            }
        }
    }

    /// Fetch A-share related flash news from Jin10 (金十数据).
    /// Filters for A-share channel (channel=2).
    pub async fn fetch_jinshi_a_share_news(&self, max_items: usize) -> Vec<NewsItem> {
        match self.fetch_jinshi_news("", max_items as u32, Some(2)).await {
            Ok(items) => items,
            Err(e) => {
                log::warn!("fetch_jinshi_a_share_news failed: {}", e);
                Vec::new()
            }
        }
    }

    // -- AkShare provider (东财个股新闻 via AkShare) ---------------------------

    /// Fetch per-stock news from EastMoney via AkShare.
    /// `symbol` is the A-share stock code (e.g. "600519").
    pub async fn fetch_akshare_stock_news(
        &self,
        symbol: &str,
        count: u32,
    ) -> Result<Vec<NewsItem>, String> {
        self.rpc_call(
            "akshare.stock_news",
            serde_json::json!({"symbol": symbol, "count": count}),
        )
        .await
    }

    // -- AkShare market stats (国债收益率 + 涨跌停家数) -------------------------

    /// Fetch China 10Y government bond yield from AkShare.
    pub async fn fetch_akshare_bond_yield(&self) -> Result<BondYield10y, String> {
        self.rpc_call("akshare_market.bond_yield_10y", serde_json::json!({}))
            .await
    }

    /// Fetch A-share market statistics (limit-up / limit-down counts) from AkShare.
    ///
    /// `date` is `"YYYYMMDD"` format; empty string defaults to today.
    pub async fn fetch_akshare_market_stats(&self, date: &str) -> Result<MarketStats, String> {
        self.rpc_call(
            "akshare_market.market_stats",
            serde_json::json!({"date": date}),
        )
        .await
    }

    /// Fetch A-share market-wide advance/decline counts from AkShare.
    ///
    /// `date` is `"YYYYMMDD"` format; empty string defaults to today.
    pub async fn fetch_akshare_advance_decline(&self, date: &str) -> Result<AdvanceDecline, String> {
        self.rpc_call(
            "akshare_market.market_advance_decline",
            serde_json::json!({"date": date}),
        )
        .await
    }

    // -- miniQMT provider (xtdata) --------------------------------------------

    /// 探测 miniQMT 客户端是否在线。不在线返回 available=false（不报错）。
    pub async fn fetch_xtdata_health(&self) -> Result<XtdataHealth, String> {
        self.rpc_call("xtdata.health", serde_json::json!({})).await
    }

    /// 获取 miniQMT 历史 K线。
    pub async fn fetch_xtdata_kline(
        &self,
        symbol: &str,
        period: &str,
        count: u32,
    ) -> Result<Vec<XtdataKlineBar>, String> {
        let resp: XtdataKlineResp = self
            .rpc_call(
                "xtdata.kline",
                serde_json::json!({ "symbol": symbol, "period": period, "count": count }),
            )
            .await?;
        Ok(resp.items)
    }

    /// 获取 miniQMT 全 A 市场广度快照。QMT 离线时返回 available=false。
    pub async fn fetch_xtdata_breadth(&self) -> Result<MarketBreadth, String> {
        self.rpc_call("xtdata.market_breadth", serde_json::json!({})).await
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
