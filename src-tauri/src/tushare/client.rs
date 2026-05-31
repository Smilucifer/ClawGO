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

/// HTTP client for the Tushare Pro API.
#[derive(Clone)]
pub struct TushareClient {
    token: String,
    client: reqwest::Client,
}

impl TushareClient {
    /// Create a new client with a 30-second request timeout.
    pub fn new(token: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build reqwest client");
        Self { token, client }
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
                .post("http://101.35.233.113:8020/")
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("request failed: {e}"))?;

            let status = resp.status();

            // Retry on 429 or 5xx
            if status.as_u16() == 429 || status.is_server_error() {
                let backoff_ms = 1000 * (1u64 << attempt);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                last_err = Some(format!("HTTP {status} (attempt {}/{max_retries})", attempt + 1));
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

    /// Fetch daily bars (日线行情) for a stock within a date range.
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

        let resp = self.call_api("daily", params, "").await?;
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

    /// Search stocks by optional name (模糊匹配). If `name` is `None`, returns
    /// the first 50 stocks.
    pub async fn stock_basic(&self, name: Option<&str>) -> Result<Vec<StockBasic>, String> {
        let params = if let Some(n) = name {
            serde_json::json!({ "name": n })
        } else {
            serde_json::json!({})
        };

        let resp = self.call_api("stock_basic", params, "").await?;
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

    /// Search ETFs by optional name (模糊匹配). Filters to listed ETFs only.
    pub async fn fund_basic(&self, name: Option<&str>) -> Result<Vec<FundBasic>, String> {
        let mut params = serde_json::json!({
            "fund_type": "E",  // ETF only
            "status": "L",     // Listed only
        });
        if let Some(n) = name {
            params["name"] = serde_json::json!(n);
        }

        let resp = self.call_api("fund_basic", params, "ts_code,name").await?;
        let fields = &resp.data.fields;

        let ts_code_idx = fields.iter().position(|f| f == "ts_code");
        let name_idx = fields.iter().position(|f| f == "name");

        let mut funds = Vec::with_capacity(resp.data.items.len());

        for row in &resp.data.items {
            funds.push(FundBasic {
                ts_code: ts_code_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
                name: name_idx
                    .and_then(|i| get_str(row, i))
                    .unwrap_or_default(),
            });
        }

        Ok(funds)
    }

    /// Get the latest close price for a given stock.
    pub async fn get_latest_price(&self, ts_code: &str) -> Result<f64, String> {
        let resp = self
            .call_api(
                "daily",
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

    /// Fetch trade calendar for an exchange within a date range.
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
