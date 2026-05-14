# Character Memory System — Design Spec

**Date:** 2026-05-14
**Status:** Foundation implemented — Tasks 0-10 done, Tasks 11-17 pending
**Review:** 3 rounds complete (R6 multi-review, R7 multi-review, Simplify 3-way)

## Overview

Redesign the AiCharacter system from lightweight persona templates into **heavyweight persistent characters**, each with an independent memory system backed by LanceDB vector search and a graphology knowledge graph. Characters accumulate knowledge, experiences, preferences, and relationships across group chat sessions, and retrieve relevant memories to inject as system prompt context during orchestration.

## Motivation

Current `AiCharacter` is a flat template (label, role_type, role_instruction, provider, model). There is no persistent memory — role prompts are static. Characters "forget" everything after each session. The goal is to give each character a persistent "brain" that:
- Accumulates knowledge from conversations
- Supports semantic search via embeddings
- Models relationships between memories as a knowledge graph
- Detects knowledge gaps and clusters
- Injects relevant memories into group chat context

## Tech Stack

| Layer | Technology | Rationale |
|---|---|---|
| Vector DB | **LanceDB** (Rust SDK, embedded) | Already validated in trading-review-wiki Tauri v2 app; file-based, zero ops |
| Embeddings | **External API** (user-configured, OpenAI-compatible) | No local model dependency; supports Ollama/llama.cpp/any compatible endpoint |
| Knowledge Graph (computation) | **petgraph** (Rust, backend) | Graph traversal for injection, 4-signal relevance, Louvain community detection, gap analysis |
| Knowledge Graph (visualization) | **sigma.js** + **graphology** (TypeScript, frontend) | ForceAtlas2 layout, interactive zoom/pan; read-only consumer of backend-computed graph |
| Memory Formation | **LLM CoT ingestion pipeline** | Pattern borrowed from trading-review-wiki ingest; SHA256 dedup |
| Storage | **File-based JSON + JSONL + LanceDB** | Follows existing project patterns; no database server |

## Data Models

### AiCharacter (upgraded)

```typescript
interface AiCharacter {
  // --- existing fields, unchanged ---
  id: string;
  label: string;
  role_type: string;
  role_instruction?: string;
  default_provider: string;
  default_model?: string;
  created_at: string;
  updated_at: string;

  // --- new fields ---
  avatar_path?: string;          // local path to uploaded avatar image (jpg/png)
  personality?: string;          // free-text personality/character description
  expertise?: string[];          // domain expertise tags
  memory_config?: {
    auto_learn: boolean;         // auto-extract memories from conversations (default: true)
    retention_days?: number;     // memory retention period (null/undefined = permanent)
  };
}
```

### EmbeddingConfig (in UserSettings)

```typescript
interface EmbeddingConfig {
  enabled: boolean;
  endpoint: string;       // OpenAI-compatible embedding API endpoint
  api_key?: string;       // independent API key (falls back to provider key if empty)
  model: string;          // e.g. "text-embedding-3-small", "bge-large-zh"
}
```

### MemoryNode

```typescript
interface MemoryNode {
  id: string;
  character_id: string;
  content: string;              // memory text
  type: "fact" | "experience" | "preference" | "rule" | "relationship";
  confidence: number;           // 0.0–1.0
  source: {
    kind: "chat" | "manual" | "inference";
    run_id?: string;
    group_chat_id?: string;
  };
  tags: string[];
  created_at: string;
  updated_at: string;
}
```

### MemoryEdge

```typescript
interface MemoryEdge {
  id: string;
  source_id: string;
  target_id: string;
  relation: "supports" | "contradicts" | "extends" | "related" | "causes";
  weight: number;  // 0.0–1.0
}
```

## Storage Layout

```
~/.claw-go/characters/{character_id}/
├── character.json          # AiCharacter metadata (migrated OUT of settings.json)
├── avatar.png              # uploaded avatar image
├── lancedb/                # LanceDB vector index
├── memory-graph.json       # knowledge graph (nodes + edges, graphology-compatible)
└── memory-log.jsonl        # append-only memory event log (replayable)
```

- Characters are no longer stored inline in `settings.json`; they get dedicated directories
- `settings.json` retains a minimal `ai_characters` reference list (`[{id, label}]`) for backward compatibility

### Data Hierarchy — Single Source of Truth

`memory-log.jsonl` is the **authoritative data source**. LanceDB and `memory-graph.json` are **derived data** that can be rebuilt from the log at any time.

| File | Role | Rebuildable? |
|---|---|---|
| `memory-log.jsonl` | Authoritative event log (append-only) | — |
| `lancedb/` | Vector index for semantic search | Yes — rebuild via `rebuild_vector_index(character_id)` |
| `memory-graph.json` | Knowledge graph (nodes + edges) | Yes — rebuild via `rebuild_graph(character_id)` |

- All CRUD operations **write to memory-log.jsonl first**, then update derived stores
- If a write to LanceDB or graph.json fails, the log entry is still valid — derived stores can be repaired
- On app startup, optionally verify derived store integrity against the log (`log.count == lancedb.count`)
- A `rebuild_all_derived_data(character_id)` command is exposed for manual repair
- Write order: (1) append log → (2) update graph → (3) LanceDB upsert. If step 2 or 3 fails, log the error and continue — next write heals

### Graph Computation Split — Backend vs Frontend

| Responsibility | Where | Why |
|---|---|---|
| Graph traversal (for injection) | **Backend** (Rust, `petgraph` crate) | Injection happens in orchestrator; can't depend on frontend state |
| 4-signal relevance computation | **Backend** (Rust) | Edges are computed when new memories are added, before frontend reads |
| Community detection (Louvain) | **Backend** (Rust, `petgraph` algo port) | Results needed by both injection logic and UI |
| Knowledge gap detection | **Backend** (Rust) | Based on graph structure, not visualization |
| Graph visualization (sigma.js) | **Frontend** (TypeScript) | Pure rendering — reads `memory-graph.json` via Tauri command |
| Interactive node selection | **Frontend** (TypeScript) | Click to show memory detail, filter, zoom |

- Backend owns the graph's truth (`memory-graph.json`) and all computation
- Frontend is a read-only consumer of the graph file for visualization
- When user manually creates/edits/deletes a memory from UI, frontend calls Tauri command → backend updates log → backend recomputes affected graph edges → backend writes graph.json → frontend re-reads

## Memory Formation Pipeline

### Source 1: Auto-extraction from group chat

1. After a group chat turn completes, collect the last N public turns
2. Send to LLM with Chain-of-Thought prompt: "Extract knowledge valuable to this character: facts, experiences, preferences, rules, relationships"
3. LLM returns structured MemoryNode list (JSON)
4. **Semantic dedup** — embed the candidate memory text, compute cosine similarity against existing memory vectors. Skip if similarity > 0.92 against any existing node for the same character. (SHA256 is byte-level only — cosine similarity catches "User prefers brevity" vs "用户喜欢简洁")
5. Append to `memory-log.jsonl`
6. Incrementally update `memory-graph.json` (add nodes, compute edges via 4-signal relevance)
7. LanceDB upsert for each new node
8. User can review/edit/delete in the memory panel

**Auto-Extraction async model:**
- Extraction is a **post-message background task** — never inline during message delivery
- Trigger: after orchestrator dispatches all participant turns, a `tokio::spawn` fires the extraction pipeline
- Model: uses the character's `default_provider` and `default_model` for the extraction LLM call
- Cost control:
  - Max 1 extraction per group chat per 5 minutes (debounce)
  - Max 10 extractions per character per day (daily cap)
  - Token budget per extraction: ~4000 input + ~1000 output
  - Each extraction produces at most 3 MemoryNodes
- If extraction fails (LLM error, timeout), silently skip — no user notification needed
- Extracted memories go into a `review_queue` (visible in memory panel as "待审核"), user can approve/reject before they're committed to the authoritative log
- Review queue entries expire after 7 days if not acted on (auto-purge)

### Source 2: Manual creation

1. User opens memory panel, clicks "手动添加"
2. Fills form: content, type, confidence, tags, optional relation to existing memory
3. Directly writes to `memory-log.jsonl` + updates graph + LanceDB upsert

## Memory Retrieval (Hybrid Search)

When a character is invoked in group chat (via `@mention`):

1. **Vector search** — embed the user's query + recent context, search LanceDB for top-K
2. **Graph expansion** — from matched nodes, traverse 1–2 hops to pull in connected memories
3. **Keyword scoring** — BM25-style boost for exact term matches
4. **Merge & rank** — weighted combination → top-N memories
5. **Inject** — format memories into system prompt alongside role instruction

Configurable parameters:
- `max_retrieval_count` (default: 5)
- `relevance_threshold` (default: 0.6)
- `graph_expansion_hops` (default: 1)

### Graceful Degradation — Embedding API Failure

The injection path must never block group chat message delivery. Degradation tiers:

| Tier | Condition | Behavior | Latency Impact |
|---|---|---|---|
| **Full** | Embedding API healthy | Vector search + graph expansion + keyword scoring | Full |
| **Degraded** | Embedding API timeout (>3s) or error | Skip vector search; use keyword BM25 + recency boost + graph-only expansion | No embedding latency |
| **Minimal** | Both embedding API down AND graph file corrupt/missing | Keyword BM25 + recency boost + type filter only | Fastest |
| **Skip** | `memory_config.auto_learn = false` OR embedding globally disabled | No memory injection; pass through to role prompt only | Zero |

- Embedding API call has a **3-second hard timeout** — if exceeded, immediately fall to Degraded tier
- The orchestrator does NOT retry the embedding call within the same turn — retry happens lazily on next turn
- Embedding health is cached for 60 seconds — if the last call failed, skip embedding for subsequent turns in the same minute
- User sees an indicator in the group chat UI when memory injection is degraded: "记忆检索降级 (关键词模式)"

### Context Window Guard — Injection Token Budget

Injected memories must not overflow the model's context window. Guard rails:
- `max_injection_tokens` (default: 2000) — total tokens for all injected memories combined
- Each memory is truncated to `max_tokens_per_memory` (default: 300) before injection
- Memories are sorted by relevance score, then greedily packed until the budget is exhausted
- If the role instruction + personality + injected memories exceed 70% of the model's context window, truncate oldest/lowest-relevance memories first
- Injection budget is configurable per character (smaller budgets for models with limited context)
- Dry-run: the frontend Memory Panel shows a preview of what would be injected for a given query, with token counts

## Data Lifecycle — Compaction & Retention

### Log Compaction
- `memory-log.jsonl` grows unbounded with append-only writes
- Background compaction job runs on app startup (async, non-blocking):
  1. Read all log entries into memory
  2. Remove entries marked as deleted (tombstone events)
  3. Merge edit events into their original entries (keep only the latest version)
  4. Write compacted log to `memory-log-compacted.jsonl.tmp`
  5. Atomic rename to replace the original
- Compaction triggered when log exceeds 10,000 lines OR 30 days since last compaction
- During compaction, writes are buffered and flushed after the rename
- `rebuild_all_derived_data` is automatically invoked after compaction

### Retention
- `memory_config.retention_days` — if set, memories older than this are tombstoned on compaction
- Default: `null` = permanent retention
- Tombstone entries remain in the log for 1 more compaction cycle before physical removal
- Manual "清空记忆" in the UI goes through the same tombstone-then-compact flow (soft delete, not instant removal)

## Knowledge Graph Features

Borrowed directly from trading-review-wiki patterns:

### 4-Signal Relevance Model (graph-relevance.ts)

| Signal | Weight | Description |
|---|---|---|
| Direct link | 3.0 | Page A links to B or vice versa |
| Source overlap | 4.0 | Number of shared source sessions |
| Common neighbors (Adamic-Adar) | 1.5 | Normalized shared neighbor count |
| Type affinity | 1.0 | Predefined type affinity matrix |

### Community Detection (Louvain)

- Uses `graphology-communities-louvain` to auto-detect knowledge clusters
- Displays community cohesion scores and member nodes

**Louvain port note:** graphology's Louvain is TypeScript. petgraph does not ship with Louvain. Implementation approach:
- Port the Louvain algorithm to Rust using petgraph's graph API — a well-understood algorithm (~200 lines)
- Alternative: run Louvain in frontend on graphology as a one-time compute → send community assignments back to backend for storage
- Decision deferred to Phase 4 implementation — whichever path is chosen, the backend owns the resulting community data

### Knowledge Gap Detection

- **Isolated nodes** — degree ≤ 1, suggest connections
- **Sparse communities** — cohesion < 0.15 with ≥ 3 nodes
- **Bridge nodes** — connected to ≥ 3 communities (high-value hubs)

### Surprise Connections

- Cross-community edges, cross-type edges, periphery-to-center edges
- Scored and surfaced for review

## New Tauri Commands

```rust
// Character memory CRUD
list_character_memories(character_id, filter?) -> Vec<MemoryNode>
get_character_memory(character_id, memory_id) -> MemoryNode
create_character_memory(character_id, content, type, confidence, tags, relations) -> MemoryNode
update_character_memory(character_id, memory_id, updates) -> MemoryNode
delete_character_memory(character_id, memory_id) -> ()

// Hybrid search
search_character_memories(character_id, query, top_k, threshold, graph_hops) -> Vec<MemoryNode>

// Knowledge graph
get_memory_graph(character_id) -> GraphData
get_memory_communities(character_id) -> Vec<CommunityInfo>
get_knowledge_gaps(character_id) -> Vec<GapInfo>

// Auto-extraction (async, triggered after group chat turn)
extract_memories_from_turns(character_id, turns: Vec<TurnData>) -> Vec<MemoryNode>

// Vector store (shared with trading-review-wiki pattern)
vector_upsert(character_id, page_id, vector: Vec<f32>)
vector_search(character_id, query_vector: Vec<f32>, top_k: u32) -> Vec<SearchResult>
vector_delete(character_id, page_id)

// Character avatar
upload_character_avatar(character_id, file_path: String) -> String  // returns stored path

// Embedding config
get_embedding_config() -> EmbeddingConfig
update_embedding_config(config) -> EmbeddingConfig
test_embedding_connection() -> TestResult
```

## UI Pages

### 1. Character Editor Upgrade (`/settings/characters`)
- Avatar upload (jpg/png) with preview, hover-to-replace
- New fields: personality, expertise tags, memory config (auto_learn toggle, retention_days)
- "Manage Memory" button per character → opens memory panel

### 2. Character Memory Panel (new)
- **Tabs:** 全部记忆 / 知识图谱 / 知识缺口 / 社区 Community
- **Left pane:** Interactive knowledge graph (sigma.js + ForceAtlas2, zoom/pan/click)
- **Right pane:** Memory list with type badges, confidence bars, source traces
- Search bar with hybrid retrieval
- "手动添加" button → modal form
- Footer: export/clear memory, navigation between characters

### 3. Embedding Config (`/settings` → 角色记忆 section)
- Enable/disable toggle (preserves data, stops updates)
- API endpoint, API key (optional, fallback to provider), model name
- Quick presets (OpenAI, Ollama, BGE, DeepSeek)
- "Test Connection" button with latency/dimension display

### 4. Group Chat Memory Injection (existing GroupChatLayout enhancement)
- During orchestration, before sending prompt to a character:
  - Build embedding from user message + context
  - Retrieve top-K relevant memories
  - Inject formatted memories into system prompt
- Injection config (max count, threshold, graph hops) exposed in group chat settings

## Migration Path

### Character Storage Migration

1. Existing `AiCharacter[]` in `settings.json` stays; new fields are optional with defaults
2. On first access to a character's memory, create the `characters/{id}/` directory lazily

### Label→ID Linkage Migration (Startup Scan, No Fallback)

1. `GroupChatParticipant` already has `character_id: String` in TypeScript (always `""` currently) — add the field to the Rust struct
2. On **every app startup**, scan all group chats for participants with empty `character_id`:
   - Match `participant.label` against `AiCharacter.label` (case-insensitive)
   - If match found, set `character_id = character.id`
   - If no match, set `character_id = "__orphan__"` to indicate unlinked
   - Write migrated data back to `group_chat.json` and participant meta files
   - This is a scan, not a one-time migration — CC session imports, plugins, and manual edits can introduce new participants without `character_id`
3. `resolve_participant_system_prompt` in `orchestrator.rs` switches to **ID-based lookup only** — `ai_characters.iter().find(|c| c.id == participant.character_id)`
4. **Remove label-based fallback entirely** — no runtime matching. If `character_id == "__orphan__"`, skip memory injection for that participant
5. Character rename no longer breaks the linkage — ID is immutable
6. All new participants created via `addCharacterParticipant` must have `character_id` populated at creation time

## Implementation Phases

**Pre-Phase-0: Dependency Spike (必须先行)**
- Add `lancedb` + `arrow` to `Cargo.toml`, run `cargo check` + `cargo build` on Windows MSVC
- Verify LanceDB can compile and link in ClawGO's specific dependency tree (not just trading-review-wiki's)
- If LanceDB fails to build, fallback plan: `sqlite-vec` (SQLite extension for vector search, pure C, broader Windows compatibility)
- Budget: 1 day. Go/no-go decision point.

0. **Review Queue Note:** Auto-extraction goes into a review queue first; user must approve/reject before memories enter the authoritative log. Expires after 7 days.

1. **Foundation** — LanceDB integration (`lancedb` + `arrow` crates), vectorstore commands (upsert/search/delete/rebuild), EmbeddingConfig model + settings UI
2. **Storage** — Character directory structure, `memory-log.jsonl` as authoritative source, MemoryNode/Edge CRUD, derived store rebuild commands
3. **Migration** — One-time label→ID linkage migration, `character_id` field added to Rust `GroupChatParticipant`, ID-based lookup in orchestrator, remove label fallback
4. **Knowledge Graph (Backend)** — `petgraph` crate for graph structure, 4-signal relevance edge computation, Louvain community detection port, gap detection, graph traversal for injection
5. **Memory Retrieval + Injection** — Hybrid search (vector + graph + keyword), graceful degradation tiers, prompt formatting with length control, injection config in group chat settings
6. **Memory Panel UI** — sigma.js graph visualization (read-only consumer), memory list with type badges/confidence bars, manual add modal, tabs (全部记忆/知识图谱/知识缺口/社区 Community), search
7. **Auto-Extraction** — LLM CoT ingestion pipeline, SHA256 semantic dedup, async trigger after group chat turn, user review queue
8. **Character Editor Upgrade** — avatar upload (jpg/png), personality/expertise/memory_config fields, "管理记忆" entry button
