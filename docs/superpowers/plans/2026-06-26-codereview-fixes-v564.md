---
status: wip
date: 2026-06-26
owner: Smilucifer
related:
  - src/lib/components/SessionStatusBar.svelte
  - src-tauri/src/group_chat/orchestrator.rs
  - src-tauri/src/group_chat/memory_extraction.rs
  - src-tauri/src/storage/settings.rs
  - src-tauri/src/invest/committee/cli_executor.rs
  - src-tauri/src/invest/macro_refresh.rs
---

# 全项目 Code Review 修复计划（v5.6.4+）

## 背景

v5.6.3 发布后出现"无法进入聊天/私聊页"的严重回归。根因定位与修复后，对全项目做了一轮并行 code review（4 个 subagent 按 commands+storage / agent+group_chat / 前端 / invest 切分，全部对照源码核验，`cargo check` 与 `svelte-check` 均通过）。

本文档记录：
1. 已修复的进不去 bug（v5.6.4 待发）。
2. review 发现的全部问题，按用户决策标注修复范围。

用户决策（2026-06-26）：
- **C1 修**、**C3 修**、**C4 修**、**C5 修**、**C6 修**。
- **C2 重新定义**：自动记忆提取是此前讨论后实现的 feature，不是 bug。真正要做的是 (a) 提供独立开关，可关闭提取；(b) 设置页加"保存配置"按钮；(c) 对提取出的记忆做质量筛选，减少无价值记忆。
- 全部 High / Medium 后续都要修，先入档排期。

---

## 已修复（v5.6.4 待发）

### FIXED-0 — 聊天/私聊页进不去（Critical 回归）

- **根因**：提交 `5c62f04` 把 `<ClaudeUsageBadge />` 的渲染从 `chat/+page.svelte` 移到 `SessionStatusBar.svelte`，删除了前者 import 却没在后者补上。官方 Claude 订阅用户进聊天页时 `showClaudeUsage=true`，渲染状态栏即抛 `Cannot find name 'ClaudeUsageBadge'`，整页崩溃。
- **修复**：
  - `SessionStatusBar.svelte` 补 `import ClaudeUsageBadge from "$lib/components/ClaudeUsageBadge.svelte"`。
  - `ClaudeUsageBadge.svelte` 给同提交新增的三个 snippet 参数补类型（`ring(u: number | null)`、`windowRow(icon: Snippet, label: string, w: UsageWindow | null)`），消除 implicit-any 类型错误。
- **验证**：`svelte-check` 目标文件 0 错误；`npm run build` 通过。

### FIXED-C1 — `/dm` 私信广播（2026-06-27 修复）

- `orchestrator.rs:strip_command_word` 不消费分隔空白，`rest.strip_prefix('@')` 永远 None。改为 `rest.trim_start().strip_prefix('@')`。已有单测 `parse_group_chat_command("/dm @Alice ...")` → Private 现可通过（此前因本机 VCRUNTIME 没跑出失败）。

### FIXED-C5 — 私聊内容进全局记忆（2026-06-27 修复）

- `orchestrator.rs` 自动抽取 spawn 前加 `if !is_private` 守卫，私聊 turn 完全跳过 `auto_extract_memories`，DM 文本不再外送 LLM/写入共享记忆。

### FIXED-C3 — 启动清空凭证（2026-06-27 修复）

- `storage/settings.rs::load()` 读/解析失败时**不再** save 默认值（改为返回内存默认 + `log::error`，保留原文件），只有文件不存在（首次启动）才写默认。
- `save()` 改 tmp+rename 原子写（参照 `runs.rs::save_meta`，含 PermissionDenied 3 次重试 + unix 0o600）。

### FIXED-C4 — 路径遍历（2026-06-27 修复）

- 新增 `storage::validate_run_id`（非空/≤128/排除 `.`/`..`/仅允许 `[A-Za-z0-9_-]`），在 `get_run_raw`、`with_meta`、`save_meta` 三个路径根入口校验，覆盖 get_run/rename/update_model/update_status/soft_delete 全部 id 拼路径入口。
- `group_chats.rs::detach_group_chat_run` 删 participant meta 前校验 `run_id`，删除错误不再 `let _` 静默吞（NotFound 忽略，其余 log::warn）。

### FIXED-C6 — committee 取消无法中断 CLI（2026-06-27 修复）

- `cli_executor::run_role` 增 `cancel: Option<&CancellationToken>` 参数，用 `tokio::select!` 在 `wait_with_output` / `cancel.cancelled()` / timeout 间竞争；取消时提前返回，`kill_on_drop` 即时回收子进程并释放 semaphore permit（不再等满 180s）。进入前若已取消直接返回。
- `run_role_phase` / `run_macro_phase` 透传 cancel；4 个调用点（macro/quant/risk R1+R2/cio/event_analyzer）全部更新（event_analyzer 传 None）。
- H2：取消检查下沉到 `for role in roles` 内层。
- H1：`commands/invest.rs` token registry 注册时检测同 symbol 冲突，已 in-flight 则跳过（不覆盖 token），清理只删本批注册项。
- **验证**：`cargo check` 通过；`cargo clippy` 我改动文件 0 新增 error（基线 55 个预存 error 未变）；`svelte-check` 仅剩 3 个预存 CodeEditor 错误；`npm run build` / `i18n:check`（0 error）通过。

### FIXED-C2 — 记忆提取改进（2026-06-27 实现）

- **独立开关**：`UserSettings` 新增 `memory_extraction_enabled: bool`（默认 true）。`can_extract` 最前面判断此开关，两个入口（group_chat orchestrator + agent/session_actor turn-idle）都过 `can_extract`，故总闸生效，且不再搭在 `embedding_config.enabled` 上。
- **质量筛选**：新增 `memory_extraction_min_confidence: u8`（默认 60）。`auto_extract_memories` 入库前过滤：confidence < 阈值丢弃、content < 6 字符丢弃、纯标点（无 alphanumeric/CJK）丢弃。prompt 收紧为"只提取稳定可长期复用的用户事实，忽略一次性闲聊/临时信息/他人信息，宁缺毋滥"。
- **设置页 UI + 保存按钮**：`memory-mgmt` 页 Memory Extraction 区加"自动提取总开关"+"置信度阈值滑块"+"保存配置"按钮（显式一次性保存主开关/阈值/LLM 配置，带保存中/已保存状态），独立于原有的 onblur debounce LLM 配置。
- 前端 `types.ts` UserSettings 补 `memory_extraction_enabled` / `memory_extraction_min_confidence`；`settings.rs::update_user_settings` 补两字段 patch 映射。
- **验证**：`cargo check`、`svelte-check`（memory-mgmt 0 新错误）、`npm run build`、`i18n:check`（0 error）通过。

---

## Critical（已确认要修）

### C1 — `/dm` 私信被当公开广播（隐私泄漏）

- **位置**：`src-tauri/src/group_chat/orchestrator.rs:955-964`，`strip_command_word` 在 `:1004-1011`。
- **根因**：`strip_command_word` 只校验命令词后是空白但不消费它，返回的 `rest` 仍带前导空格，于是 `rest.strip_prefix('@')` 永远返回 `None`，`/dm @Alice msg` 降级为 `Fanout` 公开广播。
- **修复方案**：改为 `rest.trim_start().strip_prefix('@')`（或在 `strip_command_word` 内消费掉分隔空白）。
- **失败用例**：模块内已有内联单测，但本机 Rust 测试因 VCRUNTIME 问题不跑（CLAUDE.md §11），需在 CI/干净环境验证，或加一个纯解析单测确保 `/dm @Name x` 被解析为 SingleTarget-private。

### C2 — 记忆提取改进（feature 调整，非 bug）

现状核实：
- **没有独立开关**。提取总闸搭在 `embedding_config.enabled` 上（`memory_extraction.rs:99-104`），用户无法单独关闭提取；`settings.rs` 只有 `memory_dream_enabled` 一个布尔，没有提取开关字段。
- **无质量门槛**。`memory_extraction.rs:295-346` 拿到 LLM 给的 `confidence(0-100)` 后只做 FTS5 去重（`find_duplicates 0.8`）就入库，低分照样进——这是"很多记忆没价值"的根因。

改进任务：
1. **独立开关字段**：在 `UserSettings` 加 `memory_extraction_enabled: bool`（默认沿用现有行为），提取入口（`agent/session_actor.rs` turn-idle、`group_chat/orchestrator.rs`）先判断此开关再 spawn。不要继续复用 `embedding_config.enabled`。
2. **设置页 UI + 保存按钮**：在 settings 记忆相关区块加开关与"保存配置"按钮，走 `update_user_settings`。
3. **质量筛选**：
   - 入库前加 confidence 阈值（建议可配置，默认如 60）。
   - 过滤过短/空泛内容（长度下限、纯标点/停用词）。
   - 收紧 prompt：明确"只提取稳定、可复用的用户事实，忽略一次性闲聊"。
   - 可选：低分记忆进 `pending` 由用户在 memory-mgmt 页确认后才转 active。
- 关联：C5（私聊不应进全局记忆）与本项同属记忆系统，建议一并改。

### C3 — 启动可能清空所有凭证

- **位置**：`src-tauri/src/storage/settings.rs:36-39`（`load()` 失败回落 `save(default)`）、`:439`（`settings.json` 裸 `fs::write` 非原子）。
- **根因**：读/解析失败时把含全部凭证（api_key / anthropic_api_key / tushare_token / MCP / hooks）的 settings 静默覆盖为默认空值；叠加非原子写，掉电/半截写入即触发。
- **修复方案**：
  - `load()` 解析失败时**不要**自动 save 默认值；返回错误或保留原文件，记录日志并提示用户。
  - `settings.json` 改 tmp+rename 原子写（参照 `storage/runs.rs:199-205` / `group_chats.rs`）。
  - 可选：写前做一次 `.bak` 备份。

### C4 — 路径遍历（多个 command 未校验 run_id / id）

- **位置**：`commands/runs.rs:47/164/170/176/200`（`get_run`/`rename_run`/`soft_delete_runs`/`update_run_model`/`stop_run`）；`storage/group_chats.rs:232`（detach 删任意 `*.meta.json`，错误被 `let _` 吞）。
- **根因**：前端传入的 `run_id` 未做格式校验就 `join` 进文件系统路径，`..\..\` 可逃逸数据目录。
- **修复方案**：
  - 加统一 `validate_run_id`（仅允许 UUID/受限字符集），所有以 id 拼路径的入口先校验。
  - detach 的删除错误不要静默吞，至少 log。
  - 与已存在的 participant_id/room_id UUID 校验保持一致风格。

### C5 — 私聊内容被抽进全局记忆

- **位置**：`src-tauri/src/group_chat/orchestrator.rs:514-537`（无条件 spawn `auto_extract_memories`）。
- **根因**：抽取不判断 `mode == Private` / `is_private`，DM 文本被送外部 LLM 并写入共享 user memory。
- **修复方案**：spawn 前判断私聊标志，私聊 turn 跳过抽取。与 C2 同属记忆系统，一并改。

### C6 — committee 取消无法中断在跑的 CLI 子进程

- **位置**：`src-tauri/src/invest/committee/cli_executor.rs:72-142`；并发覆盖见 `commands/invest.rs:587-619`；轮内检查见 `committee/orchestrator.rs:1352-1395`。
- **根因**：`run_role` 不接受 `CancellationToken`，`timeout(...).wait_with_output()` 内部 hold 住 `Child`，外部 cancel 不会 drop 该 future；`kill_on_drop` 只在 future 真正 drop 时生效（默认 180s）。被取消的角色仍占用 `Semaphore(5)` permit，可饿死整池。`commands/invest.rs:595` 还会用同 symbol 的新 token 覆盖 in-flight token，导致两次运行都不可取消。
- **修复方案**：
  - `run_role` 接收 `CancellationToken`，用 `tokio::select!` 在 `child.wait_with_output()` 与 `cancel.cancelled()` 间竞争，取消时显式 `child.kill().await`。
  - 取消检查下沉到 `for role in roles` 内层（H2）。
  - registry 用同 symbol 已存在 token 时拒绝/合并，避免覆盖（H1）。

---

## C7 — 交易记录日期显示不一致（yyyy/mm/dd vs yyyy-mm-dd）

用户报告：交易记录里 Sell 类型条目日期显示为 `2026/6/26`（斜杠），Buy 正常显示 `2026-06-24`。要求统一为 `YYYY-MM-DD`，含已有数据。

**根因调查（关键：数据库里没有斜杠）**：
- 用 Python 穷尽扫描 `C:\Users\InBlu\.claw-go\invest\invest.db` 全部 25 张表的每个 TEXT 列（`LIKE '%/%'` + 日期正则，不锚定位置），**零斜杠命中**。日期列全是 `2026-06-25`，紧凑列是 `20260603`，时间戳列混用 `...Z` / 空格 / `+00:00` 但都无斜杠。
- 斜杠来自**前端显示兜底**：`TradeLogTab.svelte:79` `return tr.tradeDate ?? new Date(tr.createdAt).toLocaleDateString();` —— 中文 locale 下 `toLocaleDateString()` 产出 `2026/6/26`。
- 为什么只有 Sell：数据层 `trade_date` 为 NULL 才走兜底。`trades` 表统计：`sell` 38 条 37 条 NULL、`add_watch`/`delete_watch`/`edit_holding`/部分 `cash_adjust` 全 NULL，而 `buy` 60 条全部有值。源头是 `invest-store.svelte.ts` 的 `sellStock:425` 等处硬编码 `tradeDate: null`。
- 后端已有正确兜底 `Trade::effective_date()`（`portfolio.rs:195-201`，空则取 `created_at[..10]` → `YYYY-MM-DD`），但只用于 PnL 计算，未用于显示；store 内部计算（`invest-store.svelte.ts:186`）也已用 `tr.tradeDate ?? tr.createdAt?.slice(0,10)` 的正确兜底——只有 `TradeLogTab:79` 用了 locale 斜杠。

**修复方案（三层，用户确认全做）**：
1. **显示层（根治斜杠）**：`TradeLogTab.svelte:79` 兜底从 `new Date(createdAt).toLocaleDateString()` 改为取 `createdAt.slice(0,10)`（与后端 `effective_date` / store:186 一致，产出 `YYYY-MM-DD`）。
2. **数据回填**：对 `trade_date` 为 NULL 的历史行，用 `date(created_at,'localtime')` 回填为 `YYYY-MM-DD` 写回 DB。作为一次性迁移放在 invest schema 迁移流程里（`storage/invest/mod.rs`），幂等（`WHERE trade_date IS NULL OR trade_date=''`）。
3. **新写入也填**：`record_trade`（`commands/invest.rs:48` / `portfolio.rs:432`）在 `trade_date` 为空时用 `created_at[..10]` 落库，避免未来再产生 NULL。前端 `sellStock` 等不必再硬传 null。

**验证**：迁移后重扫 DB 确认无 NULL trade_date；`cargo check`、`npm run build`、`svelte-check` 通过。

---

## High（后续修，已入档）

- ✅ **H-sec-1** `commands/session.rs:2075/2247/2278` — `extra_env` 直接注入子进程，`LD_PRELOAD`/`NODE_OPTIONS`/`PATH`/`PYTHONPATH` 绕过 `ALLOWED_EXTRA_ENV_KEYS` 白名单（白名单只作用于 JSON 设置层）。**已修**：新增 `is_dangerous_spawn_env_key`（denylist：LD_PRELOAD/LD_*/DYLD_*/NODE_OPTIONS/PYTHONPATH/PYTHONSTARTUP/BASH_ENV/ENV），在主 spawn 直接注入点 + `merge_extra_env_into_spawn_env_plan`（覆盖 BTW/claude_stream）统一丢弃。白名单不适用是因 extra_env 携带合法 AUTH_TOKEN/BASE_URL，故用 denylist。
- ✅ **H-sec-2** `commands/cli_config.rs:18` 与 `agent/provider_claude_config.rs:430-432` — API key / token 被写进 debug 日志（`commands/settings.rs:45` 已正确省略请求体，可参照）。**已修**：`update_cli_config` 改为只 log 键名集合；`merge_extra_env` 日志去掉 value。
- ✅ **H-sec-3** `commands/clipboard.rs:31-56,312-316` — `read_clipboard_file` 只校验扩展名（含 env/cfg/ini/conf/toml/sql），webview 可直接读 `.env` 凭证。**已修**：白名单移除 env/cfg/ini/conf；`validate_clipboard_path` 拒绝 dotfile（`.` 开头，挡 .env/.npmrc/.git-credentials）。
- **H-sec-4** `commands/diagnostics.rs:404/426/480` — SSH `user@host` 无 `--` 终止符、不拒绝 `-` 前缀，argv-flag 注入。
- ✅ **H-sec-5** provider 临时配置 `session-{run_id}.json` / `-mcp.json`（含 `ANTHROPIC_AUTH_TOKEN`）永不清理，每个 run 累积；仅 committee 的 `cleanup_old_committee_settings` 清理自己那批。**已修**：新增 `cleanup_provider_session_configs(run_id)`，在 `SessionActor::cleanup` 调用，删除两个 temp 文件。
- ✅ **H-sec-6** `agent/provider_claude_config.rs:621-637` — 敏感 key strip 只删顶层 `apiKey`/`primaryApiKey`，原生 `env.ANTHROPIC_API_KEY` 会与注入的 `ANTHROPIC_AUTH_TOKEN` 同时出现在写出的 settings。**已修**：新增 `SENSITIVE_ENV_KEYS`（ANTHROPIC_API_KEY/AUTH_TOKEN/BASE_URL/OPENAI_*/CLAUDE_CODE_API_KEY），构造 env_obj 后、overlay provider 值前先剥离。
- **H-state-1** `agent/session_actor.rs:1684-1694` — `SessionInit` 持久化 session_id/`conversation_ref` 失败仅 warn，重启后 `claude --resume` 无 session id 无法恢复。
- **H-state-2** `agent/session_actor.rs:2287-2294` — 终态 `update_status` 写失败仅 log，盘上 meta 仍为 `Running`，history/UI 状态不一致。
- **H-state-3** `agent/session_actor.rs:1273-1286` — `pending_interrupt` 在 `write_json_line().await?` 之前置位，写失败提前返回但 flag 已置，后续真实 `failed` 被改写成 `idle`，遮蔽 CLI 失败。
- **H-utf8** `agent/stream.rs:209-214` — 旧 pipe 路径 8KiB 固定 buf + `from_utf8_lossy`，多字节 UTF-8 跨 chunk 被替换成 U+FFFD；改 `read_line`/增量解码。`executor/codex.rs:62-64` 把 stdout `Err` 当 EOF。
- **H-gc-1** `storage/group_chats.rs:232-233` — detach 用 `{run_id}.meta.json`，但 `save_participant_meta` 存的是 `{participant_id}.meta.json`（不同 UUID）；每次 detach 留孤儿元数据，重 attach 读到陈旧 cursor。
- **H-gc-2** `group_chat/orchestrator.rs:541-558` + `storage/group_chats.rs:300-307` — 失败 turn 复用 turn_id 追加 `responses: vec![]`，`list_turns_jsonl` 按 id 保留最后一行，覆盖已落盘的部分响应（显示为零响应）。
- ✅ **H-fin-1** `invest/verdict_review.rs:123-146` — `calc_atr_pct` 因 bars 是 newest-first 取错"前一日收盘"（`bars[i-1]` 实为更新的一天），TR 两项算错，污染命中率统计。应 `bars[i+1]` 或先正序排。**已修**：改用 `bars[i].pre_close`（Tushare 直接提供前收盘，与排序无关）。
- ✅ **H-fin-2** `invest/committee/parser.rs:541-557` — CIO VERDICT 用 substring `contains("BUY")`，`merge_continuation_lines` 把 prose 拼进值，`不要买入`/`HOLD WILL BUY` 被识别成 BUY，方向反转。**已修**：新增 `normalize_verdict`，只取首个子句（按标点/换行切断 prose 续行）+ 否定守卫（`不买`/`DO NOT BUY` → HOLD）+ ACCUMULATE/SELL/TRIM 先于 BUY 判定。
- ✅ **H-fin-3** `invest/committee/parser.rs:558` — confidence 无范围归一，`"75%"` 裸切成 `75.0`，`apply_hard_rules`(`analysis.rs:305`) 用 `>= 0.95` 判定被破坏，`archive.rs:193` 渲染 `7500%`。**已修**：新增 `normalize_confidence`（>1 视百分比 ÷100，clamp [0,1]）。
- ✅ **H-fin-4** `parser.rs:479/497/524` — strength 1-10 量纲未校验（`0.7`/`7/10`/`70` 混用），`analysis.rs` 的 `>=6/>=7` 比较失真。**已修**：新增 `normalize_strength`（≤1 ×10、>10 ÷10、clamp [0,10]），Macro/Quant/Risk/R2 四处统一调用。
- ✅ **H-fin-5** `parser.rs:468-477` — macro SIGNAL 非白名单值（`bullish`/`看多`/`谨慎乐观`）被静默改写成 `neutral`，真正 risk_off 被吞，Gate 判定失效。**已修**：新增 `normalize_macro_signal`，扩展 bullish/bearish/看多/看空/避险 等映射；未识别值保留原文 + warn，不再吞成 neutral。
- ✅ **H-data-1** `invest/data_source/validity.rs:6` — `is_valid_number` 把合法 `0`（平衡日常态）判为无效，触发不必要 fallback。**已修**：新增宽松判空 `is_present_finite`（仅 None/NaN/Inf 无效，0 合法），北向资金调用点改用之；`is_valid_number`（0 即异常）保留给收益率/价格等字段。
- **H-data-2** `invest/macro_refresh.rs:104-153` — 复合返回值 `is_valid` 只查 `close`，MiniQmt 有 `close` 但 `vol20=None` 时视为成功，Tushare 永不再试，vol20 行与 close 行 age/source 漂移。
- **H-data-3** `invest/macro_refresh.rs:221-276` — `margin`/`shibor` 取最新行不校验，字段为 0 也写入，source 硬编码 `tushare`。
- **H-fe-race** 四个单例 store 在 await 后无序号/abort 守卫，快速切换用旧响应覆盖新的：`character-memory-store.svelte.ts:12-21`、`doctor-store.svelte.ts:13-29`、`user-memory-store.svelte.ts:11-19`、`claude-usage-store.svelte.ts:10-24`。推广 memo-store 的 `#loadSeq` 模式。

---

## Medium（后续修，已入档）

### 原子写 / 并发锁缺失
- `storage/mcp_registry.rs:1159`（重写 `~/.claude.json`，与活跃 CLI 写竞争）、`:656/672/690`（`~/.codex/config.toml`）— 非原子无锁。
- `storage/cli_config.rs:72-107` — `~/.claude/settings.json` load→mutate→write 非原子无锁，且与运行中 CLI 共享。
- `agent/provider_claude_config.rs:231/279/310` — 合并 settings（含 token）非原子写，且未设 `0o600`（对比 `storage/runs.rs:182-187`）。
- `storage/artifacts.rs:32`、`storage/plugins.rs:681/718`（SKILL.md）— 非原子写。
- load→mutate→save 无锁丢改动：`storage/favorites.rs:68-144`、`storage/settings.rs:544-583`（`update_user_settings`）、`:904-918`（`update_agent_settings`）。

### panic / 鲁棒性
- `commands/diagnostics.rs:262`（`&s[..50]`）、`:763`（`&k[k.len()-4..]`）— 按字节切 UTF-8 可能 panic。
- `storage/run_index.rs:410/420/424/552/609`、`storage/events.rs:178/191/234/244/293` — `lock().unwrap()` 中毒后整模块后续操作全 panic。
- `storage/teams.rs:8` — `expect("home dir")` 在 HOME/USERPROFILE 不可解析时 panic 命令线程。
- `storage/runs.rs:339-340/485-486` — `list_runs` 静默丢解析失败的 run，损坏 1 条 meta 即在历史消失。
- `invest/committee/orchestrator.rs:1757/1810` — spawn 任务里 `expect("semaphore closed")`，未来重构可致 worker panic、`JoinError` 被吞、前端收不到 `SymbolAborted`。

### Windows / 功能性
- `commands/agents.rs:157` — Windows 上 `target` 未 canonicalize 而 `parent_to_check` 已 canonicalize（`\\?\` 前缀），前缀比较恒不匹配，**project-scope agent CRUD 在 Windows 整体失效**。

### invest 解析 / 计算
- `invest/indicators.rs:108-111` — 价格分位用首个等值索引，平窗塌缩到 0%、封顶 (n-1)/n；改 tie-rank。
- `invest/committee/parser.rs:289-297`（continuation 合并污染 KEY 值）、`:525`（pnl_pct 未强制符号）、`:458`（KEY_DATA 遇任意冒号即终止丢字段）、`:371-380`（`parse_leading_f64` 不识 `,`/`，`/`。`）。
- `storage/invest/macro_cache.rs:184-195` — `is_stale` 纯墙钟，周末把最新交易日数据误判为 stale。
- `invest/scheduler/runner.rs:268-285/334-340` — 热路径在 runtime worker 上做同步 SQLite/FS I/O，应 `spawn_blocking`。

### group_chat 其它
- `orchestrator.rs:966-997/1424-1447` — `@mention` 用 `trim_start_matches('@')`，带尾标点（`@Alice,`/`@Alice.`）查不到 participant，命令降级 Fanout、auto-chain 断链。
- `orchestrator.rs:1429-1431` — `extract_first_mention` 在循环内用 `?` 碰到 `preview=None` 提前返回，应 `continue`。
- 只增不减的全局 HashMap：`orchestrator.rs:38-41`、`storage/group_chats.rs:14`、`memory_extraction.rs:39/44`（`DAILY_EXTRACTION_COUNT` 还会把昨天配额带给同 id 新 group chat）。
- `group_chat/memory_dream.rs:34-42/127` — `mark_dream_time` 用 `let _ = fs::write`，写失败后 `should_run_dream` 恒 true，每 tick 重跑快照+衰减。
- `agent/session_actor.rs:1283-1286`（孤立 oneshot sender 留在 control_waiters）、`:1291-1305/2184-2192`（`child.wait()` 无超时，Windows kill 失败则 actor 卡死）、`:1647-1664`（记忆抽取 spawn 丢 JoinHandle 不可取消）、`:559-572/697-702`（turn index 写失败前已自增，留空洞）。

---

## 已核对干净（无需改）

balance、chat 附件（`safe_filename`+上游 `get_run`）、`cli_settings.rs`（canonicalize+拒符号链接+tmp/rename）、`files.rs`/`fs.rs` 路径校验、`ssh.rs` 转义、SQLite 串行化（`with_conn_mut`）、group_chat plan/participant 的 tmp+rename、RSI/MA/volatility 公式、`regime.rs` 时序、scheduler panic 隔离（`run_dispatch_with_panic_catch`+`JobGuard::drop`）、queue 文件 I/O、`MILLION_TO_YI`/`amount/1e8` 量纲、`claude_usage.rs` 缓存原子写。

前端：runes 模式全项目覆盖（无 `export let`）、无 `$derived` 内副作用、无 `$effect(async)`、重型订阅 teardown 均平衡、`JSON.parse` 全在 try/catch、修复 CodeEditor 后 `svelte-check` 0 错误。

---

## CodeEditor 已知问题（独立，非本次回归）

`src/lib/components/CodeEditor.svelte:155` 行内注释含字面量 `<style>` token，骗过 svelte-language-tools 分词器，使 `<script>` 提前闭合 → explorer/memory 两页报 "no default export"。runtime 编译器本身能接受。修复：把注释里的 `<style>` 改成 `the style block`（去掉字面 `<style`/`</style`/`<script`/`</script` 子串）。已实验验证：改后 `svelte-check` 错误从 3 → 0。

---

## 修复顺序建议

1. **FIXED-0**（已修）→ 发 v5.6.4。
2. 隐私/数据安全优先：C1、C5、C3、C4。
3. C2 记忆 feature 改进（开关 + 保存按钮 + 质量筛选，与 C5 同批）。
4. C6 + invest 金融正确性（H-fin-*）。
5. 其余 High → Medium 按模块分批。

每项遵循 systematic-debugging：先写失败用例（或纯解析单测，规避本机 VCRUNTIME 测试问题），再单点修复，`cargo check` / `svelte-check` / `npm run build` 验证。
