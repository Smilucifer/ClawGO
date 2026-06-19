# 批次 A — 记忆系统 bug 修复 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 `/memory` 路由按 cwd 过滤失效(A1),并让记忆自动抽取在普通 `/chat` 会话中生效(A2),不再局限于群聊。

**Architecture:** A1 是单处 serde 注解修复。A2 在 `SessionActor`(普通会话的执行单元)回合完成处接线 fire-and-forget 的 `auto_extract_memories`,新增回合级用户/助手文本缓冲与群聊判别标志,并把抽取的每日上限从全局计数改为 per-source 计数。

**Tech Stack:** Rust(Tauri backend)、tokio、serde、SQLite(memory.db)、reqwest;前端 SvelteKit(仅 A1 涉及类型层,无需改动)。

## Global Constraints

- 关联 spec:`docs/superpowers/specs/2026-06-19-multi-module-maintenance-design.md`(批次 A 节)。
- 本机 Rust 单元测试存在运行时问题 `STATUS_ENTRYPOINT_NOT_FOUND`(CLAUDE.md §11)。**编译验证用 `cargo check --manifest-path src-tauri/Cargo.toml`**;纯函数逻辑可写 `#[cfg(test)]` 单测(用 `cargo test` 尝试,若因该运行时问题无法跑则以 `cargo check` 通过为准并在 commit message 注明)。
- 抽取走 fire-and-forget `tokio::spawn`,不得阻塞回合完成路径。
- Conventional Commits(`fix:` / `feat:`)。
- 记忆抽取的现有去抖(5 分钟 per-key)与每日上限(50)逻辑在 `src-tauri/src/group_chat/memory_extraction.rs`,A2 改为 per-source 计数后,**群聊路径与普通会话路径共用同一套 `can_extract`/`record_extraction`,key 语义统一为 source key**(群聊用 `group_chat_id`,普通会话用 `run_id`)。
- 群聊会话的抽取已由 `group_chat/orchestrator.rs:527-537` 负责,A2 的普通会话抽取**必须跳过群聊会话**,避免重复抽取。

---

## Task 1: 修复 `/memory` cwd 过滤(A1)

**Files:**
- Modify: `src-tauri/src/models.rs:26-34`

**Interfaces:**
- Consumes: 无
- Produces: `MemoryFileCandidate` 序列化为 camelCase(`projectSlug` 等),与前端 `src/lib/types.ts:1-8` 接口对齐。

**根因:** `MemoryFileCandidate` 缺 `#[serde(rename_all = "camelCase")]`,Tauri 把 `project_slug` 序列化为 snake_case,前端按 `projectSlug` 读取得到 `undefined`,导致 `src/routes/memory/+page.svelte:36-40` 与 `src/routes/+layout.svelte:317-320` 的 cwd 过滤恒为 false。

- [ ] **Step 1: 给结构体加 serde 注解**

修改 `src-tauri/src/models.rs:26`,在 `#[derive(...)]` 行下方插入 serde 注解:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryFileCandidate {
    pub path: String,
    pub label: String,
    pub scope: String, // "project" | "global" | "memory" | "global-memory"
    pub provider: Option<String>,
    pub exists: bool,
    pub project_slug: Option<String>, // ~/.claude/projects/{slug} for "memory" scope
}
```

- [ ] **Step 2: 编译验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过,无 error。

- [ ] **Step 3: 确认前端字段对齐(只读核对,不改动)**

确认 `src/lib/types.ts:1-8` 的接口字段为 `projectSlug`(camelCase),与新序列化输出一致。无需改动前端。

- [ ] **Step 4: 手动验证(运行 app)**

启动 `npm run tauri dev`,进入 `/memory`,在左侧选中一个具体 cwd 项,确认其 `~/.claude/projects/<slug>/memory/*.md` 文件正确列出;切换不同 cwd 时列表随之更新;全局/All Projects 模式不回归(仍显示全部)。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models.rs
git commit -m "fix(memory): MemoryFileCandidate 缺 camelCase 注解致 /memory 按 cwd 过滤失效"
```

---

## Task 2: 抽取每日上限改为 per-source 计数(A2 前置)

**Files:**
- Modify: `src-tauri/src/group_chat/memory_extraction.rs:42-82`

**Interfaces:**
- Consumes: 无
- Produces:
  - `can_extract(source_key: &str) -> bool` — 签名不变(参数语义从 group_chat_id 推广为通用 source key);去抖 5 分钟 per-key 不变;每日上限改为 **per-source 50/天**。
  - `record_extraction(source_key: &str)` — 签名不变;记录改为 per-source 计数自增。

**背景:** 当前 `DAILY_EXTRACTION_COUNT`(`memory_extraction.rs:43-44`)是全局单计数 `(String /*date*/, u32 /*count*/)`。普通会话接入后,多个 source 共享 50/天会被群聊挤占。改为 per-source `HashMap<String /*source_key*/, (String /*date*/, u32)>`。

- [ ] **Step 1: 写失败测试(纯函数逻辑)**

在 `src-tauri/src/group_chat/memory_extraction.rs` 末尾追加测试模块。注意:`can_extract` 依赖全局 `LAST_EXTRACTION`/`DAILY_EXTRACTION_COUNT` 静态量,测试需用独立 source key 避免互相污染。

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_source_daily_cap_is_independent() {
        // 两个不同 source 各自独立计数,互不挤占。
        let src_a = "test-source-a-unique";
        let src_b = "test-source-b-unique";

        // 初始都允许
        assert!(can_extract(src_a));
        // src_a 记一次后,因 5 分钟去抖,src_a 立即再问应为 false
        record_extraction(src_a);
        assert!(!can_extract(src_a), "same source within 5min should be debounced");
        // src_b 不受 src_a 影响
        assert!(can_extract(src_b), "different source must be independent");
    }

    #[test]
    fn daily_cap_counts_per_source() {
        // 单 source 当日累计达到 50 后拒绝(绕过去抖:直接操作计数)。
        let src = "test-source-cap-unique";
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        {
            let mut map = DAILY_EXTRACTION_COUNT.lock().unwrap();
            map.insert(src.to_string(), (today.clone(), 50));
        }
        // 清掉去抖记录,确保是 daily cap 在起作用
        {
            let mut last = LAST_EXTRACTION.lock().unwrap();
            last.remove(src);
        }
        assert!(!can_extract(src), "source at 50/day must be rejected");
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml memory_extraction::tests -- --nocapture`
Expected: 编译失败(`DAILY_EXTRACTION_COUNT` 当前类型是 `(String, u32)` 而非 `HashMap`,`.insert` 不存在),或断言失败。若因 §11 运行时问题无法执行,以编译期类型不匹配为"失败"信号。

- [ ] **Step 3: 改造静态量类型与 can_extract/record_extraction**

替换 `memory_extraction.rs:42-82`:

```rust
// Daily caps: per-source count of extractions today.
// Map<source_key, (date_yyyy_mm_dd, count_today)>
static DAILY_EXTRACTION_COUNT: Lazy<Mutex<HashMap<String, (String, u32)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

const DAILY_CAP_PER_SOURCE: u32 = 50;

pub fn can_extract(source_key: &str) -> bool {
    // Debounce: 5 min per source
    {
        let map = LAST_EXTRACTION.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(last) = map.get(source_key) {
            if last.elapsed().as_secs() < 300 {
                return false;
            }
        }
    }

    // Daily cap: per-source, resets on date change
    {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let mut guard = DAILY_EXTRACTION_COUNT.lock().unwrap_or_else(|e| e.into_inner());
        let entry = guard.entry(source_key.to_string())
            .or_insert_with(|| (today.clone(), 0));
        if entry.0 != today {
            *entry = (today.clone(), 0);
        }
        if entry.1 >= DAILY_CAP_PER_SOURCE {
            return false;
        }
    }

    true
}

pub fn record_extraction(source_key: &str) {
    {
        let mut map = LAST_EXTRACTION.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(source_key.to_string(), Instant::now());
    }
    {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let mut guard = DAILY_EXTRACTION_COUNT.lock().unwrap_or_else(|e| e.into_inner());
        let entry = guard.entry(source_key.to_string())
            .or_insert_with(|| (today.clone(), 0));
        if entry.0 != today {
            *entry = (today.clone(), 0);
        }
        entry.1 += 1;
    }
}
```

- [ ] **Step 4: 运行测试 / 编译验证**

Run: `cargo test --manifest-path src-tauri/Cargo.toml memory_extraction::tests -- --nocapture`
Expected: PASS。若因 §11 无法运行,退而执行 `cargo check --manifest-path src-tauri/Cargo.toml` 确认编译通过。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/group_chat/memory_extraction.rs
git commit -m "feat(memory): 抽取每日上限改为 per-source 计数,为普通会话接入做准备"
```

---

## Task 3: SessionActor 新增群聊标志与回合文本缓冲(A2 主体)

**Files:**
- Modify: `src-tauri/src/agent/session_actor.rs`(字段定义 ~244、`spawn_actor` 签名 ~258-319、`MessageComplete` 累积 ~1508-1528、回合完成 ~1596-1609)
- Modify: `src-tauri/src/commands/session.rs:888-902`、`:1482-1496`(两处 `spawn_actor` 调用)
- Modify: `src-tauri/src/commands/group_chat.rs:295-311` 附近(群聊的 `start_session` 间接调用,确认 is_group_chat 透传)
- Modify: `src-tauri/src/web_server/dispatch.rs:850-862`(web 路径 `spawn_actor` 调用)

**Interfaces:**
- Consumes: `can_extract(&str)`、`record_extraction(&str)`、`auto_extract_memories(&[String]) -> Vec<MemoryNode>`(来自 Task 2 改造后的 `group_chat::memory_extraction`)。
- Produces:
  - `SessionActor` 新增字段 `is_group_chat: bool`、`turn_user_text: Option<String>`、`turn_assistant_texts: Vec<String>`。
  - `spawn_actor(...)` 新增末位参数 `is_group_chat: bool`。

**背景与判别依据(已确认):** `auto_approve_mcp` 当前是群聊参与者的标志(`group_chat.rs:309` 传 `true`;普通会话 `session.rs:990`、审批流 `session.rs:1495`、web `dispatch.rs:860` 均传 `false`)。但该字段语义是"MCP 自动批准",不宜复用作群聊判别。改为显式新增 `is_group_chat` actor 字段。普通会话回合完成时,仅当 `!is_group_chat` 才触发抽取。

普通会话当前没有通用的逐回合助手文本缓冲——只有 ralph 的 `turn_toplevel_texts`(`session_actor.rs:77`,仅 ralph 回合累积,见 `:1523-1527`)。需新增 `turn_assistant_texts` 累积所有顶层 `MessageComplete`,并保存用户输入 `turn_user_text`。

- [ ] **Step 1: 新增 actor 字段**

在 `session_actor.rs:244`(`auto_approve_mcp: bool,` 之后、结构体闭合 `}` 之前)插入:

```rust
    /// True when this actor backs a Group Chat participant. Group chat
    /// memory extraction is handled by the orchestrator, so the actor must
    /// NOT also extract (avoids duplicate extraction).
    is_group_chat: bool,
    /// User input text for the current turn (for memory extraction on turn end).
    turn_user_text: Option<String>,
    /// Top-level assistant texts accumulated during the current turn
    /// (for memory extraction on turn end).
    turn_assistant_texts: Vec<String>,
```

- [ ] **Step 2: `spawn_actor` 增参并初始化字段**

在 `spawn_actor` 签名(`:271` `auto_approve_mcp: bool,` 之后)增加参数:

```rust
    auto_approve_mcp: bool,
    is_group_chat: bool,
) -> SessionActorHandle {
```

在 actor 构造块(`:318` `auto_approve_mcp,` 之后)增加初始化:

```rust
        auto_approve_mcp,
        is_group_chat,
        turn_user_text: None,
        turn_assistant_texts: Vec::new(),
```

- [ ] **Step 3: 保存用户输入到 turn_user_text**

在用户回合启动处(`:699-709`,`self.active_turn = Some(ActiveTurn {...})` 之前)记录用户输入。该处 `ticket.text` 即用户消息文本(见 `:690`)。插入:

```rust
        // Capture user input for end-of-turn memory extraction (non-group, normal turns only)
        if !self.is_group_chat {
            if let UserTurnKind::Normal { .. } = &ticket.kind {
                self.turn_user_text = Some(ticket.text.clone());
                self.turn_assistant_texts.clear();
            }
        }
```

- [ ] **Step 4: 累积助手顶层文本**

在 `MessageComplete` 处理处(`:1516-1528`,现有 ralph 累积分支旁)扩展。现有代码:

```rust
                        // Ralph: accumulate top-level assistant text (only during ralph turns)
                        if parent_tool_use_id.is_none() {
                            let is_ralph_turn = self
                                .active_turn
                                .as_ref()
                                .map(|t| matches!(t.origin, TurnOrigin::Ralph))
                                .unwrap_or(false);
                            if is_ralph_turn {
                                if let Some(ref mut ralph) = self.ralph_loop {
                                    ralph.turn_toplevel_texts.push(text.clone());
                                }
                            }
                        }
```

改为(在 ralph 分支之外,额外为普通会话累积):

```rust
                        // Ralph: accumulate top-level assistant text (only during ralph turns)
                        if parent_tool_use_id.is_none() {
                            let is_ralph_turn = self
                                .active_turn
                                .as_ref()
                                .map(|t| matches!(t.origin, TurnOrigin::Ralph))
                                .unwrap_or(false);
                            if is_ralph_turn {
                                if let Some(ref mut ralph) = self.ralph_loop {
                                    ralph.turn_toplevel_texts.push(text.clone());
                                }
                            }
                            // Normal (non-group) turns: accumulate for end-of-turn extraction
                            if !self.is_group_chat && self.turn_user_text.is_some() {
                                self.turn_assistant_texts.push(text.clone());
                            }
                        }
```

- [ ] **Step 5: 回合完成时触发抽取**

在回合完成处(`:1597-1609`,`if (emit_state == "idle" || emit_state == "failed") && self.active_turn.is_some()` 块内,`self.try_dispatch().await;` 之前)插入抽取触发。仅 idle(成功完成)触发,failed 不抽取:

```rust
                    if (emit_state == "idle" || emit_state == "failed")
                        && self.active_turn.is_some()
                    {
                        let turn = self.active_turn.take().unwrap();
                        self.on_user_turn_finished(&turn);
                        self.active_extractor = None;
                        self.protocol.set_pending_slash_command(None);

                        // ── Auto-extract memories from this normal-chat turn (non-group) ──
                        if emit_state == "idle" && !self.is_group_chat {
                            if let Some(user_text) = self.turn_user_text.take() {
                                let assistant_text = self.turn_assistant_texts.join("\n");
                                self.turn_assistant_texts.clear();
                                if !assistant_text.trim().is_empty() {
                                    let source_key = self.run_id.clone();
                                    let turn_texts = vec![
                                        format!("[user]: {}", user_text),
                                        format!("[assistant]: {}", assistant_text),
                                    ];
                                    tokio::spawn(async move {
                                        use crate::group_chat::memory_extraction::{
                                            can_extract, record_extraction, auto_extract_memories,
                                            log_to_file,
                                        };
                                        if !can_extract(&source_key) {
                                            return;
                                        }
                                        let memories = auto_extract_memories(&turn_texts).await;
                                        log_to_file(&format!(
                                            "[memory-extraction] RETURN run={} count={}",
                                            source_key, memories.len()
                                        ));
                                        if !memories.is_empty() {
                                            record_extraction(&source_key);
                                        }
                                    });
                                }
                            }
                        }

                        // Ralph loop: state transition on turn end
                        self.ralph_on_turn_end(&turn, &emit_state);

                        self.try_dispatch().await;
                    }
```

- [ ] **Step 6: 更新两处 `spawn_actor` 直接调用点**

**已核实(grep `spawn_actor(`):全仓库只有两处直接调用,均在 `commands/session.rs`。** `web_server/dispatch.rs:848` 与 `commands/group_chat.rs:297` 调用的是 `start_session_impl`(不是 `spawn_actor`),它们的 `is_group_chat` 实参已就位(dispatch 传 `false`、group_chat 传 `true`),由 `start_session_impl` 透传到 spawn_actor,**无需改动**。

`src-tauri/src/commands/session.rs:888-902`(主 `start_session_impl`)— 在末位 `auto_approve_mcp,` 之后加 `is_group_chat,`(该函数已有 `is_group_chat: bool` 形参,见 `session.rs:549`,直接透传):

```rust
        msvc_injected,
        auto_approve_mcp,
        is_group_chat,
    );
```

`src-tauri/src/commands/session.rs:1482-1496`(审批流重启 spawn)— 该处末位传 `false, // auto_approve_mcp`,其后加 `false, // is_group_chat`:

```rust
        msvc_injected,
        false, // auto_approve_mcp: approval flow doesn't auto-approve
        false, // is_group_chat
    );
```

- [ ] **Step 7: 编译验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过。重点确认所有 `spawn_actor` 调用点 arity 一致、`UserTurnKind::Normal` 路径名正确(它在 `on_user_turn_finished` 已被使用,见 `:776`)。

- [ ] **Step 8: 手动验证(运行 app)**

启动 `npm run tauri dev`,在普通 `/chat` 完成一轮对话(等回合进入 idle),然后:
1. 打开 `~/.claw-go/logs/memory-extraction.log`,确认出现 `RETURN run=<run_id> count=N` 日志。
2. 进入 `/memory-mgmt`,确认新记忆出现在列表。
3. 在群聊里完成一轮,确认 `memory-extraction.log` 中群聊抽取仍走 `gc=` 路径,且**没有**同一回合的 `run=` 重复抽取。

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/agent/session_actor.rs src-tauri/src/commands/session.rs
git commit -m "feat(memory): 普通 /chat 会话回合结束自动抽取记忆(跳过群聊,per-run 去抖)"
```

---

## Task 4: 批次 A 收尾验证

**Files:** 无(仅运行验证命令)

- [ ] **Step 1: Rust 编译与格式**

Run:
```bash
cargo check  --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo fmt    --manifest-path src-tauri/Cargo.toml --check
```
Expected: 全部通过。若 fmt 报格式问题,运行 `cargo fmt --manifest-path src-tauri/Cargo.toml` 修正后重提。

- [ ] **Step 2: 前端构建(确认 A1 无回归)**

Run: `npm run build`
Expected: 构建成功。

- [ ] **Step 3: 若有改动,补提交**

```bash
git add -A
git commit -m "chore(memory): 批次 A 收尾 — fmt/clippy 修正"
```

---

## Self-Review 记录

- **Spec 覆盖:** A1 → Task 1;A2 → Task 2(per-source cap)+ Task 3(actor 接线)。spec 批次 A 两项全覆盖。
- **类型一致性:** `can_extract(&str)`/`record_extraction(&str)`/`auto_extract_memories(&[String])` 签名在 Task 2 与 Task 3 一致;`spawn_actor` 新增 `is_group_chat: bool` 末位参数在定义(Task3 Step2)与三处调用(Step6)一致。
- **占位符扫描:** 无 TBD/TODO;每个代码步骤含完整代码。
- **已知不确定点(实现时核对):** Task3 Step6 中 `web_server/dispatch.rs` 是否直接调用 `spawn_actor` 还是经 `start_session` 透传 —— 实现时先读该调用点确认,二选一处理(计划已给两种情形的做法)。
