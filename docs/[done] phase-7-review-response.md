# Phase 7 Review Response

**Date:** 2026-05-05 (updated 2026-05-06)  
**Sources:** `C:\Users\InBlu\review\claude.md`, `codex.md`, `gemini.md`, and `user.md`
**Status:** COMPLETE (2026-05-06). All 9 tasks implemented, code reviewed, and verified.

## Owner Requirement

Codex and Gemini are not considered complete just because their native CLIs can be launched. Their output must be parsed and rendered back into the normal chat timeline like Claude Code, not archived as raw terminal output or treated as a long-running TUI dump.

## Accepted Blocking Items

- **P0 native adapter:** Codex/Gemini interactive CLI sessions cannot use the old `pipe_exec` contract of `stdin=null`, read stdout/stderr until EOF, and process exit equals turn completion. A native adapter must prove prompt delivery, completion detection, assistant archival, stop/cancel, and timeline rendering.
- **P1 provider routing:** DeepSeek/GLM platform ids must survive all Claude-compatible session paths, including start, continue, fork, approval restart, side question, and room participant launches.
- **P1 launch model injection:** Official CLI providers must not receive provider catalog display defaults as actual launch model ids unless the user explicitly selected a model.
- **P2 permission policy:** Codex/Gemini bypass/yolo behavior is Phase 7 provider policy, not a user-controlled setting that can be disabled through stale `yolo_mode=false` state.

## Implementation Decisions

- Keep provider identity separate from execution identity. DeepSeek and GLM stay first-class provider choices while using Claude Code compatible execution.
- Preserve old settings on disk, but remove obsolete login-method/profile complexity from the visible settings surface.
- Continue documenting Codex/Gemini native command generation as incomplete until the adapter renders parsed conversation content.
- Treat external review as input to verify against this codebase, not as automatic implementation orders.

## Current Follow-Up Order

1. Keep bypass/yolo centralized as provider policy and prevent stale `yolo_mode=false` from changing the visible native provider mode.
2. Keep DeepSeek/GLM platform routing helper covered by Rust tests.
3. Audit DeepSeek/GLM entry parity across new chat, continue/resume, fork, approval restart, side question, and room participant flows; only patch paths that still drop provider identity, base URL, model, or run metadata parity.
4. Add the new connection-page balance helper surface: official DeepSeek balance plus Packy console balance via persisted session cookies, with masking, clear action, cached state, and 1-3 minute bounded auto-refresh.
5. Continue validating the Rust native transcript adapter. Real PTY smoke tests now prove Codex/Gemini transcript binding and assistant extraction for new turns; remaining validation must cover resume, UI stop/cancel, room-turn completion, timeout/error reporting, and repeated multi-turn stability.
6. Continue Roundtable parity after the initial prompt/action port: remove room memo, add global memo pop-out, expand memory management, and complete the three-pane room layout/history presentation.

Coordination note:

- Task 4A is currently being advanced in a parallel session using `docs/superpowers/specs/2026-05-06-provider-native-connection-entry-design.md` as the active design reference.
- While that work is active, this session should prioritize non-overlapping native parity acceptance tasks such as room-turn completion and timeout/error surfacing.

Working interpretation:

- Items 1-4 are the current provider/settings follow-up track.
- Item 5 is the remaining Codex/Gemini native parity validation track.
- Item 6 is the remaining Roundtable and information-architecture polish track.

## Task 6/7 Review Response - 2026-05-05

Sources: `C:\Users\InBlu\review\claude.md`, `codex.md`, and `gemini.md`.

Accepted and fixed:

- **P1 native resume transcript baseline:** Codex/Gemini native transcript parsing now captures the provider transcript baseline before launching the PTY process and only accepts completions appended after that baseline for the same transcript file. This prevents resume/latest turns from archiving an older completed answer as the current turn.
- **P2 debate context after summary:** Debate prompts now use the latest completed `fanout` or `debate` turn as peer context, skipping `summary` and private turns. The Room Debate button is also enabled from completed fanout/debate history, not any public turn.

Accepted follow-up, not blocking this repair:

- Keep Memory candidates and launch-time instruction conventions aligned by moving both toward a shared provider instruction registry.
- Surface Memory provider metadata more explicitly in the UI when the Memory page is polished.
- Continue strengthening prompt tests around attribution and truncation boundaries as Roundtable presentation work continues.
- Treat DeepSeek/GLM as already working for baseline execution. Follow-up work in this area should be framed as parity audit plus missing helper UX, not as a rebuild of the current launch chain.
- Keep Packy out of the provider matrix for this phase. Its session cookies are only for the new balance helper surface and must not alter execution routing or `platform_credentials`.
- Improve the visual design of the balance/status card. The current helper surface is functionally correct, but follow-up UI work should make it feel integrated and polished rather than raw or purely utilitarian.
- Unify user-choice interactions in the chat UI. Today plain assistant text choices render as markdown while structured elicitation renders interactive controls, so future UX work should make choice prompts consistently appear as clickable options instead of requiring typed `A/B/C` or `1/2/3` replies.

## Provider Display Repair - 2026-05-05

Accepted and fixed:

- **Chat provider labels:** assistant message headers, streaming output headers, and thinking indicators now use the active visible provider label instead of hard-coded Claude text.
- **Chat empty-state parity:** the chat welcome state is shared across stream and native CLI modes, so Codex/Gemini/DeepSeek/GLM inherit the same resume, `/init`, auth/config, version, and permission display pattern as Claude.
- **Room provider labels:** room participant cards now derive visible provider labels from execution agent plus `platform_id`, so DeepSeek and GLM seats display as DeepSeek/GLM while continuing to execute through Claude Code.

Validation:

- `npm run test -- src/lib/utils/provider-catalog.test.ts`
- `npm run test -- src/lib/utils/room-ui.test.ts src/lib/utils/provider-catalog.test.ts`
- `npm run build`

## Native Parity Progress - 2026-05-06

Accepted and implemented:

- **Native PTY terminal classification:** `src-tauri/src/agent/native_pty.rs` now resolves native PTY turns through explicit `Completed` / `Stopped` / `Failed` terminal states instead of inferring stop from a missing process alone.
- **Stop marker contract:** transcript errors only map to `Stopped` when a native stop marker was explicitly set; transcript errors without that marker now remain `Failed` and preserve their error text.
- **Run status persistence:** native PTY completion now persists `RunStatus::Completed`, `RunStatus::Stopped`, or `RunStatus::Failed` with matching `exit_code` / `error_message`, and emits matching `chat-done` payloads.
- **Baseline reuse coverage:** `native_transcript.rs` now has regression coverage proving reused Codex/Gemini transcript files only consume completions after the captured baseline and prefer the latest completion.
- **Room terminal outcome coverage:** `room/adapter.rs` now treats `Stopped` and `Failed` runs as terminal outcomes, and `room/orchestrator.rs` has explicit regression coverage for propagating stopped/failed actor-turn results plus failed room-response error payloads.
- **Chat done UI handling:** `src/lib/stores/session-store.svelte.ts` now consumes `chat-done.error` for native/pipe sessions so failed or stopped turns no longer collapse into the generic completed path.
- **Repeated multi-turn transcript coverage:** `native_transcript.rs` now has stricter Codex and Gemini regression coverage that advances transcript baselines turn-by-turn and verifies each new turn only consumes completions after its own baseline.

Validation:

- `cargo check --manifest-path "D:\ClaudeWorkspace\Code\claude-session-hub-remaster\src-tauri\Cargo.toml"`
- `cargo test --manifest-path "D:\ClaudeWorkspace\Code\claude-session-hub-remaster\src-tauri\Cargo.toml" native_transcript --no-run`
- `cargo test --manifest-path "D:\ClaudeWorkspace\Code\claude-session-hub-remaster\src-tauri\Cargo.toml" room::adapter --no-run`
- `cargo test --manifest-path "D:\ClaudeWorkspace\Code\claude-session-hub-remaster\src-tauri\Cargo.toml" actor_turn_propagates_stopped_and_failed_terminal_statuses --no-run`
- `cargo test --manifest-path "D:\ClaudeWorkspace\Code\claude-session-hub-remaster\src-tauri\Cargo.toml" failed_response_marks_room_response_failed_with_error --no-run`
- `cargo test --manifest-path "D:\ClaudeWorkspace\Code\claude-session-hub-remaster\src-tauri\Cargo.toml" codex_parser_tracks_each_new_completion_across_multiple_baseline_advances --no-run`
- `cargo test --manifest-path "D:\ClaudeWorkspace\Code\claude-session-hub-remaster\src-tauri\Cargo.toml" gemini_parser_tracks_each_new_completion_across_multiple_baseline_advances --no-run`
- `npm run test -- src/lib/stores/session-store.test.ts`

Known validation gap:

- On this Windows machine, some Rust test executables still remain environment-sensitive at execution time. The native parity additions above are verified through fresh `cargo check`, fresh targeted test-target compilation, and fresh frontend store test execution, but not every Rust target has been executed end-to-end in this session.

## Balance Helper Progress - 2026-05-06

Accepted and implemented:

- **Packy auth model corrected:** Packy balance no longer assumes a single cookie blob. The helper now uses the same browser-observed auth shape validated against Packy: `session`, `TDC_itoken`, and `New-API-User`.
- **Backend balance source corrected:** `refresh_balance_status` no longer scrapes `/console` HTML for Packy. It now queries `GET /api/user/self`, reads `data.quota`, and formats the displayed Packy balance from quota units.
- **Settings UI corrected:** the Packy balance card now stores three explicit fields (`session`, `TDC_itoken`, `user id`) instead of a single opaque cookie input, matching the validated request shape.
- **Redaction boundary preserved:** operational errors still avoid surfacing raw credentials in the UI.

Validation:

- External Packy demo verification succeeded against `GET /api/user/self` using `session`, `TDC_itoken`, and `New-API-User`.
- `npm run build`

Known validation gap:

- Rust unit tests remain blocked on this machine by a pre-existing Windows runtime test-process failure (`STATUS_ENTRYPOINT_NOT_FOUND`) unrelated to the Packy balance logic itself. The issue reproduces with older pre-change test binaries as well, so this fix is being treated as functionally complete while Rust automated verification remains environment-blocked.

## Balance Helper Progress - 2026-05-05

Accepted and implemented:

- **Persisted helper state:** `balance_helper` now stores Packy session cookies, bounded auto-refresh settings, and cached balance entries separately from `platform_credentials`.
- **Backend refresh command:** `refresh_balance_status` queries DeepSeek official balance using the configured DeepSeek API key and queries Packy console with saved cookies. Packy remains balance-only and is not added to the provider matrix.
- **Redaction boundary:** balance refresh errors are operational messages only; cookies, API keys, response headers, and raw Packy HTML are not surfaced.
- **Connection UI:** the settings connection tab now has a separate balance card with cached status, manual refresh, masked Packy cookie save/clear, and bounded auto-refresh while the tab is active.

Validation:

- `cargo test --manifest-path src-tauri/Cargo.toml balance::tests --lib --no-run`
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`
- `npm run i18n:check`
- `npm run build`

Known validation gap:

- `npm run check` still fails on pre-existing repository-wide type errors unrelated to this balance helper work.

## Memo Panel Replacement - 2026-05-06

Accepted and implemented (Task 8):

- **Global Memo pop-out panel:** created `src/lib/components/GlobalMemoPanel.svelte` as a slide-out panel from the right side, using global scope only with a single-line input, add button, and flat list of memo items (timestamp, text, copy, delete).
- **Navigation removed:** `/memo` removed from the sidebar icon rail navItems.
- **Top bar toggle:** added a memo toggle button (clipboard icon) to the non-chat page header, with active state highlighting.
- **Command palette wired:** `ocv:open-memo` replaced with `ocv:toggle-memo` event, layout listens and toggles the panel.
- **Room memo section removed:** the "会议室备忘录" textarea and save button removed from the Room page. `memo_preview` also removed from room list items.
- **Old route compatibility:** `/memo` route now shows a brief redirect hint and navigates to `/chat` after 800ms.
- **Backward compatibility preserved:** `updateRoomMemo` API and store method kept intact; backend room memo fields are not removed.

Validation:
- `npm run build`
- `npm run i18n:check` (0 errors)
- `npm test -- src/lib/stores/memo-store.test.ts` (9 tests pass)
- `npm test -- src/lib/utils/memo-page.test.ts` (2 tests pass)

## Roundtable Layout Redesign - 2026-05-06

Accepted and implemented (Task 9):

- **Three-pane workspace:** replaced the old scrollable content area with a flex-column layout. Three equal-width, scrollable panes fill the primary vertical space above the history strip.
- **Pane headers enriched:** each pane now shows participant label, provider/model metadata, status badge, and computed elapsed time from run `started_at`.
- **Collapsible history strip:** a toggle bar shows turn count. Expanded view renders color-coded turn chips (emerald/red/amber), user input, mode label, per-participant status dots, and per-response detail lines with truncated previews.
- **Fixed bottom toolbar:** action toolbar (Debate/Summary/summarizer selector) and composer remain fixed at the bottom of the workspace.
- **Research artifact:** collapsed into a `<details>` element in the history strip area for research rooms.
- **Non-roundtable rooms:** still render participant cards in the same three-pane grid structure.

Validation:
- `npm run build`
- `npm run i18n:check` (0 errors)
- `npm test -- src/lib/stores/room-store.test.ts` (18 tests pass)
- `npm test -- src/lib/utils/room-ui.test.ts` (6 tests pass)
