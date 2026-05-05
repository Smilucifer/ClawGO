# Phase 7 Native CLI Auth and Settings Simplification Plan

**Feature:** Phase 7 - Native CLI auth, simplified provider settings, and chat-entry parity
**Status:** In progress on 2026-05-05.
**Goal:** Align Claude, Codex, Gemini, DeepSeek, and GLM around the intended auth model while simplifying setup and making Codex/Gemini session entry feel like Claude.
**Acceptance Criteria:**

- Claude, Codex, and Gemini use official CLI authentication only.
- Codex and Gemini native starts do not use `exec`; Codex uses `--dangerously-bypass-approvals-and-sandbox`, Gemini uses yolo approval mode, and Claude Code sessions default to bypass mode.
- DeepSeek requires only an official API key and runs through the Claude Code compatible JSON/env path.
- GLM requires an API key plus configurable base URL and model.
- The connection settings page shows a simple five-provider list and removes saved login method/profile complexity from the visible surface.
- The chat model switcher shows Claude, Codex, Gemini, DeepSeek, and GLM.
- Codex, Gemini, DeepSeek, and GLM new-session pages match Claude's empty-state pattern: resume, `/init` hint, auth/config status, and CLI version when applicable.
- DeepSeek and GLM selections start through a Claude Code session with the corresponding provider configuration.
- The new Roundtable participant AI selector shows Claude, Codex, Gemini, DeepSeek, and GLM.
- The Memory page manages Claude, Codex, and Gemini memory/instruction files, not only Claude files.
- Roundtable creation and turn orchestration reuse the stronger prompt system from `D:\ClaudeWorkspace\Code\claude-session-hub`: base rules, scene preset, room covenant, fanout, debate, and summary prompts.
- The Room toolbar exposes one-click Debate and one-click Summary controls with a summarizer selector.
- Remove the current room memo panel and room-specific memo editor from the Room page.
- Replace the current full Memo page/tab with a simple global pop-out memo panel modeled after `D:\ClaudeWorkspace\Code\claude-session-hub`.
- The Room workspace uses the original hub's three-column roundtable layout: three large live session output panes above, a collapsible turn-history strip below, then the action toolbar and composer.
- Roundtable answer cards adopt the stronger presentation patterns from the original hub: status, model, timing/context metadata, previews, and clearer action affordances.
  **Architecture:** Keep provider setup declarative in settings, but separate official CLI providers from Claude-compatible API providers. Add a native CLI launch path for Codex/Gemini instead of forcing them through pipe `exec`; do not reuse Claude's stream-json actor unless the target CLI protocol is proven compatible.
  **Tech Stack:** SvelteKit frontend, Tauri/Rust command layer, existing run/session stores, CLI diagnostics, and JSON settings migration.
  **Frontend Verification:** Yes - settings, chat empty state, and roundtable cards require visual checks.

---

## Review-Driven Adjustments

External review in `review/claude.md`, `review/codex.md`, and `review/gemini.md` identified the same core risk: Phase 7 is correct in direction, but should not be executed as nine equal tasks in sequence. The plan now treats native CLI protocol work, provider identity, and cross-entry verification as blocking design constraints.

Second-round review in `C:\Users\InBlu\review` added a stricter acceptance point from the project owner: Codex and Gemini must render like Claude Code, meaning CLI output is parsed back into the normal conversation timeline instead of being treated as raw terminal output. Current direct interactive command generation alone does not satisfy that requirement.

Accepted review decisions:

- Add a Codex/Gemini native protocol spike before replacing `exec` in production paths.
- Treat Codex/Gemini native interactive support as a blocking adapter task, not as a pipe-exec flag change. The adapter must prove prompt delivery, turn completion, parsed assistant archival, stop/cancel, and UI timeline rendering comparable to Claude Code before the feature is called complete.
- Keep execution agent and displayed provider identity separate. DeepSeek/GLM should run as Claude Code execution with provider metadata, not as new low-level execution agents unless a later design proves that is necessary.
- Preserve DeepSeek/GLM provider config across every Claude session path: new start, resume/continue, fork, approval-restart, side question, and room participants. Global CLI auth must not silently drop `deepseek` / `zhipu` / `zhipu-intl` platform routing.
- Strengthen bypass/yolo verification and make the elevated default permission mode visible in user-facing status surfaces.
- Model elevated permissions as provider policy for Phase 7 official CLI providers. Do not present `yolo_mode=false` as a user-controllable state when Codex/Gemini are intentionally forced to elevated defaults.
- Separate provider display defaults from actual launch model injection. Claude/Codex/Gemini official CLI providers should not receive catalog display labels as concrete model overrides unless the user explicitly chose a model.
- Keep old settings, connection profiles, room memo fields, and memo route/data backward-compatible even when removed from the main visible UI.
- Split implementation into batches: foundation, chat launch parity, roundtable parity, and memory/memo/UI polish.
- Add prompt snapshot tests and a provider-by-entry verification matrix.

Blocking review findings accepted for this batch:

- **P0:** Codex/Gemini interactive CLI cannot keep using the old `pipe_exec` completion model (`stdin=null`, stdout/stderr until EOF, process exit = turn complete) if the desired UX is Claude-like parsed conversation rendering.
- **P1:** DeepSeek/GLM platform routing must be centralized in Rust helper logic and reused by fork, approval restart, side-question, and any other `resolve_auth_env_for_platform` caller.
- **P1:** Roundtable creation must not inject provider catalog display defaults as actual model ids for official CLI providers.

Documentation updates from this review batch are tracked in `docs/phase-7-review-response.md`. The repository contributor guide is now `AGENTS.md`; Phase 7-specific contributor guidance should link back to this plan and the review response instead of duplicating the full provider matrix.

Deferred review suggestions:

- A Rust trait abstraction for launchers is worth considering during Task 3B, but it should follow the Task 3A protocol decision rather than be designed first.
- A prompt templating crate such as Tera/Handlebars is not required for Phase 7. Prefer typed Rust prompt builders plus snapshot tests unless the prompt files become hard to maintain.

## Decisions

- **Provider auth model:** Claude, Gemini, and Codex are subscription/official CLI providers. DeepSeek and GLM are API providers.
- **No custom keys for Codex/Gemini:** remove API key, base URL, and saved connection profile controls for these two from the settings UI.
- **Execution identity:** `agent` remains the execution engine (`claude`, `codex`, `gemini`). DeepSeek/GLM use Claude execution plus provider metadata such as `platform_id` / provider config for display, env, model, and auth routing.
- **Default permissions:** all Claude Code based sessions default to bypass mode. Codex defaults to `--dangerously-bypass-approvals-and-sandbox`. Gemini defaults to yolo approval mode. The UI must visibly show this elevated default mode.
- **Permission policy semantics:** for Phase 7 official CLI providers, elevated permission mode is provider policy. Legacy `yolo_mode=false` settings must not make the UI imply that Codex/Gemini can run in a lower-permission mode while the backend still forces bypass/yolo.
- **Chat model picker:** the header model dropdown should list Claude, Codex, Gemini, DeepSeek, and GLM. DeepSeek/GLM use the Claude Code session path with their stored Claude-compatible provider config and should persist as Claude execution plus provider identity.
- **Roundtable participant picker:** the new-seat AI dropdown should use the same five provider names: Claude, Codex, Gemini, DeepSeek, and GLM; DeepSeek/GLM participants still use Claude Code execution under the hood.
- **Roundtable prompts:** port the prompt sources from the original hub instead of keeping the current one-line seat prompt. Source files: `core/roundtable-scenes.js` and `core/roundtable-orchestrator.js`.
- **One-click Room actions:** add toolbar buttons for Debate and Summary. Debate reuses the current input as optional extra focus. Summary routes to the selected summarizer.
- **Room layout:** use a dense workspace layout like the original hub: three equal-width session panes, each showing that participant's live/latest output and status; a bottom history strip can expand/collapse and marks which sessions participated in each turn.
- **Memory page:** expose Claude, Codex, and Gemini memory files in the same editor. DeepSeek/GLM inherit Claude Code memory behavior because they run through Claude Code sessions.
- **Memo panel:** Memo is not room state and not a full editor page. It is a global pop-out panel with one input, an add button, and per-item copy/delete actions.
- **Visible settings surface:** show only five rows under "AI 模型":
  - Claude: `订阅模式 · claude-opus-4-7[1m]`, badge `订阅`
  - Gemini: `订阅模式 · gemini-2.5-flash`, badge `订阅`
  - Codex: `订阅模式 · gpt-5.5`, badge `订阅`
  - DeepSeek: `API · 未配置 Key`, badge `缺 Key`
  - GLM: `API · 未配置 Key`, badge `缺 Key`

## Non-Goals

- Do not add saved API-key profiles for Codex or Gemini.
- Do not keep the current complex login-method/profile editor visible on the simplified settings page.
- Do not implement a fake Codex/Gemini stream actor by parsing Claude-specific stream-json.
- Do not require users to store custom API keys for subscription CLIs.
- Do not keep a room-specific memo textarea or save button in `/rooms`.
- Do not keep the current full `/memo` editor experience with scope tabs, selected-item editing, or clear-all controls in the main visible workflow.

## Implementation Plan

### Execution Batches

1. **Foundation:** Task 1, Task 3A, then the provider/config parts of Tasks 2, 4, and 5.
2. **Chat Launch Parity:** complete Task 3B and Task 4 after the native protocol spike has a verified result.
3. **Roundtable Parity:** complete Tasks 5, 6, and 9.
4. **Information Architecture Polish:** complete Tasks 2, 7, and 8 after the run/session model is stable.

### Task 1: Update Provider Settings Schema and Migration

**Files:**

- Modify: `src-tauri/src/models.rs`
- Modify: settings load/save command files if schema migration is centralized elsewhere
- Modify: `src/lib/types.ts`

**Steps:**

1. Add explicit provider modes: `official_cli` for Claude/Gemini/Codex and `claude_compatible_api` for DeepSeek/GLM.
2. Add or formalize provider identity fields so DeepSeek/GLM can display as first-class providers while executing through Claude Code.
3. Preserve existing settings and connection profiles on disk, but stop surfacing obsolete Codex/Gemini connection profile fields.
4. Keep DeepSeek minimal: API key only.
5. Keep GLM configurable: API key, base URL, and model.
6. Add tests for loading older settings without breaking startup.
7. Add RunMeta snapshot tests for DeepSeek/GLM: execution agent, provider id, base URL/model/env, and display identity must be unambiguous.

### Task 2: Simplify Connection Settings UI

**Files:**

- Modify: `src/routes/settings/+page.svelte`
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

**Steps:**

1. Replace the current detailed connection profile editor with the five-row "AI 模型" surface.
2. Show subscription badges for Claude/Gemini/Codex.
3. Show missing-key state for DeepSeek/GLM until configured.
4. Remove visible saved login method and redundant native/API profile controls.
5. Keep controls compact and operational, not a marketing-style page.
6. Add a separate balance/status panel directly below the "AI 模型" card instead of expanding the provider rows themselves.
7. Show DeepSeek balance state in that panel using the configured official DeepSeek API key.
8. Add a Packy balance section that accepts persisted session cookies for `packycode.com` / `packyapi.com`, keeps the value masked by default, and provides explicit save and clear actions.
9. Treat Packy as a balance-only helper surface. Do not add it as a provider row, do not route model execution through it, and do not mix its cookie storage into `platform_credentials`.
10. Reuse cached balance results on page load, then refresh only on user action or on the bounded auto-refresh timer.
11. Keep all balance-related errors operational and redacted: never surface raw cookies, response headers, or full HTML in UI, logs, or event payloads.

### Task 3A: Spike Codex/Gemini Native CLI Protocol

**Files:**

- Create if useful: `src-tauri/src/agent/native_cli_probe.rs`
- Modify tests only if a fixture harness already exists

**Steps:**

1. Determine how Codex accepts a prompt without `exec`: argv, stdin, PTY paste, or another official CLI mode.
2. Determine how Gemini accepts a prompt in yolo mode without relying on one-shot pipe semantics.
3. Define completion detection for each CLI: process exit, stdout marker, PTY prompt state, or structured event.
4. Define archival semantics: assistant text, raw transcript, parsed blocks, and terminal replay.
5. Define stop/cancel behavior for chat and room turns.
6. Verify Windows spawning still avoids `.cmd` window flashes and works with the chosen stdio/PTY strategy.
7. Record the decision in this document before implementing production launch changes.

**Task 3A Decision Log - 2026-05-05**

- Local Codex CLI: `codex-cli 0.128.0`. `codex [OPTIONS] [PROMPT]` starts the native interactive CLI without a subcommand, while `codex resume --last` resumes the most recent interactive session.
- Local Gemini CLI: `0.40.1`. `gemini [query..]` starts interactive mode, `gemini --prompt-interactive <prompt>` executes an initial prompt and stays interactive, and `gemini --resume latest` resumes the most recent project session.
- Production command construction must not use `codex exec`. Codex native starts should use `codex --dangerously-bypass-approvals-and-sandbox --no-alt-screen [PROMPT]`.
- Gemini native starts should use `gemini --approval-mode yolo --prompt-interactive <prompt>` for initial-prompt startup and `gemini --approval-mode yolo --resume latest` for continue.
- Completion detection cannot reuse the old pipe-exec process-exit contract for native interactive sessions because the process intentionally remains alive. Phase 7 should add a native interactive adapter boundary for Codex/Gemini; command generation can be updated first, but chat/room production wiring must not pretend native interactive is the same as one-shot pipe output.
- Windows `.cmd` shim avoidance remains valid through the existing Node-shim resolution path in `agent/stream.rs`; a future native adapter should reuse that resolver or move it behind a shared launcher helper.

**Task 3B Implementation Note - 2026-05-05**

- Codex command generation no longer uses `exec`; normal starts use `codex --dangerously-bypass-approvals-and-sandbox --no-alt-screen [PROMPT]`.
- Gemini command generation no longer uses an `exec` wrapper; normal starts use `gemini --approval-mode yolo --prompt-interactive <prompt>`.
- Resume-last wiring is intentionally narrow until a native interactive adapter exists. Clicking continue for a Codex/Gemini pipe run marks the run as pending native resume; the next user message is sent with `codex resume --last <prompt>` or `gemini --resume latest --prompt-interactive <prompt>`.
- A first Rust native transcript adapter now follows the original hub's key pattern: launch the official interactive CLI, ignore stdout as assistant content, watch the provider-owned transcript file, extract the final assistant message, write it as a normal assistant event, emit a chat delta, and then stop the long-lived CLI process for that turn.
- Codex transcript extraction watches `~/.codex/sessions/<YYYY>/<MM>/<DD>/rollout-*.jsonl`, matches `session_meta.payload.cwd`, waits for `event_msg/task_complete`, and persists a Codex conversation reference from the rollout file.
- Gemini transcript extraction watches `~/.gemini/tmp/*/.project_root` and `chats/session-*.jsonl|json`, matches the project root to the run cwd, and waits for finalized/tokens-complete Gemini output.
- Stop/cancel now short-circuits transcript waiting by observing removal from the process map, so native waits do not hang until timeout after a user stop.
- Real local CLI smoke validation showed Codex native interactive fails under non-TTY stdio with `Error: stdin is not a terminal`, so Codex/Gemini production native execution now routes through a PTY/ConPTY branch instead of the old `stdin=null` pipe branch.
- Real Codex PTY smoke validation passed after auto-confirming the first-run workspace trust prompt: `codex --dangerously-bypass-approvals-and-sandbox --no-alt-screen <prompt>` wrote a matching rollout JSONL and `event_msg/task_complete.payload.last_agent_message` parsed to `OCV_SMOKE_OK`.
- Real Gemini PTY smoke validation passed for transcript binding and completion: `gemini --approval-mode yolo --prompt-interactive <prompt>` wrote a matching project chat JSONL with a `type:"gemini"` row and tokens. The parser was corrected to ignore `type:"user"` rows so only assistant text is archived.
- Task 6/7 review repair added transcript baseline handling for native resume/latest paths. The adapter now records the matched transcript file length before PTY launch and only parses Codex/Gemini completions appended after that point when the same transcript file is reused.
- This adapter is still not accepted as full Claude parity until fake-CLI tests or app-level real CLI validation prove resume behavior, stop/cancel from the UI, room-turn completion, timeout/error reporting, and repeated multi-turn stability across Codex and Gemini.

**Task 4/5 Implementation Note - 2026-05-05**

- Chat and Roundtable provider pickers now expose Claude, Codex, Gemini, DeepSeek, and GLM while preserving execution identity separately from provider identity.
- DeepSeek and GLM map to Claude Code execution with `platform_id` metadata (`deepseek` / `zhipu`) so their API config can be injected even when the global app auth mode remains CLI.
- The chat empty state now shows provider-specific startup status: CLI detection/version and default permission mode for official CLI providers, and API key/model/base URL status for DeepSeek/GLM.
- The settings connection tab now uses the simplified five-row "AI 模型" surface; legacy profile/login-method UI is no longer rendered, while existing settings data remains loadable.
- DeepSeek and GLM are already operational in the simplified settings/chat path. Remaining Phase 7 work in this area should focus on missing entry-path verification, run metadata parity, and the new balance/status helper surface rather than rebuilding the existing launch path.

### Task 3B: Define Native CLI Launch Paths

**Files:**

- Modify: `src-tauri/src/agent/spawn.rs`
- Modify: `src-tauri/src/room/adapter.rs`
- Modify: `src-tauri/src/commands/runs.rs`
- Modify: `src/lib/utils/agent-capabilities.ts`

**Steps:**

1. Implement the launch strategy chosen in Task 3A.
2. Replace Codex `codex exec ...` launch construction with native `codex --dangerously-bypass-approvals-and-sandbox ...` only after prompt input and completion detection are verified.
3. Ensure Gemini native startup uses yolo approval mode without `exec`.
4. Ensure Claude Code sessions default to bypass mode across normal chat, resume/continue/fork, room participants, and Claude-compatible DeepSeek/GLM paths.
5. Introduce a separate capability/adapter branch for native Codex/Gemini interactive behavior instead of pretending they are Claude stream-json sessions.
6. Add tests that assert generated command argv does not contain `exec` for Codex/Gemini.
7. Add tests that assert Codex includes `--dangerously-bypass-approvals-and-sandbox`, Gemini includes yolo approval mode, and Claude-compatible runs use bypass consistently.
8. Add a fake-CLI integration test or equivalent harness proving prompt input, assistant archival, process completion, and stop/cancel behavior.

**Review checkpoint before continuing downstream UI work:**

- Do not claim Codex/Gemini parity with Claude Code until a native adapter or equivalent structured path can replay output as normal chat timeline entries.
- If a native adapter is not ready in this batch, gate or clearly label Codex/Gemini interactive paths so Roundtable and Chat do not rely on process-exit semantics for interactive CLIs.

### Task 4: Add Chat Provider Picker and Empty-State Parity

**Files:**

- Modify: `src/routes/chat/+page.svelte`
- Modify: chat/session store tests if empty-state behavior is covered

**Steps:**

1. Update the chat header model dropdown to show Claude, Codex, Gemini, DeepSeek, and GLM.
2. Route DeepSeek/GLM chat starts through Claude Code sessions using the configured Claude-compatible API env/config.
3. Persist DeepSeek/GLM as Claude execution with provider identity, not as unrelated low-level agent kinds.
4. Reuse the existing per-provider `lastContinuableRun` behavior for Codex/Gemini/DeepSeek/GLM.
5. Show the continue/resume-last-session button on Codex/Gemini/DeepSeek/GLM empty states.
6. Show the `/init` hint for all five providers.
7. Show CLI auth success/failure and CLI version for official CLI providers; show key/config status for DeepSeek/GLM.
8. Show the current default approval mode in the empty state or startup surface.
9. Avoid showing connection profile selectors for Codex/Gemini.

### Task 4A: Audit Claude-Compatible Provider Entry Parity for DeepSeek and GLM

**Files:**

- Modify: `src-tauri/src/commands/session.rs`
- Modify: `src-tauri/src/commands/runs.rs`
- Modify: `src-tauri/src/commands/rooms.rs`
- Modify: `src-tauri/src/storage/runs.rs`
- Modify: `src/routes/chat/+page.svelte`
- Modify tests covering Claude-compatible provider routing and run metadata snapshots

**Steps:**

1. Audit the DeepSeek/GLM entry matrix across new chat, continue/resume, fork, approval restart, side question, and roundtable participant flows.
2. Verify each path preserves Claude execution plus provider identity, including `platform_id`, effective base URL, selected model, and visible provider label.
3. Keep existing helper-based routing if it already works; only patch paths that still bypass the centralized provider-resolution logic.
4. Add or expand Rust tests around `resolve_auth_env_for_platform` callers and run snapshot creation so DeepSeek/GLM identity drift is caught automatically.
5. Add or expand UI/store coverage where needed so DeepSeek/GLM resume/fork surfaces do not collapse back to generic Claude labeling.

### Task 4B: Add DeepSeek and Packy Balance Status Panel

**Files:**

- Modify: `src/routes/settings/+page.svelte`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/api.ts`
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/storage/settings.rs`
- Create if helpful: `src-tauri/src/commands/balance.rs`
- Modify: `src-tauri/src/commands/mod.rs` if command registration is centralized there
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`
- Modify tests covering settings persistence and connection-page rendering

**Steps:**

1. Add a new persisted settings section for balance-helper state instead of overloading `platform_credentials`.
2. Store Packy session cookies in that balance-helper state, keep them masked by default in the UI, and provide explicit save and clear actions.
3. Add a DeepSeek balance command path that uses the configured DeepSeek credential without changing provider execution settings.
4. Add a Packy balance command path that uses the saved session cookies to query `https://www.packyapi.com/console` and extract the visible balance server-side.
5. Keep Packy strictly balance-only: it must not appear as a provider, alter GLM/DeepSeek routing, or inject base URL/model changes anywhere else.
6. Show cached balance state plus last refresh time on page load, then allow both manual refresh and bounded automatic refresh while the connection page is active.
7. Default automatic refresh to a mid-range interval in the requested 1-3 minute window, and prevent overlapping refresh requests for the same source.
8. Redact cookies and raw HTML from logs, diagnostics, and user-facing errors.
9. Add frontend and Rust coverage for missing credentials, expired cookies, parser failure, saved-state masking, clear-cookie behavior, and auto-refresh lifecycle.

**Implementation status - 2026-05-05:**

- Completed the persisted `balance_helper` settings structure and bounded refresh validation.
- Added `refresh_balance_status` for Tauri IPC and web dispatch. DeepSeek uses the configured DeepSeek API key against the official balance endpoint, while Packy uses saved console cookies and remains balance-only.
- Added the connection-page balance card with cached status, manual refresh, masked Packy cookie save/clear, and bounded auto-refresh while the connection tab is active.
- Added Rust parser/redaction coverage and i18n keys for the balance panel.
- Remaining: manual validation with a real Packy session cookie and broader UI lifecycle tests when the existing Svelte type-check baseline is repaired.

### Task Ordering Note

- Tasks 1-4 capture the original Phase 7 foundation and chat-entry parity work.
- Task 3B and the Task 4/5 implementation note record work that has already landed in part, but the acceptance bar for Codex/Gemini native parity remains open.
- Task 4A and Task 4B are follow-up tasks added after review and owner clarification. They extend Phase 7 without replacing the original Task 4 scope.
- Tasks 5-9 remain the main Roundtable, Memory, Memo, and Room presentation backlog.

### Task 5: Update Roundtable Provider Selection

**Files:**

- Modify: `src/routes/rooms/+page.svelte`
- Modify: room store/types if participant agent ids are currently restricted to three providers
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

**Steps:**

1. Update the new-seat AI dropdown to show Claude, Codex, Gemini, DeepSeek, and GLM.
2. Ensure DeepSeek/GLM roundtable participants start through Claude Code sessions with their matching provider configuration.
3. Apply the same default bypass/yolo behavior used by normal chat.
4. Persist DeepSeek/GLM participants with execution and provider identity separated.
5. Add frontend tests or store tests for five-provider participant selection if the current test surface supports it.

### Task 6: Port Roundtable Prompts and One-Click Actions

**Source Reference:**

- Read/copy from: `D:\ClaudeWorkspace\Code\claude-session-hub\core\roundtable-scenes.js`
- Read/copy from: `D:\ClaudeWorkspace\Code\claude-session-hub\core\roundtable-orchestrator.js`
- Read/copy UI behavior from: `D:\ClaudeWorkspace\Code\claude-session-hub\renderer\meeting-room.js`

**Files:**

- Modify: `src-tauri/src/room/orchestrator.rs`
- Create if helpful: `src-tauri/src/room/prompts.rs`
- Modify: `src-tauri/src/commands/rooms.rs`
- Modify: `src/routes/rooms/+page.svelte`
- Modify: `src/lib/stores/room-store.svelte.ts`
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

**Steps:**

1. Replace the current one-line `defaultSeatPrompt` behavior with a shared Roundtable system prompt derived from old `BASE_RULES`.
2. Ship the Phase 7 required prompt core first: `BASE_RULES`, fanout, debate, summary, target exclusion, and toolbar behavior.
3. Preserve scene-style extension points from old `GENERAL_PRESET`, `RESEARCH_PRESET`, and `COVENANT_RESEARCH`, but do not fully expose research scene/covenant UI unless separately scoped.
4. Port the old fanout prompt shape: room/turn header, optional data pack hook, user question, and "answer independently" instruction.
5. Port the old debate prompt shape: optional user focus, peer responses from the previous public turn, target's own previous answer excluded, absent/error participants clearly marked, and long peer outputs truncated.
6. Port the old summary prompt shape: selected summarizer only, final-opinion instruction, conclusion-first format, consensus/disagreement/action sections, and optional recent-turn memory aid where it is safe and testable.
7. Add Room toolbar controls matching the old behavior: a Debate button and a Summary button plus summarizer dropdown.
8. Wire Debate to send the equivalent of `@debate <current composer text>` through the existing room turn path, then clear the composer.
9. Wire Summary to send `@summary @<selected participant>` through the existing room turn path.
10. Disable Debate until there is at least one completed public turn; disable both actions while a room turn is saving/in progress.
11. Add Rust snapshot/structure tests for fanout, debate, summary, target exclusion, long-output truncation, and absent/error markers.
12. Add frontend/store tests for the toolbar actions if the current test harness supports it.

**Task 6 Implementation Note - 2026-05-05**

- Added RoomStore helpers for one-click actions: Debate sends `@debate <composer focus>` and Summary sends `@summary @<participant-id>`.
- Added the Room toolbar controls for Debate, Summary, and summarizer selection. Debate is disabled until at least one completed Fanout/Debate turn exists; both controls are disabled while the room is saving.
- Replaced the roundtable fanout prompt body with the old hub's structured shape: room/turn header, optional data pack hook, user question, and independent-answer instruction.
- Strengthened debate prompts to use the previous public turn only, exclude the target participant's own prior answer, mark absent/error peers explicitly, and truncate long peer outputs with the old hub middle-ellipsis style.
- Strengthened summary prompts to request conclusion-first output with consensus/disagreement/action sections while preserving public room history.
- Added targeted frontend store tests for Debate/Summary command payloads and Rust structure tests for fanout/debate/summary prompt content.
- Task 6/7 review repair tightened Debate context: after a Summary turn, Debate now skips the Summary and uses the latest completed Fanout/Debate turn. The frontend Debate enabled state follows the same rule.

### Task 7: Expand Memory Page to Codex and Gemini

**Files:**

- Modify: `src-tauri/src/commands/files.rs`
- Modify: `src-tauri/src/models.rs` if memory candidates need an agent/provider field
- Modify: `src/lib/types.ts`
- Modify: `src/routes/+layout.svelte`
- Modify: `src/routes/memory/+page.svelte`
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

**Steps:**

1. Extend `list_memory_files` beyond Claude-only candidates.
2. Include Claude files such as `CLAUDE.md`, `.claude/CLAUDE.md`, `CLAUDE.local.md`, and `.claude/CLAUDE.local.md`.
3. Define the exact Codex instruction file scopes that the app launch path actually reads before showing them as effective memory files.
4. Include Codex instruction files such as `AGENTS.md` only at scopes used by the app.
5. Define the exact Gemini instruction file scopes that the app launch path actually reads before showing them as effective memory files.
6. Include Gemini instruction files such as `GEMINI.md` and any local variants only after the launch convention is verified.
7. Add provider labels or grouping so the sidebar clearly separates Claude, Codex, and Gemini memory.
8. Keep create/edit/save behavior shared; only the candidate list and labels differ by provider.
9. Add tests for mixed Claude/Codex/Gemini candidates and for hiding missing files unless selected.

**Task 7 Implementation Note - 2026-05-05**

- Extended memory candidates with provider metadata and provider-prefixed labels.
- Added effective project instruction candidates for Codex (`AGENTS.md`) and Gemini (`GEMINI.md`) alongside existing Claude candidates.
- Added global official CLI instruction candidates for Claude (`~/.claude/CLAUDE.md`, `~/.claude/CLAUDE.local.md`), Codex (`~/.codex/AGENTS.md`), and Gemini (`~/.gemini/GEMINI.md`).
- Updated file validation so the shared memory editor can create/save files under `~/.codex` and `~/.gemini`, matching the visible global candidates.
- Added Rust coverage for mixed Claude/Codex/Gemini candidate listing and provider metadata.

### Task 8: Replace Memo Page and Room Memo with Global Pop-Out Panel

**Source Reference:**

- Read/copy behavior from: `D:\ClaudeWorkspace\Code\claude-session-hub\renderer\index.html`
- Read/copy behavior from: `D:\ClaudeWorkspace\Code\claude-session-hub\renderer\renderer.js`
- Read/copy styling from: `D:\ClaudeWorkspace\Code\claude-session-hub\renderer\styles.css`

**Files:**

- Modify: `src/routes/+layout.svelte`
- Modify: `src/routes/rooms/+page.svelte`
- Modify: `src/lib/stores/memo-store.svelte.ts`
- Modify: `src/lib/stores/room-store.svelte.ts`
- Modify: `src/lib/api.ts`
- Modify: `src-tauri/src/commands/rooms.rs`
- Modify: `src-tauri/src/storage/rooms.rs`
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`
- Delete or retire from navigation: `src/routes/memo/+page.svelte`

**Steps:**

1. Remove the visible `/memo` navigation tab and route users to a global memo toggle button instead.
2. Add a global memo panel that can pop out or hide from any primary workspace, including chat and rooms.
3. Keep the panel intentionally small: title, close button, single-line "输入想法..." field, plus add button.
4. Render memo items as a flat global list with timestamp, text, copy button, and delete button only.
5. Persist global memos through the existing memo storage command layer if possible; otherwise add a narrow global-only compatibility wrapper.
6. Remove scope tabs, selected-item editing, clear-all, project-specific memo scope, and unsaved-state workflows from the visible UI.
7. Remove the Room page "会议室备忘录" section, its textarea, and save button.
8. Stop showing `memo_preview` in the room list and stop using room memo text as the user-editable meeting note surface.
9. Keep backend room memo fields backward-compatible for old data; do not remove deserialization support or break old room files.
10. Keep `/memo` route compatibility by redirecting, rendering the pop-out shell, or showing a clear lightweight fallback instead of a broken route.
11. Consider one-time import of old room memos into the global memo list, or expose a documented recovery path for old room memo data.
12. Add tests for add/copy/delete UI behavior and for room pages no longer rendering room memo controls.

### Task 9: Improve Roundtable Answer Presentation

**Files:**

- Modify: `src/routes/rooms/+page.svelte`
- Modify: room store/types only if extra metadata is missing

**Steps:**

1. Replace the current room content arrangement with a three-pane roundtable workspace inspired by the original hub screenshot.
2. Put the three participant panes in the primary vertical space above the toolbar. Each pane should be scrollable and stable, not a small preview card.
3. In each pane header, show participant name/provider, status, model, elapsed time, context/tokens when available, and action/escape controls.
4. Render live or latest participant output inside each pane, including thinking/tool/status snippets when available.
5. Add a bottom turn-history strip below the panes and above the action toolbar.
6. Make the history strip toggleable/collapsible so it can show only compact turn chips or expand to show per-turn details.
7. For every history turn, mark which sessions participated and each participant's terminal state: completed, in progress, skipped, absent, or errored.
8. Keep the action toolbar and composer fixed at the bottom of the Room workspace: dispatch mode, Debate, Summary, summarizer selector, role/driver state if still relevant, processing state, and send input.
9. Remove the room memo block from this layout entirely; global memo is handled by Task 8.
10. Keep public/private turn behavior unchanged unless metadata support requires a small type extension.

## Verification

- Rust unit tests for settings migration, command argv generation, capability selection, and Claude-compatible provider routing snapshots.
- Frontend tests for settings row rendering, balance panel state transitions, and chat empty-state parity.
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `npm run lint`
- `npm run check`
- `npm run i18n:check`
- `npm run build`
- Playwright/browser screenshots for settings, chat empty states, roundtable cards, and the new balance/status panel.
- Manual validation of automatic balance refresh timing, page-leave cleanup, and refresh-button reset behavior.

### Provider Entry Matrix

| Provider | Entry                  | Expected execution                               | Expected auth/config status | Expected permission mode                     |
| -------- | ---------------------- | ------------------------------------------------ | --------------------------- | -------------------------------------------- |
| Claude   | New chat               | Claude Code session                              | CLI auth + CLI version      | bypass                                       |
| Claude   | Resume / continue last | Claude Code session                              | CLI auth + CLI version      | bypass                                       |
| Claude   | Roundtable participant | Claude Code session                              | CLI auth + CLI version      | bypass                                       |
| Codex    | New chat               | Native Codex path from Task 3A/3B                | CLI auth + CLI version      | `--dangerously-bypass-approvals-and-sandbox` |
| Codex    | Resume / continue last | Native Codex path from Task 3A/3B                | CLI auth + CLI version      | `--dangerously-bypass-approvals-and-sandbox` |
| Codex    | Roundtable participant | Native Codex path from Task 3A/3B                | CLI auth + CLI version      | `--dangerously-bypass-approvals-and-sandbox` |
| Gemini   | New chat               | Native Gemini path from Task 3A/3B               | CLI auth + CLI version      | yolo                                         |
| Gemini   | Resume / continue last | Native Gemini path from Task 3A/3B               | CLI auth + CLI version      | yolo                                         |
| Gemini   | Roundtable participant | Native Gemini path from Task 3A/3B               | CLI auth + CLI version      | yolo                                         |
| DeepSeek | New chat               | Claude Code execution + DeepSeek provider config | API key present/missing     | bypass                                       |
| DeepSeek | Resume / continue last | Claude Code execution + DeepSeek provider config | API key present/missing     | bypass                                       |
| DeepSeek | Roundtable participant | Claude Code execution + DeepSeek provider config | API key present/missing     | bypass                                       |
| GLM      | New chat               | Claude Code execution + GLM provider config      | key/base URL/model status   | bypass                                       |
| GLM      | Resume / continue last | Claude Code execution + GLM provider config      | key/base URL/model status   | bypass                                       |
| GLM      | Roundtable participant | Claude Code execution + GLM provider config      | key/base URL/model status   | bypass                                       |

### Failure-State Checks

- Official CLI missing.
- Official CLI installed but not logged in.
- DeepSeek key missing.
- DeepSeek balance query unavailable, unauthorized, or rate-limited.
- Packy session cookies missing, expired, rejected, or no longer parseable from the console page.
- GLM key missing, base URL missing/invalid, or model missing.
- Codex/Gemini native protocol cannot determine completion.
- Room Debate requested before any public turn.
- Room Summary target missing, inactive, errored, or lacking relevant public history.
- Automatic balance refresh fires after the page is inactive or starts overlapping requests.
- Old settings, old connection profiles, old `/memo` links, and old room memo data still load without crashing.

## Open Technical Risks

- Codex/Gemini native interactive protocols may not expose the same event model as Claude stream-json. If they cannot support long-lived structured sessions cleanly, the implementation should add an explicit native PTY/stdio adapter and document the behavioral difference.
- Existing stored connection profiles should remain backward-compatible on disk, but the simplified UI should not encourage editing obsolete Codex/Gemini API profile data.
- Choice-prompt UX is still split between plain markdown messages and structured elicitation flows. Future UI work should unify these paths so user-facing multiple-choice questions consistently render as clickable options instead of sometimes requiring typed answers.
