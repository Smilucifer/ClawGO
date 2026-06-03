# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

This repository is a Windows-first Tauri desktop app with a SvelteKit frontend and a Rust backend. The project is a remaster built on Claw GO's local-first desktop architecture, adding Claude Session Hub concepts such as Rooms, Memo, Roundtable, Driver/Copilot, and Research workflows without disrupting the existing `/chat` path.

The core product model is:
- `Run` is the smallest execution unit.
- `GroupChat` (formerly Room) is an orchestration layer built on top of one or more runs.
- `AiCharacter` is a reusable persona template with role_type, role_instruction, and default provider/model, stored in UserSettings.
- Providers shown in the UI are not always the same as execution agents under the hood.

**Current phase:** Phase 10+ (v5.2.10, 2026-06-03). 委员会直播页面崩溃修复 — watch/hold 持仓去重+Simplify 审查修复。See `docs/changelog.md`.

## Standard workflow

Every development cycle follows this pattern:

1. **Implement** the feature or fix.
2. **Update** the relevant docs in `docs/` with status and completion notes.
3. **Code review** via `simplify` skill — three parallel agents check reuse, quality, and efficiency.
4. **Fix** all review findings.
5. **Commit** with Conventional Commit style (`feat:`, `fix:`, `chore:`).
6. **Verify** with `npm run build`, `npm run i18n:check`, and relevant tests.

## Common commands

### Frontend / app development

```bash
npm install
npx svelte-kit sync
npm run dev
npm run tauri dev
```

- `npm run dev` starts the Vite dev server on port `1420`.
- `npm run tauri dev` runs the desktop app locally.

### Frontend quality checks

```bash
npm run lint
npm run lint:fix
npm run check
npm run test
npm run build
```

### Rust quality checks

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run rust:check
```

### Project-wide verification

```bash
npm run i18n:check
npm run verify
```

`npm run verify` runs the main frontend and Rust validation path: lint, format check, i18n check, tests, build, and Rust checks.

### Packaging

```bash
npm run tauri build
```

Produces:
- `src-tauri/target/release/ClawGO.exe` (main binary)
- `src-tauri/target/release/bundle/nsis/ClawGO_<version>_x64-setup.exe`
- `src-tauri/target/release/bundle/msi/ClawGO_<version>_x64_en-US.msi`

For version bumping across all config files:

```bash
npm run release <version|patch|minor|major>
```

### Running a single test

Frontend Vitest only includes `src/**/*.test.ts`.

```bash
npm test -- src/lib/stores/memo-store.test.ts
npm test -- src/lib/stores/group-chat-store.test.ts
npm test -- src/lib/utils/agent-capabilities.test.ts
```

Rust single-module examples:

```bash
cargo test --manifest-path src-tauri/Cargo.toml storage::memos::tests:: -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml commands::memos::tests:: -- --nocapture
```

If narrowing Rust tests further, use the module path pattern accepted by `cargo test`.

## Key repo structure

- `src/`: SvelteKit frontend (Svelte 5 runes).
- `src/lib/stores/`: stateful frontend stores; the main app behavior is coordinated here.
- `src/lib/components/`: shared UI components — GlobalMemoPanel, ChatMessage, CommandPalette, GroupChatStepper, GroupChatLayout, PlanPanel, modals, and provider/settings panels.
- `src/lib/utils/`: frontend utilities — provider-catalog, format, agent-capabilities, sidebar-groups, and i18n helpers.
- `src/lib/transport/`: transport abstraction between desktop Tauri IPC and browser/WebSocket mode.
- `src/routes/`: route-level UI pages — chat, memory, explorer, plugins, usage, history, settings, settings/characters. (`/memo` redirects to `/chat`; memo is a global pop-out panel. Group chats are accessed from `/chat` with sidebar navigation.)
- `src-tauri/src/commands/`: Tauri IPC command surface consumed by the frontend.
- `src-tauri/src/agent/`: agent launch, session, stream, PTY (including native PTY for Codex), native transcript parsing, and Windows toolchain handling.
- `src-tauri/src/group_chat/`: group chat orchestration, memory system (injection, extraction, graph, context), and execution adapters.
- `src-tauri/src/storage/`: local-first persistence for runs, rooms, memos, settings, artifacts, events, and indexes.
- `messages/`: i18n resources. When adding UI text, update both `messages/en.json` and `messages/zh-CN.json`.
- `scripts/`: repo validation, release, and i18n check scripts.
- `docs/`: implementation plans and review responses. Active docs use `[wip]` prefix; completed ones use `[done]`.

## High-level architecture

### 1. Frontend state is store-centric

The frontend is not organized around thin pages with all logic inline. The important behavior lives in stores and API wrappers.

Key stores:
- `src/lib/stores/session-store.svelte.ts`: the single source of truth for chat session state. It owns the session phase/state machine, timeline, tool events, usage, permissions, elicitation prompts, task notifications, and session metadata.
- `src/lib/stores/group-chat-store.svelte.ts`: manages group chat list/detail state, group chat creation, participant creation, run attachment, roundtable messaging, one-click Debate/Summary actions, stepper snapshot state, and onboarding flow.
- `src/lib/stores/memo-store.svelte.ts`: handles global-only memos in the pop-out panel (project-scoped memo was removed from the visible UI in Phase 7; the backend still supports it for backward compatibility).

When debugging UI behavior, check stores before editing route components.

### 2. The frontend talks to a transport abstraction, not directly to one runtime

`src/lib/transport/index.ts` selects either:
- `TauriTransport` in the desktop app, or
- `WsTransport` in browser/web mode.

That means command invocation and event subscription behavior may depend on whether code runs inside Tauri or over WebSocket. Do not assume a browser-only or desktop-only path without checking transport usage.

### 3. Tauri commands are the backend API boundary

The frontend mainly talks to Rust through Tauri commands in `src-tauri/src/commands/`.

Important command groups:
- `commands/chat.rs`: chat send path for `pipe_exec` runs, attachment staging, and spawn flow.
- `commands/session.rs`: actor-backed session lifecycle, auth/env resolution, resume/stop flow, provider-native launch config generation, and Windows MSVC env injection.
- `commands/group_chat.rs`: group chat CRUD, participant creation, run attachment, `list_group_chat_run_index` (sidebar grouping), and `get_group_chat_turn_snapshot` (stepper replay).
- `commands/characters.rs`: AiCharacter CRUD (list/create/update/delete_character).
- `commands/plans.rs`: PlanArtifact CRUD (get/create/update/approve/complete_plan).
- `commands/balance.rs`: DeepSeek and MiMo balance/usage queries (Phase 7 balance helper with cookie-based auth for MiMo).
- `commands/runs.rs`, `commands/history.rs`, `commands/memos.rs`, `commands/settings.rs`: persistence-backed app features.

If a frontend API call seems to "just update UI", verify whether it actually maps to a persisted Tauri command first.

### 4. There are three execution paths for agent runs

A run can execute through:
- `SessionActor` / stream-session path (Claude Code sessions and Claude-compatible providers).
- `PipeExec` path (used for print/pipe workflows; also drives the Codex JSONL adapter).

Relevant code lives in `src-tauri/src/agent/`:
- `executor/mod.rs`: `Executor` trait + `for_agent()` dispatch (claude → `ClaudeExecutor`, codex → `CodexExecutor`).
- `executor/codex.rs`: `codex exec --json` JSONL adapter — spawns a short-lived child per turn, parses the stream, persists `thread_id` to `RunMeta.conversation_ref`.
- `executor/codex_state.rs`: protocol state machine mapping Codex JSONL events to `BusEvent`s.

When fixing bugs around chat, resume, room participation, or provider support, always confirm which execution path is in play.

### 5. Provider identity is separate from execution identity

This codebase intentionally separates what the UI presents as a provider from which execution agent actually runs the work.

Current providers (Phase 9.z):
- **Official CLI providers** (subscription): Claude, Codex — use their native CLI with bypass/yolo permissions.
- **Claude-compatible API providers**: DeepSeek, GLM, QWEN, KIMI, MiMo Pro — displayed as first-class providers but execute through Claude Code sessions with `platform_id`-based configuration injection.
- **Custom providers**: User-created `custom-{timestamp}` endpoints configured via Settings → Connection. Use the same `build_parameterized_env` path as parameterized providers. Require explicit base_url and model.

Key files:
- `src/lib/utils/provider-catalog.ts`: PHASE7_PROVIDERS array, provider metadata, and label resolution.
- `src/lib/utils/platform-presets.ts`: platform-specific base URLs and configuration defaults.

Provider-native launch config generation (Phase 9.z):
- DeepSeek and MiMo Pro use a fixed-URL template (API key only; default model and base_url from preset).
- GLM, QWEN, KIMI use a shared parameterized template (API key + base URL + model).
- Custom providers (`custom-*`) use the same parameterized template as GLM/QWEN/KIMI, with user-provided base_url and model. Validated via `validate_provider_credential` which requires api_key, base_url, and model.
- All providers use per-session temp JSON (`session-{run_id}.json`) generated fresh from the latest credential in settings, passed via `claude --settings <temp-json>` to override global `~/.claude/settings.json`.
- The temp JSON now merges native `~/.claude/settings.json` as the base (preserving hooks, plugins, env vars, MCP servers), then strips sensitive keys (`apiKey`, `primaryApiKey`), and overlays provider-specific fields. This ensures user config (hooks, enabledPlugins, enabledMcpjsonServers) survives the `--settings` override.
- Managed MCP servers (`UserSettings.mcp_servers`) are additively merged into the temp JSON alongside native MCP servers.
- User-configurable env vars are stored in `PlatformCredential.extra_env` and merged via a whitelist (`ALLOWED_EXTRA_ENV_KEYS` in `provider_claude_config.rs`). Only model tier overrides and effort level are allowed; stability vars cannot be overwritten.
- Chat page model dropdown shows tier-labeled models (Opus/Sonnet/Haiku) via `expandModelsToTiers`, with extra_env overrides applied. Model hot-switching via `set_model` control protocol works for both Anthropic and third-party providers.

Do not collapse provider selection, model display, and actual CLI spawn logic into a single assumption.

### 6. Run and GroupChat are both persisted local-first objects

The app persists state to local storage files rather than treating sessions as purely in-memory.

Key storage modules:
- `src-tauri/src/storage/runs.rs`: creates and updates `RunMeta`, resolves connection profile/platform snapshots, and stores per-run metadata.
- `src-tauri/src/storage/group_chats.rs`: stores `group_chat.json`, public timeline JSONL, private turns, plan artifacts, and participant meta files. Uses per-ID mutex locking for concurrent access safety.
- `src-tauri/src/storage/events.rs`, `artifacts.rs`, `memos.rs`, `settings.rs`: supporting persistence.

Useful mental model:
- A `Run` is the persisted execution record.
- A `GroupChat` is a persisted orchestration container that references runs.
- Deleting a group chat should not imply deleting the linked runs.
- Each group chat can have an active `PlanArtifact` with tasks, status tracking, and user notes.

### 7. Group Chat orchestration is more than simple grouping

Group chats are not just folders for runs. The backend actively orchestrates turns.

`src-tauri/src/group_chat/orchestrator.rs` handles:
- fanout turns
- `@debate`
- `@summary @name`
- `@DisplayName message` (SingleTarget — public turn to only the named participant)
- `/dm @Name message` (Private — private turn, content hidden from public timeline)
- auto-chain routing: after SingleTarget, scans response for `@mentions` and chains up to 3 hops
- role-based system prompt injection via `--append-system-prompt` from linked AiCharacter

The frontend group chat page uses a multi-pane workspace layout:
- Participant panels with toggleable visibility, showing label, provider/model, status badge, and elapsed time.
- `GroupChatStepper` component showing turn-by-turn status with clickable snapshot replay.
- `PlanPanel` component for task checklist management (status cycling, approve/complete, user notes).
- The action toolbar (Debate/Summary/summarizer selector) and composer with `@mention` autocomplete.
- Group chat participant runs appear in a virtual "Group Chats" folder in the sidebar.

If changing group chat behavior, inspect both:
- `src/lib/stores/group-chat-store.svelte.ts`
- `src/lib/components/GroupChatLayout.svelte`
- `src-tauri/src/group_chat/orchestrator.rs`

### 8. Memo is a global pop-out panel, not a full page

As of Phase 7 Task 8:
- `/memo` is a redirect page that navigates to `/chat`.
- The sidebar icon rail no longer includes a Memo link.
- A clipboard-icon toggle button in the top bar opens `GlobalMemoPanel` (a right-side slide-out panel).
- The panel uses global scope only, with a single input + add button, and flat list items (text, timestamp, copy, delete).
- Command Palette dispatches `ocv:toggle-memo` event.
- The Room page no longer has a memo textarea or `memo_preview` display.

Key files:
- `src/lib/components/GlobalMemoPanel.svelte`
- `src/lib/stores/memo-store.svelte.ts`

### 9. History reads CC native sessions, not Claw GO runs

As of Phase 9, the `/history` page reads directly from `~/.claude/projects/` via the `discover_cli_sessions` Tauri command. It no longer uses `~/.claw-go/runs/`.

Key behaviors:
- Subagent sessions (`hasSubagents: true`) are filtered out — only user-initiated conversations are shown.
- Sessions are cross-referenced with imported runs; already-imported sessions skip re-import and use `existingRunId`.
- The `import_cli_session` command imports a CC session as a `RunMeta`, then `startSession(mode="resume")` resumes it.
- The page supports text search (prompt + cwd + model) and project pill filtering.
- When `DiscoverResult.truncated` is true, a warning banner is shown.

Key files:
- `src/routes/history/+page.svelte` — History page (direct call to `discover_cli_sessions`)
- `src-tauri/src/commands/cli_sync.rs` — Tauri commands: `discover_cli_sessions`, `import_cli_session`
- `src-tauri/src/storage/cli_sessions.rs` — session discovery, parallel processing via rayon
- `src/lib/types.ts` — `CliSessionSummary`, `DiscoverResult` types

### 10. Windows-native behavior matters here

This repository is explicitly Windows-first. Do not assume WSL/macOS/Linux workflows.

Important backend support already exists for Windows-native CLI execution:
- automatic MSVC developer environment injection for native-toolchain projects.
- special handling for npm `.cmd` shims so Codex can launch as `node.exe + CLI js`.
- code in `src-tauri/src/agent/windows_msvc_env.rs` and related session/chat spawn paths.

**MSVC injection enhancements (Phase 8):**
- Auto-detection extended: `CMakeLists.txt`, `vcpkg.json`, `*.sln`, `*.vcxproj`, `*.pro`, `*.pri` (root-only).
- Chat/GroupChat policy split: `MsvcPolicy` enum — chat uses `AllowByMode`, group chats use `Disabled` (backend-enforced).
- `msvc_injected: Option<bool>` propagated via `BusEvent::SessionInit` to frontend; MSVC badge in `SessionStatusBar`.
- `MsvcEnvSkipReason::RoomPolicy` (distinct from `DisabledByUser`) for diagnostics.

When changing spawn behavior, PATH handling, or provider launch commands, preserve Windows desktop compatibility.

### 11. MSVC linker resolution (cargo config fix)

On this machine, `C:\Program Files\Git\usr\bin\link.exe` (Git's Unix `link` tool) shadows the MSVC linker. Cargo must be told to use the real linker explicitly:

**File:** `C:\Users\InBlu\.cargo\config.toml`
```toml
[target.x86_64-pc-windows-msvc]
linker = "C:/Program Files (x86)/Microsoft Visual Studio/18/BuildTools/VC/Tools/MSVC/14.50.35717/bin/Hostx64/x64/link.exe"
```

Without this config, `cargo build`, `cargo test`, and `npm run tauri build` will fail at the build-script linking stage. If the MSVC Build Tools version changes, update the path. Use forward slashes (Windows accepts them and they avoid TOML escaping issues).

**Known issue: Rust unit tests fail with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139).** Root cause: VS 18 BuildTools MSVC 14.50.35730 links against a newer VCRUNTIME140.dll than the one installed in System32 (14.50.35719). The Windows loader finds the old System32 DLL first and rejects the binary because a required CRT entry point is missing. Workaround: use `cargo check` for Rust code validation; it catches compile errors without running the binary. Full test runs need either a matching VC++ redistributable update or a clean VM/CI environment.

### 12. Target directory cleanup

The `src-tauri/target/` directory can accumulate 30+ GB of incremental compilation artifacts (primarily in `debug/incremental/` and `debug/deps/`). Periodically clean:

```bash
# Aggressive: removes everything except release artifacts
rm -rf src-tauri/target/debug
rm -rf src-tauri/target/release/{deps,build,.fingerprint,incremental}

# Keep only latest installers
find src-tauri/target/release/bundle -name '*.msi' ! -name '*<version>*' -delete
find src-tauri/target/release/bundle -name '*.exe' ! -name '*<version>*' -delete

# Cargo-native clean (use sparingly — removes all build caches)
cargo clean --manifest-path src-tauri/Cargo.toml
```

## Existing repo-specific guidance

These are already established patterns in the repo and should be preserved:

- Use Svelte 5 runes patterns in frontend code (`$state`, `$derived`, `$effect`, `$props`).
- Keep provider identity separate from execution identity.
- Tests are colocated where practical; frontend tests use `*.test.ts`, Rust tests stay near the module under test.
- Conventional Commit style is used in git history (`feat:`, `fix:`, `chore:`).
- Do not commit API keys, local settings, or generated runtime state.
- `.arena` files are local runtime context mirrors (legacy from Room era) and may contain run context, memo text, and recent public previews; they are not shareable artifacts.

## Implementation history

Key phases and their status:

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Memo implementation | [done] |
| 3 | Roundtable implementation | [done] |
| 4 | Driver/Copilot | [done] |
| 4.5 | Research follow-up | [done] |
| 5 | Capability matrix | [done] |
| 5.5 | Native CLI chat parity | [done] |
| 6 | Driver MCP | [done] |
| 7 | Native CLI auth, provider settings, roundtable layout | [done] |
| 7.x | Provider config dynamization, per-session JSON, MiMo Pro | [done] |
| 7.y | Room optimizations: delete cleanup, incremental turns, status labels, context menu | [done] |
| 8 | Gemini removal, Stepper mini-map, @Name SingleTarget, Room sidebar grouping, prompt constraint | [done] |
| 8.x | UX optimizations: sidebar preview fix, update URL, provider model auto-switch, room command hints | [done] |
| 9 | History page rewrite: CC native sessions, subagent filtering, simplified UI | [done] |
| 9.x | Room adapter timeout fix: activity-aware timeout, cancel turn, frontend UX | [done] |
| 9.y | Provider presets cleanup, extra_env whitelist, tier model labels, collapsible config panel, old ID removal, label disambiguation | [done] |
| 9.z | Custom Provider support, native config merge, managed MCP injection, SENSITIVE_KEYS centralization | [done] |
| 10 | Group Chat refactor: Room→GroupChat rename, Character Library, Plan mechanism, Context Management MVP, Role System Prompt, Auto-chain | [done] |
| 10+ | Character Memory System: LanceDB, petgraph, LLM auto-extraction, hybrid search, sigma.js viz, review queue, injection config UI | [done] |
| 10+ (v2.2.0) | 群聊体验优化: Markdown 渲染, 长文折叠, Executor 过滤, 上下文共享, P0 bug 修复, 3 轮多路审查 | [done] |
| 10+ (v2.3.0) | Memory extraction chat_api_key 分离: EmbeddingConfig 独立 chat 凭据, 设置 UI, 4 路审查, 安全修复 | [done] |
| 10+ (v2.3.0) | 记忆提取准确性: 说话人标注, LLM 置信度, 群聊侧边栏私聊修复, 时间线自动滚底 | [done] |
| 10+ (v2.4.0) | 1M 上下文窗口: per-provider AUTO_COMPACT_WINDOW, 前端进度条静态映射 + fallback, advisory 软策略, CONTEXT_TURN_WINDOW 3→1, 3 路审查修复 | [done] |
| 10+ (v2.5.0) | Doctor 诊断面板, FilesPanel 树视图重写, 7 项审查修复 | [done] |
| 10+ (v2.6.0) | Preview panel code review: keyboard scope, $effect reactivity, DOMPurify styles, regex URL FP, dirty guard, base64 guard | [done] |
| 10+ (v3.0.0) | 记忆系统重构: SQLite FTS5 用户中心架构, 移除 LanceDB + petgraph + Embedding API, 15 项审查修复, shared injection, 前端去重 | [done] |
| 10+ (v3.1.0) | openInvest Phase 1: invest.db 数据层, scope-aware 记忆, /invest + /memory-mgmt 路由, MoreMenu, 18 commits, 8 项审查修复 | [done] |
| 10+ (v3.2.0) | openInvest Phase 2: Dashboard KPI, 持仓管理(HOLD/WATCH), 交易对话框, 交易记录+CSV导出, 策略配置CRUD, Chart.js PnL 图表, Tushare HTTP client(自定义代理), PnL 定时快照, 交易日历同步, Legacy 迁移, 55+ i18n keys, 13 项审查修复 | [done] |
| 10+ (v3.3.0) | openInvest Phase 3a+3b+3c+4a: LLM 委员会编排, SSE streaming, 角色配置, Insights Feed, Pipeline Notifications, Event Watch(Tushare 新闻+LLM 归一化), Scheduler 6 jobs, Verdict Review, Dreaming 3 阶段管道, FTS5 domain_insights, Archived 视图 | [done] |
| 10+ (v3.4.0) | openInvest Phase 4b: 系统二级页 7 Tab(Regime/Datasource/PnL/Dreams+3 复用), 用户档案(/settings/profile), 每日报告定时任务, 9 项审查修复 | [done] |
| 10+ (v4.0.0+) | openInvest Fix Tasks: 4 P0 bug, 3 demo HTML, 6 P1 大改(加入观望/Profile 迁移/侧边栏顺序/运行全部/Replay 增强/角色配置重写), 5 P2/P3(多资产总览/Dashboard 卡片/i18n/MEMORY.md 默认/FilePathLinks), 15 项代码审查修复 | [done] |
| 10+ (v5.0.0) | openInvest Phase 5: 委员会 LLM 工具+Prompt 全面升级 — 角色精简(7→4 enum, Round R1/R2), Regime 模块(RSI-14/价格分位数), Tushare 宏观接口(4 方法), Yahoo Finance 客户端(6 国际指标), macro_cache 存储层(12 指标), 调度+cron+工具重写(双数据源/MA120), 工具分角色开放(role_tool_defs), 6 个新 Prompt, Parser+Analysis 更新(10 新字段/Gate 4), 15 项代码审查修复 | [done] |
| 10+ (v5.0.1) | 代码审查修复: 9 路审查 15 项修复 — 数据完整性(asset_type 迁移/notional 保护/事务包装/dry_run 透传/索引对齐), UI 正确性($derived 修复/R1 Prompt 路径/deleteTrade 刷新/regime 成功检查/TradeDialog 验证), 死代码清理(archive_decision 移除/events.jsonl 覆盖/多日期回放/手动审查/verdict ID 查询) | [done] |
| 10+ (v5.0.2) | Yahoo Finance 429 限流修复: fetch_chart_raw/fetch_yahoo_news 重试 3 次+指数退避, fetch_all_quotes/fetch_china_finance_news 串行化+300ms 间隔 | [done] |
| 10+ (v5.0.2) | 代码审查修复: 15 项修复 — 数据完整性(initial_balance 写入/family_support 迁移集中/provider config 对称), UI 正确性(bars 排序/account purpose 迁移/loading 状态/model_override 透传), 死代码清理(parse_provider_id 去重/row_to_verdict 提取/ETF 过滤/DB 错误日志/saveTimer 清理) | [done] |
| 10+ (v5.0.3) | 委员会中文化+REGIME 展示+Parser 双语+Profile 双注入: Gate notes/归档报告中文化, RegimeStep 事件扩展+前端 REGIME 卡片, Parser 双语支持(any 系列函数), 6 个 Prompt 模板字段名中文化, Profile Risk R1+CIO 双注入, 风险指标预计算(CONCENTRATION_PCT/PNL_PCT/DRY_POWDER_CNY), PortfolioData 消除重复 DB 查询, 6 项审查修复 | [done] |
| 10+ (v5.0.4) | Yahoo Finance 429 二次修复+扫描增强+add_watch+fund_basic: macro_refresh 并发改串行, 请求间隔统一 500ms 常量, ScanResult errors 字段+前端日志, Yahoo 兜底阈值常量化, log+push 去重, ScanResult TS 接口, add_watch action 迁移, fund_basic 客户端过滤 | [done] |
| 10+ (v5.0.5) | 代码审查修复: Yahoo 认证加固+Tushare 代理验证+并发安全 — TushareClient URL scheme 验证/reqwest::Proxy 静默失败修复/Mutex 中毒恢复/重试末尾退避消除/ensure_session 惊群修复/Mutex→RwLock/with_token_and_proxy 新方法/resolve_local_proxy_url 共享辅助函数/代理 URL 格式验证/端口范围检查/代理函数语义改进 | [done] |
| 10+ (v5.1.0) | UI 设计系统统一: 暖色暗黑固定主题+自定义标题栏+Inter 字体+移除主题/色彩方案切换+Settings 底部分隔 — CSS 变量重写/tauri.conf.json decorations:false/46 项功能入口回归通过 | [done] |
| 10+ (v5.1.1) | Bug 修复+Chat UI 升级+代码审查优化: 标题栏权限/Python Overlay 竞态/Index 首页/消息+输入框+状态栏+右侧面板 UI/Flat token 别名/format 工具复用/recompute_notional 方法/并发价格获取/6 项 simplify 审查修复 | [done] |
| 10+ (v5.1.2) | ETF 价格修复+事件扫描增强+simplify 审查修复: daily_api() ETF 路由/Severity::as_str()/循环合并/英文关键词/零分配截断/record_trade 自动 recalculate/notional 兜底/CIO 100 股规则 | [done] |
| 10+ (v5.2.0) | 委员会 L1-L4 策略框架升级: 5 角色+L4 Officer/8 步 Pipeline/7 Prompt 重写/AssetContext 数据注入层/4 新工具/30+ Parser 字段/行为红灯评分/catalyst Tier 框架/Simplify 8 项审查修复 | [done] |
| 10+ (v5.2.1) | Memory Extraction 设置迁移+全局记忆文件扩展+委员会 7 项改进(成本基准收益率/交易过滤/ConfirmDialog/Dream 修复/事件中文化/策略注入 Risk/L4 Officer 工具面板)+6 i18n keys | [done] |
| 10+ (v5.2.2) | /invest 全模块 UI 设计系统统一: 28 文件(27 组件+1 页面)暖色暗黑迁移/5 Tab+14 子 Tab+6 通用组件/CSS 变量映射/Badge 重设计/tab 导航重写/svelte-check+ESLint+Build 全通过 | [done] |
| 10+ (v5.2.3) | /invest UI 修复+委员会 3 子页布局重构: [data-invest-scope] CSS token 作用域覆盖(--accent 金色/--color-error 暖红/--bg-input 输入层次)/CommitteeReplayTab 250px 双栏重写/CommitteeArchiveTab 双栏+verdict regex/CommitteeToolsTab 9×5 访问矩阵真表格/抽 invest-verdict.ts+pipeline-config ROLE_COLORS+getStepState/5 项 simplify 审查修复(verdictMap 预计算/loadGen race fix/死代码清理) | [done] |
| 10+ (v5.2.4) | 批量实时行情 API+交易流程简化+返回率计算修复: get_realtime_quotes 批量 IPC(rt_k+fund_daily 降级)/refreshPrices N→1 次调用/buyStock+sellStock 移除手动 holding CRUD(依赖 record_trade recalculate)/totalReturnPct holdingsMarketValue/maxHolding 修复/--text-secondary+--text-tertiary/3 项 simplify(partition+join_all 并发+死赋值清理) | [done] |
| 10+ (v5.2.5) | 委员会数据缓存+8段注入优化+前端名称显示: stock_data_cache 永久缓存(三元主键/batch_upsert 事务)/build_asset_context cache-first+typed deserialization(DailyBasic/FinaIndicator/ReportRc)/load_prompt_for_round 统一 17 占位符/Risk prompt 资产上下文(PE/PB/ROA/负债率/评级)/CIO 数据质量警告/exec_company_info+exec_moneyflow cache-first(nameMap/isMarketOpen 盘中检测/cron sanitize/6 项 simplify | [done] |
| 10+ (v5.2.6) | 持仓名称持久化+收盘价格修复+代码搜索+数据初始化+持仓编辑+手动交易: trades.name/trade_date 字段+DB migration 回填/recalculate_holdings_inner 从 trade 恢复名称/investStore.nameMap 合并 3 源(持仓+行情+交易)/refreshPrices 智能守卫(收盘后已有缓存则跳过)/stock_basic ts_code 精确匹配+fund_basic 代码匹配/init_invest_data 命令/TradeDialog add_trade+edit_holding 模式/HoldingsTable 编辑按钮/recordTrade+updateHoldingMeta store 方法/TRADE_COLUMNS 常量+trade_from_row/6 项 simplify | [done] |
| 10+ (v5.2.8) | invest DB 迁移修复+Watch 价格刷新+委员会 Bug 修复(Quant 资金流向/Risk 集中度)+代码审查优化: trades_new 10→12 列/FALLBACK 删库重试/invest_db_path+ensure_conn+has_column 提取/冗余迁移块删除/refreshPrices 守卫修复(全持仓缓存检查)/addToWatch 价格预填/EventWatchTab String() 类型修复/ETF rt_k adj_nav fallback/PnL 快照手动触发刷新修复(refreshPnlSnapshots+按 job 定向刷新+并行化)/CSS border 统一/Quant R1 资金流向缓存检查升级(按类型逐一检查+定向 refresh_moneyflow_cache)/Risk R1 集中度分母含现金对齐前端/total_assets()+has_type 闭包+entries.push 内存追加/15 项 simplify | [done] |
| 10+ (v5.2.9) | 腾讯行情 API 集成+ETF 价格修复+asset_type 全链路修复+DB 迁移安全修复+代码审查优化: tencent_quotes 模块(fetch_quotes/~分隔解析/共享 reqwest::Client)/realtime_quotes 四层降级(腾讯→部分成功→rt_k→daily)/resolve_close_idx 统一价格列定位/get_latest_price adj_nav fallback/Trade.asset_type 字段+DB migration 回填/update_trade SQL 补 asset_type/resolve_asset_type 推导/is_etf_symbol 共享函数/前端 6 处 IPC 补 assetType/TradeDialog add_trade 传入/init_with_fallback 备份策略(迁移失败先备份再删除)/backup_db_files 时间戳备份/migrate_trades_table 宽容迁移(动态列检测+NULL 填充)/7 项 simplify(移除重复 RealtimeQuote+复用 client+共享 is_etf_symbol+长度守卫 38+部分成功降级+注释更新+conn.transaction() RAII 回滚+get_table_columns Result 防静默擦除+DB_SIDECAR_EXTS 常量+HashSet O(1) 查找+TRADES_COLUMNS 静态常量+删除 20 行死代码) | [done] |
| 10+ (v5.2.10) | 委员会直播页面崩溃修复 — watch/hold 持仓去重+Simplify 审查修复: watchHoldings store 层 holdSymbolSet 去重(根因: buyStock 不清理 watch 条目→重复 key→Svelte 运行时崩溃)/CommitteeLiveTab+CommitteeReplayTab 组件级冗余去重移除/3 项 simplify(altitude: 去重提升到 store 层/reuse: 消除跨组件重复 seen Set/simplification: 双循环合并为 map) | [done] |

Detailed plans and review responses are in `docs/`.

## Notes for future edits

- Vite dev server is configured for port `1420`, with HMR on `1421` when `TAURI_DEV_HOST` is set.
- Vite watch ignores backend/build/runtime directories such as `src-tauri`, `.claude`, `.claw-go`, `memory`, and other non-frontend paths to avoid reload churn during active agent sessions.
- SvelteKit uses `adapter-static` with `fallback: "index.html"`.
- Frontend test environment is `node`, configured in `vitest.config.ts`.
- Provider-native launch config templates are in `src-tauri/src/commands/session.rs` (builder boundary).
- Codex uses the `codex exec --json` JSONL adapter under `agent/executor/codex.rs`. Each turn is a short-lived process; multi-turn continuity is provided by Codex's native `thread_id` (stored in `RunMeta.conversation_ref` as `CodexThread`). Stop is implemented by killing the child via `commands/runs.rs::stop_run`; the JSONL stream truncates cleanly at the last completed event. The Windows `.cmd` shim resolution (`resolve_windows_npm_shim` in `stream.rs`) continues to apply — `CodexExecutor` reuses it via the dispatcher. Do not reintroduce PTY-based execution or `--last`-based resume.
- Group chat participant meta (delivery cursor, session turn count, session seq) is stored at `group-chats/{id}/participants/{participant_id}.meta.json`.
- Plan artifacts are stored at `group-chats/{id}/plan.json` with atomic writes (tmp+rename).
