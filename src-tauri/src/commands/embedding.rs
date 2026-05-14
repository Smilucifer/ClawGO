use crate::models::{EmbeddingConfig, TestEmbeddingResult};
use crate::storage::settings;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::time::Instant;
use tauri::command;

static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::new()
});

fn build_embedding_request(
    config: &EmbeddingConfig,
    input: &str,
    timeout_secs: u64,
) -> Result<reqwest::RequestBuilder, String> {
    let body = serde_json::json!({
        "input": input,
        "model": config.model,
    });
    let mut req = HTTP_CLIENT
        .post(&config.endpoint)
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .json(&body);
    if let Some(ref key) = config.api_key {
        req = req.header("Authorization", format!("Bearer {}", key));
    }
    Ok(req)
}

#[command]
pub async fn get_embedding_config() -> Result<Option<EmbeddingConfig>, String> {
    Ok(settings::get_embedding_config())
}

#[command]
pub async fn update_embedding_config(config: EmbeddingConfig) -> Result<EmbeddingConfig, String> {
    settings::update_embedding_config(config)
}

#[command]
pub async fn test_embedding_connection() -> Result<TestEmbeddingResult, String> {
    let config = settings::get_embedding_config().ok_or("No embedding config")?;

    if !config.enabled {
        return Err("Embedding is disabled".into());
    }

    let start = Instant::now();
    let req = build_embedding_request(&config, "test connection", 5)?;

    let resp = req.send().await.map_err(|e| e.to_string())?;
    let latency = start.elapsed().as_millis() as u64;

    if !resp.status().is_success() {
        return Ok(TestEmbeddingResult {
            success: false,
            latency_ms: latency,
            dimension: 0,
            error: Some(format!("HTTP {}", resp.status())),
        });
    }

    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let dimension = json["data"][0]["embedding"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    Ok(TestEmbeddingResult {
        success: true,
        latency_ms: latency,
        dimension,
        error: None,
    })
}

/// Fetch embedding vector for a text string from the configured API.
/// Used by memory CRUD and auto-extraction — not a Tauri command.
pub async fn fetch_embedding(text: &str) -> Result<Vec<f32>, String> {
    let config = settings::get_embedding_config().ok_or("No embedding config")?;

    if !config.enabled {
        return Err("Embedding is disabled".into());
    }

    // Use char-based truncation to avoid UTF-8 byte-slice panic
    let truncated: String = text.chars().take(2000).collect();
    let req = build_embedding_request(&config, &truncated, 3)?;

    let resp = req.send().await.map_err(|e| e.to_string())?;
    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let embedding: Vec<f32> = json["data"][0]["embedding"]
        .as_array()
        .ok_or("Missing embedding data")?
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect();

    Ok(embedding)
}
