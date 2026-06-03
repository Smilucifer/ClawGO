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

// ---------------------------------------------------------------------------
// DSML tool-call parser (DeepSeek/MiMo native format fallback)
// ---------------------------------------------------------------------------

/// Full-width vertical bar (U+FF5C) used by DeepSeek/MiMo DSML format.
const DSML_BAR: &str = "\u{FF5C}";

/// Parse tool calls from DSML format text content.
///
/// Some LLMs (DeepSeek, MiMo) occasionally return tool calls in their native
/// DSML format instead of OpenAI-compatible JSON. This function detects and
/// parses that format into standard `ToolCall` structs.
///
/// DSML format example:
/// ```xml
/// <｜｜DSML｜｜tool_calls>
/// <｜｜DSML｜｜invoke name="get_history_data">
/// <｜｜DSML｜｜parameter name="symbol" string="true">600519.SH</｜｜DSML｜｜parameter>
/// </｜｜DSML｜｜invoke>
/// </｜｜DSML｜｜tool_calls>
/// ```
pub(crate) fn parse_dsml_tool_calls(content: &str) -> Option<Vec<ToolCall>> {
    let dsml_tag = format!("<{0}{0}DSML{0}{0}tool_calls>", DSML_BAR);
    if !content.contains(&dsml_tag) {
        return None;
    }

    log::info!("Detected DSML tool-call format in LLM response, parsing...");

    let invoke_open = format!("<{0}{0}DSML{0}{0}invoke name=\"", DSML_BAR);
    let invoke_close = format!("</{0}{0}DSML{0}{0}invoke>", DSML_BAR);
    let param_open = format!("<{0}{0}DSML{0}{0}parameter name=\"", DSML_BAR);
    let param_close = format!("</{0}{0}DSML{0}{0}parameter>", DSML_BAR);

    let mut tool_calls = Vec::new();
    let mut remaining = content;

    while let Some(inv_start) = remaining.find(&invoke_open) {
        let after_open = &remaining[inv_start + invoke_open.len()..];
        let name_end = after_open.find("\">")?;
        let tool_name = &after_open[..name_end];

        let body_start = inv_start + invoke_open.len() + name_end + 2;
        let body_end = remaining[body_start..].find(&invoke_close)?;
        let body = &remaining[body_start..body_start + body_end];

        // Parse parameters
        let mut params = serde_json::Map::new();
        let mut param_rest = body;

        while let Some(p_start) = param_rest.find(&param_open) {
            let after_p = &param_rest[p_start + param_open.len()..];
            let p_name_end = after_p.find("\">")?;
            let name_and_attr = &after_p[..p_name_end];

            // Extract name (before first quote or space)
            let p_name = if let Some(q) = name_and_attr.find('"') {
                &name_and_attr[..q]
            } else if let Some(s) = name_and_attr.find(' ') {
                &name_and_attr[..s]
            } else {
                name_and_attr
            };
            let is_string = name_and_attr.contains("string=\"true\"");

            let val_start = p_name_end + 2;
            let val_end = after_p[val_start..].find(&param_close)?;
            let val = &after_p[val_start..val_start + val_end];

            let value = if is_string {
                serde_json::Value::String(val.to_string())
            } else if let Ok(n) = val.parse::<i64>() {
                serde_json::Value::Number(n.into())
            } else if let Ok(f) = val.parse::<f64>() {
                serde_json::json!(f)
            } else {
                serde_json::Value::String(val.to_string())
            };
            params.insert(p_name.to_string(), value);

            param_rest = &param_rest[p_start + param_open.len() + val_start + val_end + param_close.len()..];
        }

        tool_calls.push(ToolCall {
            id: format!("dsml_{}", tool_calls.len()),
            name: tool_name.to_string(),
            arguments: serde_json::Value::Object(params),
        });

        remaining = &remaining[body_start + body_end + invoke_close.len()..];
    }

    if tool_calls.is_empty() {
        log::warn!("DSML format detected but no tool calls parsed");
        None
    } else {
        log::info!(
            "Successfully parsed {} DSML tool calls: {:?}",
            tool_calls.len(),
            tool_calls.iter().map(|tc| &tc.name).collect::<Vec<_>>()
        );
        Some(tool_calls)
    }
}

/// Collect a stream into a single `CollectedResponse`.
///
/// After assembly, performs DSML normalization: if `tool_calls` is empty but
/// `content` contains DeepSeek/MiMo DSML-format tool call tags, parses them
/// into `tool_calls` and clears the raw XML from `content`.
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

    // Normalize DSML: if no OpenAI tool calls were streamed but content contains
    // DSML tags, parse them and move to tool_calls, clearing the raw XML.
    if result.tool_calls.is_empty() {
        if let Some(dsml_calls) = parse_dsml_tool_calls(&result.content) {
            result.content.clear();
            result.tool_calls = dsml_calls;
        }
    }

    result
}

use futures_util::StreamExt;
