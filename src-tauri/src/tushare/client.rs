use serde::{Deserialize, Serialize};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// Response envelope from Tushare Pro API.
#[derive(Debug, Deserialize)]
pub struct TushareResponse {
    pub code: i64,
    pub msg: String,
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

/// Trade calendar entry (交易日历).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeCal {
    pub exchange: String,
    pub cal_date: String,
    pub is_open: i64,
    pub pretrade_date: String,
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
                return Err(format!("tushare error {}: {}", parsed.code, parsed.msg));
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
