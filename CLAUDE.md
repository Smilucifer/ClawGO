# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

This repository is a Windows-first Tauri desktop app with a SvelteKit frontend and a Rust backend. The project is a remaster built on OpenCovibe's local-first desktop architecture, adding Claude Session Hub concepts such as Rooms, Memo, Roundtable, Driver/Copilot, and Research workflows without disrupting the existing `/chat` path.

The core product model is:
- `Run` is the smallest execution unit.
- `Room` is an orchestration layer built on top of one or more runs.
- Providers shown in the UI are not always the same as execution agents under the hood.

**Current phase:** Phase 7 complete (2026-05-06). Post-phase additions (2026-05-07): provider config fully dynamized (reads from settings page instead of hardcoded models/URLs), per-session temp JSON (`--settings session-{run_id}.json`), MiMo Pro provider added, and MiMo balance/usage checker (cookie-based auth to platform.xiaomimimo.com, dual API: `/api/v1/balance` + `/api/v1/tokenPlan/usage`, amber-themed card on usage page). Room optimizations (2026-05-07): delete active room now stops participant sessions and soft-deletes runs, roundtable turns stream incrementally via JSONL dedup + 1500ms frontend polling, right-click "Remove Session" context menu with force-stop, participant status labels localized (pending→Starting..., running→Thinking..., etc.), seat label change auto-syncs prompt. 3 svelte-check errors (CodeEditor false positive), 22 a11y warnings, 1217 frontend tests passing, cargo check clean. Installers built at `src-tauri/target/release/bundle/`.

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
- `src-tauri/target/release/OpenCovibe.exe` (main binary)
- `src-tauri/target/release/bundle/nsis/OpenCovibe_<version>_x64-setup.exe`
- `src-tauri/target/release/bundle/msi/OpenCovibe_<version>_x64_en-US.msi`

For version bumping across all config files:

```bash
npm run release <version|patch|minor|major>
```

### Running a single test

Frontend Vitest only includes `src/**/*.test.ts`.

```bash
npm test -- src/lib/stores/memo-store.test.ts
npm test -- src/lib/stores/room-store.test.ts
npm test -- src/lib/utils/room-ui.test.ts
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
- `src/lib/components/`: shared UI components — GlobalMemoPanel, ChatMessage, CommandPalette, modals, and provider/settings panels.
- `src/lib/utils/`: frontend utilities — provider-catalog, room-ui, format, agent-capabilities, and i18n helpers.
- `src/lib/transport/`: transport abstraction between desktop Tauri IPC and browser/WebSocket mode.
- `src/routes/`: route-level UI pages — chat, rooms, memory, explorer, plugins, usage, history, settings. (`/memo` now redirects to `/chat`; memo is a global pop-out panel.)
- `src-tauri/src/commands/`: Tauri IPC command surface consumed by the frontend.
- `src-tauri/src/agent/`: agent launch, session, stream, PTY (including native PTY for Codex/Gemini), native transcript parsing, and Windows toolchain handling.
- `src-tauri/src/room/`: room orchestration, room-specific execution adapters, and roundtable prompts.
- `src-tauri/src/storage/`: local-first persistence for runs, rooms, memos, settings, artifacts, events, and indexes.
- `messages/`: i18n resources. When adding UI text, update both `messages/en.json` and `messages/zh-CN.json`.
- `scripts/`: repo validation, release, and i18n check scripts.
- `docs/`: implementation plans and review responses. Active docs use `[wip]` prefix; completed ones use `[done]`.

## High-level architecture

### 1. Frontend state is store-centric

The frontend is not organized around thin pages with all logic inline. The important behavior lives in stores and API wrappers.

Key stores:
- `src/lib/stores/session-store.svelte.ts`: the single source of truth for chat session state. It owns the session phase/state machine, timeline, tool events, usage, permissions, elicitation prompts, task notifications, and session metadata.
- `src/lib/stores/room-store.svelte.ts`: manages room list/detail state, room creation, participant creation, run attachment, roundtable messaging, and one-click Debate/Summary actions.
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
- `commands/rooms.rs`: room CRUD, participant creation, run attachment, and room capability checks.
- `commands/balance.rs`: DeepSeek, Packy, and MiMo balance/usage queries (Phase 7 balance helper with cookie-based auth for MiMo).
- `commands/runs.rs`, `commands/history.rs`, `commands/memos.rs`, `commands/settings.rs`: persistence-backed app features.

If a frontend API call seems to "just update UI", verify whether it actually maps to a persisted Tauri command first.

### 4. There are three execution paths for agent runs

A run can execute through:
- `SessionActor` / stream-session path (Claude Code sessions and Claude-compatible providers).
- `PipeExec` path (deprecated for Codex/Gemini; used for print/pipe workflows).
- **Native PTY path** (Phase 7): Codex and Gemini use PTY-backed native CLI execution with transcript-based completion detection instead of process-exit semantics.

Relevant code lives in `src-tauri/src/agent/`:
- `native_pty.rs`: PTY spawn and terminal-state classification (Completed/Stopped/Failed).
- `native_transcript.rs`: transcript file watching, baseline tracking for resume, and Codex/Gemini completion extraction.

When fixing bugs around chat, resume, room participation, or provider support, always confirm which execution path is in play.

### 5. Provider identity is separate from execution identity

This codebase intentionally separates what the UI presents as a provider from which execution agent actually runs the work.

Current providers (Phase 7):
- **Official CLI providers** (subscription): Claude, Codex, Gemini — use their native CLI with bypass/yolo permissions.
- **Claude-compatible API providers**: DeepSeek, GLM, QWEN, KIMI, MiMo Pro — displayed as first-class providers but execute through Claude Code sessions with `platform_id`-based configuration injection.

Key files:
- `src/lib/utils/provider-catalog.ts`: PHASE7_PROVIDERS array, provider metadata, and label resolution.
- `src/lib/utils/platform-presets.ts`: platform-specific base URLs and configuration defaults.

Provider-native launch config generation (Phase 7):
- DeepSeek and MiMo Pro use a fixed-URL template (API key only; default model and base_url from preset).
- GLM, QWEN, KIMI use a shared parameterized template (API key + base URL + model).
- All providers use per-session temp JSON (`session-{run_id}.json`) generated fresh from the latest credential in settings, passed via `claude --settings <temp-json>` to override global `~/.claude/settings.json`.

Do not collapse provider selection, model display, and actual CLI spawn logic into a single assumption.

### 6. Run and Room are both persisted local-first objects

The app persists state to local storage files rather than treating sessions as purely in-memory.

Key storage modules:
- `src-tauri/src/storage/runs.rs`: creates and updates `RunMeta`, resolves connection profile/platform snapshots, and stores per-run metadata.
- `src-tauri/src/storage/rooms.rs`: stores `room.json`, public timeline JSONL, private turns, research artifacts, and `.arena` room-local context files.
- `src-tauri/src/storage/events.rs`, `artifacts.rs`, `memos.rs`, `settings.rs`: supporting persistence.

Useful mental model:
- A `Run` is the persisted execution record.
- A `Room` is a persisted orchestration container that references runs.
- Deleting a room should not imply deleting the linked runs.
- Backend room memo fields are kept for old data compatibility even though the Room page UI no longer renders them.

### 7. Room orchestration is more than simple grouping

Rooms are not just folders for runs. The backend actively orchestrates room turns.

`src-tauri/src/room/orchestrator.rs` handles:
- fanout turns
- `@debate`
- `@summary @name`
- private turns like `@name message`
- driver/review and research-oriented room flows

The frontend Room page (Phase 7) uses a three-pane workspace layout:
- Three equal-width, scrollable participant panes fill the primary space.
- Pane headers show participant label, provider/model metadata, status badge, and elapsed time.
- A collapsible turn-history strip sits below the panes with color-coded turn chips and per-participant status dots.
- The action toolbar (Debate/Summary/summarizer selector) and composer are fixed at the bottom.

If changing room behavior, inspect both:
- `src/lib/stores/room-store.svelte.ts`
- `src/routes/rooms/+page.svelte`
- `src-tauri/src/room/orchestrator.rs`

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

### 9. Windows-native behavior matters here

This repository is explicitly Windows-first. Do not assume WSL/macOS/Linux workflows.

Important backend support already exists for Windows-native CLI execution:
- automatic MSVC developer environment injection for native-toolchain projects.
- special handling for npm `.cmd` shims so Codex/Gemini can launch as `node.exe + CLI js`.
- code in `src-tauri/src/agent/windows_msvc_env.rs` and related session/chat spawn paths.

When changing spawn behavior, PATH handling, or provider launch commands, preserve Windows desktop compatibility.

### 10. MSVC linker resolution (cargo config fix)

On this machine, `C:\Program Files\Git\usr\bin\link.exe` (Git's Unix `link` tool) shadows the MSVC linker. Cargo must be told to use the real linker explicitly:

**File:** `C:\Users\InBlu\.cargo\config.toml`
```toml
[target.x86_64-pc-windows-msvc]
linker = "C:/Program Files (x86)/Microsoft Visual Studio/18/BuildTools/VC/Tools/MSVC/14.50.35717/bin/Hostx64/x64/link.exe"
```

Without this config, `cargo build`, `cargo test`, and `npm run tauri build` will fail at the build-script linking stage. If the MSVC Build Tools version changes, update the path. Use forward slashes (Windows accepts them and they avoid TOML escaping issues).

**Known issue: Rust unit tests fail with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139).** Root cause: VS 18 BuildTools MSVC 14.50.35730 links against a newer VCRUNTIME140.dll than the one installed in System32 (14.50.35719). The Windows loader finds the old System32 DLL first and rejects the binary because a required CRT entry point is missing. Workaround: use `cargo check` for Rust code validation; it catches compile errors without running the binary. Full test runs need either a matching VC++ redistributable update or a clean VM/CI environment.

### 11. Target directory cleanup

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
- `.arena` files are local runtime context mirrors and may contain room/run context, memo text, and recent public previews; they are not shareable artifacts.

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

Detailed plans and review responses are in `docs/`.

## Notes for future edits

- Vite dev server is configured for port `1420`, with HMR on `1421` when `TAURI_DEV_HOST` is set.
- Vite watch ignores backend/build/runtime directories such as `src-tauri`, `.claude`, `.opencovibe`, `memory`, and other non-frontend paths to avoid reload churn during active agent sessions.
- SvelteKit uses `adapter-static` with `fallback: "index.html"`.
- Frontend test environment is `node`, configured in `vitest.config.ts`.
- Provider-native launch config templates are in `src-tauri/src/commands/session.rs` (builder boundary).
- The PTY-based native adapter (`native_pty.rs` + `native_transcript.rs`) is the canonical execution path for Codex/Gemini. Do not reintroduce `codex exec` or pipe-based Gemini execution.
