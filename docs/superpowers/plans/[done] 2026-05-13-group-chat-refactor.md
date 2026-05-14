# Group Chat Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Room system with a unified Group Chat experience, add AiCharacter persona templates, Plan mechanism, and context management.

**Architecture:** Mechanical rename (Room → GroupChat) with dead code deletion first, then new features (Characters, Plans, Context) layered on top. Each phase produces a working, testable checkpoint.

**Tech Stack:** Rust (Tauri backend), Svelte 5 (frontend), TypeScript, Vitest, cargo check/clippy

**Spec:** `docs/superpowers/specs/2026-05-13-group-chat-refactor-design.md`

---

## Phase 10.a.1: Delete Unused Code

### Task 1: Delete seat memory Rust module

**Files:**
- Delete: `src-tauri/src/room/memory.rs` (447 lines)

- [ ] **Step 1: Remove memory module declaration**

Edit `src-tauri/src/room/mod.rs` — remove the `pub mod memory;` line.

- [ ] **Step 2: Delete the file**

```bash
rm src-tauri/src/room/memory.rs
```

- [ ] **Step 3: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

Expected: Compile errors from references to `memory::*` functions. These will be fixed in subsequent tasks.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/room/mod.rs
git commit -m "chore: remove memory module declaration from room/mod.rs"
```

### Task 2: Delete Driver/Research/seat-memory code from orchestrator

**Files:**
- Modify: `src-tauri/src/room/orchestrator.rs` (2891 lines)

- [ ] **Step 1: Delete `DriverCommand` enum** (line 70)

Remove the entire `DriverCommand` enum definition.

- [ ] **Step 2: Delete `run_driver_turn` and `run_driver_turn_with_runtime`** (lines 492-607)

Remove both functions entirely.

- [ ] **Step 3: Delete `run_research_turn` and `run_research_turn_with_runtime`** (lines 608-715)

Remove both functions entirely.

- [ ] **Step 4: Delete `parse_driver_command`** (lines 761-789)

Remove the function entirely.

- [ ] **Step 5: Delete `build_driver_review_prompt`** (lines 1006-1065)

Remove the function entirely.

- [ ] **Step 6: Delete `build_research_prompt`** (lines 1067-end)

Remove the function entirely.

- [ ] **Step 7: Remove seat memory references in `build_debate_prompt`**

In `build_debate_prompt` (line 791), remove the `seat_memory_section: Option<&str>` parameter and any code that appends it to the prompt. Update the function signature and all call sites.

- [ ] **Step 8: Remove `run_driver_turn_with_runtime` and `run_research_turn_with_runtime` calls in `send_room_message`**

In `commands/rooms.rs` `send_room_message` (line 524), remove the `RoomKind::Driver` and `RoomKind::Research` match branches. Only keep the `Roundtable` (default) branch.

- [ ] **Step 9: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

Expected: Compile errors from missing memory functions and deleted types. Fixed in next tasks.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/room/orchestrator.rs
git commit -m "chore: delete Driver/Research/seat-memory code from orchestrator"
```

### Task 3: Delete seat memory and Driver/Research from storage

**Files:**
- Modify: `src-tauri/src/storage/rooms.rs` (1239 lines)

- [ ] **Step 1: Delete `DriverMcpBundle` struct and `driver_mcp_file` helper** (lines 53-71)

Remove the struct and private function.

- [ ] **Step 2: Delete `add_seat_memory_entry`** (lines 277-295)

Remove the function entirely.

- [ ] **Step 3: Delete `delete_seat_memory_entry`** (lines 297-312)

Remove the function entirely.

- [ ] **Step 4: Delete `clear_seat_memory`** (lines 314-323)

Remove the function entirely.

- [ ] **Step 5: Delete `write_research_artifact`** (lines 347-371)

Remove the function entirely.

- [ ] **Step 6: Delete `read_research_artifact`** (lines 373-383)

Remove the function entirely.

- [ ] **Step 7: Delete `list_research_artifacts`** (lines 385-412)

Remove the function entirely.

- [ ] **Step 8: Delete `write_driver_arena_files`** (lines 414-487)

Remove the function entirely.

- [ ] **Step 9: Delete `write_driver_mcp_bundle`** (lines 489-612)

Remove the function entirely.

- [ ] **Step 10: Remove seat memory fields from `create_room` and `create_room_with_kind`**

In the `Room` struct construction within these functions, remove initialization of `seat_memories`, `seat_memory_inbox`, `seat_profile`, `last_checkpoint_turn`, `last_checkpoint_at`.

- [ ] **Step 11: Remove seat memory fields from `get_room`**

In `get_room` (line 137), if it loads seat memory data from separate files, remove that logic.

- [ ] **Step 12: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

- [ ] **Step 13: Commit**

```bash
git add src-tauri/src/storage/rooms.rs
git commit -m "chore: delete seat memory and Driver/Research from storage"
```

### Task 4: Delete seat memory types and fields from models

**Files:**
- Modify: `src-tauri/src/room/models.rs` (213 lines)

- [ ] **Step 1: Delete `RoomKind` enum** (lines 6-13)

Remove the entire enum.

- [ ] **Step 2: Delete `ArenaMemoryKind` enum** (lines 94-98)

Remove the entire enum.

- [ ] **Step 3: Delete `MemoryKind` enum** (lines 115-122)

Remove the entire enum.

- [ ] **Step 4: Delete `ArenaMemoryCandidate` struct** (lines 102-113)

Remove the entire struct.

- [ ] **Step 5: Delete `SeatMemoryEntry` struct** (lines 124-139)

Remove the entire struct.

- [ ] **Step 6: Delete `PendingMemoryCandidate` struct** (lines 141-153)

Remove the entire struct.

- [ ] **Step 7: Delete `SeatProfile` struct** (lines 155-161)

Remove the entire struct.

- [ ] **Step 8: Delete `ResearchResult` struct** (lines 84-92)

Remove the entire struct.

- [ ] **Step 9: Delete `ResearchArtifact` struct** (lines 163-173)

Remove the entire struct.

- [ ] **Step 10: Remove seat memory fields from `Room` struct** (line 15)

Remove fields: `kind`, `seat_memories`, `seat_memory_inbox`, `seat_profile`, `last_checkpoint_turn`, `last_checkpoint_at`.

- [ ] **Step 11: Remove `research_artifact` and seat memory fields from `RoomDetail`** (line 194)

Remove fields: `research_artifact`, `seat_memories`, `seat_memory_inbox`, `seat_profile`.

- [ ] **Step 12: Remove `kind` from `RoomSummary`** (line 175)

Remove the `kind` field.

- [ ] **Step 13: Remove `ResearchArtifact` from `RoomDetail`**

Remove the `research_artifact` field.

- [ ] **Step 14: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

- [ ] **Step 15: Commit**

```bash
git add src-tauri/src/room/models.rs
git commit -m "chore: delete seat memory types and RoomKind from models"
```

### Task 5: Delete seat memory Tauri commands

**Files:**
- Modify: `src-tauri/src/commands/rooms.rs` (921 lines)
- Modify: `src-tauri/src/lib.rs` (lines 172-174)

- [ ] **Step 1: Delete `add_seat_memory_entry` command** (line 474)

Remove the function entirely.

- [ ] **Step 2: Delete `delete_seat_memory_entry` command** (line 504)

Remove the function entirely.

- [ ] **Step 3: Delete `clear_seat_memory` command** (line 514)

Remove the function entirely.

- [ ] **Step 4: Remove `kind` parameter from `create_room` command** (line 55)

Change signature from `create_room(name, description, cwd, kind)` to `create_room(name, description, cwd)`. Update the function body to not pass `kind` to storage.

- [ ] **Step 5: Remove kind-based dispatch in `send_room_message`** (line 524)

Remove the match on `room.kind` — just call `run_roundtable_turn_with_runtime` directly.

- [ ] **Step 6: Remove seat memory imports**

Remove `MemoryKind`, `SeatMemoryEntry`, and related imports from the top of the file.

- [ ] **Step 7: Remove command registrations from `lib.rs`**

Remove these 3 lines from the `invoke_handler` block:
```
commands::rooms::add_seat_memory_entry,
commands::rooms::delete_seat_memory_entry,
commands::rooms::clear_seat_memory,
```

- [ ] **Step 8: Remove `kind` from `RoomRunIndexEntry`** (line 67)

Remove the `room_kind` field.

- [ ] **Step 9: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/commands/rooms.rs src-tauri/src/lib.rs
git commit -m "chore: delete seat memory commands and remove kind from create_room"
```

### Task 6: Delete Driver/Research/seat-memory from adapter

**Files:**
- Modify: `src-tauri/src/room/adapter.rs` (552 lines)

- [ ] **Step 1: Remove `PromptScope::Room` variant** (line 74-79)

Change to `PromptScope::GroupChat` (will be used in rename phase) or just remove `Room` and keep `Participant` and `Turn`. For now, keep as-is since the rename happens in 10.a.2.

- [ ] **Step 2: Remove seat memory prompt injection in `build_debate_prompt` call path**

Check if `adapter.rs` references seat memory functions. If so, remove those references.

- [ ] **Step 3: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/room/adapter.rs
git commit -m "chore: clean adapter of seat memory references"
```

### Task 7: Delete seat memory and Driver/Research from frontend

**Files:**
- Modify: `src/lib/types.ts` (lines 98-168)
- Modify: `src/lib/api.ts` (lines 258-284)
- Modify: `src/lib/stores/room-store.svelte.ts` (lines 353-396)

- [ ] **Step 1: Delete seat memory types from `types.ts`**

Remove these type definitions:
- `ArenaMemoryKind` (line 98)
- `ArenaMemoryCandidate` (line 100)
- `MemoryKind` (line 110)
- `SeatMemoryEntry` (line 112)
- `PendingMemoryCandidate` (line 124)
- `SeatProfile` (line 136)
- `ResearchResult` (line 151)
- `ResearchArtifact` (line 160)

- [ ] **Step 2: Remove `kind` from `RoomKind` type** (line 78)

Delete the entire `RoomKind` type alias.

- [ ] **Step 3: Remove `review` and `research` from `RoomTurnMode`** (line 80)

Change from `"fanout" | "debate" | "summary" | "private" | "review" | "research" | "singletarget"` to `"fanout" | "debate" | "summary" | "private" | "singletarget"`.

- [ ] **Step 4: Remove `kind` from `RoomSummary`** (line 194)

Remove the `kind` field from the interface.

- [ ] **Step 5: Remove seat memory fields from `RoomDetail`** (line 211)

Remove: `research_artifact`, `seat_memories`, `seat_memory_inbox`, `seat_profile`.

- [ ] **Step 6: Delete seat memory API functions from `api.ts`**

Remove:
- `addSeatMemoryEntry` (line 258)
- `deleteSeatMemoryEntry` (line 269)
- `clearSeatMemory` (line 278)

- [ ] **Step 7: Remove `kind` parameter from `createRoom`** (line 160)

Change signature from `createRoom(name, description, cwd, kind)` to `createRoom(name, description, cwd)`.

- [ ] **Step 8: Remove `kind` from `RoomRunIndexEntry`** (line 135)

Remove the `room_kind` field.

- [ ] **Step 9: Delete seat memory methods from `room-store.svelte.ts`**

Remove:
- `addSeatMemory` (line 353)
- `deleteSeatMemory` (line 370)
- `clearSeatMemory` (line 382)

- [ ] **Step 10: Remove `kind` parameter from `createRoom` in store** (line 85)

Update the store's `createRoom` method to not pass `kind`.

- [ ] **Step 11: Remove `kind` from `createRoundtableWithParticipants`** (line 102)

Remove `kind` parameter and related logic.

- [ ] **Step 12: Verify**

```bash
npm run check 2>&1 | tail -20
```

- [ ] **Step 13: Commit**

```bash
git add src/lib/types.ts src/lib/api.ts src/lib/stores/room-store.svelte.ts
git commit -m "chore: delete seat memory and Driver/Research from frontend"
```

### Task 8: Delete /rooms route page

**Files:**
- Delete: `src/routes/rooms/+page.svelte` (1006 lines)

- [ ] **Step 1: Delete the route page**

```bash
rm src/routes/rooms/+page.svelte
```

- [ ] **Step 2: Remove /rooms navigation from layout**

Edit `src/routes/+layout.svelte`:
- Remove the nav item `{ path: "/rooms", label: () => t("nav_rooms"), icon: "rooms" }` (line 447)
- Remove `loadRoomRunMap()` function and its call (lines 467-480, 616)
- Remove `roomRunMap` state (line 135)
- Remove `__rooms__` folder handling in sidebar rendering (lines 2329-2356)
- Remove `listRoomRunIndex` import (line 14)
- Remove `RoomRunMapping` import (line 43)

- [ ] **Step 3: Remove `__rooms__` folder logic from sidebar-groups.ts**

Edit `src/lib/utils/sidebar-groups.ts`:
- Remove `RoomRunMapping` interface (line 12-16)
- Remove `roomRunMap` parameter from `buildProjectFolders` (line 70)
- Remove room partitioning logic (lines 73-88)
- Remove virtual "Rooms" folder construction (lines 212-243)

- [ ] **Step 4: Remove room references from ProjectFolderItem.svelte**

Edit `src/lib/components/ProjectFolderItem.svelte`:
- Remove `isRoomsFolder` derived (line 68)
- Remove conditional rendering for `__rooms__` folder

- [ ] **Step 5: Verify**

```bash
npm run check 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add src/routes/rooms/ src/routes/+layout.svelte src/lib/utils/sidebar-groups.ts src/lib/components/ProjectFolderItem.svelte
git commit -m "chore: delete /rooms route and sidebar room references"
```

### Task 9: Delete room UI helpers and seat memory tests

**Files:**
- Delete: `src/lib/utils/room-ui.ts` (61 lines)
- Delete: `src/lib/utils/room-ui.test.ts` (52 lines)
- Modify: `src/lib/stores/room-store.test.ts` (495 lines)

- [ ] **Step 1: Delete room-ui.ts**

```bash
rm src/lib/utils/room-ui.ts src/lib/utils/room-ui.test.ts
```

- [ ] **Step 2: Remove seat memory tests from room-store.test.ts**

Remove test cases that test `addSeatMemory`, `deleteSeatMemory`, `clearSeatMemory`.

- [ ] **Step 3: Remove room-ui imports from any remaining files**

Check for imports of `room-ui` in remaining files and remove them.

- [ ] **Step 4: Verify**

```bash
npm run check && npm run test 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add src/lib/utils/room-ui.ts src/lib/utils/room-ui.test.ts src/lib/stores/room-store.test.ts
git commit -m "chore: delete room-ui helpers and seat memory tests"
```

### Task 10: Delete agent-capabilities room functions

**Files:**
- Modify: `src/lib/utils/agent-capabilities.ts` (~55 lines)
- Modify: `src/lib/utils/agent-capabilities.test.ts` (~50 lines)

- [ ] **Step 1: Remove room capability functions**

Remove `canUseRoomActor()`, `canUseRoomActorRun()`, `canUseRoomParticipantRun()` (lines 44-55).

- [ ] **Step 2: Remove corresponding tests**

Remove the 11 lines of room-related tests.

- [ ] **Step 3: Verify**

```bash
npm run check && npm run test 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/utils/agent-capabilities.ts src/lib/utils/agent-capabilities.test.ts
git commit -m "chore: remove room capability functions"
```

### Task 11: Phase 10.a.1 checkpoint — full verification

- [ ] **Step 1: Run all checks**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run check
npm run lint
npm run test
npm run i18n:check
```

- [ ] **Step 2: Fix any remaining compile errors**

Search for remaining references to deleted types:
```bash
grep -r "RoomKind\|SeatMemory\|MemoryKind\|ResearchArtifact\|DriverCommand\|ArenaMemory" src-tauri/src/ --include="*.rs" | grep -v "target/"
grep -r "RoomKind\|SeatMemory\|MemoryKind\|ResearchArtifact\|room-ui\|canUseRoom" src/ --include="*.ts" --include="*.svelte"
```

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "chore: Phase 10.a.1 complete — all dead code deleted"
```

---

## Phase 10.a.2: Rename Room → GroupChat

### Task 12: Rename Rust models

**Files:**
- Modify: `src-tauri/src/room/models.rs`

- [ ] **Step 1: Rename structs**

Apply these renames throughout `models.rs`:
- `Room` → `GroupChat`
- `RoomParticipant` → `GroupChatParticipant`
- `RoomTurn` → `GroupChatTurn`
- `RoomTurnMode` → `GroupChatTurnMode`
- `RoomResponseRef` → `GroupChatResponseRef`
- `RoomSummary` → `GroupChatSummary`
- `RoomDetail` → `GroupChatDetail`
- `RoomParticipantDetail` → `GroupChatParticipantDetail`

- [ ] **Step 2: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

Expected: Many errors from other files still using old names. That's OK — fixed in subsequent tasks.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/room/models.rs
git commit -m "refactor: rename Room types to GroupChat in models"
```

### Task 13: Rename orchestrator

**Files:**
- Modify: `src-tauri/src/room/orchestrator.rs`

- [ ] **Step 1: Rename types and functions**

Apply these renames:
- `Room` → `GroupChat` (type references)
- `RoomParticipant` → `GroupChatParticipant`
- `RoomTurn` → `GroupChatTurn`
- `RoomTurnMode` → `GroupChatTurnMode`
- `RoomResponseRef` → `GroupChatResponseRef`
- `RoundtableCommand` → `GroupChatCommand`
- `RoomPipeRuntime` → `GroupChatPipeRuntime`
- `run_roundtable_turn` → `run_group_chat_turn`
- `run_roundtable_turn_with_runtime` → `run_group_chat_turn_with_runtime`
- `parse_roundtable_command` → `parse_group_chat_command`

- [ ] **Step 2: Update prompt builder function parameter types**

Update `build_debate_prompt`, `build_fanout_prompt`, `build_singletarget_prompt`, `build_summary_prompt` to use new type names.

- [ ] **Step 3: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/room/orchestrator.rs
git commit -m "refactor: rename Room types to GroupChat in orchestrator"
```

### Task 14: Rename adapter

**Files:**
- Modify: `src-tauri/src/room/adapter.rs`

- [ ] **Step 1: Rename types**

- `PromptScope::Room` → `PromptScope::GroupChat`
- `can_use_room_actor_run()` → `can_use_group_chat_actor_run()`
- Update all `Room*` type references to `GroupChat*`

- [ ] **Step 2: Update error message**

Change `"Room prompt injection is not wired in Phase 2"` to `"GroupChat prompt injection is not wired"`.

- [ ] **Step 3: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/room/adapter.rs
git commit -m "refactor: rename Room types to GroupChat in adapter"
```

### Task 15: Rename storage

**Files:**
- Modify: `src-tauri/src/storage/rooms.rs`

- [ ] **Step 1: Rename all Room types to GroupChat**

Apply throughout: `Room` → `GroupChat`, `RoomTurn` → `GroupChatTurn`, `RoomSummary` → `GroupChatSummary`, etc.

- [ ] **Step 2: Rename functions**

- `create_room` → `create_group_chat`
- `create_room_with_kind` → delete (kind removed)
- `get_room` → `get_group_chat`
- `list_rooms` → `list_group_chats`
- `delete_room` → `delete_group_chat`
- `update_memo` → `update_group_chat_memo`
- `attach_run` → `attach_group_chat_run`
- `append_public_turn` → `append_group_chat_public_turn`
- `list_public_turns` → `list_group_chat_public_turns`
- `append_private_turn` → `append_group_chat_private_turn`
- `list_private_turns` → `list_group_chat_private_turns`

- [ ] **Step 3: Update storage directory path**

Change `rooms/` → `group-chats/` in all path construction. Change `room.json` → `group_chat.json`.

- [ ] **Step 4: Add first-launch check**

Add a function that checks if `~/.claw-go/rooms/` exists and logs an info warning.

- [ ] **Step 5: Rename storage module**

```bash
mv src-tauri/src/storage/rooms.rs src-tauri/src/storage/group_chats.rs
```

Update `src-tauri/src/storage/mod.rs`: change `pub mod rooms;` to `pub mod group_chats;`.

- [ ] **Step 6: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/storage/rooms.rs src-tauri/src/storage/group_chats.rs src-tauri/src/storage/mod.rs
git commit -m "refactor: rename storage/rooms.rs to storage/group_chats.rs"
```

### Task 16: Rename Tauri commands

**Files:**
- Modify: `src-tauri/src/commands/rooms.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Rename command functions**

Apply these renames in `commands/rooms.rs`:
- `list_rooms` → `list_group_chats`
- `get_room` → `get_group_chat`
- `create_room` → `create_group_chat`
- `list_room_run_index` → `list_group_chat_run_index`
- `get_room_turn_snapshot` → `get_group_chat_turn_snapshot`
- `attach_room_run` → `attach_group_chat_run`
- `create_room_participant` → `create_group_chat_participant`
- `create_room_claude_participant` → `create_group_chat_claude_participant`
- `update_room_memo` → `update_group_chat_memo`
- `send_room_message` → `send_group_chat_message`
- `delete_room` → `delete_group_chat`
- `cancel_room_turn` → `cancel_group_chat_turn`

- [ ] **Step 2: Rename structs in commands file**

- `RoomRunIndexEntry` → `GroupChatRunIndexEntry`
- `RoomTurnSnapshot` → `GroupChatTurnSnapshot`
- `ParticipantSnapshot` → stays (generic name)

- [ ] **Step 3: Update all type references**

Replace all `Room*` type references with `GroupChat*` in the commands file.

- [ ] **Step 4: Update `lib.rs` registrations**

Rename the command file module and update all registrations:
```rust
// Old
commands::rooms::list_rooms,
// New
commands::group_chat::list_group_chats,
```

Also update `pub mod rooms;` → `pub mod group_chat;` in `src-tauri/src/commands/mod.rs`.

- [ ] **Step 5: Rename the command file**

```bash
mv src-tauri/src/commands/rooms.rs src-tauri/src/commands/group_chat.rs
```

- [ ] **Step 6: Rename room module directory**

```bash
mv src-tauri/src/room/ src-tauri/src/group_chat/
```

Update `src-tauri/src/lib.rs`: change `pub mod room;` to `pub mod group_chat;`.

- [ ] **Step 7: Update cross-module imports**

Update `src-tauri/src/commands/runs.rs`: change `use crate::room::adapter::AgentCapabilities;` to `use crate::group_chat::adapter::AgentCapabilities;`.

- [ ] **Step 8: Update doc comment in models.rs**

Change "room adapter" to "group chat adapter" in `src-tauri/src/models.rs` line 740.

- [ ] **Step 9: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/commands/ src-tauri/src/group_chat/ src-tauri/src/lib.rs src-tauri/src/storage/mod.rs src-tauri/src/commands/mod.rs src-tauri/src/commands/runs.rs src-tauri/src/models.rs
git commit -m "refactor: rename room module and commands to group_chat"
```

### Task 17: Rename frontend types

**Files:**
- Modify: `src/lib/types.ts`

- [ ] **Step 1: Rename all room types**

- `RoomParticipant` → `GroupChatParticipant`
- `RoomTurnMode` → `GroupChatTurnMode`
- `RoomResponseRef` → `GroupChatResponseRef`
- `RoomTurn` → `GroupChatTurn`
- `RoomTurnSnapshot` → `GroupChatTurnSnapshot`
- `RoomSummary` → `GroupChatSummary`
- `RoomParticipantDetail` → `GroupChatParticipantDetail`
- `RoomDetail` → `GroupChatDetail`
- `ParticipantSnapshot` → stays (generic)
- `AgentCapabilities` → stays (not room-specific)
- `AgentKind` → stays
- `ResumeCapability` → stays
- `PromptInjection` → stays

- [ ] **Step 2: Add `character_id` field to `GroupChatParticipant`**

```typescript
export interface GroupChatParticipant {
  id: string;
  run_id: string;
  character_id: string;  // NEW: references AiCharacter.id
  agent: string;
  model?: string;
  label: string;
  role: string;
  joined_at: string;
}
```

- [ ] **Step 3: Verify**

```bash
npm run check 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/types.ts
git commit -m "refactor: rename Room types to GroupChat in types.ts"
```

### Task 18: Rename frontend API

**Files:**
- Modify: `src/lib/api.ts`

- [ ] **Step 1: Rename API functions**

- `listRooms` → `listGroupChats`
- `listRoomRunIndex` → `listGroupChatRunIndex`
- `getRoomTurnSnapshot` → `getGroupChatTurnSnapshot`
- `getRoom` → `getGroupChat`
- `createRoom` → `createGroupChat`
- `attachRoomRun` → `attachGroupChatRun`
- `createRoomClaudeParticipant` → `createGroupChatClaudeParticipant`
- `createRoomParticipant` → `createGroupChatParticipant`
- `updateRoomMemo` → `updateGroupChatMemo`
- `sendRoomMessage` → `sendGroupChatMessage`
- `deleteRoom` → `deleteGroupChat`
- `cancelRoomTurn` → `cancelGroupChatTurn`

- [ ] **Step 2: Update Tauri invoke strings**

Each function's `invoke()` call must use the new command name:
```typescript
// Old
invoke("list_rooms")
// New
invoke("list_group_chats")
```

- [ ] **Step 3: Rename `RoomRunIndexEntry` → `GroupChatRunIndexEntry`**

- [ ] **Step 4: Update type imports**

Change all `Room*` imports to `GroupChat*`.

- [ ] **Step 5: Verify**

```bash
npm run check 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add src/lib/api.ts
git commit -m "refactor: rename Room API functions to GroupChat"
```

### Task 19: Rename frontend store

**Files:**
- Rename: `src/lib/stores/room-store.svelte.ts` → `src/lib/stores/group-chat-store.svelte.ts`
- Rename: `src/lib/stores/room-store.test.ts` → `src/lib/stores/group-chat-store.test.ts`

- [ ] **Step 1: Rename class and file**

```bash
mv src/lib/stores/room-store.svelte.ts src/lib/stores/group-chat-store.svelte.ts
mv src/lib/stores/room-store.test.ts src/lib/stores/group-chat-store.test.ts
```

- [ ] **Step 2: Rename class and types in store**

- `RoomStore` → `GroupChatStore`
- `RoundtableSeatDraft` → `GroupChatSeatDraft` (or remove if unused)
- All `Room*` type references → `GroupChat*`
- All API function calls → new names

- [ ] **Step 3: Update test file imports**

Update imports to use new store path and new API/type names.

- [ ] **Step 4: Verify**

```bash
npm run check && npm run test 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add src/lib/stores/group-chat-store.svelte.ts src/lib/stores/group-chat-store.test.ts
git commit -m "refactor: rename room-store to group-chat-store"
```

### Task 20: Rename frontend components and utils

**Files:**
- Rename: `src/lib/components/RoomStepper.svelte` → `src/lib/components/GroupChatStepper.svelte`
- Rename: `src/lib/utils/room-ui.ts` → already deleted in Task 9

- [ ] **Step 1: Rename RoomStepper**

```bash
mv src/lib/components/RoomStepper.svelte src/lib/components/GroupChatStepper.svelte
```

- [ ] **Step 2: Update component internals**

Rename any `Room*` type references inside the component.

- [ ] **Step 3: Update imports in files that use RoomStepper**

Search for `RoomStepper` imports and update to `GroupChatStepper`.

- [ ] **Step 4: Rename agent-capabilities functions**

In `src/lib/utils/agent-capabilities.ts`, rename:
- `canUseRoomActor` → `canUseGroupChatActor`
- `canUseRoomActorRun` → `canUseGroupChatActorRun`
- `canUseRoomParticipantRun` → `canUseGroupChatParticipantRun`

Update test file accordingly.

- [ ] **Step 5: Verify**

```bash
npm run check 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/GroupChatStepper.svelte src/lib/utils/agent-capabilities.ts src/lib/utils/agent-capabilities.test.ts
git commit -m "refactor: rename RoomStepper and agent capability functions"
```

### Task 21: Create GroupChatLayout and SingleChatLayout component stubs

**Files:**
- Create: `src/lib/components/GroupChatLayout.svelte`
- Create: `src/lib/components/SingleChatLayout.svelte`
- Modify: `src/routes/chat/+page.svelte`

- [ ] **Step 1: Create `SingleChatLayout.svelte` stub**

```svelte
<script lang="ts">
  // Extract current single-chat content from +page.svelte
  // For now, just a pass-through slot
</script>

<div class="single-chat-layout">
  <slot />
</div>
```

- [ ] **Step 2: Create `GroupChatLayout.svelte` stub**

```svelte
<script lang="ts">
  import type { GroupChatDetail } from '$lib/types';
  let { groupChat }: { groupChat: GroupChatDetail } = $props();
</script>

<div class="group-chat-layout">
  <p>Group chat: {groupChat.name}</p>
  <!-- Participant panes, stepper, plan panel will be added in Phase 10.b -->
</div>
```

- [ ] **Step 3: Verify**

```bash
npm run check 2>&1 | tail -20
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/components/GroupChatLayout.svelte src/lib/components/SingleChatLayout.svelte
git commit -m "feat: add GroupChatLayout and SingleChatLayout component stubs"
```

### Task 22: Rename i18n keys

**Files:**
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

- [ ] **Step 1: Rename room_* keys to groupChat_* in en.json**

Rename all 71 `room_*` keys to `groupChat_*`. Also rename `nav_rooms` → `nav_groupChats`, `sidebar_rooms` → `sidebar_groupChats`.

- [ ] **Step 2: Rename room_* keys to groupChat_* in zh-CN.json**

Same renames as en.json.

- [ ] **Step 3: Add new keys**

Add to both files:
```json
"groupChat.newGroupChat": "New Group Chat" / "新群聊",
"groupChat.groupChats": "Group Chats" / "群聊",
"groupChat.conversations": "Conversations" / "对话"
```

- [ ] **Step 4: Verify**

```bash
npm run i18n:check 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add messages/en.json messages/zh-CN.json
git commit -m "refactor: rename room_* i18n keys to groupChat_*"
```

### Task 23: Phase 10.a.2 checkpoint — full verification + smoke test

- [ ] **Step 1: Run all checks**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run check
npm run lint
npm run test
npm run i18n:check
```

- [ ] **Step 2: Search for stale room references**

```bash
grep -r "Room[^a-z]" src-tauri/src/ --include="*.rs" | grep -v "target/" | grep -v "// " | head -20
grep -r "room[^a-z]" src/ --include="*.ts" --include="*.svelte" | grep -v "node_modules" | head -20
```

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "chore: Phase 10.a.2 complete — Room renamed to GroupChat"
```

---

## Phase 10.b: Character Library + UI Unification

### Task 24: Add AiCharacter struct and UserSettings field

**Files:**
- Modify: `src-tauri/src/models.rs`

- [ ] **Step 1: Add AiCharacter struct**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiCharacter {
    pub id: String,
    pub label: String,
    pub role_type: String,           // "planner" | "executor" | free-form
    pub role_instruction: Option<String>,
    pub default_provider: String,
    pub default_model: Option<String>,
    pub icon: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

- [ ] **Step 2: Add `ai_characters` to `UserSettings`**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    // ... existing fields ...
    #[serde(default)]
    pub ai_characters: Vec<AiCharacter>,
}
```

- [ ] **Step 3: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/models.rs
git commit -m "feat: add AiCharacter struct to UserSettings"
```

### Task 25: Add character CRUD Tauri commands

**Files:**
- Create: `src-tauri/src/commands/characters.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create `src-tauri/src/commands/characters.rs`**

```rust
use crate::models::{AiCharacter, UserSettings};
use uuid::Uuid;

#[tauri::command]
pub fn list_characters() -> Result<Vec<AiCharacter>, String> {
    let settings = UserSettings::load()?;
    Ok(settings.ai_characters)
}

#[tauri::command]
pub fn create_character(
    label: String,
    role_type: String,
    role_instruction: Option<String>,
    default_provider: String,
    default_model: Option<String>,
    icon: Option<String>,
) -> Result<AiCharacter, String> {
    let mut settings = UserSettings::load()?;
    let now = chrono::Utc::now().to_rfc3339();
    let character = AiCharacter {
        id: Uuid::new_v4().to_string(),
        label,
        role_type,
        role_instruction,
        default_provider,
        default_model,
        icon,
        created_at: now.clone(),
        updated_at: now,
    };
    settings.ai_characters.push(character.clone());
    settings.save()?;
    Ok(character)
}

#[tauri::command]
pub fn update_character(
    id: String,
    label: Option<String>,
    role_type: Option<String>,
    role_instruction: Option<Option<String>>,
    default_provider: Option<String>,
    default_model: Option<Option<String>>,
    icon: Option<Option<String>>,
) -> Result<AiCharacter, String> {
    let mut settings = UserSettings::load()?;
    let character = settings.ai_characters.iter_mut()
        .find(|c| c.id == id)
        .ok_or("Character not found")?;
    if let Some(l) = label { character.label = l; }
    if let Some(rt) = role_type { character.role_type = rt; }
    if let Some(ri) = role_instruction { character.role_instruction = ri; }
    if let Some(dp) = default_provider { character.default_provider = dp; }
    if let Some(dm) = default_model { character.default_model = dm; }
    if let Some(ic) = icon { character.icon = ic; }
    character.updated_at = chrono::Utc::now().to_rfc3339();
    let result = character.clone();
    settings.save()?;
    Ok(result)
}

#[tauri::command]
pub fn delete_character(id: String) -> Result<(), String> {
    let mut settings = UserSettings::load()?;
    let before_len = settings.ai_characters.len();
    settings.ai_characters.retain(|c| c.id != id);
    if settings.ai_characters.len() == before_len {
        return Err("Character not found".to_string());
    }
    settings.save()?;
    Ok(())
}
```

- [ ] **Step 2: Register module in commands/mod.rs**

Add `pub mod characters;` to `src-tauri/src/commands/mod.rs`.

- [ ] **Step 3: Register commands in lib.rs**

Add to the `invoke_handler` block:
```rust
commands::characters::list_characters,
commands::characters::create_character,
commands::characters::update_character,
commands::characters::delete_character,
```

- [ ] **Step 4: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/characters.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat: add character CRUD Tauri commands"
```

### Task 26: Add character API and types to frontend

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/api.ts`

- [ ] **Step 1: Add AiCharacter type to types.ts**

```typescript
export interface AiCharacter {
  id: string;
  label: string;
  role_type: string;
  role_instruction?: string;
  default_provider: string;
  default_model?: string;
  icon?: string;
  created_at: string;
  updated_at: string;
}
```

- [ ] **Step 2: Add character API functions to api.ts**

```typescript
export async function listCharacters(): Promise<AiCharacter[]> {
  return invoke("list_characters");
}

export async function createCharacter(
  label: string, roleType: string, roleInstruction: string | undefined,
  defaultProvider: string, defaultModel: string | undefined, icon: string | undefined
): Promise<AiCharacter> {
  return invoke("create_character", { label, roleType, roleInstruction, defaultProvider, defaultModel, icon });
}

export async function updateCharacter(
  id: string, updates: Partial<Omit<AiCharacter, 'id' | 'created_at' | 'updated_at'>>
): Promise<AiCharacter> {
  return invoke("update_character", { id, ...updates });
}

export async function deleteCharacter(id: string): Promise<void> {
  return invoke("delete_character", { id });
}
```

- [ ] **Step 3: Verify**

```bash
npm run check
```

- [ ] **Step 4: Commit**

```bash
git add src/lib/types.ts src/lib/api.ts
git commit -m "feat: add AiCharacter type and API functions"
```

### Task 27: Add character library UI in Settings

**Files:**
- Create: `src/routes/settings/characters/+page.svelte`
- Modify: `src/routes/settings/+layout.svelte` (add nav tab)

- [ ] **Step 1: Create character library page**

Create `src/routes/settings/characters/+page.svelte` with:
- Character card list (label, role_type, provider, instruction preview, edit/delete buttons)
- "新角色" button that opens create dialog
- Create/edit dialog with: label, role_type dropdown (planner/executor/custom), default_provider dropdown (from PHASE7_PROVIDERS), default_model, role_instruction textarea, icon
- Delete confirmation

- [ ] **Step 2: Add navigation tab**

Add "AI 角色" tab to settings layout navigation.

- [ ] **Step 3: Add i18n keys**

Add character-related keys to both `messages/en.json` and `messages/zh-CN.json`.

- [ ] **Step 4: Verify**

```bash
npm run check && npm run i18n:check
```

- [ ] **Step 5: Commit**

```bash
git add src/routes/settings/characters/ messages/
git commit -m "feat: add AI character library settings page"
```

### Task 28: Add first-launch character onboarding

**Files:**
- Modify: `src/lib/stores/group-chat-store.svelte.ts` (or new character store)

- [ ] **Step 1: Add onboarding check**

When user first opens group chat creation and `ai_characters` is empty, show a guided prompt: "还没有 AI 角色，先创建一个 Planner 角色吧" that opens the character creation dialog pre-filled with planner defaults.

- [ ] **Step 2: Verify**

Manual: open group chat creation with empty character library → see onboarding prompt.

- [ ] **Step 3: Commit**

```bash
git add src/lib/stores/group-chat-store.svelte.ts
git commit -m "feat: add first-launch character onboarding prompt"
```

### Task 29: Integrate GroupChat into /chat page

**Files:**
- Modify: `src/routes/chat/+page.svelte` (4981 lines)

- [ ] **Step 1: Add group chat detection**

At the top of the page, detect if the current run belongs to a group chat:
```typescript
import { listGroupChatRunIndex } from '$lib/api';
// Check if current run is part of a group chat
const isGroupChat = $derived(/* check run ID against group chat run index */);
```

- [ ] **Step 2: Add conditional rendering**

```svelte
{#if isGroupChat}
  <GroupChatLayout {groupChat} />
{:else}
  <!-- existing single chat content -->
{/if}
```

- [ ] **Step 3: Verify**

```bash
npm run check
```

- [ ] **Step 4: Commit**

```bash
git add src/routes/chat/+page.svelte
git commit -m "feat: add group chat detection in /chat page"
```

### Task 30: Add sidebar grouped sections

**Files:**
- Modify: `src/lib/utils/sidebar-groups.ts`
- Modify: `src/routes/+layout.svelte`

- [ ] **Step 1: Update sidebar grouping logic**

Replace the old `__rooms__` virtual folder with new grouping:
- `participant_count > 1` → "群聊" section
- `participant_count <= 1` → "对话" section
- Both sections collapsible, sorted by `updated_at` desc

- [ ] **Step 2: Add "新群聊" button**

Add a "新群聊" button alongside "新对话" in the sidebar header.

- [ ] **Step 3: Add group chat creation dialog**

Create dialog with: name + CWD (readonly input + "浏览..." button using Tauri's `dialog.open({ directory: true })` for native directory picker). No description field.

- [ ] **Step 4: Add i18n keys**

- [ ] **Step 5: Verify**

```bash
npm run check && npm run i18n:check
```

- [ ] **Step 6: Commit**

```bash
git add src/lib/utils/sidebar-groups.ts src/routes/+layout.svelte messages/
git commit -m "feat: add sidebar grouped sections and new group chat dialog"
```

### Task 31: Add participant management panel

**Files:**
- Modify: `src/lib/components/GroupChatLayout.svelte`

- [ ] **Step 1: Add participant panel**

Collapsible side panel showing:
- Participant list with role badges, provider/model info, status indicators
- "添加" button that opens character picker
- Character picker: select from library, optional provider/model override, "快速创建" for ad-hoc

- [ ] **Step 2: Add @mention autocomplete**

In the composer, add @mention autocomplete for participant names.

- [ ] **Step 3: Add @summary to composer toolbar**

- [ ] **Step 4: Verify**

```bash
npm run check
```

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/GroupChatLayout.svelte
git commit -m "feat: add participant management panel and @mention autocomplete"
```

---

## Phase 10.c: Plan Mechanism

### Task 32: Add PlanArtifact types

**Files:**
- Modify: `src-tauri/src/group_chat/models.rs`

- [ ] **Step 1: Add PlanArtifact and related types**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanArtifact {
    pub id: String,
    pub group_chat_id: String,
    pub title: String,
    pub tasks: Vec<PlanTask>,
    pub status: PlanStatus,
    pub user_notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanTask {
    pub id: String,
    pub description: String,
    pub assignee_id: Option<String>,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanStatus { Draft, Active, Completed }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus { Todo, InProgress, Done, Blocked }
```

- [ ] **Step 2: Add `active_plan_id` to GroupChat struct**

```rust
pub struct GroupChat {
    // ... existing fields ...
    pub active_plan_id: Option<String>,
}
```

- [ ] **Step 3: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/group_chat/models.rs
git commit -m "feat: add PlanArtifact and PlanTask types"
```

### Task 33: Add plan CRUD commands

**Files:**
- Create: `src-tauri/src/commands/plans.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create plan commands**

- `create_plan(group_chat_id, title, tasks)` → creates `plan.json` in `group-chats/{id}/`
- `update_plan(plan_id, title, tasks, user_notes)` → updates plan
- `approve_plan(plan_id)` → sets status to Active
- `complete_plan(plan_id)` → sets status to Completed

- [ ] **Step 2: Register commands**

- [ ] **Step 3: Add plan API functions to frontend**

- [ ] **Step 4: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml && npm run check
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/plans.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src/lib/api.ts
git commit -m "feat: add plan CRUD commands and API"
```

### Task 34: Add Plan panel UI

**Files:**
- Create: `src/lib/components/PlanPanel.svelte`
- Modify: `src/lib/components/GroupChatLayout.svelte`

- [ ] **Step 1: Create PlanPanel component**

Plan panel showing:
- Plan title and status badge
- Task checklist with assignee and status
- Approve / Execute buttons
- User notes input

- [ ] **Step 2: Integrate into GroupChatLayout**

- [ ] **Step 3: Add plan injection to executor**

When triggering an executor via `@Alice`, prepend plan context to the message.

- [ ] **Step 4: Verify**

```bash
npm run check
```

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/PlanPanel.svelte src/lib/components/GroupChatLayout.svelte
git commit -m "feat: add Plan panel UI and executor plan injection"
```

---

## Phase 10.d.1: Context Management MVP

### Task 35: Spike — session handoff feasibility

- [ ] **Step 1: Manual test — context boundary behavior**

Spawn a Claude Code session, send 25+ turns, observe behavior at context boundary.

- [ ] **Step 2: Manual test — bootstrap injection**

Seal a session, start a new one, send a structured context packet as first message. Verify the new session can continue coherently.

- [ ] **Step 3: Compare bootstrap approaches**

Test template truncation vs LLM summarization for bootstrap quality.

- [ ] **Step 4: Document findings**

Record: confirmed turn-count threshold, bootstrap approach decision, any blockers.

- [ ] **Step 5: Commit spike notes**

```bash
git add docs/
git commit -m "docs: session handoff spike findings"
```

### Task 36: Add delivery cursor tracking

**Files:**
- Create: `src-tauri/src/group_chat/context.rs`

- [ ] **Step 1: Create context module**

```rust
pub struct ParticipantMeta {
    pub delivery_cursor: usize,
    pub session_turn_count: u32,
    pub session_seq: u32,
}
```

- [ ] **Step 2: Add meta storage functions**

Read/write `participants/{id}.meta.json`.

- [ ] **Step 3: Verify**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/group_chat/context.rs
git commit -m "feat: add per-participant delivery cursor tracking"
```

### Task 37: Implement visibility filter

**Files:**
- Modify: `src-tauri/src/group_chat/context.rs`

- [ ] **Step 1: Add visibility filter function**

```rust
pub fn filter_visible_messages(
    turns: &[GroupChatTurn],
    participant_id: &str,
    mode: &GroupChatTurnMode,
) -> Vec<GroupChatTurn> { ... }
```

- [ ] **Step 2: Verify**

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/group_chat/context.rs
git commit -m "feat: implement visibility filter for participant context"
```

### Task 38: Implement session handoff

**Files:**
- Modify: `src-tauri/src/group_chat/context.rs`
- Modify: `src-tauri/src/group_chat/orchestrator.rs`

- [ ] **Step 1: Add turn count tracking**

Increment `session_turn_count` after each participant turn.

- [ ] **Step 2: Add handoff trigger**

When `session_turn_count > 25`, seal session and trigger handoff.

- [ ] **Step 3: Add bootstrap context builder**

Template-based: group chat name, character role, plan status, last 5 turns (truncated), own last response (truncated). Cap 2000 tokens.

- [ ] **Step 4: Add session restart logic**

Spawn fresh session, inject bootstrap as first message.

- [ ] **Step 5: Add fallback**

If handoff fails, keep old session sealed, surface error to user.

- [ ] **Step 6: Verify**

Manual: run 25+ turn group chat → observe session handoff → verify continuity.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/group_chat/context.rs src-tauri/src/group_chat/orchestrator.rs
git commit -m "feat: implement session handoff with bootstrap context"
```

---

## Phase 10.e: Role System Prompt + Routing

### Task 39: Implement system prompt injection

**Files:**
- Modify: `src-tauri/src/group_chat/orchestrator.rs`

- [ ] **Step 1: Add role_type → base constraint mapping**

```rust
fn build_role_system_prompt(role_type: &str, role_instruction: &Option<String>) -> String {
    let base = match role_type {
        "planner" => "你可以读取文件和搜索代码来辅助规划。你不可以执行修改文件系统或运行命令的工具。",
        "executor" => "你严格按照计划执行任务。你不可以偏离计划内容。",
        _ => "",
    };
    let custom = role_instruction.as_deref().unwrap_or("");
    format!("{}\n{}", base, custom).trim().to_string()
}
```

- [ ] **Step 2: Inject at participant spawn**

When creating a participant session, pass `--append-system-prompt` with the built system prompt.

- [ ] **Step 3: Verify**

Manual: create planner participant → verify it refuses to execute tools.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/group_chat/orchestrator.rs
git commit -m "feat: inject role system prompt at participant spawn"
```

### Task 40: Add auto-chain

**Files:**
- Modify: `src-tauri/src/group_chat/orchestrator.rs`
- Modify: `src-tauri/src/group_chat/models.rs`

- [ ] **Step 1: Add `auto_chain` field to GroupChat**

```rust
pub struct GroupChat {
    // ... existing fields ...
    pub auto_chain: bool,  // default false
}
```

- [ ] **Step 2: Add auto-chain logic to orchestrator**

After SingleTarget turn completes:
1. Scan response for `@other_participant` mentions
2. If found, trigger SingleTarget to mentioned participant
3. Depth limit: max 3 hops
4. Loop detection: stop if targeting already-chained participant
5. CancellationToken propagation

- [ ] **Step 3: Add auto-chain visual indicator**

Chain icon on auto-routed messages in timeline.

- [ ] **Step 4: Verify**

Manual: enable auto-chain → send SingleTarget with @mention → verify auto-routing.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/group_chat/orchestrator.rs src-tauri/src/group_chat/models.rs
git commit -m "feat: add auto-chain routing with depth limit and loop detection"
```

---

## Final Verification

### Task 41: Full project verification

- [ ] **Step 1: Run all checks**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run lint
npm run check
npm run test
npm run i18n:check
npm run build
```

- [ ] **Step 2: Smoke test**

1. Create a character in Settings → AI 角色
2. Create a new group chat
3. Add character as participant
4. Send a message → verify response
5. Edit character → verify "下次 session 启动时生效" tooltip
6. Create a plan → verify plan panel
7. Test @debate, @summary, /dm routing

- [ ] **Step 3: Update spec status**

Change spec status to "Implemented".

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "chore: Group Chat refactor complete — all phases verified"
```
