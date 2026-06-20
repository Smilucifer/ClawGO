use crate::models::{BalanceCacheEntry, BalanceHelperSettings};
use crate::storage;
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, USER_AGENT};
use serde_json::Value;
use std::time::Duration;

const DEEPSEEK_BALANCE_BASE_URL: &str = "https://api.deepseek.com";
const MIMO_API_BASE_URL: &str = "https://platform.xiaomimimo.com";
#[allow(dead_code)] // wired into refresh_balance_status_inner in Batch D Task 3
const PACKYAPI_BASE_URL: &str = "https://www.packyapi.com";
/// New-API quota unit: 500000 quota = $1.
#[allow(dead_code)] // wired into refresh_balance_status_inner in Batch D Task 3
const PACKYAPI_QUOTA_PER_USD: f64 = 500_000.0;

fn balance_cache_entry(source: &str, result: Result<String, String>) -> BalanceCacheEntry {
    balance_cache_entry_with_tokens(source, result.map(|bt| (bt, None, None, None)))
}

fn balance_cache_entry_with_tokens(
    source: &str,
    result: Result<(String, Option<i64>, Option<i64>, Option<f64>), String>,
) -> BalanceCacheEntry {
    match result {
        Ok((balance_text, used, limit, percent)) => BalanceCacheEntry {
            source: source.to_string(),
            status: "ok".to_string(),
            balance_text: Some(balance_text),
            error: None,
            refreshed_at: crate::models::now_iso(),
            token_plan_used: used,
            token_plan_limit: limit,
            token_plan_percent: percent,
        },
        Err(error) => BalanceCacheEntry {
            source: source.to_string(),
            status: "failed".to_string(),
            balance_text: None,
            error: Some(redacted_operational_error(&error)),
            refreshed_at: crate::models::now_iso(),
            token_plan_used: None,
            token_plan_limit: None,
            token_plan_percent: None,
        },
    }
}

fn redacted_operational_error(input: &str) -> String {
    input
        .split_whitespace()
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            if lower.starts_with("sk-")
                || lower.contains("api_key")
                || lower.contains("apikey")
                || lower.contains("authorization")
                || lower.contains("cookie")
                || lower.contains("session")
                || lower.contains("token=")
            {
                "[redacted]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_deepseek_balance(body: &Value) -> Result<String, String> {
    if body
        .get("is_available")
        .and_then(Value::as_bool)
        .is_some_and(|available| !available)
    {
        return Err("DeepSeek balance is unavailable".to_string());
    }

    let infos = body
        .get("balance_infos")
        .and_then(Value::as_array)
        .ok_or_else(|| "DeepSeek response did not include balance info".to_string())?;

    let formatted = infos
        .iter()
        .filter_map(|info| {
            let currency = info.get("currency")?.as_str()?.trim();
            let balance = info
                .get("total_balance")
                .or_else(|| info.get("granted_balance"))
                .or_else(|| info.get("topped_up_balance"))?
                .as_str()?
                .trim();
            if currency.is_empty() || balance.is_empty() {
                None
            } else {
                Some(format!("{currency} {balance}"))
            }
        })
        .collect::<Vec<_>>();

    if formatted.is_empty() {
        Err("DeepSeek response did not include a readable balance".to_string())
    } else {
        Ok(formatted.join(", "))
    }
}

async fn query_deepseek_balance(
    client: &reqwest::Client,
    api_key: &str,
    base_url: &str,
) -> Result<String, String> {
    let trimmed_key = api_key.trim();
    if trimmed_key.is_empty() {
        return Err("DeepSeek API key is not configured".to_string());
    }

    let url = format!("{}/user/balance", base_url.trim_end_matches('/'));
    let response = client
        .get(url)
        .bearer_auth(trimmed_key)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "DeepSeek balance request timed out".to_string()
            } else {
                format!("DeepSeek balance request failed: {e}")
            }
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "DeepSeek authentication failed".to_string(),
            429 => "DeepSeek balance request was rate limited".to_string(),
            code => format!("DeepSeek balance request failed with HTTP {code}"),
        });
    }

    let body = response
        .json::<Value>()
        .await
        .map_err(|_| "DeepSeek balance response was not valid JSON".to_string())?;
    format_deepseek_balance(&body)
}

#[allow(dead_code)] // wired into refresh_balance_status_inner in Batch D Task 3
fn format_packyapi_balance(body: &Value) -> Result<String, String> {
    let data = body
        .get("data")
        .filter(|d| !d.is_null())
        .ok_or_else(|| "PackyAPI response did not include account data".to_string())?;
    let quota = data
        .get("quota")
        .and_then(Value::as_f64)
        .ok_or_else(|| "PackyAPI response missing quota".to_string())?;
    let used = data.get("used_quota").and_then(Value::as_f64).unwrap_or(0.0);
    let remain_usd = quota / PACKYAPI_QUOTA_PER_USD;
    let used_usd = used / PACKYAPI_QUOTA_PER_USD;
    Ok(format!("${:.2} 剩 / ${:.2} 用", remain_usd, used_usd))
}

#[allow(dead_code)] // wired into refresh_balance_status_inner in Batch D Task 3
async fn query_packyapi_balance(
    client: &reqwest::Client,
    session: &str,
    itoken: &str,
    user_id: &str,
) -> Result<String, String> {
    let session = session.trim();
    let user_id = user_id.trim();
    if session.is_empty() {
        return Err("PackyAPI session cookie is not configured".to_string());
    }
    if user_id.is_empty() {
        return Err("PackyAPI user id is not configured".to_string());
    }

    let cookie_value = if itoken.trim().is_empty() {
        format!("session={session}")
    } else {
        format!("session={session}; TDC_itoken={}", itoken.trim())
    };

    let url = format!("{}/api/user/self", PACKYAPI_BASE_URL);
    let response = client
        .get(url)
        .header(
            COOKIE,
            HeaderValue::from_str(&cookie_value).map_err(|_| {
                "PackyAPI credentials contain invalid header characters".to_string()
            })?,
        )
        .header(
            "New-Api-User",
            HeaderValue::from_str(user_id)
                .map_err(|_| "PackyAPI user id is invalid".to_string())?,
        )
        .header("Accept", HeaderValue::from_static("application/json"))
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "PackyAPI balance request timed out".to_string()
            } else {
                format!("PackyAPI balance request failed: {e}")
            }
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "PackyAPI authentication failed".to_string(),
            429 => "PackyAPI balance request was rate limited".to_string(),
            code => format!("PackyAPI balance request failed with HTTP {code}"),
        });
    }

    let body = response
        .json::<Value>()
        .await
        .map_err(|_| "PackyAPI balance response was not valid JSON".to_string())?;

    // New-API returns success:false + message on auth/business errors with HTTP 200
    if body.get("success").and_then(Value::as_bool) == Some(false) {
        let msg = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(format!("PackyAPI: {msg}"));
    }
    format_packyapi_balance(&body)
}

fn build_mimo_headers(
    service_token: &str,
    user_id: &str,
    slh: &str,
    ph: &str,
) -> Result<HeaderMap, String> {
    let trimmed_token = service_token.trim();
    let trimmed_user_id = user_id.trim();

    if trimmed_token.is_empty() {
        return Err("MiMo service token is not configured".to_string());
    }
    if trimmed_user_id.is_empty() {
        return Err("MiMo user id is not configured".to_string());
    }

    let cookie_value = format!(
        "api-platform_serviceToken={}; userId={}; api-platform_slh={}; api-platform_ph={}",
        trimmed_token,
        trimmed_user_id,
        slh.trim(),
        ph.trim()
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        COOKIE,
        HeaderValue::from_str(&cookie_value)
            .map_err(|_| "MiMo credentials contain invalid header characters".to_string())?,
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36",
        ),
    );
    headers.insert(
        "Accept",
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    headers.insert(
        "Referer",
        HeaderValue::from_static("https://platform.xiaomimimo.com/"),
    );
    headers.insert(
        "Origin",
        HeaderValue::from_static("https://platform.xiaomimimo.com"),
    );
    headers.insert(
        "x-kl-ajax-request",
        HeaderValue::from_static("Ajax_Request"),
    );
    headers.insert("x-timezone", HeaderValue::from_static("Asia/Shanghai"));
    Ok(headers)
}

#[derive(Debug)]
struct MimoBalanceDetail {
    balance_text: String,
    token_plan_used: Option<i64>,
    token_plan_limit: Option<i64>,
    token_plan_percent: Option<f64>,
}

fn format_mimo_balance(
    balance_body: &Value,
    usage_body: &Value,
) -> Result<MimoBalanceDetail, String> {
    let balance_code = balance_body
        .get("code")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    if balance_code != 0 {
        return Err(get_mimo_error(&balance_body, "balance"));
    }

    let data = balance_body
        .get("data")
        .ok_or_else(|| "MiMo balance response did not include data".to_string())?;
    let balance = data
        .get("balance")
        .and_then(Value::as_str)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("0");
    let currency = data
        .get("currency")
        .and_then(Value::as_str)
        .map(|s| s.trim())
        .unwrap_or("CNY");

    let mut balance_text = format!("{} {}", currency, balance);
    let mut token_plan_used: Option<i64> = None;
    let mut token_plan_limit: Option<i64> = None;
    let mut token_plan_percent: Option<f64> = None;

    let usage_code = usage_body.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if usage_code == 0 {
        if let Some(usage_items) = usage_body
            .get("data")
            .and_then(|d| d.get("usage"))
            .and_then(|u| u.get("items"))
            .and_then(Value::as_array)
        {
            if let Some(plan_item) = usage_items
                .iter()
                .find(|item| item.get("name").and_then(Value::as_str) == Some("plan_total_token"))
            {
                let used = plan_item.get("used").and_then(Value::as_i64).unwrap_or(0);
                let limit = plan_item.get("limit").and_then(Value::as_i64).unwrap_or(0);
                if limit > 0 {
                    let used_str = format_token_compact(used as u64);
                    let limit_str = format_token_compact(limit as u64);
                    balance_text = format!("{} | 套餐 {}/{}", balance_text, used_str, limit_str);
                    token_plan_used = Some(used);
                    token_plan_limit = Some(limit);
                    token_plan_percent = Some(used as f64 * 100.0 / limit as f64);
                }
            }
        }
    }

    if balance_text == "CNY 0" {
        return Err("MiMo balance response was empty".to_string());
    }

    Ok(MimoBalanceDetail {
        balance_text,
        token_plan_used,
        token_plan_limit,
        token_plan_percent,
    })
}

fn get_mimo_error(body: &Value, api_name: &str) -> String {
    body.get("msg")
        .or_else(|| body.get("message"))
        .and_then(Value::as_str)
        .map(|s| format!("MiMo {} request failed: {}", api_name, s))
        .unwrap_or_else(|| format!("MiMo {} request returned non-zero code", api_name))
}

fn format_token_compact(n: u64) -> String {
    if n >= 100_000_000 {
        format!("{:.1}亿", n as f64 / 100_000_000.0)
    } else if n >= 10_000 {
        format!("{}万", (n as f64 / 10_000.0).round() as u64)
    } else {
        n.to_string()
    }
}

async fn query_mimo_balance(
    client: &reqwest::Client,
    service_token: &str,
    user_id: &str,
    slh: &str,
    ph: &str,
) -> Result<MimoBalanceDetail, String> {
    let headers = build_mimo_headers(service_token, user_id, slh, ph)?;

    let balance_url = format!("{}/api/v1/balance", MIMO_API_BASE_URL);
    let balance_resp = client
        .get(&balance_url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "MiMo balance request timed out".to_string()
            } else {
                format!("MiMo balance request failed: {e}")
            }
        })?;

    let balance_status = balance_resp.status();
    if !balance_status.is_success() {
        return Err(match balance_status.as_u16() {
            401 | 403 => "MiMo authentication failed".to_string(),
            code => format!("MiMo balance request failed with HTTP {code}"),
        });
    }

    let balance_body = balance_resp
        .json::<Value>()
        .await
        .map_err(|_| "MiMo balance response was not valid JSON".to_string())?;

    let usage_url = format!("{}/api/v1/tokenPlan/usage", MIMO_API_BASE_URL);
    let usage_body = match client.get(&usage_url).headers(headers).send().await {
        Ok(resp) => resp
            .json::<Value>()
            .await
            .unwrap_or_else(|_| serde_json::json!({"code": -1})),
        Err(_) => serde_json::json!({"code": -1}),
    };

    format_mimo_balance(&balance_body, &usage_body)
}

async fn refresh_balance_status_inner(
    source: Option<String>,
) -> Result<BalanceHelperSettings, String> {
    let requested = source.unwrap_or_else(|| "all".to_string());
    if !matches!(requested.as_str(), "all" | "deepseek" | "mimo") {
        return Err(format!("Unknown balance source: {requested}"));
    }

    let settings = storage::settings::get_user_settings();
    let mut helper = settings.balance_helper.clone();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Balance HTTP client build failed: {e}"))?;

    if requested == "all" || requested == "deepseek" {
        let deepseek_key = settings
            .platform_credentials
            .iter()
            .find(|credential| credential.platform_id == "deepseek")
            .and_then(|credential| credential.api_key.as_deref())
            .unwrap_or("");
        let result = query_deepseek_balance(&client, deepseek_key, DEEPSEEK_BALANCE_BASE_URL).await;
        helper.cache.insert(
            "deepseek".to_string(),
            balance_cache_entry("deepseek", result),
        );
    }

    if requested == "all" || requested == "mimo" {
        let result = query_mimo_balance(
            &client,
            helper.mimo_service_token.as_deref().unwrap_or(""),
            helper.mimo_user_id.as_deref().unwrap_or(""),
            helper.mimo_slh.as_deref().unwrap_or(""),
            helper.mimo_ph.as_deref().unwrap_or(""),
        )
        .await;
        let mapped = result.map(|d| {
            (
                d.balance_text,
                d.token_plan_used,
                d.token_plan_limit,
                d.token_plan_percent,
            )
        });
        helper.cache.insert(
            "mimo".to_string(),
            balance_cache_entry_with_tokens("mimo", mapped),
        );
    }

    let updated = storage::settings::update_user_settings(serde_json::json!({
        "balance_helper": helper
    }))?;
    Ok(updated.balance_helper)
}

#[tauri::command]
pub async fn refresh_balance_status(
    source: Option<String>,
) -> Result<BalanceHelperSettings, String> {
    refresh_balance_status_inner(source).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_deepseek_balance_infos() {
        let body = serde_json::json!({
            "is_available": true,
            "balance_infos": [
                {"currency": "CNY", "total_balance": "110.00"},
                {"currency": "USD", "total_balance": "2.50"}
            ]
        });

        assert_eq!(
            format_deepseek_balance(&body).unwrap(),
            "CNY 110.00, USD 2.50"
        );
    }

    #[test]
    fn formats_mimo_balance_with_usage() {
        let balance_body = serde_json::json!({
            "code": 0,
            "data": { "balance": "4.80", "frozenBalance": "0.00", "currency": "CNY", "giftBalance": "4.80", "cashBalance": "0.00" }
        });
        let usage_body = serde_json::json!({
            "code": 0,
            "data": { "usage": { "items": [{ "name": "plan_total_token", "used": 10136576, "limit": 700000000, "percent": 0.01 }] } }
        });

        let detail = format_mimo_balance(&balance_body, &usage_body).unwrap();
        assert!(detail.balance_text.contains("CNY 4.80"));
        assert!(detail.balance_text.contains("套餐 1014万/7.0亿"));
        assert_eq!(detail.token_plan_used, Some(10136576));
        assert_eq!(detail.token_plan_limit, Some(700000000));
        assert!(detail.token_plan_percent.is_some());
        // percent is computed from used/limit, not from the API's unreliable percent field
        let expected_pct = 10136576.0 * 100.0 / 700000000.0;
        assert!((detail.token_plan_percent.unwrap() - expected_pct).abs() < 0.001);
    }

    #[test]
    fn formats_mimo_balance_without_usage() {
        let balance_body = serde_json::json!({
            "code": 0,
            "data": { "balance": "10.00", "currency": "CNY" }
        });
        let usage_body = serde_json::json!({
            "code": 0,
            "data": { "usage": { "items": [] } }
        });

        let detail = format_mimo_balance(&balance_body, &usage_body).unwrap();
        assert_eq!(detail.balance_text, "CNY 10.00");
        assert_eq!(detail.token_plan_used, None);
        assert_eq!(detail.token_plan_limit, None);
        assert_eq!(detail.token_plan_percent, None);
    }

    #[test]
    fn rejects_mimo_balance_with_nonzero_code() {
        let balance_body = serde_json::json!({
            "code": 1,
            "msg": "authentication failed"
        });
        let usage_body = serde_json::json!({});

        let err = format_mimo_balance(&balance_body, &usage_body).unwrap_err();
        assert!(err.contains("authentication failed"));
    }

    #[test]
    fn rejects_mimo_headers_when_required_values_missing() {
        let err = build_mimo_headers("", "2413232036", "slh", "ph").unwrap_err();
        assert!(err.contains("service token"));

        let err = build_mimo_headers("jwt-token", "", "slh", "ph").unwrap_err();
        assert!(err.contains("user id"));
    }

    #[test]
    fn format_token_compact_works() {
        assert_eq!(format_token_compact(0), "0");
        assert_eq!(format_token_compact(500), "500");
        assert_eq!(format_token_compact(10_000), "1万");
        assert_eq!(format_token_compact(10_136_576), "1014万");
        assert_eq!(format_token_compact(700_000_000), "7.0亿");
        assert_eq!(format_token_compact(100_000_000), "1.0亿");
        assert_eq!(format_token_compact(150_000_000), "1.5亿");
    }

    #[test]
    fn redacts_sensitive_values_from_errors() {
        let err = redacted_operational_error("HTTP 401 sk-live cookie=session_token=abc");

        assert!(!err.contains("sk-live"));
        assert!(!err.contains("session_token=abc"));
        assert!(err.contains("[redacted]"));
    }

    #[test]
    fn formats_packyapi_quota() {
        let body = serde_json::json!({
            "success": true,
            "data": { "quota": 52819275, "used_quota": 397680725, "display_name": "Tester" }
        });
        let s = format_packyapi_balance(&body).unwrap();
        assert!(s.contains("$105.6"), "got: {s}"); // 52819275 / 500000
        assert!(s.contains("$795.3"), "got: {s}"); // 397680725 / 500000
    }

    #[test]
    fn rejects_packyapi_missing_data() {
        let body = serde_json::json!({ "success": false, "message": "unauthorized" });
        assert!(format_packyapi_balance(&body).is_err());
    }
}
