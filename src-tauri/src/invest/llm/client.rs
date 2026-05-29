use async_trait::async_trait;
use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use std::time::Duration;

use super::types::*;

/// OpenAI-compatible HTTP client. Single implementation for all 3 providers,
/// differentiated by `base_url` in `LlmConfig`.
pub struct OpenAiCompatClient {
    http: reqwest::Client,
}

impl OpenAiCompatClient {
    /// Build a new client. Reads proxy from environment (HTTP_PROXY/HTTPS_PROXY)
    /// via reqwest's default proxy detection (D6: reuse existing ClawGO proxy).
    pub fn new() -> Result<Self, LlmError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| LlmError::NetworkError(format!("build client: {}", e)))?;
        Ok(Self { http })
    }

    /// Build with explicit proxy URL (from UserSettings if available).
    pub fn with_proxy(proxy_url: &str) -> Result<Self, LlmError> {
        let proxy = reqwest::Proxy::all(proxy_url)
            .map_err(|e| LlmError::InvalidRequest(format!("invalid proxy: {}", e)))?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .proxy(proxy)
            .build()
            .map_err(|e| LlmError::NetworkError(format!("build client: {}", e)))?;
        Ok(Self { http })
    }

    fn build_headers(api_key: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key))
                .unwrap_or_else(|_| HeaderValue::from_static("")),
        );
        headers
    }
}

// ---------------------------------------------------------------------------
// OpenAI ChatCompletion request/response types
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
    max_tokens: u32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ChatTool>>,
}

#[derive(serde::Serialize)]
struct ChatMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(serde::Serialize)]
struct ChatTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: ToolDef,
}

// SSE streaming response chunks

#[derive(Debug, Deserialize)]
struct StreamResponse {
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<StreamUsage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: Option<StreamDelta>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<StreamToolCallDelta>>,
    #[serde(default)]
    role: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamToolCallDelta {
    index: usize,
    id: Option<String>,
    function: Option<StreamFunctionDelta>,
}

#[derive(Debug, Deserialize, Default)]
struct StreamFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

// ---------------------------------------------------------------------------
// SSE line parser
// ---------------------------------------------------------------------------

fn parse_sse_line(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(':') {
        return None;
    }
    if let Some(data) = line.strip_prefix("data: ") {
        let data = data.trim();
        if data == "[DONE]" {
            return None;
        }
        return Some(data);
    }
    None
}

// ---------------------------------------------------------------------------
// Trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl InvestLlmClient for OpenAiCompatClient {
    async fn chat_stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: Option<&[ToolDef]>,
        config: &LlmConfig,
    ) -> Result<BoxStream<'static, StreamChunk>, LlmError> {
        let url = format!("{}/chat/completions", config.provider.base_url());

        let mut chat_messages = vec![ChatMessage {
            role: "system".to_string(),
            content: system.to_string(),
            tool_call_id: None,
            name: None,
        }];
        for msg in messages {
            chat_messages.push(ChatMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
                tool_call_id: None,
                name: None,
            });
        }

        let chat_tools = tools.map(|ts| {
            ts.iter()
                .map(|t| ChatTool {
                    tool_type: "function".to_string(),
                    function: t.clone(),
                })
                .collect()
        });

        let body = ChatRequest {
            model: config.model.clone(),
            messages: chat_messages,
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            stream: true,
            tools: chat_tools,
        };

        let api_key = resolve_api_key(&config.provider)?;

        let headers = Self::build_headers(&api_key);
        let timeout = Duration::from_secs(config.timeout_secs);

        let resp = self
            .http
            .post(&url)
            .headers(headers)
            .json(&body)
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmError::Timeout
                } else {
                    LlmError::NetworkError(format!("send: {}", e))
                }
            })?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 => LlmError::Unauthorized,
                400 => LlmError::InvalidRequest(body_text),
                429 => {
                    let retry_after = body_text
                        .find("retry_after")
                        .and_then(|_| serde_json::from_str::<serde_json::Value>(&body_text).ok())
                        .and_then(|v| v["retry_after_ms"].as_u64());
                    LlmError::RateLimit {
                        retry_after_ms: retry_after,
                    }
                }
                500..=599 => LlmError::ServerError(status.as_u16()),
                _ => LlmError::NetworkError(format!("HTTP {}: {}", status, body_text)),
            });
        }

        let byte_stream = resp.bytes_stream();

        // Buffer across HTTP chunks to handle SSE lines split by chunked transfer
        let mut line_buffer = String::new();
        // Track open tool call IDs so we can emit ToolCallEnd on finish
        let mut open_tool_calls: Vec<String> = Vec::new();

        let chunk_stream = byte_stream.flat_map(move |result| {
            let bytes = match result {
                Ok(b) => b,
                Err(e) => {
                    return futures_util::stream::iter(vec![StreamChunk::Error {
                        message: format!("stream read: {}", e),
                    }]);
                }
            };

            let text = String::from_utf8_lossy(&bytes);
            line_buffer.push_str(&text);

            let mut chunks = Vec::new();

            // Process all complete lines; the last element may be a partial line.
            let lines: Vec<&str> = line_buffer.split('\n').collect();
            let last = lines.last().copied().unwrap_or("");
            let complete_lines = &lines[..lines.len().saturating_sub(1)];

            for line in complete_lines {
                if let Some(data) = parse_sse_line(line) {
                    match serde_json::from_str::<StreamResponse>(data) {
                        Ok(resp) => {
                            for choice in resp.choices {
                                if let Some(delta) = choice.delta {
                                    if let Some(content) = delta.content {
                                        if !content.is_empty() {
                                            chunks.push(StreamChunk::Delta { content });
                                        }
                                    }
                                    if let Some(tool_calls) = delta.tool_calls {
                                        for tc in tool_calls {
                                            let has_id = tc.id.is_some();
                                            let has_name = tc
                                                .function
                                                .as_ref()
                                                .and_then(|f| f.name.as_ref())
                                                .is_some();

                                            // Emit ToolCallStart when we get id + name
                                            if has_id && has_name {
                                                let id = tc.id.clone().unwrap();
                                                let name =
                                                    tc.function.as_ref().unwrap().name.clone().unwrap();
                                                chunks.push(StreamChunk::ToolCallStart {
                                                    id: id.clone(),
                                                    name,
                                                });
                                                open_tool_calls.push(id);
                                            }

                                            // Emit ToolCallDelta for arguments
                                            if let Some(ref func) = tc.function {
                                                if let Some(ref args) = func.arguments {
                                                    if !args.is_empty() {
                                                        let id = tc
                                                            .id
                                                            .clone()
                                                            .unwrap_or_else(|| {
                                                                format!("tc_{}", tc.index)
                                                            });
                                                        chunks.push(StreamChunk::ToolCallDelta {
                                                            id,
                                                            args_delta: args.clone(),
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if let Some(finish_reason) = choice.finish_reason {
                                    // Emit ToolCallEnd for all open tool calls before finishing
                                    for id in open_tool_calls.drain(..) {
                                        chunks.push(StreamChunk::ToolCallEnd { id });
                                    }
                                    let usage = resp
                                        .usage
                                        .as_ref()
                                        .map(|u| Usage {
                                            prompt_tokens: u.prompt_tokens.unwrap_or(0),
                                            completion_tokens: u.completion_tokens.unwrap_or(0),
                                            total_tokens: u.total_tokens.unwrap_or(0),
                                        })
                                        .unwrap_or_default();
                                    chunks.push(StreamChunk::Finished {
                                        finish_reason,
                                        usage,
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            log::debug!("SSE parse error: {}", e);
                        }
                    }
                }
            }

            // Keep the last (potentially incomplete) line in the buffer
            line_buffer = last.to_string();

            futures_util::stream::iter(chunks)
        });

        Ok(Box::pin(chunk_stream))
    }
}

// ---------------------------------------------------------------------------
// API key resolution from PlatformCredential (D6: reuse ClawGO config)
// ---------------------------------------------------------------------------

fn resolve_api_key(provider: &ProviderId) -> Result<String, LlmError> {
    let config_path = get_llm_config_path();
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| LlmError::NetworkError(format!("read llm_config: {}", e)))?;
        if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
            let key = match provider {
                ProviderId::DeepSeek => config["deepseek"]["api_key"].as_str(),
                ProviderId::MiMoPlan => config["mimo_plan"]["api_key"].as_str(),
                ProviderId::MiMoApi => config["mimo_api"]["api_key"].as_str(),
            };
            if let Some(k) = key {
                if !k.is_empty() {
                    return Ok(k.to_string());
                }
            }
        }
    }

    let env_key = match provider {
        ProviderId::DeepSeek => "DEEPSEEK_API_KEY",
        ProviderId::MiMoPlan => "MIMO_PLAN_API_KEY",
        ProviderId::MiMoApi => "MIMO_API_KEY",
    };
    if let Ok(key) = std::env::var(env_key) {
        if !key.is_empty() {
            return Ok(key);
        }
    }

    Err(LlmError::Unauthorized)
}

pub fn get_llm_config_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".claw-go").join("invest").join("llm_config.json")
}
