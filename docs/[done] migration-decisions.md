# Hub Capability Migration Decisions

**Date:** 2026-04-30
**Status:** Done
**Purpose:** Track how each meaningful Claude Session Hub capability maps into the Claw GO remaster.

## Progress Snapshot

Last updated: 2026-05-05 after native Codex/Gemini chat parity and connection-profile UI parity.

- Memo protocol: migrated in Phase 1.a / 1.b.
- Room membership and room memo foundation: migrated in Phase 2.
- AgentAdapter boundary: introduced in Phase 2 for run-backed Claude participants.
- Spawn environment resolver: Phase 2.x backend merged. Local Claude session actor, Codex pipe-exec, `fork_session`, and side-question one-shot now share the Windows MSVC env resolver. This is not a direct Hub capability migration; it is Windows migration friction removal discovered while integrating Hub workflows.
- Roundtable, Driver/Copilot, Research, Arena Memory candidates, the Phase 5 capability matrix, and Phase 5.5 native Codex/Gemini chat parity are merged.
- Driver MCP is in progress as Phase 6.a with a room-local bundle; stdio server wiring remains pending.

## Decision Categories

| Category | Meaning |
|----------|---------|
| Migrate | Rebuild the product protocol or UX natively in Claw GO. |
| Reference | Keep as design input, but do not port directly. |
| Drop | Claw GO already has a better native path, or the capability is no longer needed. |

## Migrate

| Hub Source | Decision | Rationale | Target |
|------------|----------|-----------|--------|
| `core/meeting-room.js` | Migrate | Room membership, timeline, cursor, and send-target ideas are central to multi-run collaboration. | `room/*`, `storage/rooms.rs`, `/rooms` |
| `core/roundtable-orchestrator.js` | Migrate | Fanout / debate / summary is a valuable collaboration protocol. | `room/orchestrator.rs` |
| `core/general-roundtable-mode.js` | Migrate | General-purpose syntax is reusable beyond investment/research use cases. | Roundtable room prompt templates |
| `core/general-roundtable-private-store.js` | Migrate | Private messages must stay out of public round history. | `rooms/{id}/private.json` |
| `core/driver-mode.js` | Migrate | Driver/Copilot is the strongest engineering workflow concept in Hub. | Driver room |
| `core/arena-memory/` | Migrate | Project facts / lessons / decisions should remain separate from personal CLI memory. | `rooms/{id}/arena/`, project `.arena/memory` |
| `renderer/renderer.js` (memo block) | Migrate | The Hub memo UX is the seed for Claw GO's three-tier memo (global / project / room). | `storage/memos.rs`, memo Svelte panel |
| `core/research-mode.js` | Migrate | Research Mode is a sibling collaboration protocol, not a minor roundtable variant. | Research room |
| `core/driver-mcp-server.js` | In progress | MCP review tools are how Driver/Copilot ships in production. Phase 6.a starts with a structured room-local `.arena/driver-mcp.json` bundle; server wiring remains later. | Driver room MCP bundle / later `room/driver/mcp.rs` |
| `core/research-mcp-server.js` | Migrate (after Research MVP) | Same pattern — MVP runs without MCP, full Research Room ports these tools. | `room/research/mcp.rs` |
| `core/data-dir.js` | Migrated | Env-driven data-dir override is useful for isolated E2E tests and parallel dev runs. | `storage::data_dir()` with `CLAWGO_DATA_DIR` |

## Native Architecture Additions

These are not direct Hub module migrations, but they support the remaster path and reduce migration friction for Hub-style workflows.

| Capability | Status | Rationale | Target |
|------------|--------|-----------|--------|
| Windows MSVC spawn environment resolver | Merged with Settings/status UI | Hub users often hit Windows native build failures when Claude/Codex child processes start outside Developer PowerShell. Claw GO should make local child CLI spawns usable from the normal desktop window. | `src-tauri/src/agent/windows_msvc_env.rs`, local spawn paths |
| Native Codex/Gemini chat parity | Merged on 2026-05-05 | Hub-style multi-agent workflows need Codex and Gemini to be usable as first-class chat seats, but Claw GO should keep native CLI pipe-exec semantics instead of adding a PTY wall. | `src-tauri/src/agent/stream.rs`, `src/lib/stores/session-store.svelte.ts`, `/chat`, Settings connection panels |

## Reference

| Hub Source | Decision | Rationale | Target |
|------------|----------|-----------|--------|
| `core/mobile-protocol.js` | Reference | PWA and Tailscale usage patterns are useful, but Claw GO already has a Rust web server. | Web remote UX notes |
| `core/deep-summary-service.js` | Reference | Summary orchestration may inform future room summaries. | Later summary backlog |
| `core/summary-providers/` | Reference | Provider fallback ideas are useful; implementation should follow Claw GO settings/provider architecture. | Later summary provider design |
| `core/meeting-store.js` | Reference | Atomic temp-file persistence, schema versioning, dirty debounce, and cancel-dirty behavior are useful test inputs for Room storage. | `storage/rooms.rs` tests |
| `core/deep-summary-config.js` | Reference | Summary settings may inform later room-summary configuration. | Later room summary settings |
| `core/summary-engine.js` | Reference | Summary orchestration is useful as product behavior reference; implementation should stay native to Room and AgentAdapter. | Later room summary orchestration |
| `core/summary-parser.js` | Reference | Parser patterns may help with fallback summaries, but structured run events should remain the primary source. | Later summary parsing fallback |
| `core/summary-prompt.js` | Reference | Prompt structure may be reused as source material, not copied as a fixed contract. | Later room summary templates |
| `core/usage-filter.js` | Reference | Monotonic filtering can inform future real-time usage badges; Claw GO's native stats path remains canonical. | Future usage UI polish |

## Drop

| Hub Source | Decision | Rationale | Replacement |
|------------|----------|-----------|-------------|
| `core/mobile-server.js` | Drop | Claw GO has `web_server` with axum, cookie/token auth, and WS broadcast. | `src-tauri/src/web_server` |
| `core/mobile-routes.js` | Drop | Same as above. | `src-tauri/src/web_server` |
| `core/mobile-auth.js` | Drop | Same as above. | Existing web auth/token model |
| `core/transcript-tap.js` | Drop | Room should not depend on PTY transcript tapping or marker fallback. | Run events + AgentAdapter |
| `core/ansi-utils.js` | Drop | Claw GO already has frontend ANSI/xterm utilities. | Existing `src/lib/utils/ansi.ts`, xterm components |
| `core/state-store.js` | Drop | Claw GO has structured Rust storage. | `storage/*` |
| `~/.claude/scripts/session-hub-hook.py` (deployed by Hub) | Drop | Claw GO receives stream-JSON events from Claude CLI directly; no external hook script needed. | Existing event stream |
| `~/.claude/scripts/claude-hub-statusline.js` (deployed by Hub) | Drop | Claw GO computes context / usage from CLI events natively. | Existing usage tracking |
| `core/session-archive.js` | Drop | Claw GO already has native CLI session import/history paths; do not reintroduce Hub's `~/.claude/projects/*/*.jsonl` archive scanner. | `storage/cli_sessions.rs`, `commands/cli_sync.rs`, history/search UX |
| `core/lindang-bridge.js` | Drop | Product-specific A-share data bridge does not belong in Claw GO core. | Optional plugin/MCP integration if needed later |

## Non-Goal

No Hub module should be copied wholesale into Claw GO. Even migrated items are protocol and UX migrations, implemented in the native Claw GO architecture.

## Pending Status

All initially identified Pending items were resolved on 2026-04-30 after Opus 4.7 review confirmation.

Future Hub files should be classified directly into Migrate / Reference / Drop before the phase that depends on them starts. If a new item genuinely cannot be decided yet, add a short phase gate and remove it as soon as the decision is made.
