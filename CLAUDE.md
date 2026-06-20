# CLAUDE.md

This file guides Claude Code (claude.ai/code) when working in this repository.

## Overview

Windows-first Tauri desktop app: SvelteKit (Svelte 5 runes) frontend + Rust backend. Built on Claw GO's local-first architecture, adding Claude Session Hub concepts (Group Chats, Memo, Roundtable) and the openInvest quant subsystem, without disrupting the core `/chat` path.

Core product model:
- `Run` — the smallest execution unit (one persisted agent session).
- `GroupChat` (formerly Room) — an orchestration layer over one or more runs.
- `AiCharacter` — a reusable persona template (role_type, role_instruction, default provider/model), stored in UserSettings.
- The provider shown in the UI is not always the execution agent under the hood (see architecture §5).

**Current version:** v5.5.4 (Phase 10+). Full per-version history lives in `docs/changelog.md` — consult it instead of duplicating release notes here.

## Standard workflow

1. **Implement** the feature or fix.
2. **Update** the relevant docs in `docs/`.
3. **Code review** via the `simplify` skill (parallel agents: reuse, quality, efficiency).
4. **Fix** all review findings.
5. **Commit** with Conventional Commit style (`feat:`, `fix:`, `chore:`).
6. **Verify** with `npm run build`, `npm run i18n:check`, and relevant tests.

## Common commands

```bash
# Dev
npm install && npx svelte-kit sync
npm run dev          # Vite dev server on port 1420
npm run tauri dev    # desktop app

# Frontend quality
npm run lint         # + npm run lint:fix
npm run check
npm run test
npm run build

# Rust quality
cargo check  --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo fmt    --manifest-path src-tauri/Cargo.toml --check
npm run rust:check

# Project-wide
npm run i18n:check
npm run verify       # lint + fmt + i18n + tests + build + Rust checks
npm run release <version|patch|minor|major>
npm run tauri build  # → src-tauri/target/release/ClawGO.exe + nsis/msi bundles
```

Single test:
```bash
npm test -- src/lib/stores/group-chat-store.test.ts
cargo test --manifest-path src-tauri/Cargo.toml storage::memos::tests:: -- --nocapture
```
See §11 for a known Rust-test runtime issue on this machine — prefer `cargo check`.

## Repo structure

**Frontend (`src/`)** — store-centric Svelte 5:
- `lib/stores/` — stateful stores; the real behavior lives here, not in route components. Key: `session-store`, `group-chat-store`, `memo-store`, `invest-store`, `invest-committee-store`, `user-memory-store`, `character-memory-store`, `preview-store`, `doctor-store`.
- `lib/components/` — shared UI (GlobalMemoPanel, ChatMessage, GroupChatLayout, GroupChatStepper, PlanPanel, modals, settings/provider panels, invest components).
- `lib/utils/` — provider-catalog, platform-presets, format, agent-capabilities, invest-verdict, invest-status, i18n helpers.
- `lib/transport/` — desktop Tauri IPC vs browser/WebSocket abstraction (§2).
- `routes/` — chat, history, explorer, plugins, usage, memory, memory-mgmt, invest, config, settings, release-notes. (`/memo` redirects to `/chat`; memo is a pop-out panel.)

**Backend (`src-tauri/src/`)** — Rust:
- `commands/` — the Tauri IPC boundary (§3). ~35 modules incl. chat, session, group_chat, characters, runs, history, memos, settings, invest, teams, agents, mcp, git, files, diagnostics, balance. (There is no `plans.rs` — PlanArtifact CRUD lives in `group_chat.rs`.)
- `agent/` — launch/session/stream, executor dispatch (claude/codex), control protocol, Windows MSVC env.
- `group_chat/` — turn orchestration + user/character memory (injection, extraction, dream, context) + execution adapters. Auto memory extraction (`memory_extraction.rs`) now fires for **both** group-chat turns (via `orchestrator.rs`) and normal `/chat` turns (via `agent/session_actor.rs` on turn idle), with per-source debounce/daily-cap.
- `invest/` — the openInvest quant subsystem (§12).
- `storage/` — local-first persistence (runs, group_chats, memos, settings, events, indexes, `invest/`).
- `tushare/`, `tencent_quotes.rs` — market-data clients. `python/` — Python RPC bridge (AkShare). `web_server/` — browser/WS mode. `hooks/` — CC hook setup + team watcher.
- `messages/` — i18n; update both `en.json` and `zh-CN.json` for any UI text.
- `scripts/` — validation, release, i18n check.
- `docs/` — plans and reviews (`[wip]`/`[done]` prefixes) plus `changelog.md`.

## High-level architecture

### 1. Frontend state is store-centric
Behavior lives in stores, not thin pages — check stores before editing route components. `session-store.svelte.ts` is the single source of truth for chat session state (phase machine, timeline, tool events, usage, permissions, elicitation, notifications).

### 2. Transport abstraction, not one runtime
`lib/transport/index.ts` selects `TauriTransport` (desktop) or `WsTransport` (browser/web). Command invocation and event subscription can differ by transport — don't assume a desktop-only or browser-only path without checking transport usage.

### 3. Tauri commands are the backend API boundary
The frontend talks to Rust through `commands/`. Notable: `chat.rs` (pipe_exec send + attachments), `session.rs` (actor session lifecycle, auth/env, resume/stop, provider launch config, MSVC injection), `group_chat.rs` (group chat CRUD, participants, run attachment, `list_group_chat_run_index`, turn snapshots, **and PlanArtifact CRUD**), `characters.rs`, `invest.rs`, `balance.rs`. If a call seems to "just update UI", verify it maps to a persisted command first.

### 4. Agent execution paths
`agent/executor/mod.rs` defines the `Executor` trait + `for_agent()` dispatch (claude → `ClaudeExecutor`, codex → `CodexExecutor`). Two runtimes: the `SessionActor`/stream-session path (Claude + Claude-compatible providers) and the pipe/exec path (`codex exec --json` JSONL adapter in `executor/codex.rs` — short-lived child per turn, `thread_id` stored in `RunMeta.conversation_ref`). The invest committee has its own CLI executor (§12). Confirm which path is in play when fixing chat/resume/provider bugs.

### 5. Provider identity ≠ execution identity
The UI's provider is not always the executing agent.
- **Official CLI** (subscription): Claude, Codex — native CLI, bypass/yolo permissions.
- **Claude-compatible API**: DeepSeek, GLM, QWEN, KIMI, MiMo Pro — shown as first-class providers but execute through Claude Code sessions with `platform_id`-based config injection.
- **Custom** (`custom-*`): user endpoints (Settings → Connection); require base_url + model, validated by `validate_provider_credential`.

Launch config: each session gets a fresh temp JSON (`session-{run_id}.json`) merged from native `~/.claude/settings.json` (preserving hooks/plugins/MCP), with sensitive keys stripped and provider fields overlaid, passed via `claude --settings`. Managed MCP servers and a whitelisted `extra_env` (`ALLOWED_EXTRA_ENV_KEYS` in `agent/provider_claude_config.rs`) are merged in. Templates live in `commands/session.rs`; the catalog in `lib/utils/provider-catalog.ts` + `platform-presets.ts`. Model dropdown shows tier-labeled models (Opus/Sonnet/Haiku); hot-switch via `set_model`. Don't collapse provider selection, model display, and CLI spawn into one assumption.

### 6. Run and GroupChat are persisted local-first
State lives in local files, not just memory. `storage/runs.rs` (RunMeta + connection/platform snapshots) and `storage/group_chats.rs` (`group_chat.json`, public timeline JSONL, private turns, plan, participant meta; per-ID mutex). A Run is the execution record; a GroupChat references runs (deleting one does not delete its runs); each GroupChat can hold an active PlanArtifact.

### 7. Group Chat orchestration
`group_chat/orchestrator.rs` actively drives turns: fanout, `@debate`, `@summary @name`, `@DisplayName` (SingleTarget — public turn to one participant), `/dm @Name` (private, hidden from public timeline), auto-chain (scan reply for @mentions, up to 3 hops), and role system-prompt injection via `--append-system-prompt`. Frontend is a multi-pane workspace. To change behavior, inspect `group-chat-store.svelte.ts`, `GroupChatLayout.svelte`, and `orchestrator.rs` together.

### 8. Memo is a global pop-out panel
`/memo` redirects to `/chat`. A top-bar clipboard button toggles `GlobalMemoPanel` (global scope, flat list). Command Palette dispatches `ocv:toggle-memo`. Files: `GlobalMemoPanel.svelte`, `memo-store.svelte.ts`.

### 9. History reads CC native sessions
`/history` reads `~/.claude/projects/` via `discover_cli_sessions` (not `~/.claw-go/runs/`). Subagent sessions are filtered out; already-imported sessions reuse `existingRunId`. `import_cli_session` imports a session as a RunMeta, then resumes. Files: `routes/history/+page.svelte`, `commands/cli_sync.rs`, `storage/cli_sessions.rs`.

### 10. Windows-first
No WSL/macOS/Linux assumptions. The backend auto-injects the MSVC dev env for native-toolchain projects (`agent/windows_msvc_env.rs`) and handles npm `.cmd` shims so Codex launches as `node.exe + CLI js`. `MsvcPolicy`: chat uses `AllowByMode`, group chats `Disabled`. `msvc_injected` flows to the frontend via `BusEvent::SessionInit` (MSVC badge). Preserve Windows compatibility when changing spawn/PATH/launch logic.

### 11. MSVC linker + Rust-test runtime issue (this machine)
Git's `C:\Program Files\Git\usr\bin\link.exe` shadows the MSVC linker. `C:\Users\InBlu\.cargo\config.toml` pins the real one:
```toml
[target.x86_64-pc-windows-msvc]
linker = "C:/Program Files (x86)/Microsoft Visual Studio/18/BuildTools/VC/Tools/MSVC/14.50.35717/bin/Hostx64/x64/link.exe"
```
Without it, `cargo build`/`test` and `npm run tauri build` fail at the link step. Update the path (forward slashes) if the Build Tools version changes.

**Known issue:** Rust unit tests fail at runtime with `STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)` — the BuildTools MSVC links a newer `VCRUNTIME140.dll` than the one in System32, and the loader picks the old one. Workaround: use `cargo check` to validate Rust code (catches compile errors without running the binary). Full test runs need a matching VC++ redist or a clean VM/CI.

### 12. openInvest subsystem (`invest/`)
A self-contained quant/portfolio assistant under `src-tauri/src/invest/`, surfaced at `/invest`. Largely independent of the chat/group-chat core; persists to `storage/invest/` (`invest.db`, SQLite). Frontend state: `invest-store.svelte.ts`, `invest-committee-store.svelte.ts`. The committee role count and pipeline shape have changed across versions — read the source rather than trusting a number here.
- **Committee** (`invest/committee/`) — a multi-role LLM debate that emits a per-symbol verdict. Active roles in `roles.rs` (Macro, Quant, Risk, CIO) across two rounds (R1/R2). `orchestrator.rs` drives the pipeline; `cli_executor.rs` runs each role through the Claude CLI (committee runs CLI-only — the legacy `OpenAiCompatClient`/`llm_config.json` path was removed in v5.5.4; provider/model now come from `CommitteeTuning` + `platform_credentials`); `parser.rs` + `analysis.rs` extract structured fields; `tools.rs` exposes role-scoped data tools; `queue.rs` persists the run queue with CancellationToken-based abort; `archive.rs` writes verdict reports.
- **Data** — `tushare/` (HTTP client, custom proxy), `tencent_quotes.rs` (realtime), Python AkShare via the `python/` bridge, `international.rs` (global indices). `indicators.rs` is shared TA (RSI/MA/percentiles); `regime.rs` computes market regime; `macro_refresh.rs` caches macro indicators.
- **Scheduler** (`invest/scheduler/`) — cron jobs: PnL snapshots, event scan, daily report (`daily_report.rs`), dreaming.
- **Events** — `jin10_collector.rs` (high-frequency Jin10 feed), `event_analyzer.rs` (LLM normalization), `event_scanner.rs`.
- **Dreaming** (`invest/dreaming/`) — periodic reflection producing domain insights.
- **Verdict review** — `verdict_review.rs` (accuracy tracking).

## Repo-specific conventions
- Svelte 5 runes (`$state`, `$derived`, `$effect`, `$props`).
- Keep provider identity separate from execution identity (§5).
- Tests colocated: frontend `*.test.ts` (Vitest, node env, `src/**` only), Rust tests beside their module.
- Conventional Commits (`feat:`/`fix:`/`chore:`).
- Never commit API keys, local settings, or generated runtime state.
- `.arena` files are legacy local runtime mirrors (run context, memo text, previews); not shareable artifacts.

## Notes for future edits
- Vite: dev port `1420`, HMR `1421` when `TAURI_DEV_HOST` is set. Watch ignores `src-tauri`, `.claude`, `.claw-go`, `memory`, etc. to avoid reload churn during agent sessions.
- SvelteKit uses `adapter-static` (`fallback: index.html`).
- Provider launch config templates: `commands/session.rs` (the builder boundary).
- Codex: `codex exec --json` JSONL adapter (`agent/executor/codex.rs`), short-lived child per turn, native `thread_id` continuity (`RunMeta.conversation_ref::CodexThread`), stop = kill the child. Windows `.cmd` shim via `resolve_windows_npm_shim` in `stream.rs`. Do not reintroduce PTY execution or `--last`-based resume.
- Group chat participant meta: `group-chats/{id}/participants/{participant_id}.meta.json`. Plan artifacts: `group-chats/{id}/plan.json` (atomic tmp+rename).
- Python RPC: set `PYTHONIOENCODING=utf-8` (handled in `python/bridge.rs`); providers use lazy imports.
