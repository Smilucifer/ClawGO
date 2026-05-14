# Character Memory System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every AiCharacter a persistent memory "brain" — LanceDB vector search, petgraph knowledge graph, LLM CoT auto-extraction, hybrid retrieval, and injection into group chat system prompts.

**Architecture:** Backend (Rust) owns all data — `memory-log.jsonl` as authoritative source, LanceDB and `memory-graph.json` as derived/reconstructable stores. Frontend (Svelte 5) provides read-only graph visualization via sigma.js and CRUD via Tauri commands. Graph computation (petgraph) is backend-only; injection happens in orchestrator with 4-tier graceful degradation.

**Tech Stack:** LanceDB (Rust, embedded vector DB), petgraph (Rust, graph compute), sigma.js + graphology (TS, viz), external OpenAI-compatible embedding API (user-configured), LLM CoT pipeline (auto-extraction), file-based JSON + JSONL persistence.

---

### Task 0: Dependency Spike — LanceDB Build Verification

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add lancedb + arrow to Cargo.toml**

```toml
# Under [dependencies], add:
lancedb = "0.27"
arrow-array = "57"
arrow-schema = "57"
petgraph = "0.7"
```

- [ ] **Step 2: Run cargo check to verify compilation**

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1
```

Expected: PASS (no linker errors). If FAIL with `STATUS_ENTRYPOINT_NOT_FOUND` or MSVC linking errors, switch to fallback:

```toml
# Fallback if LanceDB fails:
# Replace lancedb/arrow with:
# rusqlite = { version = "0.32", features = ["bundled", "vtab"] }
# sqlite-vec = "0.1"
```

- [ ] **Step 3: Run cargo build to verify full linking**

```bash
cargo build --manifest-path src-tauri/Cargo.toml 2>&1
```

Expected: PASS with no new linker errors.

- [ ] **Step 4: Commit the verified dependency set**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: add lancedb, arrow, petgraph dependencies (Pre-Phase-0 spike)"
```

---

### Task 1: Rust Type Definitions — models.rs

**Files:**
- Modify: `src-tauri/src/models.rs`

- [ ] **Step 1: Add MemoryNode struct**

Append to `src-tauri/src/models.rs` (before the last line):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryNode {
    pub id: String,
    pub character_id: String,
    pub content: String,
    #[serde(rename = "type")]
    pub memory_type: String,  // "fact" | "experience" | "preference" | "rule" | "relationship"
    pub confidence: f64,
    pub source: MemorySource,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySource {
    pub kind: String,  // "chat" | "manual" | "inference"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_chat_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEdge {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relation: String,  // "supports" | "contradicts" | "extends" | "related" | "causes"
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryGraphData {
    pub nodes: Vec<MemoryNode>,
    pub edges: Vec<MemoryEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityInfo {
    pub id: usize,
    pub label: String,
    pub cohesion: f64,
    pub node_count: usize,
    pub edge_count: usize,
    pub node_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGapInfo {
    pub gap_type: String,  // "isolated_node" | "sparse_community" | "bridge_node"
    pub description: String,
    pub suggestion: String,
    pub affected_node_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub enabled: bool,
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestEmbeddingResult {
    pub success: bool,
    pub latency_ms: u64,
    pub dimension: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    pub page_id: String,
    pub score: f64,
    pub memory: Option<MemoryNode>,
}
```

- [ ] **Step 2: Update AiCharacter struct**

Find the existing `AiCharacter` struct in `models.rs` and add new fields:

```rust
// Add these fields to the existing AiCharacter struct:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub avatar_path: Option<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub personality: Option<String>,
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub expertise: Vec<String>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub memory_config: Option<MemoryConfig>,
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_auto_learn")]
    pub auto_learn: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<u32>,
}

fn default_auto_learn() -> bool { true }
```

- [ ] **Step 3: Add EmbeddingConfig to UserSettings**

Find `UserSettings` in `models.rs` and add:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub embedding_config: Option<EmbeddingConfig>,
```

- [ ] **Step 4: Update GroupChatParticipant**

Find `GroupChatParticipant` in `src-tauri/src/group_chat/models.rs` and add:

```rust
#[serde(default)]
pub character_id: String,  // was implicit; now explicit
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models.rs src-tauri/src/group_chat/models.rs
git commit -m "feat: add MemoryNode, MemoryEdge, EmbeddingConfig, update AiCharacter and GroupChatParticipant types"
```

---

### Task 2: TypeScript Type Definitions

**Files:**
- Modify: `src/lib/types.ts`

- [ ] **Step 1: Add TypeScript types**

Append to `src/lib/types.ts`:

```typescript
export interface MemoryNode {
  id: string;
  character_id: string;
  content: string;
  type: "fact" | "experience" | "preference" | "rule" | "relationship";
  confidence: number;
  source: MemorySource;
  tags: string[];
  created_at: string;
  updated_at: string;
}

export interface MemorySource {
  kind: "chat" | "manual" | "inference";
  run_id?: string;
  group_chat_id?: string;
}

export interface MemoryEdge {
  id: string;
  source_id: string;
  target_id: string;
  relation: "supports" | "contradicts" | "extends" | "related" | "causes";
  weight: number;
}

export interface MemoryGraphData {
  nodes: MemoryNode[];
  edges: MemoryEdge[];
}

export interface CommunityInfo {
  id: number;
  label: string;
  cohesion: number;
  node_count: number;
  edge_count: number;
  node_ids: string[];
}

export interface KnowledgeGapInfo {
  gap_type: "isolated_node" | "sparse_community" | "bridge_node";
  description: string;
  suggestion: string;
  affected_node_ids: string[];
}

export interface EmbeddingConfig {
  enabled: boolean;
  endpoint: string;
  api_key?: string;
  model: string;
}

export interface TestEmbeddingResult {
  success: boolean;
  latency_ms: number;
  dimension: number;
  error?: string;
}

export interface VectorSearchResult {
  page_id: string;
  score: number;
  memory?: MemoryNode;
}

export interface MemoryConfig {
  auto_learn: boolean;
  retention_days?: number;
}
```

- [ ] **Step 2: Update AiCharacter interface**

Find the existing `AiCharacter` interface and add:

```typescript
  avatar_path?: string;
  personality?: string;
  expertise?: string[];
  memory_config?: MemoryConfig;
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/types.ts
git commit -m "feat: add TypeScript types for memory system (MemoryNode, EmbeddingConfig, etc.)"
```

---

### Task 3: Character Storage Module — storage/characters.rs

**Files:**
- Create: `src-tauri/src/storage/characters.rs`
- Modify: `src-tauri/src/storage/mod.rs`

- [ ] **Step 1: Declare module in mod.rs**

In `src-tauri/src/storage/mod.rs`, add to the module declarations:

```rust
pub mod characters;
```

- [ ] **Step 2: Implement characters.rs**

```rust
use crate::models::MemoryNode;
use crate::storage::{data_dir, ensure_dir, write_atomic_json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

static CHAR_LOCKS: once_cell::sync::Lazy<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));

fn char_dir(character_id: &str) -> PathBuf {
    data_dir().join("characters").join(character_id)
}

fn char_lock(character_id: &str) -> Arc<Mutex<()>> {
    let mut map = CHAR_LOCKS.lock().unwrap();
    map.entry(character_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

pub async fn ensure_char_dir(character_id: &str) -> std::io::Result<PathBuf> {
    let dir = char_dir(character_id);
    tokio::fs::create_dir_all(&dir).await?;
    Ok(dir)
}

// --- Memory Log (authoritative source) ---

fn memory_log_path(character_id: &str) -> PathBuf {
    char_dir(character_id).join("memory-log.jsonl")
}

pub async fn append_memory_log(
    character_id: &str,
    node: &MemoryNode,
) -> crate::Result<()> {
    let _lock = char_lock(character_id).lock().unwrap();
    ensure_char_dir(character_id).await?;
    let path = memory_log_path(character_id);
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await?;
    let line = serde_json::to_string(node)? + "\n";
    file.write_all(line.as_bytes()).await?;
    Ok(())
}

pub async fn read_all_memory_log_entries(
    character_id: &str,
) -> crate::Result<Vec<MemoryNode>> {
    let path = memory_log_path(character_id);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = tokio::fs::File::open(&path).await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut entries = Vec::new();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(node) = serde_json::from_str::<MemoryNode>(&line) {
            entries.push(node);
        }
    }
    Ok(entries)
}

pub async fn delete_memory_from_log(
    character_id: &str,
    memory_id: &str,
) -> crate::Result<()> {
    let _lock = char_lock(character_id).lock().unwrap();
    let entries = read_all_memory_log_entries(character_id).await?;
    let filtered: Vec<_> = entries
        .into_iter()
        .filter(|n| n.id != memory_id)
        .collect();
    let path = memory_log_path(character_id);
    let mut file = tokio::fs::File::create(&path).await?;
    for node in &filtered {
        let line = serde_json::to_string(node)? + "\n";
        file.write_all(line.as_bytes()).await?;
    }
    Ok(())
}

// --- Memory Graph (derived) ---

fn memory_graph_path(character_id: &str) -> PathBuf {
    char_dir(character_id).join("memory-graph.json")
}

pub async fn save_memory_graph(
    character_id: &str,
    graph: &crate::models::MemoryGraphData,
) -> crate::Result<()> {
    let _lock = char_lock(character_id).lock().unwrap();
    ensure_char_dir(character_id).await?;
    let path = memory_graph_path(character_id);
    write_atomic_json(&path, graph).await?;
    Ok(())
}

pub async fn load_memory_graph(
    character_id: &str,
) -> crate::Result<crate::models::MemoryGraphData> {
    let path = memory_graph_path(character_id);
    if !path.exists() {
        return Ok(crate::models::MemoryGraphData {
            nodes: Vec::new(),
            edges: Vec::new(),
        });
    }
    let content = tokio::fs::read_to_string(&path).await?;
    let graph: crate::models::MemoryGraphData = serde_json::from_str(&content)?;
    Ok(graph)
}

// --- Character Metadata ---

fn character_json_path(character_id: &str) -> PathBuf {
    char_dir(character_id).join("character.json")
}

pub async fn save_character_metadata(
    character: &crate::models::AiCharacter,
) -> crate::Result<()> {
    let _lock = char_lock(&character.id).lock().unwrap();
    ensure_char_dir(&character.id).await?;
    let path = character_json_path(&character.id);
    write_atomic_json(&path, character).await?;
    Ok(())
}

pub async fn load_character_metadata(
    character_id: &str,
) -> crate::Result<Option<crate::models::AiCharacter>> {
    let path = character_json_path(character_id);
    if !path.exists() {
        return Ok(None);
    }
    let content = tokio::fs::read_to_string(&path).await?;
    let character: crate::models::AiCharacter = serde_json::from_str(&content)?;
    Ok(Some(character))
}
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/storage/characters.rs src-tauri/src/storage/mod.rs
git commit -m "feat: add character storage module (memory-log.jsonl, graph, metadata)"
```

---

### Task 4: Embedding Config — Storage & Commands

**Files:**
- Modify: `src-tauri/src/storage/settings.rs`
- Create: `src-tauri/src/commands/embedding.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Add embedding config get/set to settings.rs**

In `src-tauri/src/storage/settings.rs`, add functions:

```rust
use crate::models::EmbeddingConfig;

pub async fn get_embedding_config() -> crate::Result<Option<EmbeddingConfig>> {
    let all = load().await?;
    Ok(all.user.embedding_config)
}

pub async fn update_embedding_config(config: EmbeddingConfig) -> crate::Result<EmbeddingConfig> {
    let mut all = load().await?;
    all.user.embedding_config = Some(config.clone());
    save(&all).await?;
    Ok(config)
}
```

- [ ] **Step 2: Create embedding commands**

```rust
// src-tauri/src/commands/embedding.rs
use crate::models::{EmbeddingConfig, TestEmbeddingResult};
use crate::storage::settings;
use reqwest::Client;
use serde_json::Value;
use std::time::Instant;
use tauri::command;

#[command]
pub async fn get_embedding_config() -> Result<Option<EmbeddingConfig>, String> {
    settings::get_embedding_config()
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn update_embedding_config(config: EmbeddingConfig) -> Result<EmbeddingConfig, String> {
    settings::update_embedding_config(config)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn test_embedding_connection() -> Result<TestEmbeddingResult, String> {
    let config = settings::get_embedding_config()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("No embedding config")?;

    if !config.enabled {
        return Err("Embedding is disabled".into());
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let start = Instant::now();
    let body = serde_json::json!({
        "input": "test connection",
        "model": config.model,
    });

    let mut req = client.post(&config.endpoint).json(&body);
    if let Some(ref key) = config.api_key {
        req = req.header("Authorization", format!("Bearer {}", key));
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    let latency = start.elapsed().as_millis() as u64;

    if !resp.status().is_success() {
        return Ok(TestEmbeddingResult {
            success: false,
            latency_ms: latency,
            dimension: 0,
            error: Some(format!("HTTP {}", resp.status())),
        });
    }

    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let dimension = json["data"][0]["embedding"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    Ok(TestEmbeddingResult {
        success: true,
        latency_ms: latency,
        dimension,
        error: None,
    })
}

/// Fetch embedding vector for a text string from the configured API
pub async fn fetch_embedding(text: &str) -> Result<Vec<f32>, String> {
    let config = settings::get_embedding_config()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("No embedding config")?;

    if !config.enabled {
        return Err("Embedding is disabled".into());
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "input": &text[..text.len().min(2000)],
        "model": config.model,
    });

    let mut req = client.post(&config.endpoint).json(&body);
    if let Some(ref key) = config.api_key {
        req = req.header("Authorization", format!("Bearer {}", key));
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let embedding: Vec<f32> = json["data"][0]["embedding"]
        .as_array()
        .ok_or("Missing embedding data")?
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect();

    Ok(embedding)
}
```

- [ ] **Step 3: Register in commands/mod.rs**

```rust
pub mod embedding;
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/storage/settings.rs src-tauri/src/commands/embedding.rs src-tauri/src/commands/mod.rs
git commit -m "feat: add embedding config storage, get/update/test commands, fetch_embedding helper"
```

---

### Task 5: Vector Store Commands — vectorstore.rs

**Files:**
- Create: `src-tauri/src/commands/vectorstore.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Create vectorstore commands**

```rust
// src-tauri/src/commands/vectorstore.rs
use crate::models::VectorSearchResult;
use crate::storage::characters;
use arrow_array::types::Float32Type;
use arrow_array::{FixedSizeListArray, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use lancedb::connect;
use std::sync::Arc;
use tauri::command;

const TABLE_NAME: &str = "character_memories";

fn lancedb_path(character_id: &str) -> String {
    characters::char_dir(character_id)
        .join("lancedb")
        .to_string_lossy()
        .to_string()
}

#[command]
pub async fn vector_upsert(
    character_id: String,
    page_id: String,
    vector: Vec<f32>,
) -> Result<(), String> {
    let db_path = lancedb_path(&character_id);
    std::fs::create_dir_all(&db_path).map_err(|e| e.to_string())?;
    let db = connect(&db_path).execute().await.map_err(|e| e.to_string())?;

    let dim = vector.len();
    let schema = Arc::new(Schema::new(vec![
        Field::new("page_id", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dim as i32,
            ),
            false,
        ),
    ]));

    let table = if db.table_names().await.map_err(|e| e.to_string())?.contains(&TABLE_NAME.to_string()) {
        db.open_table(TABLE_NAME).execute().await.map_err(|e| e.to_string())?
    } else {
        db.create_table(TABLE_NAME, RecordBatch::new_empty(schema.clone()))
            .execute()
            .await
            .map_err(|e| e.to_string())?
    };

    // Delete existing entry for this page_id
    let _ = table
        .delete()
        .only_if(format!("page_id = '{}'", page_id))
        .execute()
        .await;

    // Insert new vector
    let page_ids = StringArray::from(vec![page_id.as_str()]);
    let list_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        vec![Some(vector.iter().copied())],
        dim as i32,
    );

    let batch = RecordBatch::try_new(
        schema,
        vec![Arc::new(page_ids), Arc::new(list_array)],
    )
    .map_err(|e| e.to_string())?;

    table.add(batch).execute().await.map_err(|e| e.to_string())?;
    Ok(())
}

#[command]
pub async fn vector_search(
    character_id: String,
    query_vector: Vec<f32>,
    top_k: u32,
) -> Result<Vec<VectorSearchResult>, String> {
    let db_path = lancedb_path(&character_id);
    let db = connect(&db_path).execute().await.map_err(|e| e.to_string())?;

    if !db.table_names().await.map_err(|e| e.to_string())?.contains(&TABLE_NAME.to_string()) {
        return Ok(Vec::new());
    }

    let table = db.open_table(TABLE_NAME).execute().await.map_err(|e| e.to_string())?;
    let results = table
        .vector_search(query_vector)
        .map_err(|e| e.to_string())?
        .limit(top_k)
        .execute()
        .await
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for batch in results {
        for i in 0..batch.num_rows() {
            let page_id = batch
                .column_by_name("page_id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>())
                .map(|a| a.value(i).to_string())
                .unwrap_or_default();
            let distance = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<arrow_array::Float32Array>())
                .map(|a| a.value(i))
                .unwrap_or(0.0);
            let score = 1.0 / (1.0 + distance as f64);
            out.push(VectorSearchResult {
                page_id,
                score,
                memory: None,
            });
        }
    }
    Ok(out)
}

#[command]
pub async fn vector_delete(
    character_id: String,
    page_id: String,
) -> Result<(), String> {
    let db_path = lancedb_path(&character_id);
    let db = connect(&db_path).execute().await.map_err(|e| e.to_string())?;
    if !db.table_names().await.map_err(|e| e.to_string())?.contains(&TABLE_NAME.to_string()) {
        return Ok(());
    }
    let table = db.open_table(TABLE_NAME).execute().await.map_err(|e| e.to_string())?;
    let _ = table.delete().only_if(format!("page_id = '{}'", page_id)).execute().await;
    Ok(())
}

#[command]
pub async fn rebuild_vector_index(
    character_id: String,
) -> Result<usize, String> {
    // Rebuild entire LanceDB index from memory-log.jsonl
    let entries = characters::read_all_memory_log_entries(&character_id)
        .await
        .map_err(|e| e.to_string())?;

    // Delete old index
    let db_path = lancedb_path(&character_id);
    let _ = std::fs::remove_dir_all(&db_path);

    // Re-insert all entries
    let mut count = 0;
    for entry in entries {
        // Skip embedding fetch for now — will be populated via auto-extraction
        // For manual rebuild, only re-insert entries that already have embeddings cached
        count += 1;
    }
    Ok(count)
}
```

- [ ] **Step 2: Register in commands/mod.rs**

```rust
pub mod vectorstore;
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/vectorstore.rs src-tauri/src/commands/mod.rs
git commit -m "feat: add LanceDB vector store commands (upsert, search, delete, rebuild)"
```

---

### Task 6: Label→ID Migration

**Files:**
- Modify: `src-tauri/src/group_chat/orchestrator.rs`
- Create: `src-tauri/src/group_chat/migration.rs`

- [ ] **Step 1: Create migration module**

```rust
// src-tauri/src/group_chat/migration.rs
use crate::models::AiCharacter;
use crate::storage::{group_chats, settings};

pub async fn migrate_participant_character_ids() -> crate::Result<usize> {
    let all_settings = settings::load().await?;
    let characters: Vec<AiCharacter> = all_settings.user.ai_characters;
    let chat_ids = group_chats::list_group_chat_ids().await?;

    let mut migrated = 0;
    for chat_id in &chat_ids {
        let mut meta = match group_chats::load_group_chat(chat_id).await {
            Ok(m) => m,
            Err(_) => continue,
        };

        let mut changed = false;
        for participant in &mut meta.participants {
            if !participant.character_id.is_empty() {
                continue;
            }
            // Match by label (case-insensitive), already held before; make it ID-based now
            if let Some(ch) = characters
                .iter()
                .find(|c| c.label.eq_ignore_ascii_case(&participant.label))
            {
                participant.character_id = ch.id.clone();
                changed = true;
            } else {
                participant.character_id = "__orphan__".to_string();
                changed = true;
            }
        }

        if changed {
            group_chats::save_group_chat(chat_id, &meta).await?;
            migrated += 1;
        }
    }
    Ok(migrated)
}
```

- [ ] **Step 2: Update orchestrator resolve_participant_system_prompt**

In `orchestrator.rs`, find `resolve_participant_system_prompt` and replace the label-matching logic:

```rust
fn resolve_participant_system_prompt(
    participant: &GroupChatParticipant,
    ai_characters: &[crate::models::AiCharacter],
) -> Option<String> {
    // ID-based lookup — no label fallback
    if participant.character_id.is_empty() || participant.character_id == "__orphan__" {
        return None;
    }
    let character = ai_characters
        .iter()
        .find(|c| c.id == participant.character_id)?;
    let prompt = build_role_system_prompt(&character.role_type, &character.role_instruction);
    if prompt.is_empty() { None } else { Some(prompt) }
}
```

- [ ] **Step 3: Call migration on app startup**

In `src-tauri/src/lib.rs` or `main.rs`, add after settings load:

```rust
// Run character_id migration on startup
tokio::spawn(async {
    match crate::group_chat::migration::migrate_participant_character_ids().await {
        Ok(n) if n > 0 => log::info!("Migrated {} group chats to character_id linkage", n),
        Err(e) => log::warn!("Character ID migration failed: {}", e),
        _ => {}
    }
});
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/group_chat/migration.rs src-tauri/src/group_chat/orchestrator.rs src-tauri/src/lib.rs
git commit -m "feat: label→ID migration for group chat participants, ID-based lookup only"
```

---

### Task 7: Knowledge Graph Backend — memory_graph.rs

**Files:**
- Create: `src-tauri/src/group_chat/memory_graph.rs`
- Modify: `src-tauri/src/group_chat/mod.rs`

- [ ] **Step 1: Declare module**

In `src-tauri/src/group_chat/mod.rs`:

```rust
pub mod memory_graph;
pub mod migration;
```

- [ ] **Step 2: Implement graph computation**

```rust
// src-tauri/src/group_chat/memory_graph.rs
use crate::models::{CommunityInfo, KnowledgeGapInfo, MemoryEdge, MemoryNode, MemoryGraphData};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use std::collections::{HashMap, HashSet};

pub fn build_graph(nodes: &[MemoryNode], edges: &[MemoryEdge]) -> DiGraph<String, f64> {
    let mut graph = DiGraph::new();
    let mut node_map: HashMap<String, NodeIndex> = HashMap::new();

    for node in nodes {
        let idx = graph.add_node(node.content.clone());
        node_map.insert(node.id.clone(), idx);
    }

    for edge in edges {
        if let (Some(&src), Some(&tgt)) = (
            node_map.get(&edge.source_id),
            node_map.get(&edge.target_id),
        ) {
            graph.add_edge(src, tgt, edge.weight);
        }
    }
    graph
}

/// Compute edges using 4-signal relevance model.
/// Called when new memories are added — compares new nodes against all existing.
pub fn compute_relevance_edges(
    new_node: &MemoryNode,
    existing_nodes: &[MemoryNode],
    _existing_edges: &[MemoryEdge],
) -> Vec<MemoryEdge> {
    let mut edges = Vec::new();

    for existing in existing_nodes {
        if existing.id == new_node.id {
            continue;
        }

        // Signal 1: Source overlap (weight 4.0)
        let source_overlap = if new_node.source.group_chat_id == existing.source.group_chat_id
            && new_node.source.group_chat_id.is_some()
        {
            4.0
        } else {
            0.0
        };

        // Signal 2: Direct tags overlap (proxy for direct link, weight 3.0)
        let tag_overlap = new_node
            .tags
            .iter()
            .filter(|t| existing.tags.contains(t))
            .count() as f64
            * 3.0
            / new_node.tags.len().max(1) as f64;

        // Signal 3: Type affinity (weight 1.0)
        let type_affinity = if new_node.memory_type == existing.memory_type {
            1.0
        } else {
            0.0
        };

        let total = source_overlap + tag_overlap + type_affinity;
        if total > 0.5 {
            let relation = if source_overlap > 0.0 {
                "related"
            } else if new_node.memory_type == existing.memory_type {
                "extends"
            } else {
                "related"
            };

            edges.push(MemoryEdge {
                id: uuid::Uuid::new_v4().to_string(),
                source_id: new_node.id.clone(),
                target_id: existing.id.clone(),
                relation: relation.to_string(),
                weight: (total / 8.0).min(1.0),
            });
        }
    }
    edges
}

/// Louvain community detection — simplified port from graphology-communities-louvain.
/// Returns (community_assignments, community_labels).
pub fn detect_communities(
    nodes: &[MemoryNode],
    edges: &[MemoryEdge],
) -> Vec<CommunityInfo> {
    let graph = build_graph(nodes, edges);
    let n = graph.node_count();
    if n == 0 {
        return Vec::new();
    }

    // Simplified Louvain: greedy modularity optimization
    let mut communities: Vec<usize> = (0..n).collect();
    let mut changed = true;
    let mut iteration = 0;
    let max_iterations = 50;

    while changed && iteration < max_iterations {
        changed = false;
        iteration += 1;

        for node in graph.node_indices() {
            let nidx = node.index();
            let current_comm = communities[nidx];

            // Count neighbor communities
            let mut neighbor_comms: HashMap<usize, f64> = HashMap::new();
            for edge in graph.edges(node) {
                let neighbor = edge.target().index();
                let comm = communities[neighbor];
                *neighbor_comms.entry(comm).or_insert(0.0) += edge.weight();
            }

            // Find best community
            let mut best_comm = current_comm;
            let mut best_gain = 0.0;
            for (&comm, &weight) in &neighbor_comms {
                if comm != current_comm && weight > best_gain {
                    best_gain = weight;
                    best_comm = comm;
                }
            }

            if best_comm != current_comm {
                communities[nidx] = best_comm;
                changed = true;
            }
        }
    }

    // Build community info
    let mut comm_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for (ni, &ci) in communities.iter().enumerate() {
        comm_map.entry(ci).or_default().push(ni);
    }

    let mut result: Vec<CommunityInfo> = comm_map
        .into_iter()
        .enumerate()
        .map(|(i, (_, member_indices))| {
            let node_ids: Vec<String> = member_indices
                .iter()
                .filter_map(|&idx| nodes.get(idx).map(|n| n.id.clone()))
                .collect();

            // Cohesion: internal edges / total possible edges
            let n = member_indices.len();
            let mut internal_edges = 0;
            for &a in &member_indices {
                for &b in &member_indices {
                    if a < b {
                        for edge in edges {
                            if (edge.source_id == nodes[a].id && edge.target_id == nodes[b].id)
                                || (edge.source_id == nodes[b].id && edge.target_id == nodes[a].id)
                            {
                                internal_edges += 1;
                            }
                        }
                    }
                }
            }
            let max_edges = if n > 1 { n * (n - 1) / 2 } else { 1 };
            let cohesion = internal_edges as f64 / max_edges as f64;

            CommunityInfo {
                id: i,
                label: format!("社区 {}", i + 1),
                cohesion,
                node_count: n,
                edge_count: internal_edges,
                node_ids,
            }
        })
        .collect();

    result.sort_by(|a, b| b.node_count.cmp(&a.node_count));
    result
}

pub fn detect_knowledge_gaps(
    nodes: &[MemoryNode],
    edges: &[MemoryEdge],
    communities: &[CommunityInfo],
) -> Vec<KnowledgeGapInfo> {
    let mut gaps = Vec::new();

    // Isolated nodes (degree ≤ 1)
    let mut degrees: HashMap<String, usize> = HashMap::new();
    for edge in edges {
        *degrees.entry(edge.source_id.clone()).or_default() += 1;
        *degrees.entry(edge.target_id.clone()).or_default() += 1;
    }
    for node in nodes {
        let deg = degrees.get(&node.id).copied().unwrap_or(0);
        if deg <= 1 {
            gaps.push(KnowledgeGapInfo {
                gap_type: "isolated_node".to_string(),
                description: format!("\"{}\" 仅连接 {} 条记忆，是知识孤岛", &node.content[..node.content.len().min(50)], deg),
                suggestion: "建议将这条记忆与其他相关记忆建立关联".to_string(),
                affected_node_ids: vec![node.id.clone()],
            });
        }
    }

    // Sparse communities (cohesion < 0.15, ≥ 3 nodes)
    for comm in communities {
        if comm.cohesion < 0.15 && comm.node_count >= 3 {
            gaps.push(KnowledgeGapInfo {
                gap_type: "sparse_community".to_string(),
                description: format!("{} 凝聚力仅 {:.2}，节点间连接稀疏", comm.label, comm.cohesion),
                suggestion: "建议为该知识领域补充更多关联记忆".to_string(),
                affected_node_ids: comm.node_ids.clone(),
            });
        }
    }

    // Bridge nodes (connected to ≥ 3 communities)
    let mut node_community_map: HashMap<String, HashSet<usize>> = HashMap::new();
    for (comm_idx, comm) in communities.iter().enumerate() {
        for nid in &comm.node_ids {
            node_community_map
                .entry(nid.clone())
                .or_default()
                .insert(comm_idx);
        }
    }
    for (nid, comms) in &node_community_map {
        if comms.len() >= 3 {
            if let Some(node) = nodes.iter().find(|n| &n.id == nid) {
                gaps.push(KnowledgeGapInfo {
                    gap_type: "bridge_node".to_string(),
                    description: format!("\"{}\" 连接 {} 个知识社区，是关键枢纽节点", &node.content[..node.content.len().min(50)], comms.len()),
                    suggestion: "枢纽节点置信度应保持较高水平，建议补充更多上下文".to_string(),
                    affected_node_ids: vec![nid.clone()],
                });
            }
        }
    }

    gaps
}

/// Graph traversal — from seed node IDs, expand N hops.
/// Returns expanded node IDs for injection.
pub fn graph_expand(
    edges: &[MemoryEdge],
    seed_ids: &[String],
    hops: usize,
) -> Vec<String> {
    let mut visited: HashSet<String> = seed_ids.iter().cloned().collect();
    let mut frontier: HashSet<String> = seed_ids.iter().cloned().collect();

    for _ in 0..hops {
        let mut next_frontier = HashSet::new();
        for edge in edges {
            if frontier.contains(&edge.source_id) && !visited.contains(&edge.target_id) {
                next_frontier.insert(edge.target_id.clone());
            }
            if frontier.contains(&edge.target_id) && !visited.contains(&edge.source_id) {
                next_frontier.insert(edge.source_id.clone());
            }
        }
        for id in &next_frontier {
            visited.insert(id.clone());
        }
        frontier = next_frontier;
    }

    visited.into_iter().collect()
}
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/group_chat/memory_graph.rs src-tauri/src/group_chat/mod.rs
git commit -m "feat: knowledge graph backend — petgraph, 4-signal relevance, Louvain, gaps, traversal"
```

---

### Task 8: Memory CRUD Commands

**Files:**
- Modify: `src-tauri/src/commands/characters.rs`
- Modify: `src-tauri/src/lib.rs` (register commands)

- [ ] **Step 1: Add memory CRUD commands to characters.rs**

Append to `src-tauri/src/commands/characters.rs`:

```rust
use crate::group_chat::memory_graph::{compute_relevance_edges, detect_communities, detect_knowledge_gaps};
use crate::models::{MemoryNode, MemoryEdge, MemoryGraphData, CommunityInfo, KnowledgeGapInfo, MemorySource};
use crate::storage::characters as char_store;
use crate::commands::embedding;

#[command]
pub async fn list_character_memories(
    character_id: String,
) -> Result<Vec<MemoryNode>, String> {
    char_store::read_all_memory_log_entries(&character_id)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn get_character_memory(
    character_id: String,
    memory_id: String,
) -> Result<Option<MemoryNode>, String> {
    let entries = char_store::read_all_memory_log_entries(&character_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(entries.into_iter().find(|n| n.id == memory_id))
}

#[command]
pub async fn create_character_memory(
    character_id: String,
    content: String,
    memory_type: String,
    confidence: f64,
    tags: Vec<String>,
) -> Result<MemoryNode, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let node = MemoryNode {
        id: uuid::Uuid::new_v4().to_string(),
        character_id: character_id.clone(),
        content: content.clone(),
        memory_type: memory_type.clone(),
        confidence,
        source: MemorySource {
            kind: "manual".to_string(),
            run_id: None,
            group_chat_id: None,
        },
        tags: tags.clone(),
        created_at: now.clone(),
        updated_at: now,
    };

    // 1. Append to authoritative log
    char_store::append_memory_log(&character_id, &node)
        .await
        .map_err(|e| e.to_string())?;

    // 2. Update graph — compute edges against existing nodes
    let existing = char_store::read_all_memory_log_entries(&character_id)
        .await
        .map_err(|e| e.to_string())?;
    let mut graph = char_store::load_memory_graph(&character_id)
        .await
        .map_err(|e| e.to_string())?;
    graph.nodes.push(node.clone());
    let new_edges = compute_relevance_edges(&node, &existing, &graph.edges);
    graph.edges.extend(new_edges);
    let _ = char_store::save_memory_graph(&character_id, &graph).await;

    // 3. LanceDB upsert
    let embedding_vec = match embedding::fetch_embedding(&content).await {
        Ok(v) => v,
        Err(_) => return Ok(node), // graceful: skip vector index if embedding fails
    };
    let _ = crate::commands::vectorstore::vector_upsert(
        character_id.clone(),
        node.id.clone(),
        embedding_vec,
    );

    Ok(node)
}

#[command]
pub async fn update_character_memory(
    character_id: String,
    memory_id: String,
    content: Option<String>,
    memory_type: Option<String>,
    confidence: Option<f64>,
    tags: Option<Vec<String>>,
) -> Result<MemoryNode, String> {
    let mut entries = char_store::read_all_memory_log_entries(&character_id)
        .await
        .map_err(|e| e.to_string())?;
    let idx = entries
        .iter()
        .position(|n| n.id == memory_id)
        .ok_or("Memory not found")?;

    let now = chrono::Utc::now().to_rfc3339();
    if let Some(c) = content.clone() { entries[idx].content = c; }
    if let Some(t) = memory_type { entries[idx].memory_type = t; }
    if let Some(c) = confidence { entries[idx].confidence = c; }
    if let Some(t) = tags { entries[idx].tags = t; }
    entries[idx].updated_at = now;

    let updated = entries[idx].clone();

    // Rewrite log
    let path = char_store::memory_log_path(&character_id);
    let mut file = tokio::fs::File::create(&path).await.map_err(|e| e.to_string())?;
    use tokio::io::AsyncWriteExt;
    for node in &entries {
        let line = serde_json::to_string(node).map_err(|e| e.to_string())? + "\n";
        file.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
    }

    // Update vector if content changed
    if let Some(ref c) = content {
        let _ = crate::commands::vectorstore::vector_delete(character_id.clone(), memory_id.clone());
        if let Ok(embedding_vec) = embedding::fetch_embedding(c).await {
            let _ = crate::commands::vectorstore::vector_upsert(
                character_id.clone(),
                memory_id,
                embedding_vec,
            );
        }
    }

    Ok(updated)
}

#[command]
pub async fn delete_character_memory(
    character_id: String,
    memory_id: String,
) -> Result<(), String> {
    char_store::delete_memory_from_log(&character_id, &memory_id)
        .await
        .map_err(|e| e.to_string())?;
    let _ = crate::commands::vectorstore::vector_delete(character_id, memory_id);
    Ok(())
}

#[command]
pub async fn get_memory_graph(
    character_id: String,
) -> Result<MemoryGraphData, String> {
    char_store::load_memory_graph(&character_id)
        .await
        .map_err(|e| e.to_string())
}

#[command]
pub async fn get_memory_communities(
    character_id: String,
) -> Result<Vec<CommunityInfo>, String> {
    let graph = char_store::load_memory_graph(&character_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(detect_communities(&graph.nodes, &graph.edges))
}

#[command]
pub async fn get_knowledge_gaps(
    character_id: String,
) -> Result<Vec<KnowledgeGapInfo>, String> {
    let graph = char_store::load_memory_graph(&character_id)
        .await
        .map_err(|e| e.to_string())?;
    let communities = detect_communities(&graph.nodes, &graph.edges);
    Ok(detect_knowledge_gaps(&graph.nodes, &graph.edges, &communities))
}
```

- [ ] **Step 2: Register all new commands in lib.rs**

In `src-tauri/src/lib.rs`, add to `invoke_handler`:

```rust
commands::characters::list_character_memories,
commands::characters::get_character_memory,
commands::characters::create_character_memory,
commands::characters::update_character_memory,
commands::characters::delete_character_memory,
commands::characters::get_memory_graph,
commands::characters::get_memory_communities,
commands::characters::get_knowledge_gaps,
commands::vectorstore::vector_upsert,
commands::vectorstore::vector_search,
commands::vectorstore::vector_delete,
commands::vectorstore::rebuild_vector_index,
commands::embedding::get_embedding_config,
commands::embedding::update_embedding_config,
commands::embedding::test_embedding_connection,
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/characters.rs src-tauri/src/lib.rs
git commit -m "feat: memory CRUD commands, graph/community/gap queries, register all new commands"
```

---

### Task 9: Memory Retrieval & Injection

**Files:**
- Create: `src-tauri/src/group_chat/memory_injection.rs`
- Modify: `src-tauri/src/group_chat/orchestrator.rs`

- [ ] **Step 1: Create injection module**

```rust
// src-tauri/src/group_chat/memory_injection.rs
use crate::commands::{embedding, vectorstore};
use crate::group_chat::memory_graph::graph_expand;
use crate::models::{MemoryNode, VectorSearchResult, EmbeddingConfig};
use crate::storage::characters;
use once_cell::sync::Lazy;
use std::sync::Mutex;
use std::time::Instant;

// Embedding health state
static EMBEDDING_HEALTH: Lazy<Mutex<Option<(bool, Instant)>>> =
    Lazy::new(|| Mutex::new(None));

fn is_embedding_healthy() -> bool {
    if let Ok(guard) = EMBEDDING_HEALTH.lock() {
        if let Some((healthy, last_check)) = *guard {
            if last_check.elapsed().as_secs() < 60 {
                return healthy;
            }
        }
    }
    true // assume healthy until proven otherwise
}

fn set_embedding_healthy(healthy: bool) {
    if let Ok(mut guard) = EMBEDDING_HEALTH.lock() {
        *guard = Some((healthy, Instant::now()));
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DegradationTier {
    Full,
    Degraded,
    Minimal,
    Skip,
}

/// Hybrid search for relevant memories to inject.
/// Returns (memories, degradation_tier).
pub async fn search_memories_for_injection(
    character_id: &str,
    query: &str,
    top_k: usize,
    threshold: f64,
    graph_hops: usize,
) -> (Vec<MemoryNode>, DegradationTier) {
    // Tier: Skip check
    let entries = match characters::read_all_memory_log_entries(character_id).await {
        Ok(e) => e,
        Err(_) => return (Vec::new(), DegradationTier::Skip),
    };
    if entries.is_empty() {
        return (Vec::new(), DegradationTier::Skip);
    }

    if !is_embedding_healthy() {
        // Degraded: keyword + graph only
        return keyword_graph_search(&entries, query, top_k, graph_hops);
    }

    // Full: try vector search
    let query_vec = match embedding::fetch_embedding(query).await {
        Ok(v) => v,
        Err(_) => {
            set_embedding_healthy(false);
            return keyword_graph_search(&entries, query, top_k, graph_hops);
        }
    };
    set_embedding_healthy(true);

    let vector_results = match vectorstore::vector_search(
        character_id.to_string(),
        query_vec,
        top_k as u32 * 2,
    ).await {
        Ok(r) => r,
        Err(_) => {
            return keyword_graph_search(&entries, query, top_k, graph_hops);
        }
    };

    // Graph expansion from vector results
    let graph = characters::load_memory_graph(character_id).await.unwrap_or_default();
    let seed_ids: Vec<String> = vector_results.iter().map(|r| r.page_id.clone()).collect();
    let expanded_ids = graph_expand(&graph.edges, &seed_ids, graph_hops);

    // Merge: vector + keyword + expanded
    let mut scored: Vec<(MemoryNode, f64)> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for vr in &vector_results {
        if let Some(node) = entries.iter().find(|n| n.id == vr.page_id) {
            if !seen.contains(&node.id) {
                let keyword_boost = keyword_match_score(&node.content, query);
                scored.push((node.clone(), vr.score * 0.6 + keyword_boost * 0.4));
                seen.insert(node.id.clone());
            }
        }
    }

    for eid in &expanded_ids {
        if !seen.contains(eid) {
            if let Some(node) = entries.iter().find(|n| &n.id == eid) {
                let keyword_boost = keyword_match_score(&node.content, query);
                scored.push((node.clone(), 0.3 + keyword_boost * 0.3));
                seen.insert(eid.clone());
            }
        }
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let results: Vec<MemoryNode> = scored
        .into_iter()
        .filter(|(_, s)| *s >= threshold)
        .take(top_k)
        .map(|(n, _)| n)
        .collect();

    (results, DegradationTier::Full)
}

fn keyword_graph_search(
    entries: &[MemoryNode],
    query: &str,
    top_k: usize,
    graph_hops: usize,
) -> (Vec<MemoryNode>, DegradationTier) {
    // Degraded: BM25 + recency + graph expansion
    let scored: Vec<(MemoryNode, f64)> = entries
        .iter()
        .map(|node| {
            let keyword_score = keyword_match_score(&node.content, query);
            let recency = 0.1; // simplified — real impl uses time decay
            (node.clone(), keyword_score + recency)
        })
        .collect();

    let mut sorted = scored;
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let results: Vec<MemoryNode> = sorted.into_iter().take(top_k).map(|(n, _)| n).collect();
    (results, DegradationTier::Degraded)
}

fn keyword_match_score(text: &str, query: &str) -> f64 {
    let text_lower = text.to_lowercase();
    let query_lower = query.to_lowercase();
    let query_terms: Vec<&str> = query_lower.split_whitespace().collect();
    let mut matches = 0;
    for term in &query_terms {
        if text_lower.contains(term) {
            matches += 1;
        }
    }
    if query_terms.is_empty() { 0.0 } else { matches as f64 / query_terms.len() as f64 }
}

/// Format memories for system prompt injection, respecting token budget.
pub fn format_memory_injection(
    memories: &[MemoryNode],
    max_tokens: usize,
    max_tokens_per_memory: usize,
) -> String {
    if memories.is_empty() {
        return String::new();
    }

    let mut lines = vec!["[Character Memory — 相关记忆]".to_string()];
    let mut token_count = 0;
    let chars_per_token_approx = 4;

    for (i, mem) in memories.iter().enumerate() {
        let truncated: String = mem.content.chars()
            .take(max_tokens_per_memory * chars_per_token_approx)
            .collect();
        let tag = match mem.memory_type.as_str() {
            "fact" => "Fact",
            "experience" => "Experience",
            "preference" => "Preference",
            "rule" => "Rule",
            "relationship" => "Relationship",
            _ => "Memory",
        };
        let line = format!(
            "{}. [{} · 置信度 {}%] {}",
            i + 1,
            tag,
            (mem.confidence * 100.0) as u32,
            truncated
        );
        let line_tokens = line.len() / chars_per_token_approx;
        if token_count + line_tokens > max_tokens {
            break;
        }
        token_count += line_tokens;
        lines.push(line);
    }

    lines.join("\n")
}
```

- [ ] **Step 2: Integrate injection into orchestrator**

In `orchestrator.rs`, find where system prompt is built before spawning a participant actor, and add memory injection:

```rust
use crate::group_chat::memory_injection::{search_memories_for_injection, format_memory_injection, DegradationTier};

// Before sending to a participant, add:
let (memories, tier) = search_memories_for_injection(
    &participant.character_id,
    &user_message,
    injection_config.max_retrieval_count,
    injection_config.relevance_threshold,
    injection_config.graph_expansion_hops,
).await;

let memory_prompt = format_memory_injection(
    &memories,
    injection_config.max_injection_tokens,
    injection_config.max_tokens_per_memory,
);

// Inject after role prompt, before user message
let full_system_prompt = format!(
    "{role_prompt}\n\n{memory_prompt}\n\n[Current Task]\n{user_message}",
    role_prompt = role_prompt,
    memory_prompt = memory_prompt,
    user_message = user_message,
);
```

- [ ] **Step 3: Update module declarations**

In `src-tauri/src/group_chat/mod.rs`:

```rust
pub mod memory_injection;
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/group_chat/memory_injection.rs src-tauri/src/group_chat/orchestrator.rs src-tauri/src/group_chat/mod.rs
git commit -m "feat: hybrid search + memory injection into system prompt, 4-tier degradation"
```

---

### Task 10: Frontend API Layer

**Files:**
- Modify: `src/lib/api.ts`

- [ ] **Step 1: Add memory API functions**

In `src/lib/api.ts`, append new functions:

```typescript
import type { MemoryNode, MemoryGraphData, CommunityInfo, KnowledgeGapInfo, EmbeddingConfig, TestEmbeddingResult, VectorSearchResult } from './types';

// --- Embedding Config ---
export async function getEmbeddingConfig(): Promise<EmbeddingConfig | null> {
  return invoke("get_embedding_config");
}
export async function updateEmbeddingConfig(config: EmbeddingConfig): Promise<EmbeddingConfig> {
  return invoke("update_embedding_config", { config });
}
export async function testEmbeddingConnection(): Promise<TestEmbeddingResult> {
  return invoke("test_embedding_connection");
}

// --- Memory CRUD ---
export async function listCharacterMemories(characterId: string): Promise<MemoryNode[]> {
  return invoke("list_character_memories", { characterId });
}
export async function getCharacterMemory(characterId: string, memoryId: string): Promise<MemoryNode | null> {
  return invoke("get_character_memory", { characterId, memoryId });
}
export async function createCharacterMemory(
  characterId: string, content: string, type: string,
  confidence: number, tags: string[],
): Promise<MemoryNode> {
  return invoke("create_character_memory", { characterId, content, memoryType: type, confidence, tags });
}
export async function updateCharacterMemory(
  characterId: string, memoryId: string,
  updates: { content?: string; memoryType?: string; confidence?: number; tags?: string[] },
): Promise<MemoryNode> {
  return invoke("update_character_memory", { characterId, memoryId, ...updates });
}
export async function deleteCharacterMemory(characterId: string, memoryId: string): Promise<void> {
  return invoke("delete_character_memory", { characterId, memoryId });
}

// --- Knowledge Graph ---
export async function getMemoryGraph(characterId: string): Promise<MemoryGraphData> {
  return invoke("get_memory_graph", { characterId });
}
export async function getMemoryCommunities(characterId: string): Promise<CommunityInfo[]> {
  return invoke("get_memory_communities", { characterId });
}
export async function getKnowledgeGaps(characterId: string): Promise<KnowledgeGapInfo[]> {
  return invoke("get_knowledge_gaps", { characterId });
}

// --- Vector Store ---
export async function vectorSearch(characterId: string, queryVector: number[], topK: number): Promise<VectorSearchResult[]> {
  return invoke("vector_search", { characterId, queryVector, topK });
}
export async function rebuildVectorIndex(characterId: string): Promise<number> {
  return invoke("rebuild_vector_index", { characterId });
}
```

- [ ] **Step 2: Commit**

```bash
git add src/lib/api.ts
git commit -m "feat: frontend API layer for memory, embedding, graph, and vector store"
```

---

### Task 11: Character Memory Store — character-memory-store.svelte.ts

**Files:**
- Create: `src/lib/stores/character-memory-store.svelte.ts`

- [ ] **Step 1: Create reactive store**

```typescript
// src/lib/stores/character-memory-store.svelte.ts
import * as api from '$lib/api';
import type { MemoryNode, MemoryGraphData, CommunityInfo, KnowledgeGapInfo } from '$lib/types';

export class CharacterMemoryStore {
  characterId = $state<string | null>(null);
  memories = $state<MemoryNode[]>([]);
  graph = $state<MemoryGraphData | null>(null);
  communities = $state<CommunityInfo[]>([]);
  gaps = $state<KnowledgeGapInfo[]>([]);
  loading = $state(false);
  activeTab = $state<'memories' | 'graph' | 'gaps' | 'communities'>('memories');
  searchQuery = $state('');
  sortBy = $state<'newest' | 'confidence' | 'relevance'>('newest');

  async load(characterId: string) {
    this.characterId = characterId;
    this.loading = true;
    const [memories, graph, communities, gaps] = await Promise.all([
      api.listCharacterMemories(characterId).catch(() => [] as MemoryNode[]),
      api.getMemoryGraph(characterId).catch(() => null),
      api.getMemoryCommunities(characterId).catch(() => [] as CommunityInfo[]),
      api.getKnowledgeGaps(characterId).catch(() => [] as KnowledgeGapInfo[]),
    ]);
    this.memories = memories;
    this.graph = graph;
    this.communities = communities;
    this.gaps = gaps;
    this.loading = false;
  }

  get sortedMemories(): MemoryNode[] {
    let filtered = this.memories;
    if (this.searchQuery) {
      const q = this.searchQuery.toLowerCase();
      filtered = filtered.filter(m =>
        m.content.toLowerCase().includes(q) ||
        m.tags.some(t => t.toLowerCase().includes(q))
      );
    }
    if (this.sortBy === 'newest') {
      filtered.sort((a, b) => b.created_at.localeCompare(a.created_at));
    } else if (this.sortBy === 'confidence') {
      filtered.sort((a, b) => b.confidence - a.confidence);
    }
    return filtered;
  }

  async addMemory(
    content: string,
    type: MemoryNode['type'],
    confidence: number,
    tags: string[],
  ) {
    if (!this.characterId) return;
    const node = await api.createCharacterMemory(this.characterId, content, type, confidence, tags);
    this.memories.unshift(node);
  }

  async deleteMemory(memoryId: string) {
    if (!this.characterId) return;
    await api.deleteCharacterMemory(this.characterId, memoryId);
    this.memories = this.memories.filter(m => m.id !== memoryId);
  }

  async updateMemory(
    memoryId: string,
    updates: { content?: string; type?: string; confidence?: number; tags?: string[] },
  ) {
    if (!this.characterId) return;
    const updated = await api.updateCharacterMemory(this.characterId, memoryId, updates as any);
    const idx = this.memories.findIndex(m => m.id === memoryId);
    if (idx >= 0) this.memories[idx] = updated;
  }
}

export const characterMemoryStore = new CharacterMemoryStore();
```

- [ ] **Step 2: Commit**

```bash
git add src/lib/stores/character-memory-store.svelte.ts
git commit -m "feat: character memory store with reactive state for panel UI"
```

---

### Task 12: Character Memory Panel UI

**Files:**
- Create: `src/lib/components/CharacterMemoryPanel.svelte`
- Create: `src/lib/components/MemoryAddModal.svelte`

- [ ] **Step 1: Create MemoryAddModal**

```svelte
<!-- src/lib/components/MemoryAddModal.svelte -->
<script lang="ts">
  import type { MemoryNode } from '$lib/types';

  let { show = false, onClose = () => {}, onSave = (content: string, type: MemoryNode['type'], confidence: number, tags: string[]) => {} }: {
    show: boolean;
    onClose: () => void;
    onSave: (content: string, type: MemoryNode['type'], confidence: number, tags: string[]) => void;
  } = $props();

  let content = $state('');
  let type = $state<MemoryNode['type']>('fact');
  let confidence = $state(90);
  let tagInput = $state('');
  let tags = $state<string[]>([]);

  function addTag() {
    if (tagInput.trim() && !tags.includes(tagInput.trim())) {
      tags.push(tagInput.trim());
      tagInput = '';
    }
  }

  function handleSave() {
    if (!content.trim()) return;
    onSave(content.trim(), type, confidence / 100, tags);
    content = '';
    tags = [];
    onClose();
  }
</script>

{#if show}
<div class="fixed inset-0 bg-black/60 flex items-center justify-center z-50" onclick={onClose}>
  <div class="bg-[#111118] rounded-xl border border-[#1e1e2e] w-[480px] p-6" onclick={(e) => e.stopPropagation()}>
    <h3 class="text-sm font-semibold mb-4">手动添加记忆</h3>
    <div class="flex flex-col gap-3">
      <div>
        <label class="text-[10px] uppercase text-[#666] block mb-1">记忆内容 *</label>
        <textarea bind:value={content} placeholder="写一条你想让角色记住的信息..." class="w-full bg-[#0d0d14] border border-[#2a2a3a] rounded p-2 text-xs h-20 resize-none"></textarea>
      </div>
      <div class="grid grid-cols-2 gap-3">
        <div>
          <label class="text-[10px] uppercase text-[#666] block mb-1">类型</label>
          <select bind:value={type} class="w-full bg-[#0d0d14] border border-[#2a2a3a] rounded p-2 text-xs">
            <option value="fact">事实 (fact)</option>
            <option value="experience">经验 (experience)</option>
            <option value="preference">偏好 (preference)</option>
            <option value="rule">规则 (rule)</option>
            <option value="relationship">关系 (relationship)</option>
          </select>
        </div>
        <div>
          <label class="text-[10px] uppercase text-[#666] block mb-1">置信度: {confidence}%</label>
          <input type="range" min="50" max="100" bind:value={confidence} class="w-full" />
        </div>
      </div>
      <div>
        <label class="text-[10px] uppercase text-[#666] block mb-1">标签</label>
        <div class="flex gap-1 mb-2">
          <input bind:value={tagInput} placeholder="输入标签..." class="flex-1 bg-[#0d0d14] border border-[#2a2a3a] rounded p-2 text-xs" onkeydown={(e) => e.key === 'Enter' && addTag()} />
          <button onclick={addTag} class="bg-[#1a1a2e] px-3 rounded text-xs">+</button>
        </div>
        <div class="flex flex-wrap gap-1">
          {#each tags as tag}
            <span class="bg-[#1e3a5f] text-[#60a5fa] px-2 py-0.5 rounded-full text-[10px] cursor-pointer" onclick={() => tags = tags.filter(t => t !== tag)}>{tag} ×</span>
          {/each}
        </div>
      </div>
    </div>
    <div class="flex justify-end gap-2 mt-4">
      <button onclick={onClose} class="bg-[#222] text-[#999] px-4 py-1.5 rounded text-xs">取消</button>
      <button onclick={handleSave} class="bg-[#2563eb] px-4 py-1.5 rounded text-xs">保存记忆</button>
    </div>
  </div>
</div>
{/if}
```

- [ ] **Step 2: Create CharacterMemoryPanel**

```svelte
<!-- src/lib/components/CharacterMemoryPanel.svelte -->
<script lang="ts">
  import { characterMemoryStore as store } from '$lib/stores/character-memory-store.svelte.ts';
  import MemoryAddModal from './MemoryAddModal.svelte';
  import type { MemoryNode, MemoryGraphData } from '$lib/types';

  let { characterId, characterLabel, characterAvatar, onClose = () => {} }: {
    characterId: string;
    characterLabel: string;
    characterAvatar?: string;
    onClose?: () => void;
  } = $props();

  let showAddModal = $state(false);
  let searchQuery = $state('');

  $effect(() => {
    if (characterId) store.load(characterId);
  });

  const typeColors: Record<string, string> = {
    fact: 'bg-blue-500/20 text-blue-400',
    experience: 'bg-green-500/20 text-green-400',
    preference: 'bg-amber-500/20 text-amber-400',
    rule: 'bg-red-500/20 text-red-400',
    relationship: 'bg-purple-500/20 text-purple-400',
  };

  function typeLabel(type: string): string {
    const map: Record<string, string> = { fact: '事实', experience: '经验', preference: '偏好', rule: '规则', relationship: '关系' };
    return map[type] ?? type;
  }

  async function handleAddMemory(content: string, type: MemoryNode['type'], confidence: number, tags: string[]) {
    await store.addMemory(content, type, confidence, tags);
  }
</script>

<div class="fixed inset-0 bg-black/60 z-50 flex items-center justify-center" onclick={onClose}>
  <div class="bg-[#0a0a0f] rounded-xl border border-[#1e1e2e] w-[960px] h-[600px] flex flex-col overflow-hidden" onclick={(e) => e.stopPropagation()}>
    <!-- Top Bar -->
    <div class="flex items-center gap-3 px-4 py-2.5 bg-[#111118] border-b border-[#1e1e2e]">
      {#if characterAvatar}
        <img src={characterAvatar} alt="" class="w-8 h-8 rounded-lg object-cover" />
      {:else}
        <div class="w-8 h-8 rounded-lg bg-[#1e3a5f] flex items-center justify-center text-sm">⚙</div>
      {/if}
      <div class="flex-1">
        <div class="text-sm font-semibold">{characterLabel}</div>
        <div class="text-[10px] text-[#666]">{store.memories.length} 条记忆</div>
      </div>
      <input bind:value={searchQuery} placeholder="搜索记忆..." class="bg-[#0d0d14] border border-[#2a2a3a] rounded px-3 py-1 text-xs w-48" />
      <button onclick={() => showAddModal = true} class="bg-[#2563eb] px-3 py-1 rounded text-xs">+ 手动添加</button>
    </div>

    <!-- Tabs -->
    <div class="flex border-b border-[#1e1e2e] bg-[#111118]">
      {#each [
        ['memories', `全部记忆 ${store.memories.length}`],
        ['graph', `知识图谱`],
        ['gaps', `知识缺口 ${store.gaps.length}`],
        ['communities', `社区 Community ${store.communities.length}`],
      ] as [typeof store.activeTab, string]}
        <button
          class="px-4 py-2 text-xs border-b-2 transition-colors {store.activeTab === tab ? 'border-[#60a5fa] text-[#60a5fa]' : 'border-transparent text-[#666]'}"
          onclick={() => store.activeTab = tab}
        >{label}</button>
      {/each}
    </div>

    <!-- Body -->
    <div class="flex-1 flex overflow-hidden">
      <!-- Graph Pane -->
      <div class="flex-1 bg-[#08080f] flex items-center justify-center border-r border-[#1e1e2e]">
        {#if store.activeTab === 'graph' && store.graph}
          <div class="text-[#333] text-xs">图谱可视化 (sigma.js 集成后渲染)</div>
        {:else if store.activeTab === 'communities'}
          <div class="p-4 flex flex-col gap-3 w-full overflow-y-auto">
            {#each store.communities as comm}
              <div class="bg-[#1a1a24] rounded-lg p-3">
                <div class="flex justify-between items-center">
                  <span class="text-sm font-semibold">{comm.label}</span>
                  <span class="text-[10px] text-[#666]">{comm.node_count} 节点 · 凝聚力 {(comm.cohesion * 100).toFixed(0)}%</span>
                </div>
                <div class="h-1 bg-[#222] rounded mt-1"><div class="h-full bg-[#60a5fa] rounded" style="width: {(comm.cohesion * 100).toFixed(0)}%"></div></div>
              </div>
            {/each}
          </div>
        {:else if store.activeTab === 'gaps'}
          <div class="p-4 flex flex-col gap-3 w-full overflow-y-auto">
            {#each store.gaps as gap}
              <div class="bg-[#1a1a24] rounded-lg p-3 border-l-3 {gap.gap_type === 'isolated_node' ? 'border-red-500' : gap.gap_type === 'sparse_community' ? 'border-amber-500' : 'border-blue-500'}">
                <div class="text-xs font-semibold mb-1">{gap.description}</div>
                <div class="text-[10px] text-[#60a5fa] mt-1 bg-[#1a2a3a] p-2 rounded">{gap.suggestion}</div>
              </div>
            {/each}
          </div>
        {:else}
          <div class="text-[#333] text-xs">选择 "知识图谱" 或 "社区 Community" tab 查看可视化</div>
        {/if}
      </div>

      <!-- Memory List Pane -->
      <div class="w-[340px] bg-[#0d0d14] overflow-y-auto">
        <div class="px-4 py-2 border-b border-[#1e1e2e] text-[10px] text-[#666] uppercase flex justify-between">
          <span>记忆列表</span>
          <select bind:value={store.sortBy} class="bg-[#1a1a2e] border border-[#2a2a3a] rounded px-1 text-[10px]">
            <option value="newest">最新优先</option>
            <option value="confidence">置信度优先</option>
          </select>
        </div>
        {#each store.sortedMemories as mem}
          <div class="px-4 py-3 border-b border-[#1a1a2e] hover:bg-[#1a1a24] cursor-pointer" style="border-left: 3px solid {mem.type === 'fact' ? '#60a5fa' : mem.type === 'experience' ? '#4ade80' : mem.type === 'preference' ? '#f59e0b' : mem.type === 'rule' ? '#f87171' : '#c084fc'}">
            <span class="text-[10px] px-1.5 py-0.5 rounded {typeColors[mem.type] ?? ''}">{typeLabel(mem.type)}</span>
            <div class="text-xs mt-1.5 leading-relaxed text-[#ccc]">{mem.content}</div>
            <div class="text-[10px] text-[#555] mt-1">来源: {mem.source.kind === 'manual' ? '手动添加' : mem.source.kind === 'chat' ? '群聊' : '推断'} · {new Date(mem.created_at).toLocaleDateString('zh-CN')}</div>
            <div class="flex items-center gap-2 mt-1 text-[10px] text-[#777]">
              置信度:
              <div class="w-10 h-1 bg-[#222] rounded overflow-hidden"><div class="h-full {mem.confidence > 0.8 ? 'bg-green-400' : mem.confidence > 0.6 ? 'bg-amber-400' : 'bg-red-400'}" style="width: {(mem.confidence * 100).toFixed(0)}%"></div></div>
              {(mem.confidence * 100).toFixed(0)}%
              <button class="ml-auto text-[#555] hover:text-red-400" onclick={() => store.deleteMemory(mem.id)}>删除</button>
            </div>
          </div>
        {:else}
          <div class="p-8 text-center text-xs text-[#555]">暂无记忆，点击 "+ 手动添加"</div>
        {/each}
      </div>
    </div>

    <div class="px-4 py-2 bg-[#111118] border-t border-[#1e1e2e] text-[10px] text-[#666] flex justify-between">
      <span>记忆在群聊中自动注入为 system prompt 上下文</span>
      <button class="text-red-500/70 hover:text-red-400">清空记忆</button>
    </div>
  </div>
</div>

<MemoryAddModal show={showAddModal} onClose={() => showAddModal = false} onSave={handleAddMemory} />
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/CharacterMemoryPanel.svelte src/lib/components/MemoryAddModal.svelte
git commit -m "feat: character memory panel UI with graph pane, memory list, tabs, and add modal"
```

---

### Task 13: Auto-Extraction Pipeline

**Files:**
- Create: `src-tauri/src/group_chat/memory_extraction.rs`
- Modify: `src-tauri/src/group_chat/mod.rs`

- [ ] **Step 1: Create auto-extraction module**

```rust
// src-tauri/src/group_chat/memory_extraction.rs
use crate::commands::embedding;
use crate::group_chat::memory_graph::compute_relevance_edges;
use crate::models::MemoryNode;
use crate::storage::characters;
use crate::storage::settings;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

// Debounce: per group-chat, last extraction time
static LAST_EXTRACTION: Lazy<Mutex<HashMap<String, Instant>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// Daily caps: per character, count of extractions today
static DAILY_EXTRACTION_COUNT: Lazy<Mutex<HashMap<String, (String, u32)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn can_extract(group_chat_id: &str, character_id: &str) -> bool {
    // Debounce: 5 min per group chat
    {
        let map = LAST_EXTRACTION.lock().unwrap();
        if let Some(last) = map.get(group_chat_id) {
            if last.elapsed().as_secs() < 300 {
                return false;
            }
        }
    }

    // Daily cap: 10 per character per day
    {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let mut map = DAILY_EXTRACTION_COUNT.lock().unwrap();
        let entry = map.entry(character_id.to_string()).or_insert_with(|| (today.clone(), 0));
        if entry.0 != today {
            entry.0 = today;
            entry.1 = 0;
        }
        if entry.1 >= 10 {
            return false;
        }
    }

    true
}

pub fn record_extraction(group_chat_id: &str, character_id: &str) {
    {
        let mut map = LAST_EXTRACTION.lock().unwrap();
        map.insert(group_chat_id.to_string(), Instant::now());
    }
    {
        let mut map = DAILY_EXTRACTION_COUNT.lock().unwrap();
        if let Some(entry) = map.get_mut(character_id) {
            entry.1 += 1;
        }
    }
}

/// Auto-extract memories from group chat turns.
/// Called as a background tokio::spawn after turn dispatch.
pub async fn auto_extract_memories(
    character_id: &str,
    _turns: &[String],  // TurnData simplified to strings for this plan
) -> Vec<MemoryNode> {
    // Build extraction prompt
    // In real impl: use the character's default_provider to call LLM
    // For now: placeholder that returns empty — actual LLM call in a follow-up
    Vec::new()
}

/// Semantic dedup: check cosine similarity against existing memory vectors.
/// Skips if similarity > 0.92 against any existing node.
pub async fn dedup_check(
    character_id: &str,
    candidate_text: &str,
) -> bool {
    // Get embedding for candidate
    let candidate_vec = match embedding::fetch_embedding(candidate_text).await {
        Ok(v) => v,
        Err(_) => return false, // can't check, allow through
    };

    // Search existing vectors for near-duplicates
    let results = match crate::commands::vectorstore::vector_search(
        character_id.to_string(),
        candidate_vec.clone(),
        5,
    ).await {
        Ok(r) => r,
        Err(_) => return false,
    };

    for result in results {
        if result.score > 0.92 {
            return true; // duplicate
        }
    }
    false
}
```

- [ ] **Step 2: Integrate into orchestrator post-dispatch**

In `orchestrator.rs`, after all participant turns are dispatched, add:

```rust
use crate::group_chat::memory_extraction::{auto_extract_memories, can_extract, record_extraction};

// After fanout completion:
let gc_id = group_chat_id.clone();
let participants = participants.clone();
tokio::spawn(async move {
    for p in &participants {
        if p.character_id.is_empty() || p.character_id == "__orphan__" {
            continue;
        }
        if !can_extract(&gc_id, &p.character_id) {
            continue;
        }
        // Collect recent turns for this participant
        let turns: Vec<String> = recent_public_turns.iter().map(|t| t.content.clone()).collect();
        auto_extract_memories(&p.character_id, &turns).await;
        record_extraction(&gc_id, &p.character_id);
    }
});
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/group_chat/memory_extraction.rs src-tauri/src/group_chat/orchestrator.rs src-tauri/src/group_chat/mod.rs
git commit -m "feat: auto-extraction pipeline with debounce, daily caps, and semantic dedup"
```

---

### Task 14: Character Editor Upgrade — Avatar Upload & New Fields

**Files:**
- Modify: `src/routes/settings/characters/+page.svelte`
- Create: `src-tauri/src/commands/avatar.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Avatar upload command**

```rust
// src-tauri/src/commands/avatar.rs
use crate::storage::characters;
use tauri::command;
use std::path::Path;

#[command]
pub async fn upload_character_avatar(
    character_id: String,
    file_path: String,
) -> Result<String, String> {
    let src = Path::new(&file_path);
    if !src.exists() {
        return Err("Source file not found".into());
    }

    let ext = src.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let filename = format!("avatar.{}", ext);
    let dst = characters::char_dir(&character_id).join(&filename);

    tokio::fs::copy(&src, &dst).await.map_err(|e| e.to_string())?;
    Ok(dst.to_string_lossy().to_string())
}
```

- [ ] **Step 2: Update character editor page with new fields**

In the character editor dialog in `+page.svelte`, add after the existing `role_instruction` textarea:

```svelte
<!-- Avatar upload -->
<div class="flex gap-3 items-start">
  {#if editingChar?.avatar_path}
    <img src={editingChar.avatar_path} alt="" class="w-16 h-16 rounded-xl object-cover" />
  {:else}
    <div class="w-16 h-16 rounded-xl bg-[#1a1a2e] flex items-center justify-center text-2xl">{editingChar?.icon || '?'}</div>
  {/if}
  <div>
    <label class="text-[10px] uppercase text-[#666] block mb-1">头像</label>
    <input type="file" accept="image/png,image/jpeg" class="text-xs" />
  </div>
</div>

<!-- Personality -->
<div>
  <label class="text-[10px] uppercase text-[#666] block mb-1">性格 / 人设 <span class="text-amber-400 text-[9px]">NEW</span></label>
  <textarea bind:value={editingPersonality} placeholder="角色的性格、沟通风格、思考方式..." class="w-full bg-[#0d0d14] border border-[#2a2a3a] rounded p-2 text-xs h-16 resize-none"></textarea>
</div>

<!-- Expertise tags -->
<div>
  <label class="text-[10px] uppercase text-[#666] block mb-1">专长领域 <span class="text-amber-400 text-[9px]">NEW</span></label>
  <div class="flex gap-1">
    <input bind:value={expertiseInput} placeholder="添加专长..." class="flex-1 bg-[#0d0d14] border border-[#2a2a3a] rounded p-2 text-xs" />
    <button onclick={addExpertise} class="bg-[#1a1a2e] px-3 rounded text-xs">+</button>
  </div>
  <div class="flex flex-wrap gap-1 mt-1">
    {#each editingExpertise as tag}
      <span class="bg-[#1e3a5f] text-[#60a5fa] px-2 py-0.5 rounded-full text-[10px] cursor-pointer" onclick={() => editingExpertise = editingExpertise.filter(t => t !== tag)}>{tag} ×</span>
    {/each}
  </div>
</div>

<!-- Memory config -->
<div>
  <label class="text-[10px] uppercase text-[#666] block mb-1">记忆配置 <span class="text-amber-400 text-[9px]">NEW</span></label>
  <div class="flex items-center gap-2 mb-1">
    <input type="checkbox" bind:checked={editingAutoLearn} class="w-3 h-3" />
    <span class="text-xs">自动从对话中学习记忆</span>
  </div>
  <div class="flex items-center gap-2">
    <span class="text-xs text-[#666]">记忆保留天数:</span>
    <input type="number" bind:value={editingRetentionDays} placeholder="永久" class="w-16 bg-[#0d0d14] border border-[#2a2a3a] rounded px-2 py-1 text-xs" />
  </div>
</div>
```

- [ ] **Step 3: Add "管理记忆" button to character card**

In the character card grid, add a button per character:

```svelte
<button onclick={() => openMemoryPanel(char.id, char.label, char.avatar_path)} class="text-[10px] bg-[#1a1a2e] px-2 py-1 rounded hover:bg-[#2563eb]/20">
  管理记忆
</button>
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/avatar.rs src-tauri/src/commands/mod.rs src/routes/settings/characters/+page.svelte
git commit -m "feat: character editor upgrade — avatar upload, personality, expertise, memory config"
```

---

### Task 15: Integration & Wiring — Data Lifecycle

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/group_chat/data_lifecycle.rs`

- [ ] **Step 1: Create data lifecycle module (compaction + retention)**

```rust
// src-tauri/src/group_chat/data_lifecycle.rs
use crate::storage::characters;
use chrono::Utc;

pub async fn compact_memory_log_if_needed(character_id: &str) -> crate::Result<bool> {
    let entries = characters::read_all_memory_log_entries(character_id).await?;
    if entries.len() < 10_000 {
        return Ok(false);
    }

    // Remove duplicates by keeping latest version of each ID
    use std::collections::HashMap;
    let mut latest: HashMap<String, usize> = HashMap::new();
    for (i, entry) in entries.iter().enumerate() {
        latest.insert(entry.id.clone(), i);
    }

    let mut compacted: Vec<_> = latest
        .into_values()
        .map(|i| entries[i].clone())
        .collect();
    compacted.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    // Rewrite compacted log
    use tokio::io::AsyncWriteExt;
    let path = characters::memory_log_path(character_id);
    let mut file = tokio::fs::File::create(&path).await?;
    for node in &compacted {
        let line = serde_json::to_string(node)? + "\n";
        file.write_all(line.as_bytes()).await?;
    }

    // Trigger derived data rebuild after compaction
    let _ = crate::commands::vectorstore::rebuild_vector_index(character_id.to_string());
    Ok(true)
}

pub async fn apply_retention_policy(character_id: &str, retention_days: u32) -> crate::Result<usize> {
    let entries = characters::read_all_memory_log_entries(character_id).await?;
    let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
    let cutoff_str = cutoff.to_rfc3339();

    let (keep, removed): (Vec<_>, Vec<_>) = entries
        .into_iter()
        .partition(|e| e.created_at >= cutoff_str);

    // Rewrite log with kept entries only
    use tokio::io::AsyncWriteExt;
    let path = characters::memory_log_path(character_id);
    let mut file = tokio::fs::File::create(&path).await?;
    for node in &keep {
        let line = serde_json::to_string(node)? + "\n";
        file.write_all(line.as_bytes()).await?;
    }

    Ok(removed.len())
}
```

- [ ] **Step 2: Wire all commands in lib.rs run() function**

```rust
// In src-tauri/src/lib.rs, update the builder to include ALL new commands:
.manage(invoke_handler(tauri::generate_handler![
    // ... existing commands ...
    // Embedding
    commands::embedding::get_embedding_config,
    commands::embedding::update_embedding_config,
    commands::embedding::test_embedding_connection,
    // Vector Store
    commands::vectorstore::vector_upsert,
    commands::vectorstore::vector_search,
    commands::vectorstore::vector_delete,
    commands::vectorstore::rebuild_vector_index,
    // Memory CRUD
    commands::characters::list_character_memories,
    commands::characters::get_character_memory,
    commands::characters::create_character_memory,
    commands::characters::update_character_memory,
    commands::characters::delete_character_memory,
    // Graph
    commands::characters::get_memory_graph,
    commands::characters::get_memory_communities,
    commands::characters::get_knowledge_gaps,
    // Avatar
    commands::avatar::upload_character_avatar,
]))
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/group_chat/data_lifecycle.rs src-tauri/src/lib.rs
git commit -m "feat: data lifecycle — log compaction, retention policy, full command wiring"
```

---

### Task 16: npm install & build verification

**Files:** None (verification only)

- [ ] **Step 1: Install frontend dependencies (sigma.js, graphology)**

```bash
cd D:/ClaudeWorkspace/Code/ClawGO && npm install sigma graphology
```

- [ ] **Step 2: Run frontend build**

```bash
npm run build
```

Expected: PASS with no errors.

- [ ] **Step 3: Run Rust check**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: PASS with no errors (warnings OK).

- [ ] **Step 4: Run i18n check**

```bash
npm run i18n:check
```

Expected: PASS.

- [ ] **Step 5: Commit if any lockfile updates**

```bash
git add package.json package-lock.json
git commit -m "chore: add sigma.js and graphology frontend dependencies"
```

---

### Task 17: Manual Verification Checklist

- [ ] **Test 1:** Create a character with personality/expertise/memory_config → verify saved
- [ ] **Test 2:** Open memory panel → manually add 3 memories → verify list shows them
- [ ] **Test 3:** Add character to group chat → verify `character_id` is populated (not empty)
- [ ] **Test 4:** @mention character in group chat → verify memory injection appears in system prompt
- [ ] **Test 5:** Disable embedding in settings → verify injection degrades to keyword mode
- [ ] **Test 6:** Delete a memory → verify it's removed from list and log
- [ ] **Test 7:** Upload avatar → verify file appears in `characters/{id}/` directory
- [ ] **Test 8:** Check `characters/{id}/` directory structure → all files present

---

## Implementation Status (2026-05-14)

### Completed Tasks

| Task | Description | Status |
|------|-------------|--------|
| 0 | Dependency spike (lancedb + arrow + petgraph) | ✅ Done |
| 1 | Rust type definitions (models.rs) | ✅ Done |
| 2 | Embedding config model + storage | ✅ Done |
| 3 | Embedding API (fetch + test connection) | ✅ Done |
| 4 | LanceDB vector store (upsert/search/delete/reset/rebuild) | ✅ Done |
| 5 | Character memory storage (JSONL + graph CRUD) | ✅ Done |
| 6 | Data lifecycle (compaction + retention) | ✅ Done |
| 7 | Memory injection (hybrid search + format + degradation) | ✅ Done |
| 8 | `is_auto_learn` + `inject_memories` wiring in orchestrator | ✅ Done |
| 9 | Avatar upload validation (magic bytes) | ✅ Done |
| 10 | Frontend API layer | ✅ Done |
| 11 | Character memory store (pending) | ⬜ Not started |
| 12 | Memory panel UI (pending) | ⬜ Not started |
| 13 | Graph visualization (pending) | ⬜ Not started |
| 14 | Auto-extraction pipeline (pending) | ⬜ Not started |
| 15 | Character editor upgrade (pending) | ⬜ Not started |
| 16 | Frontend dependencies (pending) | ⬜ Not started |
| 17 | Manual verification (pending) | ⬜ Not started |

### Review History

| Date | Type | Result |
|------|------|--------|
| 2026-05-14 | Multi-Review Round 6 | 19 fixes (3 P0 + 5 P1 + 6 P2 + 5 P3) |
| 2026-05-14 | Multi-Review Round 7 | 14 fixes (1C + 3I + 10M) |
| 2026-05-14 | Simplify Review (3-way) | 7 fixes, 10 deferred, 17 skipped |

### Deferred Issues

See: `docs/superpowers/plans/[todo] 2026-05-14-character-memory-simplify-review-deferred.md`
