# Codex JSONL Executor 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用 `codex exec --json` JSONL 适配器替换 Codex 的 PTY+transcript 执行路径,通过新的 `agent/executor/` trait 模块统一 Claude 和 Codex 两条派发路径,实现实时工具执行可见性、精确 thread_id Resume、删除约 1250 行 PTY 代码。

**Architecture:** 新建 `agent/executor/` 子模块(mod.rs trait + claude.rs 薄包装 + codex.rs 进程驱动 + codex_state.rs 纯状态机),`stream.rs::run_agent` 退化为分发器。Codex 每 turn 一个有界 `tokio::process::Command` 子进程,逐行解析 JSONL 事件流为 `BusEvent`,thread_id 在 `thread.started` 即落盘到 `RunMeta.conversation_ref`,Stop 复用 `commands/runs.rs::stop_run` 现有的 ProcessMap-kill 机制。前端 `canResumeStructurally` 放宽允许 `pipe_exec + CodexThread`。Claude 路径仅做薄包装,SessionActor 内部完全不动。

**Tech Stack:** Rust(tokio process / async-trait), Tauri 2 IPC, Svelte 5 runes 前端, Codex CLI v0.130 JSONL schema。

**Spike 数据(2026-05-20 已确认,直接基于此实现)**:
- `item.started(command_execution)` 的 payload **包含** `command` 字段(可立即用于 ToolStart 渲染)。
- `item.completed(command_execution)` 的字段名是 `aggregated_output`(不是 `output`)。
- `item.id` 形如 `item_0`、`item_1`,在每个 turn 内自增,**跨 turn 会复用从 0 重新开始**。
- `turn.completed.usage` 含 `input_tokens` / `cached_input_tokens` / `output_tokens` / `reasoning_output_tokens`(无 `cache_creation_tokens`)。
- `codex exec resume <SESSION_ID>` 接受 thread_id 作为位置参数(无 `--thread-id` 选项)。
- `codex exec --help` 列表中**不含** `--no-alt-screen`(那是交互模式 flag),故 exec 模式下应移除。

---

### Task 1: 加 `async-trait` 依赖

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: 在 [dependencies] 添加 async-trait**

打开 `src-tauri/Cargo.toml`,找到 `[dependencies]` 段,加一行(若已存在则跳过)：

```toml
async-trait = "0.1"
```

- [ ] **Step 2: 验证可编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过(没有 unused dep 警告,因为还未使用——这是预期的,下一任务即用)。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml
git commit -m "chore: add async-trait dep for executor abstraction"
```

---

### Task 2: 新建 `agent/executor/mod.rs` trait + ExecutorRequest + for_agent

**Files:**
- Create: `src-tauri/src/agent/executor/mod.rs`
- Create: `src-tauri/src/agent/executor/claude.rs` (stub)
- Create: `src-tauri/src/agent/executor/codex.rs` (stub)
- Modify: `src-tauri/src/agent/mod.rs`

- [ ] **Step 1: 写 mod.rs 骨架**

```rust
// src-tauri/src/agent/executor/mod.rs
use crate::agent::stream::ProcessMap;
use crate::agent::windows_msvc_env::SpawnEnvPlan;
use std::sync::Arc;
use tauri::AppHandle;

pub mod claude;
pub mod codex;

/// Inputs an Executor needs to spawn one turn.
/// Command-line and resume thread_id are baked into `args` by the caller
/// (commands/chat.rs via build_agent_command / build_agent_resume_command);
/// executors do not re-derive them.
pub struct ExecutorRequest {
    pub run_id: String,
    pub cwd: String,
    pub agent: String,
    pub spawn_env_plan: SpawnEnvPlan,
    pub display_command: String,
    pub process_command: String,
    pub args: Vec<String>,
}

#[async_trait::async_trait]
pub trait Executor: Send + Sync {
    async fn run(
        &self,
        app: AppHandle,
        process_map: ProcessMap,
        request: ExecutorRequest,
    ) -> Result<(), String>;
}

pub fn for_agent(agent: &str) -> Result<Arc<dyn Executor>, String> {
    match agent {
        "claude" => Ok(Arc::new(claude::ClaudeExecutor) as Arc<dyn Executor>),
        "codex" => Ok(Arc::new(codex::CodexExecutor) as Arc<dyn Executor>),
        other => Err(format!("Unsupported executor: {other}")),
    }
}
```

- [ ] **Step 2: 写 claude.rs 占位 stub**

```rust
// src-tauri/src/agent/executor/claude.rs
use super::{Executor, ExecutorRequest};
use crate::agent::stream::ProcessMap;
use tauri::AppHandle;

pub struct ClaudeExecutor;

#[async_trait::async_trait]
impl Executor for ClaudeExecutor {
    async fn run(
        &self,
        _app: AppHandle,
        _process_map: ProcessMap,
        _req: ExecutorRequest,
    ) -> Result<(), String> {
        Err("ClaudeExecutor not yet wired".to_string())
    }
}
```

- [ ] **Step 3: 写 codex.rs 占位 stub**

```rust
// src-tauri/src/agent/executor/codex.rs
use super::{Executor, ExecutorRequest};
use crate::agent::stream::ProcessMap;
use tauri::AppHandle;

pub struct CodexExecutor;

#[async_trait::async_trait]
impl Executor for CodexExecutor {
    async fn run(
        &self,
        _app: AppHandle,
        _process_map: ProcessMap,
        _req: ExecutorRequest,
    ) -> Result<(), String> {
        Err("CodexExecutor not yet wired".to_string())
    }
}
```

- [ ] **Step 4: 在 agent/mod.rs 注册 executor 模块**

打开 `src-tauri/src/agent/mod.rs`,在末尾(或合适位置,保持字母顺序)加入:

```rust
pub mod executor;
```

- [ ] **Step 5: 验证可编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。可能有 dead_code / unused warnings(占位 stub 还没被调用),允许。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/agent/executor src-tauri/src/agent/mod.rs
git commit -m "feat: add executor trait and module skeleton"
```

---

### Task 3: 抽 `stream.rs::run_agent` 的 Claude 分支为 `run_claude_pipe_or_session`

**Files:**
- Modify: `src-tauri/src/agent/stream.rs`

- [ ] **Step 1: 读 stream.rs 当前的 run_agent**

Read: `src-tauri/src/agent/stream.rs:112-439`
Expected: 看到 `run_agent` 内 `if native_transcript_mode { native_pty::run_native_pty_agent }` 之后是 Codex JSONL 分支(line 230-293)和 Claude 分支(line 296-end)。

- [ ] **Step 2: 添加 run_claude_pipe_or_session 函数**

在 `stream.rs` 中,新增一个函数 `run_claude_pipe_or_session`,签名:

```rust
#[allow(clippy::too_many_arguments)]
pub async fn run_claude_pipe_or_session(
    app: AppHandle,
    process_map: ProcessMap,
    run_id: String,
    process_command: String,
    args: Vec<String>,
    cwd: String,
    agent: String,
    spawn_env_plan: SpawnEnvPlan,
    display_command: String,
) -> Result<(), String> {
    // 把现有 run_agent 中,从 emit_run_event(System "Started ...") 开始,
    // 一直到末尾的所有 Claude 路径代码(不含 codex JSONL 分支、不含 native_pty 分支)
    // 复制到这里。stdout 处理只保留 Claude 分支(line 296-323)。
    // ……
    Ok(())
}
```

实操要点:
- 把现有 `run_agent` 函数从 `emit_run_event(RunEventType::System, ...)` (line 168) 开始的代码体直接搬到新函数。
- stdout 处理中只保留 `else { /* Claude */ }` 分支(line 296-323)的内容,作为 stdout reader 的逻辑。
- **不删 `run_agent`**——下一个任务会改造它为分发器。

- [ ] **Step 3: 编译验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。会有 `run_claude_pipe_or_session` 未被调用的警告,允许。

- [ ] **Step 4: 跑 stream 模块的现有测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml stream:: -- --nocapture`
Expected: 现有 stream.rs 测试(`display_command_quotes_prompt_arguments_without_losing_boundaries` / `bare_cli_names_are_resolved_before_spawn_but_paths_are_left_intact` / `npm_shim_invocation_prefers_node_without_shelling_out_to_cmd`)继续通过。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/agent/stream.rs
git commit -m "refactor: extract claude pipe/session path into run_claude_pipe_or_session"
```

---

### Task 4: 实现 ClaudeExecutor 并验证 Claude 路径无回归

**Files:**
- Modify: `src-tauri/src/agent/executor/claude.rs`

- [ ] **Step 1: 让 ClaudeExecutor::run 调用 run_claude_pipe_or_session**

```rust
// src-tauri/src/agent/executor/claude.rs
use super::{Executor, ExecutorRequest};
use crate::agent::stream::{run_claude_pipe_or_session, ProcessMap};
use tauri::AppHandle;

pub struct ClaudeExecutor;

#[async_trait::async_trait]
impl Executor for ClaudeExecutor {
    async fn run(
        &self,
        app: AppHandle,
        process_map: ProcessMap,
        req: ExecutorRequest,
    ) -> Result<(), String> {
        // Claude path is identical to the legacy pipe/session execution.
        run_claude_pipe_or_session(
            app,
            process_map,
            req.run_id,
            req.process_command,
            req.args,
            req.cwd,
            req.agent,
            req.spawn_env_plan,
            req.display_command,
        )
        .await
    }
}
```

文件级 doc 注释加一段:`/// ClaudeExecutor 仅服务于 stream.rs::run_agent 派发的 pipe-exec Claude 路径; SessionActor-backed 会话由 commands/session.rs 独立启动,不经过 Executor trait。`

- [ ] **Step 2: 编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/agent/executor/claude.rs
git commit -m "feat: implement ClaudeExecutor as thin wrapper around run_claude_pipe_or_session"
```

---

### Task 5: 写 `codex_state.rs` 状态机骨架 + 第一组单元测试(thread.started / turn.started)

**Files:**
- Create: `src-tauri/src/agent/executor/codex_state.rs`
- Modify: `src-tauri/src/agent/executor/mod.rs`(声明子模块)

- [ ] **Step 1: 在 executor/mod.rs 加子模块声明**

```rust
pub mod codex_state;
```

- [ ] **Step 2: 写状态机骨架**

```rust
// src-tauri/src/agent/executor/codex_state.rs
use crate::models::BusEvent;
use serde_json::Value;
use std::collections::HashMap;

pub struct CodexProtocolState {
    run_id: String,
    pending_tools: HashMap<String, ()>,
    sent_session_init: bool,
    turn_active: bool,
    captured_thread_id: Option<String>,
    seen_turn_completed_count: u32,
}

impl CodexProtocolState {
    pub fn new(run_id: String) -> Self {
        Self {
            run_id,
            pending_tools: HashMap::new(),
            sent_session_init: false,
            turn_active: false,
            captured_thread_id: None,
            seen_turn_completed_count: 0,
        }
    }

    pub fn captured_thread_id(&self) -> Option<&str> {
        self.captured_thread_id.as_deref()
    }

    pub fn has_seen_turn_completed(&self) -> bool {
        self.seen_turn_completed_count > 0
    }

    pub fn map_event(&mut self, raw: &Value) -> Vec<BusEvent> {
        let type_str = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match type_str {
            "thread.started" => self.handle_thread_started(raw),
            "turn.started" => self.handle_turn_started(),
            _ => Vec::new(),
        }
    }

    fn handle_thread_started(&mut self, raw: &Value) -> Vec<BusEvent> {
        let Some(tid) = raw.get("thread_id").and_then(|v| v.as_str()) else {
            return Vec::new();
        };
        self.captured_thread_id = Some(tid.to_string());
        if self.sent_session_init {
            return Vec::new();
        }
        self.sent_session_init = true;
        vec![BusEvent::SessionInit {
            run_id: self.run_id.clone(),
            session_id: Some(tid.to_string()),
            model: None,
            tools: Vec::new(),
            cwd: String::new(),
            slash_commands: Vec::new(),
            mcp_servers: Vec::new(),
            permission_mode: None,
            api_key_source: None,
            claude_code_version: None,
            output_style: None,
            agents: Vec::new(),
            skills: Vec::new(),
            plugins: Vec::new(),
            plugin_errors: Vec::new(),
            fast_mode_state: None,
            msvc_injected: None,
        }]
    }

    fn handle_turn_started(&mut self) -> Vec<BusEvent> {
        self.turn_active = true;
        vec![BusEvent::RunState {
            run_id: self.run_id.clone(),
            state: "running".to_string(),
            exit_code: None,
            error: None,
        }]
    }
}
```

- [ ] **Step 3: 写 thread.started / turn.started 测试**

在 `codex_state.rs` 文件底部加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn state() -> CodexProtocolState {
        CodexProtocolState::new("run-1".to_string())
    }

    #[test]
    fn thread_started_emits_session_init_and_captures_tid() {
        let mut s = state();
        let events = s.map_event(&json!({"type":"thread.started","thread_id":"019e4159-ca48"}));
        assert_eq!(events.len(), 1);
        match &events[0] {
            BusEvent::SessionInit { session_id, .. } => {
                assert_eq!(session_id.as_deref(), Some("019e4159-ca48"));
            }
            other => panic!("expected SessionInit, got {:?}", other),
        }
        assert_eq!(s.captured_thread_id(), Some("019e4159-ca48"));
    }

    #[test]
    fn thread_started_session_init_is_only_sent_once() {
        let mut s = state();
        let _ = s.map_event(&json!({"type":"thread.started","thread_id":"a"}));
        let events2 = s.map_event(&json!({"type":"thread.started","thread_id":"a"}));
        assert!(events2.is_empty());
    }

    #[test]
    fn turn_started_emits_run_state_running() {
        let mut s = state();
        let events = s.map_event(&json!({"type":"turn.started"}));
        assert_eq!(events.len(), 1);
        match &events[0] {
            BusEvent::RunState { state, .. } => assert_eq!(state, "running"),
            other => panic!("expected RunState, got {:?}", other),
        }
    }
}
```

- [ ] **Step 4: 跑测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml codex_state:: -- --nocapture`
Expected: 三个测试全 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/agent/executor/codex_state.rs src-tauri/src/agent/executor/mod.rs
git commit -m "feat: add CodexProtocolState skeleton with thread.started + turn.started"
```

---

### Task 6: 状态机扩展 — `item.started(command_execution)` → ToolStart

**Files:**
- Modify: `src-tauri/src/agent/executor/codex_state.rs`

- [ ] **Step 1: 写失败测试**

在 `codex_state.rs` 的 `mod tests` 内加:

```rust
#[test]
fn item_started_command_execution_emits_tool_start() {
    let mut s = state();
    let raw = json!({
        "type":"item.started",
        "item":{
            "id":"item_1",
            "type":"command_execution",
            "command":"cargo build",
            "aggregated_output":"",
            "exit_code":null,
            "status":"in_progress"
        }
    });
    let events = s.map_event(&raw);
    assert_eq!(events.len(), 1);
    match &events[0] {
        BusEvent::ToolStart { tool_use_id, tool_name, input, .. } => {
            assert_eq!(tool_use_id, "item_1");
            assert_eq!(tool_name, "bash");
            assert_eq!(input.get("command").and_then(|v| v.as_str()), Some("cargo build"));
        }
        other => panic!("expected ToolStart, got {:?}", other),
    }
}

#[test]
fn item_started_agent_message_is_ignored() {
    let mut s = state();
    let raw = json!({"type":"item.started","item":{"id":"item_0","type":"agent_message"}});
    assert!(s.map_event(&raw).is_empty());
}
```

- [ ] **Step 2: 跑测试,确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml codex_state::tests::item_started -- --nocapture`
Expected: FAIL — 当前 `map_event` 不处理 `item.started`。

- [ ] **Step 3: 实现 item.started 分支**

在 `map_event` 的 match 加分支:

```rust
"item.started" => self.handle_item_started(raw),
```

并新增方法:

```rust
fn handle_item_started(&mut self, raw: &Value) -> Vec<BusEvent> {
    let Some(item) = raw.get("item") else { return Vec::new(); };
    let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if item_type != "command_execution" {
        return Vec::new();
    }
    let Some(item_id) = item.get("id").and_then(|v| v.as_str()) else {
        return Vec::new();
    };
    let command = item.get("command").and_then(|v| v.as_str()).unwrap_or("");
    self.pending_tools.insert(item_id.to_string(), ());
    vec![BusEvent::ToolStart {
        run_id: self.run_id.clone(),
        tool_use_id: item_id.to_string(),
        tool_name: "bash".to_string(),
        input: serde_json::json!({ "command": command }),
        parent_tool_use_id: None,
    }]
}
```

- [ ] **Step 4: 跑测试,确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml codex_state:: -- --nocapture`
Expected: 全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/agent/executor/codex_state.rs
git commit -m "feat: map item.started(command_execution) to ToolStart"
```

---

### Task 7: 状态机扩展 — `item.completed(command_execution)` → ToolEnd

**Files:**
- Modify: `src-tauri/src/agent/executor/codex_state.rs`

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn item_completed_command_execution_success_emits_tool_end() {
    let mut s = state();
    let raw = json!({
        "type":"item.completed",
        "item":{
            "id":"item_1",
            "type":"command_execution",
            "command":"ls",
            "aggregated_output":"file.txt\n",
            "exit_code":0,
            "status":"completed"
        }
    });
    let events = s.map_event(&raw);
    assert_eq!(events.len(), 1);
    match &events[0] {
        BusEvent::ToolEnd { tool_use_id, tool_name, status, output, .. } => {
            assert_eq!(tool_use_id, "item_1");
            assert_eq!(tool_name, "bash");
            assert_eq!(status, "success");
            assert_eq!(output.get("aggregated_output").and_then(|v| v.as_str()), Some("file.txt\n"));
            assert_eq!(output.get("exit_code").and_then(|v| v.as_i64()), Some(0));
        }
        other => panic!("expected ToolEnd, got {:?}", other),
    }
}

#[test]
fn item_completed_command_execution_nonzero_emits_error_status() {
    let mut s = state();
    let raw = json!({
        "type":"item.completed",
        "item":{"id":"item_2","type":"command_execution","aggregated_output":"err","exit_code":2,"status":"completed"}
    });
    let events = s.map_event(&raw);
    match &events[0] {
        BusEvent::ToolEnd { status, .. } => assert_eq!(status, "error"),
        other => panic!("expected ToolEnd, got {:?}", other),
    }
}
```

- [ ] **Step 2: 跑测试,确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml codex_state::tests::item_completed_command_execution -- --nocapture`
Expected: FAIL。

- [ ] **Step 3: 实现 item.completed (command_execution) 分支**

在 `map_event` match 加:

```rust
"item.completed" => self.handle_item_completed(raw),
```

并新增方法:

```rust
fn handle_item_completed(&mut self, raw: &Value) -> Vec<BusEvent> {
    let Some(item) = raw.get("item") else { return Vec::new(); };
    let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let Some(item_id) = item.get("id").and_then(|v| v.as_str()) else {
        return Vec::new();
    };
    match item_type {
        "command_execution" => {
            let exit_code = item.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(-1);
            let aggregated_output = item
                .get("aggregated_output")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let status = if exit_code == 0 { "success" } else { "error" };
            self.pending_tools.remove(item_id);
            vec![BusEvent::ToolEnd {
                run_id: self.run_id.clone(),
                tool_use_id: item_id.to_string(),
                tool_name: "bash".to_string(),
                output: serde_json::json!({
                    "aggregated_output": aggregated_output,
                    "exit_code": exit_code,
                }),
                status: status.to_string(),
                duration_ms: None,
                parent_tool_use_id: None,
                tool_use_result: None,
            }]
        }
        _ => Vec::new(),
    }
}
```

- [ ] **Step 4: 跑测试,确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml codex_state:: -- --nocapture`
Expected: 全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/agent/executor/codex_state.rs
git commit -m "feat: map item.completed(command_execution) to ToolEnd"
```

---

### Task 8: 状态机扩展 — `item.completed(agent_message)` → MessageDelta + MessageComplete

**Files:**
- Modify: `src-tauri/src/agent/executor/codex_state.rs`

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn item_completed_agent_message_emits_delta_and_complete() {
    let mut s = state();
    let raw = json!({
        "type":"item.completed",
        "item":{"id":"item_3","type":"agent_message","text":"Hello world"}
    });
    let events = s.map_event(&raw);
    assert_eq!(events.len(), 2);
    match &events[0] {
        BusEvent::MessageDelta { text, .. } => assert_eq!(text, "Hello world"),
        other => panic!("expected MessageDelta, got {:?}", other),
    }
    match &events[1] {
        BusEvent::MessageComplete { message_id, text, .. } => {
            assert_eq!(message_id, "item_3");
            assert_eq!(text, "Hello world");
        }
        other => panic!("expected MessageComplete, got {:?}", other),
    }
}
```

- [ ] **Step 2: 跑测试,确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml codex_state::tests::item_completed_agent_message -- --nocapture`
Expected: FAIL。

- [ ] **Step 3: 在 handle_item_completed 加 agent_message 分支**

```rust
"agent_message" => {
    let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if text.is_empty() {
        return Vec::new();
    }
    vec![
        BusEvent::MessageDelta {
            run_id: self.run_id.clone(),
            text: text.clone(),
            parent_tool_use_id: None,
        },
        BusEvent::MessageComplete {
            run_id: self.run_id.clone(),
            message_id: item_id.to_string(),
            text,
            parent_tool_use_id: None,
            model: None,
            stop_reason: None,
            message_usage: None,
        },
    ]
}
```

- [ ] **Step 4: 跑测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml codex_state:: -- --nocapture`
Expected: 全 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/agent/executor/codex_state.rs
git commit -m "feat: map agent_message item.completed to MessageDelta+MessageComplete"
```

---

### Task 9: 状态机扩展 — `turn.completed` / `turn.failed` 与 pending_tools 清空

**Files:**
- Modify: `src-tauri/src/agent/executor/codex_state.rs`

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn turn_completed_emits_usage_and_idle_and_clears_pending() {
    let mut s = state();
    // 先触发 ToolStart 让 pending_tools 有内容
    let _ = s.map_event(&json!({"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"x"}}));
    assert_eq!(s.pending_tools.len(), 1);

    let raw = json!({
        "type":"turn.completed",
        "usage":{"input_tokens":100,"cached_input_tokens":50,"output_tokens":20,"reasoning_output_tokens":10}
    });
    let events = s.map_event(&raw);
    assert_eq!(events.len(), 2);
    match &events[0] {
        BusEvent::UsageUpdate { input_tokens, output_tokens, cache_read_tokens, .. } => {
            assert_eq!(*input_tokens, 100);
            assert_eq!(*output_tokens, 20);
            assert_eq!(*cache_read_tokens, Some(50));
        }
        other => panic!("expected UsageUpdate, got {:?}", other),
    }
    match &events[1] {
        BusEvent::RunState { state, .. } => assert_eq!(state, "idle"),
        other => panic!("expected RunState idle, got {:?}", other),
    }
    assert!(s.pending_tools.is_empty());
    assert!(!s.turn_active);
    assert!(s.has_seen_turn_completed());
}

#[test]
fn turn_failed_emits_failed_state_and_clears_pending() {
    let mut s = state();
    let _ = s.map_event(&json!({"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"x"}}));
    assert_eq!(s.pending_tools.len(), 1);

    let events = s.map_event(&json!({"type":"turn.failed","error":{"message":"oops"}}));
    assert_eq!(events.len(), 1);
    match &events[0] {
        BusEvent::RunState { state, error, .. } => {
            assert_eq!(state, "failed");
            assert!(error.as_deref().unwrap_or("").contains("oops"));
        }
        other => panic!("expected RunState failed, got {:?}", other),
    }
    assert!(s.pending_tools.is_empty());
    assert!(!s.turn_active);
}

#[test]
fn cross_turn_item_id_reuse_after_failure_does_not_panic() {
    let mut s = state();
    let _ = s.map_event(&json!({"type":"turn.started"}));
    let _ = s.map_event(&json!({"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"x"}}));
    let _ = s.map_event(&json!({"type":"turn.failed","error":{"message":"e"}}));
    let _ = s.map_event(&json!({"type":"turn.started"}));
    let events = s.map_event(&json!({"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"y"}}));
    // Second turn's item_0 must produce a fresh ToolStart
    assert_eq!(events.len(), 1);
    match &events[0] {
        BusEvent::ToolStart { input, .. } => {
            assert_eq!(input.get("command").and_then(|v| v.as_str()), Some("y"));
        }
        other => panic!("expected ToolStart, got {:?}", other),
    }
}

#[test]
fn unknown_event_type_returns_empty() {
    let mut s = state();
    assert!(s.map_event(&json!({"type":"some.unknown.thing"})).is_empty());
}
```

- [ ] **Step 2: 跑测试,确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml codex_state::tests::turn_ -- --nocapture`
Expected: FAIL。

- [ ] **Step 3: 实现 turn.completed / turn.failed 分支**

在 `map_event` match 加:

```rust
"turn.completed" => self.handle_turn_completed(raw),
"turn.failed" => self.handle_turn_failed(raw),
```

新增方法:

```rust
fn handle_turn_completed(&mut self, raw: &Value) -> Vec<BusEvent> {
    let usage = raw.get("usage");
    let input_tokens = usage
        .and_then(|u| u.get("input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output_tokens = usage
        .and_then(|u| u.get("output_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cache_read = usage
        .and_then(|u| u.get("cached_input_tokens"))
        .and_then(|v| v.as_u64());

    self.pending_tools.clear();
    self.turn_active = false;
    self.seen_turn_completed_count += 1;

    vec![
        BusEvent::UsageUpdate {
            run_id: self.run_id.clone(),
            input_tokens,
            output_tokens,
            cache_read_tokens: cache_read,
            cache_write_tokens: None,
            total_cost_usd: 0.0,
            turn_index: None,
            model_usage: None,
            duration_api_ms: None,
            duration_ms: None,
            num_turns: None,
            stop_reason: None,
            service_tier: None,
        },
        BusEvent::RunState {
            run_id: self.run_id.clone(),
            state: "idle".to_string(),
            exit_code: None,
            error: None,
        },
    ]
}

fn handle_turn_failed(&mut self, raw: &Value) -> Vec<BusEvent> {
    let error = raw
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("Codex turn failed")
        .to_string();
    self.pending_tools.clear();
    self.turn_active = false;
    vec![BusEvent::RunState {
        run_id: self.run_id.clone(),
        state: "failed".to_string(),
        exit_code: None,
        error: Some(error),
    }]
}
```

⚠️ 注意:`UsageUpdate` 的字段(包括 `service_tier`)要严格匹配 `models.rs:1295-1325` 当前定义。如有不匹配,先 `Read` `models.rs` 这一段对齐字段名再实现。

- [ ] **Step 4: 跑测试,确认全 PASS**

Run: `cargo test --manifest-path src-tauri/Cargo.toml codex_state:: -- --nocapture`
Expected: 全 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/agent/executor/codex_state.rs
git commit -m "feat: handle turn.completed/turn.failed and clear pending_tools"
```

---

### Task 10: 改造 `spawn.rs::build_agent_command` / `build_agent_resume_command`

**Files:**
- Modify: `src-tauri/src/agent/spawn.rs`

- [ ] **Step 1: 翻转 + 新增断言(先写失败测试)**

打开 `src-tauri/src/agent/spawn.rs`,改测试:

修改 `builds_codex_native_bypass_and_add_dir_args`(line 167-183):
```rust
assert_eq!(command, "codex");
assert!(args.contains(&"exec".to_string()));
assert!(args.contains(&"--json".to_string()));
assert!(args.contains(&"--skip-git-repo-check".to_string()));
assert!(args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
assert!(!args.contains(&"--no-alt-screen".to_string()));
assert!(args.windows(2).any(|w| w == ["--add-dir", "D:/shared"]));
assert!(args.windows(2).any(|w| w == ["--model", "gpt-5.5"]));
assert_eq!(args.last().map(String::as_str), Some("Fix it"));
```

修改 `builds_codex_resume_latest_without_exec`(line 226-235),并把测试名改为 `builds_codex_resume_with_thread_id`:
```rust
#[test]
fn builds_codex_resume_with_thread_id() {
    let (command, args) = build_agent_resume_command(
        "codex",
        "Continue work",
        &settings(None),
        "019e4113-8979-7000-aaaa-bbbbbbbbbbbb",
    )
    .expect("codex resume command");

    assert_eq!(command, "codex");
    assert!(args.contains(&"exec".to_string()));
    assert!(args.contains(&"resume".to_string()));
    assert!(args.contains(&"--json".to_string()));
    assert!(args.contains(&"019e4113-8979-7000-aaaa-bbbbbbbbbbbb".to_string()));
    assert!(!args.contains(&"--last".to_string()));
    assert_eq!(args.last().map(String::as_str), Some("Continue work"));
}
```

修改 `codex_native_bypass_flag_is_not_duplicated_from_extra_args`(line 186-212),把 extra_args 加入 `--json` 与 `--skip-git-repo-check` 验证去重:
```rust
s.extra_args = vec![
    "--dangerously-bypass-approvals-and-sandbox".to_string(),
    "--no-alt-screen".to_string(),  // 用户加这个,我们应不传(不在 exec 模式)
    "--yolo".to_string(),
    "--json".to_string(),
    "--skip-git-repo-check".to_string(),
    "--search".to_string(),
];
// ... 现有断言 + 新增:
assert_eq!(args.iter().filter(|a| a.as_str() == "--json").count(), 1);
assert_eq!(args.iter().filter(|a| a.as_str() == "--skip-git-repo-check").count(), 1);
```

修改 `codex_plan_mode_omits_bypass_flag`(line 237-248),不再断言 `--no-alt-screen` 出现,改为只断言 `--json`:
```rust
assert!(!args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
assert!(args.contains(&"--json".to_string()));
assert!(args.windows(2).any(|w| w == ["--model", "gpt-5.5"]));
```

- [ ] **Step 2: 跑测试,确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml spawn:: -- --nocapture`
Expected: FAIL — 现有实现还没改。

- [ ] **Step 3: 改 build_codex_base_args**

把 `build_codex_base_args` 改为(注意删除 `--no-alt-screen`,加入 `--json` / `--skip-git-repo-check`):

```rust
fn build_codex_base_args(settings: &AdapterSettings) -> Vec<String> {
    let plan_mode = settings.permission_mode.as_deref() == Some("plan");
    let mut args: Vec<String> = vec!["exec".to_string()];
    args.push("--json".to_string());
    args.push("--skip-git-repo-check".to_string());
    if !plan_mode {
        args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
    }
    if let Some(ref m) = settings.model {
        if !m.is_empty() {
            args.push("--model".to_string());
            args.push(m.to_string());
        }
    }
    for dir in &settings.add_dirs {
        args.push("--add-dir".to_string());
        args.push(dir.to_string());
    }
    if settings.no_session_persistence {
        args.push("--ephemeral".to_string());
    }
    append_extra_args_without_controlled_flags(
        &mut args,
        &settings.extra_args,
        &[
            "exec",
            "--json",
            "--skip-git-repo-check",
            "--dangerously-bypass-approvals-and-sandbox",
            "--no-alt-screen",
            "--yolo",
        ],
        &[],
    );
    args
}
```

- [ ] **Step 4: 改 build_agent_resume_command 签名加 thread_id**

```rust
pub fn build_agent_resume_command(
    agent: &str,
    prompt: &str,
    settings: &AdapterSettings,
    thread_id: &str,
) -> Result<(String, Vec<String>), String> {
    match agent {
        "codex" => {
            let mut args = build_codex_base_args(settings);
            // base_args 已含 "exec";在 exec 之后插 "resume <tid>"
            // 找到 exec 的位置,在它后面加
            let exec_pos = args.iter().position(|a| a == "exec").unwrap_or(0);
            args.insert(exec_pos + 1, "resume".to_string());
            args.insert(exec_pos + 2, thread_id.to_string());
            if !prompt.is_empty() {
                args.push(prompt.to_string());
            }
            Ok((native_command("codex", settings), args))
        }
        _ => Err(format!("Resume latest is unsupported for agent: {}", agent)),
    }
}
```

- [ ] **Step 5: 跑测试,确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml spawn:: -- --nocapture`
Expected: 全 PASS。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/agent/spawn.rs
git commit -m "feat: build codex commands with --json / exec / thread_id resume"
```

---

### Task 11: 实现 `CodexExecutor::run` 进程驱动主体

**Files:**
- Modify: `src-tauri/src/agent/executor/codex.rs`

- [ ] **Step 1: 写完整实现**

```rust
// src-tauri/src/agent/executor/codex.rs
use super::codex_state::CodexProtocolState;
use super::{Executor, ExecutorRequest};
use crate::agent::adapter;
use crate::agent::stream::ProcessMap;
use crate::models::{BusEvent, ChatDone, ConversationRef, RunEventType, RunStatus};
use crate::process_ext::HideConsole;
use crate::storage;
use serde_json::Value;
use std::process::Stdio;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub struct CodexExecutor;

#[async_trait::async_trait]
impl Executor for CodexExecutor {
    async fn run(
        &self,
        app: AppHandle,
        process_map: ProcessMap,
        req: ExecutorRequest,
    ) -> Result<(), String> {
        let ExecutorRequest {
            run_id,
            cwd,
            spawn_env_plan,
            display_command,
            process_command,
            args,
            ..
        } = req;

        let _ = storage::events::append_event(
            &run_id,
            RunEventType::System,
            serde_json::json!({
                "message": format!("Started {}", display_command),
                "source": "ui_chat"
            }),
        );

        let mut cmd = Command::new(&process_command);
        cmd.args(&args)
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(path) = &spawn_env_plan.path_override {
            cmd.env("PATH", path);
        }
        for key in adapter::auth_env_removals_for_extra_env(&spawn_env_plan.msvc_env) {
            cmd.env_remove(key);
        }
        for (key, value) in &spawn_env_plan.msvc_env {
            cmd.env(key, value);
        }

        let mut child = cmd
            .env("CLAW_GO_TASK_ID", &run_id)
            .env("CLAW_GO_RUN_ID", &run_id)
            .env_remove("CLAUDECODE")
            .hide_console()
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    "Codex CLI not found. Please install codex and ensure it is in your PATH."
                        .to_string()
                } else {
                    e.to_string()
                }
            })?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        {
            let mut map = process_map.lock().await;
            map.insert(run_id.clone(), child);
        }

        // stderr reader (background)
        let app_err = app.clone();
        let run_id_err = run_id.clone();
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = storage::events::append_event(
                    &run_id_err,
                    RunEventType::Stderr,
                    serde_json::json!({"text": line, "source": "ui_chat"}),
                );
                let _ = app_err.emit(
                    "run-event",
                    serde_json::json!({"run_id": run_id_err, "type": "stderr", "text": line}),
                );
            }
        });

        // stdout JSONL parsing loop
        let mut state = CodexProtocolState::new(run_id.clone());
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = storage::events::append_event(
                &run_id,
                RunEventType::Stdout,
                serde_json::json!({"text": line, "source": "ui_chat"}),
            );
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(_) => {
                    let _ = app.emit(
                        "run-event",
                        serde_json::json!({"run_id": run_id, "type": "stdout", "text": line}),
                    );
                    continue;
                }
            };

            // Persist conversation_ref as soon as thread.started arrives,
            // before going through the state machine, so it's durable even if
            // the turn fails mid-stream.
            if value.get("type").and_then(|v| v.as_str()) == Some("thread.started") {
                if let Some(tid) = value.get("thread_id").and_then(|v| v.as_str()) {
                    let tid_str = tid.to_string();
                    let rid = run_id.clone();
                    if let Err(e) = storage::runs::with_meta(&rid, |meta| {
                        meta.conversation_ref = Some(ConversationRef::CodexThread(tid_str));
                        Ok(())
                    }) {
                        log::warn!("[codex] failed to persist conversation_ref: {}", e);
                    }
                }
            }

            for ev in state.map_event(&value) {
                emit_bus_event(&app, &run_id, ev);
            }
        }

        let _ = stderr_handle.await;

        // Stop / exit reconciliation (uses ProcessMap.contains_key as the stop signal)
        let was_killed_by_stop = {
            let map = process_map.lock().await;
            !map.contains_key(&run_id)
        };
        let removed = {
            let mut map = process_map.lock().await;
            map.remove(&run_id)
        };
        let exit_code = if let Some(mut child) = removed {
            child.wait().await.ok().and_then(|s| s.code()).unwrap_or(1)
        } else {
            -1
        };
        let saw_turn_completed = state.has_seen_turn_completed();

        let (status, code, error) = if was_killed_by_stop {
            (RunStatus::Stopped, -1, Some("Stopped by user".to_string()))
        } else if exit_code == 0 && saw_turn_completed {
            (RunStatus::Completed, 0, None)
        } else if exit_code == 0 {
            (
                RunStatus::Failed,
                1,
                Some("Codex exited before turn completion".to_string()),
            )
        } else {
            (
                RunStatus::Failed,
                exit_code,
                Some(format!("Codex exited with code {exit_code}")),
            )
        };

        if let Err(e) = storage::runs::update_status(
            &run_id,
            status.clone(),
            Some(code),
            error.clone(),
        ) {
            log::warn!("[codex] failed to update status: {}", e);
        }

        let _ = storage::events::append_event(
            &run_id,
            RunEventType::System,
            serde_json::json!({
                "message": format!("Process exited with code {}", code),
                "source": "ui_chat"
            }),
        );

        let _ = app.emit(
            "chat-done",
            ChatDone {
                ok: status == RunStatus::Completed,
                code,
                error,
            },
        );

        Ok(())
    }
}

fn emit_bus_event(app: &AppHandle, _run_id: &str, ev: BusEvent) {
    // 现有事件流约定:bus 事件通过单独通道转发;这里走 run-event 兼容
    let _ = app.emit("bus-event", &ev);
    if let BusEvent::MessageDelta { text, .. } = &ev {
        let _ = app.emit(
            "chat-delta",
            crate::models::ChatDelta { text: text.clone() },
        );
    }
}
```

⚠️ 注意:`emit_bus_event` 的实际通道名称(`bus-event` / `run-event`)以现有 `stream.rs` 与 session_actor 的发射方式为准——实施时 `Grep` `app.emit("bus-event"` 与 `app.emit("run-event"` 在 session_actor 找用法,对齐当前规范。如果现有 Codex 路径(`stream.rs:246-253`)是发 `run-event` 的 stdout 类型,新代码不破坏这个约定即可。

- [ ] **Step 2: 编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。可能有 unused warnings,允许。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/agent/executor/codex.rs
git commit -m "feat: implement CodexExecutor JSONL process loop"
```

---

### Task 12: 把 `stream.rs::run_agent` 改为分发器 + 删 native_pty/pipe_parser/codex_parser 引用 + 注册 executor 模块(原子提交)

**Files:**
- Modify: `src-tauri/src/agent/stream.rs`
- Modify: `src-tauri/src/agent/mod.rs`

⚠️ **此任务的所有改动必须在一个 commit 里完成**——中间状态 stream.rs 仍然 `use native_pty` 但 `agent/mod.rs` 已删 `pub mod native_pty` 会编译不过。

- [ ] **Step 1: 改 stream.rs::run_agent 为分发器**

把 `run_agent`(line 112-439)的函数体替换为:

```rust
#[allow(clippy::too_many_arguments)]
pub async fn run_agent(
    app: AppHandle,
    process_map: ProcessMap,
    run_id: String,
    command: String,
    args: Vec<String>,
    cwd: String,
    agent: String,
    spawn_env_plan: SpawnEnvPlan,
) -> Result<(), String> {
    let display_command = format_started_command(&command, &args);
    let process_command = resolve_process_command(&command);
    let (process_command, args) = resolve_spawn_invocation(process_command, args);

    let executor = crate::agent::executor::for_agent(&agent)?;

    let req = crate::agent::executor::ExecutorRequest {
        run_id: run_id.clone(),
        cwd,
        agent: agent.clone(),
        spawn_env_plan,
        display_command,
        process_command,
        args,
    };

    executor.run(app, process_map, req).await
}
```

- [ ] **Step 2: 删 stream.rs 顶部的废弃 import**

```rust
// 删除这两行
use crate::agent::native_pty;
use crate::agent::pipe_parser::{CodexStdoutParser, PipeStdoutParser};
```

- [ ] **Step 3: 删 stop_process 中的 native_pty 调用**

打开 `stream.rs:441-461`,删去:

```rust
if native_pty::stop_native_pty_process(run_id) {
    log::debug!("[stream] stop_process: killed native pty run_id={}", run_id);
    return true;
}
```

- [ ] **Step 4: 在 agent/mod.rs 删除 dead 模块声明**

```rust
// 删除以下三行
pub mod codex_parser;
pub mod native_pty;
pub mod native_transcript;
pub mod pipe_parser;
```

- [ ] **Step 5: 编译 + 跑全部 Rust 单元测试**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过,无引用错误。

Run: `cargo test --manifest-path src-tauri/Cargo.toml -- --nocapture`
Expected: stream.rs / spawn.rs / codex_state.rs 测试全 PASS。

- [ ] **Step 6: 单一原子 commit**

```bash
git add src-tauri/src/agent/stream.rs src-tauri/src/agent/mod.rs
git commit -m "refactor: dispatch run_agent through Executor trait, remove pipe_parser/native_pty/codex_parser references

native_pty/native_transcript/pipe_parser/codex_parser modules are no longer
referenced. Files removed in next commit."
```

---

### Task 13: 改造 chat resume 调用方让后端从 RunMeta 自查 thread_id

**Files:**
- Modify: `src-tauri/src/commands/chat.rs`

- [ ] **Step 1: 改 send_chat_message 的 build_agent_resume_command 调用**

打开 `commands/chat.rs:240-249`,把 `build_agent_resume_command` 的调用改为先读 RunMeta:

```rust
let (command, args) = if resume_latest.unwrap_or(false) {
    let thread_id = match run.conversation_ref.as_ref() {
        Some(crate::models::ConversationRef::CodexThread(tid)) => tid.clone(),
        Some(other) => {
            return Err(format!(
                "resume requested but conversation_ref is not a Codex thread: {:?}",
                other
            ));
        }
        None => {
            return Err(
                "resume requested but no Codex thread is recorded for this run".to_string(),
            );
        }
    };
    build_agent_resume_command(&run.agent, &full_prompt, &adapter_settings, &thread_id)?
} else {
    build_agent_command(&run.agent, &full_prompt, &adapter_settings, true)?
};
```

⚠️ 实施时 `Read` `models.rs` 确认 `ConversationRef::CodexThread` 的精确变体名与字段访问方式;若枚举变体名不同(例如 `Codex` / `Thread`),按代码事实调整。

- [ ] **Step 2: 编译 + 测试**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/chat.rs
git commit -m "feat: chat resume reads thread_id from RunMeta.conversation_ref"
```

---

### Task 14: 前端 Resume 门控 — `canResumeStructurally` 加 codex_thread 分支

**Files:**
- Modify: `src/lib/stores/types.ts`
- Modify: `src/lib/stores/session-store.test.ts` (扩展现有测试)

- [ ] **Step 1: 扩展 canResumeRun 测试覆盖 Codex thread**

打开 `src/lib/stores/session-store.test.ts:3320` 附近,加新测试:

```typescript
it("allows resume for Codex pipe_exec runs with conversation_ref", () => {
  expect(
    canResumeRun(
      {
        execution_path: "pipe_exec",
        conversation_ref: { kind: "codex_thread", id: "019e4113-…" },
        status: "completed",
      },
      "completed",
    ),
  ).toBe(true);
});

it("rejects resume for pipe_exec runs without conversation_ref", () => {
  expect(
    canResumeRun(
      { execution_path: "pipe_exec", status: "completed" },
      "completed",
    ),
  ).toBe(false);
});
```

⚠️ 测试中 `conversation_ref.kind` 的值要与后端 serde 序列化对齐。Run `Grep "conversation_ref"` in `src/` to confirm the canonical lowercase form actually used by the IPC. 如果是 `"CodexThread"` 而不是 `"codex_thread"`,按代码事实改测试。

- [ ] **Step 2: 跑测试,确认失败**

Run: `npm test -- src/lib/stores/session-store.test.ts -t "canResumeRun"`
Expected: 新两条 FAIL。

- [ ] **Step 3: 改 canResumeStructurally**

打开 `src/lib/stores/types.ts:194-210`,把 path 检查放宽:

```typescript
export function canResumeStructurally(
  run: {
    session_id?: string;
    status?: string;
    execution_path?: string;
    conversation_ref?: { kind: string; id: string };
  } | null,
  phase: SessionPhase,
): boolean {
  const hasRef = run?.conversation_ref != null || !!run?.session_id;
  if (!hasRef) return false;

  const path = run?.execution_path ?? (run?.session_id ? "session_actor" : null);
  const isSessionActor = path === "session_actor";
  const isCodexThread =
    path === "pipe_exec" && run?.conversation_ref?.kind === "codex_thread";

  if (!isSessionActor && !isCodexThread) return false;
  if (ACTIVE_PHASES.includes(phase)) return false;
  return TERMINAL_PHASES.includes(phase);
}
```

- [ ] **Step 4: 跑测试,确认全部 PASS**

Run: `npm test -- src/lib/stores/session-store.test.ts`
Expected: 全 PASS,包括新增两条。

- [ ] **Step 5: Commit**

```bash
git add src/lib/stores/types.ts src/lib/stores/session-store.test.ts
git commit -m "feat: allow resume for Codex pipe_exec runs with conversation_ref"
```

---

### Task 15: 前端 `resumeSession` 放宽 session_id 检查 + 清理 `_pendingNativeResumeLatest` 标志

**Files:**
- Modify: `src/lib/stores/session-store.svelte.ts`

- [ ] **Step 1: 检查 resumeSession 现有 session_id 检查**

Run: `Grep "session_id available for resume" src/lib/stores/session-store.svelte.ts`
Expected: 找到当前的硬性检查行(spec 中提到约 line 2011-2013)。

- [ ] **Step 2: 修改检查为允许 Codex 走 conversation_ref**

在 `resumeSession` 内,把 `if (!run.session_id) throw ...` 改为:

```typescript
const hasResumeRef =
  !!run.session_id ||
  (run.execution_path === "pipe_exec" && !!run.conversation_ref);
if (!hasResumeRef) {
  throw new Error("No session_id or conversation_ref available for resume");
}
```

⚠️ 实施时先 `Read` 当前 resumeSession 的 200 行上下文,确认变量名(`run` vs `this.run`)与 throw 风格一致。

- [ ] **Step 3: `_pendingNativeResumeLatest` — 保留语义,改名更准确**

`_pendingNativeResumeLatest` 的语义本来是"下一次 sendChatMessage 用 resume 路径";因为后端现在自查 conversation_ref,前端只需把 `resumeLatest: true` 透传给 `api.sendChatMessage` 即可。当前调用方已经做了这件事(`session-store.svelte.ts:1877-1885`),所以**不需要修改 sendChatMessage 内部**——只把字段重命名为 `_pendingResumeLatest`(去掉 "Native" 字样,因为 PTY 已死),纯 cosmetic。

```typescript
private _pendingResumeLatest = false;
```

并替换文件内所有 `_pendingNativeResumeLatest` 引用(用 IDE 重命名或 `replace_all` 编辑)。

- [ ] **Step 4: 编译 + 跑前端测试**

Run: `npm run check`
Expected: 通过。

Run: `npm test -- src/lib/stores/`
Expected: 全 PASS。

- [ ] **Step 5: Commit**

```bash
git add src/lib/stores/session-store.svelte.ts
git commit -m "feat: resumeSession accepts Codex conversation_ref + rename pending flag"
```

---

### Task 16: i18n keys

**Files:**
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

- [ ] **Step 1: 在 en.json 加两条 key**

按字母顺序在 `chat_*` 与 `errors_*` 段(若有)合适位置插入:

```json
"chat_resumeUnavailableNoThread": "No Codex thread available to resume.",
"errors_codexCliNotInstalled": "Codex CLI not found. Please install codex and ensure it is in your PATH.",
```

- [ ] **Step 2: 在 zh-CN.json 加同名 key**

```json
"chat_resumeUnavailableNoThread": "没有可恢复的 Codex 会话。",
"errors_codexCliNotInstalled": "未找到 Codex CLI。请安装 codex 并确保其位于 PATH 中。",
```

- [ ] **Step 3: 跑 i18n 检查**

Run: `npm run i18n:check`
Expected: 通过(无 missing/orphaned key 报错)。

- [ ] **Step 4: 把 errors_codexCliNotInstalled 接到后端错误透传**

由于错误从后端传上来是英文字符串(`"Codex CLI not found..."`),前端需要在错误展示路径检测此 substring 并替换为 i18n key。最简方式:在 `commands/chat.rs` 的错误返回处直接返回 key 字符串 `"errors_codexCliNotInstalled"`,前端 chat-done 错误处理处用 `m[errorString]?.()` 翻译;若无对应翻译则原样显示。

实施:在 `executor/codex.rs` 把 `Codex CLI not found...` 字符串改为返回 key:

```rust
.map_err(|e| {
    if e.kind() == std::io::ErrorKind::NotFound {
        "errors_codexCliNotInstalled".to_string()
    } else {
        e.to_string()
    }
})?;
```

前端 chat-done 收到 error 后,先尝试当作 i18n key 查询,失败则原样显示。

⚠️ 如果当前 chat-done 错误展示没有这个查询逻辑,本步只改后端,前端用户看到的依然是 key 字符串——这种情况下,把错误信息组成 `format!("[i18n:errors_codexCliNotInstalled] Codex CLI not found...")` 双语形式,等下次前端国际化重构再统一处理。

- [ ] **Step 5: Commit**

```bash
git add messages/en.json messages/zh-CN.json src-tauri/src/agent/executor/codex.rs
git commit -m "i18n: add codex CLI install + resume unavailable keys"
```

---

### Task 17: 删除 `native_pty.rs` / `native_transcript.rs` / `codex_parser.rs` / `pipe_parser.rs`

**Files:**
- Delete: `src-tauri/src/agent/native_pty.rs`
- Delete: `src-tauri/src/agent/native_transcript.rs`
- Delete: `src-tauri/src/agent/codex_parser.rs`
- Delete: `src-tauri/src/agent/pipe_parser.rs`

- [ ] **Step 1: 确认无任何引用**

Run: `Grep "native_pty\|native_transcript\|codex_parser\|pipe_parser\|CodexStdoutParser\|PipeStdoutParser" src-tauri/`
Expected: 无 hit(全部已在 Task 12 移除)。

- [ ] **Step 2: 删四个文件**

```bash
rm src-tauri/src/agent/native_pty.rs
rm src-tauri/src/agent/native_transcript.rs
rm src-tauri/src/agent/codex_parser.rs
rm src-tauri/src/agent/pipe_parser.rs
```

- [ ] **Step 3: 编译 + 全测试**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Run: `cargo test --manifest-path src-tauri/Cargo.toml -- --nocapture`
Expected: 全 PASS。

- [ ] **Step 4: Commit**

```bash
git add -u src-tauri/src/agent/
git commit -m "refactor: remove dead PTY + pipe_parser + codex_parser modules"
```

---

### Task 18: 更新 CLAUDE.md 第 14 节

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: 替换第 14 节文本**

打开 `CLAUDE.md`,找到 `## 14.` 或 "Notes for future edits" 中关于 native_pty 的段落:

```
The PTY-based native adapter (native_pty.rs + native_transcript.rs) is the canonical execution path for Codex. Do not reintroduce codex exec or pipe-based execution for native CLI providers.
```

替换为:

```
Codex uses the `codex exec --json` JSONL adapter under `agent/executor/codex.rs`. Each turn is a short-lived process; multi-turn continuity is provided by Codex's native `thread_id` (stored in `RunMeta.conversation_ref` as `CodexThread`). Stop is implemented by killing the child via `commands/runs.rs::stop_run`; the JSONL stream truncates cleanly at the last completed event. The Windows `.cmd` shim resolution (`resolve_windows_npm_shim` in `stream.rs`) continues to apply — `CodexExecutor` reuses it via the dispatcher. Do not reintroduce PTY-based execution or `--last`-based resume.
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md note 14 — Codex JSONL adapter replaces PTY"
```

---

### Task 19: 全量验证

**Files:** None (only commands)

- [ ] **Step 1: Lint / format / i18n / tests / build / Rust check / clippy**

Run: `npm run verify`
Expected: 全 PASS。

- [ ] **Step 2: 手动 Codex 跑一次**

```bash
npm run tauri dev
```

在 chat 页面新建 Codex Run,发一句 "运行 ls 并告诉我看到的文件"。

预期可观测行为(对应 spec 的"用户可观测行为"验收清单):
- 用户能在 turn 进行中看到 ToolStart 卡片(命令运行 spinner)。
- 命令完成后看到 ToolEnd 卡片(可展开看 stdout)。
- turn 结束看到 UsageUpdate(token 数)。
- 进程零退出 → RunStatus = Completed。

- [ ] **Step 3: 手动测 Resume**

跑完一个 Codex turn 后,在侧边栏点击该 Run → 看到 Resume 按钮(关键:验证 `canResumeStructurally` 改动生效)→ 输入 "继续刚才的工作" 发送 → 后端走 `exec resume <tid>` → token 累积体现上下文延续。

- [ ] **Step 4: 手动测 Stop**

跑一个长命令 turn,中途点 Stop:
- RunStatus = Stopped(不是 Failed)。
- 前端 chat-done 收到,输入框可用。
- 无 orphan codex 进程(任务管理器确认)。

- [ ] **Step 5: 多 Run 并行 Resume 不串**

新建两个 Codex Run A、B,各跑一句话。Resume Run B 后断言上下文是 B 的、不是 A 的。

- [ ] **Step 6: 群聊 Codex 参与者**

创建一个群聊,加 codex 参与者,发一条 fanout 消息,确认 codex turn 正常输出 BusEvent。

- [ ] **Step 7: 模拟未安装 Codex**

把 `~/.npm/codex.cmd` 临时移走,新建 Codex Run → 用户看到"未找到 Codex CLI"本地化错误信息(中文 / 英文按 i18n)。验证后恢复 codex。

- [ ] **Step 8: Commit 验证**

如果以上手动验证全部通过,本计划完成。无新代码 commit;若发现 bug,在对应 Task 中追加修复并 commit。

---

## 完成后

- 净结果:删除约 1250 行(native_pty.rs / native_transcript.rs / codex_parser.rs / pipe_parser.rs)、新增约 600-700 行(executor/{mod,claude,codex,codex_state}.rs)、净减少约 600 行。
- Codex 实时工具执行可见、精确 thread_id Resume、stop 干净。
- Claude 路径行为与 SessionActor 内部完全保持不变。
- 验收点对照 `docs/superpowers/specs/2026-05-20-codex-jsonl-executor-design.md` 的"验收"段落。
