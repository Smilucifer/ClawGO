# Phase 4.5 Research Follow-up Implementation Plan

> **[废弃 2026-06-20]** 本计划的 Group Chat Research/Driver 产物已从代码树移除（Rust 类型、storage、turn 枚举变体、前端消费者均不存在）。文档保留作历史记录，不代表现状。

**Feature:** Phase 4.5 follow-up - Full Research Room artifact and Arena Memory candidates
**Status:** Implemented locally on 2026-05-02 in `feat/research-followup`.
**Goal:** Turn the Research Room MVP output into a reusable artifact stream that can preserve history and surface durable facts, decisions, and lessons as Arena Memory candidates.
**Acceptance Criteria:**
- Research artifacts keep `artifact.json` as the latest snapshot.
- Research artifacts append every generated artifact to `research/artifacts.jsonl`.
- Research prompts request `[fact]`, `[decision]`, and `[lesson]` markers for durable project knowledge.
- Marked participant output is parsed into Arena Memory candidates with source room turn/run references.
- `RoomDetail` exposes the latest research artifact to `/rooms`.
- Existing Research turn behavior still writes the artifact before appending the public room turn.
**Architecture:** Keep Run as the canonical output source. Research Room stores references and previews, then derives candidate memory records from marked preview lines without duplicating full participant transcripts into room storage.
**Tech Stack:** Rust/Tauri backend, room storage JSON/JSONL, Svelte 5 room UI, Vitest/Rust unit tests.
**Frontend Verification:** Yes - `/rooms` renders the latest Research artifact and memory candidates when present.

---

## Finish Line

The follow-up is complete when each Research turn writes a schema-versioned latest artifact, appends an artifact history entry, extracts marked Arena Memory candidates, and exposes the latest artifact in the room detail UI.

Out of scope:

- Promotion workflow from candidate to permanent project `.arena/memory`.
- Full transcript-level fact extraction beyond currently stored response previews.
- Research MCP server port.
- Phase 5 multi-CLI capability expansion.

## Verification Evidence

Recorded during local implementation:

```powershell
cargo test research_ --manifest-path src-tauri/Cargo.toml
npm test -- src/lib/stores/room-store.test.ts --run
npm run i18n:check
npm run lint
```

Remaining gate before merge:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings
npm run build
git diff --check
```
