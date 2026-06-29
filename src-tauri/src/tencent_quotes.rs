use crate::tushare::client::RealtimeQuote;
use serde::{Deserialize, Serialize};

/// 将 Tushare ts_code (如 "159248.SZ") 转换为腾讯格式 (如 "sz159248")
fn to_tencent_symbol(ts_code: &str) -> Option<String> {
    let parts: Vec<&str> = ts_code.split('.').collect();
    if parts.len() != 2 {
        return None;
    }
    let prefix = match parts[1] {
        "SZ" => "sz",
        "SH" => "sh",
        _ => return None,
    };
    Some(format!("{}{}", prefix, parts[0]))
}

/// 将腾讯格式 (如 "sz159248") 转换回 Tushare ts_code (如 "159248.SZ")
fn from_tencent_symbol(tencent: &str) -> Option<String> {
    if tencent.len() < 3 {
        return None;
    }
    let (prefix, code) = tencent.split_at(2);
    let market = match prefix {
        "sz" => "SZ",
        "sh" => "SH",
        _ => return None,
    };
    Some(format!("{}.{}", code, market))
}

/// 解析腾讯行情 API 的单行响应
/// 格式: v_sz159248="1~纳斯达克~159248~1.942~1.891~1.896~123456~...~2.70~..."
/// 字段按 ~ 分割: [1]=name, [2]=code, [3]=price(close), [4]=pre_close, [5]=open,
///               [31]=high, [32]=low, [6]=vol(手), [37]=amount(万)
fn parse_quote_line(line: &str) -> Option<RealtimeQuote> {
    let line = line.trim();
    if !line.starts_with("v_") {
        return None;
    }

    let eq_pos = line.find('=')?;
    let symbol_part = &line[2..eq_pos]; // "sz159248"
    let ts_code = from_tencent_symbol(symbol_part)?;

    let value_part = &line[eq_pos + 2..]; // 跳过 ="
    let end_quote = value_part.find('"')?;
    let data_str = &value_part[..end_quote];

    let parts: Vec<&str> = data_str.split('~').collect();
    // 需要至少 38 个字段才能安全访问 parts[37]
    if parts.len() < 38 {
        return None;
    }

    let name = parts[1].to_string();
    let close: f64 = parts[3].parse().unwrap_or(0.0);
    let pre_close: f64 = parts[4].parse().unwrap_or(0.0);
    let open: f64 = parts[5].parse().unwrap_or(0.0);
    let vol: f64 = parts[6].parse().unwrap_or(0.0);
    let high: f64 = parts[31].parse().unwrap_or(0.0);
    let low: f64 = parts[32].parse().unwrap_or(0.0);
    let amount_raw: f64 = parts[37].parse().unwrap_or(0.0);

    // vol 单位是手 (100股)，amount 单位是万元，转换为与 Tushare 一致的单位
    let vol = vol * 100.0;
    let amount = amount_raw * 10000.0;

    // trade_time: parts[30] 格式 "20260603150000"
    let trade_time_raw = parts.get(30).unwrap_or(&"");
    let trade_time = if trade_time_raw.len() >= 14 {
        format!(
            "{}-{}-{} {}:{}:{}",
            &trade_time_raw[0..4],
            &trade_time_raw[4..6],
            &trade_time_raw[6..8],
            &trade_time_raw[8..10],
            &trade_time_raw[10..12],
            &trade_time_raw[12..14]
        )
    } else {
        String::new()
    };

    Some(RealtimeQuote {
        ts_code,
        name,
        open,
        high,
        low,
        close,
        pre_close,
        vol,
        amount,
        trade_time,
    })
}

/// 从腾讯行情 API 批量获取实时行情
/// API: http://qt.gtimg.cn/q={symbols}
/// symbols 用逗号分隔，如 "sz159248,sh601888"
///
/// 返回成功解析的行情列表。调用方可通过比较 `result.len()` 与 `ts_codes.len()`
/// 判断是否有部分符号未返回（例如腾讯不支持的交易所）。
pub async fn fetch_quotes(
    client: &reqwest::Client,
    ts_codes: &[&str],
) -> Result<Vec<RealtimeQuote>, String> {
    if ts_codes.is_empty() {
        return Ok(vec![]);
    }

    // 转换为腾讯格式，记录可转换的原始 ts_code
    let tencent_pairs: Vec<(&str, String)> = ts_codes
        .iter()
        .filter_map(|code| to_tencent_symbol(code).map(|sym| (*code, sym)))
        .collect();

    if tencent_pairs.is_empty() {
        return Err("no valid symbols to query".to_string());
    }

    let url = format!(
        "http://qt.gtimg.cn/q={}",
        tencent_pairs.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join(",")
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("tencent request failed: {e}"))?;

    let body = resp
        .text()
        .await
        .map_err(|e| format!("tencent response read failed: {e}"))?;

    let quotes: Vec<RealtimeQuote> = body
        .lines()
        .filter_map(|line| parse_quote_line(line))
        .filter(|q| q.close > 0.0)
        .collect();

    Ok(quotes)
}

// ---------------------------------------------------------------------------
// Index quote (lightweight — close + amount only)
// ---------------------------------------------------------------------------

/// Lightweight index quote for macro indicators.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexQuote {
    pub close: f64,
    /// Turnover in yuan (两市成交额 use case: pass two index symbols and sum).
    pub amount: f64,
}

/// Fetch a single index quote from Tencent Finance.
///
/// `symbol` is a raw Tencent-format symbol (e.g. `"sh000300"`, `"sh000001"`, `"sz399001"`).
/// This is intentionally NOT the ts_code format used by `fetch_quotes` — index
/// symbols are already in Tencent format.
pub async fn fetch_index_quote(
    client: &reqwest::Client,
    symbol: &str,
) -> Result<IndexQuote, String> {
    let url = format!("http://qt.gtimg.cn/q={}", symbol);

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("tencent index request failed: {e}"))?;

    let body = resp
        .text()
        .await
        .map_err(|e| format!("tencent index response read failed: {e}"))?;

    for line in body.lines() {
        if let Some(quote) = parse_quote_line(line) {
            if quote.close > 0.0 {
                return Ok(IndexQuote {
                    close: quote.close,
                    amount: quote.amount,
                });
            }
        }
    }

    Err(format!("tencent index {symbol}: no valid data"))
}

// ---------------------------------------------------------------------------
// Index K-line + 20-day volatility
// ---------------------------------------------------------------------------

/// Index K-line result with latest close and 20-day annualized volatility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexKlineResult {
    pub close: f64,
    /// 20-day annualized volatility (percent), e.g. 19.96.
    pub vol20: Option<f64>,
}

/// Fetch any index daily K-line from Tencent Finance and compute 20-day volatility.
///
/// Uses the `web.ifzq.gtimg.cn` K-line API (same endpoint used by the web chart).
/// `symbol` is the Tencent format, e.g. `"sh000001"` for Shanghai Composite.
/// `days` controls the lookback window (25 is enough for vol20 computation).
pub async fn fetch_index_kline(
    client: &reqwest::Client,
    symbol: &str,
    days: u32,
) -> Result<IndexKlineResult, String> {
    let url = format!(
        "https://web.ifzq.gtimg.cn/appstock/app/fqkline/get?param={symbol},day,,,{days},qfq"
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("tencent kline request failed: {e}"))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("tencent kline parse failed: {e}"))?;

    // Response: {"data":{"sh000001":{"day":[[date,open,close,high,low,vol],...], "qfqday":[...]}}}
    let day_data = body
        .get("data")
        .and_then(|d| d.get(symbol))
        .and_then(|s| {
            s.get("day")
                .or_else(|| s.get("qfqday"))
                .and_then(|v| v.as_array())
        })
        .ok_or(format!("tencent kline: missing data.{symbol}.day"))?;

    if day_data.is_empty() {
        return Err("tencent kline: empty day data".into());
    }

    // Parse closing prices (index 2 in each [date, open, close, high, low, vol] array)
    let closes: Vec<f64> = day_data
        .iter()
        .filter_map(|bar| {
            bar.as_array()
                .and_then(|arr| arr.get(2))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
        })
        .filter(|&c| c > 0.0)
        .collect();

    if closes.is_empty() {
        return Err("tencent kline: no valid closing prices".into());
    }

    let latest_close = closes[0]; // newest first

    Ok(IndexKlineResult {
        close: latest_close,
        vol20: compute_vol20(&closes),
    })
}

// ---------------------------------------------------------------------------
// Shared volatility computation
// ---------------------------------------------------------------------------

/// Compute 20-day annualized volatility from closing prices (newest first).
///
/// Uses log returns on the first 21 closes, population variance,
/// annualized by sqrt(252) * 100 (percent).
/// Returns `None` if fewer than 21 closes are available.
pub fn compute_vol20(closes: &[f64]) -> Option<f64> {
    if closes.len() < 21 {
        return None;
    }
    let returns: Vec<f64> = closes[..21]
        .windows(2)
        .filter_map(|w| {
            if w[0] > 0.0 && w[1] > 0.0 {
                Some((w[0] / w[1]).ln())
            } else {
                None
            }
        })
        .collect();
    if returns.is_empty() {
        return None;
    }
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let var = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
    Some(var.sqrt() * 252.0_f64.sqrt() * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_tencent_symbol() {
        assert_eq!(to_tencent_symbol("159248.SZ"), Some("sz159248".to_string()));
        assert_eq!(to_tencent_symbol("601888.SH"), Some("sh601888".to_string()));
        assert_eq!(to_tencent_symbol("159248"), None);
        assert_eq!(to_tencent_symbol("159248.BJ"), None);
    }

    #[test]
    fn test_from_tencent_symbol() {
        assert_eq!(from_tencent_symbol("sz159248"), Some("159248.SZ".to_string()));
        assert_eq!(from_tencent_symbol("sh601888"), Some("601888.SH".to_string()));
        assert_eq!(from_tencent_symbol("sz"), None);
    }

    #[test]
    fn test_parse_quote_line() {
        let line = r#"v_sz159248="1~纳斯达克~159248~1.942~1.891~1.896~12345678~6123456~6222222~1.941~100~1.940~200~1.939~300~1.938~400~1.937~500~1.943~100~1.944~200~1.945~300~1.946~400~1.947~500~15:00:03~20260603150003~0.051~2.70~1.947~1.891~1.942/12345678/240000000~12345678~240000~1.02~22.50~~1.947~1.891~2.94~4567.89~5678.90~1.80~2.10~0.78~5000.00~6000.00~1.20";"#;
        let quote = parse_quote_line(line).unwrap();
        assert_eq!(quote.ts_code, "159248.SZ");
        assert_eq!(quote.name, "纳斯达克");
        assert!((quote.close - 1.942).abs() < 0.001);
        assert!((quote.pre_close - 1.891).abs() < 0.001);
        assert!((quote.open - 1.896).abs() < 0.001);
    }

    #[test]
    fn test_parse_index_quote_line() {
        // Simulate an index quote line — reuse parse_quote_line then extract IndexQuote
        let line = r#"v_sh000300="1~沪深300~000300~4931.39~4884.23~4859.70~276053950~0~0~0.000~0~0.000~0~0.000~0~0.000~0~0.000~0~0.000~0~0.000~0~0.000~0~0.000~0~0.000~0~15:00:00~20260617150000~47.16~0.97~4933.93~4859.70~4931.39/276053950/850367580000~276053950~850367~0.00~0.00~~4933.93~4859.70~1.53~850367.00~850367.00~0.00~0.00~0.00~0.00~0.00~0.00";"#;
        let quote = parse_quote_line(line).unwrap();
        assert_eq!(quote.ts_code, "000300.SH");
        assert_eq!(quote.name, "沪深300");
        assert!((quote.close - 4931.39).abs() < 0.01);
        // amount = parts[37] * 10000; parts[37] = "850367.00" => 8503670000
        assert!(quote.amount > 0.0);

        let index_quote = super::IndexQuote {
            close: quote.close,
            amount: quote.amount,
        };
        assert!((index_quote.close - 4931.39).abs() < 0.01);
    }

    #[test]
    fn test_csi300_kline_vol20_calculation() {
        // With constant 1% daily returns, vol20 should be near 0 (no variance)
        let base = 4500.0_f64;
        let constant_closes: Vec<f64> = (0..21).map(|i| base * 1.01_f64.powi(i)).collect();
        let vol_constant = compute_vol20(&constant_closes).unwrap();
        assert!(
            vol_constant < 0.01,
            "constant returns should yield ~0 vol, got {vol_constant}"
        );

        // With varying returns, vol20 should be a meaningful positive number
        let varying: Vec<f64> = vec![
            4500.0, 4520.0, 4480.0, 4510.0, 4490.0, 4530.0, 4470.0, 4500.0, 4520.0, 4480.0,
            4510.0, 4490.0, 4530.0, 4470.0, 4500.0, 4520.0, 4480.0, 4510.0, 4490.0, 4530.0,
            4470.0,
        ];
        let vol_varying = compute_vol20(&varying).unwrap();
        assert!(
            vol_varying > 5.0 && vol_varying < 100.0,
            "varying returns should yield meaningful vol, got {vol_varying}"
        );

        // Too few closes should return None
        let short: Vec<f64> = vec![4500.0; 10];
        assert!(compute_vol20(&short).is_none());

        // Realistic data with both up and down days must not produce NaN
        let mixed: Vec<f64> = vec![
            4152.0, 4145.0, 4093.0, 4098.0, 4068.0, 4090.0, 4110.0, 4085.0, 4120.0, 4055.0,
            4070.0, 4095.0, 4030.0, 4060.0, 4080.0, 4050.0, 4075.0, 4100.0, 4085.0, 4120.0,
            4027.0,
        ];
        let vol_mixed = compute_vol20(&mixed).unwrap();
        assert!(
            vol_mixed.is_finite() && vol_mixed > 0.0,
            "mixed returns should yield finite positive vol, got {vol_mixed}"
        );
    }
}
