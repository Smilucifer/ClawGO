# 委员会直播 UI 重构 — Debate Flow Card + 动态队列 + Abort 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把委员会直播页从"批量一键运行 + 7 步纵向圆点列表"重构为"动态执行队列(可追加/可中止/可重试) + 菱形 debate flow 卡片布局",并让后端通过 CancellationToken 真正取消 LLM 调用。

**Architecture:** 后端 `run_committee` 注入 `CancellationToken`,在每个 phase 之间检查取消;新增 `CommitteeCancelRegistry`(`Arc<Mutex<HashMap<String, CancellationToken>>>`)作为 Tauri state,key 为 symbol。前端 store 成为并发调度器——维护队列状态机,每次对单个 symbol 调用 `run_committee_stream([symbol])`(fire-and-forget),并发上限由前端 `maxConcurrent` 控制而非后端 Semaphore。队列与 portfolio 快照持久化到 `~/.claw-go/invest/committee-queue.json`。

**Tech Stack:** Rust (Tauri 2, tokio, tokio-util CancellationToken, serde) + SvelteKit (Svelte 5 runes) + Vitest。

## Global Constraints

- **Windows-first**:不假设 WSL/macOS/Linux 工作流;保留 `.cmd` shim / MSVC 等既有行为。
- **Svelte 5 runes**:前端用 `$state` / `$derived` / `$effect` / `$props` / `{#snippet}`。
- **i18n 双语**:新增 UI 文案必须同时更新 `messages/en.json` 与 `messages/zh-CN.json`,并通过 `npm run i18n:check`。
- **Rust 测试无法运行**:本机 Rust 单元测试因 `STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)` 无法执行(见 CLAUDE.md §11)。后端 TDD 调整为:**写测试 + 用 `cargo check` 验证编译通过**(测试将在 CI/干净环境运行)。`cargo check` 不链接,不受 linker / VCRUNTIME 问题影响。
- **前端测试真跑**:Vitest 可运行(`npm test`),前端逻辑用真正的红-绿 TDD。测试 glob 为 `src/**/*.test.ts`,environment 为 `node`。
- **原子写入**:`committee-queue.json` 用 tmp+rename+PermissionDenied 重试模式,复用 `storage/runs.rs::save_meta` 的写法。
- **事件名**:Tauri 事件名为 `'committee-event'`(已有,不改)。
- **Conventional Commits**:`feat:` / `fix:` / `chore:` / `refactor:`。
- **每个 task 完成后 commit**;前端改动跑 `npm run check`,后端改动跑 `cargo check --manifest-path src-tauri/Cargo.toml`。

---

## 架构关键决策(实现前必读)

**为什么前端驱动并发,而不是后端 Semaphore?**
核心需求是"运行中可追加标的、retry 追加队尾、可中止单个/全部"。后端 `run_committee_batch_stream` 一次接收整批 + 内部 Semaphore 的模型无法在运行中动态追加(批次已固定)。因此改为:前端 store 维护队列,每次对**单个** symbol 调用 `run_committee_stream([symbol])`,fire-and-forget;前端用 `maxConcurrent` 控制同时 in-flight 的调用数;完成事件回来后补下一个。后端 `run_committee_batch_stream` 仍处理传入的 `Vec`(此处长度恒为 1),其内部 Semaphore 退化为 1,不再是并发主控。`CommitteeConfig.max_concurrent_symbols` 字段保留(向后兼容非 stream 的 `run_committee_batch` 命令),但在 stream 路径下失效。代价:每个 symbol 单独 load portfolio(可接受,非本计划优化目标)。

**取消如何传播到 UI?**
`run_committee` 检测到 `token.is_cancelled()` 时,先 `emit(CommitteeEvent::SymbolAborted { symbol })` 再返回 `Err("aborted: {symbol}")`。前端监听 `symbol_aborted` 事件 → 标记该 symbol 队列状态为 `aborted` + 释放槽 + drain 下一个。

**两个状态维度:**
- **队列状态**(`QueueItem.status`: `queued` | `running` | `done` | `failed` | `aborted`)— 调度层面,store.queue 数组持有。
- **pipeline 进度**(`SymbolProgress.activeStep` / `completedSteps` / `completedRounds`)— 单个 symbol 内部 7 步进度。
- 为方便卡片渲染(显示 abort/retry 按钮、置灰中止步骤),`SymbolProgress` 额外加一个 `status` 字段,与队列状态同步。

---

## 文件变更总览

### 后端 (Rust)

| 文件 | 变更 | 说明 |
|------|------|------|
| `src-tauri/src/invest/committee/queue.rs` | 新建 | 队列 + 快照持久化(load/save) |
| `src-tauri/src/invest/committee/mod.rs` | 修改 | 加 `pub mod queue;` |
| `src-tauri/src/invest/committee/events.rs` | 修改 | 加 `SymbolAborted` 变体 |
| `src-tauri/src/invest/committee/orchestrator.rs` | 修改 | 注入 `CancellationToken` + `check_cancellation` helper + `run_committee_batch_stream` 加 tokens 参数 |
| `src-tauri/src/commands/invest.rs` | 修改 | `CommitteeCancelRegistry` + 4 个新命令 + `run_committee_stream` 改造 |
| `src-tauri/src/lib.rs` | 修改 | `.manage(registry)` + 注册 4 个命令 |

注:**无需改 `Cargo.toml`** — `tokio-util = "0.7"` 已在依赖中,且 `lib.rs:17` 已 `use tokio_util::sync::CancellationToken`,说明当前 features 已够用。

### 前端 (Svelte/TS)

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/lib/stores/invest-committee-store.svelte.ts` | 修改 | 导出 class + 队列类型 + 队列状态机 + 持久化 |
| `src/lib/components/invest/pipeline-config.ts` | 修改 | `getStepState` 加 `'aborted'` |
| `src/lib/components/invest/CommitteeLiveTab.svelte` | 重写 | debate flow grid + abort/retry + 并发设置 |
| `src/lib/components/invest/ProviderConfigPanel.svelte` | 修改 | 移除并发上限设置(迁移到 LiveTab) |
| `src/lib/components/invest/PipelineFlow.svelte` | 删除 | 孤儿组件(0 引用),被 debate flow card 取代 |
| `src/lib/stores/invest-committee-store.test.ts` | 新建 | 队列状态机 Vitest 测试 |
| `src/lib/components/invest/pipeline-config.test.ts` | 新建 | `getStepState` aborted 测试 |
| `messages/en.json` + `messages/zh-CN.json` | 修改 | 新增 i18n keys |

---

## Task 1: queue.rs 持久化模块

新建一个纯持久化模块:把前端传来的队列状态序列化到磁盘,重启后读回。前端是状态真相源,本模块只做 load/save。

**Files:**
- Create: `src-tauri/src/invest/committee/queue.rs`
- Modify: `src-tauri/src/invest/committee/mod.rs`(在现有 `pub mod` 列表中加 `pub mod queue;`)

**Interfaces:**
- Consumes: `crate::storage::data_dir()`(返回 `PathBuf`,`~/.claw-go`)、`crate::storage::ensure_dir(&Path)`(返回 `io::Result<()>`)。
- Produces:
  - `pub struct CommitteeQueueState { items: Vec<QueueItem>, snapshot: Option<PortfolioSnapshot>, max_concurrent: usize, updated_at: String }`(derive `Default`)
  - `pub enum QueueItemStatus { Queued, Running, Done, Failed, Aborted }`(serde snake_case)
  - `pub struct QueueItem { symbol: String, status: QueueItemStatus, error: Option<String> }`
  - `pub struct PortfolioSnapshot { holdings: Vec<SnapshotHolding>, cash: f64, total_notional: f64, timestamp: String }`
  - `pub struct SnapshotHolding { symbol: String, name: Option<String>, shares: Option<f64>, notional: f64, kind: String }`
  - `pub fn load_queue() -> CommitteeQueueState`
  - `pub fn save_queue(state: &CommitteeQueueState) -> Result<(), String>`
  - 所有 struct 用 `#[serde(rename_all = "camelCase")]`,与前端 TS camelCase 对齐。

- [ ] **Step 1: 写完整模块(含失败测试)**

创建 `src-tauri/src/invest/committee/queue.rs`:

```rust
//! Committee live-queue persistence.
//!
//! Stores the live-tab execution queue and a portfolio snapshot to
//! `~/.claw-go/invest/committee-queue.json` using the same atomic
//! tmp+rename pattern as `storage::runs::save_meta`. The frontend store is
//! the source of truth; this module only loads/saves.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueItemStatus {
    Queued,
    Running,
    Done,
    Failed,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub symbol: String,
    pub status: QueueItemStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotHolding {
    pub symbol: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shares: Option<f64>,
    pub notional: f64,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioSnapshot {
    pub holdings: Vec<SnapshotHolding>,
    pub cash: f64,
    pub total_notional: f64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CommitteeQueueState {
    #[serde(default)]
    pub items: Vec<QueueItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<PortfolioSnapshot>,
    #[serde(default)]
    pub max_concurrent: usize,
    #[serde(default)]
    pub updated_at: String,
}

fn queue_path() -> Result<PathBuf, String> {
    let invest_dir = crate::storage::data_dir().join("invest");
    crate::storage::ensure_dir(&invest_dir).map_err(|e| format!("create invest dir: {e}"))?;
    Ok(invest_dir.join("committee-queue.json"))
}

/// Load persisted queue state. Returns default (empty) state when the file is
/// missing or fails to parse — never errors, so the live tab always opens.
pub fn load_queue() -> CommitteeQueueState {
    let path = match queue_path() {
        Ok(p) => p,
        Err(e) => {
            log::warn!("committee queue: path resolve failed: {e}");
            return CommitteeQueueState::default();
        }
    };
    if !path.exists() {
        return CommitteeQueueState::default();
    }
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
            log::warn!("committee queue: parse failed: {e}");
            CommitteeQueueState::default()
        }),
        Err(e) => {
            log::warn!("committee queue: read failed: {e}");
            CommitteeQueueState::default()
        }
    }
}

/// Persist queue state atomically (tmp write + rename, retry on
/// PermissionDenied), mirroring `storage::runs::save_meta`.
pub fn save_queue(state: &CommitteeQueueState) -> Result<(), String> {
    let path = queue_path()?;
    let dir = path
        .parent()
        .ok_or_else(|| "queue path has no parent".to_string())?;
    let tmp = dir.join(format!(
        "committee-queue.json.{}.{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(&tmp, &json).map_err(|e| format!("write tmp: {e}"))?;
    for attempt in 0..3u8 {
        match fs::rename(&tmp, &path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && attempt < 2 => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return Err(format!("rename: {e}"));
            }
        }
    }
    let _ = fs::remove_file(&tmp);
    Err("rename: PermissionDenied after 3 retries".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_state_roundtrips_through_json() {
        let state = CommitteeQueueState {
            items: vec![
                QueueItem {
                    symbol: "600519".into(),
                    status: QueueItemStatus::Done,
                    error: None,
                },
                QueueItem {
                    symbol: "000001".into(),
                    status: QueueItemStatus::Failed,
                    error: Some("boom".into()),
                },
            ],
            snapshot: Some(PortfolioSnapshot {
                holdings: vec![SnapshotHolding {
                    symbol: "600519".into(),
                    name: Some("贵州茅台".into()),
                    shares: Some(100.0),
                    notional: 170000.0,
                    kind: "hold".into(),
                }],
                cash: 50000.0,
                total_notional: 170000.0,
                timestamp: "2026-06-18T00:00:00Z".into(),
            }),
            max_concurrent: 5,
            updated_at: "2026-06-18T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: CommitteeQueueState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.items.len(), 2);
        assert_eq!(back.items[0].symbol, "600519");
        assert_eq!(back.items[0].status, QueueItemStatus::Done);
        assert_eq!(back.items[1].error.as_deref(), Some("boom"));
        assert_eq!(back.max_concurrent, 5);
        assert_eq!(back.snapshot.unwrap().holdings[0].kind, "hold");
    }

    #[test]
    fn status_serializes_snake_case() {
        let json = serde_json::to_string(&QueueItemStatus::Aborted).unwrap();
        assert_eq!(json, "\"aborted\"");
    }

    #[test]
    fn default_state_is_empty() {
        let s = CommitteeQueueState::default();
        assert!(s.items.is_empty());
        assert!(s.snapshot.is_none());
        assert_eq!(s.max_concurrent, 0);
    }
}
```

- [ ] **Step 2: 声明模块**

修改 `src-tauri/src/invest/committee/mod.rs`,在现有声明块中按字母序插入 `pub mod queue;`:

```rust
pub mod analysis;
pub mod archive;
pub mod cli_executor;
pub mod events;
pub mod orchestrator;
pub mod parser;
pub mod queue;
pub mod roles;
pub mod tools;
```

- [ ] **Step 3: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过,无 error(可能有 `queue` 未被使用的 warning,Task 3 接入后消失)。

> 注:本机 `cargo test` 会因 `STATUS_ENTRYPOINT_NOT_FOUND` 无法运行二进制(CLAUDE.md §11)。测试已写入,用 `cargo check` 确认编译;测试将在 CI/干净环境运行。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/invest/committee/queue.rs src-tauri/src/invest/committee/mod.rs
git commit -m "feat(invest): add committee live-queue persistence module"
```

---

## Task 2: CancellationToken 注入 orchestrator + SymbolAborted 事件

让 `run_committee` 支持按 symbol 取消,并在取消时发出 `SymbolAborted` 事件。

**Files:**
- Modify: `src-tauri/src/invest/committee/events.rs`(`CommitteeEvent` enum 加变体)
- Modify: `src-tauri/src/invest/committee/orchestrator.rs`(use 区、新 helper、3 个函数签名、调用点)

**Interfaces:**
- Consumes: `CommitteeEvent`(events.rs)、`EventEmitter = Arc<dyn Fn(CommitteeEvent) + Send + Sync>`(orchestrator.rs:53)、`tokio_util::sync::CancellationToken`。
- Produces:
  - `CommitteeEvent::SymbolAborted { symbol: String }`(序列化为 `{ "type": "symbol_aborted", "symbol": "..." }`)
  - `fn check_cancellation(cancel: Option<&CancellationToken>, emitter: &Option<EventEmitter>, symbol: &str) -> Result<(), String>`
  - `run_committee(... , cancel: Option<CancellationToken>)`(新增第 6 参,owned)
  - `run_debate_rounds(... , cancel: Option<&CancellationToken>)`(新增末参,借用)
  - `run_committee_batch_stream(... , tokens: HashMap<String, CancellationToken>)`(新增末参)

- [ ] **Step 1: 加 SymbolAborted 事件变体**

修改 `src-tauri/src/invest/committee/events.rs`,在 `CommitteeEvent` enum 的 `Error` 变体之后插入(`Error` 变体当前是 enum 最后一个):

```rust
    /// A symbol's pipeline errored (non-retryable).
    Error {
        symbol: String,
        error: String,
    },
    /// A symbol's pipeline was cancelled by the user via abort.
    SymbolAborted {
        symbol: String,
    },
```

(enum 顶部已有 `#[serde(tag = "type", rename_all = "snake_case")]`,新变体自动序列化为 `type: "symbol_aborted"`,无需额外属性。)

- [ ] **Step 2: 验证事件编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过(可能有 `SymbolAborted` 未构造的 warning,Step 5 接入后消失)。

- [ ] **Step 3: 加 use + check_cancellation helper**

修改 `src-tauri/src/invest/committee/orchestrator.rs`。在 use 区(`use tokio::sync::Semaphore;` 之后)加:

```rust
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
```

在文件中 `pub type EventEmitter = ...;`(第 53 行)之后加模块级 helper:

```rust
pub type EventEmitter = Arc<dyn Fn(CommitteeEvent) + Send + Sync>;

/// Returns `Err` (and emits `SymbolAborted`) when the symbol has been
/// cancelled. Called between pipeline phases so cancellation takes effect at
/// the next phase boundary rather than mid-LLM-call.
fn check_cancellation(
    cancel: Option<&CancellationToken>,
    emitter: &Option<EventEmitter>,
    symbol: &str,
) -> Result<(), String> {
    if cancel.is_some_and(|c| c.is_cancelled()) {
        if let Some(emit) = emitter {
            emit(CommitteeEvent::SymbolAborted {
                symbol: symbol.to_string(),
            });
        }
        return Err(format!("aborted: {symbol}"));
    }
    Ok(())
}
```

- [ ] **Step 4: run_committee 加 cancel 参数 + 各 phase 前检查**

修改 `run_committee` 签名(orchestrator.rs:1697),在 `portfolio_override` 后加 `cancel`:

```rust
pub(crate) async fn run_committee(
    symbol: &str,
    config: &CommitteeConfig,
    emitter: Option<EventEmitter>,
    dry_run: bool,
    portfolio_override: Option<std::sync::Arc<PortfolioData>>,
    cancel: Option<CancellationToken>,
) -> Result<CommitteeResult, String> {
```

在每个 phase 调用**之前**插入检查。具体锚点(用现有调用代码作 old_string,在其前面加一行):

宏 phase 之前(macro 调用在 ~1764 行,`let (macro_output, macro_tokens) = run_macro_phase(...)`):
```rust
    check_cancellation(cancel.as_ref(), &emitter, symbol)?;
    let (macro_output, macro_tokens) =
        run_macro_phase(symbol, config, &portfolio_summary, &emitter, &asset_context).await?;
```

debate phase 之前(~1838 行,`let converged = run_debate_rounds(...)`),同时把 `cancel.as_ref()` 作为新末参传入:
```rust
    check_cancellation(cancel.as_ref(), &emitter, symbol)?;
    let converged = run_debate_rounds(
        symbol,
        config,
        &mut round_outputs,
        &mut total_tokens,
        &macro_signal,
        effective_buffer,
        &emitter,
        &portfolio_summary,
        regime_context.as_deref(),
        &portfolio_data,
        &asset_context,
        cancel.as_ref(),
    )
    .await?;
```

CIO phase 之前(~1854 行,`let cio_output = run_role_phase(...)`):
```rust
    check_cancellation(cancel.as_ref(), &emitter, symbol)?;
    let cio_output = run_role_phase(
        symbol,
        CommitteeRole::Cio,
        1,
        config,
        &round_outputs,
        &macro_signal,
        effective_buffer,
        &portfolio_summary,
        regime_context.as_deref(),
        &emitter,
        &portfolio_data,
        &asset_context,
    )
    .await?;
```

(regime 是同步本地计算,紧跟 macro,无需单独检查点。)

- [ ] **Step 5: run_debate_rounds 加 cancel 参数 + 循环内检查**

修改 `run_debate_rounds` 签名(orchestrator.rs:1602),在末尾 `asset_context: &AssetContext,` 后加:

```rust
async fn run_debate_rounds(
    symbol: &str,
    config: &CommitteeConfig,
    round_outputs: &mut Vec<RoundOutput>,
    total_tokens: &mut u32,
    macro_signal: &str,
    min_cash_reserve: f64,
    emitter: &Option<EventEmitter>,
    portfolio_summary: &str,
    regime_context: Option<&str>,
    portfolio_data: &PortfolioData,
    asset_context: &AssetContext,
    cancel: Option<&CancellationToken>,
) -> Result<bool, String> {
```

在 round 循环开头(`for round in 1..=max_rounds {` 之后第一行)插入检查:

```rust
    for round in 1..=max_rounds {
        check_cancellation(cancel, emitter, symbol)?;
        // Both Quant and Risk participate in each round
        let roles = vec![CommitteeRole::Quant, CommitteeRole::Risk];
```

- [ ] **Step 6: 更新 run_committee 的两个调用点**

`run_committee_batch_stream` 内的调用(~2046 行)在 Step 7 改;先改 `run_committee_batch` 内的调用(~2002 行,非 stream 路径,无 abort,传 `None`):

```rust
            run_committee(&symbol, &config, None, dry_run, Some(portfolio), None).await
```

- [ ] **Step 7: run_committee_batch_stream 加 tokens 参数**

修改 `run_committee_batch_stream` 签名(orchestrator.rs:2019),在 `dry_run: bool,` 后加 `tokens`,并 import HashMap(顶部已有 `use std::collections::HashMap;`,无需重复):

```rust
pub async fn run_committee_batch_stream(
    symbols: &[String],
    config: &CommitteeConfig,
    emitter: EventEmitter,
    dry_run: bool,
    tokens: HashMap<String, CancellationToken>,
) -> Vec<Result<CommitteeResult, String>> {
```

修改 spawn 循环(2032-2047),取出每个 symbol 的 token 并传入 `run_committee`:

```rust
    for symbol in symbols {
        let config = config.clone();
        let symbol = symbol.clone();
        let emitter = emitter.clone();
        let portfolio = portfolio_arc.clone();
        let sem = semaphore.clone();
        let token = tokens.get(&symbol).cloned();
        handles.push((
            symbol.clone(),
            tokio::spawn(async move {
                let _permit = sem
                    .acquire_owned()
                    .await
                    .expect("semaphore closed unexpectedly");
                run_committee(&symbol, &config, Some(emitter), dry_run, Some(portfolio), token).await
            }),
        ));
    }
```

- [ ] **Step 8: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 报错 `run_committee_batch_stream` 的调用点(commands/invest.rs:881)参数数量不符 —— 这是预期的,Task 3 修复。若 orchestrator.rs 自身有其他 error,先修复。可临时用 `cargo check` 只看 orchestrator 模块是否结构正确。

> 该 step 预期会因下游调用点未更新而失败,这正常。确认错误**仅**来自 commands/invest.rs:881 的参数不匹配,而非 orchestrator.rs 内部。

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/invest/committee/events.rs src-tauri/src/invest/committee/orchestrator.rs
git commit -m "feat(invest): inject CancellationToken into committee pipeline + SymbolAborted event"
```

---

## Task 3: Tauri 命令 — abort/registry/queue + run_committee_stream 改造

新增 abort 与 queue 持久化命令,改造 `run_committee_stream` 为每个 symbol 创建并注册 CancellationToken。

**Files:**
- Modify: `src-tauri/src/commands/invest.rs`(type alias、构造函数、4 新命令、`run_committee_stream` 改造)
- Modify: `src-tauri/src/lib.rs`(`.manage(...)` + `generate_handler!` 注册)

**Interfaces:**
- Consumes: `run_committee_batch_stream(symbols, config, emitter, dry_run, tokens)`(Task 2)、`queue::{load_queue, save_queue, CommitteeQueueState}`(Task 1)、`CancellationToken`。
- Produces:
  - `pub type CommitteeCancelRegistry = Arc<Mutex<HashMap<String, CancellationToken>>>`
  - `pub fn new_committee_cancel_registry() -> CommitteeCancelRegistry`
  - `#[tauri::command] abort_committee_symbol(cancel_registry, symbol)`
  - `#[tauri::command] abort_committee_all(cancel_registry)`
  - `#[tauri::command] load_committee_queue() -> CommitteeQueueState`
  - `#[tauri::command] save_committee_queue(state: CommitteeQueueState)`
  - `run_committee_stream` 新增参数 `cancel_registry: State<'_, CommitteeCancelRegistry>`

- [ ] **Step 1: 加 type alias + 构造函数**

修改 `src-tauri/src/commands/invest.rs`,在 use 区(`use tauri::Emitter;` 之后)加:

```rust
use tauri::Emitter;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// Per-symbol cancellation tokens for in-flight committee pipelines.
/// Key = symbol. Managed as Tauri state.
pub type CommitteeCancelRegistry = Arc<Mutex<HashMap<String, CancellationToken>>>;

/// Build an empty cancel registry for `App::manage`.
pub fn new_committee_cancel_registry() -> CommitteeCancelRegistry {
    Arc::new(Mutex::new(HashMap::new()))
}
```

- [ ] **Step 2: 改造 run_committee_stream**

替换整个 `run_committee_stream`(invest.rs:863-…)。新增 `cancel_registry` 参数;为每个 symbol 建 token 注册;调用 batch_stream 传 tokens;结束后清理 registry:

```rust
#[tauri::command]
pub async fn run_committee_stream(
    app: tauri::AppHandle,
    symbols: Vec<String>,
    debate_rounds: Option<u8>,
    dry_run: Option<bool>,
    cancel_registry: tauri::State<'_, CommitteeCancelRegistry>,
) -> Result<Vec<crate::invest::committee::orchestrator::CommitteeResult>, String> {
    let config_data = get_llm_config()?;
    let committee_config = build_committee_config(&config_data, debate_rounds)?;

    let emitter: crate::invest::committee::orchestrator::EventEmitter = {
        let app = app.clone();
        std::sync::Arc::new(move |event: crate::invest::committee::events::CommitteeEvent| {
            let _ = app.emit("committee-event", &event);
        })
    };

    // Register a fresh cancellation token per symbol.
    let mut tokens: HashMap<String, CancellationToken> = HashMap::new();
    {
        let mut reg = cancel_registry
            .lock()
            .map_err(|e| format!("cancel registry poisoned: {e}"))?;
        for s in &symbols {
            let tok = CancellationToken::new();
            reg.insert(s.clone(), tok.clone());
            tokens.insert(s.clone(), tok);
        }
    }

    let results = crate::invest::committee::orchestrator::run_committee_batch_stream(
        &symbols,
        &committee_config,
        emitter,
        dry_run.unwrap_or(false),
        tokens,
    )
    .await;

    // Clean up registry entries for this batch.
    {
        let mut reg = cancel_registry
            .lock()
            .map_err(|e| format!("cancel registry poisoned: {e}"))?;
        for s in &symbols {
            reg.remove(s);
        }
    }

    let mut out = Vec::with_capacity(results.len());
    let mut first_err: Option<String> = None;
    for r in results {
        match r {
            Ok(v) => out.push(v),
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
    }
    if out.is_empty() {
        Err(first_err.unwrap_or_else(|| "all symbols failed".to_string()))
    } else {
        Ok(out)
    }
}
```

> 注:锁 guard 在 `.await` 之前的 block scope 内释放,避免 `MutexGuard` 跨 await 的 `!Send` 问题。

- [ ] **Step 3: 加 abort 命令**

在 `run_committee_stream` 之后加两个 abort 命令(同步 fn,无 await):

```rust
/// Cancel one in-flight committee symbol pipeline.
#[tauri::command]
pub fn abort_committee_symbol(
    cancel_registry: tauri::State<'_, CommitteeCancelRegistry>,
    symbol: String,
) -> Result<(), String> {
    let reg = cancel_registry
        .lock()
        .map_err(|e| format!("cancel registry poisoned: {e}"))?;
    if let Some(token) = reg.get(&symbol) {
        token.cancel();
    }
    Ok(())
}

/// Cancel all in-flight committee pipelines.
#[tauri::command]
pub fn abort_committee_all(
    cancel_registry: tauri::State<'_, CommitteeCancelRegistry>,
) -> Result<(), String> {
    let reg = cancel_registry
        .lock()
        .map_err(|e| format!("cancel registry poisoned: {e}"))?;
    for token in reg.values() {
        token.cancel();
    }
    Ok(())
}
```

- [ ] **Step 4: 加 queue 持久化命令**

在 abort 命令之后加:

```rust
/// Load the persisted committee live-queue state.
#[tauri::command]
pub fn load_committee_queue() -> Result<crate::invest::committee::queue::CommitteeQueueState, String>
{
    Ok(crate::invest::committee::queue::load_queue())
}

/// Persist the committee live-queue state.
#[tauri::command]
pub fn save_committee_queue(
    state: crate::invest::committee::queue::CommitteeQueueState,
) -> Result<(), String> {
    crate::invest::committee::queue::save_queue(&state)
}
```

- [ ] **Step 5: lib.rs 注册 state + 命令**

修改 `src-tauri/src/lib.rs`。在 `.manage(...)` 链中(236-252 附近)加一行:

```rust
        .manage(commands::invest::new_committee_cancel_registry())
```

在 `generate_handler!` 块中(`commands::invest::run_committee_stream,` 之后)加 4 行:

```rust
            commands::invest::run_committee_stream,
            commands::invest::abort_committee_symbol,
            commands::invest::abort_committee_all,
            commands::invest::load_committee_queue,
            commands::invest::save_committee_queue,
```

- [ ] **Step 6: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过,无 error。`queue`/`SymbolAborted` 的未使用 warning 此时应消失。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands/invest.rs src-tauri/src/lib.rs
git commit -m "feat(invest): committee abort + queue persistence Tauri commands"
```

---

## Task 4: 前端 store — 队列状态机重构

把 store 从"单批 runCommittee"改造成并发队列调度器。导出 class 以便测试。这是前端最复杂的一步,用真正的红-绿 TDD(Vitest 可运行)。

**Files:**
- Modify: `src/lib/stores/invest-committee-store.svelte.ts`(类型区 + class)
- Test: `src/lib/stores/invest-committee-store.test.ts`(新建)

**Interfaces:**
- Consumes: `getTransport().invoke` / `getTransport().listen`(`$lib/transport`)、Task 3 的命令名 `'run_committee_stream'` / `'abort_committee_symbol'` / `'abort_committee_all'` / `'load_committee_queue'` / `'save_committee_queue'`、`roleToBackendIdx`(pipeline-config)。
- Produces(供 Task 5/6 使用):
  - `export class InvestCommitteeStore`(新增 `export`)
  - `export type QueueItemStatus = 'queued' | 'running' | 'done' | 'failed' | 'aborted'`
  - `export interface QueueItem { symbol: string; status: QueueItemStatus; error?: string }`
  - `export interface SnapshotHolding`、`export interface PortfolioSnapshot`、`export interface CommitteeQueueState`
  - `SymbolProgress` 新增 `status: QueueItemStatus`
  - `CommitteeEventType` 新增 `{ type: 'symbol_aborted'; symbol: string }`
  - 字段:`queue: QueueItem[]`、`maxConcurrent: number`、`portfolioSnapshot: PortfolioSnapshot | null`
  - 方法:`addToQueue(symbols, snapshot?)`、`abortSymbol(symbol)`、`abortAll()`、`retrySymbol(symbol)`、`setMaxConcurrent(n)`、`loadQueue()`
  - getter:`queuedCount` / `runningCount` / `doneCount`

- [ ] **Step 1: 写失败测试**

创建 `src/lib/stores/invest-committee-store.test.ts`:

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { CommitteeResult } from './invest-committee-store.svelte';

// ── transport mock ────────────────────────────────────────────────
const invokeMock = vi.fn();
let eventHandler: ((e: unknown) => void) | null = null;
const listenMock = vi.fn(async (_name: string, cb: (e: unknown) => void) => {
  eventHandler = cb;
  return () => {};
});

vi.mock('$lib/transport', () => ({
  getTransport: () => ({ invoke: invokeMock, listen: listenMock }),
}));

import { InvestCommitteeStore } from './invest-committee-store.svelte';

function makeResult(symbol: string): CommitteeResult {
  return {
    symbol,
    finalVerdict: 'HOLD',
    finalConfidence: 50,
    macroSignal: 'neutral',
    macroStrength: null,
    reasoning: '',
    rounds: [],
    sanityCheck: {
      gate1Pass: true,
      gate2Pass: true,
      finalVerdict: 'HOLD',
      finalConfidence: 50,
      notes: [],
    },
    sentinelOverride: null,
    converged: true,
    totalLatencyMs: 0,
    totalTokens: 0,
  };
}

const streamCalls = () =>
  invokeMock.mock.calls.filter((c) => c[0] === 'run_committee_stream');

describe('InvestCommitteeStore queue', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue([]);
    eventHandler = null;
  });

  it('enqueues symbols and starts up to maxConcurrent', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 2;
    await store.addToQueue(['A', 'B', 'C']);

    expect(store.queue.map((q) => q.symbol)).toEqual(['A', 'B', 'C']);
    expect(store.runningCount).toBe(2);
    expect(store.queuedCount).toBe(1);
    expect(streamCalls().length).toBe(2);
  });

  it('drains the next queued symbol when one completes', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 1;
    await store.addToQueue(['A', 'B']);
    expect(store.runningCount).toBe(1);

    eventHandler?.({ type: 'symbol_complete', symbol: 'A', result: makeResult('A') });
    await Promise.resolve();

    expect(store.queue.find((q) => q.symbol === 'A')?.status).toBe('done');
    expect(store.queue.find((q) => q.symbol === 'B')?.status).toBe('running');
  });

  it('abortSymbol cancels and frees the slot for the next symbol', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 1;
    await store.addToQueue(['A', 'B']);

    await store.abortSymbol('A');
    expect(
      invokeMock.mock.calls.some(
        (c) => c[0] === 'abort_committee_symbol' && (c[1] as { symbol: string }).symbol === 'A',
      ),
    ).toBe(true);
    expect(store.queue.find((q) => q.symbol === 'A')?.status).toBe('aborted');
    expect(store.queue.find((q) => q.symbol === 'B')?.status).toBe('running');
  });

  it('abortAll cancels all running and clears queued', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 1;
    await store.addToQueue(['A', 'B', 'C']);

    await store.abortAll();
    expect(invokeMock.mock.calls.some((c) => c[0] === 'abort_committee_all')).toBe(true);
    expect(store.queue.find((q) => q.symbol === 'A')?.status).toBe('aborted');
    expect(store.queue.find((q) => q.symbol === 'B')?.status).toBe('aborted');
    expect(store.queue.find((q) => q.symbol === 'C')?.status).toBe('aborted');
  });

  it('retrySymbol re-enqueues a finished symbol at the tail', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 5;
    await store.addToQueue(['A']);
    eventHandler?.({ type: 'symbol_complete', symbol: 'A', result: makeResult('A') });
    await Promise.resolve();
    expect(store.queue.find((q) => q.symbol === 'A')?.status).toBe('done');

    await store.retrySymbol('A');
    expect(store.queue.find((q) => q.symbol === 'A')?.status).toBe('running');
  });

  it('ignores symbols already running (dedup)', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 5;
    await store.addToQueue(['A']);
    const before = streamCalls().length;
    await store.addToQueue(['A']);
    expect(streamCalls().length).toBe(before);
  });

  it('marks symbol aborted on symbol_aborted event and drains next', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 1;
    await store.addToQueue(['A', 'B']);

    eventHandler?.({ type: 'symbol_aborted', symbol: 'A' });
    await Promise.resolve();

    expect(store.queue.find((q) => q.symbol === 'A')?.status).toBe('aborted');
    expect(store.queue.find((q) => q.symbol === 'B')?.status).toBe('running');
  });
});
```

- [ ] **Step 2: 跑测试确认失败**

Run: `npm test -- src/lib/stores/invest-committee-store.test.ts`
Expected: FAIL —— `InvestCommitteeStore` 未导出(`The requested module ... does not provide an export named 'InvestCommitteeStore'`),或 `addToQueue is not a function`。

- [ ] **Step 3: 改类型区**

在 `src/lib/stores/invest-committee-store.svelte.ts` 中:

(a) `CommitteeEventType`(97-105)的 `error` 变体后加 `symbol_aborted` 变体:

```typescript
  | { type: 'symbol_complete'; symbol: string; result: CommitteeResult }
  | { type: 'done'; completed: number; total: number }
  | { type: 'error'; symbol: string; error: string }
  | { type: 'symbol_aborted'; symbol: string };
```

(b) 替换 `SymbolProgress` 接口(119-128),新增 `status` 字段:

```typescript
export type QueueItemStatus = 'queued' | 'running' | 'done' | 'failed' | 'aborted';

export interface SymbolProgress {
  activeStep: number; // stepIndex of currently running role (-1 if idle)
  completedSteps: number; // how many roles finished
  completedRounds: RoundOutputSummary[];
  done: boolean;
  error: string | null;
  result: CommitteeResult | null;
  regimeData: RegimeStepData | null; // REGIME step output (populated during streaming)
  failedSteps?: Set<number>; // explicit per-step failure from orchestrator
  status: QueueItemStatus; // queue-level scheduling status
}
```

(c) 在 `SymbolProgress` 之后(`class InvestCommitteeStore` 之前)新增队列类型:

```typescript
export interface QueueItem {
  symbol: string;
  status: QueueItemStatus;
  error?: string;
}

export interface SnapshotHolding {
  symbol: string;
  name?: string | null;
  shares?: number | null;
  notional: number;
  kind: string;
}

export interface PortfolioSnapshot {
  holdings: SnapshotHolding[];
  cash: number;
  totalNotional: number;
  timestamp: string;
}

export interface CommitteeQueueState {
  items: QueueItem[];
  snapshot?: PortfolioSnapshot | null;
  maxConcurrent: number;
  updatedAt: string;
}
```

- [ ] **Step 4: 导出 class**

把 `class InvestCommitteeStore {`(132)改为:

```typescript
export class InvestCommitteeStore {
```

(底部 `export const investCommitteeStore = new InvestCommitteeStore();` 保持不变。)

- [ ] **Step 5: 新增队列字段**

在 class 字段区(`toolCallHistory = $state<ToolCallRecord[]>([]);` 之后,即 144 行附近)新增:

```typescript
  // Queue scheduler state
  queue = $state<QueueItem[]>([]);
  maxConcurrent = $state(5);
  portfolioSnapshot = $state<PortfolioSnapshot | null>(null);
  private _saveTimer: ReturnType<typeof setTimeout> | null = null;
```

- [ ] **Step 6: 用队列调度方法替换 runCommittee**

删除整个 `async runCommittee(...) {...}` 方法(171-232),替换为以下方法集合。`runCommittee` 保留为兼容 wrapper(Task 6 末尾移除),其余为新调度逻辑:

```typescript
  // ── Derived counts ──────────────────────────────────────────────
  get queuedCount() {
    return this.queue.filter((q) => q.status === 'queued').length;
  }
  get runningCount() {
    return this.queue.filter((q) => q.status === 'running').length;
  }
  get doneCount() {
    return this.queue.filter((q) => q.status === 'done').length;
  }

  /** @deprecated compat shim — use addToQueue. Removed after Task 6. */
  runCommittee(symbols: string[]) {
    return this.addToQueue(symbols);
  }

  /** Enqueue symbols and start draining. Optional snapshot is captured once. */
  async addToQueue(symbols: string[], snapshot?: PortfolioSnapshot) {
    if (symbols.length === 0) return;
    await this._ensureListening();
    if (snapshot && !this.portfolioSnapshot) {
      this.portfolioSnapshot = snapshot;
    }
    for (const sym of symbols) {
      const existing = this.queue.find((q) => q.symbol === sym);
      if (existing && (existing.status === 'queued' || existing.status === 'running')) {
        continue; // already pending — dedup
      }
      // Re-enqueue at tail: drop any prior entry, then push fresh.
      this.queue = this.queue.filter((q) => q.symbol !== sym);
      this.queue.push({ symbol: sym, status: 'queued' });
      this.perSymbolProgress.set(sym, this._freshProgress('queued'));
      this.results = this.results.filter((r) => r.symbol !== sym);
      this.toolCallHistory = this.toolCallHistory.filter((e) => e.symbol !== sym);
    }
    this.queue = [...this.queue];
    this.perSymbolProgress = new Map(this.perSymbolProgress);
    this._recomputeRunning();
    this._persistQueue();
    this._drainQueue();
  }

  /** Re-run a finished/failed/aborted symbol — appended to the queue tail. */
  async retrySymbol(symbol: string) {
    await this.addToQueue([symbol]);
  }

  /** Cancel one in-flight symbol; backend also emits symbol_aborted. */
  async abortSymbol(symbol: string) {
    try {
      await invoke('abort_committee_symbol', { symbol });
    } catch (e) {
      console.error('abort_committee_symbol failed:', e);
    }
    this._settleQueue(symbol, 'aborted');
  }

  /** Cancel everything in flight and clear queued items. */
  async abortAll() {
    try {
      await invoke('abort_committee_all');
    } catch (e) {
      console.error('abort_committee_all failed:', e);
    }
    for (const item of this.queue) {
      if (item.status === 'running' || item.status === 'queued') {
        item.status = 'aborted';
      }
    }
    this.queue = [...this.queue];
    for (const [sym, p] of this.perSymbolProgress) {
      if (p.status === 'running' || p.status === 'queued') {
        this.perSymbolProgress.set(sym, { ...p, status: 'aborted', activeStep: -1 });
      }
    }
    this.perSymbolProgress = new Map(this.perSymbolProgress);
    this._recomputeRunning();
    this._persistQueue();
  }

  setMaxConcurrent(n: number) {
    this.maxConcurrent = n;
    this._persistQueue();
    this._drainQueue();
  }

  // ── Persistence ─────────────────────────────────────────────────
  async loadQueue() {
    try {
      const state = await invoke<CommitteeQueueState>('load_committee_queue');
      this.maxConcurrent = state.maxConcurrent && state.maxConcurrent > 0 ? state.maxConcurrent : 5;
      this.portfolioSnapshot = state.snapshot ?? null;
      // Restore queue for display; running items (interrupted by restart) → queued.
      this.queue = (state.items ?? []).map((it) => ({
        symbol: it.symbol,
        status: it.status === 'running' ? ('queued' as QueueItemStatus) : it.status,
        error: it.error,
      }));
      const progress = new Map<string, SymbolProgress>();
      for (const item of this.queue) {
        progress.set(item.symbol, this._freshProgress(item.status));
      }
      this.perSymbolProgress = progress;
      this._recomputeRunning();
    } catch (e) {
      console.error('load_committee_queue failed:', e);
    }
  }

  private _persistQueue() {
    if (this._saveTimer) clearTimeout(this._saveTimer);
    this._saveTimer = setTimeout(() => void this._flushQueue(), 300);
  }

  private async _flushQueue() {
    const state: CommitteeQueueState = {
      items: this.queue.map((q) => ({ symbol: q.symbol, status: q.status, error: q.error })),
      snapshot: this.portfolioSnapshot,
      maxConcurrent: this.maxConcurrent,
      updatedAt: new Date().toISOString(),
    };
    try {
      await invoke('save_committee_queue', { state });
    } catch (e) {
      console.error('save_committee_queue failed:', e);
    }
  }

  // ── Internal scheduling ─────────────────────────────────────────
  private _freshProgress(status: QueueItemStatus): SymbolProgress {
    return {
      activeStep: -1,
      completedSteps: 0,
      completedRounds: [],
      done: false,
      error: null,
      result: null,
      regimeData: null,
      failedSteps: new Set(),
      status,
    };
  }

  private async _ensureListening() {
    if (this._unlisten) return;
    this._unlisten = await getTransport().listen<CommitteeEventType>(
      'committee-event',
      (event) => this._handleCommitteeEvent(event),
    );
  }

  private _recomputeRunning() {
    const active = this.queue.some((q) => q.status === 'queued' || q.status === 'running');
    this.running = active;
    this.streaming = active;
  }

  private _drainQueue() {
    const running = this.runningCount;
    let slots = this.maxConcurrent - running;
    if (slots <= 0) return;
    const toStart: string[] = [];
    for (const item of this.queue) {
      if (slots <= 0) break;
      if (item.status !== 'queued') continue;
      toStart.push(item.symbol);
      slots -= 1;
    }
    for (const sym of toStart) this._startSymbol(sym);
  }

  private _startSymbol(symbol: string) {
    this._markRunning(symbol);
    invoke<CommitteeResult[]>('run_committee_stream', {
      symbols: [symbol],
      debateRounds: null,
      dryRun: false,
    }).catch((e) => {
      // Whole invoke rejected without emitting symbol_complete/error.
      const item = this.queue.find((q) => q.symbol === symbol);
      if (item && item.status === 'running') {
        this._settleQueue(symbol, 'failed', String(e));
      }
    });
  }

  private _markRunning(symbol: string) {
    const item = this.queue.find((q) => q.symbol === symbol);
    if (item) {
      item.status = 'running';
      item.error = undefined;
      this.queue = [...this.queue];
    }
    const p = this.perSymbolProgress.get(symbol);
    if (p) {
      this.perSymbolProgress.set(symbol, { ...p, status: 'running' });
      this.perSymbolProgress = new Map(this.perSymbolProgress);
    }
    this._recomputeRunning();
  }

  /** Move a symbol to a terminal/aborted status, then persist + drain. */
  private _settleQueue(symbol: string, status: QueueItemStatus, error?: string) {
    const item = this.queue.find((q) => q.symbol === symbol);
    if (item && item.status !== status) {
      item.status = status;
      item.error = error;
      this.queue = [...this.queue];
    }
    const p = this.perSymbolProgress.get(symbol);
    if (p && p.status !== status) {
      const next: SymbolProgress = { ...p, status };
      if (status === 'aborted') next.activeStep = -1;
      this.perSymbolProgress.set(symbol, next);
      this.perSymbolProgress = new Map(this.perSymbolProgress);
    }
    this._recomputeRunning();
    this._persistQueue();
    this._drainQueue();
  }
```

- [ ] **Step 7: 改造 _handleCommitteeEvent 末尾**

`_handleCommitteeEvent`(235-354)的两个 switch 内部逻辑**保持不变**,仅把方法**最后一行** `this.perSymbolProgress = progress;`(353)替换为:

```typescript
    this.perSymbolProgress = progress;

    // Queue-level transitions after progress is committed.
    if (event.type === 'symbol_complete') {
      this._settleQueue(event.symbol, 'done');
    } else if (event.type === 'error') {
      this._settleQueue(event.symbol, 'failed', event.error);
    } else if (event.type === 'symbol_aborted') {
      this._settleQueue(event.symbol, 'aborted');
    }
  }
```

> 注:`symbol_aborted` 不在第二个 switch 的 case 列表里,会安全落空(局部 `progress` Map 未改),由 `_settleQueue` 统一处理 progress.status。

- [ ] **Step 8: 跑测试确认通过**

Run: `npm test -- src/lib/stores/invest-committee-store.test.ts`
Expected: PASS —— 7 个测试全绿。

- [ ] **Step 9: 类型检查**

Run: `npm run check`
Expected: 无 error。LiveTab 仍调用 `store.runCommittee([sym])`(兼容 wrapper),不报错。

- [ ] **Step 10: Commit**

```bash
git add src/lib/stores/invest-committee-store.svelte.ts src/lib/stores/invest-committee-store.test.ts
git commit -m "feat(invest): committee store queue scheduler with abort + persistence"
```

---

## Task 5: pipeline-config 支持 aborted 状态

让步骤指示器能渲染"已中止"状态。

**Files:**
- Modify: `src/lib/components/invest/pipeline-config.ts:44-62`(`getStepState`)
- Test: `src/lib/components/invest/pipeline-config.test.ts`(新建)

**Interfaces:**
- Consumes: `SymbolProgress`(含 Task 4 新增的 `status` 字段)、`roleToBackendIdx`。
- Produces: `getStepState(...)` 返回类型新增 `'aborted'`。

- [ ] **Step 1: 写失败测试**

创建 `src/lib/components/invest/pipeline-config.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { getStepState } from './pipeline-config';
import type { SymbolProgress } from '$lib/stores/invest-committee-store.svelte';

function progress(overrides: Partial<SymbolProgress>): SymbolProgress {
  return {
    activeStep: -1,
    completedSteps: 0,
    completedRounds: [],
    done: false,
    error: null,
    result: null,
    regimeData: null,
    failedSteps: new Set(),
    status: 'running',
    ...overrides,
  };
}

describe('getStepState', () => {
  it('returns aborted for incomplete steps when symbol is aborted', () => {
    const p = progress({ status: 'aborted', completedSteps: 2 });
    // step 4 (quant_r2) not yet completed → aborted
    expect(getStepState(p, 4)).toBe('aborted');
  });

  it('keeps done steps as done even when aborted', () => {
    const p = progress({
      status: 'aborted',
      completedRounds: [
        { role: 'macro', round: 1, label: '', parsed: { rawText: '' }, latencyMs: 0, tokensUsed: 0 },
      ],
    });
    // macro (backendIdx 0) already completed → stays done
    expect(getStepState(p, 0)).toBe('done');
  });

  it('returns pending when no progress', () => {
    expect(getStepState(undefined, 0)).toBe('pending');
  });
});
```

- [ ] **Step 2: 跑测试确认失败**

Run: `npm test -- src/lib/components/invest/pipeline-config.test.ts`
Expected: FAIL —— 第一个测试期望 `'aborted'` 但实际返回 `'pending'`。

- [ ] **Step 3: 改 getStepState**

替换 `getStepState`(pipeline-config.ts:44-62),返回类型加 `'aborted'`,并在 failedSteps 检查之后、activeStep 检查之前插入 aborted 分支:

```typescript
export function getStepState(
  symProgress: SymbolProgress | undefined,
  backendIdx: number,
  pipelineStarted?: boolean,
): 'pending' | 'active' | 'done' | 'error' | 'failed' | 'aborted' {
  if (!symProgress) return 'pending';
  if (backendIdx === -1) return pipelineStarted ? 'done' : 'pending';

  // Completed steps stay done regardless of later abort.
  for (const round of symProgress.completedRounds) {
    if (roleToBackendIdx(round.role, round.round) === backendIdx) return 'done';
  }

  // Check failed steps (explicit failure from orchestrator).
  if (symProgress.failedSteps?.has(backendIdx)) return 'failed';

  // Aborted symbol: any not-yet-completed step is aborted.
  if (symProgress.status === 'aborted') return 'aborted';

  if (symProgress.activeStep === backendIdx) return 'active';
  if (symProgress.done && !symProgress.error) return 'done';
  if (symProgress.error && backendIdx >= symProgress.completedSteps) return 'error';
  return 'pending';
}
```

> 注:原函数的 `completedRounds` 循环被提到 failed 检查之前,确保已完成步骤优先判定为 `done`(测试 2 的要求)。逻辑等价——一个步骤不可能同时在 completedRounds 和 failedSteps 里。

- [ ] **Step 4: 跑测试确认通过**

Run: `npm test -- src/lib/components/invest/pipeline-config.test.ts`
Expected: PASS —— 3 个测试全绿。

- [ ] **Step 5: 回归 store 测试 + 类型检查**

Run: `npm test -- src/lib/stores/invest-committee-store.test.ts && npm run check`
Expected: 全绿,无类型 error。

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/invest/pipeline-config.ts src/lib/components/invest/pipeline-config.test.ts
git commit -m "feat(invest): getStepState supports aborted state"
```

---

## Task 6: CommitteeLiveTab 重写 — Debate Flow Grid + Abort/Retry + 并发设置

把直播页从"7 步纵向圆点列表"重写为"菱形 debate flow 卡片 + 队列驱动的 abort/retry + 页面内并发选择器"。视觉对标 `docs/demo-committee-live.html`。Svelte 组件不做单元测试(本项目测试仅覆盖 store/util),验证靠 `npm run check` + `npm run lint` + 手动冒烟。

**Files:**
- Modify (整体重写): `src/lib/components/invest/CommitteeLiveTab.svelte`

**Interfaces:**
- Consumes: Task 4 的 `store.{queue,maxConcurrent,portfolioSnapshot,perSymbolProgress,results,toolCallHistory,queuedCount,runningCount,doneCount}` + `store.{addToQueue,abortSymbol,abortAll,retrySymbol,setMaxConcurrent,loadQueue}`、`PortfolioSnapshot`/`SnapshotHolding` 类型、Task 5 的 `getStepState`(含 `'aborted'`)、`STEP_DEFS`/`getRoundForStep`(pipeline-config)、`getVerdictBadgeStyle`(`$lib/utils/invest-verdict`)、`renderMarkdown`(`$lib/utils/markdown`)、`investStore.{holdHoldings,watchHoldings,cash,holdingsMarketValue,totalAssets,totalReturnPct,holdCount,watchCount}`、Task 9 的 i18n keys。
- Produces: 无下游消费(叶子组件)。

- [ ] **Step 1: 整体重写组件**

用以下内容**完整替换** `src/lib/components/invest/CommitteeLiveTab.svelte`。

> 关于 portfolio summary:下方标注 `<!-- PRESERVE: portfolio summary -->` 处,从重写前的旧 `CommitteeLiveTab.svelte` 里把 action bar 与 symbol 卡片列表之间的 **portfolio summary 区块**(展示总资产/市值/现金/收益率/持仓数的 grid,使用 `invest.totalAssets` / `invest.holdingsMarketValue` / `invest.cash` / `invest.totalReturnPct` / `invest.holdCount` / `invest.watchCount`)原样粘贴过来。它已用现有 i18n key,无需改动。若旧文件已被覆盖,用 git 取回:`git show HEAD:src/lib/components/invest/CommitteeLiveTab.svelte`。

```svelte
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import {
    investCommitteeStore,
    type PortfolioSnapshot,
    type SnapshotHolding,
    type SymbolProgress,
  } from '$lib/stores/invest-committee-store.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { STEP_DEFS, getStepState, getRoundForStep } from './pipeline-config';
  import { getVerdictBadgeStyle } from '$lib/utils/invest-verdict';
  import { renderMarkdown } from '$lib/utils/markdown';
  import { onMount } from 'svelte';

  const store = investCommitteeStore;
  const invest = investStore;

  let includeWatch = $state(true);
  let selectedSymbols = $state<Set<string>>(new Set());
  let expandedSymbols = $state<Set<string>>(new Set());

  const CONCURRENCY_OPTIONS = [1, 2, 3, 5, 8, 10];

  // Display metadata not present in STEP_DEFS (icons live here, colors in STEP_DEFS).
  const STEP_ICONS: Record<string, string> = {
    macro: '🌐',
    regime: '🧭',
    quant_r1: '📊',
    risk_r1: '🛡',
    quant_r2: '📊',
    risk_r2: '🛡',
    cio: '👔',
  };
  const STEP_ROUND: Record<string, string> = {
    quant_r1: 'R1',
    risk_r1: 'R1',
    quant_r2: 'R2',
    risk_r2: 'R2',
  };

  function stepDef(key: string) {
    return STEP_DEFS.find((s) => s.key === key)!;
  }

  function started(p: SymbolProgress | undefined): boolean {
    return !!p && (p.status === 'running' || p.completedSteps > 0 || p.done);
  }

  function segIcon(state: string): string {
    if (state === 'done') return '✓';
    if (state === 'active') return '◉';
    if (state === 'failed') return '⚠';
    if (state === 'error') return '✗';
    if (state === 'aborted') return '⊘';
    return '';
  }

  const allAssets = $derived.by(() => {
    const assets: { symbol: string; name: string | null; kind: 'hold' | 'watch' }[] = [
      ...invest.holdHoldings.map((h) => ({ symbol: h.symbol, name: h.name, kind: h.kind })),
    ];
    if (includeWatch) {
      assets.push(
        ...invest.watchHoldings.map((h) => ({ symbol: h.symbol, name: h.name, kind: h.kind })),
      );
    }
    return assets;
  });

  function buildSnapshot(): PortfolioSnapshot {
    const holdings: SnapshotHolding[] = [
      ...invest.holdHoldings.map((h) => ({
        symbol: h.symbol,
        name: h.name,
        shares: h.shares,
        notional: h.notional,
        kind: h.kind,
      })),
      ...invest.watchHoldings.map((h) => ({
        symbol: h.symbol,
        name: h.name,
        shares: h.shares,
        notional: h.notional,
        kind: h.kind,
      })),
    ];
    return {
      holdings,
      cash: invest.cash,
      totalNotional: invest.holdingsMarketValue,
      timestamp: new Date().toISOString(),
    };
  }

  function runSelected() {
    const syms = Array.from(selectedSymbols);
    if (syms.length === 0) return;
    for (const s of syms) expandedSymbols.add(s);
    expandedSymbols = new Set(expandedSymbols);
    store.addToQueue(syms, buildSnapshot());
  }

  function runAll() {
    const syms = allAssets.map((a) => a.symbol);
    if (syms.length === 0) return;
    store.addToQueue(syms, buildSnapshot());
  }

  function toggleSel(sym: string) {
    const next = new Set(selectedSymbols);
    if (next.has(sym)) next.delete(sym);
    else next.add(sym);
    selectedSymbols = next;
  }

  function toggleAll(checked: boolean) {
    selectedSymbols = checked ? new Set(allAssets.map((a) => a.symbol)) : new Set();
  }

  function toggleExpand(sym: string) {
    const next = new Set(expandedSymbols);
    if (next.has(sym)) next.delete(sym);
    else next.add(sym);
    expandedSymbols = next;
  }

  function onConcurrencyChange(e: Event) {
    store.setMaxConcurrent(Number((e.target as HTMLSelectElement).value));
  }

  onMount(() => {
    store.loadQueue();
  });
</script>

{#snippet pipelineBar(p: SymbolProgress | undefined)}
  <div class="pipeline-bar">
    {#each STEP_DEFS as step}
      {@const state = getStepState(p, step.backendIdx, started(p))}
      <div class="seg {state}" style="--seg-color:{step.color}" title={t(step.labelKey)}>
        {segIcon(state)}
      </div>
    {/each}
  </div>
{/snippet}

{#snippet stepCard(stepKey: string, p: SymbolProgress | undefined)}
  {@const def = stepDef(stepKey)}
  {@const state = getStepState(p, def.backendIdx, started(p))}
  {@const round = getRoundForStep(p, def.backendIdx)}
  <div class="step-card {state}" style="--sc:{def.color}">
    <div class="step-head">
      <div class="step-dot {state}"></div>
      <span class="step-title">
        {STEP_ICONS[stepKey]}
        {t(def.labelKey)}
        {#if STEP_ROUND[stepKey]}<span class="step-round">{STEP_ROUND[stepKey]}</span>{/if}
      </span>
      {#if round}
        <div class="step-meta">
          <span>{(round.latencyMs / 1000).toFixed(1)}s</span>
          <span>{round.tokensUsed} tok</span>
        </div>
      {/if}
    </div>
    <div class="step-body">
      {#if state === 'active'}
        <div class="waiting"><div class="spinner"></div><span>{t('invest_committee_analyzing')}</span></div>
      {:else if state === 'aborted'}
        <span class="muted">{t('invest_committee_aborted')}</span>
      {:else if round?.parsed?.fallbackReason}
        <div class="fallback-message">
          <span class="fallback-icon">⚠</span><span>{round.parsed.fallbackReason}</span>
        </div>
      {:else if stepKey === 'regime' && p?.regimeData}
        {@const rd = p.regimeData}
        <div class="regime-box">
          <span class="regime-tag">{rd.regime}</span>
          <div class="regime-metrics">
            <span>RSI-14 {rd.metrics.rsi14.toFixed(1)}</span>
            <span>MA20 {rd.metrics.ma20.toFixed(2)}</span>
            <span>MA60 {rd.metrics.ma60.toFixed(2)}</span>
            <span>Vol {(rd.metrics.volatilityAnn * 100).toFixed(1)}%</span>
            <span>{(rd.metrics.priceQuantile2y * 100).toFixed(0)}%</span>
          </div>
          <div class="regime-hint">{rd.strategyHint}</div>
        </div>
      {:else if round?.parsed?.rawText}
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        {@html renderMarkdown(round.parsed.rawText)}
      {:else}
        <span class="muted">{t('invest_committee_waiting')}</span>
      {/if}
    </div>
  </div>
{/snippet}

<div class="space-y-3" data-invest-scope>
  <!-- Action Bar -->
  <div class="action-bar">
    <button class="btn primary" disabled={selectedSymbols.size === 0} onclick={runSelected}>
      ▶ {t('invest_committee_run_selected')}
    </button>
    <button class="btn" disabled={allAssets.length === 0} onclick={runAll}>
      ⏵ {t('invest_committee_add_all')}
    </button>
    {#if store.runningCount > 0}
      <button class="btn danger" onclick={() => store.abortAll()}>
        ⏹ {t('invest_committee_abort_all')}
      </button>
    {/if}
    <div class="action-sep"></div>
    <label class="checkbox-row">
      <input type="checkbox" bind:checked={includeWatch} />
      {t('invest_committee_include_watch')}
    </label>
    <label class="checkbox-row">
      <input
        type="checkbox"
        checked={selectedSymbols.size === allAssets.length && allAssets.length > 0}
        onchange={(e) => toggleAll(e.currentTarget.checked)}
      />
      {t('invest_committee_select_all')}
    </label>
    <div class="spacer"></div>
    <label class="conc-row">
      {t('invest_committee_concurrency')}
      <select value={store.maxConcurrent} onchange={onConcurrencyChange}>
        {#each CONCURRENCY_OPTIONS as n}
          <option value={n}>{n}</option>
        {/each}
      </select>
    </label>
    {#if store.runningCount > 0 || store.queuedCount > 0}
      <span class="progress-text">
        <span class="dot"></span>
        {t('invest_committee_in_progress', {
          current: store.doneCount,
          total: store.queue.length,
          running: store.runningCount,
        })}
      </span>
    {/if}
  </div>

  <!-- PRESERVE: portfolio summary —— 从旧组件粘贴 portfolio summary grid 区块到此处 -->

  <!-- Symbol cards -->
  {#each allAssets as asset (asset.symbol)}
    {@const p = store.perSymbolProgress.get(asset.symbol)}
    {@const queueItem = store.queue.find((q) => q.symbol === asset.symbol)}
    {@const result = p?.result ?? store.results.find((r) => r.symbol === asset.symbol) ?? null}
    {@const isExpanded = expandedSymbols.has(asset.symbol)}
    <div class="symbol-card" class:streaming={queueItem?.status === 'running'}>
      <div class="card-header" onclick={() => toggleExpand(asset.symbol)}>
        <input
          type="checkbox"
          class="card-checkbox"
          checked={selectedSymbols.has(asset.symbol)}
          onclick={(e) => {
            e.stopPropagation();
            toggleSel(asset.symbol);
          }}
        />
        <div class="card-id">
          <span class="card-name">{asset.name ?? asset.symbol}</span>
          <span class="card-ticker">{asset.symbol}</span>
        </div>
        <span class="badge {asset.kind}">{asset.kind === 'hold' ? 'HOLD' : 'WATCH'}</span>
        {@render pipelineBar(p)}
        {#if result}
          <span class="verdict-badge-sm" style={getVerdictBadgeStyle(result.finalVerdict)}>
            {result.finalVerdict}
          </span>
        {/if}
        {#if queueItem?.status === 'running'}
          <button
            class="abort-btn"
            onclick={(e) => {
              e.stopPropagation();
              store.abortSymbol(asset.symbol);
            }}
            title={t('invest_committee_abort')}
          >
            ⏹
          </button>
        {:else if queueItem && queueItem.status !== 'queued'}
          <button
            class="retry-btn"
            onclick={(e) => {
              e.stopPropagation();
              store.retrySymbol(asset.symbol);
            }}
            title={t('invest_retry')}
          >
            ↻
          </button>
        {/if}
        <span class="expand-arrow" class:open={isExpanded}>▶</span>
      </div>

      {#if isExpanded}
        {@const tools = store.toolCallHistory.filter((tc) => tc.symbol === asset.symbol)}
        <div class="card-body">
          <div class="flow-grid">
            <div class="fw">{@render stepCard('macro', p)}</div>
            <div class="fw">{@render stepCard('regime', p)}</div>

            <div class="connector">
              <svg viewBox="0 0 400 32" preserveAspectRatio="xMidYMid meet">
                <line x1="200" y1="4" x2="130" y2="28" stroke="var(--border)" stroke-width="1.5" stroke-dasharray="4,3" />
                <line x1="200" y1="4" x2="270" y2="28" stroke="var(--border)" stroke-width="1.5" stroke-dasharray="4,3" />
              </svg>
            </div>

            <div>{@render stepCard('quant_r1', p)}</div>
            <div>{@render stepCard('risk_r1', p)}</div>

            <div class="connector">
              <svg viewBox="0 0 400 32" preserveAspectRatio="xMidYMid meet">
                <line x1="130" y1="0" x2="130" y2="28" stroke="var(--border)" stroke-width="1.5" stroke-dasharray="4,3" />
                <line x1="270" y1="0" x2="270" y2="28" stroke="var(--border)" stroke-width="1.5" stroke-dasharray="4,3" />
              </svg>
            </div>

            <div>{@render stepCard('quant_r2', p)}</div>
            <div>{@render stepCard('risk_r2', p)}</div>

            <div class="connector">
              <svg viewBox="0 0 400 32" preserveAspectRatio="xMidYMid meet">
                <line x1="130" y1="0" x2="200" y2="28" stroke="var(--border)" stroke-width="1.5" stroke-dasharray="4,3" />
                <line x1="270" y1="0" x2="200" y2="28" stroke="var(--border)" stroke-width="1.5" stroke-dasharray="4,3" />
              </svg>
            </div>

            <div class="fw">{@render stepCard('cio', p)}</div>

            {#if result}
              <div class="verdict-block">
                <div class="verdict-row">
                  <span class="verdict-action" style={getVerdictBadgeStyle(result.finalVerdict)}>
                    {result.finalVerdict}
                  </span>
                  <span class="verdict-confidence">
                    {t('invest_committee_confidence')}
                    {result.finalConfidence}%
                  </span>
                  <span class="gate-badge {result.sanityCheck.gate1Pass ? 'pass' : 'fail'}">
                    {result.sanityCheck.gate1Pass ? '✓' : '✗'} Gate 1
                  </span>
                  <span class="gate-badge {result.sanityCheck.gate2Pass ? 'pass' : 'fail'}">
                    {result.sanityCheck.gate2Pass ? '✓' : '✗'} Gate 2
                  </span>
                </div>
                <div class="verdict-reasoning">{result.reasoning}</div>
                <div class="verdict-meta">
                  <span class="meta-item">⏱ {(result.totalLatencyMs / 1000).toFixed(1)}s</span>
                  <span class="meta-item">🔤 {result.totalTokens} tok</span>
                  {#if result.converged}
                    <span class="meta-item">✅ {t('invest_committee_converged')}</span>
                  {/if}
                </div>
                {#if result.sanityCheck.notes.length > 0}
                  <ul class="verdict-notes">
                    {#each result.sanityCheck.notes as note}
                      <li>{note}</li>
                    {/each}
                  </ul>
                {/if}
                {#if result.sentinelOverride}
                  <div class="sentinel-override">
                    ⚠ {result.sentinelOverride.reason} → {result.sentinelOverride.forcedVerdict}
                  </div>
                {/if}
              </div>
            {/if}

            {#if tools.length > 0}
              <div class="tool-strip">
                🔧 {t('invest_committee_tools')}：
                {#each tools as tc}
                  <span class="tool-chip">{tc.toolName} <span class="tool-ms">{tc.latencyMs}ms</span></span>
                {/each}
              </div>
            {/if}
          </div>
        </div>
      {/if}
    </div>
  {/each}

  {#if allAssets.length === 0}
    <div class="empty-hint">{t('invest_committee_queue_empty')}</div>
  {/if}
</div>

<style>
  .action-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    flex-wrap: wrap;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-input);
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
  }
  .btn:hover:not(:disabled) { background: var(--bg-hover); border-color: var(--accent-muted); }
  .btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .btn.primary { background: var(--accent); color: #111; border-color: var(--accent); }
  .btn.danger { color: var(--color-error); border-color: var(--color-error); }
  .btn.danger:hover:not(:disabled) { background: rgba(168, 122, 122, 0.12); }
  .action-sep { width: 1px; height: 24px; background: var(--border); }
  .spacer { flex: 1; }
  .checkbox-row,
  .conc-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-secondary);
    cursor: pointer;
  }
  .checkbox-row input { accent-color: var(--accent); cursor: pointer; }
  .conc-row select {
    border: 1px solid var(--border);
    background: var(--bg-input);
    color: var(--text-primary);
    border-radius: var(--radius-sm);
    padding: 3px 6px;
    font-size: 12px;
  }
  .progress-text { font-size: 12px; color: var(--text-secondary); display: flex; align-items: center; gap: 6px; }
  .progress-text .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); animation: pulse 1.5s ease-in-out infinite; }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.3; } }

  .symbol-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
    transition: border-color 0.2s;
  }
  .symbol-card:hover { border-color: var(--accent-muted); }
  .symbol-card.streaming { border-color: var(--accent); }
  .card-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    cursor: pointer;
    user-select: none;
    transition: background 0.15s;
  }
  .card-header:hover { background: var(--bg-hover); }
  .card-checkbox { accent-color: var(--accent); cursor: pointer; flex-shrink: 0; }
  .card-id { display: flex; flex-direction: column; min-width: 84px; }
  .card-name { font-size: 14px; font-weight: 600; }
  .card-ticker { font-size: 11px; color: var(--text-tertiary); font-family: var(--font-mono); }
  .badge {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.3px;
    flex-shrink: 0;
  }
  .badge.hold { background: rgba(138, 154, 118, 0.15); color: var(--color-success); }
  .badge.watch { background: rgba(196, 169, 110, 0.12); color: var(--accent-muted); }
  .verdict-badge-sm {
    padding: 3px 10px;
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    flex-shrink: 0;
  }
  .abort-btn,
  .retry-btn {
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--bg-input);
    font-size: 11px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .abort-btn { color: var(--color-error); border-color: var(--color-error); }
  .abort-btn:hover { background: rgba(168, 122, 122, 0.12); }
  .retry-btn { color: var(--color-warning); }
  .retry-btn:hover { background: rgba(255, 193, 7, 0.1); border-color: var(--color-warning); }
  .expand-arrow {
    width: 20px;
    height: 20px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-tertiary);
    transition: transform 0.2s;
    flex-shrink: 0;
    font-size: 12px;
  }
  .expand-arrow.open { transform: rotate(90deg); }
  .card-body { border-top: 1px solid var(--border); padding: 20px; }

  /* Pipeline bar */
  .pipeline-bar {
    display: flex;
    flex: 1;
    height: 22px;
    border-radius: var(--radius-sm);
    overflow: hidden;
    background: var(--bg-input);
    gap: 2px;
    margin: 0 4px;
    min-width: 200px;
  }
  .seg {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 9px;
    font-weight: 700;
    border-radius: 3px;
    transition: all 0.4s;
    color: transparent;
  }
  .seg.pending { background: var(--bg-input); }
  .seg.active { background: color-mix(in srgb, var(--seg-color) 25%, transparent); color: var(--seg-color); animation: seg-pulse 1.5s ease-in-out infinite; }
  .seg.done { background: color-mix(in srgb, var(--seg-color) 35%, transparent); color: var(--seg-color); }
  .seg.failed { background: color-mix(in srgb, var(--color-warning) 25%, transparent); color: var(--color-warning); }
  .seg.error { background: color-mix(in srgb, var(--color-error) 25%, transparent); color: var(--color-error); }
  .seg.aborted { background: color-mix(in srgb, var(--text-tertiary) 25%, transparent); color: var(--text-tertiary); }
  @keyframes seg-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }

  /* Debate flow grid */
  .flow-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    max-width: 1000px;
    margin: 0 auto;
    align-items: start;
  }
  .flow-grid .fw { grid-column: 1 / -1; justify-self: center; width: 65%; max-width: 560px; min-width: 320px; }
  .connector { grid-column: 1 / -1; display: flex; justify-content: center; height: 28px; }
  .connector svg { width: 100%; height: 100%; }
  @media (max-width: 700px) {
    .flow-grid { grid-template-columns: 1fr; }
    .flow-grid .fw { width: 100%; max-width: none; }
  }

  /* Step card */
  .step-card {
    width: 100%;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
    transition: border-color 0.3s, box-shadow 0.3s, opacity 0.4s;
  }
  .step-card.pending { opacity: 0.4; }
  .step-card.active { border-color: var(--sc); box-shadow: 0 0 20px color-mix(in srgb, var(--sc) 12%, transparent); }
  .step-card.done { border-color: color-mix(in srgb, var(--sc) 40%, transparent); }
  .step-card.failed { border-color: var(--color-warning); }
  .step-card.error { border-color: var(--color-error); }
  .step-card.aborted { border-color: var(--text-tertiary); opacity: 0.6; }
  .step-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    background: color-mix(in srgb, var(--sc) 6%, var(--bg-card));
    border-bottom: 1px solid var(--border);
  }
  .step-dot { width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; transition: all 0.3s; }
  .step-dot.pending { background: var(--bg-input); border: 1.5px solid var(--border); }
  .step-dot.active { background: var(--sc); animation: dot-pulse 1.2s ease-in-out infinite; }
  .step-dot.done { background: var(--sc); }
  .step-dot.failed { background: var(--color-warning); }
  .step-dot.error { background: var(--color-error); }
  .step-dot.aborted { background: var(--text-tertiary); }
  @keyframes dot-pulse { 0%, 100% { opacity: 1; transform: scale(1); } 50% { opacity: 0.4; transform: scale(0.7); } }
  .step-title { font-size: 13px; font-weight: 600; }
  .step-round { font-size: 10px; color: var(--text-tertiary); padding: 1px 5px; border-radius: 3px; background: var(--bg-input); }
  .step-meta { margin-left: auto; display: flex; gap: 10px; font-size: 10px; color: var(--text-tertiary); font-family: var(--font-mono); }
  .step-body {
    padding: 14px;
    font-size: 12.5px;
    color: var(--text-secondary);
    line-height: 1.85;
    max-height: 320px;
    overflow-y: auto;
    word-break: break-word;
  }
  .muted { color: var(--text-tertiary); }
  .waiting { display: flex; align-items: center; gap: 8px; color: var(--text-tertiary); }
  .spinner {
    width: 14px;
    height: 14px;
    border: 2px solid var(--border);
    border-top-color: var(--sc);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .fallback-message {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1rem;
    background: rgba(255, 193, 7, 0.1);
    border: 1px solid rgba(255, 193, 7, 0.3);
    border-radius: 6px;
    color: var(--color-warning);
  }
  .fallback-icon { font-size: 1.1rem; flex-shrink: 0; }
  .regime-box { display: flex; flex-direction: column; gap: 6px; }
  .regime-tag { align-self: flex-start; padding: 2px 10px; border-radius: var(--radius-sm); background: color-mix(in srgb, var(--sc) 18%, transparent); color: var(--sc); font-weight: 600; font-size: 12px; }
  .regime-metrics { display: flex; flex-wrap: wrap; gap: 10px; font-family: var(--font-mono); font-size: 11px; color: var(--text-tertiary); }
  .regime-hint { font-size: 12px; }

  /* Verdict block */
  .verdict-block {
    grid-column: 1 / -1;
    justify-self: center;
    width: 65%;
    max-width: 560px;
    min-width: 320px;
    background: var(--bg-base);
    border: 1px solid var(--accent-muted);
    border-radius: var(--radius-md);
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .verdict-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .verdict-action { padding: 3px 12px; border-radius: var(--radius-sm); font-size: 12px; font-weight: 700; text-transform: uppercase; }
  .verdict-confidence { font-size: 12px; color: var(--text-secondary); }
  .gate-badge { font-size: 10px; padding: 2px 7px; border-radius: 3px; }
  .gate-badge.pass { background: rgba(138, 154, 118, 0.18); color: var(--color-success); }
  .gate-badge.fail { background: rgba(168, 122, 122, 0.18); color: var(--color-error); }
  .verdict-reasoning { font-size: 12.5px; color: var(--text-secondary); line-height: 1.8; }
  .verdict-meta { display: flex; gap: 14px; font-size: 11px; color: var(--text-tertiary); font-family: var(--font-mono); }
  .verdict-notes { margin: 0; padding-left: 18px; font-size: 11.5px; color: var(--text-tertiary); }
  .sentinel-override { font-size: 12px; color: var(--color-warning); }

  /* Tool strip */
  .tool-strip {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--text-tertiary);
  }
  .tool-chip { padding: 2px 8px; border-radius: var(--radius-sm); background: var(--bg-input); font-family: var(--font-mono); }
  .tool-ms { opacity: 0.5; }
  .empty-hint { padding: 32px; text-align: center; color: var(--text-tertiary); font-size: 13px; }
</style>
```

- [ ] **Step 2: 类型检查**

Run: `npm run check`
Expected: 无 error。若报 `getVerdictBadgeStyle` 返回类型不匹配 `style` 属性,确认它返回 `string`;若报 snippet 参数类型问题,确认 `SymbolProgress` 已从 store 正确 import。

- [ ] **Step 3: Lint**

Run: `npm run lint`
Expected: 通过。`{@html renderMarkdown(...)}` 处已加 `<!-- eslint-disable-next-line svelte/no-at-html-tags -->`,沿用现有组件做法。

- [ ] **Step 4: 手动冒烟测试**

启动 `npm run tauri dev`,进入 `/invest` → 委员会 → 直播 Tab,验证:
1. 勾选 2 个标的 → Run Selected → 卡片自动展开,出现菱形 debate flow grid。
2. 运行中标的 header 出现 ⏹ abort 按钮;点击 → 该标的步骤变灰("已中止"),槽位释放,队列下一个开始。
3. 已完成/失败/中止标的 header 出现 ↻ retry → 重新入队尾运行。
4. 并发选择器改成 2 → Run All 5 个标的 → 同时最多 2 个 running,其余 queued。
5. 运行中点 Abort All → 所有 running/queued 变中止。
6. 进度文字显示 `N / M 完成 · K 进行中`。

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte
git commit -m "feat(invest): rewrite committee live tab with debate flow grid + queue abort/retry"
```

---

## Task 7: ProviderConfigPanel 移除并发设置 + 清理 store 兼容 shim

并发上限已迁移到直播页(Task 6),从设置面板移除;同时删掉 Task 4 留的 `runCommittee` 兼容 wrapper。

**Files:**
- Modify: `src/lib/components/invest/ProviderConfigPanel.svelte`(移除 `CONCURRENCY_OPTIONS`/`handleConcurrencyChange`/并发 select)
- Modify: `src/lib/stores/invest-committee-store.svelte.ts`(删除 `runCommittee` shim)

**Interfaces:**
- Consumes: 无新增。
- Produces: `maxConcurrentSymbols` 不再由设置面板写入(改由 `store.maxConcurrent` + queue 持久化管理)。

- [ ] **Step 1: 移除并发 select 渲染**

删除 `ProviderConfigPanel.svelte` 中的并发 `<select>` 区块(153-166):

```svelte
          <div>
            <label class="mr-1 text-[11px] text-[var(--text-tertiary)]">
              {t('invest_committee_concurrency')}
            </label>
            <select
              class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-[var(--space-2)] py-[var(--space-1)] text-[13px] text-[var(--text-primary)]"
              value={config.maxConcurrentSymbols ?? 5}
              onchange={handleConcurrencyChange}
            >
              {#each CONCURRENCY_OPTIONS as opt}
                <option value={opt}>{opt}</option>
              {/each}
            </select>
          </div>
```

整段删除(包括外层 `<div>`)。

- [ ] **Step 2: 移除常量与 handler**

删除 `CONCURRENCY_OPTIONS` 常量(line 10):

```typescript
const CONCURRENCY_OPTIONS = [1, 2, 3, 5, 8, 10];
```

删除 `handleConcurrencyChange` 函数(50-54):

```typescript
function handleConcurrencyChange(e: Event) {
  const target = e.target as HTMLSelectElement;
  config.maxConcurrentSymbols = Number(target.value);
  scheduleSave();
}
```

- [ ] **Step 3: 删除 store 兼容 shim**

在 `src/lib/stores/invest-committee-store.svelte.ts` 中删除 Task 4 Step 6 加的兼容 wrapper:

```typescript
  /** @deprecated compat shim — use addToQueue. Removed after Task 6. */
  runCommittee(symbols: string[]) {
    return this.addToQueue(symbols);
  }
```

(LiveTab 已全部改用 `addToQueue`/`retrySymbol`,无残留调用。)

- [ ] **Step 4: 类型检查**

Run: `npm run check`
Expected: 无 error。若报 "runCommittee does not exist",说明某处仍在调用——grep `runCommittee` 在 `src/` 下确认仅剩 store 测试中无引用,LiveTab 已改完。

> 验证命令:`grep -rn "runCommittee" src/`,预期仅匹配本计划无关的注释或零结果。

- [ ] **Step 5: Lint**

Run: `npm run lint`
Expected: 通过(无未使用变量 `CONCURRENCY_OPTIONS`/`config` 警告)。

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/invest/ProviderConfigPanel.svelte src/lib/stores/invest-committee-store.svelte.ts
git commit -m "refactor(invest): move concurrency control to live tab, drop runCommittee shim"
```

---

## Task 8: 删除孤儿组件 PipelineFlow.svelte

`PipelineFlow.svelte` 在 `src/` 下 0 引用(探查确认),被 debate flow card 布局取代。

**Files:**
- Delete: `src/lib/components/invest/PipelineFlow.svelte`

**Interfaces:**
- Consumes: 无。
- Produces: 无。

- [ ] **Step 1: 二次确认无引用**

Run: `grep -rn "PipelineFlow" src/`
Expected: 零结果(组件未被任何文件 import)。若有结果,停止并先处理引用方。

- [ ] **Step 2: 删除文件**

```bash
git rm src/lib/components/invest/PipelineFlow.svelte
```

- [ ] **Step 3: 类型检查**

Run: `npm run check`
Expected: 无 error。

- [ ] **Step 4: Commit**

```bash
git commit -m "chore(invest): remove orphaned PipelineFlow component"
```

---

## Task 9: i18n keys

新增直播页 UI 文案,en/zh 同步。

**Files:**
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

**Interfaces:**
- Consumes: 无。
- Produces: 以下 key 供 Task 6 的 `t(...)` 调用。

> 这些 key 在 Task 6 的 LiveTab 里已被引用。本 task 补齐定义,使 `npm run i18n:check` 通过。

- [ ] **Step 1: 加 en.json keys**

在 `messages/en.json` 中,找到现有 `invest_committee_concurrency` 所在区域(该 key 已存在,被 Task 7 移除使用方但 key 保留),在其附近加入以下键(注意 JSON 末尾逗号合法性):

```json
  "invest_committee_run_selected": "Run Selected",
  "invest_committee_add_all": "Run All",
  "invest_committee_abort": "Abort",
  "invest_committee_abort_all": "Abort All",
  "invest_committee_include_watch": "Include Watch",
  "invest_committee_select_all": "Select All",
  "invest_committee_aborted": "Aborted",
  "invest_committee_waiting": "Waiting to start",
  "invest_committee_analyzing": "LLM analyzing…",
  "invest_committee_queue_empty": "No symbols. Add holdings or watch items first.",
  "invest_committee_confidence": "Confidence",
  "invest_committee_converged": "Converged",
  "invest_committee_tools": "Tools",
  "invest_committee_in_progress": "{current} / {total} done · {running} running"
```

- [ ] **Step 2: 加 zh-CN.json keys**

在 `messages/zh-CN.json` 对应位置加入相同 key 的中文:

```json
  "invest_committee_run_selected": "运行所选",
  "invest_committee_add_all": "运行全部",
  "invest_committee_abort": "终止",
  "invest_committee_abort_all": "全部终止",
  "invest_committee_include_watch": "包含观望",
  "invest_committee_select_all": "全选",
  "invest_committee_aborted": "已终止",
  "invest_committee_waiting": "等待开始",
  "invest_committee_analyzing": "LLM 正在分析…",
  "invest_committee_queue_empty": "暂无标的,请先添加持仓或观望项。",
  "invest_committee_confidence": "置信度",
  "invest_committee_converged": "已收敛",
  "invest_committee_tools": "工具",
  "invest_committee_in_progress": "{current} / {total} 完成 · {running} 进行中"
```

- [ ] **Step 3: i18n 校验**

Run: `npm run i18n:check`
Expected: 通过 —— en 与 zh-CN 键集合一致,无缺失。

> 若 `invest_committee_concurrency` 因 Task 7 移除使用方而被 i18n:check 报"未使用",确认 LiveTab(Task 6)的 `.conc-row` 仍调用 `t('invest_committee_concurrency')` —— 它在直播页并发选择器中复用,不会变孤儿。

- [ ] **Step 4: 类型检查(message 类型重新生成)**

Run: `npm run check`
Expected: 无 error。Task 6 中引用的 i18n key 现已全部定义,`t(...)` 类型不报 "unknown key"。

- [ ] **Step 5: Commit**

```bash
git add messages/en.json messages/zh-CN.json
git commit -m "feat(invest): i18n keys for committee live queue UI"
```

---

## Task 10: simplify 代码审查 + 全量验证

按项目标准工作流(CLAUDE.md §"Standard workflow")执行 simplify 三路审查,修复发现项,然后跑全量验证。

**Files:**
- 视审查结果而定(预期触及 Task 1-9 改动文件)。

**Interfaces:**
- Consumes: Task 1-9 全部产物。
- Produces: 通过全量验证的最终代码。

- [ ] **Step 1: 运行 simplify 审查**

调用 `simplify` skill(三路并行 agent),审查范围为本计划全部改动。重点关注:
- **复用**:`STEP_DEFS`/`getStepState`/`getRoundForStep`/`roleToBackendIdx` 无重复实现;`_freshProgress` 统一构造 `SymbolProgress`;snapshot 构造逻辑(Task 6 `buildSnapshot` 的 hold/watch map 重复)可否抽辅助函数。
- **质量**:`_persistQueue` debounce(300ms)正确清理 timer;`perSymbolProgress = new Map(...)` reactivity 触发一致;queue 去重逻辑无竞态;`check_cancellation` 在每个 phase 边界覆盖完整。
- **效率**:`store.queue.find(...)` 在 each 块内的重复查找;`toolCallHistory.filter` 每次渲染重算可否 `$derived` 分桶;`results.find` vs `perSymbolProgress` 双源 result 是否冗余。

- [ ] **Step 2: 修复审查发现项**

按审查反馈逐项修复。每修一处后跑对应验证(前端 `npm run check`,后端 `cargo check`)。

- [ ] **Step 3: 前端全量验证**

Run: `npm run check && npm run lint && npm run i18n:check`
Expected: 全部通过。

- [ ] **Step 4: 前端测试**

Run: `npm test -- src/lib/stores/invest-committee-store.test.ts src/lib/components/invest/pipeline-config.test.ts`
Expected: PASS —— store 7 个 + pipeline-config 3 个测试全绿。

- [ ] **Step 5: 前端构建**

Run: `npm run build`
Expected: 构建成功,无 error。

- [ ] **Step 6: 后端验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过,无 error/warning(queue 模块已被 commands 接入)。

> 注:`cargo test` 在本机无法运行(STATUS_ENTRYPOINT_NOT_FOUND,CLAUDE.md §11)。queue.rs 与 events 的单元测试已写入,将在 CI/干净环境运行。

- [ ] **Step 7: 更新文档**

按 CLAUDE.md 工作流,在 `docs/changelog.md` 顶部加 v5.5.0 条目(参照现有条目格式),并更新 CLAUDE.md "Implementation history" 表格新增一行,Overview 的 "Current phase" 改为新版本号。

```bash
git add docs/changelog.md CLAUDE.md
git commit -m "docs: v5.5.0 changelog — committee live UI redesign (debate flow + queue abort)"
```

- [ ] **Step 8: 提交审查修复**

```bash
git add -A
git commit -m "fix(invest): simplify review findings for committee live UI redesign"
```

---

## 执行顺序与依赖

```
Task 1 (queue.rs)           ← 纯后端,独立
  ↓
Task 2 (CancellationToken)  ← 依赖无;Step 8 预期下游报错(正常)
  ↓
Task 3 (Tauri commands)     ← 依赖 Task 1+2,修复 Task 2 的下游报错
  ↓
Task 4 (store 队列状态机)   ← 前端核心,真 TDD;依赖 Task 3 命令名
  ↓
Task 5 (pipeline-config)    ← 依赖 Task 4 的 SymbolProgress.status;真 TDD
  ↓
Task 6 (LiveTab 重写)       ← 依赖 Task 4+5
  ↓
Task 7 (移除并发设置+shim)  ← 依赖 Task 6 接管并发
Task 8 (删 PipelineFlow)    ← 独立小改
Task 9 (i18n)               ← 依赖 Task 6 引用的 key
  ↓
Task 10 (simplify + 验证)   ← 全量
```

后端(1-3)与前端(4-9)可由不同 worker 并行,但前端 Task 4 的命令名必须与 Task 3 一致(已在 Interfaces 中锁定:`run_committee_stream`/`abort_committee_symbol`/`abort_committee_all`/`load_committee_queue`/`save_committee_queue`)。

## 最终验证清单

- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` —— 后端编译
- [ ] `npm run check` —— 前端类型
- [ ] `npm run lint` —— 前端 lint
- [ ] `npm run i18n:check` —— i18n 一致性
- [ ] `npm test` —— Vitest(store + pipeline-config)
- [ ] `npm run build` —— 前端构建
- [ ] 手动:Run Selected → 卡片展开 debate flow grid → 600 字文本滚动
- [ ] 手动:运行中 abort 单个 → 步骤变灰 → 槽位释放 → 下一个开始
- [ ] 手动:retry → 入队尾重跑
- [ ] 手动:并发=2 → Run All → 最多 2 个 running
- [ ] 手动:Abort All → 全部中止
- [ ] 手动:重启 app → loadQueue 恢复队列 → 原 running 重置为 queued

## 风险与回滚

- **取消粒度**:取消在 phase 边界生效,不会中断进行中的单次 LLM 调用(可能延迟到当前 role 完成)。这是有意权衡——避免在 LLM HTTP 请求中途强杀。若需更细粒度,后续可把 `CancellationToken` 传入 `collect_stream` 的 select! 循环(超出本计划范围)。
- **并发模型变更**:stream 路径下后端 Semaphore 退化为单 symbol,真正的并发由前端控制。若前端崩溃,in-flight 的后端 task 仍会跑完(无 emitter 接收方),但不会泄漏(task 自然结束)。
- **回滚**:每个 Task 独立 commit。回滚顺序为 Task 10→1 逆序 `git revert`。Task 6(LiveTab 重写)回滚后需确认旧 `runCommittee` shim 是否已被 Task 7 删除——若已删,需一并 revert Task 7。

