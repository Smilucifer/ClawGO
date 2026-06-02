use serde::{Deserialize, Serialize};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// Response envelope from Tushare Pro API.
#[derive(Debug, Deserialize)]
pub struct TushareResponse {
    pub code: i64,
    pub msg: Option<String>,
    pub data: TushareResponseData,
}

#[derive(Debug, Deserialize)]
pub struct TushareResponseData {
    pub fields: Vec<String>,
    pub items: Vec<Vec<serde_json::Value>>,
}

/// A single daily bar (日线行情).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyBar {
    pub ts_code: String,
    pub trade_date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub pre_close: f64,
    pub change: f64,
    pub pct_chg: f64,
    pub vol: f64,
    pub amount: f64,
}

/// A real-time quote (实时行情, via `rt_k` API).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeQuote {
    pub ts_code: String,
    pub name: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub pre_close: f64,
    pub vol: f64,
    pub amount: f64,
    pub trade_time: String,
}

/// Stock basic info (股票列表).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockBasic {
    pub ts_code: String,
    pub symbol: String,
    pub name: String,
    pub area: String,
    pub industry: String,
    pub market: String,
    pub list_date: String,
}

/// Fund basic info (基金列表, includes ETFs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundBasic {
    pub ts_code: String,
    pub name: String,
}

/// Trade calendar entry (交易日历).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeCal {
    pub exchange: String,
    pub cal_date: String,
    pub is_open: i64,
    pub pretrade_date: String,
}

/// A single major news item (新闻通讯).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MajorNewsItem {
    pub datetime: String,
    pub title: String,
    pub content: String,
    pub src: String,
}

/// A single company announcement (上市公司公告).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announcement {
    pub ann_date: String,
    pub ts_code: String,
    pub name: String,
    pub title: String,
    pub url: String,
}

/// A single HSGT money flow entry (沪深港通资金流向).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoneyflowHsgt {
    pub trade_date: String,
    pub north_money: f64,
    pub south_money: f64,
    pub net_money: f64,
}

/// A single margin trading detail entry (融资融券交易明细).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginDetail {
    pub trade_date: String,
    pub rzye: f64,
    pub rzmre: f64,
    pub rzche: f64,
}

/// A single SHIBOR entry (上海银行间同业拆放利率).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shibor {
    pub date: String,
    pub on: f64,
    pub w1: f64,
    pub m1: f64,
    pub m3: f64,
}

/// A single China government bond yield entry (中国国债收益率).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CnBondYield {
    pub ts_code: String,
    pub yield_10y: f64,
}

/// Daily basic indicators for a stock (每日指标).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyBasic {
    pub ts_code: String,
    pub trade_date: String,
    pub close: Option<f64>,
    pub turnover_rate: Option<f64>,
    pub turnover_rate_f: Option<f64>,
    pub volume_ratio: Option<f64>,
    pub pe: Option<f64>,
    pub pe_ttm: Option<f64>,
    pub pb: Option<f64>,
    pub ps: Option<f64>,
    pub ps_ttm: Option<f64>,
    pub dv_ratio: Option<f64>,
    pub dv_ttm: Option<f64>,
    pub total_share: Option<f64>,
    pub float_share: Option<f64>,
    pub free_share: Option<f64>,
    pub total_mv: Option<f64>,
    pub circ_mv: Option<f64>,
}

/// Financial indicators for a stock (财务指标).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinaIndicator {
    pub ts_code: String,
    pub ann_date: Option<String>,
    pub end_date: Option<String>,
    pub roe: Option<f64>,
    pub roe_waa: Option<f64>,
    pub roe_dt: Option<f64>,
    pub roa: Option<f64>,
    pub netprofit_yoy: Option<f64>,
    pub or_yoy: Option<f64>,
    pub debt_to_assets: Option<f64>,
}

/// Analyst ratings for a stock (机构评级).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportRc {
    pub ts_code: String,
    pub report_date: Option<String>,
    pub org_num: Option<f64>,
    pub buy_num: Option<f64>,
    pub hold_num: Option<f64>,
    pub reduce_num: Option<f64>,
    pub sell_num: Option<f64>,
    pub strong_buy_num: Option<f64>,
    pub rating_avg: Option<f64>,
}

/// Individual stock money flow detail (个股资金流向).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyflowDc {
    pub ts_code: String,
    pub trade_date: String,
    pub buy_sm_vol: Option<f64>,
    pub sell_sm_vol: Option<f64>,
    pub buy_md_vol: Option<f64>,
    pub sell_md_vol: Option<f64>,
    pub buy_lg_vol: Option<f64>,
    pub sell_lg_vol: Option<f64>,
    pub buy_elg_vol: Option<f64>,
    pub sell_elg_vol: Option<f64>,
    pub net_mf_vol: Option<f64>,
}

impl MoneyflowDc {
    /// 聚合主力/散户净流入（万手）
    /// 主力 = 超大单 + 大单 净流入
    /// 散户 = 中单 + 小单 净流入
    pub fn aggregate_moneyflow(rows: &[MoneyflowDc]) -> (f64, f64) {
        let mut main_net = 0.0;
        let mut retail_net = 0.0;
        for r in rows {
            main_net += r.buy_elg_vol.unwrap_or(0.0) - r.sell_elg_vol.unwrap_or(0.0)
                      + r.buy_lg_vol.unwrap_or(0.0) - r.sell_lg_vol.unwrap_or(0.0);
            retail_net += r.buy_md_vol.unwrap_or(0.0) - r.sell_md_vol.unwrap_or(0.0)
                        + r.buy_sm_vol.unwrap_or(0.0) - r.sell_sm_vol.unwrap_or(0.0);
        }
        (main_net, retail_net)
    }

    /// 格式化资金流向摘要
    pub fn format_moneyflow_summary(rows: &[MoneyflowDc]) -> String {
        let (main_net, retail_net) = Self::aggregate_moneyflow(rows);
        let main_label = if main_net >= 0.0 { "主力净流入" } else { "主力净流出" };
        let retail_label = if retail_net >= 0.0 { "散户净流入" } else { "散户净流出" };
        format!("{} {:.1}万手，{} {:.1}万手", main_label, main_net.abs(), retail_label, retail_net.abs())
    }
}

// ---------------------------------------------------------------------------
// Row helpers — extract typed values from a positional row slice
// ---------------------------------------------------------------------------

/// Extract a `String` from `row[idx]`.
/// Returns `None` if the index is out of bounds or the value is null.
pub fn get_str(row: &[serde_json::Value], idx: usize) -> Option<String> {
    row.get(idx).and_then(|v| {
        if v.is_null() {
            None
        } else if let Some(s) = v.as_str() {
            Some(s.to_string())
        } else {
            Some(v.to_string())
        }
    })
}

/// Extract an `f64` from `row[idx]`.
/// Returns `None` if the index is out of bounds, the value is null, or not numeric.
pub fn get_f64(row: &[serde_json::Value], idx: usize) -> Option<f64> {
    row.get(idx).and_then(|v| {
        if v.is_null() {
            return None;
        }
        match v {
            serde_json::Value::Number(n) => n.as_f64(),
            serde_json::Value::String(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    })
}

/// Extract an `i64` from `row[idx]`.
/// Returns `None` if the index is out of bounds, the value is null, or not numeric.
pub fn get_i64(row: &[serde_json::Value], idx: usize) -> Option<i64> {
    row.get(idx).and_then(|v| {
        if v.is_null() {
            return None;
        }
        match v {
            serde_json::Value::Number(n) => n.as_i64(),
            serde_json::Value::String(s) => s.parse::<i64>().ok(),
            _ => None,
        }
    })
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// Default Tushare Pro official API endpoint.
const TUSHARE_OFFICIAL_URL: &str = "https://api.tushare.pro";

/// HTTP client for the Tushare Pro API.
#[derive(Clone)]
pub struct TushareClient {
    token: String,
    base_url: String,
    client: reqwest::Client,
}

/// Validate that a URL has an http/https scheme. Returns error message if invalid.
fn validate_url_scheme(url: &str) -> Result<(), String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(())
    } else {
        Err(format!(
            "invalid URL scheme (expected http:// or https://): {url}"
        ))
    }
}

impl TushareClient {
    /// Create a new client with an explicit token and base URL.
    ///
    /// **Callers must validate `base_url` before calling** (e.g. via
    /// `validate_url_scheme`). This constructor trusts its inputs for
    /// flexibility; the factory methods apply validation automatically.
    pub fn new(token: String, base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client");
        Self { token, base_url, client }
    }

    /// Resolve a base URL from an optional proxy URL string.
    /// Filters empty values, defaults to the official Tushare API, and
    /// validates the URL scheme. On validation failure, logs a warning
    /// and falls back to the official URL.
    fn resolve_base_url(proxy_url: Option<&str>) -> String {
        let raw = proxy_url
            .filter(|u| !u.is_empty())
            .unwrap_or(TUSHARE_OFFICIAL_URL);
        if raw == TUSHARE_OFFICIAL_URL {
            return raw.to_string();
        }
        match validate_url_scheme(raw) {
            Ok(()) => raw.to_string(),
            Err(e) => {
                log::warn!("[tushare] invalid proxy URL, falling back to official: {e}");
                TUSHARE_OFFICIAL_URL.to_string()
            }
        }
    }

    /// Build from `UserSettings`: reads `tushare_token` (required) and
    /// `tushare_proxy_url` (optional, defaults to official API).
    pub fn from_settings() -> Result<Self, String> {
        let settings = crate::storage::settings::get_user_settings();
        let token = settings
            .tushare_token
            .ok_or_else(|| "tushare_token not configured".to_string())?;
        let base_url = Self::resolve_base_url(settings.tushare_proxy_url.as_deref());
        validate_url_scheme(&base_url)?;
        Ok(Self::new(token, base_url))
    }

    /// Build with an explicit token (e.g. passed from frontend) but proxy URL
    /// from `UserSettings`. Falls back to the official API if not configured.
    pub fn with_token(token: String) -> Self {
        let settings = crate::storage::settings::get_user_settings();
        let base_url = Self::resolve_base_url(settings.tushare_proxy_url.as_deref());
        Self::new(token, base_url)
    }

    /// Build with an explicit token and an explicit proxy URL.
    /// Avoids re-reading settings when the caller already has the proxy URL.
    /// Falls back to the official API if `proxy_url` is `None` or empty.
    pub fn with_token_and_proxy(token: String, proxy_url: Option<String>) -> Self {
        let base_url = Self::resolve_base_url(proxy_url.as_deref());
        Self::new(token, base_url)
    }

    // -- low-level -----------------------------------------------------------

    /// Generic POST to the Tushare Pro API with automatic retry on 429 / 5xx.
    ///
    /// Retries up to 3 times with exponential back-off (1s, 2s, 4s).
    pub async fn call_api(
        &self,
        api_name: &str,
        params: serde_json::Value,
        fields: &str,
    ) -> Result<TushareResponse, String> {
        let body = serde_json::json!({
            "api_name": api_name,
            "token": self.token,
            "params": params,
            "fields": fields,
        });

        let max_retries = 3u32;
        let mut last_err: Option<String> = None;

        for attempt in 0..max_retries {
            let resp = self
                .client
                .post(&self.base_url)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("request failed: {e}"))?;

            let status = resp.status();

            // Retry on 429 or 5xx
            if status.as_u16() == 429 || status.is_server_error() {
                last_err = Some(format!("HTTP {status} (attempt {}/{max_retries})", attempt + 1));
                if attempt + 1 < max_retries {
                    let backoff_ms = 1000 * (1u64 << attempt);
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                }
                continue;
            }

            if !status.is_success() {
                return Err(format!("HTTP {status}"));
            }

            let text = resp
                .text()
                .await
                .map_err(|e| format!("failed to read body: {e}"))?;

            let parsed: TushareResponse =
                serde_json::from_str(&text).map_err(|e| format!("json parse error: {e}"))?;

            if parsed.code != 0 {
                return Err(format!("tushare error {}: {}", parsed.code, parsed.msg.unwrap_or_else(|| "(no message)".into())));
            }

            return Ok(parsed);
        }

        Err(last_err.unwrap_or_else(|| "max retries exceeded".into()))
    }

    // -- high-level helpers --------------------------------------------------

    /// 根据 ts_code 前缀选择 Tushare 日线 API。
    /// ETF/基金用 `fund_daily`，股票用 `daily`。
    fn daily_api(ts_code: &str) -> &'static str {
        let prefix = ts_code.split('.').next().unwrap_or("");
        if matches!(
            prefix.get(..3).unwrap_or(""),
            "159" | "510" | "512" | "515" | "588" | "150" | "500" | "501"
                | "160" | "161" | "162" | "163" | "164"
        ) {
            "fund_daily"
        } else {
            "daily"
        }
    }

    /// 判断 ts_code 是否为 ETF/基金代码。
    fn is_etf_code(ts_code: &str) -> bool {
        Self::daily_api(ts_code) == "fund_daily"
    }

    /// Fetch daily bars (日线行情) for a stock or ETF within a date range.
    /// Automatically selects `fund_daily` for ETF codes and `daily` for stocks.
    pub async fn daily(
        &self,
        ts_code: &str,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<DailyBar>, String> {
        let params = serde_json::json!({
            "ts_code": ts_code,
            "start_date": start_date,
            "end_date": end_date,
        });

        let resp = self.call_api(Self::daily_api(ts_code), params, "").await?;
        let fields = &resp.data.fields;

        let ts_code_idx = fields.iter().position(|f| f == "ts_code");
        let trade_date_idx = fields.iter().position(|f| f == "trade_date");
        let open_idx = fields.iter().position(|f| f == "open");
        let high_idx = fields.iter().position(|f| f == "high");
        let low_idx = fields.iter().position(|f| f == "low");
        let close_idx = fields.iter().position(|f| f == "close");
        let pre_close_idx = fields.iter().position(|f| f == "pre_close");
        let change_idx = fields.iter().position(|f| f == "change");
        let pct_chg_idx = fields.iter().position(|f| f == "pct_chg");
        let vol_idx = fields.iter().position(|f| f == "vol");
        let amount_idx = fields.iter().position(|f| f == "amount");

        let mut bars = Vec::with_capacity(resp.data.items.len());

        for row in &resp.data.items {
            let get = |idx: Option<usize>| -> Option<f64> {
                idx.and_then(|i| get_f64(row, i))
            };

            bars.push(DailyBar {
                ts_code: ts_code_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                trade_date: trade_date_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                open: get(open_idx).unwrap_or_default(),
                high: get(high_idx).unwrap_or_default(),
                low: get(low_idx).unwrap_or_default(),
                close: get(close_idx).unwrap_or_default(),
                pre_close: get(pre_close_idx).unwrap_or_default(),
                change: get(change_idx).unwrap_or_default(),
                pct_chg: get(pct_chg_idx).unwrap_or_default(),
                vol: get(vol_idx).unwrap_or_default(),
                amount: get(amount_idx).unwrap_or_default(),
            });
        }

        Ok(bars)
    }

    /// Search stocks by optional name or ts_code (精确匹配 ts_code 优先).
    /// If `name` is `None`, returns the first 50 stocks.
    pub async fn stock_basic(&self, name: Option<&str>) -> Result<Vec<StockBasic>, String> {
        // If query looks like a ts_code (e.g. "600519" or "600519.SH"), try exact match first
        if let Some(n) = name {
            let trimmed = n.trim();
            // Precise ts_code format: 6 digits, optionally followed by .SH or .SZ
            let is_ts_code = trimmed.len() >= 6
                && trimmed.chars().take(6).all(|c| c.is_ascii_digit())
                && (trimmed.len() == 6 || trimmed.eq_ignore_ascii_case("6") || {
                    let rest = &trimmed[6..];
                    rest.eq_ignore_ascii_case(".SH") || rest.eq_ignore_ascii_case(".SZ")
                });
            if is_ts_code && !trimmed.is_empty() {
                // Try exact ts_code match first
                let ts_code_param = if trimmed.contains('.') {
                    trimmed.to_string()
                } else {
                    format!("{}.{}", trimmed, if trimmed.starts_with('6') { "SH" } else { "SZ" })
                };
                let exact_params = serde_json::json!({
                    "ts_code": ts_code_param,
                    "list_status": "L",
                });
                if let Ok(resp) = self.call_api("stock_basic", exact_params, "").await {
                    if !resp.data.items.is_empty() {
                        return self.parse_stock_basic_response(&resp);
                    }
                }
                // Fallback: search by name (fuzzy)
                let params = serde_json::json!({ "name": trimmed });
                let resp = self.call_api("stock_basic", params, "").await?;
                let mut results = self.parse_stock_basic_response(&resp)?;
                // If name search returns nothing, try broader symbol search
                if results.is_empty() {
                    let symbol_params = serde_json::json!({
                        "symbol": trimmed,
                        "list_status": "L",
                    });
                    if let Ok(resp2) = self.call_api("stock_basic", symbol_params, "").await {
                        results = self.parse_stock_basic_response(&resp2)?;
                    }
                }
                return Ok(results);
            }
            // Regular name search
            let params = serde_json::json!({ "name": trimmed });
            let resp = self.call_api("stock_basic", params, "").await?;
            return self.parse_stock_basic_response(&resp);
        }

        let params = serde_json::json!({});
        let resp = self.call_api("stock_basic", params, "").await?;
        self.parse_stock_basic_response(&resp)
    }

    fn parse_stock_basic_response(&self, resp: &TushareResponse) -> Result<Vec<StockBasic>, String> {
        let fields = &resp.data.fields;
        let ts_code_idx = fields.iter().position(|f| f == "ts_code");
        let symbol_idx = fields.iter().position(|f| f == "symbol");
        let name_idx = fields.iter().position(|f| f == "name");
        let area_idx = fields.iter().position(|f| f == "area");
        let industry_idx = fields.iter().position(|f| f == "industry");
        let market_idx = fields.iter().position(|f| f == "market");
        let list_date_idx = fields.iter().position(|f| f == "list_date");

        let mut stocks = Vec::with_capacity(resp.data.items.len());

        for row in &resp.data.items {
            stocks.push(StockBasic {
                ts_code: ts_code_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                symbol: symbol_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                name: name_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                area: area_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                industry: industry_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                market: market_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                list_date: list_date_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
            });
        }

        Ok(stocks)
    }

    /// Search ETFs by optional name or ts_code (模糊匹配). Filters to listed ETFs only.
    /// Supports code-based lookup: if query looks like a numeric code, also matches on ts_code.
    pub async fn fund_basic(&self, name: Option<&str>) -> Result<Vec<FundBasic>, String> {
        let params = serde_json::json!({
            "fund_type": "E",  // ETF only
            "status": "L",     // Listed only
        });
        // Note: Tushare fund_basic API ignores the `name` param, so we fetch
        // all listed ETFs and filter client-side.

        let resp = self.call_api("fund_basic", params, "ts_code,name").await?;
        let fields = &resp.data.fields;

        let ts_code_idx = fields.iter().position(|f| f == "ts_code");
        let name_idx = fields.iter().position(|f| f == "name");

        let query = name.map(|n| n.to_lowercase());
        let mut funds = Vec::with_capacity(resp.data.items.len());

        for row in &resp.data.items {
            let fund_name = name_idx
                .and_then(|i| get_str(row, i))
                .unwrap_or_default();
            let fund_ts_code = ts_code_idx
                .and_then(|i| get_str(row, i))
                .unwrap_or_default();
            // Client-side filter: skip if neither name nor ts_code matches query
            if let Some(ref q) = query {
                let name_match = fund_name.to_lowercase().contains(q);
                let code_match = fund_ts_code.to_lowercase().contains(q)
                    || fund_ts_code.split('.').next().unwrap_or("").to_lowercase().contains(q);
                if !name_match && !code_match {
                    continue;
                }
            }
            funds.push(FundBasic {
                ts_code: fund_ts_code,
                name: fund_name,
            });
        }

        Ok(funds)
    }

    /// Get the latest close price for a given stock or ETF.
    /// Automatically selects `fund_daily` for ETF codes and `daily` for stocks.
    pub async fn get_latest_price(&self, ts_code: &str) -> Result<f64, String> {
        let resp = self
            .call_api(
                Self::daily_api(ts_code),
                serde_json::json!({ "ts_code": ts_code }),
                "ts_code,trade_date,close",
            )
            .await?;

        let fields = &resp.data.fields;
        let close_idx = fields
            .iter()
            .position(|f| f == "close")
            .ok_or("field 'close' not found in response")?;

        resp.data
            .items
            .first()
            .and_then(|row| get_f64(row, close_idx))
            .ok_or_else(|| format!("no daily data for {ts_code}"))
    }

    /// 批量获取实时行情（盘中最新价）。
    ///
    /// - 股票：使用 `rt_k` 接口（返回盘中最新价）
    /// - ETF/基金：`rt_k` 可能不支持，降级到 `fund_daily`（返回前一交易日结算价）
    ///
    /// 返回的 `RealtimeQuote.close` 始终是最新的可用价格。
    pub async fn realtime_quotes(
        &self,
        ts_codes: &[&str],
    ) -> Result<Vec<RealtimeQuote>, String> {
        if ts_codes.is_empty() {
            return Ok(vec![]);
        }

        // Partition into stocks vs ETFs in a single pass
        let (etf_codes, stock_codes): (Vec<&str>, Vec<&str>) =
            ts_codes.iter().partition(|c| Self::is_etf_code(c));

        let mut results: Vec<RealtimeQuote> = Vec::new();

        // ── Stocks via rt_k ────────────────────────────────────────────
        if !stock_codes.is_empty() {
            let codes_str = stock_codes.join(",");
            match self
                .call_api("rt_k", serde_json::json!({ "ts_code": codes_str }), "")
                .await
            {
                Ok(resp) => {
                    results.extend(Self::parse_realtime_quotes(&resp));
                }
                Err(e) => {
                    log::warn!("rt_k failed for stocks, falling back to daily: {e}");
                    let futs: Vec<_> = stock_codes
                        .iter()
                        .map(|c| self.fallback_daily_quote(c))
                        .collect();
                    for quote in futures_util::future::join_all(futs).await {
                        if let Ok(q) = quote {
                            results.push(q);
                        }
                    }
                }
            }
        }

        // ── ETFs via fund_daily (rt_k likely unsupported) ──────────────
        let etf_futs: Vec<_> = etf_codes
            .iter()
            .map(|c| self.fallback_daily_quote(c))
            .collect();
        for quote in futures_util::future::join_all(etf_futs).await {
            if let Ok(q) = quote {
                results.push(q);
            }
        }

        Ok(results)
    }

    /// Parse `rt_k` API response into `RealtimeQuote` list.
    fn parse_realtime_quotes(resp: &TushareResponse) -> Vec<RealtimeQuote> {
        let fields = &resp.data.fields;
        let ts_code_idx = fields.iter().position(|f| f == "ts_code");
        let name_idx = fields.iter().position(|f| f == "name");
        let open_idx = fields.iter().position(|f| f == "open");
        let high_idx = fields.iter().position(|f| f == "high");
        let low_idx = fields.iter().position(|f| f == "low");
        let close_idx = fields.iter().position(|f| f == "close");
        let pre_close_idx = fields.iter().position(|f| f == "pre_close");
        let vol_idx = fields.iter().position(|f| f == "vol");
        let amount_idx = fields.iter().position(|f| f == "amount");
        let trade_time_idx = fields.iter().position(|f| f == "trade_time");

        let mut quotes = Vec::with_capacity(resp.data.items.len());
        for row in &resp.data.items {
            let get = |idx: Option<usize>| -> Option<f64> { idx.and_then(|i| get_f64(row, i)) };
            quotes.push(RealtimeQuote {
                ts_code: ts_code_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                name: name_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                open: get(open_idx).unwrap_or_default(),
                high: get(high_idx).unwrap_or_default(),
                low: get(low_idx).unwrap_or_default(),
                close: get(close_idx).unwrap_or_default(),
                pre_close: get(pre_close_idx).unwrap_or_default(),
                vol: get(vol_idx).unwrap_or_default(),
                amount: get(amount_idx).unwrap_or_default(),
                trade_time: trade_time_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
            });
        }
        quotes
    }

    /// 降级方案：通过 `daily`/`fund_daily` 获取最新一条日线作为实时价替代。
    async fn fallback_daily_quote(&self, ts_code: &str) -> Result<RealtimeQuote, String> {
        let resp = self
            .call_api(
                Self::daily_api(ts_code),
                serde_json::json!({ "ts_code": ts_code }),
                "",
            )
            .await?;

        let fields = &resp.data.fields;
        let ts_code_idx = fields.iter().position(|f| f == "ts_code");
        let open_idx = fields.iter().position(|f| f == "open");
        let high_idx = fields.iter().position(|f| f == "high");
        let low_idx = fields.iter().position(|f| f == "low");
        let close_idx = fields.iter().position(|f| f == "close");
        let pre_close_idx = fields.iter().position(|f| f == "pre_close");
        let vol_idx = fields.iter().position(|f| f == "vol");
        let amount_idx = fields.iter().position(|f| f == "amount");
        let trade_date_idx = fields.iter().position(|f| f == "trade_date");

        let row = resp
            .data
            .items
            .first()
            .ok_or_else(|| format!("no daily data for {ts_code}"))?;

        let get = |idx: Option<usize>| -> Option<f64> { idx.and_then(|i| get_f64(row, i)) };

        Ok(RealtimeQuote {
            ts_code: ts_code_idx
                .and_then(|i| get_str(row, i))
                .unwrap_or_else(|| ts_code.to_string()),
            name: String::new(),
            open: get(open_idx).unwrap_or_default(),
            high: get(high_idx).unwrap_or_default(),
            low: get(low_idx).unwrap_or_default(),
            close: get(close_idx).unwrap_or_default(),
            pre_close: get(pre_close_idx).unwrap_or_default(),
            vol: get(vol_idx).unwrap_or_default(),
            amount: get(amount_idx).unwrap_or_default(),
            trade_time: trade_date_idx
                .and_then(|i| get_str(row, i))
                .unwrap_or_default(),
        })
    }
    pub async fn trade_cal(
        &self,
        exchange: &str,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<TradeCal>, String> {
        let params = serde_json::json!({
            "exchange": exchange,
            "start_date": start_date,
            "end_date": end_date,
        });

        let resp = self.call_api("trade_cal", params, "").await?;
        let fields = &resp.data.fields;

        let exchange_idx = fields.iter().position(|f| f == "exchange");
        let cal_date_idx = fields.iter().position(|f| f == "cal_date");
        let is_open_idx = fields.iter().position(|f| f == "is_open");
        let pretrade_date_idx = fields.iter().position(|f| f == "pretrade_date");

        let mut cal = Vec::with_capacity(resp.data.items.len());

        for row in &resp.data.items {
            cal.push(TradeCal {
                exchange: exchange_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                cal_date: cal_date_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                is_open: is_open_idx
                    .and_then(|i| get_i64(row, i))
                    .unwrap_or_default(),
                pretrade_date: pretrade_date_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
            });
        }

        Ok(cal)
    }

    /// Fetch major news (新闻通讯) for a given source within a date range.
    pub async fn major_news(
        &self,
        src: &str,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<MajorNewsItem>, String> {
        let params = serde_json::json!({
            "src": src,
            "start_date": start_date,
            "end_date": end_date,
        });
        let resp = self.call_api("major_news", params, "").await?;

        let fields = &resp.data.fields;
        let datetime_idx = fields.iter().position(|f| f == "datetime");
        let title_idx = fields.iter().position(|f| f == "title");
        let content_idx = fields.iter().position(|f| f == "content");
        let src_idx = fields.iter().position(|f| f == "src");

        let mut items = Vec::with_capacity(resp.data.items.len());
        for row in &resp.data.items {
            items.push(MajorNewsItem {
                datetime: datetime_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                title: title_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                content: content_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                src: src_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
            });
        }
        Ok(items)
    }

    /// Fetch company announcements (上市公司公告) for a stock within a date range.
    pub async fn anns_d(
        &self,
        ts_code: &str,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<Announcement>, String> {
        let params = serde_json::json!({
            "ts_code": ts_code,
            "start_date": start_date,
            "end_date": end_date,
        });
        let resp = self.call_api("anns_d", params, "").await?;

        let fields = &resp.data.fields;
        let ann_date_idx = fields.iter().position(|f| f == "ann_date");
        let ts_code_idx = fields.iter().position(|f| f == "ts_code");
        let name_idx = fields.iter().position(|f| f == "name");
        let title_idx = fields.iter().position(|f| f == "title");
        let url_idx = fields.iter().position(|f| f == "url");

        let mut items = Vec::with_capacity(resp.data.items.len());
        for row in &resp.data.items {
            items.push(Announcement {
                ann_date: ann_date_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                ts_code: ts_code_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                name: name_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                title: title_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                url: url_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
            });
        }
        Ok(items)
    }

    /// Fetch HSGT money flow (沪深港通资金流向) within a date range.
    pub async fn moneyflow_hsgt(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<MoneyflowHsgt>, String> {
        let params = serde_json::json!({
            "start_date": start_date,
            "end_date": end_date,
        });
        let resp = self.call_api("moneyflow_hsgt", params, "").await?;

        let fields = &resp.data.fields;
        let trade_date_idx = fields.iter().position(|f| f == "trade_date");
        let north_money_idx = fields.iter().position(|f| f == "north_money");
        let south_money_idx = fields.iter().position(|f| f == "south_money");
        let net_money_idx = fields.iter().position(|f| f == "net_money");

        let mut items = Vec::with_capacity(resp.data.items.len());
        for row in &resp.data.items {
            items.push(MoneyflowHsgt {
                trade_date: trade_date_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                north_money: north_money_idx
                    .and_then(|i| get_f64(row, i))
                    .unwrap_or_default(),
                south_money: south_money_idx
                    .and_then(|i| get_f64(row, i))
                    .unwrap_or_default(),
                net_money: net_money_idx
                    .and_then(|i| get_f64(row, i))
                    .unwrap_or_default(),
            });
        }
        Ok(items)
    }

    /// Fetch margin trading detail (融资融券交易明细) within a date range.
    pub async fn margin_detail(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<MarginDetail>, String> {
        let params = serde_json::json!({
            "start_date": start_date,
            "end_date": end_date,
        });
        let resp = self.call_api("margin_detail", params, "").await?;

        let fields = &resp.data.fields;
        let trade_date_idx = fields.iter().position(|f| f == "trade_date");
        let rzye_idx = fields.iter().position(|f| f == "rzye");
        let rzmre_idx = fields.iter().position(|f| f == "rzmre");
        let rzche_idx = fields.iter().position(|f| f == "rzche");

        let mut items = Vec::with_capacity(resp.data.items.len());
        for row in &resp.data.items {
            items.push(MarginDetail {
                trade_date: trade_date_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                rzye: rzye_idx
                    .and_then(|i| get_f64(row, i))
                    .unwrap_or_default(),
                rzmre: rzmre_idx
                    .and_then(|i| get_f64(row, i))
                    .unwrap_or_default(),
                rzche: rzche_idx
                    .and_then(|i| get_f64(row, i))
                    .unwrap_or_default(),
            });
        }
        Ok(items)
    }

    /// Fetch SHIBOR rates (上海银行间同业拆放利率) within a date range.
    pub async fn shibor(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<Shibor>, String> {
        let params = serde_json::json!({
            "start_date": start_date,
            "end_date": end_date,
        });
        let resp = self.call_api("shibor", params, "").await?;

        let fields = &resp.data.fields;
        let date_idx = fields.iter().position(|f| f == "date");
        let on_idx = fields.iter().position(|f| f == "on");
        let w1_idx = fields.iter().position(|f| f == "w1");
        let m1_idx = fields.iter().position(|f| f == "m1");
        let m3_idx = fields.iter().position(|f| f == "m3");

        let mut items = Vec::with_capacity(resp.data.items.len());
        for row in &resp.data.items {
            items.push(Shibor {
                date: date_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                on: on_idx
                    .and_then(|i| get_f64(row, i))
                    .unwrap_or_default(),
                w1: w1_idx
                    .and_then(|i| get_f64(row, i))
                    .unwrap_or_default(),
                m1: m1_idx
                    .and_then(|i| get_f64(row, i))
                    .unwrap_or_default(),
                m3: m3_idx
                    .and_then(|i| get_f64(row, i))
                    .unwrap_or_default(),
            });
        }
        Ok(items)
    }

    /// Fetch China government bond yields (中国国债收益率) within a date range.
    pub async fn cn_bond_yield(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<CnBondYield>, String> {
        let params = serde_json::json!({
            "start_date": start_date,
            "end_date": end_date,
        });
        let resp = self.call_api("cn_bond_yield", params, "").await?;

        let fields = &resp.data.fields;
        let ts_code_idx = fields.iter().position(|f| f == "ts_code");
        let yield_10y_idx = fields.iter().position(|f| f == "yield_10y");

        let mut items = Vec::with_capacity(resp.data.items.len());
        for row in &resp.data.items {
            items.push(CnBondYield {
                ts_code: ts_code_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                yield_10y: yield_10y_idx
                    .and_then(|i| get_f64(row, i))
                    .unwrap_or_default(),
            });
        }
        Ok(items)
    }

    /// 获取个股每日指标（换手率、PE、PB、市值等）。
    /// ETF 标的可能返回空数据，调用方需处理空结果。
    pub async fn daily_basic(
        &self,
        ts_code: &str,
        trade_date: Option<&str>,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> Result<Vec<DailyBasic>, String> {
        let mut params = serde_json::json!({ "ts_code": ts_code });
        if let Some(d) = trade_date {
            params["trade_date"] = serde_json::json!(d);
        }
        if let Some(s) = start_date {
            params["start_date"] = serde_json::json!(s);
        }
        if let Some(e) = end_date {
            params["end_date"] = serde_json::json!(e);
        }

        let resp = self.call_api("daily_basic", params, "").await?;
        let fields = &resp.data.fields;

        let ts_code_idx = fields.iter().position(|f| f == "ts_code");
        let trade_date_idx = fields.iter().position(|f| f == "trade_date");
        let close_idx = fields.iter().position(|f| f == "close");
        let turnover_rate_idx = fields.iter().position(|f| f == "turnover_rate");
        let turnover_rate_f_idx = fields.iter().position(|f| f == "turnover_rate_f");
        let volume_ratio_idx = fields.iter().position(|f| f == "volume_ratio");
        let pe_idx = fields.iter().position(|f| f == "pe");
        let pe_ttm_idx = fields.iter().position(|f| f == "pe_ttm");
        let pb_idx = fields.iter().position(|f| f == "pb");
        let ps_idx = fields.iter().position(|f| f == "ps");
        let ps_ttm_idx = fields.iter().position(|f| f == "ps_ttm");
        let dv_ratio_idx = fields.iter().position(|f| f == "dv_ratio");
        let dv_ttm_idx = fields.iter().position(|f| f == "dv_ttm");
        let total_share_idx = fields.iter().position(|f| f == "total_share");
        let float_share_idx = fields.iter().position(|f| f == "float_share");
        let free_share_idx = fields.iter().position(|f| f == "free_share");
        let total_mv_idx = fields.iter().position(|f| f == "total_mv");
        let circ_mv_idx = fields.iter().position(|f| f == "circ_mv");

        let mut items = Vec::with_capacity(resp.data.items.len());
        for row in &resp.data.items {
            items.push(DailyBasic {
                ts_code: ts_code_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                trade_date: trade_date_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                close: close_idx.and_then(|i| get_f64(row, i)),
                turnover_rate: turnover_rate_idx.and_then(|i| get_f64(row, i)),
                turnover_rate_f: turnover_rate_f_idx.and_then(|i| get_f64(row, i)),
                volume_ratio: volume_ratio_idx.and_then(|i| get_f64(row, i)),
                pe: pe_idx.and_then(|i| get_f64(row, i)),
                pe_ttm: pe_ttm_idx.and_then(|i| get_f64(row, i)),
                pb: pb_idx.and_then(|i| get_f64(row, i)),
                ps: ps_idx.and_then(|i| get_f64(row, i)),
                ps_ttm: ps_ttm_idx.and_then(|i| get_f64(row, i)),
                dv_ratio: dv_ratio_idx.and_then(|i| get_f64(row, i)),
                dv_ttm: dv_ttm_idx.and_then(|i| get_f64(row, i)),
                total_share: total_share_idx.and_then(|i| get_f64(row, i)),
                float_share: float_share_idx.and_then(|i| get_f64(row, i)),
                free_share: free_share_idx.and_then(|i| get_f64(row, i)),
                total_mv: total_mv_idx.and_then(|i| get_f64(row, i)),
                circ_mv: circ_mv_idx.and_then(|i| get_f64(row, i)),
            });
        }
        Ok(items)
    }

    /// 获取个股财务指标（ROE、ROA、净利润增速、营收增速、资产负债率等）。
    pub async fn fina_indicator(
        &self,
        ts_code: &str,
        period: Option<&str>,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> Result<Vec<FinaIndicator>, String> {
        let mut params = serde_json::json!({ "ts_code": ts_code });
        if let Some(p) = period {
            params["period"] = serde_json::json!(p);
        }
        if let Some(s) = start_date {
            params["start_date"] = serde_json::json!(s);
        }
        if let Some(e) = end_date {
            params["end_date"] = serde_json::json!(e);
        }

        let resp = self.call_api("fina_indicator", params, "").await?;
        let fields = &resp.data.fields;

        let ts_code_idx = fields.iter().position(|f| f == "ts_code");
        let ann_date_idx = fields.iter().position(|f| f == "ann_date");
        let end_date_idx = fields.iter().position(|f| f == "end_date");
        let roe_idx = fields.iter().position(|f| f == "roe");
        let roe_waa_idx = fields.iter().position(|f| f == "roe_waa");
        let roe_dt_idx = fields.iter().position(|f| f == "roe_dt");
        let roa_idx = fields.iter().position(|f| f == "roa");
        let netprofit_yoy_idx = fields.iter().position(|f| f == "netprofit_yoy");
        let or_yoy_idx = fields.iter().position(|f| f == "or_yoy");
        let debt_to_assets_idx = fields.iter().position(|f| f == "debt_to_assets");

        let mut items = Vec::with_capacity(resp.data.items.len());
        for row in &resp.data.items {
            items.push(FinaIndicator {
                ts_code: ts_code_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                ann_date: ann_date_idx.and_then(|i| get_str(row, i)),
                end_date: end_date_idx.and_then(|i| get_str(row, i)),
                roe: roe_idx.and_then(|i| get_f64(row, i)),
                roe_waa: roe_waa_idx.and_then(|i| get_f64(row, i)),
                roe_dt: roe_dt_idx.and_then(|i| get_f64(row, i)),
                roa: roa_idx.and_then(|i| get_f64(row, i)),
                netprofit_yoy: netprofit_yoy_idx.and_then(|i| get_f64(row, i)),
                or_yoy: or_yoy_idx.and_then(|i| get_f64(row, i)),
                debt_to_assets: debt_to_assets_idx.and_then(|i| get_f64(row, i)),
            });
        }
        Ok(items)
    }

    /// 获取个股机构评级汇总（买入/增持/减持/卖出家数及评级均值）。
    pub async fn report_rc(
        &self,
        ts_code: &str,
        report_date: Option<&str>,
    ) -> Result<Vec<ReportRc>, String> {
        let mut params = serde_json::json!({ "ts_code": ts_code });
        if let Some(d) = report_date {
            params["report_date"] = serde_json::json!(d);
        }

        let resp = self.call_api("report_rc", params, "").await?;
        let fields = &resp.data.fields;

        let ts_code_idx = fields.iter().position(|f| f == "ts_code");
        let report_date_idx = fields.iter().position(|f| f == "report_date");
        let org_num_idx = fields.iter().position(|f| f == "org_num");
        let buy_num_idx = fields.iter().position(|f| f == "buy_num");
        let hold_num_idx = fields.iter().position(|f| f == "hold_num");
        let reduce_num_idx = fields.iter().position(|f| f == "reduce_num");
        let sell_num_idx = fields.iter().position(|f| f == "sell_num");
        let strong_buy_num_idx = fields.iter().position(|f| f == "strong_buy_num");
        let rating_avg_idx = fields.iter().position(|f| f == "rating_avg");

        let mut items = Vec::with_capacity(resp.data.items.len());
        for row in &resp.data.items {
            items.push(ReportRc {
                ts_code: ts_code_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                report_date: report_date_idx.and_then(|i| get_str(row, i)),
                org_num: org_num_idx.and_then(|i| get_f64(row, i)),
                buy_num: buy_num_idx.and_then(|i| get_f64(row, i)),
                hold_num: hold_num_idx.and_then(|i| get_f64(row, i)),
                reduce_num: reduce_num_idx.and_then(|i| get_f64(row, i)),
                sell_num: sell_num_idx.and_then(|i| get_f64(row, i)),
                strong_buy_num: strong_buy_num_idx.and_then(|i| get_f64(row, i)),
                rating_avg: rating_avg_idx.and_then(|i| get_f64(row, i)),
            });
        }
        Ok(items)
    }

    /// 获取个股每日资金流向明细（小单/中单/大单/超大单买卖量及主力净流入）。
    pub async fn moneyflow_dc(
        &self,
        ts_code: &str,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<MoneyflowDc>, String> {
        let params = serde_json::json!({
            "ts_code": ts_code,
            "start_date": start_date,
            "end_date": end_date,
        });

        let resp = self.call_api("moneyflow_dc", params, "").await?;
        let fields = &resp.data.fields;

        let ts_code_idx = fields.iter().position(|f| f == "ts_code");
        let trade_date_idx = fields.iter().position(|f| f == "trade_date");
        let buy_sm_vol_idx = fields.iter().position(|f| f == "buy_sm_vol");
        let sell_sm_vol_idx = fields.iter().position(|f| f == "sell_sm_vol");
        let buy_md_vol_idx = fields.iter().position(|f| f == "buy_md_vol");
        let sell_md_vol_idx = fields.iter().position(|f| f == "sell_md_vol");
        let buy_lg_vol_idx = fields.iter().position(|f| f == "buy_lg_vol");
        let sell_lg_vol_idx = fields.iter().position(|f| f == "sell_lg_vol");
        let buy_elg_vol_idx = fields.iter().position(|f| f == "buy_elg_vol");
        let sell_elg_vol_idx = fields.iter().position(|f| f == "sell_elg_vol");
        let net_mf_vol_idx = fields.iter().position(|f| f == "net_mf_vol");

        let mut items = Vec::with_capacity(resp.data.items.len());
        for row in &resp.data.items {
            items.push(MoneyflowDc {
                ts_code: ts_code_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                trade_date: trade_date_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                buy_sm_vol: buy_sm_vol_idx.and_then(|i| get_f64(row, i)),
                sell_sm_vol: sell_sm_vol_idx.and_then(|i| get_f64(row, i)),
                buy_md_vol: buy_md_vol_idx.and_then(|i| get_f64(row, i)),
                sell_md_vol: sell_md_vol_idx.and_then(|i| get_f64(row, i)),
                buy_lg_vol: buy_lg_vol_idx.and_then(|i| get_f64(row, i)),
                sell_lg_vol: sell_lg_vol_idx.and_then(|i| get_f64(row, i)),
                buy_elg_vol: buy_elg_vol_idx.and_then(|i| get_f64(row, i)),
                sell_elg_vol: sell_elg_vol_idx.and_then(|i| get_f64(row, i)),
                net_mf_vol: net_mf_vol_idx.and_then(|i| get_f64(row, i)),
            });
        }
        Ok(items)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn get_str_valid() {
        let row = vec![json!("600519.SH"), json!("贵州茅台"), json!(1234.5)];
        assert_eq!(get_str(&row, 0), Some("600519.SH".into()));
        assert_eq!(get_str(&row, 1), Some("贵州茅台".into()));
    }

    #[test]
    fn get_str_null() {
        let row = vec![json!(null), json!("abc")];
        assert_eq!(get_str(&row, 0), None);
        assert_eq!(get_str(&row, 1), Some("abc".into()));
    }

    #[test]
    fn get_str_out_of_bounds() {
        let row = vec![json!("a")];
        assert_eq!(get_str(&row, 5), None);
    }

    #[test]
    fn get_str_numeric_to_string() {
        let row = vec![json!(42)];
        assert_eq!(get_str(&row, 0), Some("42".into()));
    }

    #[test]
    fn get_f64_from_number() {
        let row = vec![json!(3.14), json!(100), json!(null)];
        assert!((get_f64(&row, 0).unwrap() - 3.14).abs() < f64::EPSILON);
        assert!((get_f64(&row, 1).unwrap() - 100.0).abs() < f64::EPSILON);
        assert_eq!(get_f64(&row, 2), None);
    }

    #[test]
    fn get_f64_from_string() {
        let row = vec![json!("27.50"), json!("not_a_number")];
        assert!((get_f64(&row, 0).unwrap() - 27.50).abs() < f64::EPSILON);
        assert_eq!(get_f64(&row, 1), None);
    }

    #[test]
    fn get_f64_out_of_bounds() {
        let row = vec![json!(1.0)];
        assert_eq!(get_f64(&row, 10), None);
    }

    #[test]
    fn get_i64_basic() {
        let row = vec![json!(42), json!(null), json!("7")];
        assert_eq!(get_i64(&row, 0), Some(42));
        assert_eq!(get_i64(&row, 1), None);
        assert_eq!(get_i64(&row, 2), Some(7));
    }

    #[test]
    fn get_i64_out_of_bounds() {
        let row = vec![json!(1)];
        assert_eq!(get_i64(&row, 3), None);
    }
}
