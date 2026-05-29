use async_trait::async_trait;
use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Provider identity
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    DeepSeek,
    MiMoPlan,
    MiMoApi,
}

impl ProviderId {
    pub fn base_url(&self) -> &'static str {
        match self {
            Self::DeepSeek => "https://api.deepseek.com/v1",
            Self::MiMoPlan => "https://token-plan-cn.xiaomimimo.com/v1",
            Self::MiMoApi => "https://api.xiaomimimo.com/v1",
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            Self::DeepSeek => "deepseek-v4-pro",
            Self::MiMoPlan => "mimo-v2.5-pro",
            Self::MiMoApi => "mimo-v2.5-pro",
        }
    }

    pub fn platform_id(&self) -> &'static str {
        match self {
            Self::DeepSeek => "deepseek",
            Self::MiMoPlan => "mimo-plan",
            Self::MiMoApi => "mimo-api",
        }
    }

    pub fn all() -> &'static [ProviderId] {
        &[Self::DeepSeek, Self::MiMoPlan, Self::MiMoApi]
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeepSeek => write!(f, "DeepSeek"),
            Self::MiMoPlan => write!(f, "MiMo Plan"),
            Self::MiMoApi => write!(f, "MiMo API"),
        }
    }
}

// ---------------------------------------------------------------------------
// LLM config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: ProviderId,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: u32,
    pub timeout_secs: u64,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: ProviderId::DeepSeek,
            model: "deepseek-v4-pro".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            timeout_secs: 60,
        }
    }
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".to_string(), content: content.into(), tool_call_id: None, tool_calls: None, name: None }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".to_string(), content: content.into(), tool_call_id: None, tool_calls: None, name: None }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".to_string(), content: content.into(), tool_call_id: None, tool_calls: None, name: None }
    }
}

// ---------------------------------------------------------------------------
// Tool definitions (OpenAI function calling)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Usage
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// ---------------------------------------------------------------------------
// Streaming chunks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum StreamChunk {
    Delta { content: String },
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, args_delta: String },
    ToolCallEnd { id: String },
    Finished { finish_reason: String, usage: Usage },
    Error { message: String },
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum LlmError {
    RateLimit { retry_after_ms: Option<u64> },
    Timeout,
    NetworkError(String),
    ParseError(String),
    Unauthorized,
    InvalidRequest(String),
    ServerError(u16),
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimit { retry_after_ms } => {
                write!(f, "Rate limited")?;
                if let Some(ms) = retry_after_ms {
                    write!(f, " (retry after {}ms)", ms)?;
                }
                Ok(())
            }
            Self::Timeout => write!(f, "Request timed out"),
            Self::NetworkError(e) => write!(f, "Network error: {}", e),
            Self::ParseError(e) => write!(f, "Parse error: {}", e),
            Self::Unauthorized => write!(f, "Unauthorized (401)"),
            Self::InvalidRequest(e) => write!(f, "Invalid request: {}", e),
            Self::ServerError(code) => write!(f, "Server error ({})", code),
        }
    }
}

impl std::error::Error for LlmError {}

// ---------------------------------------------------------------------------
// Retry helper (RFC 1.4)
// ---------------------------------------------------------------------------

pub async fn call_with_retry<F, Fut, T>(f: F) -> Result<T, LlmError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, LlmError>>,
{
    let mut delay = std::time::Duration::from_millis(500);
    let mut last_err = LlmError::Timeout;
    for attempt in 0..3 {
        match f().await {
            Ok(v) => return Ok(v),
            Err(LlmError::RateLimit { retry_after_ms }) => {
                let d = retry_after_ms
                    .map(std::time::Duration::from_millis)
                    .unwrap_or(delay);
                last_err = LlmError::RateLimit { retry_after_ms };
                tokio::time::sleep(d).await;
                delay *= 2;
            }
            Err(e @ (LlmError::Timeout | LlmError::NetworkError(_) | LlmError::ServerError(_))) => {
                log::warn!("LLM call attempt {} failed, retrying in {:?}", attempt + 1, delay);
                last_err = e;
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
            Err(e) => return Err(e), // 401/400 never retry
        }
    }
    Err(last_err)
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait InvestLlmClient: Send + Sync {
    /// Stream a chat completion. Returns a stream of `StreamChunk`.
    /// For batch mode (Phase 3a), callers collect the stream into a full response.
    /// For streaming mode (Phase 3b), callers forward chunks to Tauri event channel.
    async fn chat_stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: Option<&[ToolDef]>,
        config: &LlmConfig,
    ) -> Result<BoxStream<'static, StreamChunk>, LlmError>;
}

// ---------------------------------------------------------------------------
// Collected response (batch mode convenience)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct CollectedResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: String,
    pub usage: Usage,
}

/// Collect a stream into a single `CollectedResponse`.
pub async fn collect_stream(mut stream: BoxStream<'static, StreamChunk>) -> CollectedResponse {
    let mut result = CollectedResponse::default();
    let mut tool_call_map: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();

    while let Some(chunk) = stream.next().await {
        match chunk {
            StreamChunk::Delta { content } => {
                result.content.push_str(&content);
            }
            StreamChunk::ToolCallStart { id, name } => {
                tool_call_map.insert(id.clone(), (name, String::new()));
            }
            StreamChunk::ToolCallDelta { id, args_delta } => {
                if let Some((_, args)) = tool_call_map.get_mut(&id) {
                    args.push_str(&args_delta);
                }
            }
            StreamChunk::ToolCallEnd { id } => {
                if let Some((name, args)) = tool_call_map.remove(&id) {
                    let arguments = serde_json::from_str(&args).unwrap_or_else(|e| {
                        log::warn!("Failed to parse tool call args for {}: {}", id, e);
                        serde_json::Value::Null
                    });
                    result.tool_calls.push(ToolCall { id, name, arguments });
                }
            }
            StreamChunk::Finished { finish_reason, usage } => {
                result.finish_reason = finish_reason;
                result.usage = usage;
            }
            StreamChunk::Error { message } => {
                result.finish_reason = format!("error: {}", message);
            }
        }
    }
    result
}

use futures_util::StreamExt;
