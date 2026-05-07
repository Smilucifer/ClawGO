# Phase 8: Roundtable Enhancements + Gemini Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove Gemini support, add roundtable UX improvements (stepper, @name, sidebar grouping, prompt constraint), and verify context event display across all session types.

**Architecture:** Six independent changes ordered by dependency. Gemini removal is the foundation (removes dead code paths). Prompt constraint and @Name are small backend+frontend changes. Room grouping adds a new Tauri command + frontend sidebar logic. Stepper replaces the History strip with a replay-capable component. Context verification is a diagnostic pass.

**Tech Stack:** Svelte 5 (runes), Rust (Tauri commands), Vitest, cargo check

**Spec:** `docs/superpowers/specs/2026-05-07-roundtable-phase8-design.md`

---

## File Map

### Task 1: Gemini Removal — Frontend Types & Provider Catalog
- Modify: `src/lib/types.ts` (lines 5, 82, 422, 1571)
- Modify: `src/lib/utils/provider-catalog.ts` (lines 2, 3-11, 44-52, 114-122)
- Modify: `src/lib/utils/agent-capabilities.ts`
- Modify: `src/lib/utils/agent-features.ts`
- Modify: `src/lib/utils/native-permission.ts`
- Modify: `src/lib/utils/continuable-run.ts`
- Modify: `src/lib/commands.ts`
- Modify: `src/lib/api.ts`

### Task 2: Gemini Removal — Frontend UI & Stores
- Modify: `src/routes/chat/+page.svelte`
- Modify: `src/routes/settings/+page.svelte`
- Modify: `src/routes/rooms/+page.svelte`
- Modify: `src/routes/plugins/+page.svelte`
- Modify: `src/lib/components/McpConfiguredPanel.svelte`
- Modify: `src/lib/components/McpDiscoverPanel.svelte`
- Modify: `src/lib/stores/agent-settings-cache.svelte.ts`
- Modify: `src/lib/stores/session-store.svelte.ts`

### Task 3: Gemini Removal — Rust Backend
- Modify: `src-tauri/src/agent/spawn.rs`
- Modify: `src-tauri/src/agent/stream.rs`
- Modify: `src-tauri/src/agent/native_transcript.rs`
- Modify: `src-tauri/src/agent/native_pty.rs`
- Modify: `src-tauri/src/agent/adapter.rs`
- Modify: `src-tauri/src/room/adapter.rs`
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/storage/managed_apps.rs`
- Modify: `src-tauri/src/storage/mcp_registry.rs`
- Modify: `src-tauri/src/commands/files.rs`
- Modify: `src-tauri/src/commands/rooms.rs`
- Modify: `src-tauri/src/commands/runs.rs`
- Modify: `src-tauri/src/commands/diagnostics.rs`

### Task 4: Gemini Removal — Tests
- Modify: Multiple `*.test.ts` files
- Modify: Multiple Rust test modules

### Task 5: Seat Prompt Constraint
- Modify: `src/routes/rooms/+page.svelte` (line 342)

### Task 6: @Name SingleTarget
- Modify: `src-tauri/src/room/models.rs`
- Modify: `src-tauri/src/room/orchestrator.rs`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/utils/room-ui.ts`
- Modify: `messages/en.json`, `messages/zh-CN.json`

### Task 7: Room Session Sidebar Grouping
- Create: `src-tauri/src/commands/rooms.rs` (new command)
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/api.ts`
- Modify: `src/lib/utils/sidebar-groups.ts`
- Modify: Sidebar host component
- Modify: `src/lib/components/ProjectFolderItem.svelte`
- Modify: `messages/en.json`, `messages/zh-CN.json`

### Task 8: Stepper Mini-Map
- Create: `src/lib/components/RoomStepper.svelte`
- Modify: `src-tauri/src/commands/rooms.rs` (new command)
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/routes/rooms/+page.svelte`
- Modify: `src/lib/stores/room-store.svelte.ts`
- Modify: `src/lib/types.ts`
- Modify: `messages/en.json`, `messages/zh-CN.json`

### Task 9: Context Events Verification
- Verify: `src-tauri/src/agent/stream.rs`
- Verify: `src-tauri/src/agent/native_transcript.rs`
- Verify: `src/routes/chat/+page.svelte`

---

## Task 1: Gemini Removal — Frontend Types & Provider Catalog

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/utils/provider-catalog.ts`
- Modify: `src/lib/utils/agent-capabilities.ts`
- Modify: `src/lib/utils/agent-features.ts`
- Modify: `src/lib/utils/native-permission.ts`
- Modify: `src/lib/utils/continuable-run.ts`
- Modify: `src/lib/commands.ts`
- Modify: `src/lib/api.ts`

- [ ] **Step 1: Remove gemini from `src/lib/types.ts`**

Remove `"gemini"` from all type unions. Keep `"unknown"` as the fallback for unrecognized agents.

```typescript
// Line 5 — was: provider?: "claude" | "codex" | "gemini";
provider?: "claude" | "codex";

// Line 82 — was: export type AgentKind = "claude" | "codex" | "gemini" | "unknown";
export type AgentKind = "claude" | "codex" | "unknown";

// Line 422 — was: agent: "claude" | "codex" | "gemini";
agent: "claude" | "codex";

// Line 1571 — was: agent?: "claude" | "codex" | "gemini";
agent?: "claude" | "codex";
```

- [ ] **Step 2: Remove gemini from `src/lib/utils/provider-catalog.ts`**

```typescript
// Line 2 — was: export type ExecutionAgent = "claude" | "codex" | "gemini";
export type ExecutionAgent = "claude" | "codex";

// Lines 3-11 — remove "gemini" from Phase7ProviderId
export type Phase7ProviderId =
  | "claude"
  | "codex"
  | "deepseek"
  | "glm"
  | "qwen"
  | "kimi"
  | "mimo-pro";

// Lines 44-52 — delete the entire Gemini provider entry from PHASE7_PROVIDERS array

// Lines 114-122 — remove gemini branch from providerIdForRun()
export function providerIdForRun(agent: string, platformId?: string | null): Phase7ProviderId {
  if (platformId === "deepseek") return "deepseek";
  if (platformId === "zhipu" || platformId === "zhipu-intl") return "glm";
  if (platformId === "bailian") return "qwen";
  if (platformId === "kimi") return "kimi";
  if (platformId === "mimo-pro") return "mimo-pro";
  if (agent === "codex") return "codex";
  return "claude";
}
```

- [ ] **Step 3: Remove gemini from `src/lib/utils/agent-capabilities.ts`**

Remove the `"gemini"` branch in `normalizeAgentKind()` — it should fall through to `"unknown"`. Remove `kind === "gemini"` from the `pipe_exec` capability check in the fallback branch.

- [ ] **Step 4: Remove gemini from `src/lib/utils/agent-features.ts`**

Delete the `"gemini"` entry from `FEATURES_MAP`.

- [ ] **Step 5: Remove gemini from `src/lib/utils/native-permission.ts`**

Remove `"gemini"` from the `NATIVE_AGENTS` set.

- [ ] **Step 6: Remove gemini from `src/lib/utils/continuable-run.ts`**

Remove the `if (normalizedProvider === "gemini") return run.agent === "gemini";` branch.

- [ ] **Step 7: Remove gemini from `src/lib/commands.ts`**

Remove `"gemini"` from `CommandAgent` type. Delete the `new-gemini` command palette entry.

- [ ] **Step 8: Remove gemini from `src/lib/api.ts`**

Remove `"gemini"` from `ManagedCliApp` type and any agent type parameters.

- [ ] **Step 9: Run frontend type check**

Run: `npm run check`
Expected: No type errors from removed gemini references.

- [ ] **Step 10: Commit**

```bash
git add src/lib/types.ts src/lib/utils/provider-catalog.ts src/lib/utils/agent-capabilities.ts src/lib/utils/agent-features.ts src/lib/utils/native-permission.ts src/lib/utils/continuable-run.ts src/lib/commands.ts src/lib/api.ts
git commit -m "feat: remove Gemini from frontend types and provider catalog"
```

---

## Task 2: Gemini Removal — Frontend UI & Stores

**Files:**
- Modify: `src/routes/chat/+page.svelte`
- Modify: `src/routes/settings/+page.svelte`
- Modify: `src/routes/rooms/+page.svelte`
- Modify: `src/routes/plugins/+page.svelte`
- Modify: `src/lib/components/McpConfiguredPanel.svelte`
- Modify: `src/lib/components/McpDiscoverPanel.svelte`
- Modify: `src/lib/stores/agent-settings-cache.svelte.ts`
- Modify: `src/lib/stores/session-store.svelte.ts`

- [ ] **Step 1: Clean `src/routes/chat/+page.svelte`**

Remove `"gemini"` from:
- `CHAT_AGENTS` set (line 46)
- `startupCliChecks` state (lines 151-154)
- Startup CLI check logic (lines 276-285)
- URL query parameter handling `?agent=gemini` (line 1075)

- [ ] **Step 2: Clean `src/routes/settings/+page.svelte`**

Remove `"gemini"` from:
- `ConnectionAgentTab` type (line 93)
- Connection profile tab entry (line 97)
- CLI check state (line 102)

- [ ] **Step 3: Clean `src/routes/rooms/+page.svelte`**

Remove default Gemini participant slot:
- Line 262: default `agent: "gemini"` — change to `"codex"` or remove the third default slot
- Line 265: `label: "Gemini"` — update accordingly
- Line 268: `defaultSeatPrompt(2, "gemini")` — update accordingly
- Line 306: `profile?.agent === "gemini"` detection — remove branch

- [ ] **Step 4: Clean `src/routes/plugins/+page.svelte`**

Remove `"gemini"` from `ManagedApp` type (line 50). Delete the Gemini managed app entry (lines 73-77).

- [ ] **Step 5: Clean MCP panel components**

Remove `"gemini"` from `app` prop type in both:
- `src/lib/components/McpConfiguredPanel.svelte` (line 19)
- `src/lib/components/McpDiscoverPanel.svelte` (line 24)

- [ ] **Step 6: Clean `src/lib/stores/agent-settings-cache.svelte.ts`**

Remove Gemini settings loading (lines 32-40).

- [ ] **Step 7: Clean `src/lib/stores/session-store.svelte.ts`**

Remove Gemini-specific resume logic (`_pendingNativeResumeLatest` for gemini, line 2093) and comments (line 1612).

- [ ] **Step 8: Run frontend check**

Run: `npm run check`
Expected: No type errors.

- [ ] **Step 9: Commit**

```bash
git add src/routes/ src/lib/components/ src/lib/stores/
git commit -m "feat: remove Gemini from frontend UI, routes, and stores"
```

---

## Task 3: Gemini Removal — Rust Backend

**Files:**
- Modify: `src-tauri/src/agent/spawn.rs`
- Modify: `src-tauri/src/agent/stream.rs`
- Modify: `src-tauri/src/agent/native_transcript.rs`
- Modify: `src-tauri/src/agent/native_pty.rs`
- Modify: `src-tauri/src/agent/adapter.rs`
- Modify: `src-tauri/src/room/adapter.rs`
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/storage/managed_apps.rs`
- Modify: `src-tauri/src/storage/mcp_registry.rs`
- Modify: `src-tauri/src/commands/files.rs`
- Modify: `src-tauri/src/commands/rooms.rs`
- Modify: `src-tauri/src/commands/runs.rs`
- Modify: `src-tauri/src/commands/diagnostics.rs`

- [ ] **Step 1: Clean `src-tauri/src/agent/spawn.rs`**

Delete `build_gemini_base_args()` (lines 45-73). Remove the `"gemini"` match arm from `build_agent_command()` (lines 141-149). Remove the `"gemini"` match arm from `build_agent_resume_command()` (lines 172-181). Remove all Gemini tests (lines 289-303, 335-348, 364-373).

- [ ] **Step 2: Clean `src-tauri/src/agent/stream.rs`**

In the `native_transcript_mode` check (line 137), change `agent == "codex" || agent == "gemini"` to just `agent == "codex"`. Remove Gemini from `resolve_windows_npm_shim()` if it handles gemini.cmd. Remove Gemini test references.

- [ ] **Step 3: Clean `src-tauri/src/agent/native_transcript.rs`**

Delete all Gemini functions: `find_gemini_session()`, `find_latest_gemini_session()`, `parse_gemini_turn()`, `parse_gemini_turn_after()`, `wait_for_gemini_turn()`. Remove Gemini branches in `capture_native_transcript_baseline()`, `wait_for_native_transcript_turn()`, `try_native_transcript_once()`. Remove all Gemini tests (lines 600-715).

- [ ] **Step 4: Clean `src-tauri/src/agent/native_pty.rs`**

Remove Gemini comments (line 438-439) and Gemini test references (lines 127-141).

- [ ] **Step 5: Clean `src-tauri/src/agent/adapter.rs`**

Remove `"gemini"` match arms from `default_api_key_env()` (lines 200-206) and `default_base_url_env()` (lines 208-214).

- [ ] **Step 6: Clean `src-tauri/src/room/adapter.rs`**

Remove `AgentKind::Gemini` variant from enum (line 37). Remove `"gemini"` match arm from `from_agent()` (line 47). Merge `AgentKind::Gemini` capabilities into the `AgentKind::Unknown` fallback branch (remove `pipe_exec: matches!(kind, AgentKind::Gemini)`, set `pipe_exec: false`). Remove Gemini test (lines 398-403).

- [ ] **Step 7: Clean `src-tauri/src/models.rs`**

Remove `"gemini"` agent settings from `AllSettings::default()` (line 595).

- [ ] **Step 8: Clean `src-tauri/src/storage/managed_apps.rs`**

Remove `ManagedCliApp::Gemini` variant (line 7). Remove its match arms (lines 15-17, 27, 35).

- [ ] **Step 9: Clean `src-tauri/src/storage/mcp_registry.rs`**

Delete all Gemini functions: `list_configured_gemini()`, `gemini_settings_path()`, `read_gemini_settings()`, `write_gemini_settings()`, `write_gemini_mcp_server()`, `remove_gemini_mcp_server()`, `toggle_gemini_server()`. Remove `ManagedCliApp::Gemini` dispatch (line 259). Remove all Gemini error message references.

- [ ] **Step 10: Clean `src-tauri/src/commands/files.rs`**

Remove `~/.gemini/` from protected config directories (line 26, 96-109). Remove Gemini memory file candidate (lines 379-381, 404-407). Remove Gemini tests (lines 574, 586).

- [ ] **Step 11: Clean `src-tauri/src/commands/rooms.rs`**

Remove `"gemini"` from `normalize_agent()` (line 315) — change to `"claude" | "codex"`. Update error message (line 317). Remove Gemini from `default_participant_label()` (line 336).

- [ ] **Step 12: Clean `src-tauri/src/commands/runs.rs`**

Remove Gemini test cases (lines 311-312, 326).

- [ ] **Step 13: Clean `src-tauri/src/commands/diagnostics.rs`**

Remove `"gemini"` CLI binary mapping (line 19).

- [ ] **Step 14: Run Rust checks**

Run: `cargo check --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: Clean compile, no warnings.

- [ ] **Step 15: Commit**

```bash
git add src-tauri/
git commit -m "feat: remove Gemini from Rust backend — agent, storage, commands"
```

---

## Task 4: Gemini Removal — Tests

**Files:**
- Modify: `src/lib/utils/provider-catalog.test.ts`
- Modify: `src/lib/utils/agent-capabilities.test.ts`
- Modify: `src/lib/utils/continuable-run.test.ts`
- Modify: `src/lib/utils/room-ui.test.ts`
- Modify: `src/lib/utils/__tests__/agent-features.test.ts`
- Modify: `src/lib/utils/__tests__/native-permission.test.ts`
- Modify: `src/lib/utils/__tests__/add-dir-action.test.ts`
- Modify: `src/lib/stores/room-store.test.ts`
- Modify: `src/lib/stores/session-store.test.ts`

- [ ] **Step 1: Update frontend test files**

For each test file, remove Gemini-specific test cases, assertions, and test data. Grep for `"gemini"` in each file and remove/adjust.

- [ ] **Step 2: Run frontend tests**

Run: `npm test`
Expected: All tests pass with zero gemini references.

- [ ] **Step 3: Verify no remaining gemini references**

Run: `grep -ri "gemini" src/ src-tauri/src/ --include="*.ts" --include="*.svelte" --include="*.rs" | grep -v node_modules | grep -v target`
Expected: Zero matches (or only comments/docs that are acceptable).

- [ ] **Step 4: Run full verification**

Run: `npm run check && npm test && cargo check --manifest-path src-tauri/Cargo.toml`
Expected: All clean.

- [ ] **Step 5: Commit**

```bash
git add src/lib/utils/*.test.ts src/lib/utils/__tests__/ src/lib/stores/*.test.ts
git commit -m "test: remove Gemini test cases from frontend tests"
```

---

## Task 5: Seat Prompt Constraint

**Files:**
- Modify: `src/routes/rooms/+page.svelte` (line 342)

- [ ] **Step 1: Update `defaultSeatPromptWithLabel()`**

```typescript
function defaultSeatPromptWithLabel(label: string): string {
  return `You are ${label} in a three-seat roundtable. Answer independently, be concrete, and keep your reasoning concise. Don't do any change. Only read, analyze and discuss. Now wait for the topic. IMPORTANT: Roundtable outputs are "discussable judgments" — not research reports, not executable plans. When the user needs to take action, suggest switching to an independent session for implementation.`;
}
```

- [ ] **Step 2: Verify the function is called correctly**

Check that `defaultSeatPromptWithLabel()` is called from `defaultSeatPrompt()` (line 339) and the label-change handler (line 233). No changes needed to callers — the constraint is part of the returned string.

- [ ] **Step 3: Run check**

Run: `npm run check`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add src/routes/rooms/+page.svelte
git commit -m "feat: add roundtable scope constraint to seat prompt"
```

---

## Task 6: @Name SingleTarget — Backend

**Files:**
- Modify: `src-tauri/src/room/models.rs`
- Modify: `src-tauri/src/room/orchestrator.rs`

- [ ] **Step 1: Add `SingleTarget` variant to `RoomTurnMode`**

In `src-tauri/src/room/models.rs`, add the variant:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoomTurnMode {
    Fanout,
    Debate,
    Summary,
    Private,
    Review,
    Research,
    SingleTarget,
}
```

- [ ] **Step 2: Verify cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: Clean compile. The new variant is handled by existing `match` arms that have `_ =>` wildcards, or needs explicit arms added.

- [ ] **Step 3: Add `SingleTarget` to `RoundtableCommand` enum**

In `src-tauri/src/room/orchestrator.rs`, find the `RoundtableCommand` enum and add:

```rust
pub enum RoundtableCommand {
    Fanout { input: String },
    Debate { input: String },
    Summary { target: String },
    Private { target: String, message: String },
    SingleTarget { target: String, message: String },
}
```

- [ ] **Step 4: Update `parse_roundtable_command()`**

Change the `@TargetName message` parsing (currently returns `Private`) to return `SingleTarget`. Add `/dm @TargetName message` parsing for `Private`:

```rust
pub fn parse_roundtable_command(input: &str) -> RoundtableCommand {
    let trimmed = input.trim();
    if let Some(rest) = strip_command_word(trimmed, "@debate") {
        return RoundtableCommand::Debate {
            input: rest.trim().to_string(),
        };
    }
    if let Some(rest) = strip_command_word(trimmed, "@summary") {
        let target = rest.split_whitespace().next().unwrap_or_default()
            .trim_start_matches('@').to_string();
        return RoundtableCommand::Summary { target };
    }
    // /dm @TargetName message -> Private (preserved for backward compat)
    if let Some(rest) = strip_command_word(trimmed, "/dm") {
        let rest = rest.trim();
        if let Some(rest) = rest.strip_prefix('@') {
            let mut parts = rest.splitn(2, char::is_whitespace);
            let target = parts.next().unwrap_or_default().to_string();
            let message = parts.next().unwrap_or_default().trim().to_string();
            if !target.is_empty() && !message.is_empty() {
                return RoundtableCommand::Private { target, message };
            }
        }
    }
    // @TargetName message -> SingleTarget (public, only target responds)
    if let Some(rest) = trimmed.strip_prefix('@') {
        let mut parts = rest.splitn(2, char::is_whitespace);
        let target = parts.next().unwrap_or_default().to_string();
        let message = parts.next().unwrap_or_default().trim().to_string();
        if !target.is_empty() && !message.is_empty() {
            return RoundtableCommand::SingleTarget { target, message };
        }
    }
    RoundtableCommand::Fanout { input: trimmed.to_string() }
}
```

- [ ] **Step 5: Add `build_singletarget_prompt()` function**

Add this function in `orchestrator.rs`:

```rust
pub fn build_singletarget_prompt(
    turn_num: u64,
    target_label: &str,
    user_message: &str,
) -> String {
    format!(
        "[通用圆桌 · 第 {turn_num} 轮 · @single-target → {target_label}]\n\n\
         ## 用户指名提问\n\
         {user_message}\n\n\
         你是本轮唯一被指名回答的参与者。请给出你的完整观点。"
    )
}
```

- [ ] **Step 6: Handle `SingleTarget` in `run_roundtable_turn_with_runtime()`**

Find where `RoundtableCommand` is matched in the roundtable turn execution. Add a `SingleTarget` arm that:
1. Calls `find_participant()` to resolve the target
2. Returns error if not found: `"Participant '{name}' not found in this room"`
3. Returns error if ambiguous (multiple label matches): `"Ambiguous participant name '{name}'"`
4. Builds prompt via `build_singletarget_prompt()`
5. Sends prompt only to the target participant
6. Creates a `RoomTurn` with `mode: SingleTarget`, `target_participant_ids: [target.id]`
7. Writes to public `timeline.jsonl`

- [ ] **Step 7: Run Rust checks**

Run: `cargo check --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: Clean.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/room/models.rs src-tauri/src/room/orchestrator.rs
git commit -m "feat: add @Name SingleTarget — public turn to one participant"
```

---

## Task 7: @Name SingleTarget — Frontend

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/utils/room-ui.ts`
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

- [ ] **Step 1: Add `singletarget` to TypeScript types**

In `src/lib/types.ts`, find the `RoomTurnMode` type and add `"singletarget"`:

```typescript
export type RoomTurnMode = "fanout" | "debate" | "summary" | "private" | "review" | "research" | "singletarget";
```

- [ ] **Step 2: Add UI labels in `src/lib/utils/room-ui.ts`**

Add `singletarget` handling to `roomTurnModeLabel()` and `roomTurnModeColor()` (or equivalent functions that map mode to display label/color). Follow the existing pattern for other modes.

- [ ] **Step 3: Add i18n messages**

In `messages/en.json`, add under the room-related keys:
```json
"room.mode.singletarget": "Single Target",
"room.mode.singletarget.short": "ST"
```

In `messages/zh-CN.json`:
```json
"room.mode.singletarget": "指名回答",
"room.mode.singletarget.short": "指名"
```

- [ ] **Step 4: Run checks**

Run: `npm run check && npm run i18n:check`
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add src/lib/types.ts src/lib/utils/room-ui.ts messages/
git commit -m "feat: add SingleTarget mode labels to frontend"
```

---

## Task 8: Room Session Sidebar Grouping — Backend

**Files:**
- Modify: `src-tauri/src/commands/rooms.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add `RoomRunIndexEntry` struct and `list_room_run_index` command**

In `src-tauri/src/commands/rooms.rs`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomRunIndexEntry {
    pub room_id: String,
    pub room_name: String,
    pub room_kind: String,
    pub run_ids: Vec<String>,
}

#[tauri::command]
pub fn list_room_run_index() -> Result<Vec<RoomRunIndexEntry>, String> {
    let rooms_dir = crate::storage::rooms_dir();
    if !rooms_dir.exists() {
        return Ok(vec![]);
    }
    let mut entries = Vec::new();
    for dir_entry in std::fs::read_dir(&rooms_dir).map_err(|e| e.to_string())? {
        let dir_entry = dir_entry.map_err(|e| e.to_string())?;
        if !dir_entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        let room_json = dir_entry.path().join("room.json");
        if !room_json.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&room_json).map_err(|e| e.to_string())?;
        let room: crate::room::models::Room =
            serde_json::from_str(&content).map_err(|e| e.to_string())?;
        entries.push(RoomRunIndexEntry {
            room_id: room.id,
            room_name: room.name,
            room_kind: format!("{:?}", room.kind).to_lowercase(),
            run_ids: room.participants.iter().map(|p| p.run_id.clone()).collect(),
        });
    }
    Ok(entries)
}
```

- [ ] **Step 2: Register command in `src-tauri/src/lib.rs`**

Add `list_room_run_index` to the rooms section (line 161-169) of `.invoke_handler(tauri::generate_handler![...])`.

- [ ] **Step 3: Run Rust checks**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: Clean compile.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/rooms.rs src-tauri/src/lib.rs
git commit -m "feat: add list_room_run_index Tauri command for sidebar grouping"
```

---

## Task 9: Room Session Sidebar Grouping — Frontend

**Files:**
- Modify: `src/lib/api.ts`
- Modify: `src/lib/utils/sidebar-groups.ts`
- Modify: Sidebar host component (wherever `buildProjectFolders` is called)
- Modify: `src/lib/components/ProjectFolderItem.svelte`
- Modify: `messages/en.json`, `messages/zh-CN.json`

- [ ] **Step 1: Add API wrapper in `src/lib/api.ts`**

```typescript
export async function listRoomRunIndex(): Promise<RoomRunIndexEntry[]> {
  return invoke("list_room_run_index");
}

export interface RoomRunIndexEntry {
  room_id: string;
  room_name: string;
  room_kind: string;
  run_ids: string[];
}
```

- [ ] **Step 2: Update `buildProjectFolders()` in `src/lib/utils/sidebar-groups.ts`**

Add a new parameter and room-filtering logic:

```typescript
export interface RoomRunMapping {
  roomId: string;
  roomName: string;
  roomKind: string;
}

export function buildProjectFolders(
  runs: TaskRun[],
  favoriteRunIds: Set<string>,
  pinnedCwds: string[],
  removedCwds: string[] = [],
  pinnedConversationKeys: Set<string> = new Set(),
  roomRunMap: Map<string, RoomRunMapping> = new Map(),
): ProjectFolder[] {
  // Partition runs: room runs vs regular runs
  const regularRuns: TaskRun[] = [];
  const roomRunsByRoom = new Map<string, TaskRun[]>();

  for (const run of runs) {
    const mapping = roomRunMap.get(run.id);
    if (mapping) {
      const existing = roomRunsByRoom.get(mapping.roomId) ?? [];
      existing.push(run);
      roomRunsByRoom.set(mapping.roomId, existing);
    } else {
      regularRuns.push(run);
    }
  }

  // Build regular folders from regularRuns using existing bucketing logic
  // (copy the existing lines 65-187 logic, replacing `runs` with `regularRuns`)
  const cwdBuckets = new Map<string, TaskRun[]>();
  for (const run of regularRuns) {
    const cwd = normalizeCwd(run.cwd ?? "");
    const bucket = cwdBuckets.get(cwd) ?? [];
    bucket.push(run);
    cwdBuckets.set(cwd, bucket);
  }
  // ... rest of existing grouping/sorting logic from lines 65-187 ...
  const folders: ProjectFolder[] = []; // populated by existing logic

  // Build virtual "Rooms" folder if there are any room runs
  if (roomRunsByRoom.size > 0) {
    const roomConversations: ConversationGroup[] = [];
    for (const [roomId, roomRuns] of roomRunsByRoom) {
      const mapping = [...roomRunMap.values()].find(m => m.roomId === roomId);
      const roomName = mapping?.roomName ?? "未命名房间";
      // Group room runs into a single ConversationGroup per room
      const sortedRuns = roomRuns.sort((a, b) =>
        (b.started_at ?? "").localeCompare(a.started_at ?? "")
      );
      const latestRun = sortedRuns[0];
      roomConversations.push({
        groupKey: `room:${roomId}`,
        runs: sortedRuns,
        title: roomName,
        latestRun,
        isFavorite: sortedRuns.some(r => favoriteRunIds.has(r.id)),
        totalMessages: sortedRuns.length,
      });
    }
    roomConversations.sort((a, b) =>
      (b.latestRun.started_at ?? "").localeCompare(a.latestRun.started_at ?? "")
    );
    const latestActivity = roomConversations[0]?.latestRun.started_at ?? "";
    const roomsFolder: ProjectFolder = {
      cwd: "__rooms__",
      folderKey: "cwd:__rooms__",
      isUncategorized: false,
      conversations: roomConversations,
      conversationCount: roomConversations.length,
      latestActivityAt: latestActivity,
    };
    folders.unshift(roomsFolder); // pin at top
  }

  return folders;
}
```

- [ ] **Step 3: Update sidebar host to load room data**

Find where `buildProjectFolders` is called (likely in a layout or sidebar component). Add:

```typescript
import { listRoomRunIndex } from "$lib/api";
import type { RoomRunMapping } from "$lib/utils/sidebar-groups";

let roomRunMap = $state<Map<string, RoomRunMapping>>(new Map());

onMount(async () => {
  try {
    const index = await listRoomRunIndex();
    const map = new Map<string, RoomRunMapping>();
    for (const entry of index) {
      for (const runId of entry.run_ids) {
        map.set(runId, {
          roomId: entry.room_id,
          roomName: entry.room_name || "未命名房间",
          roomKind: entry.room_kind,
        });
      }
    }
    roomRunMap = map;
  } catch {}
});
```

Pass `roomRunMap` to `buildProjectFolders()`.

- [ ] **Step 4: Handle "Rooms" folder in `ProjectFolderItem.svelte`**

Add special handling for `folderKey === "cwd:__rooms__"`:
- Show a room icon (e.g., grid/会议 icon)
- Make it non-removable (hide the remove/hide controls)
- Always pin it at the top
- Room run items navigate to `/rooms?room={roomId}` on click

- [ ] **Step 5: Add i18n messages**

In `messages/en.json`:
```json
"sidebar.rooms": "Rooms"
```

In `messages/zh-CN.json`:
```json
"sidebar.rooms": "会议室"
```

- [ ] **Step 6: Run checks and tests**

Run: `npm run check && npm test -- src/lib/utils/sidebar-groups.test.ts`
Expected: All pass. Add new test cases for room-run filtering.

- [ ] **Step 7: Commit**

```bash
git add src/lib/api.ts src/lib/utils/sidebar-groups.ts src/lib/components/ProjectFolderItem.svelte messages/
git commit -m "feat: virtual Rooms folder in sidebar for room participant runs"
```

---

## Task 10: Stepper Mini-Map — Backend

**Files:**
- Modify: `src-tauri/src/commands/rooms.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add snapshot types and command**

In `src-tauri/src/commands/rooms.rs`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantSnapshot {
    pub participant_id: String,
    pub label: String,
    pub content: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomTurnSnapshot {
    pub turn: crate::room::models::RoomTurn,
    pub participant_contents: Vec<ParticipantSnapshot>,
}

#[tauri::command]
pub fn get_room_turn_snapshot(room_id: String, turn_id: String) -> Result<RoomTurnSnapshot, String> {
    // 1. Load room timeline to find the turn
    let room = crate::storage::rooms::load_room(&room_id)?;
    let timeline_path = crate::storage::rooms::room_dir(&room_id).join("timeline.jsonl");
    let turns = crate::storage::rooms::list_turns_jsonl(&room_id, timeline_path)?;
    let turn = turns.iter().find(|t| t.id == turn_id)
        .ok_or_else(|| format!("Turn {turn_id} not found"))?;

    // 2. For each response, load events by seq range
    let mut participant_contents = Vec::new();
    for response in &turn.responses {
        let label = room.participants.iter()
            .find(|p| p.id == response.participant_id)
            .map(|p| p.label.clone())
            .unwrap_or_else(|| response.participant_id.clone());

        let content = if response.status == "deleted" {
            response.preview.clone().unwrap_or_default()
        } else {
            // Load events from events.jsonl for this run
            let events = crate::storage::events::list_events(&response.run_id, 0);
            let filtered: Vec<_> = events.into_iter()
                .filter(|e| e.seq >= response.event_seq_start && e.seq <= response.event_seq_end)
                .collect();
            // Extract assistant text from events
            extract_assistant_text(&filtered)
                .or_else(|| response.preview.clone())
                .unwrap_or_default()
        };

        participant_contents.push(ParticipantSnapshot {
            participant_id: response.participant_id.clone(),
            label,
            content,
            status: response.status.clone(),
            error: response.error.clone(),
        });
    }

    Ok(RoomTurnSnapshot {
        turn: turn.clone(),
        participant_contents,
    })
}

fn extract_assistant_text(events: &[crate::storage::events::RunEvent]) -> Option<String> {
    // Concatenate assistant message payloads from events
    let mut texts = Vec::new();
    for event in events {
        if let Some(text) = event.payload.get("message").and_then(|v| v.as_str()) {
            texts.push(text.to_string());
        }
    }
    if texts.is_empty() { None } else { Some(texts.join("\n")) }
}
```

- [ ] **Step 2: Register command in `src-tauri/src/lib.rs`**

Add `get_room_turn_snapshot` to the rooms section of `.invoke_handler(...)`.

- [ ] **Step 3: Run Rust checks**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: Clean compile.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/rooms.rs src-tauri/src/lib.rs
git commit -m "feat: add get_room_turn_snapshot Tauri command for stepper replay"
```

---

## Task 11: Stepper Mini-Map — Frontend Component

**Files:**
- Create: `src/lib/components/RoomStepper.svelte`
- Modify: `src/lib/types.ts`
- Modify: `src/lib/api.ts`
- Modify: `messages/en.json`, `messages/zh-CN.json`

- [ ] **Step 1: Add TypeScript types in `src/lib/types.ts`**

```typescript
export interface ParticipantSnapshot {
  participant_id: string;
  label: string;
  content: string;
  status: string;
  error?: string;
}

export interface RoomTurnSnapshot {
  turn: RoomTurn;
  participant_contents: ParticipantSnapshot[];
}
```

- [ ] **Step 2: Add API wrapper in `src/lib/api.ts`**

```typescript
export async function getRoomTurnSnapshot(roomId: string, turnId: string): Promise<RoomTurnSnapshot> {
  return invoke("get_room_turn_snapshot", { roomId, turnId });
}
```

- [ ] **Step 3: Create `src/lib/components/RoomStepper.svelte`**

```svelte
<script lang="ts">
  import type { RoomTurn, RoomTurnSnapshot } from "$lib/types";
  import { getRoomTurnSnapshot } from "$lib/api";
  import { t } from "$lib/i18n";

  let {
    roomId,
    turns,
    activeSnapshot = $bindable(null),
  }: {
    roomId: string;
    turns: RoomTurn[];
    activeSnapshot: RoomTurnSnapshot | null;
  } = $props();

  let loading = $state(false);

  function turnStatus(turn: RoomTurn): "complete" | "running" | "failed" | "pending" {
    if (turn.responses.some(r => r.status === "failed")) return "failed";
    if (turn.responses.some(r => r.status === "running")) return "running";
    if (turn.completed_at) return "complete";
    return "pending";
  }

  function statusColor(status: string): string {
    switch (status) {
      case "complete": return "bg-green-500";
      case "running": return "bg-amber-500";
      case "failed": return "bg-red-500";
      default: return "bg-gray-400";
    }
  }

  function modeLabel(mode: string): string {
    // Map mode to display label using i18n
    return mode;
  }

  async function handleClick(turn: RoomTurn) {
    if (activeSnapshot?.turn.id === turn.id) {
      activeSnapshot = null; // exit snapshot
      return;
    }
    loading = true;
    try {
      activeSnapshot = await getRoomTurnSnapshot(roomId, turn.id);
    } catch (e) {
      console.error("Failed to load snapshot:", e);
    } finally {
      loading = false;
    }
  }

  function exitSnapshot() {
    activeSnapshot = null;
  }
</script>

<div class="flex flex-col gap-1 max-h-64 overflow-y-auto px-3 py-2">
  {#if turns.length === 0}
    <p class="text-sm text-muted-foreground">No turns yet</p>
  {:else}
    {#each turns as turn, i}
      {@const status = turnStatus(turn)}
      {@const isActive = activeSnapshot?.turn.id === turn.id}
      <button
        class="flex items-start gap-2 text-left rounded px-2 py-1.5 hover:bg-accent/50 transition-colors {isActive ? 'bg-accent' : ''}"
        onclick={() => handleClick(turn)}
        disabled={loading}
      >
        <!-- Dot -->
        <span class="mt-1.5 h-2.5 w-2.5 rounded-full shrink-0 {statusColor(status)}"></span>
        <!-- Label -->
        <span class="flex flex-col gap-0.5 min-w-0">
          <span class="text-xs font-medium">
            Turn {turn.idx} · {modeLabel(turn.mode)}
          </span>
          <span class="text-xs text-muted-foreground truncate">
            {turn.user_input.slice(0, 60)}{turn.user_input.length > 60 ? "…" : ""}
          </span>
        </span>
      </button>
    {/each}
  {/if}
</div>

{#if activeSnapshot}
  <div class="shrink-0 border-t border-purple-300 bg-purple-50 dark:bg-purple-950/30 px-3 py-2 flex items-center justify-between">
    <span class="text-sm font-medium text-purple-700 dark:text-purple-300">
      {t("room.snapshot.banner", { turn: activeSnapshot.turn.idx.toString() })}
    </span>
    <button
      class="text-xs text-purple-600 dark:text-purple-400 hover:underline"
      onclick={exitSnapshot}
    >
      {t("room.snapshot.exit")}
    </button>
  </div>
{/if}
```

- [ ] **Step 4: Add i18n messages**

In `messages/en.json`:
```json
"room.snapshot.banner": "Read-only history · Turn {turn} snapshot",
"room.snapshot.exit": "Exit snapshot"
```

In `messages/zh-CN.json`:
```json
"room.snapshot.banner": "只读历史 · 第 {turn} 轮快照",
"room.snapshot.exit": "退出快照"
```

- [ ] **Step 5: Run checks**

Run: `npm run check`
Expected: No errors.

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/RoomStepper.svelte src/lib/types.ts src/lib/api.ts messages/
git commit -m "feat: add RoomStepper component for turn-by-turn replay"
```

---

## Task 12: Stepper Mini-Map — Integrate into Room Page

**Files:**
- Modify: `src/routes/rooms/+page.svelte`
- Modify: `src/lib/stores/room-store.svelte.ts`

- [ ] **Step 1: Add snapshot state to room store**

In `src/lib/stores/room-store.svelte.ts`, add:

```typescript
activeSnapshot: RoomTurnSnapshot | null = $state(null);

snapshotTurn(snapshot: RoomTurnSnapshot) {
  this.activeSnapshot = snapshot;
}

exitSnapshot() {
  this.activeSnapshot = null;
}
```

- [ ] **Step 2: Replace History strip with RoomStepper in `+page.svelte`**

Replace the History strip section (lines 608-683) with:

```svelte
<RoomStepper
  roomId={store.room.id}
  turns={store.room.turns}
  bind:activeSnapshot={store.activeSnapshot}
/>
```

Import `RoomStepper` at the top of the file.

- [ ] **Step 3: Wire snapshot mode to participant panes**

In the participant panes section (lines 526-583), add conditional rendering:

```svelte
{#if store.activeSnapshot}
  <!-- Snapshot mode: show read-only content -->
  {@const snapshot = store.activeSnapshot.participant_contents[i]}
  <div class="...">
    <div class="text-sm">{snapshot?.content ?? "No data"}</div>
  </div>
{:else}
  <!-- Live mode: existing pane rendering -->
  <!-- ... existing code ... -->
{/if}
```

- [ ] **Step 4: Disable composer in snapshot mode**

Wrap the composer section (lines 722-743) with:

```svelte
{#if !store.activeSnapshot}
  <!-- existing composer -->
{/if}
```

- [ ] **Step 5: Run checks and test manually**

Run: `npm run check`
Manual: Start the app, create a room, send a few messages, verify the stepper shows turns and clicking loads snapshots.

- [ ] **Step 6: Commit**

```bash
git add src/routes/rooms/+page.svelte src/lib/stores/room-store.svelte.ts
git commit -m "feat: integrate RoomStepper into room page, replace History strip"
```

---

## Task 13: Context Events Verification

**Files:**
- Verify: `src-tauri/src/agent/stream.rs`
- Verify: `src-tauri/src/agent/native_transcript.rs`
- Verify: `src/routes/chat/+page.svelte`

- [ ] **Step 1: Check stream-session path for context events**

Read `src-tauri/src/agent/stream.rs` and trace how `context_window` events are parsed from the Claude CLI stream-json output and forwarded to the frontend. Verify this works for both Claude native and provider-based sessions (both use `SessionActor`).

- [ ] **Step 2: Check native PTY path for context events**

Read `src-tauri/src/agent/native_transcript.rs` and check whether `context_window` events are extracted from Codex transcript files and forwarded to the frontend.

- [ ] **Step 3: Check chat page renders context UI for all session types**

Read `src/routes/chat/+page.svelte` and verify that `ContextUsageGrid` and rate limit indicators are rendered unconditionally (not gated by agent type).

- [ ] **Step 4: If gaps found, fix event forwarding**

If any path is missing context events, add forwarding logic. If all paths work, no code changes needed — just document the verification result.

- [ ] **Step 5: Commit (only if changes made)**

```bash
git add src-tauri/src/agent/stream.rs src-tauri/src/agent/native_transcript.rs src/routes/chat/+page.svelte
git commit -m "fix: ensure context events display for all CC session types"
```

---

## Post-Implementation Verification

Run the full verification suite:

```bash
npm run check && npm test && cargo check --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Verify:
1. `grep -ri "gemini" src/ src-tauri/src/ --include="*.ts" --include="*.svelte" --include="*.rs"` returns zero matches
2. Room page stepper loads and displays turn snapshots
3. `@Alice hello` produces a SingleTarget public turn
4. `/dm @Alice hello` produces a Private turn
5. Sidebar shows a "Rooms" virtual folder with room participant runs
6. Seat prompt includes the scope constraint sentence
