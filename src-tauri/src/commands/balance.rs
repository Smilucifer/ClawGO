use crate::models::{BalanceCacheEntry, BalanceHelperSettings};
use crate::storage;
use reqwest::header::{COOKIE, HeaderMap, HeaderValue, USER_AGENT};
use serde_json::Value;
use std::time::Duration;

const DEEPSEEK_BALANCE_BASE_URL: &str = "https://api.deepseek.com";
const PACKY_API_BASE_URL: &str = "https://www.packyapi.com";
const PACKY_QUOTA_PER_UNIT: f64 = 500_000.0;
const PACKY_DISPLAY_CURRENCY: &str = "USD";

fn balance_cache_entry(source: &str, result: Result<String, String>) -> BalanceCacheEntry {
    match result {
        Ok(balance_text) => BalanceCacheEntry {
            source: source.to_string(),
            status: "ok".to_string(),
            balance_text: Some(balance_text),
            error: None,
            refreshed_at: crate::models::now_iso(),
        },
        Err(error) => BalanceCacheEntry {
            source: source.to_string(),
            status: "failed".to_string(),
            balance_text: None,
            error: Some(redacted_operational_error(&error)),
            refreshed_at: crate::models::now_iso(),
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

fn format_packy_balance(body: &Value) -> Result<String, String> {
    let data = body
        .get("data")
        .ok_or_else(|| "Packy user response did not include data".to_string())?;

    let quota = data
        .get("quota")
        .and_then(Value::as_i64)
        .or_else(|| data.get("quota").and_then(Value::as_u64).map(|v| v as i64))
        .ok_or_else(|| "Packy user response did not include quota".to_string())?;

    let amount = quota as f64 / PACKY_QUOTA_PER_UNIT;
    Ok(format!("{} {:.2}", PACKY_DISPLAY_CURRENCY, amount))
}

fn build_packy_headers(
    session: &str,
    tdc_itoken: &str,
    user_id: &str,
) -> Result<HeaderMap, String> {
    let trimmed_session = session.trim();
    let trimmed_itoken = tdc_itoken.trim();
    let trimmed_user_id = user_id.trim();

    if trimmed_session.is_empty() {
        return Err("Packy session is not configured".to_string());
    }
    if trimmed_itoken.is_empty() {
        return Err("Packy TDC_itoken is not configured".to_string());
    }
    if trimmed_user_id.is_empty() {
        return Err("Packy user id is not configured".to_string());
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        COOKIE,
        HeaderValue::from_str(&format!(
            "session={}; TDC_itoken={}",
            trimmed_session, trimmed_itoken
        ))
        .map_err(|_| "Packy credentials contain invalid header characters".to_string())?,
    );
    headers.insert(
        "New-API-User",
        HeaderValue::from_str(trimmed_user_id)
            .map_err(|_| "Packy user id contains invalid header characters".to_string())?,
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36",
        ),
    );
    headers.insert("Accept", HeaderValue::from_static("application/json, text/plain, */*"));
    headers.insert(
        "Referer",
        HeaderValue::from_static("https://www.packyapi.com/console"),
    );
    headers.insert(
        "Origin",
        HeaderValue::from_static("https://www.packyapi.com"),
    );
    Ok(headers)
}

async fn query_packy_balance(
    client: &reqwest::Client,
    session: &str,
    tdc_itoken: &str,
    user_id: &str,
) -> Result<String, String> {
    let headers = build_packy_headers(session, tdc_itoken, user_id)?;
    let response = client
        .get(format!("{}/api/user/self", PACKY_API_BASE_URL))
        .headers(headers)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "Packy balance request timed out".to_string()
            } else {
                format!("Packy balance request failed: {e}")
            }
        })?;

    let status = response.status();
    let body = response
        .json::<Value>()
        .await
        .map_err(|_| "Packy balance response was not valid JSON".to_string())?;

    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => body
                .get("message")
                .and_then(Value::as_str)
                .map(|s| format!("Packy authentication failed: {s}"))
                .unwrap_or_else(|| "Packy authentication failed".to_string()),
            code => format!("Packy balance request failed with HTTP {code}"),
        });
    }

    if body.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(body
            .get("message")
            .and_then(Value::as_str)
            .map(|s| format!("Packy balance request failed: {s}"))
            .unwrap_or_else(|| "Packy balance request failed".to_string()));
    }

    format_packy_balance(&body)
}

async fn refresh_balance_status_inner(
    source: Option<String>,
) -> Result<BalanceHelperSettings, String> {
    let requested = source.unwrap_or_else(|| "all".to_string());
    if !matches!(requested.as_str(), "all" | "deepseek" | "packy") {
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

    if requested == "all" || requested == "packy" {
        let result = query_packy_balance(
            &client,
            helper.packy_session.as_deref().unwrap_or(""),
            helper.packy_tdc_itoken.as_deref().unwrap_or(""),
            helper.packy_user_id.as_deref().unwrap_or(""),
        )
        .await;
        helper
            .cache
            .insert("packy".to_string(), balance_cache_entry("packy", result));
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
    fn formats_packy_balance_from_quota() {
        let body = serde_json::json!({
            "success": true,
            "data": {
                "quota": 87_304_703
            }
        });

        assert_eq!(format_packy_balance(&body).unwrap(), "USD 174.61");
    }

    #[test]
    fn rejects_packy_headers_when_required_values_missing() {
        let err = build_packy_headers("", "595383047:1776349439", "98264").unwrap_err();
        assert_eq!(err, "Packy session is not configured");

        let err = build_packy_headers("session", "", "98264").unwrap_err();
        assert_eq!(err, "Packy TDC_itoken is not configured");

        let err = build_packy_headers("session", "595383047:1776349439", "").unwrap_err();
        assert_eq!(err, "Packy user id is not configured");
    }

    #[test]
    fn redacts_sensitive_values_from_errors() {
        let err = redacted_operational_error("HTTP 401 sk-live cookie=session_token=abc");

        assert!(!err.contains("sk-live"));
        assert!(!err.contains("session_token=abc"));
        assert!(err.contains("[redacted]"));
    }
}
