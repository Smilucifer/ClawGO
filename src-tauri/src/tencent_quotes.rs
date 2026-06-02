use crate::tushare::client::RealtimeQuote;

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
}
