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

/// Common tail for all tool-call parsers: log result and wrap in `Option`.
/// Infer a JSON value from a string: try bool, i64, f64, then fallback to string.
/// Known tool-call tag pairs for residual stripping.
/// Shared between parsers and strip_residual_tool_call_tags.
const TOOL_CALL_TAG_PAIRS: &[(&str, &str)] = &[
    ("<function_calls>", "</function_calls>"),
    ("<tool_call>", "</tool_call>"),
    ("<｜DSML｜function_calls>", "</｜DSML｜function_calls>"),
    ("<｜DSML｜tool_calls>", "</｜DSML｜tool_calls>"),
];

fn infer_json_value(val: &str) -> serde_json::Value {
    if val.eq_ignore_ascii_case("true") {
        serde_json::Value::Bool(true)
    } else if val.eq_ignore_ascii_case("false") {
        serde_json::Value::Bool(false)
    } else if let Ok(n) = val.parse::<i64>() {
        serde_json::Value::Number(n.into())
    } else if let Ok(f) = val.parse::<f64>() {
        serde_json::json!(f)
    } else {
        serde_json::Value::String(val.to_string())
    }
}

fn finish_tool_parse(tool_calls: Vec<ToolCall>, label: &str) -> Option<Vec<ToolCall>> {
    if tool_calls.is_empty() {
        log::warn!("{label} format detected but no tool calls parsed");
        None
    } else {
        log::info!(
            "Successfully parsed {} {label} tool calls: {:?}",
            tool_calls.len(),
            tool_calls.iter().map(|tc| &tc.name).collect::<Vec<_>>()
        );
        Some(tool_calls)
    }
}

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
    let has_double = content.contains(&dsml_tag);

    // Also handle single-bar variant: <｜DSML｜function_calls> or <｜DSML｜tool_calls>
    let sb_fc_tag = "<｜DSML｜function_calls>";
    let sb_tc_tag = "<｜DSML｜tool_calls>";
    let has_single = content.contains(sb_fc_tag) || content.contains(sb_tc_tag);

    if !has_double && !has_single {
        return None;
    }

    log::info!("Detected DSML tool-call format in LLM response, parsing...");

    // Normalize single-bar to double-bar: replace all single-bar DSML prefixes
    let normalized: String;
    let work_content = if has_single {
        let double_prefix = format!("<{0}{0}DSML{0}{0}", DSML_BAR);
        normalized = content.replace("<｜DSML｜", &double_prefix);
        &normalized
    } else {
        content
    };

    let invoke_open = format!("<{0}{0}DSML{0}{0}invoke name=\"", DSML_BAR);
    let invoke_close = format!("</{0}{0}DSML{0}{0}invoke>", DSML_BAR);
    let param_open = format!("<{0}{0}DSML{0}{0}parameter name=\"", DSML_BAR);
    let param_close = format!("</{0}{0}DSML{0}{0}parameter>", DSML_BAR);

    let mut tool_calls = Vec::new();
    let mut remaining = work_content;

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

    finish_tool_parse(tool_calls, "DSML")
}
/// Parse tool calls from `<function_calls><invoke name=...">...` XML format.
///
/// Some LLMs occasionally emit tool calls in this format.
pub(crate) fn parse_function_calls(content: &str) -> Option<Vec<ToolCall>> {
    if !content.contains("<function_calls>") {
        return None;
    }

    log::info!("Detected <function_calls> tool-call format in LLM response, parsing...");

    let mut tool_calls = Vec::new();
    let mut remaining = content;

    while let Some(inv_start) = remaining.find(r#"<invoke name=""#) {
        let tag = r#"<invoke name=""#;
        let after_open = &remaining[inv_start + tag.len()..];
        let Some(name_end) = after_open.find(r#"">"#) else {
            break;
        };
        let tool_name = &after_open[..name_end];

        let body_start = inv_start + tag.len() + name_end + 2;
        let Some(body_end) = remaining[body_start..].find("</invoke>") else {
            break;
        };
        let body = &remaining[body_start..body_start + body_end];

        let mut params = serde_json::Map::new();
        let mut param_rest = body;
        let ptag = r#"<parameter name=""#;

        while let Some(p_start) = param_rest.find(ptag) {
            let after_p = &param_rest[p_start + ptag.len()..];
            let Some(p_name_end) = after_p.find(r#"">"#) else {
                break;
            };
            let p_name = &after_p[..p_name_end];

            let val_start = p_name_end + 2;
            let Some(val_end) = after_p[val_start..].find("</parameter>") else {
                break;
            };
            let val = after_p[val_start..val_start + val_end].trim();

            let value = infer_json_value(val);
            params.insert(p_name.to_string(), value);

            param_rest =
                &param_rest[p_start + ptag.len() + val_start + val_end + "</parameter>".len()..];
        }

        tool_calls.push(ToolCall {
            id: format!("fn_{}", tool_calls.len()),
            name: tool_name.to_string(),
            arguments: serde_json::Value::Object(params),
        });

        remaining = &remaining[body_start + body_end + "</invoke>".len()..];
    }

    finish_tool_parse(tool_calls, "<function_calls>")
}


/// Parse `<function=name>` and `<parameter=name>value</parameter>` tags from non-JSON tool_call body.<</parameter>> tags from non-JSON tool_call body.
fn parse_fn_tag_body(body: &str, tool_calls: &mut Vec<ToolCall>) {
    let fn_m = "<function=";
    let fn_name = if let Some(start) = body.find(fn_m) {
        let after = &body[start + fn_m.len()..];
        let end_idx = after.find('>').unwrap_or(after.len());
        let raw = &after[..end_idx];
        raw.trim_matches(|c| c == '"' || c == '\'')
    } else {
        return;
    };

    let mut params = serde_json::Map::new();
    let mut rest = body;
    let po = "<parameter=";
    let pc = "</parameter>";

    while let Some(ps) = rest.find(po) {
        let ao = &rest[ps + po.len()..];
        let ne = ao.find('>').unwrap_or(ao.len());
        let pn = ao[..ne].trim_matches(|c| c == '"' || c == '\'');
        let vs = ne + 1;
        let Some(ve) = ao[vs..].find(pc) else { break; };
        let val = ao[vs..vs + ve].trim();
        let value = if val.eq_ignore_ascii_case("true") {
            serde_json::Value::Bool(true)
        } else if val.eq_ignore_ascii_case("false") {
            serde_json::Value::Bool(false)
        } else if let Ok(n) = val.parse::<i64>() {
            serde_json::Value::Number(n.into())
        } else if let Ok(f) = val.parse::<f64>() {
            serde_json::json!(f)
        } else {
            serde_json::Value::String(val.to_string())
        };
        params.insert(pn.to_string(), value);
        rest = &rest[ps + po.len() + vs + ve + pc.len()..];
    }

    tool_calls.push(ToolCall {
        id: format!("xml_{}", tool_calls.len()),
        name: fn_name.to_string(),
        arguments: serde_json::Value::Object(params),
    });
}

/// Parse tool calls from plain XML-like `<tool_call>` tags.
///
/// Some LLMs occasionally return tool calls as plain text with
/// `<tool_call>{"name":"...","arguments":{...}}</tool_call>` format instead
/// of using the OpenAI-compatible streaming protocol or DSML tags.
/// This function detects and parses that format into standard `ToolCall` structs.
pub(crate) fn parse_xml_tool_calls(content: &str) -> Option<Vec<ToolCall>> {
    if !content.contains("<tool_call>") {
        return None;
    }

    log::info!("Detected plain XML tool-call format in LLM response, parsing...");

    let mut tool_calls = Vec::new();
    let mut remaining = content;

    while let Some(start) = remaining.find("<tool_call>") {
        let after_open = &remaining[start + "<tool_call>".len()..];
        let Some(end) = after_open.find("</tool_call>") else {
            break; // missing closing tag — stop parsing, keep what we have
        };
        let body = after_open[..end].trim();

        // The body may contain the JSON directly, or have extra wrapping.
        // Try to parse as JSON with "name" and "arguments" fields.
        // Try JSON format first
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                let arguments = obj
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                tool_calls.push(ToolCall {
                    id: format!("xml_{}", tool_calls.len()),
                    name: name.to_string(),
                    arguments,
                });
            }
        }
        // Try <function=name> + <parameter=name>value format
        else if body.contains("<function=") {
            parse_fn_tag_body(body, &mut tool_calls);
        } else {
            log::warn!("body format not recognized, skipping");
        }

        remaining = &remaining[start + "<tool_call>".len() + end + "</tool_call>".len()..];
    }

    finish_tool_parse(tool_calls, "plain XML")
}

/// Strip residual tool-call XML tags from content when all parsers failed.
///
/// This is a last-resort safety net: if the LLM emitted tool-call XML but
/// the inner JSON was malformed (preventing successful parsing), we strip
/// the XML wrapper tags so the raw markup doesn't leak into role output.
fn strip_residual_tool_call_tags(content: &mut String) {
    // Quick check: is any known tag pattern present?
    let has_any = content.contains("<function=")
        || TOOL_CALL_TAG_PAIRS.iter().any(|(open, _)| content.contains(open));

    if !has_any {
        return;
    }

    log::warn!("Stripping residual tool-call XML tags from unparseable content");

    // Strip all known tag pairs uniformly
    for (open, close) in TOOL_CALL_TAG_PAIRS {
        while let Some(start) = content.find(open) {
            if let Some(end_rel) = content[start + open.len()..].find(close) {
                let end = start + open.len() + end_rel + close.len();
                content.replace_range(start..end, "
");
            } else {
                content.truncate(start);
                break;
            }
        }
    }

    // Clean up excessive whitespace left by stripping
    while content.contains("


") {
        *content = content.replace("


", "

");
    }
    *content = content.trim().to_string();
}

/// Collect a stream into a single `CollectedResponse`.
///
/// After assembly, performs format normalization: if `tool_calls` is empty but
/// `content` contains DSML or plain `<tool_call>` tags, parses them
/// into `tool_calls` and clears the raw text from `content`.
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

    // Normalize: if no OpenAI tool calls were streamed, try DSML → function_calls → plain XML.
    if result.tool_calls.is_empty() {
        if let Some(dsml_calls) = parse_dsml_tool_calls(&result.content) {
            result.content.clear();
            result.tool_calls = dsml_calls;
        } else if let Some(fc_calls) = parse_function_calls(&result.content) {
            result.content.clear();
            result.tool_calls = fc_calls;
        } else if let Some(xml_calls) = parse_xml_tool_calls(&result.content) {
            result.content.clear();
            result.tool_calls = xml_calls;
        } else {
            // Safety net: if no parser matched but XML tags are still present
            // (e.g. malformed JSON inside tags), strip the raw tags from content
            // to prevent leaking tool-call markup into role output.
            strip_residual_tool_call_tags(&mut result.content);
        }
    }

    result
}

use futures_util::StreamExt;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_xml_tool_calls_basic() {
        let content = r#"<tool_call>
{"name":"get_history_data","arguments":{"symbol":"000300.SH","days":90}}
</tool_call>"#;
        let calls = parse_xml_tool_calls(content).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_history_data");
        assert_eq!(calls[0].arguments["symbol"], "000300.SH");
        assert_eq!(calls[0].arguments["days"], 90);
    }

    #[test]
    fn test_parse_xml_tool_calls_multiple() {
        let content = r#"Some preamble
<tool_call>
{"name":"get_history_data","arguments":{"symbol":"600519.SH","days":60}}
</tool_call>
Some text in between
<tool_call>
{"name":"scan_stocks","arguments":{"query":"test"}}
</tool_call>
trailing text"#;
        let calls = parse_xml_tool_calls(content).expect("should parse");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "get_history_data");
        assert_eq!(calls[1].name, "scan_stocks");
    }

    #[test]
    fn test_parse_xml_tool_calls_no_tags() {
        let content = "Just a normal response with no tool calls.";
        assert!(parse_xml_tool_calls(content).is_none());
    }

    #[test]
    fn test_parse_xml_tool_calls_malformed_json() {
        // Malformed JSON inside the tag should skip gracefully
        let content = "<tool_call>\nnot valid json\n</tool_call>";
        // The parser returns None because no valid tool calls were parsed
        assert!(parse_xml_tool_calls(content).is_none());
    }

    #[test]
    fn test_parse_xml_tool_calls_empty_arguments() {
        let content = r#"<tool_call>
{"name":"some_tool","arguments":{}}
</tool_call>"#;
        let calls = parse_xml_tool_calls(content).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "some_tool");
    }

    #[test]
    fn test_parse_xml_tool_calls_missing_arguments_field() {
        let content = r#"<tool_call>
{"name":"some_tool"}
</tool_call>"#;
        let calls = parse_xml_tool_calls(content).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "some_tool");
        // Should default to empty object
        assert_eq!(calls[0].arguments, serde_json::json!({}));
    }

    #[test]
    fn test_parse_xml_tool_calls_missing_closing_tag() {
        // Missing closing tag should gracefully stop, preserving earlier results
        let content = r#"<tool_call>
{"name":"first_tool","arguments":{"x":1}}
</tool_call>
<tool_call>
{"name":"second_tool","arguments":{"y":2}}"#;
        let calls = parse_xml_tool_calls(content).expect("should parse first");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "first_tool");
    }

    // -----------------------------------------------------------------------
    // parse_function_calls tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_function_calls_basic() {
        let content = r#"<function_calls>
<invoke name="analyze_multi_timeframe">
<parameter name="symbol">002156.SZ</parameter>
<parameter name="include_news">false</parameter>
</invoke>
</function_calls>"#;
        let calls = parse_function_calls(content).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "analyze_multi_timeframe");
        assert_eq!(calls[0].arguments["symbol"], "002156.SZ");
        assert_eq!(calls[0].arguments["include_news"], false);
    }

    #[test]
    fn test_parse_function_calls_multiple() {
        let content = r#"<function_calls>
<invoke name="get_history_data">
<parameter name="symbol">600519.SH</parameter>
<parameter name="days">90</parameter>
</invoke>
<invoke name="scan_stocks">
<parameter name="query">low PE high ROE</parameter>
</invoke>
</function_calls>"#;
        let calls = parse_function_calls(content).expect("should parse");
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "get_history_data");
        assert_eq!(calls[0].arguments["days"], 90);
        assert_eq!(calls[1].name, "scan_stocks");
    }

    #[test]
    fn test_parse_function_calls_numeric_params() {
        let content = r#"<function_calls>
<invoke name="some_tool">
<parameter name="price">123.45</parameter>
<parameter name="count">100</parameter>
</invoke>
</function_calls>"#;
        let calls = parse_function_calls(content).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].arguments["price"], 123.45);
        assert_eq!(calls[0].arguments["count"], 100);
    }

    #[test]
    fn test_parse_function_calls_no_tags() {
        let content = "Just a normal response with no tool calls.";
        assert!(parse_function_calls(content).is_none());
    }

    #[test]
    fn test_parse_function_calls_with_surrounding_text() {
        let content = r#"I'll analyze the stock for you.
<function_calls>
<invoke name="get_quote">
<parameter name="symbol">600519.SH</parameter>
</invoke>
</function_calls>
Let me check the results."#;
        let calls = parse_function_calls(content).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_quote");
    }

    // -----------------------------------------------------------------------
    // strip_residual_tool_call_tags tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_strip_residual_function_calls_tags() {
        let mut content = r#"Some analysis text
<function_calls>
<invoke name="bad">
<parameter name="x">broken json
</invoke>
</function_calls>
More text"#.to_string();
        strip_residual_tool_call_tags(&mut content);
        assert!(!content.contains("<function_calls>"));
        assert!(content.contains("Some analysis text"));
        assert!(content.contains("More text"));
    }

    #[test]
    fn test_strip_residual_tool_call_tags() {
        let mut content = r#"Before
<tool_call>
not valid json
</tool_call>
After"#.to_string();
        strip_residual_tool_call_tags(&mut content);
        assert!(!content.contains("<tool_call>"));
        assert!(content.contains("Before"));
        assert!(content.contains("After"));
    }

    #[test]
    fn test_strip_no_tags_unchanged() {
        let mut content = "Normal text without any XML tags.".to_string();
        strip_residual_tool_call_tags(&mut content);
        assert_eq!(content, "Normal text without any XML tags.");
    }


    // -----------------------------------------------------------------------
    // Single-bar DSML tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_dsml_single_bar() {
        let content = r#"<｜DSML｜function_calls>
<｜DSML｜invoke name="get_quote">
<｜DSML｜parameter name="symbol" string="true">600519.SH</｜DSML｜parameter>
</｜DSML｜invoke>
</｜DSML｜function_calls>"#;
        let calls = parse_dsml_tool_calls(content).expect("should parse single-bar DSML");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_quote");
        assert_eq!(calls[0].arguments["symbol"], "600519.SH");
    }

    // -----------------------------------------------------------------------
    // parse_fn_tag_body tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_xml_fn_eq_basic() {
        let content = r#"<tool_call>
<function=get_account_state>
</function>
</tool_call>"#;
        let calls = parse_xml_tool_calls(content).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_account_state");
    }

    #[test]
    fn test_parse_xml_fn_eq_with_params() {
        let content = r#"<tool_call>
<function=evaluate_pullback>
<parameter=current_pct>0.015</parameter>
<parameter=days_range>5</parameter>
<parameter=drawdown_pct>0.105</parameter>
</function>
</tool_call>"#;
        let calls = parse_xml_tool_calls(content).expect("should parse");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "evaluate_pullback");
        assert_eq!(calls[0].arguments["current_pct"], 0.015);
        assert_eq!(calls[0].arguments["days_range"], 5);
    }

}
