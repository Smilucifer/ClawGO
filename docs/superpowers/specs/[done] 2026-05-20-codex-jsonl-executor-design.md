---
name: codex-jsonl-executor
description: Replace Codex PTY/transcript path with a JSONL adapter under a new agent::executor module; Claude path is wrapped in the same trait without internal changes.
status: wip
---

# Codex JSONL Executor 设计

## 背景

ClawGO 当前的 Codex 执行路径用 PTY + transcript 文件监控（`agent/native_pty.rs` 570 行 + `agent/native_transcript.rs` 419 行）。这条路径是 Phase 7 时建立的，目的是绕开"老 pipe_exec 把 EOF 当 turn 结束"的语义错配。

PTY 路径的实际局限：

- **无实时输出**：turn 中间用户看不到任何东西，要等进程退出 + transcript 解析完才一次性出文本。
- **工具执行不可见**：Codex 跑 `cargo build` 这类命令时,前端完全黑盒。
- **会话恢复靠 `--last`**：多 Run 并行时 `codex exec resume --last` 会拿到错误的 thread,串到隔壁 Run 的上下文。
- **trust prompt / TTY 控制码处理**：为了应付登录态以外的 edge case,PTY 那一套有 ANSI 解析、`\x1b[6n` 响应、trust prompt 模式匹配等不该出现在执行层的代码。

Spike 验证（2026-05-20）证实 `codex exec --json` 在已登录态下完全够用：

- 输出结构化 JSONL（`thread.started` / `turn.started` / `item.started` / `item.completed` / `turn.completed`）。
- 已登录态下无 TTY 提示,stderr 干净。
- `codex exec resume <thread_id>` 精确续接,token 累积证明上下文延续。
- `kill` 信号干净退出,JSONL 截断在最后一个完整事件,无 orphan。

CLAUDE.md 第 14 节当时反对 pipe_exec 的理由是"进程 EOF = turn 结束"在 PTY 长进程模型下错配——新方案是**每轮一个有界进程 + `thread_id` 串多轮**,该约束的前提消失。

**Codex vs Claude 的 UX 差异(必读)**:Codex 的 `agent_message` 事件在 turn 结尾才到达——长命令期间用户只看到 ToolStart spinner → ToolEnd 输出,文字总结在 `turn.completed` 之前最后一个 `item.completed(agent_message)` 一次性出现。这与 Claude 的"思考-工具-思考"交错模型不同。这不是缺陷,是 Codex CLI 的固有行为,无法通过适配器改变。

## 目标

- 删除 PTY 与 transcript 整套代码（~1000 行）。
- 新建 `agent/executor/` 子模块,以 trait 统一 Claude 和 Codex 两条执行路径。
- Codex 走 `codex exec --json`,实时把 JSONL 事件映射成现有 `BusEvent`。
- Resume 用精确 `thread_id` 而非 `--last`。
- Claude 路径**仅做薄包装**,不触碰 `SessionActor` / `claude_protocol` 内部。

## 非目标

- 不引入 Codex 端的 control protocol / permission / hook / fork（Codex CLI 不支持双向通道,且产品也不要）。
- 不重构 `SessionActor`,也不把 `ProtocolState` 抽成 trait。Claude 内部保持原样。
- 不改 Claude 行为、不动 group chat orchestrator 的 codex 分支语义。
- 不为 Gemini 或其它 CLI 做提前抽象——`Executor` trait 只为当下两个实现服务。

## 范围（受影响角色）

- 普通 `/chat` 中的 Codex 会话（新起 / Resume / Stop）。
- 群聊参与者中 `agent == "codex"` 的执行（group_chat orchestrator 把它当一次性 turn 来调）。
- **前端 Resume 门控** —— `canResumeStructurally`(`src/lib/stores/types.ts`) 当前硬要求 `execution_path === "session_actor"`,Codex 是 `PipeExec`,所以不改前端的话 Resume 按钮永远不显示(=新方案的 Resume UX 对用户不可见)。本设计**必须**包含前端改动。
- 不影响 Claude / 任何 Claude-compatible 第三方 provider（DeepSeek、GLM、Custom 等仍走 SessionActor + Claude CLI）。

## 设计

### 1. 模块结构

新建 `src-tauri/src/agent/executor/`,与现有 agent 子模块平级。

```
src-tauri/src/agent/
  executor/
    mod.rs               # Executor trait + ExecutorRequest + for_agent 工厂
    claude.rs            # ClaudeExecutor — 包装 SessionActor 启动
    codex.rs             # CodexExecutor — 跑 codex exec --json,解析 JSONL,发 BusEvent
    codex_state.rs       # Codex JSONL → BusEvent 状态机（纯函数,易测）
```

**为什么 codex 拆两个文件**：`codex.rs` 负责 spawn、stdout 行读取、ProcessMap 注册、stop 信号、thread_id 持久化（I/O 与副作用）;`codex_state.rs` 是纯状态机 `(JsonlEvent, &mut CodexProtocolState) -> Vec<BusEvent>`,可单测,不依赖 tokio / Tauri。镜像 Claude 那边 `session_actor.rs` 与 `claude_protocol.rs` 的拆分关系。

### 2. Executor trait

`agent/executor/mod.rs`:

```rust
use crate::agent::adapter::AdapterSettings;
use crate::agent::stream::ProcessMap;
use crate::agent::windows_msvc_env::SpawnEnvPlan;
use crate::models::ConversationRef;
use std::sync::Arc;
use tauri::AppHandle;

pub struct ExecutorRequest {
    pub run_id: String,
    pub prompt: String,
    pub cwd: String,
    pub agent: String,
    pub settings: AdapterSettings,
    pub spawn_env_plan: SpawnEnvPlan,
    pub display_command: String,
    /// None = 新 Run; Some = Resume(精确 conversation_ref)
    pub resume_from: Option<ConversationRef>,
}

#[async_trait::async_trait]
pub trait Executor: Send + Sync {
    /// 启动一次执行(一个 turn / 一段 Codex 会话)。
    /// 通过 app.emit("run-event"|"chat-delta"|"chat-done") 发 BusEvent / 进度。
    /// 内部负责:进程 spawn、ProcessMap 注册、状态机推进、conversation_ref 持久化、
    ///         RunStatus 更新、System start/exit 事件、kill 钩子的接驳。
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

trait 不暴露 stdin / control（Codex 没有这个能力）。Claude 那边的 control protocol 仍由 `SessionActor` 内部独立处理,**不通过 trait 暴露**。理由:一旦把 control 提到 trait 上,Codex 就被迫给 stub 实现,会污染接口。Control 是 SessionActor 的专属能力,通过 `ActorSessionMap` 直接路由,不走 Executor trait。

### 3. ClaudeExecutor —— 薄包装

`agent/executor/claude.rs`:

```rust
pub struct ClaudeExecutor;

#[async_trait::async_trait]
impl Executor for ClaudeExecutor {
    async fn run(
        &self,
        app: AppHandle,
        process_map: ProcessMap,
        req: ExecutorRequest,
    ) -> Result<(), String> {
        crate::agent::stream::run_claude_pipe_or_session(app, process_map, req).await
    }
}
```

**第一版只把现有 `stream.rs::run_agent` 里的 Claude 分支抽出一个 `run_claude_pipe_or_session`,然后让 ClaudeExecutor 调用。不重构 SessionActor、不动 claude_protocol、不改 stream-json 解析。** Claude 路径行为完全保留(包括 Windows shim、msvc env、kill_on_drop、stop_run 机制)。

`SessionActor` 仍由 `commands/session.rs` 独立启动（actor-backed session 不经过 `stream.rs`）。`ClaudeExecutor` 只服务于经由 `run_agent` 的那条 pipe-exec Claude 路径。这一点放进文件级 doc 注释里讲清楚。

> 备注: B+ 方案的"统一性"是接口层面的,不强求把所有 Claude 启动收敛到 Executor 之下。`SessionActor` 启动路径继续独立——它有自己的 mailbox / actor command / control protocol,塞进 trait 反而失真。Executor trait 服务于"run_agent 的派发点统一",这是当前唯一的实际收益。

### 4. CodexExecutor —— 主要工作量

#### 4.1 spawn

`agent/spawn.rs` 的 `build_agent_command("codex", ...)` 改造：

- 新 Run: `codex exec --json --skip-git-repo-check --dangerously-bypass-approvals-and-sandbox [--model X] [--add-dir X] [--ephemeral] <prompt>`
- Resume: `codex exec resume <thread_id> --json --skip-git-repo-check --dangerously-bypass-approvals-and-sandbox [...] <prompt>`

变更点：

- 移除 `--no-alt-screen`（JSONL 模式不进 alt screen,不需要）。
- 加 `--json`（始终）。
- 加 `--skip-git-repo-check`（chat cwd 不保证是 git 项目）。
- `build_agent_resume_command` 签名加 `thread_id: &str` 参数,生成 `exec resume <thread_id>` 而非 `exec resume --last`。
- `--dangerously-bypass-approvals-and-sandbox`、`--model`、`--add-dir`、`--ephemeral` 全部保留。

调用方（chat）改 `--last` → 传 thread_id：调用方需要先从 `RunMeta.conversation_ref` 取出 `CodexThread(tid)` 字符串。group_chat 路径已确认不调 `build_agent_resume_command`(见 4.7)。

**`--json` / `--skip-git-repo-check` 加入受控 flag 列表**:`spawn.rs:33-42` 的 `append_extra_args_without_controlled_flags` 受控 singleton 列表当前只有 `--dangerously-bypass-approvals-and-sandbox` / `--no-alt-screen` / `--yolo`。本次改造**必须**把 `--json` 和 `--skip-git-repo-check` 也加进去,否则用户在 extra_args 里加 `--json` 会重复出现两次。

#### 4.2 进程管理

- 不用 portable_pty,直接 `tokio::process::Command`,`stdin(Stdio::null()) / stdout(Stdio::piped()) / stderr(Stdio::piped())`,`kill_on_drop(true)`(防止 ProcessMap 被意外清理时进程残留)。
- `.hide_console()` 仍然加(Windows 不弹 cmd 窗口,沿用 `process_ext::HideConsole` trait 的 `CREATE_NO_WINDOW = 0x0800_0000`)。
- 注册进 `ProcessMap`（现有 `Arc<Mutex<HashMap<String, tokio::process::Child>>>`）—— Codex 不再有自己的 `NATIVE_PTY_PROCESSES` 静态 map。
- `stop_run` 复用 `commands/runs.rs:180` 的 stop_run 现有路径(从 ProcessMap 取 child + `child.kill()` + 写 `RunStatus::Stopped`)。CodexExecutor 不重新实现 stop,只负责退出循环后做幂等收尾(见 4.6)。

#### 4.3 stdout 行读取

```rust
let mut lines = BufReader::new(stdout).lines();
let mut state = CodexProtocolState::new(run_id.clone());

while let Some(line) = lines.next_line().await? {
    let trimmed = line.trim();
    if trimmed.is_empty() { continue; }

    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        // 非 JSON 行降级为 stdout RunEvent,不进协议
        emit_raw_stdout(&run_id, &line);
        continue;
    };

    let events = state.map_event(&value);
    for ev in events {
        emit_bus_event(&app, &run_id, ev);
    }
}
```

stderr 单独 reader,逐行存为 `RunEventType::Stderr`。Codex stderr 在已登录态只会出 `Reading additional input from stdin...` banner,不影响。

#### 4.4 状态机（`codex_state.rs`）

```rust
pub struct CodexProtocolState {
    run_id: String,
    /// item.id → 已发出的 BusEvent::ToolStart 对应的 tool_use_id (使用 item.id 即可)
    pending_tools: HashMap<String, String>,
    /// 是否已经发过 SessionInit（资源准备好的标志）
    sent_session_init: bool,
    /// turn.started 后才允许接收 item.*
    turn_active: bool,
}

impl CodexProtocolState {
    pub fn map_event(&mut self, raw: &Value) -> Vec<BusEvent> { ... }
}
```

**事件映射表**：

| Codex JSONL                                                    | 映射到 BusEvent                                        | 备注                              |
| -------------------------------------------------------------- | ------------------------------------------------------ | --------------------------------- |
| `{"type":"thread.started","thread_id":"..."}`                  | `SessionInit { session_id: Some(thread_id), model, .. }` + 持久化 `RunMeta.conversation_ref = CodexThread(tid)` | 仅在首次出现时发 SessionInit          |
| `{"type":"turn.started"}`                                      | `RunState { state: "running" }`                        | 标记 turn_active=true              |
| `{"type":"item.started","item":{"type":"command_execution",...}}` | `ToolStart { tool_use_id: item.id, tool_name: "bash", input: {command} }` | 仅 command_execution 才发,agent_message 跳过 |
| `{"type":"item.completed","item":{"type":"command_execution","exit_code":N,"aggregated_output":"..."}}` | `ToolEnd { tool_use_id: item.id, tool_name: "bash", output, status: "success"\|"error", duration_ms: None }` | exit_code 0 → success,否则 error |
| `{"type":"item.completed","item":{"type":"agent_message","text":"..."}}` | `MessageDelta { text } + MessageComplete { message_id: item.id, text }` | 文本一次到位,delta=完整文本,complete 同步发 |
| `{"type":"turn.completed","usage":{...}}`                       | `UsageUpdate { input_tokens, output_tokens, cache_read_tokens, ... }` + `RunState { state: "idle" }` | turn 结束,turn_active=false,清空 pending_tools |
| `{"type":"turn.failed","error":{...}}`                          | `RunState { state: "failed", error }`                  | 罕见,Codex 内部错误。**必须同步清空 pending_tools + turn_active=false**(否则跨 turn item.id 复用会留下悬空 ToolStart,前端 spinner 永不消失) |
| 其它未知 type                                                  | log::debug,不发 BusEvent                              | 容错降级                          |

**关于工具名称**: Codex 的 `command_execution` 是通用 shell exec,统一映射为 `tool_name = "bash"`,`input.command` 放实际命令字符串。这与 Claude 的 Bash 工具语义一致,前端 ToolCall 卡片可复用现有 Claude Bash 渲染。**重要**: 这里 input 的 schema 简化为 `{"command": "..."}`,Claude Bash 还有 `description`、`timeout` 等字段,Codex 缺这些时前端要能优雅处理 `undefined`(前端验证项)。

**关于 `agent_message`**: Codex 不发 delta,只在 `item.completed` 一次性给完整 text。我们的策略是同时发 `MessageDelta { text }`(让现有 chat-delta 渲染拿到内容) + `MessageComplete { ... }`(标记完成)。前端 timeline 不会重复显示——`MessageComplete` 是按 message_id 收尾,不重新插一条。

**关于 `tool_use_id`**: 直接用 Codex 的 `item.id`(形如 `"item_0"`、`"item_1"`)。`item.id` 在一个 turn 内唯一,跨 turn 会复用——所以 `pending_tools` 必须在 **`turn.completed` 和 `turn.failed`** 两条路径上都清空。

**关于字段名(实施前必须确认)**: 现有 `codex_parser.rs:27` 读的是 `item.output`,本 spec 写为 `item.aggregated_output`——两者哪个是 v0.130 实际字段名,取决于 Codex CLI 版本。**步骤 0**:在写 codex_state.rs 之前,跑一次 spike 抓取真实 `item.completed(command_execution)` 的完整 payload(stdout 重定向到文件即可),确认字段名后回填本 spec 与 codex_state.rs。同样需要确认的是 `item.started(command_execution)` 的 payload 是否包含 `command` 字段(若包含 → 实时拿到命令文本;若只在 `item.completed` 才有 → ToolStart 的 input 留空,等 ToolEnd 一起补)。

**关于 MessageDelta + MessageComplete 双发**: 经过审查确认 `session-store.svelte.ts` 的 dedup 走 `message_id`,`MessageComplete` 是收尾标记,不会重新插入新条。所以双发安全(`MessageDelta` 让现有 chat-delta 渲染拿到内容,`MessageComplete` 标记完成)。**不**改为只发 MessageComplete。

#### 4.5 持久化 conversation_ref

在 `thread.started` 事件到达时立刻通过 `storage::runs` 现有的 RunMeta 更新接口写入 `conversation_ref = Some(ConversationRef::CodexThread(tid))`。具体 API 名称对照 `native_pty.rs:509-517` 当前在 turn 结束后写 conversation_ref 的调用——**新方案把写入点从 turn 结束提前到 thread.started**(更早一拍,即使 turn 中途崩溃也能 Resume)。

错误降级为 `log::warn`,不阻断 turn(沿用现有 PTY 路径的处理模式)。

#### 4.6 Stop 与异常退出

**stop_run 实际机制(代码事实)**:`commands/runs.rs:180` 的 `stop_run` 命令做两件事:(a) `process_map.lock().remove(run_id)` 取出 child 并 `kill()`,(b) 调 `storage::runs::update_status(run_id, RunStatus::Stopped, ...)` 写盘。这是已有的、工作中的机制——pipe 路径没有显式的"stop flag / cancel token",`stream.rs:365-378` 是直接靠 `process_map.remove` 之后 `next_line` 返回 None 来检测的。**CodexExecutor 必须复用同一约定,不要发明新机制。**

CodexExecutor 的退出处理(伪代码):

```text
循环退出后(next_line 返回 None 或 Err):
    let was_killed_by_stop = !process_map.contains_key(&run_id);
    let exit_code = child.wait().await.code();
    let saw_turn_completed = state.has_seen_turn_completed();

    match (was_killed_by_stop, exit_code, saw_turn_completed) {
        (true, _, _)            => Stopped,  // stop_run 已经写过 Stopped,这里幂等再写一次也无妨
        (false, Some(0), true)  => Completed,
        (false, Some(0), false) => Failed("Codex exited before turn completion"),
        (false, _, _)           => Failed(parse_stderr_or("non-zero exit")),
    }

    process_map.remove(&run_id);   // 兜底,防止残留
    emit chat-done { ok, code };   // 必须 emit,否则前端永远 spinning
```

要点:
- **必须 emit `chat-done`** —— 现有 `commands/chat.rs:291-298` 的 chat-done 兜底只在 `run_agent` 返回 Err 时触发,正常退出走 CodexExecutor 自己负责。漏发 = 前端 chat 输入框永久 disabled。
- **`process_map.contains_key`(而非 remove)** 用来判定是否被 stop_run kill;退出时再 remove 一次兜底。
- JSONL 中途解析失败的单行: 降级为 stdout RunEvent + log::warn,不中断 turn。

#### 4.7 group chat 集成

`group_chat::orchestrator` 当前对 codex 参与者的调用最终走到 `agent/stream.rs::run_agent`。trait 化之后,orchestrator 不需要改——它仍然 spawn 一个 Run,Run 走 chat 同一条路径派发到 CodexExecutor。orchestrator 只关心 BusEvent 流。

**Codex resume 调用方调研结论(已 grep 确认)**:`group_chat/orchestrator.rs:845`(`execute_pipe_turn`)只调用 `build_agent_command`,**从未调用 `build_agent_resume_command`**——每次 group chat turn 都是独立的新 Run、新 thread。所以 `build_agent_resume_command` 的调用方**只剩 `/chat` 一处**(`commands/chat.rs` 的 resume 入口),改造范围确定。

### 4.8 前端 Resume 门控改动

**问题**:`src/lib/stores/types.ts:206-207` 的 `canResumeStructurally` 当前长这样:

```ts
const path = run?.execution_path ?? (run?.session_id ? "session_actor" : null);
if (path !== "session_actor") return false;
```

Codex 运行的 `execution_path` 是 `PipeExec`(`models.rs:109` 的 `resolved_execution_path` 对 non-claude agent 返回 `PipeExec`)。结果:不管 `conversation_ref` 有没有,Resume 按钮永远不显示。新方案的"精确 thread_id Resume" UX 目标对用户**完全不可见**——这是个隐藏的、内部不可达的功能。

**改动**:

1. **`canResumeStructurally`**:增加 `execution_path === "pipe_exec" && conversation_ref?.kind === "codex_thread"` 的允许分支。具体语义:Codex 运行只要 `conversation_ref` 是 `CodexThread(_)` 就可 Resume,不再受 `execution_path` 限制。
2. **`resumeSession`(`session-store.svelte.ts:2011-2013` 附近)**:当前的 `if (!session_id) throw "No session_id available for resume"` 会拦截 Codex(Codex 没 session_id,只有 conversation_ref)。改为:Claude 路径仍按 session_id 分支,Codex 路径走 conversation_ref 分支。
3. **`_pendingNativeResumeLatest` 标志(session-store.svelte.ts:2097)**:重命名为 `_pendingCodexResume` 或直接删除,改用后端从 RunMeta 自查 conversation_ref 的语义。
4. **IPC 数据流(选定)**:**后端在 `run_agent` 被调用、且 agent == "codex" 且需要 resume 时,直接从 `RunMeta.conversation_ref` 读出 thread_id,前端无需显式传 thread_id**。这样 IPC 边界最小化,前端只发"resume 这个 run"的语义。`sendChatMessage` IPC 现有的 `resumeLatest: boolean` 参数语义保留(true = 让后端自查并 resume,false = 新 turn)。

### 4.9 i18n keys

新增至少两条:

- `errors_codexCliNotInstalled`(en: "Codex CLI not found. Please install codex and ensure it is in your PATH." / zh-CN: "未找到 Codex CLI。请安装 codex 并确保其位于 PATH 中。")
- `chat_resumeUnavailableNoThread`(en: "No Codex thread available to resume." / zh-CN: "没有可恢复的 Codex 会话。") —— 用作 Resume 按钮 disabled 时的 tooltip。

在 `messages/en.json` 和 `messages/zh-CN.json` 同步加入。`npm run i18n:check` 必须通过。

### 5. stream.rs 收缩

`agent/stream.rs::run_agent` 现状 515 行,职责混杂。改造后:

- 保留 `ProcessMap` 类型定义、`resolve_windows_npm_shim`、`resolve_process_command`、`format_started_command`、`quote_display_arg`。
- `run_agent` 退化为:
  1. 解析 `process_command` + Windows shim。
  2. 构造 `ExecutorRequest`。
  3. `executor::for_agent(&agent)?.run(app, process_map, req).await`。
- 现有 line 230-292 的 Codex JSONL 分支(dead code) **删除**——它会被 CodexExecutor 完整替代,不留过渡。
- 现有 line 144 的 `if native_transcript_mode { return native_pty::... }` **删除**。
- Claude 分支(line 296-end)抽成 `run_claude_pipe_or_session`,被 `ClaudeExecutor` 调用。

净结果: `stream.rs` 大约 ~200 行,只做分发与共享 helper。

### 6. 模块删除

完全删除:

- `src-tauri/src/agent/native_pty.rs`(570 行)
- `src-tauri/src/agent/native_transcript.rs`(419 行)
- `src-tauri/src/agent/codex_parser.rs`(179 行)——逻辑迁移到 `executor/codex_state.rs`,这个旧文件只服务于 dead path
- `src-tauri/src/agent/pipe_parser.rs`(80 行)——`CodexStdoutParser` trait 的唯一实现迁移,trait 一并删除(没有其他实现)

`agent/mod.rs` 移除对应 `pub mod` 声明,新增 `pub mod executor`。

净代码量: 删除约 1250 行,新增约 600-700 行(executor/mod.rs ~80 + claude.rs ~30 + codex.rs ~250 + codex_state.rs ~250),净减少约 600 行。

### 7. CLAUDE.md 更新

第 14 节当前文本:

> The PTY-based native adapter (native_pty.rs + native_transcript.rs) is the canonical execution path for Codex. Do not reintroduce codex exec or pipe-based execution for native CLI providers.

改为:

> Codex uses the `codex exec --json` JSONL adapter under `agent/executor/codex.rs`. Each turn is a short-lived process; multi-turn continuity is provided by Codex's native `thread_id` (stored in `RunMeta.conversation_ref` as `CodexThread`). Stop is implemented by killing the child via `commands/runs.rs::stop_run`; the JSONL stream truncates cleanly at the last completed event. The Windows `.cmd` shim resolution (`resolve_windows_npm_shim` in `stream.rs`) continues to apply — `CodexExecutor` reuses it via the dispatcher. Do not reintroduce PTY-based execution or `--last`-based resume.

## 数据流

```
用户在 /chat 发送 "继续修测试" (Run B,已有 thread_id=019e4113-…)
        │
        ▼
commands/chat.rs → run_agent(...)
        │
        ▼
stream.rs::run_agent
  ├─ 解析 Windows npm shim (codex.cmd → node + codex.js)
  ├─ 准备 SpawnEnvPlan (MSVC env / PATH override)
  └─ executor::for_agent("codex") → CodexExecutor
        │
        ▼
CodexExecutor::run(ExecutorRequest { resume_from: Some(CodexThread("019e4113-…")), ... })
  ├─ build_agent_resume_command("codex", "继续修测试", &settings, thread_id="019e4113-…")
  ├─ tokio Command spawn (.hide_console(), stdout=piped, stdin=null)
  ├─ 注册到 ProcessMap[run_id] = Child
  └─ 启动 stdout reader loop
        │
        ▼
逐行解析 JSONL:
  thread.started{tid=019e4113-…}
      → 写 RunMeta.conversation_ref
      → emit SessionInit { session_id, model }
  turn.started
      → emit RunState { state: "running" }
  item.started { item_0, command_execution, "cargo test ..." }
      → emit ToolStart { tool_use_id: "item_0", tool_name: "bash", input: {command: "..."} }
  item.completed { item_0, command_execution, exit_code: 0, aggregated_output: "..." }
      → emit ToolEnd { tool_use_id: "item_0", tool_name: "bash", output, status: "success" }
  item.completed { item_1, agent_message, text: "..." }
      → emit MessageDelta { text } + MessageComplete { message_id: "item_1", text }
  turn.completed { usage: {...} }
      → emit UsageUpdate { input_tokens, output_tokens, ... }
      → emit RunState { state: "idle" }

进程退出 (exit 0)
      → ProcessMap.remove(run_id)
      → RunStatus::Completed
      → emit chat-done { ok: true, code: 0 }
```

## 错误处理

| 场景                                  | 处理                                                                 |
| ------------------------------------- | -------------------------------------------------------------------- |
| `codex` 命令找不到                    | spawn 返回 NotFound → 用 i18n key `errors_codexCliNotInstalled` 显示,`RunStatus::Failed` |
| 已登录态丢失(未来 token 过期)         | Codex 会在 stderr 报错并非零退出 → 透传,前端按 Failed 处理              |
| JSONL 解析异常单行                   | 降级为 stdout RunEvent + log::warn,不中断 turn                       |
| `thread.started` 未出现就退出       | conversation_ref 不更新,RunMeta 沿用旧值或保持 None;前端 Resume 仍能用旧 tid |
| `turn.completed` 未出现就退出       | `RunStatus::Failed`,error = "Codex exited before turn completion"(没有 UsageUpdate) |
| 用户中途 stop_run                     | `commands/runs.rs::stop_run` 已写 `Stopped` + kill;CodexExecutor 退出循环后 `process_map.contains_key` 返回 false → 走 Stopped 分支(幂等) |
| 进程异常退出(非 stop)                | 解析 stderr 拼成 error,`RunStatus::Failed`                          |
| 进程零退出但未收到 turn.completed     | `RunStatus::Failed`,error = "Codex exited before turn completion"   |
| `build_agent_resume_command` 拿不到 thread_id | 前端责任:Resume 按钮在 `conversation_ref` 为 None 时灰掉 + tooltip 用 i18n key `chat_resumeUnavailableNoThread`;后端额外保险:`resume_from` 为 None 时退化为新 Run |
| CodexExecutor 退出未 emit `chat-done` | 不允许出现——退出分支强制 emit,否则前端 chat 输入框永久 disabled |

## 测试

### 单元测试（`codex_state.rs`）

进 JSONL 字符串,出 BusEvent 列表,断言:

1. `thread.started` → 1 个 `SessionInit`(含 session_id) + 副作用: state 标记 thread_id 已捕获
2. `turn.started` → 1 个 `RunState(running)`
3. `item.started(command_execution)` → 1 个 `ToolStart`,tool_use_id == item.id,tool_name == "bash"
4. `item.completed(command_execution, exit_code=0)` → 1 个 `ToolEnd`,status == "success"
5. `item.completed(command_execution, exit_code=1)` → 1 个 `ToolEnd`,status == "error"
6. `item.completed(agent_message)` → 1 个 `MessageDelta` + 1 个 `MessageComplete`
7. `turn.completed(usage)` → 1 个 `UsageUpdate` + 1 个 `RunState(idle)` + pending_tools 清空
8. `turn.failed` → 1 个 `RunState(failed)` + **pending_tools 清空 + turn_active=false**
9. 未知 type 不产生事件
10. 多 turn 序列 happy path:thread.started → turn.started → ... → turn.completed → turn.started → ... → turn.completed,pending_tools 在每个 turn.completed 后清空
11. **跨 turn item.id 复用 + turn.failed**:turn.started → item.started(item_0) → turn.failed → turn.started → item.started(item_0) → 验证第二个 item_0 不会因为第一个的悬空状态而报错或漏发 ToolStart
12. **进程提前退出**(状态机不直接测,但留出 `state.has_seen_turn_completed()` 接口供 CodexExecutor 判断 Failed("exited before turn completion"))

### 单元测试（`spawn.rs`）

**翻转的现有断言**(实施前需要明确,否则 cargo test 直接红):

- `builds_codex_native_bypass_and_add_dir_args` 测试中的 `assert!(!args.contains("--json"))` → `assert!(args.contains("--json"))`
- 同测试的 `assert!(!args.contains("exec"))` → `assert!(args.contains("exec"))`
- `builds_codex_resume_latest_without_exec` 测试的 `--last` 断言 → 改为断言 `resume <thread_id>`(thread_id 在位置参数位)、不含 `--last`

**新增断言**:

1. `build_agent_command("codex", prompt, settings, true)` 包含 `--json`、`--skip-git-repo-check`、`--dangerously-bypass-approvals-and-sandbox`,`--no-alt-screen` **不**出现
2. `build_agent_resume_command("codex", prompt, settings, "019e4113-…")` 包含 `resume`、`019e4113-…`(thread_id 在位置参数位)、不含 `--last`
3. `--ephemeral` / `--model` / `--add-dir` / `extra_args` 行为保留(沿用现有测试,只验证新增 flag 不破坏旧行为)
4. **受控 flag 列表**:用户在 extra_args 里加 `--json` 或 `--skip-git-repo-check`,最终命令里这两个 flag 仍只出现一次(由 `append_extra_args_without_controlled_flags` 去重)

### 集成测试

- 启动一个真实 Codex turn(只在本地手工/CI 跑,因为依赖已登录的 codex CLI):验证 JSONL 解析全程不报错、conversation_ref 落盘、stop_run 中途打断后 RunStatus 是 Stopped 不是 Failed。
- Resume 测试:跑一次 turn 拿到 tid,再用 `build_agent_resume_command` 调一次,断言进程能起来且 stdout 第一条 thread.started 的 tid 匹配。

### 回归

- 现有 Claude 路径所有测试(`stream.rs` 相关、`session_actor.rs` 相关、`claude_protocol.rs` 相关)继续通过。Claude 走 ClaudeExecutor 包装,行为不变。
- 群聊 codex 参与者的 orchestrator 集成测试(`group_chat/orchestrator.rs` 内的 `tests::create_run_for_agent("run-codex", "codex")` 等)继续通过。

## 实现顺序（写进 plan 时再细化）

0. **Spike 字段名确认**:跑一次 `codex exec --json ...` 抓取真实 `item.started(command_execution)` 与 `item.completed(command_execution)` 的完整 payload(stdout 重定向到文件即可),确认 `command` 字段位置(started 有 / 还是只有 completed 有)、`output` vs `aggregated_output` 实际字段名。把结论回填 4.4 节与 codex_state.rs。**这一步不做完不要写 codex_state.rs**。
1. 加 `async-trait` crate 依赖(如未引入)。
2. 新建 `agent/executor/mod.rs` 的 trait + ExecutorRequest + for_agent。
3. 抽 `stream.rs::run_agent` 的 Claude 分支为 `run_claude_pipe_or_session` 函数（不改逻辑）。
4. 实现 `ClaudeExecutor::run` —— 一行调用,跑通端到端;在此节点验证 Claude 路径无回归。
5. 实现 `codex_state.rs` 状态机 + 全套单元测试(含 turn.failed 清空、跨 turn item_0 复用)。
6. 实现 `CodexExecutor::run` —— spawn + stdout loop + persist conversation_ref + stop(复用 `process_map.contains_key` 判定)+ status + **必发 chat-done**。
7. 改造 `spawn.rs::build_agent_command` / `build_agent_resume_command`(thread_id 参数)+ **翻转现有断言** + 新增断言 + **把 `--json` / `--skip-git-repo-check` 加进 `append_extra_args_without_controlled_flags` 受控列表**。
8. **(单提交,原子)** 改造 `stream.rs::run_agent` 为分发器 + 删 `use native_pty` + 删 `use pipe_parser` + 更新 `agent/mod.rs`(删旧 pub mod、加 `pub mod executor`)。中间状态编译不过,必须打包在同一 commit。
9. 改造 chat resume 调用方(`commands/chat.rs` 的 `resumeLatest` 路径让后端从 `RunMeta.conversation_ref` 自查 thread_id);group_chat 不需要改(已确认)。
10. **前端 Resume 门控改动**:`canResumeStructurally`(types.ts) 加 codex_thread 分支;`resumeSession`(session-store) 放宽 session_id 检查;清理 `_pendingNativeResumeLatest` 标志。
11. **i18n keys**:在 `messages/en.json` / `messages/zh-CN.json` 加 `errors_codexCliNotInstalled` 与 `chat_resumeUnavailableNoThread`,跑 `npm run i18n:check` 验证。
12. 删除 `native_pty.rs` / `native_transcript.rs` / `codex_parser.rs` / `pipe_parser.rs`(此时已无引用)。
13. 更新 CLAUDE.md 第 14 节。
14. 跑 `npm run verify`(lint / format / i18n / tests / build / Rust check / clippy),补任何遗漏的 import。

## 风险

| 风险                                                | 评估                                                                                              |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Codex CLI 升级后 JSONL schema 变化                | 我们映射的事件类型(thread.started / turn.started / item.* / turn.completed)是 v0.130 公开的稳定 schema,小版本不会破坏;升级后回归测试能立刻发现 |
| `aggregated_output` 一次性给完整 stdout,长命令导致单个 ToolEnd 事件巨大 | 监测一次。如成为问题:在 codex_state.rs 加 truncate(比如 64KB),并在 ToolEnd 里加 truncated 标志 |
| 前端 ToolCall 卡片对 Codex 的 "bash" 工具渲染有差异 | tool_name 故意复用 "bash" 让前端复用 Claude 的渲染。若有差异,前端做小适配(展示阶段验证)            |
| Windows .cmd shim 解析跟旧 PTY 路径行为不一致     | 现有 `resolve_windows_npm_shim` 仍生效(它在 stream.rs 顶层,不在 native_pty 里),CodexExecutor 走同一份逻辑 |
| Group chat 里如果有 codex resume 调用方,签名变化要传 tid | 实现阶段 grep 确认;如有调用方,补传 conversation_ref;如无,跳过 |
| Claude `--last` resume 行为改变                    | 不动 — `build_agent_resume_command` 的 codex 分支改签名,Claude 分支不变                                |

## 验收

**代码**:

- 删除文件:`native_pty.rs`、`native_transcript.rs`、`codex_parser.rs`、`pipe_parser.rs` 不再存在。
- `agent/executor/` 下四个文件就位。
- `npm run verify` 通过(lint / format / i18n / tests / build / Rust check / Rust clippy)。
- CLAUDE.md 第 14 节已更新。
- `messages/en.json` 和 `messages/zh-CN.json` 包含 `errors_codexCliNotInstalled` 与 `chat_resumeUnavailableNoThread`。

**用户可观测行为**:

- 一次完整 Codex turn:用户能在 turn 进行中看到 ToolStart 卡片(命令运行 spinner),命令完成后看到 ToolEnd 卡片(可展开看 stdout),turn 结束看到 UsageUpdate(token 数)。
- 多 Run 并行 Codex,Resume Run B 不会拿到 Run C 的上下文(thread_id 精确匹配)。
- **前端 Resume 按钮对 Codex 已完成 Run 显示且可点**(关键:验证 `canResumeStructurally` 改动生效);Resume 后能成功延续 thread,UsageUpdate 的 input_tokens 体现上下文累积。
- Stop 中途打断,RunStatus 为 Stopped(不是 Failed),前端 chat-done 收到、输入框可用,无 orphan 进程。
- 模拟"codex 未安装"(改 PATH 移除 codex)→ 用户看到本地化错误信息(中文 / 英文按 i18n 配置),非裸露的英文 Rust 报错。
- 群聊里 codex 参与者跑一个 turn,行为与现状一致,能正常出 BusEvent。
