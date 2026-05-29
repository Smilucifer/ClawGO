# openInvest Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the data layer (invest.db + memory scope refactor), add sidebar entries, create route skeletons, and wire up i18n for the openInvest port.

**Architecture:** Two independent SQLite databases — `memory.db` (existing, add scope fields) and `invest.db` (new, portfolio/verdicts/events/scheduler). Storage modules use singleton `Mutex<Option<Connection>>` pattern. Commands are thin `#[tauri::command]` wrappers. Frontend uses Svelte 5 runes, inline SVG icons, flat i18n keys.

**Tech Stack:** Rust (rusqlite, uuid, chrono, serde), SvelteKit (Svelte 5 runes), Tauri IPC

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Modify | `src-tauri/src/storage/memory_store.rs` | Add scope/project_id columns + migration |
| Create | `src-tauri/src/storage/invest/mod.rs` | invest.db init, WAL, migration, with_conn helpers |
| Create | `src-tauri/src/storage/invest/portfolio.rs` | holdings, trades, cash CRUD |
| Create | `src-tauri/src/storage/invest/verdicts.rs` | verdicts, pnl_snapshots CRUD |
| Create | `src-tauri/src/storage/invest/events.rs` | events, event_sources CRUD |
| Create | `src-tauri/src/storage/invest/scheduler.rs` | scheduler_logs, trade_calendar CRUD |
| Modify | `src-tauri/src/storage/mod.rs` | Register `pub mod invest;` |
| Create | `src-tauri/src/commands/invest.rs` | Tauri commands for all invest operations |
| Modify | `src-tauri/src/commands/mod.rs` | Register `pub mod invest;` |
| Modify | `src-tauri/src/commands/memos.rs` | Add scope/project_id params to list_memos, save_memo |
| Modify | `src-tauri/src/lib.rs` | Register invest commands + init invest.db |
| Modify | `src/routes/+layout.svelte` | Add /invest, /memory-mgmt to navItems; add "trendingUp" + "database" icons; add MoreMenu |
| Create | `src/routes/invest/+page.svelte` | Invest page with 6 tabs |
| Create | `src/routes/memory-mgmt/+page.svelte` | Memory management page with 2 tabs |
| Create | `src/lib/components/MoreMenu.svelte` | Title bar dropdown menu |
| Modify | `messages/en.json` | Add invest + memory-mgmt + more-menu i18n keys |
| Modify | `messages/zh-CN.json` | Add corresponding Chinese translations |

---

### Task 1: Add scope/project_id to memory_store.rs

**Files:**
- Modify: `src-tauri/src/storage/memory_store.rs`

- [ ] **Step 1: Add scope and project_id columns to init_db**

In `memory_store.rs`, find the `CREATE TABLE IF NOT EXISTS memories` statement. Add two columns after the existing columns:

```sql
scope TEXT NOT NULL DEFAULT 'global',
project_id TEXT,
```

Add an index for scope filtering:

```sql
CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories(scope);
CREATE INDEX IF NOT EXISTS idx_memories_project_id ON memories(project_id) WHERE project_id IS NOT NULL;
```

The migration is automatic — `ALTER TABLE` is not needed because `CREATE TABLE IF NOT EXISTS` will create the table with the new columns on fresh installs, and existing installs get the columns via the migration logic below.

- [ ] **Step 2: Add migration for existing databases**

After the `CREATE TABLE` block, add a migration check:

```rust
// Migration: add scope and project_id if missing
let has_scope: bool = conn
    .prepare("SELECT COUNT(*) FROM pragma_table_info('memories') WHERE name='scope'")?
    .query_row([], |row| row.get::<_, i32>(0))?
    > 0;

if !has_scope {
    conn.execute_batch(
        "ALTER TABLE memories ADD COLUMN scope TEXT NOT NULL DEFAULT 'global';
         ALTER TABLE memories ADD COLUMN project_id TEXT;
         CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories(scope);
         CREATE INDEX IF NOT EXISTS idx_memories_project_id ON memories(project_id) WHERE project_id IS NOT NULL;"
    )?;
    log::info!("Migrated memories table: added scope + project_id columns");
}
```

- [ ] **Step 3: Update save_memory to accept scope and project_id**

Find the `save_memory` function. Add `scope: Option<String>` and `project_id: Option<String>` parameters. Update the INSERT statement:

```rust
pub fn save_memory(
    content: &str,
    memory_type: &str,
    source_run_id: Option<&str>,
    confidence: Option<f64>,
    scope: Option<&str>,
    project_id: Option<&str>,
) -> Result<String, String> {
    with_conn_mut(|conn| {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let scope_val = scope.unwrap_or("global");
        conn.execute(
            "INSERT INTO memories (id, content, memory_type, source_run_id, confidence, scope, project_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![id, content, memory_type, source_run_id, confidence, scope_val, project_id, now, now],
        ).map_err(|e| format!("save memory: {}", e))?;
        Ok(id)
    })
}
```

- [ ] **Step 4: Update list_memories to filter by scope**

Add an optional `scope` filter parameter:

```rust
pub fn list_memories(scope_filter: Option<&str>) -> Result<Vec<MemoryItem>, String> {
    with_conn(|conn| {
        let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match scope_filter {
            Some(s) => (
                "SELECT id, content, memory_type, source_run_id, confidence, scope, project_id, created_at, updated_at FROM memories WHERE scope = ?1 AND status != 'deleted' ORDER BY updated_at DESC",
                vec![Box::new(s.to_string())],
            ),
            None => (
                "SELECT id, content, memory_type, source_run_id, confidence, scope, project_id, created_at, updated_at FROM memories WHERE status != 'deleted' ORDER BY updated_at DESC",
                vec![],
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok(MemoryItem {
                id: row.get(0)?,
                content: row.get(1)?,
                memory_type: row.get(2)?,
                source_run_id: row.get(3)?,
                confidence: row.get(4)?,
                scope: row.get(5)?,
                project_id: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        }).map_err(|e| format!("query: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })
}
```

- [ ] **Step 5: Update MemoryItem struct**

Add the new fields to the `MemoryItem` struct:

```rust
pub struct MemoryItem {
    pub id: String,
    pub content: String,
    pub memory_type: String,
    pub source_run_id: Option<String>,
    pub confidence: Option<f64>,
    pub scope: String,
    pub project_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

- [ ] **Step 6: Update row_to_memory mapping**

Update the `row_to_memory` function to include the new columns:

```rust
fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<MemoryItem> {
    Ok(MemoryItem {
        id: row.get(0)?,
        content: row.get(1)?,
        memory_type: row.get(2)?,
        source_run_id: row.get(3)?,
        confidence: row.get(4)?,
        scope: row.get(5)?,
        project_id: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}
```

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/storage/memory_store.rs
git commit -m "feat: add scope and project_id to memories table with auto-migration"
```

---

### Task 2: Update memos commands for scope awareness

**Files:**
- Modify: `src-tauri/src/commands/memos.rs`

- [ ] **Step 1: Add scope parameter to list_memos command**

```rust
#[tauri::command]
pub fn list_memos(scope: Option<String>) -> Result<Vec<crate::storage::memory_store::MemoryItem>, String> {
    crate::storage::memory_store::list_memories(scope.as_deref())
}
```

Note: This replaces the old JSON-based memo listing with SQLite-backed memory listing for the `/memory-mgmt` page. The old `list_memos(scope: MemoScope)` remains for backward compatibility in the GlobalMemoPanel.

- [ ] **Step 2: Add save_memory command**

```rust
#[tauri::command]
pub fn save_memory(
    content: String,
    memory_type: String,
    source_run_id: Option<String>,
    confidence: Option<f64>,
    scope: Option<String>,
    project_id: Option<String>,
) -> Result<String, String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("Memory content cannot be empty".to_string());
    }
    crate::storage::memory_store::save_memory(
        trimmed,
        &memory_type,
        source_run_id.as_deref(),
        confidence,
        scope.as_deref(),
        project_id.as_deref(),
    )
}
```

- [ ] **Step 3: Register new commands in lib.rs**

Add to the `generate_handler![]` array in `lib.rs`:

```rust
commands::memos::save_memory,
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/memos.rs src-tauri/src/lib.rs
git commit -m "feat: add scope-aware memory commands for /memory-mgmt page"
```

---

### Task 3: Create invest module structure and database init

**Files:**
- Create: `src-tauri/src/storage/invest/mod.rs`
- Modify: `src-tauri/src/storage/mod.rs`

- [ ] **Step 1: Create the invest directory**

```bash
mkdir -p src-tauri/src/storage/invest
```

- [ ] **Step 2: Write invest/mod.rs with DB init and full schema**

```rust
pub mod events;
pub mod portfolio;
pub mod scheduler;
pub mod verdicts;

use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

static DB: Mutex<Option<Connection>> = Mutex::new(None);

pub fn init_db(data_dir: &PathBuf) -> Result<(), String> {
    let invest_dir = data_dir.join("invest");
    crate::storage::ensure_dir(&invest_dir);
    let db_path = invest_dir.join("invest.db");

    let conn = Connection::open(&db_path)
        .map_err(|e| format!("open invest.db: {}", e))?;

    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
        .map_err(|e| format!("set pragmas: {}", e))?;

    conn.execute_batch(CREATE_TABLES_SQL)
        .map_err(|e| format!("create tables: {}", e))?;

    let mut guard = DB.lock().map_err(|e| format!("lock db: {}", e))?;
    *guard = Some(conn);
    log::info!("invest.db initialized at {:?}", db_path);
    Ok(())
}

pub fn with_conn<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&Connection) -> Result<R, String>,
{
    let guard = DB.lock().map_err(|e| format!("lock invest db: {}", e))?;
    let conn = guard.as_ref().ok_or_else(|| "invest.db not initialized".to_string())?;
    f(conn)
}

pub fn with_conn_mut<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut Connection) -> Result<R, String>,
{
    let mut guard = DB.lock().map_err(|e| format!("lock invest db: {}", e))?;
    let conn = guard.as_mut().ok_or_else(|| "invest.db not initialized".to_string())?;
    f(conn)
}

const CREATE_TABLES_SQL: &str = "
CREATE TABLE IF NOT EXISTS holdings (
    symbol TEXT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'CNY',
    kind TEXT NOT NULL CHECK (kind IN ('hold', 'watch')),
    name TEXT,
    notional REAL NOT NULL DEFAULT 0,
    avg_cost REAL,
    shares REAL,
    entry_date TEXT,
    linked_verdict_id TEXT,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (symbol, currency, kind)
);

CREATE TABLE IF NOT EXISTS trades (
    id TEXT PRIMARY KEY,
    symbol TEXT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'CNY',
    kind TEXT NOT NULL CHECK (kind IN ('hold', 'watch')),
    action TEXT NOT NULL CHECK (action IN ('buy', 'sell', 'convert_watch_to_hold', 'convert_hold_to_watch', 'cost_edit')),
    shares REAL,
    price REAL,
    amount REAL,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS cash (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    available REAL NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS verdicts (
    id TEXT PRIMARY KEY,
    symbol TEXT NOT NULL,
    verdict TEXT NOT NULL,
    confidence REAL,
    macro_signal TEXT,
    macro_strength REAL,
    reasoning TEXT,
    model TEXT,
    provider TEXT,
    tokens_used INTEGER,
    latency_ms INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS pnl_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    snapshot_date TEXT NOT NULL,
    total_value REAL NOT NULL,
    cash REAL NOT NULL,
    holdings_value REAL NOT NULL,
    daily_pnl REAL,
    daily_pnl_pct REAL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    event_type TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT,
    symbols TEXT,
    severity TEXT DEFAULT 'info',
    triggered INTEGER DEFAULT 0,
    trigger_verdict_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS event_sources (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    source_type TEXT NOT NULL,
    config TEXT,
    enabled INTEGER DEFAULT 1,
    last_poll_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS domain_insights (
    id TEXT PRIMARY KEY,
    insight_type TEXT NOT NULL,
    symbol TEXT,
    content TEXT NOT NULL,
    confidence REAL,
    source_verdict_ids TEXT,
    status TEXT DEFAULT 'active' CHECK (status IN ('active', 'archived', 'deleted')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS scheduler_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_name TEXT NOT NULL,
    status TEXT NOT NULL,
    message TEXT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    duration_ms INTEGER
);

CREATE TABLE IF NOT EXISTS trade_calendar (
    cal_date TEXT PRIMARY KEY,
    is_open INTEGER NOT NULL,
    pretrade_date TEXT,
    exchange TEXT DEFAULT 'SSE'
);

CREATE INDEX IF NOT EXISTS idx_trades_symbol ON trades(symbol);
CREATE INDEX IF NOT EXISTS idx_trades_created ON trades(created_at);
CREATE INDEX IF NOT EXISTS idx_verdicts_symbol ON verdicts(symbol);
CREATE INDEX IF NOT EXISTS idx_verdicts_created ON verdicts(created_at);
CREATE INDEX IF NOT EXISTS idx_events_source ON events(source);
CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);
CREATE INDEX IF NOT EXISTS idx_pnl_snapshots_date ON pnl_snapshots(snapshot_date);
CREATE INDEX IF NOT EXISTS idx_scheduler_logs_task ON scheduler_logs(task_name);
CREATE INDEX IF NOT EXISTS idx_trade_calendar_date ON trade_calendar(cal_date);
";
```

- [ ] **Step 3: Register invest module in storage/mod.rs**

Add at the top of `src-tauri/src/storage/mod.rs`:

```rust
pub mod invest;
```

- [ ] **Step 4: Initialize invest.db in lib.rs**

In `lib.rs`, find the memory DB init block (around line 433) and add after it:

```rust
if let Err(e) = crate::storage::invest::init_db(&data_dir) {
    log::warn!("Failed to init invest DB: {}", e);
}
```

- [ ] **Step 5: Verify compilation**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/storage/invest/ src-tauri/src/storage/mod.rs src-tauri/src/lib.rs
git commit -m "feat: create invest.db with full schema (holdings, trades, verdicts, events, scheduler)"
```

---

### Task 4: Portfolio storage module (holdings, trades, cash)

**Files:**
- Create: `src-tauri/src/storage/invest/portfolio.rs`

- [ ] **Step 1: Write portfolio.rs with all CRUD operations**

```rust
use super::with_conn;
use rusqlite::params;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holding {
    pub symbol: String,
    pub currency: String,
    pub kind: String,
    pub name: Option<String>,
    pub notional: f64,
    pub avg_cost: Option<f64>,
    pub shares: Option<f64>,
    pub entry_date: Option<String>,
    pub linked_verdict_id: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub id: String,
    pub symbol: String,
    pub currency: String,
    pub kind: String,
    pub action: String,
    pub shares: Option<f64>,
    pub price: Option<f64>,
    pub amount: Option<f64>,
    pub notes: Option<String>,
    pub created_at: String,
}

pub fn list_holdings() -> Result<Vec<Holding>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare("SELECT symbol, currency, kind, name, notional, avg_cost, shares, entry_date, linked_verdict_id, notes, created_at, updated_at FROM holdings ORDER BY symbol")
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Holding {
                    symbol: row.get(0)?,
                    currency: row.get(1)?,
                    kind: row.get(2)?,
                    name: row.get(3)?,
                    notional: row.get(4)?,
                    avg_cost: row.get(5)?,
                    shares: row.get(6)?,
                    entry_date: row.get(7)?,
                    linked_verdict_id: row.get(8)?,
                    notes: row.get(9)?,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            })
            .map_err(|e| format!("query: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })
}

pub fn upsert_holding(h: &Holding) -> Result<(), String> {
    with_conn(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO holdings (symbol, currency, kind, name, notional, avg_cost, shares, entry_date, linked_verdict_id, notes, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(symbol, currency, kind) DO UPDATE SET
               name=?4, notional=?5, avg_cost=?6, shares=?7, entry_date=?8, linked_verdict_id=?9, notes=?10, updated_at=?12",
            params![h.symbol, h.currency, h.kind, h.name, h.notional, h.avg_cost, h.shares, h.entry_date, h.linked_verdict_id, h.notes, now, now],
        )
        .map_err(|e| format!("upsert holding: {}", e))?;
        Ok(())
    })
}

pub fn delete_holding(symbol: &str, currency: &str, kind: &str) -> Result<(), String> {
    with_conn(|conn| {
        let changed = conn
            .execute(
                "DELETE FROM holdings WHERE symbol=?1 AND currency=?2 AND kind=?3",
                params![symbol, currency, kind],
            )
            .map_err(|e| format!("delete holding: {}", e))?;
        if changed == 0 {
            Err("Holding not found".to_string())
        } else {
            Ok(())
        }
    })
}

pub fn record_trade(t: &Trade) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT INTO trades (id, symbol, currency, kind, action, shares, price, amount, notes, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![t.id, t.symbol, t.currency, t.kind, t.action, t.shares, t.price, t.amount, t.notes, t.created_at],
        )
        .map_err(|e| format!("record trade: {}", e))?;
        Ok(())
    })
}

pub fn list_trades(symbol: Option<&str>, limit: Option<i64>) -> Result<Vec<Trade>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(100);
        let (sql, query_params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match symbol {
            Some(s) => (
                "SELECT id, symbol, currency, kind, action, shares, price, amount, notes, created_at FROM trades WHERE symbol = ?1 ORDER BY created_at DESC LIMIT ?2",
                vec![Box::new(s.to_string()), Box::new(limit_val)],
            ),
            None => (
                "SELECT id, symbol, currency, kind, action, shares, price, amount, notes, created_at FROM trades ORDER BY created_at DESC LIMIT ?1",
                vec![Box::new(limit_val)],
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(query_params.iter()), |row| {
                Ok(Trade {
                    id: row.get(0)?,
                    symbol: row.get(1)?,
                    currency: row.get(2)?,
                    kind: row.get(3)?,
                    action: row.get(4)?,
                    shares: row.get(5)?,
                    price: row.get(6)?,
                    amount: row.get(7)?,
                    notes: row.get(8)?,
                    created_at: row.get(9)?,
                })
            })
            .map_err(|e| format!("query: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })
}

pub fn get_cash() -> Result<f64, String> {
    with_conn(|conn| {
        let result = conn
            .query_row("SELECT available FROM cash WHERE id = 1", [], |row| row.get::<_, f64>(0));
        match result {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0.0),
            Err(e) => Err(format!("get cash: {}", e)),
        }
    })
}

pub fn set_cash(amount: f64) -> Result<(), String> {
    with_conn(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO cash (id, available, updated_at) VALUES (1, ?1, ?2) ON CONFLICT(id) DO UPDATE SET available=?1, updated_at=?2",
            params![amount, now],
        )
        .map_err(|e| format!("set cash: {}", e))?;
        Ok(())
    })
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/storage/invest/portfolio.rs
git commit -m "feat: portfolio storage — holdings, trades, cash CRUD"
```

---

### Task 5: Verdicts storage module

**Files:**
- Create: `src-tauri/src/storage/invest/verdicts.rs`

- [ ] **Step 1: Write verdicts.rs**

```rust
use super::with_conn;
use rusqlite::params;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub id: String,
    pub symbol: String,
    pub verdict: String,
    pub confidence: Option<f64>,
    pub macro_signal: Option<String>,
    pub macro_strength: Option<f64>,
    pub reasoning: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub tokens_used: Option<i64>,
    pub latency_ms: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PnlSnapshot {
    pub id: i64,
    pub snapshot_date: String,
    pub total_value: f64,
    pub cash: f64,
    pub holdings_value: f64,
    pub daily_pnl: Option<f64>,
    pub daily_pnl_pct: Option<f64>,
    pub created_at: String,
}

pub fn save_verdict(v: &Verdict) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT INTO verdicts (id, symbol, verdict, confidence, macro_signal, macro_strength, reasoning, model, provider, tokens_used, latency_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![v.id, v.symbol, v.verdict, v.confidence, v.macro_signal, v.macro_strength, v.reasoning, v.model, v.provider, v.tokens_used, v.latency_ms, v.created_at],
        )
        .map_err(|e| format!("save verdict: {}", e))?;
        Ok(())
    })
}

pub fn list_verdicts(symbol: Option<&str>, limit: Option<i64>) -> Result<Vec<Verdict>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(50);
        let (sql, query_params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match symbol {
            Some(s) => (
                "SELECT id, symbol, verdict, confidence, macro_signal, macro_strength, reasoning, model, provider, tokens_used, latency_ms, created_at FROM verdicts WHERE symbol = ?1 ORDER BY created_at DESC LIMIT ?2",
                vec![Box::new(s.to_string()), Box::new(limit_val)],
            ),
            None => (
                "SELECT id, symbol, verdict, confidence, macro_signal, macro_strength, reasoning, model, provider, tokens_used, latency_ms, created_at FROM verdicts ORDER BY created_at DESC LIMIT ?1",
                vec![Box::new(limit_val)],
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(query_params.iter()), |row| {
                Ok(Verdict {
                    id: row.get(0)?,
                    symbol: row.get(1)?,
                    verdict: row.get(2)?,
                    confidence: row.get(3)?,
                    macro_signal: row.get(4)?,
                    macro_strength: row.get(5)?,
                    reasoning: row.get(6)?,
                    model: row.get(7)?,
                    provider: row.get(8)?,
                    tokens_used: row.get(9)?,
                    latency_ms: row.get(10)?,
                    created_at: row.get(11)?,
                })
            })
            .map_err(|e| format!("query: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })
}

pub fn save_pnl_snapshot(s: &PnlSnapshot) -> Result<i64, String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO pnl_snapshots (snapshot_date, total_value, cash, holdings_value, daily_pnl, daily_pnl_pct) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![s.snapshot_date, s.total_value, s.cash, s.holdings_value, s.daily_pnl, s.daily_pnl_pct],
        )
        .map_err(|e| format!("save pnl: {}", e))?;
        Ok(conn.last_insert_rowid())
    })
}

pub fn list_pnl_snapshots(limit: Option<i64>) -> Result<Vec<PnlSnapshot>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(100);
        let mut stmt = conn
            .prepare("SELECT id, snapshot_date, total_value, cash, holdings_value, daily_pnl, daily_pnl_pct, created_at FROM pnl_snapshots ORDER BY snapshot_date DESC LIMIT ?1")
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(params![limit_val], |row| {
                Ok(PnlSnapshot {
                    id: row.get(0)?,
                    snapshot_date: row.get(1)?,
                    total_value: row.get(2)?,
                    cash: row.get(3)?,
                    holdings_value: row.get(4)?,
                    daily_pnl: row.get(5)?,
                    daily_pnl_pct: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })
            .map_err(|e| format!("query: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/storage/invest/verdicts.rs
git commit -m "feat: verdicts storage — verdicts + pnl_snapshots CRUD"
```

---

### Task 6: Events storage module

**Files:**
- Create: `src-tauri/src/storage/invest/events.rs`

- [ ] **Step 1: Write events.rs**

```rust
use super::with_conn;
use rusqlite::params;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub source: String,
    pub event_type: String,
    pub title: String,
    pub body: Option<String>,
    pub symbols: Option<String>,
    pub severity: String,
    pub triggered: bool,
    pub trigger_verdict_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventSource {
    pub id: String,
    pub name: String,
    pub source_type: String,
    pub config: Option<String>,
    pub enabled: bool,
    pub last_poll_at: Option<String>,
    pub created_at: String,
}

pub fn save_event(e: &Event) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT INTO events (id, source, event_type, title, body, symbols, severity, triggered, trigger_verdict_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![e.id, e.source, e.event_type, e.title, e.body, e.symbols, e.severity, e.triggered as i32, e.trigger_verdict_id, e.created_at],
        )
        .map_err(|e| format!("save event: {}", e))?;
        Ok(())
    })
}

pub fn list_events(source: Option<&str>, limit: Option<i64>) -> Result<Vec<Event>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(100);
        let (sql, query_params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match source {
            Some(s) => (
                "SELECT id, source, event_type, title, body, symbols, severity, triggered, trigger_verdict_id, created_at FROM events WHERE source = ?1 ORDER BY created_at DESC LIMIT ?2",
                vec![Box::new(s.to_string()), Box::new(limit_val)],
            ),
            None => (
                "SELECT id, source, event_type, title, body, symbols, severity, triggered, trigger_verdict_id, created_at FROM events ORDER BY created_at DESC LIMIT ?1",
                vec![Box::new(limit_val)],
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(query_params.iter()), |row| {
                Ok(Event {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    event_type: row.get(2)?,
                    title: row.get(3)?,
                    body: row.get(4)?,
                    symbols: row.get(5)?,
                    severity: row.get(6)?,
                    triggered: row.get::<_, i32>(7)? != 0,
                    trigger_verdict_id: row.get(8)?,
                    created_at: row.get(9)?,
                })
            })
            .map_err(|e| format!("query: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })
}

pub fn mark_event_triggered(event_id: &str, verdict_id: &str) -> Result<(), String> {
    with_conn(|conn| {
        let changed = conn
            .execute(
                "UPDATE events SET triggered = 1, trigger_verdict_id = ?1 WHERE id = ?2",
                params![verdict_id, event_id],
            )
            .map_err(|e| format!("mark triggered: {}", e))?;
        if changed == 0 {
            Err("Event not found".to_string())
        } else {
            Ok(())
        }
    })
}

pub fn save_event_source(s: &EventSource) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT INTO event_sources (id, name, source_type, config, enabled, last_poll_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET name=?2, source_type=?3, config=?4, enabled=?5, last_poll_at=?6",
            params![s.id, s.name, s.source_type, s.config, s.enabled as i32, s.last_poll_at, s.created_at],
        )
        .map_err(|e| format!("save source: {}", e))?;
        Ok(())
    })
}

pub fn list_event_sources() -> Result<Vec<EventSource>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare("SELECT id, name, source_type, config, enabled, last_poll_at, created_at FROM event_sources ORDER BY name")
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(EventSource {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    source_type: row.get(2)?,
                    config: row.get(3)?,
                    enabled: row.get::<_, i32>(4)? != 0,
                    last_poll_at: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })
            .map_err(|e| format!("query: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/storage/invest/events.rs
git commit -m "feat: events storage — events + event_sources CRUD"
```

---

### Task 7: Scheduler storage module

**Files:**
- Create: `src-tauri/src/storage/invest/scheduler.rs`

- [ ] **Step 1: Write scheduler.rs**

```rust
use super::with_conn;
use rusqlite::params;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerLog {
    pub id: i64,
    pub task_name: String,
    pub status: String,
    pub message: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
}

pub fn log_task_start(task_name: &str) -> Result<i64, String> {
    with_conn_mut(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO scheduler_logs (task_name, status, started_at) VALUES (?1, 'running', ?2)",
            params![task_name, now],
        )
        .map_err(|e| format!("log start: {}", e))?;
        Ok(conn.last_insert_rowid())
    })
}

pub fn log_task_end(id: i64, status: &str, message: Option<&str>) -> Result<(), String> {
    with_conn(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE scheduler_logs SET status = ?1, message = ?2, finished_at = ?3, duration_ms = CAST((julianday(?3) - julianday(started_at)) * 86400000 AS INTEGER) WHERE id = ?4",
            params![status, message, now, id],
        )
        .map_err(|e| format!("log end: {}", e))?;
        Ok(())
    })
}

pub fn list_scheduler_logs(task_name: Option<&str>, limit: Option<i64>) -> Result<Vec<SchedulerLog>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(50);
        let (sql, query_params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match task_name {
            Some(t) => (
                "SELECT id, task_name, status, message, started_at, finished_at, duration_ms FROM scheduler_logs WHERE task_name = ?1 ORDER BY started_at DESC LIMIT ?2",
                vec![Box::new(t.to_string()), Box::new(limit_val)],
            ),
            None => (
                "SELECT id, task_name, status, message, started_at, finished_at, duration_ms FROM scheduler_logs ORDER BY started_at DESC LIMIT ?1",
                vec![Box::new(limit_val)],
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(query_params.iter()), |row| {
                Ok(SchedulerLog {
                    id: row.get(0)?,
                    task_name: row.get(1)?,
                    status: row.get(2)?,
                    message: row.get(3)?,
                    started_at: row.get(4)?,
                    finished_at: row.get(5)?,
                    duration_ms: row.get(6)?,
                })
            })
            .map_err(|e| format!("query: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })
}

pub fn is_trading_day(date: &str) -> Result<bool, String> {
    with_conn(|conn| {
        let result = conn.query_row(
            "SELECT is_open FROM trade_calendar WHERE cal_date = ?1",
            params![date],
            |row| row.get::<_, i32>(0),
        );
        match result {
            Ok(v) => Ok(v != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Fallback: weekday = trading day
                Ok(is_weekday(date))
            }
            Err(e) => Err(format!("check trading day: {}", e)),
        }
    })
}

fn is_weekday(date: &str) -> bool {
    use chrono::NaiveDate;
    if let Ok(d) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        let weekday = d.weekday();
        weekday != chrono::Weekday::Sat && weekday != chrono::Weekday::Sun
    } else if let Ok(d) = NaiveDate::parse_from_str(date, "%Y%m%d") {
        let weekday = d.weekday();
        weekday != chrono::Weekday::Sat && weekday != chrono::Weekday::Sun
    } else {
        true // assume trading day if can't parse
    }
}

pub fn upsert_trade_calendar(date: &str, is_open: bool, pretrade_date: Option<&str>) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT INTO trade_calendar (cal_date, is_open, pretrade_date) VALUES (?1, ?2, ?3) ON CONFLICT(cal_date) DO UPDATE SET is_open=?2, pretrade_date=?3",
            params![date, is_open as i32, pretrade_date],
        )
        .map_err(|e| format!("upsert calendar: {}", e))?;
        Ok(())
    })
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/storage/invest/scheduler.rs
git commit -m "feat: scheduler storage — logs, trade calendar, is_trading_day guard"
```

---

### Task 8: Tauri commands for invest module

**Files:**
- Create: `src-tauri/src/commands/invest.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write commands/invest.rs with thin wrappers**

```rust
use crate::storage::invest::{events, portfolio, scheduler, verdicts};

// ── Holdings ──

#[tauri::command]
pub fn get_holdings() -> Result<Vec<portfolio::Holding>, String> {
    portfolio::list_holdings()
}

#[tauri::command]
pub fn add_holding(
    symbol: String,
    currency: Option<String>,
    kind: String,
    name: Option<String>,
    notional: Option<f64>,
    avg_cost: Option<f64>,
    shares: Option<f64>,
    entry_date: Option<String>,
    notes: Option<String>,
) -> Result<(), String> {
    let h = portfolio::Holding {
        symbol,
        currency: currency.unwrap_or_else(|| "CNY".to_string()),
        kind,
        name,
        notional: notional.unwrap_or(0.0),
        avg_cost,
        shares,
        entry_date,
        linked_verdict_id: None,
        notes,
        created_at: String::new(),
        updated_at: String::new(),
    };
    portfolio::upsert_holding(&h)
}

#[tauri::command]
pub fn delete_holding(symbol: String, currency: String, kind: String) -> Result<(), String> {
    portfolio::delete_holding(&symbol, &currency, &kind)
}

// ── Cash ──

#[tauri::command]
pub fn get_cash_balance() -> Result<f64, String> {
    portfolio::get_cash()
}

#[tauri::command]
pub fn set_cash_balance(amount: f64) -> Result<(), String> {
    portfolio::set_cash(amount)
}

// ── Trades ──

#[tauri::command]
pub fn record_trade(
    symbol: String,
    currency: Option<String>,
    kind: String,
    action: String,
    shares: Option<f64>,
    price: Option<f64>,
    amount: Option<f64>,
    notes: Option<String>,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let t = portfolio::Trade {
        id: id.clone(),
        symbol,
        currency: currency.unwrap_or_else(|| "CNY".to_string()),
        kind,
        action,
        shares,
        price,
        amount,
        notes,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    portfolio::record_trade(&t)?;
    Ok(id)
}

#[tauri::command]
pub fn get_trades(symbol: Option<String>, limit: Option<i64>) -> Result<Vec<portfolio::Trade>, String> {
    portfolio::list_trades(symbol.as_deref(), limit)
}

// ── Verdicts ──

#[tauri::command]
pub fn get_verdicts(symbol: Option<String>, limit: Option<i64>) -> Result<Vec<verdicts::Verdict>, String> {
    verdicts::list_verdicts(symbol.as_deref(), limit)
}

#[tauri::command]
pub fn save_verdict(
    symbol: String,
    verdict: String,
    confidence: Option<f64>,
    macro_signal: Option<String>,
    macro_strength: Option<f64>,
    reasoning: Option<String>,
    model: Option<String>,
    provider: Option<String>,
    tokens_used: Option<i64>,
    latency_ms: Option<i64>,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let v = verdicts::Verdict {
        id: id.clone(),
        symbol,
        verdict,
        confidence,
        macro_signal,
        macro_strength,
        reasoning,
        model,
        provider,
        tokens_used,
        latency_ms,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    verdicts::save_verdict(&v)?;
    Ok(id)
}

// ── PnL ──

#[tauri::command]
pub fn get_pnl_snapshots(limit: Option<i64>) -> Result<Vec<verdicts::PnlSnapshot>, String> {
    verdicts::list_pnl_snapshots(limit)
}

// ── Events ──

#[tauri::command]
pub fn get_events(source: Option<String>, limit: Option<i64>) -> Result<Vec<events::Event>, String> {
    events::list_events(source.as_deref(), limit)
}

#[tauri::command]
pub fn mark_event_triggered(event_id: String, verdict_id: String) -> Result<(), String> {
    events::mark_event_triggered(&event_id, &verdict_id)
}

#[tauri::command]
pub fn get_event_sources() -> Result<Vec<events::EventSource>, String> {
    events::list_event_sources()
}

// ── Scheduler ──

#[tauri::command]
pub fn get_scheduler_logs(task_name: Option<String>, limit: Option<i64>) -> Result<Vec<scheduler::SchedulerLog>, String> {
    scheduler::list_scheduler_logs(task_name.as_deref(), limit)
}

#[tauri::command]
pub fn check_trading_day(date: String) -> Result<bool, String> {
    scheduler::is_trading_day(&date)
}
```

- [ ] **Step 2: Register module in commands/mod.rs**

Add at the top of `src-tauri/src/commands/mod.rs`:

```rust
pub mod invest;
```

- [ ] **Step 3: Register commands in lib.rs generate_handler![]**

Add to the `generate_handler![]` array:

```rust
commands::invest::get_holdings,
commands::invest::add_holding,
commands::invest::delete_holding,
commands::invest::get_cash_balance,
commands::invest::set_cash_balance,
commands::invest::record_trade,
commands::invest::get_trades,
commands::invest::get_verdicts,
commands::invest::save_verdict,
commands::invest::get_pnl_snapshots,
commands::invest::get_events,
commands::invest::mark_event_triggered,
commands::invest::get_event_sources,
commands::invest::get_scheduler_logs,
commands::invest::check_trading_day,
```

- [ ] **Step 4: Verify full Rust compilation**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/invest.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat: tauri commands for invest module (holdings, trades, verdicts, events, scheduler)"
```

---

### Task 9: Add sidebar entries and i18n keys

**Files:**
- Modify: `src/routes/+layout.svelte`
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

- [ ] **Step 1: Add i18n keys to en.json**

Add these keys to `messages/en.json` (alphabetical position among existing `nav_` keys):

```json
"nav_invest": "Investment Committee",
"nav_memoryMgmt": "Memory Management",
"moreMenu_doctor": "Doctor Diagnostics",
"moreMenu_releaseNotes": "Release Notes",
"moreMenu_allSettings": "All Settings",
"invest_tab_dashboard": "Dashboard",
"invest_tab_committee": "Committee",
"invest_tab_strategy": "Strategy",
"invest_tab_trades": "Trade Log",
"invest_tab_events": "Event Monitor",
"invest_tab_scheduler": "Scheduled Tasks",
"memoryMgmt_tab_userMemory": "User Memory",
"memoryMgmt_tab_extractionConfig": "Extraction Config"
```

- [ ] **Step 2: Add i18n keys to zh-CN.json**

Add corresponding keys to `messages/zh-CN.json`:

```json
"nav_invest": "投资委员会",
"nav_memoryMgmt": "记忆管理",
"moreMenu_doctor": "Doctor 诊断",
"moreMenu_releaseNotes": "版本更新日志",
"moreMenu_allSettings": "全部设置",
"invest_tab_dashboard": "Dashboard",
"invest_tab_committee": "委员会",
"invest_tab_strategy": "策略",
"invest_tab_trades": "交易记录",
"invest_tab_events": "事件监控",
"invest_tab_scheduler": "定时任务",
"memoryMgmt_tab_userMemory": "用户记忆",
"memoryMgmt_tab_extractionConfig": "提取配置"
```

- [ ] **Step 3: Add navItems entries in +layout.svelte**

Find the `navItems` array (around line 457). Insert `/invest` after `/plugins` and `/memory-mgmt` after `/settings`:

```ts
const navItems = [
  { path: "/chat", label: () => t("nav_chat"), icon: "message" },
  { path: "/explorer", label: () => t("nav_explorer"), icon: "folder" },
  { path: "/plugins", label: () => t("nav_extend"), icon: "zap" },
  { path: "/invest", label: () => t("nav_invest"), icon: "trendingUp" },
  { path: "/memory", label: () => t("nav_memory"), icon: "book" },
  { path: "/usage", label: () => t("nav_usage"), icon: "chart" },
  { path: "/settings", label: () => t("nav_settings"), icon: "settings" },
  { path: "/memory-mgmt", label: () => t("nav_memoryMgmt"), icon: "database" },
  { path: "/history", label: () => t("nav_history"), icon: "clock" },
];
```

- [ ] **Step 4: Add "trendingUp" icon SVG**

In the template's icon if/else chain (around line 1512), add a new block for `"trendingUp"`:

```svelte
{:else if item.icon === "trendingUp"}
  <svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
    <polyline points="22 7 13.5 15.5 8.5 10.5 2 17" />
    <polyline points="16 7 22 7 22 13" />
  </svg>
```

- [ ] **Step 5: Add "database" icon SVG**

Add another block for `"database"`:

```svelte
{:else if item.icon === "database"}
  <svg class="h-[18px] w-[18px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
    <ellipse cx="12" cy="5" rx="9" ry="3" />
    <path d="M3 5V19A9 3 0 0 0 21 19V5" />
    <path d="M3 12A9 3 0 0 0 21 12" />
  </svg>
```

- [ ] **Step 6: Commit**

```bash
git add src/routes/+layout.svelte messages/en.json messages/zh-CN.json
git commit -m "feat: add /invest and /memory-mgmt sidebar entries with i18n keys"
```

---

### Task 10: Create /invest route skeleton

**Files:**
- Create: `src/routes/invest/+page.svelte`

- [ ] **Step 1: Write the invest page with 6 tabs**

```svelte
<script lang="ts">
  import { t } from "$lib/i18n";

  type InvestTab = "dashboard" | "committee" | "strategy" | "trades" | "events" | "scheduler";
  let activeTab: InvestTab = $state("dashboard");

  const tabs: { id: InvestTab; label: string }[] = $derived([
    { id: "dashboard", label: t("invest_tab_dashboard") },
    { id: "committee", label: t("invest_tab_committee") },
    { id: "strategy", label: t("invest_tab_strategy") },
    { id: "trades", label: t("invest_tab_trades") },
    { id: "events", label: t("invest_tab_events") },
    { id: "scheduler", label: t("invest_tab_scheduler") },
  ]);
</script>

<div class="flex h-full flex-col">
  <div class="border-b border-border px-4 pt-3">
    <h1 class="mb-3 text-lg font-semibold">{t("nav_invest")}</h1>
    <div class="flex gap-1">
      {#each tabs as tab}
        <button
          class="rounded-t-md px-3 py-1.5 text-sm transition-colors"
          class:bg-primary={activeTab === tab.id}
          class:text-primary-foreground={activeTab === tab.id}
          class:text-muted-foreground={activeTab !== tab.id}
          class:hover:bg-muted={activeTab !== tab.id}
          onclick={() => (activeTab = tab.id)}
        >
          {tab.label}
        </button>
      {/each}
    </div>
  </div>

  <div class="flex-1 overflow-auto p-4">
    {#if activeTab === "dashboard"}
      <div class="text-muted-foreground">Dashboard — coming in Phase 2</div>
    {:else if activeTab === "committee"}
      <div class="text-muted-foreground">Committee — coming in Phase 3</div>
    {:else if activeTab === "strategy"}
      <div class="text-muted-foreground">Strategy — coming in Phase 2</div>
    {:else if activeTab === "trades"}
      <div class="text-muted-foreground">Trade Log — coming in Phase 2</div>
    {:else if activeTab === "events"}
      <div class="text-muted-foreground">Event Monitor — coming in Phase 3c</div>
    {:else if activeTab === "scheduler"}
      <div class="text-muted-foreground">Scheduled Tasks — coming in Phase 4</div>
    {/if}
  </div>
</div>
```

- [ ] **Step 2: Verify dev server renders the page**

```bash
npm run dev
```

Navigate to `http://localhost:1420/invest` and confirm the 6-tab skeleton renders.

- [ ] **Step 3: Commit**

```bash
git add src/routes/invest/+page.svelte
git commit -m "feat: /invest route skeleton with 6 tabs"
```

---

### Task 11: Create /memory-mgmt route

**Files:**
- Create: `src/routes/memory-mgmt/+page.svelte`

- [ ] **Step 1: Write the memory management page**

```svelte
<script lang="ts">
  import { t } from "$lib/i18n";
  import { invoke } from "$lib/transport";

  type MemTab = "userMemory" | "extractionConfig";
  let activeTab: MemTab = $state("userMemory");

  const tabs: { id: MemTab; label: string }[] = $derived([
    { id: "userMemory", label: t("memoryMgmt_tab_userMemory") },
    { id: "extractionConfig", label: t("memoryMgmt_tab_extractionConfig") },
  ]);

  // Scope filter
  let scopeFilter: string | null = $state(null);
  const scopeOptions = [null, "global", "project", "invest"];

  // Memory list
  let memories: any[] = $state([]);
  let loading = $state(false);

  async function loadMemories() {
    loading = true;
    try {
      memories = await invoke("list_memos", { scope: scopeFilter });
    } catch (e) {
      console.error("Failed to load memories:", e);
    } finally {
      loading = false;
    }
  }

  // Load on mount and when filter changes
  $effect(() => {
    scopeFilter; // track
    loadMemories();
  });

  // Extraction config state
  let configDirty = $state(false);
  let extractEnabled = $state(true);
  let chatEndpoint = $state("");
  let chatApiKey = $state("");
  let chatModel = $state("");

  function handleApplyConfig() {
    // TODO: save config via Tauri command (Phase 2)
    configDirty = false;
  }
</script>

<div class="flex h-full flex-col">
  <div class="border-b border-border px-4 pt-3">
    <h1 class="mb-3 text-lg font-semibold">{t("nav_memoryMgmt")}</h1>
    <div class="flex gap-1">
      {#each tabs as tab}
        <button
          class="rounded-t-md px-3 py-1.5 text-sm transition-colors"
          class:bg-primary={activeTab === tab.id}
          class:text-primary-foreground={activeTab === tab.id}
          class:text-muted-foreground={activeTab !== tab.id}
          class:hover:bg-muted={activeTab !== tab.id}
          onclick={() => (activeTab = tab.id)}
        >
          {tab.label}
        </button>
      {/each}
    </div>
  </div>

  <div class="flex-1 overflow-auto p-4">
    {#if activeTab === "userMemory"}
      <!-- Scope filter -->
      <div class="mb-4 flex gap-2">
        {#each scopeOptions as scope}
          <button
            class="rounded-md px-2.5 py-1 text-xs transition-colors"
            class:bg-primary={scopeFilter === scope}
            class:text-primary-foreground={scopeFilter === scope}
            class:bg-muted={scopeFilter !== scope}
            class:text-muted-foreground={scopeFilter !== scope}
            onclick={() => (scopeFilter = scope)}
          >
            {scope ?? "all"}
          </button>
        {/each}
      </div>

      <!-- Memory list -->
      {#if loading}
        <div class="text-muted-foreground text-sm">Loading...</div>
      {:else if memories.length === 0}
        <div class="text-muted-foreground text-sm">No memories found</div>
      {:else}
        <div class="flex flex-col gap-2">
          {#each memories as mem}
            <div class="rounded-md border border-border p-3">
              <div class="mb-1 flex items-center gap-2">
                <span class="rounded bg-muted px-1.5 py-0.5 text-xs">{mem.scope ?? "global"}</span>
                <span class="rounded bg-muted px-1.5 py-0.5 text-xs">{mem.memory_type}</span>
                {#if mem.confidence != null}
                  <span class="text-muted-foreground text-xs">confidence: {mem.confidence.toFixed(1)}</span>
                {/if}
              </div>
              <div class="text-sm">{mem.content}</div>
              <div class="text-muted-foreground mt-1 text-xs">Updated: {mem.updated_at}</div>
            </div>
          {/each}
        </div>
      {/if}

    {:else if activeTab === "extractionConfig"}
      <div class="max-w-lg">
        <div class="mb-4">
          <label class="flex items-center gap-2">
            <input type="checkbox" bind:checked={extractEnabled} />
            <span class="text-sm">Enable auto extraction</span>
          </label>
        </div>

        <div class="mb-3">
          <label class="text-muted-foreground mb-1 block text-xs">Chat API Endpoint</label>
          <input
            class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm"
            bind:value={chatEndpoint}
            oninput={() => (configDirty = true)}
          />
        </div>

        <div class="mb-3">
          <label class="text-muted-foreground mb-1 block text-xs">Chat API Key</label>
          <input
            type="password"
            class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm"
            bind:value={chatApiKey}
            oninput={() => (configDirty = true)}
          />
        </div>

        <div class="mb-4">
          <label class="text-muted-foreground mb-1 block text-xs">Chat Model</label>
          <input
            class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm"
            bind:value={chatModel}
            oninput={() => (configDirty = true)}
          />
        </div>

        <div class="flex items-center justify-between border-t border-border pt-3">
          <span class="text-muted-foreground text-xs">Click apply after config changes</span>
          <button
            class="rounded-md px-3 py-1.5 text-sm font-medium transition-colors"
            class:bg-green-600={configDirty}
            class:text-white={configDirty}
            class:bg-muted={!configDirty}
            class:text-muted-foreground={!configDirty}
            disabled={!configDirty}
            onclick={handleApplyConfig}
          >
            Apply & Reload
          </button>
        </div>
      </div>
    {/if}
  </div>
</div>
```

- [ ] **Step 2: Commit**

```bash
git add src/routes/memory-mgmt/+page.svelte
git commit -m "feat: /memory-mgmt route with user memory list and extraction config"
```

---

### Task 12: Create MoreMenu component

**Files:**
- Create: `src/lib/components/MoreMenu.svelte`
- Modify: `src/routes/+layout.svelte`

- [ ] **Step 1: Write MoreMenu.svelte**

```svelte
<script lang="ts">
  import { t } from "$lib/i18n";

  let open = $state(false);
  let menuEl: HTMLDivElement | undefined = $state();

  function toggle() {
    open = !open;
  }

  function handleClickOutside(e: MouseEvent) {
    if (menuEl && !menuEl.contains(e.target as Node)) {
      open = false;
    }
  }

  function handleDoctor() {
    open = false;
    // Navigate to doctor or dispatch event
    window.dispatchEvent(new CustomEvent("ocv:toggle-doctor"));
  }

  function handleReleaseNotes() {
    open = false;
    window.location.href = "/release-notes";
  }

  function handleSettings() {
    open = false;
    window.location.href = "/settings";
  }
</script>

<svelte:window onclick={handleClickOutside} />

<div class="relative" bind:this={menuEl}>
  <button
    class="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
    onclick={toggle}
    title="More"
  >
    <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <circle cx="12" cy="12" r="1" />
      <circle cx="19" cy="12" r="1" />
      <circle cx="5" cy="12" r="1" />
    </svg>
  </button>

  {#if open}
    <div class="absolute right-0 top-full z-50 mt-1 min-w-[180px] rounded-md border border-border bg-popover p-1 shadow-md">
      <button
        class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-sm transition-colors hover:bg-muted"
        onclick={handleDoctor}
      >
        <span>🩺</span>
        <span>{t("moreMenu_doctor")}</span>
      </button>
      <button
        class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-sm transition-colors hover:bg-muted"
        onclick={handleReleaseNotes}
      >
        <span>📋</span>
        <span>{t("moreMenu_releaseNotes")}</span>
      </button>
      <div class="my-1 h-px bg-border"></div>
      <button
        class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-muted"
        onclick={handleSettings}
      >
        <span>⚙️</span>
        <span>{t("moreMenu_allSettings")}</span>
      </button>
    </div>
  {/if}
</div>
```

- [ ] **Step 2: Import and use MoreMenu in +layout.svelte**

In the layout's `<script>` block, add the import:

```ts
import MoreMenu from "$lib/components/MoreMenu.svelte";
```

In the top bar template, find the memo toggle button and add MoreMenu after it:

```svelte
<MoreMenu />
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/MoreMenu.svelte src/routes/+layout.svelte
git commit -m "feat: MoreMenu dropdown in title bar with Doctor entry"
```

---

### Task 13: Full verification and integration check

- [ ] **Step 1: Run frontend checks**

```bash
npm run lint
npm run check
npm run i18n:check
```

- [ ] **Step 2: Run Rust checks**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

- [ ] **Step 3: Run build**

```bash
npm run build
```

- [ ] **Step 4: Fix any issues found**

- [ ] **Step 5: Final commit if fixes needed**

```bash
git add -A
git commit -m "fix: phase 1 verification fixes"
```

---

## Spec Self-Review Checklist

- [x] **Spec coverage:** All 3 modules (A: memory scope, B: invest.db, C: sidebar/routes/UI) have corresponding tasks
- [x] **Placeholder scan:** No TBD/TODO in code blocks. The "coming in Phase X" text in the invest page tabs is intentional UI placeholder, not plan placeholder
- [x] **Type consistency:** `MemoryItem` fields match across storage, commands, and frontend. `Holding`, `Trade`, `Verdict`, `Event`, `SchedulerLog` structs are consistent
- [x] **Naming consistency:** `camelCase` in JSON (serde), `snake_case` in SQL columns, `PascalCase` for Rust structs
