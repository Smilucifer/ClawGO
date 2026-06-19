# 批次 C — 功能删除 / 数据源补全 / llm_config 迁移 / DB 清理 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 删除两个业务无用前端 tab 与一批孤儿后端命令(任务1+5),为两个真正缺 UI 的命令接线(任务1),补全数据源检测(任务6),把 event 分析迁移到 CLI 后删除 `llm_config.json` 全链路(任务6 关联),清理数据库与文件系统中的无用数据(任务2)。

**Architecture:** 五个相对独立的任务,各自可验证、可单独 commit。删除类先做(C1),接线类(C2),数据源(C3),最高风险的 event 迁移(C4,严格"先迁移后删除"),DB 清理放最后并做成"只读核对→报告→确认→删除"的一次性命令(C5)。

**Tech Stack:** Rust(Tauri commands、SQLite rusqlite)、Svelte 5、i18n(en.json + zh-CN.json)、CliCommitteeExecutor。

## Global Constraints

- 关联 spec:`docs/superpowers/specs/2026-06-19-multi-module-maintenance-design.md`(批次 C 节)。
- 本机 Rust 单测有 §11 运行时问题 → 编译验证用 `cargo check --manifest-path src-tauri/Cargo.toml` + `cargo clippy ... -- -D warnings`。
- **C4 铁律:先迁移 event_scanner/event_analyzer 到 CLI(Task 4 Step 1-4),再删除 llm_config 链路(Step 5+)。** 顺序颠倒会导致 event_scan/event_analyzer cron 每 10 分钟报错。
- **C5 铁律:删行前必须先 SELECT 报告行数与样本,经用户确认后才删。** 删除做成显式一次性命令,不进自动启动路径。
- 删除/新增任何 UI 文案都要同步 `messages/en.json` + `zh-CN.json` 并通过 `npm run i18n:check`。
- `save_memory`(lib.rs:270)与 `restart_python_runtime`(lib.rs:479)**已在 generate_handler! 注册**,C2 只需加前端 UI,不重复注册。
- Conventional Commits;每个 Task 结束 commit。

## 关键事实(已核对)

- 孤儿命令在 `src-tauri/src/lib.rs` 注册行:`add_holding`(404)、`update_holding`(405)、`delete_holding`(406)、`set_initial_cash`(414)、`update_cash`(415)、`save_verdict`(417)、`save_pnl_snapshot`(419)、`save_event`(422)、`get_event_sources`(424)、`save_event_source`(425)、`is_trading_day`(426)、`get_scheduler_logs`(427)、`get_strategy`(428)、`get_daily_bars`(435)、`sync_trade_calendar`(437)、`get_llm_config`(440)、`save_llm_config`(441)、`run_committee`(442,非流式)、`get_tracking_status`(459)、`list_all_tracking`(460)、`generate_daily_report`(474)、`list_daily_reports`(475)、`get_regime_classification`(476)。`run_committee_stream`(443)是现役**保留**。
- `CliCommitteeExecutor::global() -> Option<Self>`;`run_role(system_prompt, user_message, timeout_secs, settings_path: Option<&Path>) -> Result<String, String>`(`cli_executor.rs:56, 72`)。
- `event_scanner::normalize_events(client, config, raw_events, system_prompt)`(`:165`)与 `event_analyzer::normalize_events_batch(client, config, events, system_prompt)`(`:113`)模式相同:`client.chat_stream(...)` → `collect_stream` → `parse_normalized_response(content, events, fallback)`。
- `build_scan_clients()`(`commands/invest.rs:1103`)构造 `OpenAiCompatClient` + `LlmConfig`;`scan_events`(`:1131`)、scheduler `runner.rs:77` 调用它。
- 数据源检测 `get_datasource_health`(`commands/invest.rs:1391`),LLM Config 行 `:1442-1497`。

---

## Task 1: 删除两个前端 tab + tool-strip + store 死状态 + regime 命令(任务5 + 任务1 部分)

**Files:**
- Delete: `src/lib/components/invest/CommitteeToolsTab.svelte`
- Delete: `src/lib/components/invest/SystemRegimeTab.svelte`
- Modify: `src/routes/invest/+page.svelte`(移除 `tools`/`regime` sub-tab 注册与渲染、相关 import)
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`(删 tool-strip `:410-417`、`toolMap` `:87-95`)
- Modify: `src/lib/stores/invest-committee-store.svelte.ts`(删 `toolCallHistory` 字段、`ToolCallRecord` 类型、`tool_call` 事件分支 `:519-534`、入队清理 `:251`)
- Modify: `src-tauri/src/commands/invest.rs`(删 `get_regime_classification` `:1354-1378`)
- Modify: `src-tauri/src/lib.rs:476`(删 `get_regime_classification` 注册)
- Modify: `messages/en.json`、`zh-CN.json`(删相关 key)

**Interfaces:**
- Consumes: 无。
- Produces: 委员会 sub-tabs 减为 `live/replay/archive/roles/accuracy`;系统 sub-tabs 移除 `regime`。

- [ ] **Step 1: 读取 invest/+page.svelte 确认 tab 注册结构**

先 Read `src/routes/invest/+page.svelte`,定位:committee sub-tab 数组(含 `tools`)、system sub-tab 数组(含 `regime`)、`{#if systemSubTab === 'regime'}` / `{:else if committeeSubTab === 'tools'}` 渲染分支、`SystemRegimeTab`/`CommitteeToolsTab` 的 import 行。记录确切行号。

- [ ] **Step 2: 移除 tab 注册、渲染分支与 import**

在 `src/routes/invest/+page.svelte`:
- 从 committee sub-tab 数组删除 `tools` 项;从 system sub-tab 数组删除 `regime` 项。
- 删除 `{:else if ... === 'tools'} <CommitteeToolsTab />` 与 `{:else if systemSubTab === 'regime'} <SystemRegimeTab />` 渲染分支。
- 删除两个组件的 `import` 行。

- [ ] **Step 3: 删除两个组件文件**

```bash
git rm src/lib/components/invest/CommitteeToolsTab.svelte
git rm src/lib/components/invest/SystemRegimeTab.svelte
```

- [ ] **Step 4: 删除 CommitteeLiveTab 的 tool-strip 与 toolMap**

`src/lib/components/invest/CommitteeLiveTab.svelte`:
- 删除 `toolMap` derived(`:87-95`)。
- 删除展开体内 `{#if tools.length > 0}<div class="tool-strip">...</div>{/if}`(`:410-417`)及其上方 `{@const tools = toolMap.get(asset.symbol) ?? []}`(`:335`)。
- 删除 `.tool-strip` / `.tool-chip` / `.tool-ms` CSS(`:671-677`)。

> 注:若批次 B 已先合入,行号会变 —— 实现时按 class 名/标识定位而非硬行号。

- [ ] **Step 5: 删除 store 中的 toolCallHistory 死状态**

`src/lib/stores/invest-committee-store.svelte.ts`:
- 删除 `ToolCallRecord` interface(`:128-138`)。
- 删除 `toolCallHistory = $state<ToolCallRecord[]>([])` 字段(`:207`)。
- 删除 `_handleCommitteeEvent` 中 `tool_call` 事件分支(`:519-534`)。
- 删除入队时 `this.toolCallHistory = this.toolCallHistory.filter(...)`(`:251`)。
- 从 `CommitteeEventType` 联合类型移除 `tool_call`(`:122`),或保留类型但不处理(若后端仍发,保留类型避免 TS 报未知事件;选保留类型、删处理分支——实现时若 `_handleCommitteeEvent` 有 exhaustive 检查则保留 `tool_call` case 为 no-op)。

- [ ] **Step 6: 删除 get_regime_classification 命令与注册**

- `src-tauri/src/commands/invest.rs`:删除 `get_regime_classification` 函数(`:1354-1378`)。
- `src-tauri/src/lib.rs:476`:删除 `commands::invest::get_regime_classification,` 注册行。
- 确认 `src-tauri/src/invest/regime.rs` **不动**(委员会依赖)。

- [ ] **Step 7: 清理 i18n key**

grep `invest_system_sub_regime`、`invest_system_regime_`、`invest_committee_tools`(及 CommitteeToolsTab/SystemRegimeTab 专用 key),从 `en.json` + `zh-CN.json` 删除仅被这两个组件使用的键。保留 `invest_committee_tools` 若 tool-strip 外仍有引用——grep 确认无其它引用后再删。

- [ ] **Step 8: 验证**

Run:
```bash
npm run check
npm run i18n:check
cargo check --manifest-path src-tauri/Cargo.toml
```
Expected: 前端 svelte-check 0 errors(无对已删组件/字段的引用残留)、i18n 无缺键也无未使用告警阻断、Rust 编译通过。

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat(invest): 删除委员会-Tool调用 tab、系统-市场Regime tab 及 toolCallHistory 死状态"
```

---

## Task 2: 为 restart_python_runtime 与 save_memory 接线前端入口(任务1 接线部分)

**Files:**
- Modify: Python 状态相关组件(`src/lib/components/PythonSetupOverlay.svelte` 或 Python 状态展示处 —— Step1 确认)
- Modify: `src/routes/memory-mgmt/+page.svelte`(加"手动新增记忆"入口)
- Modify: `messages/en.json`、`zh-CN.json`(按钮/表单文案)

**Interfaces:**
- Consumes: `restart_python_runtime`(已注册,无参,返回状态)、`save_memory`(已注册;先读 `commands/memos.rs:58` 确认参数签名)。
- Produces: 两个 UI 入口。

- [ ] **Step 1: 确认两个命令签名与现有 UI 挂载点**

Read `src-tauri/src/commands/python_status.rs:60`(`restart_python_runtime` 签名/返回)、`src-tauri/src/commands/memos.rs:41-93`(`save_memory` 参数,如 content/scope/type/tags)。Read `src/lib/components/PythonSetupOverlay.svelte` 与 `src/routes/memory-mgmt/+page.svelte` 确认 invoke 调用模式(经 `getTransport().invoke(...)`)与现有按钮/表单结构。

- [ ] **Step 2: 加 Python 重启按钮**

在 Python 状态展示组件(Step1 确认的文件)加一个"重启 Python 运行时"按钮,onclick 调 `getTransport().invoke('restart_python_runtime')`,带 loading 态与成功/失败提示(沿用该组件已有的状态刷新逻辑,如调用后重新 `get_python_status`)。文案 key:`python_restart_runtime`。

- [ ] **Step 3: 加手动新增记忆入口**

在 `src/routes/memory-mgmt/+page.svelte` 加一个"新增记忆"按钮 + 简单表单(content 文本域 + type 下拉 [fact/preference/skill/feedback] + tags 输入),提交时调 `getTransport().invoke('save_memory', { ... })`(参数按 Step1 确认的签名),成功后刷新 `list_memories`。文案 key:`memory_mgmt_add`、`memory_mgmt_add_content`、`memory_mgmt_add_type`、`memory_mgmt_add_tags`、`memory_mgmt_add_submit`。

- [ ] **Step 4: 新增 i18n 键**

`zh-CN.json` / `en.json` 各加上述键。示例(zh-CN):
```json
  "python_restart_runtime": "重启 Python 运行时",
  "memory_mgmt_add": "新增记忆",
  "memory_mgmt_add_content": "内容",
  "memory_mgmt_add_type": "类型",
  "memory_mgmt_add_tags": "标签(逗号分隔)",
  "memory_mgmt_add_submit": "保存",
```
en.json 对应英文。

- [ ] **Step 5: 验证**

Run: `npm run check && npm run i18n:check`
Expected: 通过。

- [ ] **Step 6: 运行目测**

`npm run tauri dev`:
- Python 状态处点"重启运行时",确认 Python 子进程重启(可在卡死/正常态各试一次,看状态刷新)。
- `/memory-mgmt` 新增一条记忆,确认列表出现该条且 `memory.db` 写入成功。

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(invest): 接线 restart_python_runtime 与 save_memory 前端入口"
```

---

## Task 3: 删除其余孤儿命令(任务1 删除部分)

**Files:**
- Modify: `src-tauri/src/commands/invest.rs`(删除孤儿命令函数)
- Modify: `src-tauri/src/lib.rs`(删除对应注册行)

**Interfaces:**
- Consumes: 无。
- Produces: IPC 表瘦身。

**说明:** `get_llm_config`/`save_llm_config`/`run_committee` 留到 Task 4(随 llm_config 迁移一起删,因 `build_committee_config` 等仍引用 `get_llm_config`)。本任务删除其余确认无前端、无内部调用方的孤儿命令。

**⚠️ 例外 — `sync_trade_calendar` 不能整删:** 已核实 `commands/invest.rs:584` 的 `init_invest_data`(现役 IPC,lib.rs:439 注册)在 Rust 内直接调用 `sync_trade_calendar(token).await`,且无同名 storage 层替代。整删函数体会让 `init_invest_data` 编译失败。对它**只删 `#[tauri::command]` 属性 + lib.rs 注册行,保留私有 `async fn`**(供 init_invest_data 继续调用)。

- [ ] **Step 1: 逐个确认无调用方**

对下列命令,先 grep 确认前端无 invoke、后端无内部调用(deprecated 的 add/update_holding 已知):
`add_holding`、`update_holding`、`delete_holding`、`set_initial_cash`、`update_cash`、`save_verdict`、`save_pnl_snapshot`、`save_event`、`get_event_sources`、`save_event_source`、`is_trading_day`、`get_scheduler_logs`、`get_strategy`、`get_daily_bars`、`get_tracking_status`、`list_all_tracking`、`generate_daily_report`、`list_daily_reports`。

(`sync_trade_calendar` **不在整删清单**,见上方例外。)

grep 命令(示例,逐个跑):
```bash
rg "save_event_source|get_event_sources" src/ src-tauri/src/ --type-add 'svelte:*.svelte' -t svelte -t ts -t rust
```
若某命令仍被内部函数调用(如 `save_pnl_snapshot` 的内部版本 `verdicts::save_pnl_snapshot` 在 `lib.rs:137` 被 scheduler 用——注意那是 storage 层函数,不是 IPC command,二者同名不同路径),则只删 IPC command 包装,保留 storage 层函数。

- [ ] **Step 2: 删除函数定义**

在 `src-tauri/src/commands/invest.rs` 删除上述每个 `#[tauri::command]` 函数定义。`is_trading_day` 若被 scheduler 内部用的是 `scheduler::is_trading_day`(不同路径),只删 commands 层包装。
**`sync_trade_calendar`:只删除其 `#[tauri::command]` 属性那一行,保留 `pub async fn sync_trade_calendar(...)` 函数体**(init_invest_data 仍调用)。

- [ ] **Step 3: 删除 lib.rs 注册行**

删除 `src-tauri/src/lib.rs` 对应注册行:404、405、406、414、415、417、419、422、424、425、426、427、428、435、437(`sync_trade_calendar` —— 仅删此注册行,函数保留)、459、460、474、475。(行号以删除前为准;逐行按命令名匹配删除,避免错删。)

- [ ] **Step 4: 处理 deprecated 标记与 import**

删除后,若 `lib.rs:141` 的 `#[allow(deprecated)]` 因 add_holding/update_holding 已删而不再需要,移除它;若仍有其它 deprecated 引用则保留。清理 `invest.rs` 中因删除而 unused 的 import。

- [ ] **Step 5: 验证**

Run:
```bash
cargo check  --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```
Expected: 编译通过、clippy 无 warning(尤其无 unused import / dead_code 残留)。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/invest.rs src-tauri/src/lib.rs
git commit -m "chore(invest): 删除 19 个已注册但无前端连接的孤儿命令"
```

---

## Task 4: event 分析迁移到 CLI + 删除 llm_config 全链路(任务6 关联)

**Files:**
- Modify: `src-tauri/src/invest/event_scanner.rs`(`normalize_events` `:165-205`)
- Modify: `src-tauri/src/invest/event_analyzer.rs`(`normalize_events_batch` `:113-159`、`analyze_pending_events` 签名)
- Modify: `src-tauri/src/commands/invest.rs`(`build_scan_clients` `:1103`、`scan_events` `:1131`、`get_datasource_health` LLM 行 `:1442-1497`、`get_llm_config`/`save_llm_config`/`InvestLlmConfig` 等 `:601-789`)
- Modify: `src-tauri/src/invest/scheduler/runner.rs:77`(event_scan / event_analyzer 调用)
- Delete: `src-tauri/src/invest/llm/client.rs`(`OpenAiCompatClient`)
- Modify: `src-tauri/src/invest/committee/orchestrator.rs`(死代码 `build_llm_config`/`llm_call_with_retry`/`retry_on_fallback`/`run_with_tool_loop`)
- Modify: `src-tauri/src/lib.rs:440-441`(删 get/save_llm_config 注册)
- Delete: `src/lib/components/invest/ProviderConfigPanel.svelte`(评估后)
- Modify: `src/lib/stores/invest-committee-store.svelte.ts`(`loadConfig`/`saveConfig`/`llmConfig` `:495-509`)
- Modify: i18n(llm config 相关键)

**Interfaces:**
- Consumes: `CliCommitteeExecutor::global()` + `run_role(system, user, 0, settings_path)`。
- Produces: event 归一化走 CLI;`OpenAiCompatClient`/`llm_config.json`/`get_llm_config`/`save_llm_config` 移除;委员会调参(debate_rounds/selected_provider/timeout/max_concurrent)迁到保留结构。

### 阶段一:迁移(必须先做)

- [ ] **Step 1: 给 event 归一化加 CLI 执行辅助**

在 `src-tauri/src/invest/` 下新增一个共享辅助(放 `event_analyzer.rs` 顶部或新 `llm_cli.rs`),把 system+items 拼成单次 CLI 调用并返回文本:

```rust
/// Run a single text-completion via the committee CLI executor.
/// Used by event scanner/analyzer normalization (replaces OpenAiCompatClient).
pub async fn cli_complete(system_prompt: &str, user_message: &str) -> Result<String, String> {
    let exec = crate::invest::committee::cli_executor::CliCommitteeExecutor::global()
        .ok_or("claude CLI not available")?;
    // settings_path: None → use ambient ~/.claude settings (committee uses platform_credentials via write_committee_settings_json;
    // for event normalization the default provider/settings is acceptable — confirm during impl whether a settings json is needed)
    exec.run_role(system_prompt, user_message, 0, None).await
}
```

> 实现时确认:committee 角色调用通过 `write_committee_settings_json` 生成 `--settings` 临时 JSON 注入 platform_credentials。event 归一化若也需指定 provider,则复用同一 settings 生成逻辑并把 path 传入 `run_role`;若用默认 claude 即可,则 `None`。Step1 先按 `None` 实现,Step6 目测验证归一化结果正常,否则补 settings。

- [ ] **Step 2: 改 event_scanner::normalize_events 走 CLI**

替换 `event_scanner.rs:191-204` 的 chat_stream 段:

```rust
    // Call LLM via committee CLI executor
    let content = match crate::invest::event_analyzer::cli_complete(system, &items).await {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Event normalizer CLI call failed: {}, falling back to rule-based", e);
            return raw_events.iter().map(|ev| fallback_normalize_from(&ev.title, &ev.body)).collect();
        }
    };

    // Parse JSON response
    parse_normalized_response(&content, raw_events, |ev| fallback_normalize_from(&ev.title, &ev.body))
```

同步修改 `normalize_events` 签名:删除 `client: &dyn InvestLlmClient, config: &LlmConfig` 参数(改为只接 `raw_events` + `system_prompt`)。更新 `scan_events`(`event_scanner.rs` 内调用处)与 `commands/invest.rs::scan_events` 调用点。

- [ ] **Step 3: 改 event_analyzer::normalize_events_batch 走 CLI**

替换 `event_analyzer.rs:137-158` 的 chat_stream 段为 `cli_complete(system_prompt, &items)`,同样删除 `client`/`config` 参数;更新 `analyze_pending_events` 签名(删 `&llm_client, &llm_config`)与 `runner.rs` 中 event_analyzer 分支调用点(不再需要 `build_scan_clients`)。

- [ ] **Step 4: 改 scheduler runner 与 scan_events 命令不再构造 LLM client**

`src-tauri/src/invest/scheduler/runner.rs`:**`build_scan_clients()` 在该文件有两处调用 —— `:55`(event_scan 分支)与 `:77`(event_analyzer 分支),两处都要改。** event_scan 仍需 tushare,改为只取 tushare;event_analyzer 不再需要 client/config 参数。两处都去掉 client/config 形参传递。
`commands/invest.rs::scan_events`(`:1131`):同样改为只构造 tushare,调用迁移后的 `normalize_events`/`scan_events`。

- [ ] **Step 5: 编译验证迁移完成(此时 llm_config 仍在)**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过。**此处确认迁移自洽后再进入删除阶段。**

- [ ] **Step 6: 运行目测迁移正确性**

`npm run tauri dev`,触发一次事件扫描(`/invest` 事件相关入口或等 cron),确认事件被正常归一化(severity/stance/symbols 填充),CLI 路径工作。若归一化质量异常,回到 Step1 补 settings_path。

- [ ] **Step 7: 提交迁移(独立 commit,便于回滚)**

```bash
git add src-tauri/src/invest/event_scanner.rs src-tauri/src/invest/event_analyzer.rs src-tauri/src/invest/scheduler/runner.rs src-tauri/src/commands/invest.rs
git commit -m "refactor(invest): event 扫描/分析迁移到 CLI 执行器,不再依赖 OpenAiCompatClient"
```

### 阶段二:删除 llm_config 链路

- [ ] **Step 8: 迁移委员会调参到保留结构**

`build_committee_config`(`commands/invest.rs:796-836`)从 `InvestLlmConfig` 读取:`selected_provider`、`debate_rounds`、`timeout_secs`、`max_concurrent_symbols`,以及 `providers[]`——**`model_override` 不是 `InvestLlmConfig` 的直接字段,而是从 `providers[selected_provider].default_model` 派生**,再连同 `selected_provider`(经 `try_parse_provider_id`)传给 `write_committee_settings_json(platform_id, model_override)`。

新增一个轻量结构(如 `CommitteeTuning`,持久化到 `~/.claw-go/invest/committee_tuning.json` 或并入 `UserSettings`)承载:`selected_provider: String`、`model: String`(即原 default_model,直接作为 model_override,免去 providers 数组)、`debate_rounds`、`timeout_secs`、`max_concurrent_symbols`。改 `build_committee_config` 读它并保留 `try_parse_provider_id` + `write_committee_settings_json` 调用入口。

> 实现时先 Read `build_committee_config` 全文(`:796-836`)确认它消费的字段与 model_override 派生逻辑,再决定新结构形状。前端 `ProviderConfigPanel` 若仅控制这几个字段,改为读写新结构。

- [ ] **Step 9: 删除 OpenAiCompatClient 与 build_scan_clients**

- 删除 `src-tauri/src/invest/llm/client.rs` 整文件(`OpenAiCompatClient`/`resolve_api_key`/`get_llm_config_path`)。
- 删除 `commands/invest.rs::build_scan_clients`(`:1103`)。
- 评估 `src-tauri/src/invest/llm/` 模块:`LlmConfig`/`Message`/`InvestLlmClient`/`collect_stream`/`ProviderId` 若仅剩死代码委员会路径与已迁移 event 路径引用,则一并删;`ProviderId` 若仍被 `CommitteeConfig.role_providers` 引用则保留该 enum。逐个 grep 确认。

- [ ] **Step 10: 删除 orchestrator 死代码**

`src-tauri/src/invest/committee/orchestrator.rs`:删除 `#[allow(dead_code)]` 的 `build_llm_config`(`:663`)、`llm_call_with_retry`(`:694`)、`retry_on_fallback`(`:1210`)、`run_with_tool_loop`(`:1258`)及顶部相关 import(`:11` 的 `InvestLlmClient`/`Message`/`ToolDef`/`collect_stream`/`CollectedResponse`)。

- [ ] **Step 11: 删除 get_llm_config/save_llm_config/InvestLlmConfig/run_committee**

- `commands/invest.rs`:删 `get_llm_config`/`save_llm_config`(`:661-770`)、`InvestLlmConfig`/`InvestLlmProviderConfig` 结构(`:601-625`)、`default_llm_config`(`:632`)、`llm_config_path`(`:627`);保留 `try_parse_provider_id`(Step8 用)。删非流式 `run_committee`(`:858`)。
- `lib.rs`:删注册行 440(`get_llm_config`)、441(`save_llm_config`)、442(`run_committee`)。

- [ ] **Step 12: 删除数据源检测的 LLM Config 行**

`commands/invest.rs::get_datasource_health`:删除 LLM Config 检测块(`:1442-1497`)及 `llm_config_path` 引用(Task 3 数据源补全在 Task 5 单独做,这里只删 LLM 行)。

> 注:数据源补全(C3)是 Task 5,放在本 Task 之后,因为本 Task 改了 `get_datasource_health`。

- [ ] **Step 13: 前端删除 ProviderConfigPanel / llmConfig**

- `src/lib/stores/invest-committee-store.svelte.ts`:删 `loadConfig`/`saveConfig`/`llmConfig`(`:495-509`),改为读写 Step8 的新 tuning 结构(若 debate_rounds 仍需 UI)。
- `src/lib/components/invest/ProviderConfigPanel.svelte`:若只剩 debate_rounds + selected_provider,改为读新结构;若新结构已并入别处 UI,则 `git rm` 该组件并移除挂载点。
- **`src/lib/components/invest/CommitteeRolesTab.svelte`:已核实它在 `:5` import、`:311` 挂载 `<ProviderConfigPanel />`。若 `git rm` 了该组件,必须同步删除此处 import 行与渲染标签(否则 svelte-check 立即报错);若 ProviderConfigPanel 改写为读新结构则保留挂载。**
- `src/lib/components/invest/SystemDatasourceTab.svelte`:已核实 `:36` 独立调 `invoke('get_llm_config')`(不经 store)。删 LLM provider 部分(`:13-17, 33-44, 52-57`)及该 `get_llm_config` 调用。

- [ ] **Step 14: 清理 i18n**

从 en.json + zh-CN.json 删除:`invest_committee_llm_config`、`invest_committee_provider`、`invest_committee_provider_default`、`invest_committee_provider_hint`、`invest_committee_debate_rounds`(若 debate_rounds UI 保留则保留此键)、`invest_committee_no_config`、`invest_system_ds_llm_none`。grep 确认无残留引用再删。

- [ ] **Step 15: 全量验证**

Run:
```bash
cargo check  --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run check
npm run i18n:check
```
Expected: 全通过。

- [ ] **Step 16: 运行目测**

`npm run tauri dev`:委员会仍能正常运行(debate_rounds 等调参生效);事件扫描正常;数据源检测不再有 LLM Config 行且不报错。

- [ ] **Step 17: Commit**

```bash
git add -A
git commit -m "refactor(invest): 删除 llm_config.json 全链路(OpenAiCompatClient/get-save_llm_config/死代码),委员会调参迁至独立结构"
```

---

## Task 5: 数据源检测补全(任务6)

**Files:**
- Modify: `src-tauri/src/commands/invest.rs::get_datasource_health`(`:1391`+)
- Modify: i18n(新数据源名若用 key)

**Interfaces:**
- Consumes: `tencent_quotes::fetch_quotes`/`fetch_csi300_kline`、`TushareClient::major_news`、`InternationalClient::fetch_akshare_bond_yield`/`fetch_akshare_market_stats`/`fetch_yahoo_history`、`crate::python::require()`。
- Produces: `get_datasource_health` 返回更完整的 `DataSourceStatus[]`。

**说明:** 在 Task 4 删除 LLM Config 行之后做。LLM provider 不再单独检测(已走 CLI,用户确认)。

- [ ] **Step 1: 补 Python 运行时检测**

在 `get_datasource_health` 增加一项:调用 `crate::python::require()`(或等价的 Python 就绪检查),ok 则 sample="ready",err 则暴露错误。放在 international 探测之前,使其成为 yfinance/AkShare/Jin10 的根因指示。

```rust
    // Python runtime — root dependency for yfinance / AkShare / Jin10
    match crate::python::require() {
        Ok(_) => sources.push(DataSourceStatus {
            name: "Python 运行时".into(), ok: true,
            last_success: Some(now_str.clone()), sample_value: Some("ready".into()),
        }),
        Err(e) => sources.push(DataSourceStatus {
            name: "Python 运行时".into(), ok: false,
            last_success: None, sample_value: Some(format!("{e}")),
        }),
    }
```

> 实现时确认 `crate::python` 的就绪检查 API 名(grep `pub fn require` / `python_status`)。

- [ ] **Step 2: 补腾讯实时 + CSI300 K线**

增加两项,调 `crate::tencent_quotes::fetch_quotes(&["sh000001"])`(或现有签名)与 `fetch_csi300_kline(...)`,各探一次,ok 则取样本值。先 Read `tencent_quotes.rs:105, 214` 确认签名。

- [ ] **Step 3: 补 Tushare 新闻 + AkShare 债券/涨跌停 + Yahoo history**

在已有 Tushare 行情探测后,增加:
- Tushare 新闻:`client.major_news(...)`(确认签名,探一次)。
- AkShare 债券:`intl_client.fetch_akshare_bond_yield()`(复用 `probe_news` 或新探针)。
- AkShare 市场统计:`intl_client.fetch_akshare_market_stats()`。
- Yahoo history:`intl_client.fetch_yahoo_history("^VIX", ...)`(区别于已有的 quote 探测)。

每项构造一个 `DataSourceStatus`,失败暴露错误信息。

- [ ] **Step 4: invest.db schema 校验增强(可选小改)**

把现有 `invest.db` 检测从"仅 with_conn 打开"增强为执行一条轻量 `SELECT 1 FROM holdings LIMIT 1`(或检查关键表存在),sample 注明 schema ok。

- [ ] **Step 5: 验证 + 目测**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
`npm run tauri dev` → /invest → 系统 → 数据源检测,确认新增源全部列出,各自状态正确(Python 关掉时相关源应同时红,且 Python 行明确指出根因)。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/invest.rs messages/en.json messages/zh-CN.json
git commit -m "feat(invest): 数据源检测补全 — 腾讯/Tushare新闻/AkShare/Yahoo history/Python运行时"
```

---

## Task 6: 数据库 / 文件系统清理(任务2)

**Files:**
- Delete: `src-tauri/src/storage/invest/round_cache.rs`(死代码模块)
- Modify: `src-tauri/src/storage/invest/mod.rs:7`(移除 `#[allow(unused)]` round_cache mod 声明)
- Create: `src-tauri/src/commands/invest_cleanup.rs`(一次性清理命令)
- Modify: `src-tauri/src/lib.rs`(注册清理命令)
- Modify: `src/routes/invest/+page.svelte` 或系统 tab(加"数据清理"入口)

**Interfaces:**
- Consumes: `crate::storage::invest::with_conn`。
- Produces: `invest_cleanup_scan() -> CleanupReport`(只读核对)、`invest_cleanup_apply(targets) -> CleanupResult`(确认后删除)。

- [ ] **Step 1: 删除 round_cache 死代码**

```bash
git rm src-tauri/src/storage/invest/round_cache.rs
```
在 `src-tauri/src/storage/invest/mod.rs:7` 删除 `#[allow(unused)] mod round_cache;`(或相应声明行)。`cargo check` 确认无引用残留。

- [ ] **Step 2: 删除 legacy rooms 目录(运行时,非代码)**

这是用户数据目录清理,做成清理命令的一部分而非代码删除。在清理命令中检测 `~/.claw-go/rooms/` 是否存在并报告,apply 时删除。**不在编译期处理。**

- [ ] **Step 3: 写清理扫描命令(只读)**

新建 `src-tauri/src/commands/invest_cleanup.rs`,`invest_cleanup_scan` 返回各清理目标的行数/大小报告(不删):

```rust
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupReport {
    pub daily_reports_rows: i64,
    pub event_sources_rows: i64,
    pub domain_insights_rows: i64,
    pub domain_insights_empty_rows: i64,   // 空转 dreaming 记录(空内容/无关联)
    pub rooms_dir_exists: bool,
}

#[tauri::command]
pub async fn invest_cleanup_scan() -> Result<CleanupReport, String> {
    let (dr, es, di, di_empty) = crate::storage::invest::with_conn(|conn| {
        let dr: i64 = conn.query_row("SELECT COUNT(*) FROM daily_reports", [], |r| r.get(0)).unwrap_or(0);
        let es: i64 = conn.query_row("SELECT COUNT(*) FROM event_sources", [], |r| r.get(0)).unwrap_or(0);
        let di: i64 = conn.query_row("SELECT COUNT(*) FROM domain_insights", [], |r| r.get(0)).unwrap_or(0);
        // 空转 dreaming 记录的特征:空 content 或 content 长度过短(实现时按实际数据调整)
        let di_empty: i64 = conn.query_row(
            "SELECT COUNT(*) FROM domain_insights WHERE content IS NULL OR TRIM(content) = '' OR LENGTH(TRIM(content)) < 10",
            [], |r| r.get(0)).unwrap_or(0);
        Ok((dr, es, di, di_empty))
    })?;
    let rooms = crate::storage::data_dir().join("rooms");
    Ok(CleanupReport {
        daily_reports_rows: dr, event_sources_rows: es,
        domain_insights_rows: di, domain_insights_empty_rows: di_empty,
        rooms_dir_exists: rooms.exists(),
    })
}
```

> 实现时:先 Read `storage/invest/mod.rs` 的 domain_insights schema 确认列名;空转记录的判定特征(SQL WHERE)用 scan 先跑一次看实际命中,再定稿——spec 明确"实现时先查询确认特征再删"。

- [ ] **Step 4: 写清理应用命令(确认后删除)**

`invest_cleanup_apply(targets: CleanupTargets)`,按前端勾选删除对应目标。每类删除前 log 实际行数:

```rust
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupTargets {
    pub daily_reports: bool,
    pub event_sources: bool,
    pub domain_insights_empty: bool,
    pub rooms_dir: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResult { pub deleted: Vec<String> }

#[tauri::command]
pub async fn invest_cleanup_apply(targets: CleanupTargets) -> Result<CleanupResult, String> {
    let mut deleted = Vec::new();
    crate::storage::invest::with_conn(|conn| {
        if targets.daily_reports {
            let n = conn.execute("DELETE FROM daily_reports", [])?;
            deleted.push(format!("daily_reports: {n} rows"));
        }
        if targets.event_sources {
            let n = conn.execute("DELETE FROM event_sources", [])?;
            deleted.push(format!("event_sources: {n} rows"));
        }
        if targets.domain_insights_empty {
            let n = conn.execute(
                "DELETE FROM domain_insights WHERE content IS NULL OR TRIM(content) = '' OR LENGTH(TRIM(content)) < 10", [])?;
            deleted.push(format!("domain_insights(empty): {n} rows"));
        }
        Ok(())
    })?;
    if targets.rooms_dir {
        let rooms = crate::storage::data_dir().join("rooms");
        if rooms.exists() {
            std::fs::remove_dir_all(&rooms).map_err(|e| format!("remove rooms: {e}"))?;
            deleted.push("rooms/ dir".into());
        }
    }
    Ok(CleanupResult { deleted })
}
```

注:孤儿 IPC 写入的脏 verdicts/pnl_snapshots(spec C5)判定标准不确定,**不纳入自动删除**;若 scan 阶段发现明显脏数据,实现时单独评估,本计划不预设删除 SQL。

- [ ] **Step 5: 注册命令**

`src-tauri/src/lib.rs`:加 `commands::invest_cleanup::invest_cleanup_scan,` 与 `invest_cleanup_apply,`;在 commands 模块声明处加 `pub mod invest_cleanup;`(确认 `commands/mod.rs` 或 lib.rs 的 mod 声明位置)。

- [ ] **Step 6: 前端清理入口**

在 `/invest` 系统 tab 加一个"数据清理"区:挂载时调 `invest_cleanup_scan` 显示各项行数;每项一个勾选框;"执行清理"按钮弹二次确认后调 `invest_cleanup_apply`,展示 `deleted` 结果。文案 key:`invest_cleanup_title`、`invest_cleanup_scan`、`invest_cleanup_apply`、`invest_cleanup_confirm`、各目标标签。

- [ ] **Step 7: 验证**

Run:
```bash
cargo check --manifest-path src-tauri/Cargo.toml
npm run check && npm run i18n:check
```
Expected: 通过。

- [ ] **Step 8: 运行验证(谨慎)**

`npm run tauri dev` → 系统 → 数据清理:
1. 先只看 scan 报告,确认各行数合理(尤其 domain_insights_empty 命中数符合"几万行空转"预期)。
2. 若空转判定 SQL 命中数与预期差异大,调整 Step3/4 的 WHERE 条件重测。
3. 勾选确认后执行,核对 deleted 结果;再次 scan 确认归零。

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat(invest): 数据清理命令(死代码/legacy目录/只写不读表/空转dreaming记录,扫描+确认后删除)"
```

---

## Task 7: 批次 C 收尾验证

- [ ] **Step 1: 全量验证**

Run: `npm run verify`(lint + fmt + i18n + tests + build + Rust checks)
Expected: 通过。逐一修正失败项。

- [ ] **Step 2: 回归目测**

委员会运行正常、事件扫描正常、数据源检测完整、两个新接线入口可用、清理命令工作、被删 tab 确实消失。

- [ ] **Step 3: 补提交(若有)**

```bash
git add -A
git commit -m "chore(invest): 批次 C 收尾修正"
```

---

## Self-Review 记录

- **Spec 覆盖:** 任务5(删两 tab)→ Task1;任务1 删除孤儿 → Task3 + Task4(llm/run_committee)、接线 → Task2;任务6 数据源 → Task5、llm_config 迁移删除 → Task4;任务2 DB 清理 → Task6。全覆盖。
- **顺序正确性:** Task4 严格"先迁移(Step1-7)后删除(Step8-17)";数据源补全 Task5 在 Task4 改完 `get_datasource_health` 之后;DB 删除 Task6 走 scan→确认→apply。
- **类型一致性:** `cli_complete(system, user)` 在 event_scanner/event_analyzer 共用;`CleanupReport`/`CleanupTargets`/`CleanupResult` 在 scan/apply/前端一致;孤儿命令删除按 lib.rs 已核对行号(404-476)。
- **占位符扫描:** 代码步骤含完整代码;标注的"实现时确认"点(cli settings_path、python::require API 名、tencent/tushare 签名、domain_insights 列名与空转判定 SQL、build_committee_config 字段、commands mod 声明位置)均为需现场 Read 核对的集成点,非占位符——已给出确认方法。
- **已知风险:** Task4 是最高风险项,独立 commit 迁移与删除两段便于回滚;Task6 删行不可逆,强制 scan→确认→apply 三段式。
