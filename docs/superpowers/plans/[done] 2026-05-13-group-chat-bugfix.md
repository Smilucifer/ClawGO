# Plan: Group Chat Bug Fixes + Enhancement (Phase 10 Post-Merge)

## Context

7 issues reported after Phase 10 Group Chat refactor merge. All exploration complete, root causes identified.

---

## Issue 1: Group Chat Delete Button

**Root cause**: Full stack exists (backend `delete_group_chat` at `commands/group_chat.rs:472`, API `deleteGroupChat` at `api.ts:244`, store `deleteRoom` at `group-chat-store.svelte.ts:359`). Missing: UI delete button in sidebar.

**Fix**: Add × delete button to each group chat entry in `+layout.svelte` sidebar (near line 2443). Button appears on hover. Calls `groupChatStore.deleteRoom(chat.id)`. Use a simple SVG × icon button matching the app's existing sidebar patterns.

**Files**:
- `src/routes/+layout.svelte` — add delete button to sidebar group chat entries (~line 2443)
- Add `groupChatStore` import if not already accessible

---

## Issue 2: Filter Tools & Thinking Chain from Output

**Root cause**: 
- Actor path: Session is started with `mode: "plan"` (commands/group_chat.rs:299) but `stream_message` (adapter.rs:286) doesn't enforce per-message permission_mode. The plan mode at session creation time should restrict tools but may not suppress thinking output.
- Preview extraction: `run_preview` (orchestrator.rs:1080-1170) Pass 1 grabs any `Assistant` event with `text` payload, including thinking chains.

**Fix**:
1. In `execute_actor_turn` (orchestrator.rs:410): Before `adapter.stream_message()`, set run's `permission_mode` to `"plan"` to ensure tools remain disabled.
2. In `run_preview` Pass 1 (orchestrator.rs:1093-1110): Filter out assistant events where the `subtype` is `"thinking"` or `"tool_use"`. Only use events with subtype `"text"` or no subtype.
3. In `run_preview` Pass 2 (orchestrator.rs:1115-1150): Similarly filter bus envelope events — skip `thinking_delta` events, only use `message_delta`/`message_complete` with text.

**Files**:
- `src-tauri/src/group_chat/orchestrator.rs` — fix permission_mode in actor path (~line 410), filter thinking from `run_preview` (~line 1080-1170)

---

## Issue 3 & 5: Navigation Fix — Always Use groupChatId

**Root cause**: `navigateToGroupChat()` in `+layout.svelte:551-559` routes to `/chat?run=<runId>` when participants exist, causing the regular chat page to render the first participant's private chat instead of GroupChatLayout. Same bug in `submitGroupChatCreate()` at line 538-543.

**Fix**: Always navigate to `/chat?groupChatId=<chat.id>`. The chat page (`chat/+page.svelte:1141-1144`) already detects `groupChatId` param and renders `GroupChatLayout`. Remove the `runIds` branch entirely.

**Files**:
- `src/routes/+layout.svelte` — simplify `navigateToGroupChat()` and `submitGroupChatCreate()` to always use `groupChatId`

---

## Issue 4: Participant Message Dispatch

**Root cause**: Consequence of Issue 3. When GroupChatLayout isn't rendered (because navigation went to `/chat?run=...`), participants don't receive their prompts visually. After fixing Issue 3, GroupChatLayout renders correctly.

The `handleSend` function in GroupChatLayout.svelte:245 already updates `detail` with the server response. The participant side panel reads `detail.participants` and shows status dots. The fanout prompt is correctly built by `build_fanout_prompt` (orchestrator.rs:789) and dispatched to all active participants.

**No additional changes needed** beyond Issue 3 fix.

---

## Issue 6: Settings Characters Dropdown (White-on-White)

**Root cause**: Both `<select>` elements in `characters/+page.svelte` use `bg-transparent` without `text-foreground`. Every other `<select>` in the codebase uses `bg-background text-foreground`.

**Fix**: Change `bg-transparent` → `bg-background text-foreground` on both selects (role_type at ~line 276, default_provider at ~line 292).

**Files**:
- `src/routes/settings/characters/+page.svelte` — fix both `<select>` class attributes

---

## Issue 7: Default Planner & Executor Prompts

**Design**: 按职责分层 (layered by responsibility).

### Planner prompt:
```
You are a strategic planner in a multi-agent group chat. Your responsibilities:

1. TASK DECOMPOSITION: Break complex user requests into concrete, ordered sub-tasks.
2. CONTEXT ANALYSIS: Read relevant project files to understand the codebase before planning.
3. COORDINATION: Assign tasks to appropriate participants. Use @mentions to route subtasks.
4. PLAN OUTPUT: Produce a numbered checklist with clear success criteria for each item.

CONSTRAINTS:
- You can READ files and search code, but you CANNOT modify the filesystem, run commands, or execute tools that change state.
- Do NOT implement — only plan. The executors will carry out your instructions.
- When uncertain about the codebase, request more context rather than guessing.
```

### Executor prompt:
```
You are a task executor in a multi-agent group chat. Your responsibilities:

1. FOLLOW THE PLAN: Execute only the tasks assigned to you. Do not deviate from the plan.
2. REPORT PROGRESS: Clearly state which task you completed and what the result was.
3. ASK FOR CLARIFICATION: If a task is ambiguous, ask the planner for clarification before executing.
4. SIGNAL COMPLETION: End your response with a brief summary of what was accomplished.

CONSTRAINTS:
- Stay within the scope of your assigned task. Do not expand or reinterpret the plan.
- If you encounter an obstacle, report it rather than working around it silently.
- Coordinate with other executors — reference their outputs when relevant.
```

### Implementation:
Update `build_role_system_prompt` in `orchestrator.rs:587-597` to use these layered prompts. Custom `role_instruction` is appended after the base prompt. Apply the system prompt in BOTH Actor and Pipe execution paths:
- Pipe path: already applied at line 491 via `adapter_settings.append_system_prompt`
- Actor path: inject via `adapter.inject_prompt()` before `adapter.stream_message()` (line 423)

**Files**:
- `src-tauri/src/group_chat/orchestrator.rs` — update `build_role_system_prompt`, add prompt injection to `execute_actor_turn`
- `src-tauri/src/group_chat/adapter.rs` — wire `inject_prompt` for Actor path (currently returns Err at line 307-311)

---

## Verification

1. `cargo check --manifest-path src-tauri/Cargo.toml` — Rust compiles
2. `cargo test --manifest-path src-tauri/Cargo.toml group_chat` — existing tests pass
3. `npm run build` — frontend builds
4. `npm run i18n:check` — i18n keys valid
5. Manual: Delete group chat from sidebar → disappears from list, runs soft-deleted
6. Manual: Send message in group chat → participants respond, no tools/thinking in output
7. Manual: Click group chat in sidebar → GroupChatLayout opens (not private chat)
8. Manual: Settings → Characters → dropdown options readable
9. Manual: Create Planner character → correct system prompt injected in group chat turns
