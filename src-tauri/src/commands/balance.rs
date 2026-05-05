use crate::models::{BalanceCacheEntry, BalanceHelperSettings};
use crate::storage;
use reqwest::header::{COOKIE, USER_AGENT};
use serde_json::Value;
use std::time::Duration;

const DEEPSEEK_BALANCE_BASE_URL: &str = "https://api.deepseek.com";
const PACKY_CONSOLE_URL: &str = "https://www.packyapi.com/console";

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

fn html_to_visible_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }

    out.replace("&yen;", "¥")
        .replace("&#165;", "¥")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn has_digit(value: &str) -> bool {
    value.chars().any(|ch| ch.is_ascii_digit())
}

fn trim_amount_token(value: &str) -> String {
    value
        .trim_matches(|ch: char| {
            ch == ','
                || ch == ';'
                || ch == ':'
                || ch == '"'
                || ch == '\''
                || ch == ')'
                || ch == ']'
                || ch == '}'
        })
        .to_string()
}

fn is_currency_token(value: &str) -> bool {
    matches!(value, "¥" | "￥" | "$" | "USD" | "CNY" | "RMB")
}

fn extract_packy_balance_text(html: &str) -> Result<String, String> {
    let visible = html_to_visible_text(html);
    let tokens = visible.split_whitespace().collect::<Vec<_>>();

    for pair in tokens.windows(2) {
        let marker = trim_amount_token(pair[0]);
        let amount = trim_amount_token(pair[1]);
        if is_currency_token(&marker) && has_digit(&amount) {
            return Ok(format!("{marker} {amount}"));
        }
        if marker.chars().any(|ch| matches!(ch, '¥' | '￥' | '$')) && has_digit(&marker) {
            return Ok(marker);
        }
    }

    for triple in tokens.windows(3) {
        let label = triple[0].to_ascii_lowercase();
        if label.contains("balance") || triple[0].contains("余额") || triple[0].contains("剩余")
        {
            let marker = trim_amount_token(triple[1]);
            let amount = trim_amount_token(triple[2]);
            if is_currency_token(&marker) && has_digit(&amount) {
                return Ok(format!("{marker} {amount}"));
            }
            if has_digit(&marker) {
                return Ok(marker);
            }
        }
    }

    Err("Packy balance was not found in the console page".to_string())
}

async fn query_packy_balance(
    client: &reqwest::Client,
    cookies: &str,
    console_url: &str,
) -> Result<String, String> {
    let trimmed_cookies = cookies.trim();
    if trimmed_cookies.is_empty() {
        return Err("Packy cookies are not configured".to_string());
    }

    let response = client
        .get(console_url)
        .header(COOKIE, trimmed_cookies)
        .header(USER_AGENT, "OpenCovibe balance helper")
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
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "Packy cookies were rejected".to_string(),
            code => format!("Packy balance request failed with HTTP {code}"),
        });
    }

    let body = response
        .text()
        .await
        .map_err(|_| "Packy console response could not be read".to_string())?;
    extract_packy_balance_text(&body)
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
        let cookies = helper.packy_session_cookies.as_deref().unwrap_or("");
        let result = query_packy_balance(&client, cookies, PACKY_CONSOLE_URL).await;
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
    fn extracts_packy_balance_from_console_html() {
        let html = r#"
            <html><body>
              <main><span>账户余额</span><strong>¥ 88.12</strong></main>
            </body></html>
        "#;

        assert_eq!(extract_packy_balance_text(html).unwrap(), "¥ 88.12");
    }

    #[test]
    fn redacts_sensitive_values_from_errors() {
        let err = redacted_operational_error("HTTP 401 sk-live cookie=session_token=abc");

        assert!(!err.contains("sk-live"));
        assert!(!err.contains("session_token=abc"));
        assert!(err.contains("[redacted]"));
    }
}
