use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct UsageWindow {
    pub utilization: f64,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaudeSubscriptionUsage {
    pub five_hour: Option<UsageWindow>,
    pub seven_day: Option<UsageWindow>,
    pub seven_day_opus: Option<UsageWindow>,
    pub subscription_type: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub fetched_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OauthBlock>,
}

#[derive(Debug, Deserialize)]
struct OauthBlock {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
    /// B1 correction: rate_limit_tier lives in credentials, not in the usage response body.
    #[serde(rename = "rateLimitTier")]
    rate_limit_tier: Option<String>,
}

/// Extract (access_token, subscription_type, rate_limit_tier) from credentials JSON text.
fn parse_credentials(text: &str) -> Result<(String, Option<String>, Option<String>), String> {
    let parsed: CredentialsFile =
        serde_json::from_str(text).map_err(|e| format!("credentials parse error: {e}"))?;
    let oauth = parsed
        .claude_ai_oauth
        .ok_or_else(|| "no claudeAiOauth block".to_string())?;
    let token = oauth
        .access_token
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| "no accessToken".to_string())?;
    Ok((token, oauth.subscription_type, oauth.rate_limit_tier))
}

fn credentials_path() -> PathBuf {
    crate::storage::teams::claude_home_dir().join(".credentials.json")
}

fn json_window(v: &serde_json::Value, key: &str) -> Option<UsageWindow> {
    let w = v.get(key)?;
    // B1 correction: utilization is 0-100 (percent); divide by 100 so frontend receives 0..1.
    Some(UsageWindow {
        utilization: w.get("utilization").and_then(|x| x.as_f64()).unwrap_or(0.0) / 100.0,
        resets_at: w
            .get("resets_at")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
    })
}

fn empty_with_error(fetched_at: String, error: String) -> ClaudeSubscriptionUsage {
    ClaudeSubscriptionUsage {
        five_hour: None,
        seven_day: None,
        seven_day_opus: None,
        subscription_type: None,
        rate_limit_tier: None,
        fetched_at,
        error: Some(error),
    }
}

#[tauri::command]
pub async fn get_claude_subscription_usage() -> Result<ClaudeSubscriptionUsage, String> {
    let fetched_at = crate::models::now_iso();

    let text = match std::fs::read_to_string(credentials_path()) {
        Ok(t) => t,
        Err(e) => {
            return Ok(empty_with_error(fetched_at, format!("no credentials: {e}")));
        }
    };
    let (token, sub_type, rate_limit_tier) = match parse_credentials(&text) {
        Ok(v) => v,
        Err(e) => return Ok(empty_with_error(fetched_at, e)),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(&token)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => return Ok(empty_with_error(fetched_at, format!("request failed: {e}"))),
    };
    if !resp.status().is_success() {
        // 401 etc.: token expired, degrade gracefully (no refreshToken refresh).
        return Ok(empty_with_error(
            fetched_at,
            format!("usage http {}", resp.status().as_u16()),
        ));
    }
    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return Ok(empty_with_error(fetched_at, format!("bad json: {e}"))),
    };

    Ok(ClaudeSubscriptionUsage {
        five_hour: json_window(&body, "five_hour"),
        seven_day: json_window(&body, "seven_day"),
        seven_day_opus: json_window(&body, "seven_day_opus"),
        subscription_type: sub_type,
        // B1 correction: rate_limit_tier comes from credentials, not from response body.
        rate_limit_tier,
        fetched_at,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_credentials_extracts_token_and_type() {
        let json = r#"{"claudeAiOauth":{"accessToken":"abc","subscriptionType":"max","rateLimitTier":"default_claude_max_5x"}}"#;
        let (tok, sub, tier) = parse_credentials(json).unwrap();
        assert_eq!(tok, "abc");
        assert_eq!(sub.as_deref(), Some("max"));
        assert_eq!(tier.as_deref(), Some("default_claude_max_5x"));
    }

    #[test]
    fn parse_credentials_errors_when_missing() {
        assert!(parse_credentials(r#"{}"#).is_err());
        assert!(parse_credentials(r#"{"claudeAiOauth":{"accessToken":""}}"#).is_err());
    }

    #[test]
    fn json_window_divides_utilization_by_100() {
        let v = serde_json::json!({
            "five_hour": { "utilization": 26.0, "resets_at": "2026-06-26T06:40:00+00:00" }
        });
        let w = json_window(&v, "five_hour").unwrap();
        assert!((w.utilization - 0.26).abs() < 1e-9);
        assert_eq!(w.resets_at.as_deref(), Some("2026-06-26T06:40:00+00:00"));
    }

    #[test]
    fn json_window_returns_none_for_null_key() {
        let v = serde_json::json!({ "seven_day_opus": null });
        assert!(json_window(&v, "seven_day_opus").is_none());
        assert!(json_window(&v, "nonexistent").is_none());
    }
}
