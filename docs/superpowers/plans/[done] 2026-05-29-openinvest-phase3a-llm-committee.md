# Phase 3a: LLM 核心 + 委员会批处理 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Rust LLM client abstraction and committee orchestration engine so `run_committee(symbol)` produces a complete verdict from DeepSeek / MiMo Plan / MiMo API, with batch-mode output (streaming deferred to Phase 3b).

**Architecture:** A new `src-tauri/src/invest/llm/` module provides an OpenAI-compatible HTTP client with per-provider semaphore concurrency. A new `src-tauri/src/invest/committee/` module implements the 5-role multi-round debate algorithm (strictly reproducing openInvest's `core/committee.py`), convergence detection, SENTINEL override, and CIO Sanity Check. The orchestrator runs assets in parallel via `tokio::spawn` but serializes LLM calls within each asset. Verdicts are archived to `.committee/<date>/<symbol>.md` + `events.jsonl`.

**Tech Stack:** reqwest (existing, `json`+`stream` features), tokio (existing, `full`), serde/serde_json (existing), async-trait (existing), futures-util (existing), chrono (existing), uuid (existing). No new crate dependencies needed.

**Design references:**
- RFC: `docs/superpowers/plans/[done] 2026-05-29-committee-engineering-rfc.md` (D1-D11)
- Master plan: `docs/superpowers/plans/[wip] 2026-05-28-openinvest-investgui-port.md` (§11)

---

## File Structure

### New files — Rust backend

| File | Responsibility |
|------|---------------|
| `src-tauri/src/invest/mod.rs` | Module root, re-exports `llm` and `committee` submodules |
| `src-tauri/src/invest/llm/mod.rs` | Re-exports; `LlmConfig`, `ProviderId`, `Message`, `Usage` types |
| `src-tauri/src/invest/llm/types.rs` | `StreamChunk`, `ToolDef`, `ToolCall`, `LlmError`, `InvestLlmClient` trait |
| `src-tauri/src/invest/llm/client.rs` | `OpenAiCompatClient` — single reqwest-based implementation |
| `src-tauri/src/invest/llm/governor.rs` | `LlmGovernor` — per-provider `Semaphore(8)` concurrency |
| `src-tauri/src/invest/committee/mod.rs` | Module root, re-exports |
| `src-tauri/src/invest/committee/roles.rs` | `CommitteeRole` enum, role config, prompt loading from `~/.claw-go/invest/prompts/` |
| `src-tauri/src/invest/committee/parser.rs` | Parse LLM output into `ParsedFields` (SIGNAL, STRENGTH, CONCENTRATION_PCT, verdict, confidence) |
| `src-tauri/src/invest/committee/analysis.rs` | Convergence detection, SENTINEL override, CIO Sanity Check 3 Gates |
| `src-tauri/src/invest/committee/tools.rs` | 5 whitelisted tools for Macro role (get_history_data, analyze_multi_timeframe, get_macro_snapshot, query_dreaming_insights, get_recent_committee_verdicts) |
| `src-tauri/src/invest/committee/orchestrator.rs` | `run_committee_session()` — multi-round debate loop, asset-level concurrency |
| `src-tauri/src/invest/committee/archive.rs` | Verdict archiving to `.committee/<date>/<symbol>.md` + `events.jsonl` |

### Modified files — Rust backend

| File | Change |
|------|--------|
| `src-tauri/src/lib.rs` | Add `pub mod invest;` (top-level invest module), register new Tauri commands |
| `src-tauri/src/commands/invest.rs` | Add `run_committee`, `cancel_committee_run`, `get_llm_config`, `save_llm_config` commands |
| `src-tauri/src/models.rs` | Add `InvestLlmConfig` struct for provider settings persistence |

### New files — Frontend

| File | Responsibility |
|------|---------------|
| `src/lib/stores/invest-committee-store.svelte.ts` | Committee run state, cancel, batch result display |
| `src/lib/components/invest/ProviderConfigPanel.svelte` | 3-row manual input matrix (DeepSeek / MiMo Plan / MiMo API) |
| `src/lib/components/invest/RolePromptEditor.svelte` | 5 role prompt file editor |
| `src/lib/components/invest/CommitteeTab.svelte` | Basic batch-mode committee tab (start, results list) |

### Modified files — Frontend

| File | Change |
|------|--------|
| `src/routes/invest/+page.svelte` | Wire CommitteeTab into the `committee` tab slot |
| `messages/en.json` | Add i18n keys for committee UI |
| `messages/zh-CN.json` | Add i18n keys for committee UI |

---

## Task 1: LLM Types, Error, and Trait Definition

**Files:**
- Create: `src-tauri/src/invest/mod.rs`
- Create: `src-tauri/src/invest/llm/mod.rs`
- Create: `src-tauri/src/invest/llm/types.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod invest;`)

- [ ] **Step 1: Create `src-tauri/src/invest/mod.rs` module root**

```rust
pub mod llm;
pub mod committee;
```

- [ ] **Step 2: Create `src-tauri/src/invest/llm/mod.rs`**

```rust
pub mod client;
pub mod governor;
pub mod types;

pub use types::*;
```

- [ ] **Step 3: Create `src-tauri/src/invest/llm/types.rs`**

```rust
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
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".to_string(), content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".to_string(), content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".to_string(), content: content.into() }
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
    for attempt in 0..3 {
        match f().await {
            Ok(v) => return Ok(v),
            Err(LlmError::RateLimit { retry_after_ms }) => {
                let d = retry_after_ms
                    .map(std::time::Duration::from_millis)
                    .unwrap_or(delay);
                tokio::time::sleep(d).await;
                delay *= 2;
            }
            Err(LlmError::Timeout)
            | Err(LlmError::NetworkError(_))
            | Err(LlmError::ServerError(_)) => {
                log::warn!("LLM call attempt {} failed, retrying in {:?}", attempt + 1, delay);
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
            Err(e) => return Err(e), // 401/400 never retry
        }
    }
    Err(LlmError::Timeout)
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
                    let arguments = serde_json::from_str(&args).unwrap_or(serde_json::Value::Null);
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
```

- [ ] **Step 4: Add `pub mod invest;` to `src-tauri/src/lib.rs`**

Find the line near the top of `lib.rs` where other modules are declared (e.g., `pub mod commands;`, `pub mod storage;`). Add:

```rust
pub mod invest;
```

Place it after the existing module declarations (after `pub mod tushare;`).

- [ ] **Step 5: Verify compilation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30`

Expected: Compiles with possible warnings about unused imports, no errors.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/invest/mod.rs src-tauri/src/invest/llm/mod.rs src-tauri/src/invest/llm/types.rs src-tauri/src/lib.rs
git commit -m "feat(invest): add LLM types, error, and InvestLlmClient trait"
```

---

## Task 2: OpenAI-Compatible HTTP Client

**Files:**
- Create: `src-tauri/src/invest/llm/client.rs`
- Modify: `src-tauri/src/invest/llm/mod.rs` (already has `pub mod client;` from Task 1)

- [ ] **Step 1: Create `src-tauri/src/invest/llm/client.rs`**

```rust
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
            .timeout(Duration::from_secs(120)) // generous; per-request timeout via config
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

        // Build messages: system + user/assistant
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

        // Resolve API key: read from the provider's PlatformCredential
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

        // Stream the SSE response
        let byte_stream = resp.bytes_stream();
        let chunk_stream = byte_stream.filter_map(move |result| {
            let bytes = match result {
                Ok(b) => b,
                Err(e) => {
                    return futures_util::future::ready(Some(StreamChunk::Error {
                        message: format!("stream read: {}", e),
                    }))
                }
            };
            let text = String::from_utf8_lossy(&bytes);
            // Process each SSE line
            let mut chunks = Vec::new();
            for line in text.split('\n') {
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
                                            if let Some(id) = tc.id {
                                                if let Some(ref func) = tc.function {
                                                    if let Some(ref name) = func.name {
                                                        chunks.push(StreamChunk::ToolCallStart {
                                                            id: id.clone(),
                                                            name: name.clone(),
                                                        });
                                                    }
                                                }
                                            }
                                            if let Some(ref func) = tc.function {
                                                if let Some(ref args) = func.arguments {
                                                    if !args.is_empty() {
                                                        let id = tc
                                                            .id
                                                            .clone()
                                                            .unwrap_or_else(|| format!("tc_{}", tc.index));
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
                        Err(_) => {
                            // Skip unparseable lines (SSE comments, malformed data)
                        }
                    }
                }
            }
            if chunks.is_empty() {
                futures_util::future::ready(None)
            } else {
                futures_util::future::ready(Some(chunks.remove(0)))
            }
        });

        Ok(Box::pin(chunk_stream))
    }
}

// ---------------------------------------------------------------------------
// API key resolution from PlatformCredential (D6: reuse ClawGO config)
// ---------------------------------------------------------------------------

fn resolve_api_key(provider: &ProviderId) -> Result<String, LlmError> {
    // Read from settings file. The invest module stores its own config at
    // ~/.claw-go/invest/llm_config.json, separate from the main ClawGO settings.
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

    // Fallback: check environment variable
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
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30`

Expected: Compiles. If `dirs` crate is missing, add `dirs = "6"` to Cargo.toml dependencies.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/invest/llm/client.rs
git commit -m "feat(invest): add OpenAI-compatible LLM client with SSE streaming"
```

---

## Task 3: LLM Governor (Per-Provider Semaphore)

**Files:**
- Create: `src-tauri/src/invest/llm/governor.rs`

- [ ] **Step 1: Create `src-tauri/src/invest/llm/governor.rs`**

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::types::ProviderId;

/// Per-provider concurrency governor (RFC D8).
/// Each provider gets an independent Semaphore with 8 permits.
/// 5 assets × 3 roles = 15 concurrent requests capped to 8 per provider.
pub struct LlmGovernor {
    semaphores: HashMap<ProviderId, Arc<Semaphore>>,
}

impl LlmGovernor {
    pub fn new() -> Self {
        let mut semaphores = HashMap::new();
        for provider in ProviderId::all() {
            semaphores.insert(*provider, Arc::new(Semaphore::new(8)));
        }
        Self { semaphores }
    }

    /// Acquire a permit for the given provider. Blocks if all 8 permits are taken.
    pub async fn acquire(&self, provider: ProviderId) -> OwnedSemaphorePermit {
        self.semaphores[&provider]
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed unexpectedly")
    }

    /// Try to acquire without blocking. Returns None if no permits available.
    pub fn try_acquire(&self, provider: ProviderId) -> Option<OwnedSemaphorePermit> {
        self.semaphores[&provider]
            .clone()
            .try_acquire_owned()
            .ok()
    }
}

impl Default for LlmGovernor {
    fn default() -> Self {
        Self::new()
    }
}

// Global singleton — created once, shared across all committee runs.
use std::sync::OnceLock;

static GOVERNOR: OnceLock<LlmGovernor> = OnceLock::new();

pub fn global_governor() -> &'static LlmGovernor {
    GOVERNOR.get_or_init(LlmGovernor::new)
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -20`

Expected: Compiles clean.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/invest/llm/governor.rs
git commit -m "feat(invest): add LlmGovernor with per-provider Semaphore(8)"
```

---

## Task 4: Committee Roles, Prompt Loading, and Length Constraints

**Files:**
- Create: `src-tauri/src/invest/committee/mod.rs`
- Create: `src-tauri/src/invest/committee/roles.rs`

**Reference:** RFC §1.7 (D9: output length constraints), master plan §11.1 (D1)

- [ ] **Step 1: Create `src-tauri/src/invest/committee/mod.rs`**

```rust
pub mod analysis;
pub mod archive;
pub mod orchestrator;
pub mod parser;
pub mod roles;
pub mod tools;
```

- [ ] **Step 2: Create `src-tauri/src/invest/committee/roles.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Committee roles
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitteeRole {
    Macro,
    QuantR1,
    RiskR1,
    Wealth,
    QuantR2,
    RiskR2,
    Cio,
}

impl CommitteeRole {
    pub fn all() -> &'static [CommitteeRole] {
        &[
            Self::Macro,
            Self::QuantR1,
            Self::RiskR1,
            Self::Wealth,
            Self::QuantR2,
            Self::RiskR2,
            Self::Cio,
        ]
    }

    /// The execution order within a single asset's pipeline.
    pub fn execution_order() -> &'static [CommitteeRole] {
        Self::all()
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Macro => "Macro Strategist",
            Self::QuantR1 => "Quant Analyst (R1)",
            Self::RiskR1 => "Risk Officer (R1)",
            Self::Wealth => "Wealth Context",
            Self::QuantR2 => "Quant Analyst (R2)",
            Self::RiskR2 => "Risk Officer (R2)",
            Self::Cio => "CIO",
        }
    }

    /// Whether this role is a debate participant (R1/R2 rounds).
    pub fn is_debate(&self) -> bool {
        matches!(self, Self::QuantR1 | Self::RiskR1 | Self::QuantR2 | Self::RiskR2)
    }

    /// Whether this is a rebuttal round (R2).
    pub fn is_rebuttal(&self) -> bool {
        matches!(self, Self::QuantR2 | Self::RiskR2)
    }

    /// Whether this role uses the Macro prompt (has tools).
    pub fn has_tools(&self) -> bool {
        matches!(self, Self::Macro)
    }

    /// Prompt file name in `~/.claw-go/invest/prompts/`.
    pub fn prompt_filename(&self) -> &'static str {
        match self {
            Self::Macro => "macro.md",
            Self::QuantR1 => "quant.md",
            Self::RiskR1 => "risk.md",
            Self::Wealth => "wealth.md",
            Self::QuantR2 => "quant_rebuttal.md",
            Self::RiskR2 => "risk_rebuttal.md",
            Self::Cio => "cio.md",
        }
    }

    /// Max output length in Chinese characters (RFC D9).
    pub fn max_chars(&self, round: u8) -> usize {
        match self {
            Self::Macro => 400,          // first round data digest
            Self::Cio => 300,            // final verdict
            _ if round >= 2 => 200,      // rebuttal rounds
            _ => 200,                    // debate rounds
        }
    }

    /// Provider override for this role. CIO uses temperature=0.1.
    pub fn temperature(&self) -> f64 {
        match self {
            Self::Cio => 0.1,
            _ => 0.7,
        }
    }
}

impl std::fmt::Display for CommitteeRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ---------------------------------------------------------------------------
// Role configuration (persisted)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    pub role: CommitteeRole,
    pub provider: crate::invest::llm::ProviderId,
    pub model: String,
    pub temperature: f64,
    pub enabled: bool,
}

impl RoleConfig {
    pub fn defaults() -> Vec<RoleConfig> {
        CommitteeRole::all()
            .iter()
            .map(|role| RoleConfig {
                role: *role,
                provider: crate::invest::llm::ProviderId::DeepSeek,
                model: "deepseek-v4-pro".to_string(),
                temperature: role.temperature(),
                enabled: true,
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Prompt management
// ---------------------------------------------------------------------------

pub fn prompts_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".claw-go").join("invest").join("prompts")
}

/// Load the prompt for a role from disk. Falls back to built-in default if file missing.
pub fn load_prompt(role: CommitteeRole) -> String {
    let path = prompts_dir().join(role.prompt_filename());
    if path.exists() {
        std::fs::read_to_string(&path).unwrap_or_else(|e| {
            log::warn!("Failed to read prompt {}: {}", path.display(), e);
            default_prompt(role)
        })
    } else {
        default_prompt(role)
    }
}

/// Save a prompt to disk.
pub fn save_prompt(role: CommitteeRole, content: &str) -> Result<(), String> {
    let dir = prompts_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("create prompts dir: {}", e))?;
    let path = dir.join(role.prompt_filename());
    std::fs::write(&path, content)
        .map_err(|e| format!("write prompt {}: {}", path.display(), e))
}

/// Length constraint suffix appended to all prompts (RFC D9).
pub fn length_constraint_suffix(role: CommitteeRole, round: u8) -> String {
    let max = role.max_chars(round);
    format!(
        "\n\n【输出长度约束】\n不超过 {} 汉字。超出部分会被截断,要点请放在前部。",
        max
    )
}

/// Hard-truncate output to the role's max length. Returns (truncated_text, was_truncated).
pub fn hard_truncate(text: &str, role: CommitteeRole, round: u8) -> (String, bool) {
    let max = role.max_chars(round);
    let char_count = text.chars().count();
    if char_count <= max {
        (text.to_string(), false)
    } else {
        let truncated: String = text.chars().take(max).collect();
        log::warn!(
            "Output truncated for {} round {}: {} chars -> {} chars",
            role, round, char_count, max
        );
        (truncated, true)
    }
}

// ---------------------------------------------------------------------------
// Built-in default prompts (A-stock localized)
// ---------------------------------------------------------------------------

fn default_prompt(role: CommitteeRole) -> String {
    match role {
        CommitteeRole::Macro => MACRO_PROMPT.to_string(),
        CommitteeRole::QuantR1 => QUANT_PROMPT.to_string(),
        CommitteeRole::RiskR1 => RISK_PROMPT.to_string(),
        CommitteeRole::Wealth => WEALTH_PROMPT.to_string(),
        CommitteeRole::QuantR2 => QUANT_REBUTTAL_PROMPT.to_string(),
        CommitteeRole::RiskR2 => RISK_REBUTTAL_PROMPT.to_string(),
        CommitteeRole::Cio => CIO_PROMPT.to_string(),
    }
}

const MACRO_PROMPT: &str = r#"你是一位 A 股宏观策略分析师。你的职责是分析当前市场环境并输出宏观信号。

**输出格式要求**（严格遵守）:
1. 先用 2-3 句话概括当前市场数据
2. 最后一行必须是: SIGNAL: risk_on | risk_off | neutral
3. 如果是 risk_off,追加一行: STRENGTH: 1-10 (10=最强烈看空)

**可用工具**: 你可以调用以下工具获取实时数据:
- `get_history_data`: 获取股票历史行情
- `analyze_multi_timeframe`: 多时间框架技术分析
- `get_macro_snapshot`: 获取 A 股宏观指标快照
- `query_dreaming_insights`: 查询投资洞察
- `get_recent_committee_verdicts`: 查询近期裁决

**A 股宏观指标参考**:
- HV20/HV60 波动率比值 > 1.5 → 警戒
- 北向资金连续 5 日净流出 → 外资撤退信号
- 融资余额连续下降 → 杠杆资金退潮
- 涨停/跌停家数比 → 市场情绪广度
- 沪深 300 60 日分位数 → 估值水平

**Crash regime 判定**: 连续 5 日累计跌幅 > 8% 且跌停占比 > 5% → crash"#;

const QUANT_PROMPT: &str = r#"你是一位量化分析师,参与投资委员会第一轮辩论。

**你的任务**:
1. 分析 Macro 的 SIGNAL,给出你的量化观点
2. 基于技术指标(RSI/MACD/均线/成交量)评估当前价格位置
3. 如果 Macro 的 SIGNAL 是 risk_off,你可以 AGREE 或 CHALLENGE
4. 如果 Macro 的 SIGNAL 是 risk_on,你可以 AGREE 或提出反面证据

**输出格式**:
- 第一行: QUANT_VIEW: AGREE_with_Macro | CHALLENGE_Macro | NEUTRAL
- 接下来 2-3 句话说明理由
- 最后一行: STRENGTH: 1-10 (你对自己观点的强度)

**REGIME 约束**: 如果 Macro 判定 crash regime,你的技术分析必须优先考虑下行风险。"#;

const RISK_PROMPT: &str = r#"你是一位风险官,参与投资委员会第一轮辩论。

**你的任务**:
1. 评估当前持仓集中度风险
2. 评估现金储备充足性
3. 对 Macro 和 Quant 的观点进行风险视角的审视

**输出格式**:
- 第一行: RISK_VIEW: AGREE_with_consensus | CHALLENGE | OVERRIDE
- 接下来评估: CONCENTRATION_PCT (当前最大单一持仓占比)
- DRY_POWDER_CNY (可用现金)
- 最后一行: STRENGTH: 1-10

**关键约束**:
- CONCENTRATION_PCT > 30% → 必须 CHALLENGE 任何加仓建议
- DRY_POWDER_CNY < 应急储备金 → 必须 OVERRIDE 为 TRIM/SELL"#;

const WEALTH_PROMPT: &str = r#"你是一位财富背景顾问,负责从用户个人财务状况角度评估投资决策。

**你的输入**: 用户的 emergency_buffer_cny、family_backup_available、account_purpose、lifestyle_notes。

**你的任务**:
1. 评估当前投资决策是否与用户的财务目标一致
2. 评估用户的风险承受能力
3. 考虑 SOLVENCY_BUFFER_LEVEL (偿债缓冲水平)

**输出格式**:
- WEALTH_CONTEXT: FAVORABLE | NEUTRAL | CAUTIOUS
- 1-2 句话说明
- SOLVENCY_BUFFER_LEVEL: HIGH | MEDIUM | LOW | CRITICAL"#;

const QUANT_REBUTTAL_PROMPT: &str = r#"你是量化分析师,参与第二轮辩论(rebuttal)。

**你的输入**: 第一轮所有角色的输出 + Risk Officer 的 CONCENTRATION_PCT 和 DRY_POWDER_CNY。

**你的任务**:
1. 回应 Risk Officer 的挑战
2. 更新你的量化观点(可以上调或下调 STRENGTH)
3. REGIME 硬保护: 如果是 crash regime,你不能建议加仓

**输出格式**:
- QUANT_R2_VIEW: AGREE_with_Risk | MAINTAIN | UPGRADE
- 2-3 句话
- STRENGTH: 1-10 (可与 R1 不同)"#;

const RISK_REBUTTAL_PROMPT: &str = r#"你是风险官,参与第二轮辩论(rebuttal)。

**合法升级规则**（只能在这 2 条中选）:
1. 如果 Quant R2 提供了新证据改变你的风险评估 → 可以升级
2. 如果 Wealth Context 的 SOLVENCY_BUFFER_LEVEL 为 CRITICAL → 必须升级为 OVERRIDE

**输出格式**:
- RISK_R2_VIEW: MAINTAIN | UPGRADE_TO_OVERRIDE
- 1-2 句话
- STRENGTH: 1-10"#;

const CIO_PROMPT: &str = r#"你是首席投资官(CIO),负责做出最终裁决。

**你的输入**: 所有角色的完整输出(Macro + Quant R1/R2 + Risk R1/R2 + Wealth Context)。

**三道 Sanity Check**:
1. Gate 1 — 信号一致性: Macro SIGNAL 与 Quant/Risk 的共识是否一致?不一致 → HOLD
2. Gate 2 — 风险集中度: CONCENTRATION_PCT > 40%? → 裁决必须包含 TRIM
3. Gate 3 — 现金充足性: DRY_POWDER_CNY 低于应急储备? → 裁决降级为 HOLD,confidence ≤ 0.4

**输出格式**（严格遵守）:
- 第一行: VERDICT: BUY | ACCUMULATE | HOLD | TRIM | SELL
- 第二行: CONFIDENCE: 0.0-1.0
- 第三行: CONCENTRATION_PCT: X%
- 接下来 2-3 句话理由
- PERSONAL_NOTE: 一句话给用户的备注
- EXECUTION_PLAN: 执行计划(如果需要交易)
- RISK_PLAN: 风险控制计划

**温度**: 你的 temperature 设为 0.1,输出应稳定一致。"#;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30`

Expected: Compiles. The `committee/mod.rs` references modules not yet created (analysis, archive, orchestrator, parser, tools) — those will produce "file not found" errors. Create empty stub files:

Create `src-tauri/src/invest/committee/parser.rs`:
```rust
// Stub — implemented in Task 5
```

Create `src-tauri/src/invest/committee/analysis.rs`:
```rust
// Stub — implemented in Task 6
```

Create `src-tauri/src/invest/committee/tools.rs`:
```rust
// Stub — implemented in Task 7
```

Create `src-tauri/src/invest/committee/orchestrator.rs`:
```rust
// Stub — implemented in Task 8
```

Create `src-tauri/src/invest/committee/archive.rs`:
```rust
// Stub — implemented in Task 9
```

Then re-run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -20`

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/invest/committee/
git commit -m "feat(invest): add committee roles, prompt loading, and length constraints"
```

---

## Task 5: LLM Output Parser

**Files:**
- Modify: `src-tauri/src/invest/committee/parser.rs` (replace stub)

**Reference:** RFC §1.7 (D9), openInvest `core/committee.py` parser logic

- [ ] **Step 1: Implement `parser.rs`**

Replace the stub content of `src-tauri/src/invest/committee/parser.rs` with:

```rust
use super::roles::CommitteeRole;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Parsed fields from LLM output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedFields {
    /// Macro: "risk_on" | "risk_off" | "neutral"
    pub signal: Option<String>,
    /// Strength 1-10 (Macro, Quant, Risk)
    pub strength: Option<f64>,
    /// Risk Officer: current max concentration %
    pub concentration_pct: Option<f64>,
    /// Risk Officer: available dry powder in CNY
    pub dry_powder_cny: Option<f64>,
    /// Quant R1/R2: AGREE / CHALLENGE / NEUTRAL / MAINTAIN / UPGRADE
    pub quant_view: Option<String>,
    /// Risk R1/R2: AGREE / CHALLENGE / OVERRIDE / MAINTAIN / UPGRADE_TO_OVERRIDE
    pub risk_view: Option<String>,
    /// Wealth: FAVORABLE / NEUTRAL / CAUTIOUS
    pub wealth_context: Option<String>,
    /// Wealth: HIGH / MEDIUM / LOW / CRITICAL
    pub solvency_buffer_level: Option<String>,
    /// CIO: BUY / ACCUMULATE / HOLD / TRIM / SELL
    pub verdict: Option<String>,
    /// CIO: 0.0-1.0
    pub confidence: Option<f64>,
    /// CIO: personal note
    pub personal_note: Option<String>,
    /// CIO: execution plan
    pub execution_plan: Option<String>,
    /// CIO: risk plan
    pub risk_plan: Option<String>,
    /// Whether output was truncated by hard limit
    pub truncated: bool,
    /// Raw text (preserved for archiving)
    pub raw_text: String,
}

// ---------------------------------------------------------------------------
// Parser functions
// ---------------------------------------------------------------------------

/// Parse LLM output for any role into structured fields.
pub fn parse_role_output(role: CommitteeRole, text: &str, truncated: bool) -> ParsedFields {
    let mut parsed = ParsedFields {
        raw_text: text.to_string(),
        truncated,
        ..Default::default()
    };

    match role {
        CommitteeRole::Macro => parse_macro(text, &mut parsed),
        CommitteeRole::QuantR1 | CommitteeRole::QuantR2 => parse_quant(text, &mut parsed),
        CommitteeRole::RiskR1 | CommitteeRole::RiskR2 => parse_risk(text, &mut parsed),
        CommitteeRole::Wealth => parse_wealth(text, &mut parsed),
        CommitteeRole::Cio => parse_cio(text, &mut parsed),
    }

    parsed
}

fn extract_field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&format!("{}:", key)) {
            return Some(rest.trim().to_string());
        }
        if let Some(rest) = line.strip_prefix(&format!("{}：", key)) {
            // Chinese colon variant
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn extract_f64(text: &str, key: &str) -> Option<f64> {
    extract_field(text, key).and_then(|v| v.parse::<f64>().ok())
}

fn parse_macro(text: &str, parsed: &mut ParsedFields) {
    parsed.signal = extract_field(text, "SIGNAL").map(|s| {
        let s = s.to_lowercase();
        if s.contains("risk_on") || s.contains("risk on") {
            "risk_on".to_string()
        } else if s.contains("risk_off") || s.contains("risk off") {
            "risk_off".to_string()
        } else {
            "neutral".to_string()
        }
    });
    parsed.strength = extract_f64(text, "STRENGTH");
}

fn parse_quant(text: &str, parsed: &mut ParsedFields) {
    parsed.quant_view = extract_field(text, "QUANT_VIEW")
        .or_else(|| extract_field(text, "QUANT_R2_VIEW"));
    parsed.strength = extract_f64(text, "STRENGTH");
}

fn parse_risk(text: &str, parsed: &mut ParsedFields) {
    parsed.risk_view = extract_field(text, "RISK_VIEW")
        .or_else(|| extract_field(text, "RISK_R2_VIEW"));
    parsed.concentration_pct = extract_f64(text, "CONCENTRATION_PCT");
    parsed.dry_powder_cny = extract_f64(text, "DRY_POWDER_CNY");
    parsed.strength = extract_f64(text, "STRENGTH");
}

fn parse_wealth(text: &str, parsed: &mut ParsedFields) {
    parsed.wealth_context = extract_field(text, "WEALTH_CONTEXT");
    parsed.solvency_buffer_level = extract_field(text, "SOLVENCY_BUFFER_LEVEL");
}

fn parse_cio(text: &str, parsed: &mut ParsedFields) {
    parsed.verdict = extract_field(text, "VERDICT").map(|v| {
        let v = v.to_uppercase();
        // Normalize verdict strings
        if v.contains("BUY") { "BUY".to_string() }
        else if v.contains("ACCUMULATE") { "ACCUMULATE".to_string() }
        else if v.contains("HOLD") { "HOLD".to_string() }
        else if v.contains("TRIM") { "TRIM".to_string() }
        else if v.contains("SELL") { "SELL".to_string() }
        else { v }
    });
    parsed.confidence = extract_f64(text, "CONFIDENCE");
    parsed.concentration_pct = extract_f64(text, "CONCENTRATION_PCT");
    parsed.personal_note = extract_field(text, "PERSONAL_NOTE");
    parsed.execution_plan = extract_field(text, "EXECUTION_PLAN");
    parsed.risk_plan = extract_field(text, "RISK_PLAN");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_macro_risk_on() {
        let text = "当前市场处于上升趋势,沪深300 60日分位75%,北向资金持续流入。\n\nSIGNAL: risk_on";
        let parsed = parse_role_output(CommitteeRole::Macro, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("risk_on"));
    }

    #[test]
    fn test_parse_macro_risk_off_with_strength() {
        let text = "市场恐慌,连续5日跌幅超8%。\nSIGNAL: risk_off\nSTRENGTH: 8";
        let parsed = parse_role_output(CommitteeRole::Macro, text, false);
        assert_eq!(parsed.signal.as_deref(), Some("risk_off"));
        assert_eq!(parsed.strength, Some(8.0));
    }

    #[test]
    fn test_parse_quant_agree() {
        let text = "QUANT_VIEW: AGREE_with_Macro\n技术指标支持看多。\nSTRENGTH: 7";
        let parsed = parse_role_output(CommitteeRole::QuantR1, text, false);
        assert_eq!(parsed.quant_view.as_deref(), Some("AGREE_with_Macro"));
        assert_eq!(parsed.strength, Some(7.0));
    }

    #[test]
    fn test_parse_risk_with_concentration() {
        let text = "RISK_VIEW: CHALLENGE\nCONCENTRATION_PCT: 35\nDRY_POWDER_CNY: 50000\nSTRENGTH: 6";
        let parsed = parse_role_output(CommitteeRole::RiskR1, text, false);
        assert_eq!(parsed.risk_view.as_deref(), Some("CHALLENGE"));
        assert_eq!(parsed.concentration_pct, Some(35.0));
        assert_eq!(parsed.dry_powder_cny, Some(50000.0));
    }

    #[test]
    fn test_parse_cio_verdict() {
        let text = "VERDICT: HOLD\nCONFIDENCE: 0.6\nCONCENTRATION_PCT: 25\nPERSONAL_NOTE: 等待确认\nEXECUTION_PLAN: 无操作\nRISK_PLAN: 维持现有仓位";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.verdict.as_deref(), Some("HOLD"));
        assert_eq!(parsed.confidence, Some(0.6));
        assert_eq!(parsed.personal_note.as_deref(), Some("等待确认"));
    }

    #[test]
    fn test_parse_wealth() {
        let text = "WEALTH_CONTEXT: CAUTIOUS\nSOLVENCY_BUFFER_LEVEL: LOW";
        let parsed = parse_role_output(CommitteeRole::Wealth, text, false);
        assert_eq!(parsed.wealth_context.as_deref(), Some("CAUTIOUS"));
        assert_eq!(parsed.solvency_buffer_level.as_deref(), Some("LOW"));
    }

    #[test]
    fn test_parse_with_chinese_colon() {
        let text = "VERDICT：BUY\nCONFIDENCE：0.8";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.verdict.as_deref(), Some("BUY"));
        assert_eq!(parsed.confidence, Some(0.8));
    }

    #[test]
    fn test_parse_empty_text() {
        let parsed = parse_role_output(CommitteeRole::Macro, "", false);
        assert!(parsed.signal.is_none());
        assert!(parsed.strength.is_none());
    }

    #[test]
    fn test_truncated_flag() {
        let parsed = parse_role_output(CommitteeRole::Macro, "SIGNAL: risk_on", true);
        assert!(parsed.truncated);
    }

    #[test]
    fn test_hard_truncate_noop() {
        let short = "short text";
        let (result, was_truncated) = super::super::roles::hard_truncate(
            short, CommitteeRole::Macro, 1
        );
        assert_eq!(result, short);
        assert!(!was_truncated);
    }

    #[test]
    fn test_hard_truncate_actual() {
        let long = "这是一段超过200个汉字的测试文本".repeat(50);
        let (result, was_truncated) = super::super::roles::hard_truncate(
            &long, CommitteeRole::QuantR1, 1
        );
        assert!(was_truncated);
        assert!(result.chars().count() <= 200);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::parser::tests -- --nocapture 2>&1`

Expected: All 10 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/invest/committee/parser.rs
git commit -m "feat(invest): add committee output parser with field extraction and tests"
```

---

## Task 6: Convergence Detection, SENTINEL Override, CIO Sanity Check

**Files:**
- Modify: `src-tauri/src/invest/committee/analysis.rs` (replace stub)

**Reference:** RFC §1.7, openInvest `core/committee.py` — convergence detection, SENTINEL, CIO 3 Gates

- [ ] **Step 1: Implement `analysis.rs`**

Replace the stub content of `src-tauri/src/invest/committee/analysis.rs` with:

```rust
use super::parser::ParsedFields;
use super::roles::CommitteeRole;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Convergence detection
// ---------------------------------------------------------------------------

/// Check if Quant and Risk have converged over the last 2 rounds.
/// Convergence = same SIGNAL + strength difference < 1.0.
pub fn check_convergence(round_outputs: &[RoundOutput]) -> bool {
    if round_outputs.len() < 4 {
        return false; // need at least Q1, R1, Q2, R2
    }

    // Find the last 2 Quant and Risk outputs
    let quant_rounds: Vec<&RoundOutput> = round_outputs
        .iter()
        .filter(|o| matches!(o.role, CommitteeRole::QuantR1 | CommitteeRole::QuantR2))
        .collect();
    let risk_rounds: Vec<&RoundOutput> = round_outputs
        .iter()
        .filter(|o| matches!(o.role, CommitteeRole::RiskR1 | CommitteeRole::RiskR2))
        .collect();

    if quant_rounds.len() < 2 || risk_rounds.len() < 2 {
        return false;
    }

    let q1 = &quant_rounds[quant_rounds.len() - 2];
    let q2 = &quant_rounds[quant_rounds.len() - 1];
    let r1 = &risk_rounds[risk_rounds.len() - 2];
    let r2 = &risk_rounds[risk_rounds.len() - 1];

    // Check SIGNAL agreement across all 4
    let signals: Vec<Option<&str>> = [q1, q2, r1, r2]
        .iter()
        .map(|o| o.parsed.signal.as_deref().or(o.parsed.quant_view.as_deref()))
        .collect();

    // All must agree on direction (simplified: check if quant_view/risk_view are consistent)
    let q_views_match = q1.parsed.quant_view == q2.parsed.quant_view;
    let r_views_match = r1.parsed.risk_view == r2.parsed.risk_view;

    // Strength difference < 1.0
    let q_strength_diff = (q1.parsed.strength.unwrap_or(5.0) - q2.parsed.strength.unwrap_or(5.0)).abs();
    let r_strength_diff = (r1.parsed.strength.unwrap_or(5.0) - r2.parsed.strength.unwrap_or(5.0)).abs();

    q_views_match && r_views_match && q_strength_diff < 1.0 && r_strength_diff < 1.0
}

// ---------------------------------------------------------------------------
// SENTINEL override
// ---------------------------------------------------------------------------

/// Check if SENTINEL should override the CIO verdict.
/// Triggers when CONCENTRATION_PCT difference between Risk R1 and Risk R2 > 0.3%.
pub fn check_sentinel(round_outputs: &[RoundOutput]) -> Option<SentinelOverride> {
    let risk_outputs: Vec<&RoundOutput> = round_outputs
        .iter()
        .filter(|o| matches!(o.role, CommitteeRole::RiskR1 | CommitteeRole::RiskR2))
        .collect();

    if risk_outputs.len() < 2 {
        return None;
    }

    let r1 = &risk_outputs[0];
    let r2 = &risk_outputs[risk_outputs.len() - 1];

    let r1_pct = r1.parsed.concentration_pct.unwrap_or(0.0);
    let r2_pct = r2.parsed.concentration_pct.unwrap_or(0.0);
    let diff = (r2_pct - r1_pct).abs();

    if diff > 0.3 {
        Some(SentinelOverride {
            reason: format!(
                "SENTINEL: CONCENTRATION_PCT shifted by {:.1}% (R1={:.1}% → R2={:.1}%)",
                diff, r1_pct, r2_pct
            ),
            forced_verdict: "TRIM".to_string(),
            forced_confidence: 0.3,
        })
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentinelOverride {
    pub reason: String,
    pub forced_verdict: String,
    pub forced_confidence: f64,
}

// ---------------------------------------------------------------------------
// CIO Sanity Check — 3 Gates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanityCheckResult {
    pub gate1_pass: bool,  // signal consistency
    pub gate2_pass: bool,  // concentration < 40%
    pub gate3_pass: bool,  // dry powder sufficient
    pub final_verdict: String,
    pub final_confidence: f64,
    pub notes: Vec<String>,
}

/// Run CIO Sanity Check 3 Gates on the parsed CIO output.
pub fn cio_sanity_check(
    cio_parsed: &ParsedFields,
    round_outputs: &[RoundOutput],
    macro_signal: &str,
    emergency_buffer_cny: f64,
) -> SanityCheckResult {
    let mut result = SanityCheckResult {
        gate1_pass: true,
        gate2_pass: true,
        gate3_pass: true,
        final_verdict: cio_parsed.verdict.clone().unwrap_or_else(|| "HOLD".to_string()),
        final_confidence: cio_parsed.confidence.unwrap_or(0.5),
        notes: Vec::new(),
    };

    // Gate 1 — Signal consistency
    // Check if Macro SIGNAL agrees with CIO verdict direction
    let macro_is_bullish = macro_signal == "risk_on";
    let cio_is_bullish = matches!(
        result.final_verdict.as_str(),
        "BUY" | "ACCUMULATE"
    );
    let cio_is_bearish = matches!(
        result.final_verdict.as_str(),
        "TRIM" | "SELL"
    );

    if (macro_is_bullish && cio_is_bearish) || (!macro_is_bullish && cio_is_bullish) {
        // Macro and CIO disagree — check if Quant/Risk override
        let has_override = round_outputs.iter().any(|o| {
            o.parsed.risk_view.as_deref() == Some("OVERRIDE")
                || o.parsed.risk_view.as_deref() == Some("UPGRADE_TO_OVERRIDE")
        });
        if !has_override {
            result.gate1_pass = false;
            result.final_verdict = "HOLD".to_string();
            result.notes.push("Gate 1: signal inconsistency without override".to_string());
        }
    }

    // Gate 2 — Concentration > 40%
    let concentration = cio_parsed.concentration_pct.unwrap_or(
        round_outputs
            .iter()
            .filter_map(|o| o.parsed.concentration_pct)
            .last()
            .unwrap_or(0.0),
    );
    if concentration > 40.0 {
        result.gate2_pass = false;
        if !matches!(result.final_verdict.as_str(), "TRIM" | "SELL") {
            result.final_verdict = "TRIM".to_string();
            result.notes.push(format!(
                "Gate 2: concentration {:.1}% > 40%, forced to TRIM",
                concentration
            ));
        }
    }

    // Gate 3 — Dry powder check
    let dry_powder = cio_parsed.dry_powder_cny.unwrap_or(
        round_outputs
            .iter()
            .filter_map(|o| o.parsed.dry_powder_cny)
            .last()
            .unwrap_or(0.0),
    );
    if dry_powder < emergency_buffer_cny {
        result.gate3_pass = false;
        result.final_verdict = "HOLD".to_string();
        result.final_confidence = result.final_confidence.min(0.4);
        result.notes.push(format!(
            "Gate 3: dry powder {:.0} < emergency buffer {:.0}, downgraded to HOLD",
            dry_powder, emergency_buffer_cny
        ));
    }

    // Check for WORKER_UNAVAILABLE (retry exhaustion)
    let has_unavailable = round_outputs.iter().any(|o| {
        o.parsed.raw_text.contains("[WORKER_UNAVAILABLE]")
    });
    if has_unavailable {
        result.final_verdict = "HOLD".to_string();
        result.final_confidence = result.final_confidence.min(0.4);
        result.notes.push("Worker unavailable, degraded to HOLD".to_string());
    }

    result
}

// ---------------------------------------------------------------------------
// Round output — accumulated per-role result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RoundOutput {
    pub role: CommitteeRole,
    pub round: u8,
    pub parsed: ParsedFields,
    pub latency_ms: u64,
    pub tokens_used: u32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_output(
        role: CommitteeRole,
        round: u8,
        signal: Option<&str>,
        strength: Option<f64>,
        concentration: Option<f64>,
        dry_powder: Option<f64>,
        view: Option<&str>,
    ) -> RoundOutput {
        let mut parsed = ParsedFields::default();
        parsed.signal = signal.map(|s| s.to_string());
        parsed.strength = strength;
        parsed.concentration_pct = concentration;
        parsed.dry_powder_cny = dry_powder;
        match role {
            CommitteeRole::QuantR1 | CommitteeRole::QuantR2 => {
                parsed.quant_view = view.map(|s| s.to_string());
            }
            CommitteeRole::RiskR1 | CommitteeRole::RiskR2 => {
                parsed.risk_view = view.map(|s| s.to_string());
            }
            _ => {}
        }
        parsed.raw_text = format!("test output for {:?}", role);
        RoundOutput { role, round, parsed, latency_ms: 100, tokens_used: 200 }
    }

    #[test]
    fn test_convergence_detected() {
        let outputs = vec![
            make_output(CommitteeRole::QuantR1, 1, None, Some(7.0), None, None, Some("AGREE")),
            make_output(CommitteeRole::RiskR1, 1, None, Some(6.0), None, None, Some("AGREE")),
            make_output(CommitteeRole::QuantR2, 2, None, Some(7.0), None, None, Some("AGREE")),
            make_output(CommitteeRole::RiskR2, 2, None, Some(6.5), None, None, Some("AGREE")),
        ];
        assert!(check_convergence(&outputs));
    }

    #[test]
    fn test_convergence_not_detected_different_views() {
        let outputs = vec![
            make_output(CommitteeRole::QuantR1, 1, None, Some(7.0), None, None, Some("AGREE")),
            make_output(CommitteeRole::RiskR1, 1, None, Some(6.0), None, None, Some("CHALLENGE")),
            make_output(CommitteeRole::QuantR2, 2, None, Some(7.0), None, None, Some("AGREE")),
            make_output(CommitteeRole::RiskR2, 2, None, Some(6.0), None, None, Some("CHALLENGE")),
        ];
        assert!(!check_convergence(&outputs));
    }

    #[test]
    fn test_convergence_not_detected_strength_drift() {
        let outputs = vec![
            make_output(CommitteeRole::QuantR1, 1, None, Some(3.0), None, None, Some("AGREE")),
            make_output(CommitteeRole::RiskR1, 1, None, Some(6.0), None, None, Some("AGREE")),
            make_output(CommitteeRole::QuantR2, 2, None, Some(8.0), None, None, Some("AGREE")),
            make_output(CommitteeRole::RiskR2, 2, None, Some(6.0), None, None, Some("AGREE")),
        ];
        assert!(!check_convergence(&outputs));
    }

    #[test]
    fn test_sentinel_triggers_on_large_shift() {
        let outputs = vec![
            make_output(CommitteeRole::RiskR1, 1, None, None, Some(20.0), None, None),
            make_output(CommitteeRole::RiskR2, 2, None, None, Some(35.0), None, None),
        ];
        let sentinel = check_sentinel(&outputs);
        assert!(sentinel.is_some());
        let s = sentinel.unwrap();
        assert_eq!(s.forced_verdict, "TRIM");
    }

    #[test]
    fn test_sentinel_no_trigger_small_shift() {
        let outputs = vec![
            make_output(CommitteeRole::RiskR1, 1, None, None, Some(20.0), None, None),
            make_output(CommitteeRole::RiskR2, 2, None, None, Some(20.2), None, None),
        ];
        assert!(check_sentinel(&outputs).is_none());
    }

    #[test]
    fn test_sanity_gate1_inconsistency() {
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.7),
            ..Default::default()
        };
        let outputs = vec![];
        let result = cio_sanity_check(&cio, &outputs, "risk_off", 100000.0);
        assert!(!result.gate1_pass);
        assert_eq!(result.final_verdict, "HOLD");
    }

    #[test]
    fn test_sanity_gate2_high_concentration() {
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.7),
            concentration_pct: Some(45.0),
            ..Default::default()
        };
        let outputs = vec![];
        let result = cio_sanity_check(&cio, &outputs, "risk_on", 100000.0);
        assert!(!result.gate2_pass);
        assert_eq!(result.final_verdict, "TRIM");
    }

    #[test]
    fn test_sanity_gate3_low_dry_powder() {
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.7),
            dry_powder_cny: Some(50000.0),
            ..Default::default()
        };
        let outputs = vec![];
        let result = cio_sanity_check(&cio, &outputs, "risk_on", 100000.0);
        assert!(!result.gate3_pass);
        assert_eq!(result.final_verdict, "HOLD");
        assert!(result.final_confidence <= 0.4);
    }

    #[test]
    fn test_sanity_worker_unavailable() {
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.8),
            ..Default::default()
        };
        let outputs = vec![RoundOutput {
            role: CommitteeRole::QuantR1,
            round: 1,
            parsed: ParsedFields {
                raw_text: "[WORKER_UNAVAILABLE]".to_string(),
                ..Default::default()
            },
            latency_ms: 0,
            tokens_used: 0,
        }];
        let result = cio_sanity_check(&cio, &outputs, "risk_on", 100000.0);
        assert_eq!(result.final_verdict, "HOLD");
        assert!(result.final_confidence <= 0.4);
    }

    #[test]
    fn test_sanity_all_gates_pass() {
        let cio = ParsedFields {
            verdict: Some("ACCUMULATE".to_string()),
            confidence: Some(0.7),
            concentration_pct: Some(20.0),
            dry_powder_cny: Some(200000.0),
            ..Default::default()
        };
        let outputs = vec![];
        let result = cio_sanity_check(&cio, &outputs, "risk_on", 100000.0);
        assert!(result.gate1_pass);
        assert!(result.gate2_pass);
        assert!(result.gate3_pass);
        assert_eq!(result.final_verdict, "ACCUMULATE");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::analysis::tests -- --nocapture 2>&1`

Expected: All 10 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/invest/committee/analysis.rs
git commit -m "feat(invest): add convergence detection, SENTINEL, and CIO Sanity Check"
```

---

## Task 7: Macro Role Whitelisted Tools

**Files:**
- Modify: `src-tauri/src/invest/committee/tools.rs` (replace stub)

**Reference:** RFC §1.2 (5 whitelist tools), master plan §11.3

- [ ] **Step 1: Implement `tools.rs`**

Replace the stub content of `src-tauri/src/invest/committee/tools.rs` with:

```rust
use crate::invest::llm::{ToolDef, Message};
use crate::tushare::client::TushareClient;
use serde_json::json;

// ---------------------------------------------------------------------------
// Tool definitions (OpenAI-compatible JSON Schema)
// ---------------------------------------------------------------------------

/// All 5 whitelisted tools for the Macro role.
pub fn macro_tool_defs() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_history_data".to_string(),
            description: "获取指定股票的历史行情数据（日线）".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "股票代码，如 600519.SH"
                    },
                    "days": {
                        "type": "integer",
                        "description": "获取最近N个交易日的数据，默认60"
                    }
                },
                "required": ["symbol"]
            }),
        },
        ToolDef {
            name: "analyze_multi_timeframe".to_string(),
            description: "对股票进行多时间框架技术分析（5日/20日/60日）".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "股票代码"
                    }
                },
                "required": ["symbol"]
            }),
        },
        ToolDef {
            name: "get_macro_snapshot".to_string(),
            description: "获取A股宏观指标快照：沪深300波动率、北向资金、融资余额、涨跌停广度".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDef {
            name: "query_dreaming_insights".to_string(),
            description: "查询投资洞察和历史裁决".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "查询关键词"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "返回条数，默认10"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "get_recent_committee_verdicts".to_string(),
            description: "获取近期委员会裁决记录".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "股票代码（可选）"
                    },
                    "days": {
                        "type": "integer",
                        "description": "最近N天，默认7"
                    }
                }
            }),
        },
    ]
}

// ---------------------------------------------------------------------------
// Tool execution
// ---------------------------------------------------------------------------

/// Execute a tool call and return the result as a string.
pub async fn execute_tool(
    tool_name: &str,
    arguments: &str,
    symbol: &str,
) -> Result<String, String> {
    let args: serde_json::Value = serde_json::from_str(arguments)
        .unwrap_or_else(|_| json!({}));

    match tool_name {
        "get_history_data" => {
            let sym = args["symbol"].as_str().unwrap_or(symbol);
            let days = args["days"].as_i64().unwrap_or(60) as usize;
            exec_history_data(sym, days).await
        }
        "analyze_multi_timeframe" => {
            let sym = args["symbol"].as_str().unwrap_or(symbol);
            exec_multi_timeframe(sym).await
        }
        "get_macro_snapshot" => {
            exec_macro_snapshot().await
        }
        "query_dreaming_insights" => {
            let query = args["query"].as_str().unwrap_or("");
            let limit = args["limit"].as_i64().unwrap_or(10) as usize;
            exec_dreaming_insights(query, limit)
        }
        "get_recent_committee_verdicts" => {
            let sym = args["symbol"].as_str();
            let days = args["days"].as_i64().unwrap_or(7);
            exec_recent_verdicts(sym, days)
        }
        _ => Err(format!("unknown tool: {}", tool_name)),
    }
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

async fn exec_history_data(symbol: &str, days: usize) -> Result<String, String> {
    let token = read_tushare_token()?;
    let client = TushareClient::new(token);

    let end_date = chrono::Local::now().format("%Y%m%d").to_string();
    let start_date = (chrono::Local::now() - chrono::Duration::days(days as i64 * 2))
        .format("%Y%m%d")
        .to_string();

    let bars = client.daily(symbol, &start_date, &end_date).await?;
    let bars: Vec<_> = bars.into_iter().rev().take(days).collect();

    if bars.is_empty() {
        return Ok(format!("没有找到 {} 的历史数据", symbol));
    }

    let latest = &bars[0];
    let oldest = &bars[bars.len() - 1];
    let pct_change = if oldest.close > 0.0 {
        (latest.close - oldest.close) / oldest.close * 100.0
    } else {
        0.0
    };

    let avg_vol: f64 = bars.iter().map(|b| b.vol).sum::<f64>() / bars.len() as f64;

    Ok(format!(
        "【{} 历史行情（最近{}个交易日）】\n\
         最新收盘: {:.2} ({})\n\
         区间涨跌: {:.2}%\n\
         最高: {:.2} / 最低: {:.2}\n\
         平均成交量: {:.0} 手\n\
         近5日K线: {}",
        symbol,
        bars.len(),
        latest.close,
        latest.trade_date,
        pct_change,
        bars.iter().map(|b| b.high).fold(f64::NEG_INFINITY, f64::max),
        bars.iter().map(|b| b.low).fold(f64::INFINITY, f64::min),
        avg_vol,
        bars.iter().take(5).map(|b| format!("{}:{:.2}", &b.trade_date[4..], b.close)).collect::<Vec<_>>().join(" → ")
    ))
}

async fn exec_multi_timeframe(symbol: &str) -> Result<String, String> {
    let token = read_tushare_token()?;
    let client = TushareClient::new(token);

    let end_date = chrono::Local::now().format("%Y%m%d").to_string();
    let start_date = (chrono::Local::now() - chrono::Duration::days(180))
        .format("%Y%m%d")
        .to_string();

    let bars = client.daily(symbol, &start_date, &end_date).await?;
    if bars.len() < 5 {
        return Ok(format!("{} 数据不足，无法进行多时间框架分析", symbol));
    }

    let ma5: f64 = bars.iter().take(5).map(|b| b.close).sum::<f64>() / 5.0;
    let ma20 = if bars.len() >= 20 {
        bars.iter().take(20).map(|b| b.close).sum::<f64>() / 20.0
    } else {
        bars.iter().map(|b| b.close).sum::<f64>() / bars.len() as f64
    };
    let ma60 = if bars.len() >= 60 {
        bars.iter().take(60).map(|b| b.close).sum::<f64>() / 60.0
    } else {
        bars.iter().map(|b| b.close).sum::<f64>() / bars.len() as f64
    };

    let latest_close = bars[0].close;

    let returns: Vec<f64> = bars.windows(2).take(20).map(|w| {
        if w[1].close > 0.0 { (w[0].close - w[1].close) / w[1].close } else { 0.0 }
    }).collect();
    let mean_ret = returns.iter().sum::<f64>() / returns.len().max(1) as f64;
    let variance = returns.iter().map(|r| (r - mean_ret).powi(2)).sum::<f64>() / returns.len().max(1) as f64;
    let hv20 = variance.sqrt() * (252.0_f64).sqrt() * 100.0;

    Ok(format!(
        "【{} 多时间框架分析】\n\
         MA5: {:.2} | MA20: {:.2} | MA60: {:.2}\n\
         价格 vs MA20: {}\n\
         HV20(年化): {:.1}%\n\
         趋势判断: {}",
        symbol,
        ma5, ma20, ma60,
        if latest_close > ma20 { "上方（偏多）" } else { "下方（偏空）" },
        hv20,
        if latest_close > ma5 && ma5 > ma20 && ma20 > ma60 {
            "多头排列"
        } else if latest_close < ma5 && ma5 < ma20 && ma20 < ma60 {
            "空头排列"
        } else {
            "震荡整理"
        }
    ))
}

async fn exec_macro_snapshot() -> Result<String, String> {
    let token = read_tushare_token()?;
    let client = TushareClient::new(token);

    let end_date = chrono::Local::now().format("%Y%m%d").to_string();
    let start_date = (chrono::Local::now() - chrono::Duration::days(90))
        .format("%Y%m%d")
        .to_string();

    let idx_bars = client.daily("000300.SH", &start_date, &end_date).await.unwrap_or_default();

    let csi300_latest = idx_bars.first().map(|b| b.close).unwrap_or(0.0);
    let csi300_pct = idx_bars.first().map(|b| b.pct_chg).unwrap_or(0.0);

    let csi300_60d: Vec<f64> = idx_bars.iter().take(60).map(|b| b.close).collect();
    let csi300_pctile = if !csi300_60d.is_empty() {
        let sorted = {
            let mut s = csi300_60d.clone();
            s.sort_by(|a, b| a.partial_cmp(b).unwrap());
            s
        };
        let rank = sorted.iter().position(|&v| v >= csi300_latest).unwrap_or(0);
        rank as f64 / sorted.len() as f64 * 100.0
    } else {
        50.0
    };

    Ok(format!(
        "【A股宏观指标快照】\n\
         沪深300: {:.2} (今日 {:.2}%)\n\
         沪深300 60日分位: {:.0}%\n\
         北向资金: 需通过 moneyflow_hsgt 接口获取\n\
         融资余额: 需通过 margin 接口获取\n\
         涨跌停广度: 需通过 limit_list_d 接口获取",
        csi300_latest, csi300_pct, csi300_pctile
    ))
}

fn exec_dreaming_insights(query: &str, limit: usize) -> Result<String, String> {
    use crate::storage::invest::with_conn;

    let results: Vec<String> = with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT content, created_at FROM domain_insights WHERE content LIKE ?1 ORDER BY created_at DESC LIMIT ?2"
        ).map_err(|e| format!("prepare: {}", e))?;

        let pattern = format!("%{}%", query);
        let rows = stmt.query_map(rusqlite::params![pattern, limit as i64], |row| {
            let content: String = row.get(0)?;
            let created: String = row.get(1)?;
            Ok(format!("[{}] {}", &created[..10], content))
        }).map_err(|e| format!("query: {}", e))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })?;

    if results.is_empty() {
        Ok(format!("没有找到与 '{}' 相关的投资洞察", query))
    } else {
        Ok(format!("【投资洞察 - {}】\n{}", query, results.join("\n")))
    }
}

fn exec_recent_verdicts(symbol: Option<&str>, days: i64) -> Result<String, String> {
    use crate::storage::invest::verdicts::list_verdicts;

    let verdicts = list_verdicts(symbol, Some(days * 5))?;

    let cutoff = (chrono::Local::now() - chrono::Duration::days(days))
        .format("%Y-%m-%d")
        .to_string();

    let recent: Vec<String> = verdicts
        .iter()
        .filter(|v| v.created_at >= cutoff)
        .take(10)
        .map(|v| {
            format!(
                "[{}] {} → {} (conf={:.1}, signal={}, {}ms)",
                &v.created_at[..10],
                v.symbol,
                v.verdict,
                v.confidence.unwrap_or(0.0),
                v.macro_signal.as_deref().unwrap_or("?"),
                v.latency_ms.unwrap_or(0)
            )
        })
        .collect();

    if recent.is_empty() {
        Ok(format!("最近{}天没有裁决记录", days))
    } else {
        Ok(format!("【近{}天裁决记录】\n{}", days, recent.join("\n")))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_tushare_token() -> Result<String, String> {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let settings_path = home.join(".claw-go").join("settings.json");

    if !settings_path.exists() {
        return Err("ClawGO settings not found".to_string());
    }

    let content = std::fs::read_to_string(&settings_path)
        .map_err(|e| format!("read settings: {}", e))?;
    let settings: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("parse settings: {}", e))?;

    settings["tushare_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "tushare_token not found in settings".to_string())
}

pub fn tool_result_message(tool_call_id: &str, result: &str) -> Message {
    Message {
        role: "tool".to_string(),
        content: result.to_string(),
        tool_call_id: Some(tool_call_id.to_string()),
        tool_calls: None,
        name: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macro_tool_defs_count() {
        assert_eq!(macro_tool_defs().len(), 5);
    }

    #[test]
    fn test_macro_tool_names() {
        let names: Vec<&str> = macro_tool_defs().iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"get_history_data"));
        assert!(names.contains(&"analyze_multi_timeframe"));
        assert!(names.contains(&"get_macro_snapshot"));
        assert!(names.contains(&"query_dreaming_insights"));
        assert!(names.contains(&"get_recent_committee_verdicts"));
    }

    #[test]
    fn test_tool_result_message_format() {
        let msg = tool_result_message("call_123", "test result");
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.tool_call_id.as_deref(), Some("call_123"));
        assert_eq!(msg.content, "test result");
    }

    #[tokio::test]
    async fn test_execute_unknown_tool() {
        let result = execute_tool("nonexistent", "{}", "600519.SH").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown tool"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::tools::tests -- --nocapture 2>&1`

Expected: All 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/invest/committee/tools.rs
git commit -m "feat(invest): add 5 whitelisted Macro tools with Tushare integration"
```

---

## Task 8: Committee Orchestrator

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs` (replace stub)

**Reference:** RFC §1.7 (orchestration flow), §11.2 (asset-level concurrency), D7 (debate rounds)

- [ ] **Step 1: Implement `orchestrator.rs`**

Replace the stub content of `src-tauri/src/invest/committee/orchestrator.rs` with:

```rust
use super::analysis::{check_convergence, check_sentinel, cio_sanity_check, RoundOutput, SanityCheckResult, SentinelOverride};
use super::archive::archive_decision;
use super::parser::{parse_role_output, ParsedFields};
use super::roles::{CommitteeRole, RoleConfig, load_prompt, length_constraint_suffix, hard_truncate};
use super::tools::{execute_tool, macro_tool_defs, tool_result_message};
use crate::invest::llm::{InvestLlmClient, LlmGovernor, Message, ProviderId, call_with_retry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Committee configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitteeConfig {
    pub debate_rounds: u8,
    pub roles: Vec<RoleConfig>,
    pub emergency_buffer_cny: f64,
    pub timeout_secs: u64,
}

impl Default for CommitteeConfig {
    fn default() -> Self {
        Self {
            debate_rounds: 4,
            roles: RoleConfig::defaults(),
            emergency_buffer_cny: 100000.0,
            timeout_secs: 120,
        }
    }
}

// ---------------------------------------------------------------------------
// Committee result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitteeResult {
    pub symbol: String,
    pub verdict: String,
    pub confidence: f64,
    pub macro_signal: String,
    pub macro_strength: f64,
    pub reasoning: String,
    pub round_outputs: Vec<RoundOutputSummary>,
    pub sanity_check: SanityCheckResult,
    pub sentinel_override: Option<SentinelOverride>,
    pub convergence_detected: bool,
    pub total_latency_ms: u64,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundOutputSummary {
    pub role: String,
    pub round: u8,
    pub output: String,
    pub latency_ms: u64,
    pub tokens_used: u32,
}

// ---------------------------------------------------------------------------
// Run committee for a single symbol
// ---------------------------------------------------------------------------

pub async fn run_committee(
    symbol: &str,
    clients: &HashMap<ProviderId, Arc<dyn InvestLlmClient>>,
    config: &CommitteeConfig,
) -> Result<CommitteeResult, String> {
    let start = std::time::Instant::now();
    let governor = LlmGovernor::global();

    let mut round_outputs: Vec<RoundOutput> = Vec::new();
    let mut messages: Vec<Message> = Vec::new();
    let mut total_tokens: u32 = 0;

    // --- Phase 1: Macro (with tools) ---
    let macro_config = config.roles.iter().find(|r| r.role == CommitteeRole::Macro)
        .ok_or("Macro role config missing")?;
    let macro_client = clients.get(&macro_config.provider)
        .ok_or("Macro provider client not found")?;

    let macro_prompt = load_prompt(CommitteeRole::Macro)
        + &length_constraint_suffix(CommitteeRole::Macro, 1);
    messages.push(Message {
        role: "system".to_string(),
        content: macro_prompt,
        tool_call_id: None,
        tool_calls: None,
        name: None,
    });
    messages.push(Message {
        role: "user".to_string(),
        content: format!("请分析 {} 的当前市场环境和宏观信号。", symbol),
        tool_call_id: None,
        tool_calls: None,
        name: None,
    });

    let macro_tools = macro_tool_defs();
    let permit = governor.acquire(ProviderId::DeepSeek).await;

    let macro_response = call_with_retry(|| async {
        macro_client
            .chat_completion(&messages, Some(&macro_tools), macro_config.temperature)
            .await
    })
    .await
    .map_err(|e| format!("Macro LLM call failed: {}", e))?;

    drop(permit);

    // Handle tool calls
    let macro_text = if let Some(tool_calls) = &macro_response.tool_calls {
        let mut tool_messages = messages.clone();
        tool_messages.push(Message {
            role: "assistant".to_string(),
            content: macro_response.text.clone(),
            tool_call_id: None,
            tool_calls: Some(tool_calls.clone()),
            name: None,
        });

        for tc in tool_calls {
            let result = execute_tool(&tc.function.name, &tc.function.arguments, symbol)
                .await
                .unwrap_or_else(|e| format!("Tool error: {}", e));
            tool_messages.push(tool_result_message(&tc.id, &result));
        }

        let permit2 = governor.acquire(ProviderId::DeepSeek).await;
        let final_response = call_with_retry(|| async {
            macro_client
                .chat_completion(&tool_messages, Some(&macro_tools), macro_config.temperature)
                .await
        })
        .await
        .map_err(|e| format!("Macro final call failed: {}", e))?;
        drop(permit2);

        total_tokens += macro_response.usage.total_tokens + final_response.usage.total_tokens;
        final_response.text
    } else {
        total_tokens += macro_response.usage.total_tokens;
        macro_response.text
    };

    let (macro_text, macro_truncated) = hard_truncate(&macro_text, CommitteeRole::Macro, 1);
    let macro_parsed = parse_role_output(CommitteeRole::Macro, &macro_text, macro_truncated);
    round_outputs.push(RoundOutput {
        role: CommitteeRole::Macro,
        round: 1,
        parsed: macro_parsed.clone(),
        latency_ms: start.elapsed().as_millis() as u64,
        tokens_used: total_tokens,
    });

    let macro_signal = macro_parsed.signal.clone().unwrap_or_else(|| "neutral".to_string());

    // --- Phase 2: Debate rounds ---
    let debate_count = config.debate_rounds.min(8);

    for round_idx in 0..debate_count {
        let round_num = round_idx + 1;
        let is_rebuttal = round_idx >= 1;

        // Quant
        let quant_role = if is_rebuttal { CommitteeRole::QuantR2 } else { CommitteeRole::QuantR1 };
        let quant_config = config.roles.iter().find(|r| r.role == quant_role)
            .ok_or("Quant role config missing")?;
        let quant_client = clients.get(&quant_config.provider)
            .ok_or("Quant provider client not found")?;

        let quant_prompt = load_prompt(quant_role)
            + &length_constraint_suffix(quant_role, round_num);
        let mut quant_messages = build_context_messages(
            &quant_prompt, symbol, &round_outputs, &macro_signal, config.emergency_buffer_cny,
        );

        let permit = governor.acquire(quant_config.provider).await;
        let quant_response = call_with_retry(|| async {
            quant_client
                .chat_completion(&quant_messages, None, quant_config.temperature)
                .await
        })
        .await
        .map_err(|e| format!("Quant R{} LLM call failed: {}", round_num, e))?;
        drop(permit);

        total_tokens += quant_response.usage.total_tokens;
        let (quant_text, quant_trunc) = hard_truncate(&quant_response.text, quant_role, round_num);
        let quant_parsed = parse_role_output(quant_role, &quant_text, quant_trunc);
        round_outputs.push(RoundOutput {
            role: quant_role,
            round: round_num,
            parsed: quant_parsed,
            latency_ms: start.elapsed().as_millis() as u64,
            tokens_used: total_tokens,
        });

        // Risk
        let risk_role = if is_rebuttal { CommitteeRole::RiskR2 } else { CommitteeRole::RiskR1 };
        let risk_config = config.roles.iter().find(|r| r.role == risk_role)
            .ok_or("Risk role config missing")?;
        let risk_client = clients.get(&risk_config.provider)
            .ok_or("Risk provider client not found")?;

        let risk_prompt = load_prompt(risk_role)
            + &length_constraint_suffix(risk_role, round_num);
        let risk_messages = build_context_messages(
            &risk_prompt, symbol, &round_outputs, &macro_signal, config.emergency_buffer_cny,
        );

        let permit = governor.acquire(risk_config.provider).await;
        let risk_response = call_with_retry(|| async {
            risk_client
                .chat_completion(&risk_messages, None, risk_config.temperature)
                .await
        })
        .await
        .map_err(|e| format!("Risk R{} LLM call failed: {}", round_num, e))?;
        drop(permit);

        total_tokens += risk_response.usage.total_tokens;
        let (risk_text, risk_trunc) = hard_truncate(&risk_response.text, risk_role, round_num);
        let risk_parsed = parse_role_output(risk_role, &risk_text, risk_trunc);
        round_outputs.push(RoundOutput {
            role: risk_role,
            round: round_num,
            parsed: risk_parsed,
            latency_ms: start.elapsed().as_millis() as u64,
            tokens_used: total_tokens,
        });

        // Early convergence check
        if round_num >= 2 && check_convergence(&round_outputs) {
            log::info!("Convergence detected at round {}, stopping debate", round_num);
            break;
        }
    }

    // --- Phase 3: Wealth Context ---
    let wealth_config = config.roles.iter().find(|r| r.role == CommitteeRole::Wealth)
        .ok_or("Wealth role config missing")?;
    let wealth_client = clients.get(&wealth_config.provider)
        .ok_or("Wealth provider client not found")?;

    let wealth_prompt = load_prompt(CommitteeRole::Wealth)
        + &length_constraint_suffix(CommitteeRole::Wealth, 1);
    let wealth_messages = build_context_messages(
        &wealth_prompt, symbol, &round_outputs, &macro_signal, config.emergency_buffer_cny,
    );

    let permit = governor.acquire(wealth_config.provider).await;
    let wealth_response = call_with_retry(|| async {
        wealth_client
            .chat_completion(&wealth_messages, None, wealth_config.temperature)
            .await
    })
    .await
    .map_err(|e| format!("Wealth LLM call failed: {}", e))?;
    drop(permit);

    total_tokens += wealth_response.usage.total_tokens;
    let (wealth_text, wealth_trunc) = hard_truncate(&wealth_response.text, CommitteeRole::Wealth, 1);
    let wealth_parsed = parse_role_output(CommitteeRole::Wealth, &wealth_text, wealth_trunc);
    round_outputs.push(RoundOutput {
        role: CommitteeRole::Wealth,
        round: 1,
        parsed: wealth_parsed,
        latency_ms: start.elapsed().as_millis() as u64,
        tokens_used: total_tokens,
    });

    // --- Phase 4: CIO verdict ---
    let cio_config = config.roles.iter().find(|r| r.role == CommitteeRole::Cio)
        .ok_or("CIO role config missing")?;
    let cio_client = clients.get(&cio_config.provider)
        .ok_or("CIO provider client not found")?;

    let cio_prompt = load_prompt(CommitteeRole::Cio)
        + &length_constraint_suffix(CommitteeRole::Cio, 1);
    let cio_messages = build_context_messages(
        &cio_prompt, symbol, &round_outputs, &macro_signal, config.emergency_buffer_cny,
    );

    let permit = governor.acquire(cio_config.provider).await;
    let cio_response = call_with_retry(|| async {
        cio_client
            .chat_completion(&cio_messages, None, 0.1)
            .await
    })
    .await
    .map_err(|e| format!("CIO LLM call failed: {}", e))?;
    drop(permit);

    total_tokens += cio_response.usage.total_tokens;
    let (cio_text, cio_trunc) = hard_truncate(&cio_response.text, CommitteeRole::Cio, 1);
    let cio_parsed = parse_role_output(CommitteeRole::Cio, &cio_text, cio_trunc);
    round_outputs.push(RoundOutput {
        role: CommitteeRole::Cio,
        round: 1,
        parsed: cio_parsed.clone(),
        latency_ms: start.elapsed().as_millis() as u64,
        tokens_used: total_tokens,
    });

    // --- Phase 5: Post-analysis ---
    let sentinel = check_sentinel(&round_outputs);
    let convergence = check_convergence(&round_outputs);

    let sanity = cio_sanity_check(
        &cio_parsed,
        &round_outputs,
        &macro_signal,
        config.emergency_buffer_cny,
    );

    let final_verdict = if let Some(ref s) = sentinel {
        s.forced_verdict.clone()
    } else {
        sanity.final_verdict.clone()
    };
    let final_confidence = if let Some(ref s) = sentinel {
        s.forced_confidence
    } else {
        sanity.final_confidence
    };

    let total_latency = start.elapsed().as_millis() as u64;

    let result = CommitteeResult {
        symbol: symbol.to_string(),
        verdict: final_verdict,
        confidence: final_confidence,
        macro_signal: macro_signal.clone(),
        macro_strength: macro_parsed.strength.unwrap_or(5.0),
        reasoning: cio_parsed.raw_text.clone(),
        round_outputs: round_outputs.iter().map(|ro| RoundOutputSummary {
            role: ro.role.to_string(),
            round: ro.round,
            output: ro.parsed.raw_text.clone(),
            latency_ms: ro.latency_ms,
            tokens_used: ro.tokens_used,
        }).collect(),
        sanity_check: sanity,
        sentinel_override: sentinel,
        convergence_detected: convergence,
        total_latency_ms: total_latency,
        total_tokens,
    };

    // Archive
    if let Err(e) = archive_decision(symbol, &result) {
        log::warn!("Failed to archive committee decision: {}", e);
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Multi-symbol concurrency
// ---------------------------------------------------------------------------

pub async fn run_committee_batch(
    symbols: &[String],
    clients: &HashMap<ProviderId, Arc<dyn InvestLlmClient>>,
    config: &CommitteeConfig,
) -> Vec<Result<CommitteeResult, String>> {
    use futures_util::stream::{self, StreamExt};

    let results: Vec<_> = stream::iter(symbols.iter())
        .map(|symbol| {
            let clients = clients.clone();
            let config = config.clone();
            async move {
                tokio::time::timeout(
                    std::time::Duration::from_secs(config.timeout_secs),
                    run_committee(symbol, &clients, &config),
                )
                .await
                .unwrap_or_else(|_| Err(format!("{}: committee timed out", symbol)))
            }
        })
        .buffer_unordered(5)
        .collect()
        .await;

    results
}

// ---------------------------------------------------------------------------
// Context building
// ---------------------------------------------------------------------------

fn build_context_messages(
    system_prompt: &str,
    symbol: &str,
    round_outputs: &[RoundOutput],
    macro_signal: &str,
    emergency_buffer: f64,
) -> Vec<Message> {
    let mut messages = Vec::new();

    messages.push(Message {
        role: "system".to_string(),
        content: system_prompt.to_string(),
        tool_call_id: None,
        tool_calls: None,
        name: None,
    });

    let mut context = format!("【标的: {}】\n\n", symbol);
    for ro in round_outputs {
        context.push_str(&format!(
            "=== {} Round {} ===\n{}\n\n",
            ro.role, ro.round, ro.parsed.raw_text
        ));
    }
    context.push_str(&format!(
        "Macro SIGNAL: {}\nEmergency Buffer: {:.0} CNY\n",
        macro_signal, emergency_buffer
    ));

    messages.push(Message {
        role: "user".to_string(),
        content: context,
        tool_call_id: None,
        tool_calls: None,
        name: None,
    });

    messages
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30`

Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/invest/committee/orchestrator.rs
git commit -m "feat(invest): add committee orchestrator with multi-round debate and batch mode"
```

---

## Task 9: Verdict Archiving

**Files:**
- Modify: `src-tauri/src/invest/committee/archive.rs` (replace stub)

**Reference:** master plan §11.4 (decision archiving)

- [ ] **Step 1: Implement `archive.rs`**

Replace the stub content of `src-tauri/src/invest/committee/archive.rs` with:

```rust
use super::orchestrator::CommitteeResult;
use chrono::Local;
use std::path::PathBuf;

fn archive_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".claw-go").join("invest").join("committee")
}

pub fn archive_decision(symbol: &str, result: &CommitteeResult) -> Result<(), String> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let dir = archive_dir().join(&today);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("create archive dir: {}", e))?;

    // Markdown
    let md_content = format_decision_markdown(symbol, result);
    let md_path = dir.join(format!("{}.md", symbol));
    std::fs::write(&md_path, md_content)
        .map_err(|e| format!("write archive md: {}", e))?;

    // events.jsonl
    let events_path = archive_dir().join("events.jsonl");
    let event = serde_json::json!({
        "type": "committee_decision",
        "date": today,
        "symbol": symbol,
        "verdict": result.verdict,
        "confidence": result.confidence,
        "macro_signal": result.macro_signal,
        "macro_strength": result.macro_strength,
        "convergence": result.convergence_detected,
        "sentinel_override": result.sentinel_override.is_some(),
        "total_latency_ms": result.total_latency_ms,
        "total_tokens": result.total_tokens,
        "timestamp": Local::now().to_rfc3339(),
    });

    let mut line = serde_json::to_string(&event)
        .map_err(|e| format!("serialize event: {}", e))?;
    line.push('\n');

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&events_path)
        .map_err(|e| format!("open events.jsonl: {}", e))?;
    file.write_all(line.as_bytes())
        .map_err(|e| format!("write event: {}", e))?;

    log::info!("Archived decision for {} to {}", symbol, dir.display());
    Ok(())
}

fn format_decision_markdown(symbol: &str, result: &CommitteeResult) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut md = String::new();

    md.push_str(&format!("# {} — Committee Decision\n\n", symbol));
    md.push_str(&format!("**Date:** {}\n", now));
    md.push_str(&format!("**Verdict:** {} (confidence: {:.1})\n", result.verdict, result.confidence));
    md.push_str(&format!("**Macro Signal:** {} (strength: {:.1})\n\n", result.macro_signal, result.macro_strength));

    md.push_str("## Sanity Check\n\n");
    md.push_str(&format!("- Gate 1 (Signal Consistency): {}\n", if result.sanity_check.gate1_pass { "PASS" } else { "FAIL" }));
    md.push_str(&format!("- Gate 2 (Concentration < 40%): {}\n", if result.sanity_check.gate2_pass { "PASS" } else { "FAIL" }));
    md.push_str(&format!("- Gate 3 (Dry Powder): {}\n\n", if result.sanity_check.gate3_pass { "PASS" } else { "FAIL" }));

    if !result.sanity_check.notes.is_empty() {
        md.push_str("**Notes:**\n");
        for note in &result.sanity_check.notes {
            md.push_str(&format!("- {}\n", note));
        }
        md.push('\n');
    }

    if let Some(ref sentinel) = result.sentinel_override {
        md.push_str(&format!("## Sentinel Override\n\n{}\n\n", sentinel.reason));
    }

    if result.convergence_detected {
        md.push_str("**Convergence detected** — debate ended early.\n\n");
    }

    md.push_str("## Round Outputs\n\n");
    for ro in &result.round_outputs {
        md.push_str(&format!("### {} Round {}\n\n", ro.role, ro.round));
        md.push_str(&format!("{}\n\n", ro.output));
    }

    md.push_str("## CIO Reasoning\n\n");
    md.push_str(&format!("{}\n", result.reasoning));

    md.push_str(&format!(
        "\n---\n*Latency: {}ms | Tokens: {}*\n",
        result.total_latency_ms, result.total_tokens
    ));

    md
}

pub fn load_archive(symbol: &str, days: i64) -> Result<Vec<ArchivedDecision>, String> {
    let base = archive_dir();
    let mut decisions = Vec::new();

    for day_offset in 0..days {
        let date = (Local::now() - chrono::Duration::days(day_offset))
            .format("%Y-%m-%d")
            .to_string();
        let md_path = base.join(&date).join(format!("{}.md", symbol));
        if md_path.exists() {
            let content = std::fs::read_to_string(&md_path)
                .map_err(|e| format!("read archive {}: {}", md_path.display(), e))?;
            decisions.push(ArchivedDecision {
                date,
                symbol: symbol.to_string(),
                content,
            });
        }
    }

    Ok(decisions)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedDecision {
    pub date: String,
    pub symbol: String,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::super::orchestrator::{CommitteeResult, RoundOutputSummary};
    use super::super::analysis::SanityCheckResult;
    use super::*;

    fn make_test_result() -> CommitteeResult {
        CommitteeResult {
            symbol: "600519.SH".to_string(),
            verdict: "HOLD".to_string(),
            confidence: 0.6,
            macro_signal: "neutral".to_string(),
            macro_strength: 5.0,
            reasoning: "Test reasoning".to_string(),
            round_outputs: vec![RoundOutputSummary {
                role: "Macro".to_string(),
                round: 1,
                output: "SIGNAL: neutral".to_string(),
                latency_ms: 500,
                tokens_used: 300,
            }],
            sanity_check: SanityCheckResult {
                gate1_pass: true,
                gate2_pass: true,
                gate3_pass: true,
                final_verdict: "HOLD".to_string(),
                final_confidence: 0.6,
                notes: Vec::new(),
            },
            sentinel_override: None,
            convergence_detected: false,
            total_latency_ms: 5000,
            total_tokens: 2000,
        }
    }

    #[test]
    fn test_format_markdown() {
        let result = make_test_result();
        let md = format_decision_markdown("600519.SH", &result);
        assert!(md.contains("# 600519.SH — Committee Decision"));
        assert!(md.contains("HOLD"));
        assert!(md.contains("neutral"));
        assert!(md.contains("Gate 1"));
    }

    #[test]
    fn test_archive_dir_is_absolute() {
        let dir = archive_dir();
        assert!(dir.is_absolute());
        assert!(dir.to_string_lossy().contains("invest"));
        assert!(dir.to_string_lossy().contains("committee"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::archive::tests -- --nocapture 2>&1`

Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/invest/committee/archive.rs
git commit -m "feat(invest): add committee verdict archiving with markdown and events.jsonl"
```

---

## Task 10: LLM Config Persistence + Tauri Commands

**Files:**
- Modify: `src-tauri/src/commands/invest.rs` (add 4 new commands)
- Modify: `src-tauri/src/lib.rs` (register new commands)

**Reference:** RFC D1 (config storage), D5 (debate rounds dropdown)

- [ ] **Step 1: Add LLM config persistence helpers**

Add to the top of `src-tauri/src/commands/invest.rs` (after existing imports):

```rust
// ── LLM Config ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestLlmProviderConfig {
    pub provider_id: String,
    pub api_key: String,
    pub base_url: String,
    pub default_model: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvestLlmConfig {
    pub providers: Vec<InvestLlmProviderConfig>,
    pub debate_rounds: u8,
    pub emergency_buffer_cny: f64,
    pub timeout_secs: u64,
}

fn llm_config_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".claw-go").join("invest").join("llm_config.json")
}

#[tauri::command]
pub fn get_llm_config() -> Result<InvestLlmConfig, String> {
    let path = llm_config_path();
    if !path.exists() {
        return Ok(InvestLlmConfig {
            providers: vec![
                InvestLlmProviderConfig {
                    provider_id: "deepseek".to_string(),
                    api_key: String::new(),
                    base_url: "https://api.deepseek.com/v1".to_string(),
                    default_model: "deepseek-v4-pro".to_string(),
                },
                InvestLlmProviderConfig {
                    provider_id: "mimo_plan".to_string(),
                    api_key: String::new(),
                    base_url: "https://token-plan-cn.xiaomimimo.com/v1".to_string(),
                    default_model: "mimo-v2.5-pro".to_string(),
                },
                InvestLlmProviderConfig {
                    provider_id: "mimo_api".to_string(),
                    api_key: String::new(),
                    base_url: "https://api.xiaomimimo.com/v1".to_string(),
                    default_model: "mimo-v2.5-pro".to_string(),
                },
            ],
            debate_rounds: 4,
            emergency_buffer_cny: 100000.0,
            timeout_secs: 120,
        });
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("read llm_config: {}", e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("parse llm_config: {}", e))
}

#[tauri::command]
pub fn save_llm_config(config: InvestLlmConfig) -> Result<(), String> {
    let path = llm_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create dir: {}", e))?;
    }
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("serialize config: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("write llm_config: {}", e))
}
```

- [ ] **Step 2: Add run_committee and cancel commands**

Add after the LLM config commands in `src-tauri/src/commands/invest.rs`:

```rust
// ── Committee ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn run_committee(
    symbols: Vec<String>,
    debate_rounds: Option<u8>,
) -> Result<Vec<crate::invest::committee::orchestrator::CommitteeResult>, String> {
    use crate::invest::committee::orchestrator::{run_committee_batch, CommitteeConfig};
    use crate::invest::llm::{OpenAiCompatClient, ProviderId, LlmGovernor};
    use std::collections::HashMap;
    use std::sync::Arc;

    let llm_config = get_llm_config()?;
    let config = CommitteeConfig {
        debate_rounds: debate_rounds.unwrap_or(llm_config.debate_rounds),
        emergency_buffer_cny: llm_config.emergency_buffer_cny,
        timeout_secs: llm_config.timeout_secs,
        ..Default::default()
    };

    // Build clients from config
    let mut clients: HashMap<ProviderId, Arc<dyn crate::invest::llm::InvestLlmClient>> = HashMap::new();

    for pc in &llm_config.providers {
        let provider_id = match pc.provider_id.as_str() {
            "deepseek" => ProviderId::DeepSeek,
            "mimo_plan" => ProviderId::MiMoPlan,
            "mimo_api" => ProviderId::MiMoApi,
            _ => continue,
        };

        if pc.api_key.is_empty() {
            continue; // skip unconfigured providers
        }

        let client = OpenAiCompatClient::new(
            pc.base_url.clone(),
            pc.api_key.clone(),
            pc.default_model.clone(),
        );
        clients.insert(provider_id, Arc::new(client) as Arc<dyn crate::invest::llm::InvestLlmClient>);
    }

    if clients.is_empty() {
        return Err("No LLM providers configured. Please configure at least one provider in Settings → Invest → LLM Config.".to_string());
    }

    let results = run_committee_batch(&symbols, &clients, &config).await;

    // Collect results, converting Err to a partial result with error info
    let mut output = Vec::new();
    for (i, result) in results.into_iter().enumerate() {
        match result {
            Ok(r) => output.push(r),
            Err(e) => {
                log::warn!("Committee failed for {}: {}", symbols[i], e);
                // Return a HOLD verdict with error info
                output.push(crate::invest::committee::orchestrator::CommitteeResult {
                    symbol: symbols[i].clone(),
                    verdict: "HOLD".to_string(),
                    confidence: 0.0,
                    macro_signal: "error".to_string(),
                    macro_strength: 0.0,
                    reasoning: format!("[WORKER_UNAVAILABLE] {}", e),
                    round_outputs: vec![],
                    sanity_check: crate::invest::committee::analysis::SanityCheckResult {
                        gate1_pass: false,
                        gate2_pass: false,
                        gate3_pass: false,
                        final_verdict: "HOLD".to_string(),
                        final_confidence: 0.0,
                        notes: vec![e],
                    },
                    sentinel_override: None,
                    convergence_detected: false,
                    total_latency_ms: 0,
                    total_tokens: 0,
                });
            }
        }
    }

    Ok(output)
}

#[tauri::command]
pub fn get_role_prompts() -> Result<std::collections::HashMap<String, String>, String> {
    use crate::invest::committee::roles::{CommitteeRole, load_prompt};

    let mut map = std::collections::HashMap::new();
    for role in CommitteeRole::all() {
        map.insert(format!("{:?}", role).to_lowercase(), load_prompt(*role));
    }
    Ok(map)
}

#[tauri::command]
pub fn save_role_prompt(role: String, content: String) -> Result<(), String> {
    use crate::invest::committee::roles::{CommitteeRole, save_prompt};

    let role_enum = match role.as_str() {
        "macro" => CommitteeRole::Macro,
        "qu_analyst" => CommitteeRole::QuantR1,
        "risk_officer" => CommitteeRole::RiskR1,
        "wealth_context" => CommitteeRole::Wealth,
        "qu_analyst_rebuttal" => CommitteeRole::QuantR2,
        "risk_officer_rebuttal" => CommitteeRole::RiskR2,
        "cio" => CommitteeRole::Cio,
        _ => return Err(format!("unknown role: {}", role)),
    };

    save_prompt(role_enum, &content)
}
```

- [ ] **Step 3: Register new commands in `lib.rs`**

In `src-tauri/src/lib.rs`, find the invest commands registration block (around line 387-414) and add the new commands to the `invoke_handler`:

```rust
// Add these to the .invoke_handler(tauri::generate_handler![...]) block:
get_llm_config,
save_llm_config,
run_committee,
get_role_prompts,
save_role_prompt,
```

- [ ] **Step 4: Add `pub mod invest;` to `lib.rs` module tree**

In `src-tauri/src/lib.rs`, find the module declarations section and add:

```rust
pub mod invest;
```

This should be near the other `pub mod` declarations (e.g., near `pub mod tushare;`).

- [ ] **Step 5: Verify compilation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -40`

Expected: Compiles. May have warnings about unused imports — those are fine.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/invest.rs src-tauri/src/lib.rs
git commit -m "feat(invest): add LLM config persistence and committee Tauri commands"
```

---

## Task 11: Frontend — Committee Store, Provider Config, Committee Tab

**Files:**
- Create: `src/lib/stores/invest-committee-store.svelte.ts`
- Create: `src/lib/components/invest/ProviderConfigPanel.svelte`
- Modify: `src/routes/invest/+page.svelte` (replace placeholder committee tab)

**Reference:** RFC D5 (debate rounds dropdown), D1 (provider config UI)

- [ ] **Step 1: Create `invest-committee-store.svelte.ts`**

```typescript
import { invoke } from '@tauri-apps/api/core';

export interface InvestLlmProviderConfig {
    providerId: string;
    apiKey: string;
    baseUrl: string;
    defaultModel: string;
}

export interface InvestLlmConfig {
    providers: InvestLlmProviderConfig[];
    debateRounds: number;
    emergencyBufferCny: number;
    timeoutSecs: number;
}

export interface RoundOutputSummary {
    role: string;
    round: number;
    output: string;
    latencyMs: number;
    tokensUsed: number;
}

export interface SanityCheckResult {
    gate1Pass: boolean;
    gate2Pass: boolean;
    gate3Pass: boolean;
    finalVerdict: string;
    finalConfidence: number;
    notes: string[];
}

export interface CommitteeResult {
    symbol: string;
    verdict: string;
    confidence: number;
    macroSignal: string;
    macroStrength: number;
    reasoning: string;
    roundOutputs: RoundOutputSummary[];
    sanityCheck: SanityCheckResult;
    sentinelOverride: { reason: string; forcedVerdict: string; forcedConfidence: number } | null;
    convergenceDetected: boolean;
    totalLatencyMs: number;
    totalTokens: number;
}

export interface RolePrompts {
    [role: string]: string;
}

class InvestCommitteeStore {
    // LLM config
    llmConfig = $state<InvestLlmConfig | null>(null);
    configLoading = $state(false);

    // Committee run
    running = $state(false);
    results = $state<CommitteeResult[]>([]);
    runError = $state<string | null>(null);

    // Role prompts
    rolePrompts = $state<RolePrompts>({});

    // Config panel
    showConfigPanel = $state(false);

    async loadConfig() {
        this.configLoading = true;
        try {
            this.llmConfig = await invoke<InvestLlmConfig>('get_llm_config');
        } catch (e) {
            console.error('Failed to load LLM config:', e);
        } finally {
            this.configLoading = false;
        }
    }

    async saveConfig(config: InvestLlmConfig) {
        await invoke('save_llm_config', { config });
        this.llmConfig = config;
    }

    async runCommittee(symbols: string[], debateRounds?: number) {
        this.running = true;
        this.runError = null;
        this.results = [];
        try {
            this.results = await invoke<CommitteeResult[]>('run_committee', {
                symbols,
                debateRounds: debateRounds ?? null,
            });
        } catch (e) {
            this.runError = String(e);
        } finally {
            this.running = false;
        }
    }

    async loadRolePrompts() {
        try {
            this.rolePrompts = await invoke<RolePrompts>('get_role_prompts');
        } catch (e) {
            console.error('Failed to load role prompts:', e);
        }
    }

    async saveRolePrompt(role: string, content: string) {
        await invoke('save_role_prompt', { role, content });
        this.rolePrompts[role] = content;
    }
}

export const investCommitteeStore = new InvestCommitteeStore();
```

- [ ] **Step 2: Create `ProviderConfigPanel.svelte`**

```svelte
<script lang="ts">
    import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
    import type { InvestLlmProviderConfig } from '$lib/stores/invest-committee-store.svelte';
    import { _ } from '$lib/i18n';

    const DEBOUNCE_OPTIONS = [1, 2, 3, 4, 6, 8];

    let config = $derived(investCommitteeStore.llmConfig);
    let providers = $derived(config?.providers ?? []);
    let debateRounds = $derived(config?.debateRounds ?? 4);
    let emergencyBuffer = $derived(config?.emergencyBufferCny ?? 100000);

    function updateProvider(index: number, field: keyof InvestLlmProviderConfig, value: string) {
        if (!config) return;
        const updated = { ...config };
        updated.providers = [...updated.providers];
        updated.providers[index] = { ...updated.providers[index], [field]: value };
        investCommitteeStore.saveConfig(updated);
    }

    function updateDebateRounds(value: number) {
        if (!config) return;
        investCommitteeStore.saveConfig({ ...config, debateRounds: value });
    }

    function updateEmergencyBuffer(value: string) {
        if (!config) return;
        const num = parseFloat(value);
        if (!isNaN(num) && num >= 0) {
            investCommitteeStore.saveConfig({ ...config, emergencyBufferCny: num });
        }
    }
</script>

<div class="provider-config-panel">
    <h3>{$_('invest.llm_config') || 'LLM Provider Configuration'}</h3>

    <div class="provider-grid">
        <div class="grid-header">
            <span>{$_('invest.provider') || 'Provider'}</span>
            <span>{$_('invest.api_key') || 'API Key'}</span>
            <span>{$_('invest.base_url') || 'Base URL'}</span>
            <span>{$_('invest.model') || 'Model'}</span>
        </div>
        {#each providers as provider, i}
            <div class="grid-row">
                <span class="provider-label">{provider.providerId}</span>
                <input
                    type="password"
                    value={provider.apiKey}
                    oninput={(e) => updateProvider(i, 'apiKey', (e.target as HTMLInputElement).value)}
                    placeholder="sk-..."
                />
                <input
                    type="text"
                    value={provider.baseUrl}
                    oninput={(e) => updateProvider(i, 'baseUrl', (e.target as HTMLInputElement).value)}
                />
                <input
                    type="text"
                    value={provider.defaultModel}
                    oninput={(e) => updateProvider(i, 'defaultModel', (e.target as HTMLInputElement).value)}
                />
            </div>
        {/each}
    </div>

    <div class="config-row">
        <label>
            {$_('invest.debate_rounds') || 'Debate Rounds'}
            <select onchange={(e) => updateDebateRounds(parseInt((e.target as HTMLSelectElement).value))}>
                {#each DEBOUNCE_OPTIONS as opt}
                    <option value={opt} selected={opt === debateRounds}>{opt}</option>
                {/each}
            </select>
        </label>

        <label>
            {$_('invest.emergency_buffer') || 'Emergency Buffer (CNY)'}
            <input
                type="number"
                value={emergencyBuffer}
                oninput={(e) => updateEmergencyBuffer((e.target as HTMLInputElement).value)}
                min="0"
                step="10000"
            />
        </label>
    </div>
</div>

<style>
    .provider-config-panel {
        padding: 1rem;
        border: 1px solid var(--border-color, #333);
        border-radius: 8px;
        margin-bottom: 1rem;
    }
    .provider-grid {
        display: grid;
        grid-template-columns: 100px 1fr 1fr 120px;
        gap: 0.5rem;
        margin-bottom: 1rem;
    }
    .grid-header {
        font-weight: 600;
        font-size: 0.85rem;
        color: var(--text-secondary, #888);
    }
    .grid-row {
        display: contents;
    }
    .provider-label {
        font-size: 0.9rem;
        font-weight: 500;
        display: flex;
        align-items: center;
    }
    input, select {
        padding: 0.4rem 0.6rem;
        border: 1px solid var(--border-color, #444);
        border-radius: 4px;
        background: var(--bg-input, #1a1a2e);
        color: var(--text-primary, #eee);
        font-size: 0.85rem;
    }
    .config-row {
        display: flex;
        gap: 1.5rem;
        align-items: center;
    }
    .config-row label {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        font-size: 0.85rem;
    }
</style>
```

- [ ] **Step 3: Update `src/routes/invest/+page.svelte` — replace committee tab placeholder**

Read the current file first, then replace the committee tab content. The committee tab currently shows "coming in Phase 3" placeholder. Replace with:

```svelte
<!-- In the {#if activeTab === 'committee'} block, replace the placeholder with: -->
{#if activeTab === 'committee'}
    <div class="committee-tab">
        <ProviderConfigPanel />

        <div class="committee-controls">
            <div class="run-section">
                <input
                    type="text"
                    bind:value={committeeSymbols}
                    placeholder={$_('invest.symbols_placeholder') || 'Enter symbols, comma-separated (e.g., 600519.SH,000858.SZ)'}
                    class="symbols-input"
                />
                <button
                    onclick={() => runCommitteeAction()}
                    disabled={investCommitteeStore.running || !committeeSymbols.trim()}
                    class="run-btn"
                >
                    {investCommitteeStore.running
                        ? ($_('invest.running') || 'Running...')
                        : ($_('invest.run_committee') || 'Run Committee')}
                </button>
            </div>

            {#if investCommitteeStore.runError}
                <div class="error-banner">{investCommitteeStore.runError}</div>
            {/if}
        </div>

        {#if investCommitteeStore.results.length > 0}
            <div class="results-list">
                {#each investCommitteeStore.results as result}
                    <div class="result-card verdict-{result.verdict.toLowerCase()}">
                        <div class="result-header">
                            <span class="symbol">{result.symbol}</span>
                            <span class="verdict">{result.verdict}</span>
                            <span class="confidence">({(result.confidence * 100).toFixed(0)}%)</span>
                        </div>
                        <div class="result-meta">
                            <span>Macro: {result.macroSignal} ({result.macroStrength.toFixed(1)})</span>
                            <span>Tokens: {result.totalTokens}</span>
                            <span>Latency: {(result.totalLatencyMs / 1000).toFixed(1)}s</span>
                            {#if result.convergenceDetected}
                                <span class="badge">Converged</span>
                            {/if}
                            {#if result.sentinelOverride}
                                <span class="badge sentinel">SENTINEL</span>
                            {/if}
                        </div>
                        <div class="sanity-checks">
                            <span class:pass={result.sanityCheck.gate1Pass} class:fail={!result.sanityCheck.gate1Pass}>G1</span>
                            <span class:pass={result.sanityCheck.gate2Pass} class:fail={!result.sanityCheck.gate2Pass}>G2</span>
                            <span class:pass={result.sanityCheck.gate3Pass} class:fail={!result.sanityCheck.gate3Pass}>G3</span>
                        </div>
                        <div class="reasoning">{result.reasoning}</div>
                        <details class="round-details">
                            <summary>{$_('invest.round_outputs') || 'Round Outputs'} ({result.roundOutputs.length})</summary>
                            {#each result.roundOutputs as ro}
                                <div class="round-output">
                                    <strong>{ro.role} R{ro.round}</strong>
                                    <span class="round-meta">{ro.tokensUsed}t / {ro.latencyMs}ms</span>
                                    <pre>{ro.output}</pre>
                                </div>
                            {/each}
                        </details>
                    </div>
                {/each}
            </div>
        {/if}
    </div>
{/if}
```

- [ ] **Step 4: Add committee-specific styles to the page**

Add styles for the committee tab (in the `<style>` block of `+page.svelte`):

```css
.committee-tab {
    padding: 1rem;
}
.committee-controls {
    margin: 1rem 0;
}
.run-section {
    display: flex;
    gap: 0.5rem;
    align-items: center;
}
.symbols-input {
    flex: 1;
    padding: 0.5rem;
    border: 1px solid var(--border-color, #444);
    border-radius: 4px;
    background: var(--bg-input, #1a1a2e);
    color: var(--text-primary, #eee);
}
.run-btn {
    padding: 0.5rem 1.5rem;
    background: var(--accent-color, #4a9eff);
    color: white;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-weight: 600;
}
.run-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
}
.error-banner {
    padding: 0.75rem;
    margin-top: 0.5rem;
    background: #ff444422;
    border: 1px solid #ff4444;
    border-radius: 4px;
    color: #ff4444;
}
.results-list {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    margin-top: 1rem;
}
.result-card {
    padding: 1rem;
    border: 1px solid var(--border-color, #333);
    border-radius: 8px;
    background: var(--bg-card, #1a1a2e);
}
.result-card.verdict-buy, .result-card.verdict-accumulate { border-left: 4px solid #4caf50; }
.result-card.verdict-hold { border-left: 4px solid #ff9800; }
.result-card.verdict-trim, .result-card.verdict-sell { border-left: 4px solid #f44336; }
.result-header {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
}
.symbol { font-size: 1.2rem; font-weight: 700; }
.verdict { font-size: 1.1rem; font-weight: 600; }
.confidence { font-size: 0.9rem; color: var(--text-secondary, #888); }
.result-meta {
    display: flex;
    gap: 1rem;
    font-size: 0.8rem;
    color: var(--text-secondary, #888);
    margin-bottom: 0.5rem;
}
.badge {
    padding: 0.1rem 0.4rem;
    background: #4a9eff33;
    border-radius: 3px;
    font-size: 0.75rem;
}
.badge.sentinel { background: #ff444433; }
.sanity-checks {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 0.5rem;
}
.sanity-checks span {
    padding: 0.2rem 0.5rem;
    border-radius: 3px;
    font-size: 0.8rem;
    font-weight: 600;
}
.sanity-checks .pass { background: #4caf5033; color: #4caf50; }
.sanity-checks .fail { background: #f4433633; color: #f44336; }
.reasoning {
    font-size: 0.9rem;
    line-height: 1.5;
    white-space: pre-wrap;
    margin-bottom: 0.5rem;
}
.round-details summary {
    cursor: pointer;
    font-size: 0.85rem;
    color: var(--accent-color, #4a9eff);
}
.round-output {
    margin: 0.5rem 0;
    padding: 0.5rem;
    background: var(--bg-code, #0d0d1a);
    border-radius: 4px;
}
.round-output pre {
    margin: 0.25rem 0 0;
    font-size: 0.8rem;
    white-space: pre-wrap;
}
.round-meta {
    font-size: 0.75rem;
    color: var(--text-secondary, #888);
    margin-left: 0.5rem;
}
```

- [ ] **Step 5: Add import for ProviderConfigPanel**

In `src/routes/invest/+page.svelte`, add at the top of the script section:

```typescript
import ProviderConfigPanel from '$lib/components/invest/ProviderConfigPanel.svelte';
import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';

let committeeSymbols = $state('');

async function runCommitteeAction() {
    const symbols = committeeSymbols
        .split(',')
        .map(s => s.trim())
        .filter(s => s.length > 0);
    if (symbols.length > 0) {
        await investCommitteeStore.runCommittee(symbols);
    }
}
```

- [ ] **Step 6: Verify frontend builds**

Run: `npm run check 2>&1 | tail -20`

Expected: No type errors. Svelte 5 rune syntax should pass.

- [ ] **Step 7: Commit**

```bash
git add src/lib/stores/invest-committee-store.svelte.ts
git add src/lib/components/invest/ProviderConfigPanel.svelte
git add src/routes/invest/+page.svelte
git commit -m "feat(invest): add committee store, provider config panel, and committee tab UI"
```

---

## Task 12: Integration Wiring + lib.rs Module Registration

**Files:**
- Modify: `src-tauri/src/lib.rs` (add `pub mod invest;` and register commands)

**Reference:** Phase 2 pattern (invest DB init at lines 527-530)

- [ ] **Step 1: Add `pub mod invest;` to lib.rs**

In `src-tauri/src/lib.rs`, find the module declarations section (near `pub mod tushare;` around line 20-30) and add:

```rust
pub mod invest;
```

- [ ] **Step 2: Register the 6 new Tauri commands**

In `src-tauri/src/lib.rs`, find the `invoke_handler` block (around lines 387-414) and add these commands:

```rust
commands::invest::get_llm_config,
commands::invest::save_llm_config,
commands::invest::run_committee,
commands::invest::get_role_prompts,
commands::invest::save_role_prompt,
```

- [ ] **Step 3: Full verification**

Run the full verification sequence:

```bash
# Rust compilation
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -10

# Rust tests
cargo test --manifest-path src-tauri/Cargo.toml invest:: -- --nocapture 2>&1 | tail -30

# Frontend type check
npm run check 2>&1 | tail -20

# i18n check
npm run i18n:check 2>&1 | tail -10
```

Expected: All pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(invest): wire up invest module and register committee commands"
```

---

## Task 13: i18n Keys + Final Polish

**Files:**
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

- [ ] **Step 1: Add i18n keys for the committee UI**

Add to `messages/en.json` under the `invest` section:

```json
"invest.llm_config": "LLM Provider Configuration",
"invest.provider": "Provider",
"invest.api_key": "API Key",
"invest.base_url": "Base URL",
"invest.model": "Model",
"invest.debate_rounds": "Debate Rounds",
"invest.emergency_buffer": "Emergency Buffer (CNY)",
"invest.symbols_placeholder": "Enter symbols, comma-separated (e.g., 600519.SH,000858.SZ)",
"invest.run_committee": "Run Committee",
"invest.running": "Running...",
"invest.round_outputs": "Round Outputs",
"invest.gate1": "Gate 1: Signal Consistency",
"invest.gate2": "Gate 2: Concentration",
"invest.gate3": "Gate 3: Dry Powder"
```

Add to `messages/zh-CN.json` under the `invest` section:

```json
"invest.llm_config": "LLM 提供商配置",
"invest.provider": "提供商",
"invest.api_key": "API 密钥",
"invest.base_url": "基础 URL",
"invest.model": "模型",
"invest.debate_rounds": "辩论轮数",
"invest.emergency_buffer": "应急储备金 (CNY)",
"invest.symbols_placeholder": "输入股票代码，逗号分隔（如 600519.SH,000858.SZ）",
"invest.run_committee": "运行委员会",
"invest.running": "运行中...",
"invest.round_outputs": "轮次输出",
"invest.gate1": "第一关：信号一致性",
"invest.gate2": "第二关：集中度",
"invest.gate3": "第三关：现金充足性"
```

- [ ] **Step 2: Run i18n check**

Run: `npm run i18n:check 2>&1 | tail -10`

Expected: No missing keys.

- [ ] **Step 3: Commit**

```bash
git add messages/en.json messages/zh-CN.json
git commit -m "feat(i18n): add committee UI translation keys"
```

---

## Task 14: Final Verification + Phase 3a Completion

- [ ] **Step 1: Full build verification**

```bash
npm run verify 2>&1 | tail -30
```

Expected: All checks pass.

- [ ] **Step 2: Run all invest tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml invest:: -- --nocapture 2>&1
```

Expected: All tests pass (parser tests, analysis tests, tools tests, archive tests).

- [ ] **Step 3: Update plan doc status**

Rename the plan file from `[wip]` to `[done]`:

```bash
mv "docs/superpowers/plans/[wip] 2026-05-29-openinvest-phase3a-llm-committee.md" \
   "docs/superpowers/plans/[done] 2026-05-29-openinvest-phase3a-llm-committee.md"
```

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "chore(invest): mark Phase 3a plan as done"
```

---

## Acceptance Criteria

| Criterion | Verification |
|-----------|-------------|
| `InvestLlmClient` trait compiles with `async-trait` | `cargo check` |
| `OpenAiCompatClient` streams SSE correctly | Unit test + manual test with DeepSeek API |
| `LlmGovernor` enforces per-provider Semaphore(8) | Concurrent test |
| Parser extracts SIGNAL/STRENGTH/verdict from LLM output | 10 parser unit tests pass |
| Convergence detection stops debate early | Convergence unit tests pass |
| SENTINEL triggers on concentration shift > 0.3% | Sentinel unit tests pass |
| CIO 3 Gates enforce safety constraints | 7 sanity check unit tests pass |
| 5 Macro tools return formatted results | Tool unit tests + manual Tushare test |
| Orchestrator runs Macro→Q1→R1→Wealth→Q2→R2→CIO | Manual integration test |
| Batch mode runs 5 assets concurrently | Manual test with 5 symbols |
| Verdict archived to `.committee/<date>/<symbol>.md` | File exists after run |
| `events.jsonl` appended | File has new line after run |
| LLM config persisted to `~/.claw-go/invest/llm_config.json` | File exists after save |
| Provider config UI shows 3-row matrix | Visual verification |
| Committee tab shows results with sanity check badges | Visual verification |
| `cargo check`, `cargo test invest::`, `npm run check` all pass | CI green |
| i18n keys present in both en.json and zh-CN.json | `npm run i18n:check` passes |
