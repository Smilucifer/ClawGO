# Phase 7 Review Response

**Date:** 2026-05-05  
**Sources:** `C:\Users\InBlu\review\claude.md`, `codex.md`, `gemini.md`, and `user.md`

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
5. Continue validating the Rust native transcript adapter. Real PTY smoke tests now prove Codex/Gemini transcript binding and assistant extraction for new turns; remaining validation must cover resume, UI stop/cancel, room-turn completion, and repeated multi-turn stability.
6. Continue Roundtable parity after the initial prompt/action port: remove room memo, add global memo pop-out, expand memory management, and complete the three-pane room layout/history presentation.

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
