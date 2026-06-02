pub mod committees;
pub mod domain_insights;
pub mod dream_snapshots;
pub mod events;
pub mod macro_cache;
pub mod portfolio;
#[allow(unused)]
pub mod round_cache;
pub mod scheduler;
pub mod strategy;
pub mod stock_data_cache;
pub mod user_profile;
pub mod verdict_reviews;
pub mod verdict_tracking;
pub mod verdicts;

use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

static DB: Mutex<Option<Connection>> = Mutex::new(None);

/// Resolve the invest.db path, creating the parent directory if needed.
fn invest_db_path() -> Result<std::path::PathBuf, String> {
    let data_dir = crate::storage::data_dir();
    let invest_dir = data_dir.join("invest");
    crate::storage::ensure_dir(&invest_dir).map_err(|e| format!("create invest dir: {}", e))?;
    Ok(invest_dir.join("invest.db"))
}

/// Initialize the invest database and store the connection in the static `DB`.
/// Called once at app startup from `lib.rs`.
pub fn init_db(_data_dir: &Path) -> Result<(), String> {
    let db_path = invest_db_path()?;
    let conn = init_with_fallback(&db_path)?;
    let mut guard = DB.lock().map_err(|e| format!("lock db: {}", e))?;
    *guard = Some(conn);
    Ok(())
}

/// Open and migrate the DB; if it fails, delete the file (incl. WAL/SHM)
/// and retry once with a fresh database.
fn init_with_fallback(db_path: &Path) -> Result<Connection, String> {
    match init_db_inner(db_path) {
        Ok(conn) => Ok(conn),
        Err(e) => {
            log::warn!("invest DB init failed ({}), deleting and retrying with fresh DB", e);
            delete_db_files(db_path);
            init_db_inner(db_path)
                .map_err(|e2| format!("invest DB retry also failed: {}", e2))
        }
    }
}

/// Remove the SQLite database file and its WAL/SHM sidecars.
fn delete_db_files(db_path: &Path) {
    for ext in ["db", "db-wal", "db-shm"] {
        let p = db_path.with_extension(ext);
        if p.exists() {
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// Lazy-init helper: ensure the static `DB` guard holds a live connection.
/// Called by `with_conn` and `with_conn_mut` when the guard is `None`.
fn ensure_conn(guard: &mut Option<Connection>) -> Result<(), String> {
    log::warn!("invest DB was not initialized, attempting lazy init");
    let db_path = invest_db_path()?;
    let conn = init_with_fallback(&db_path)?;
    *guard = Some(conn);
    Ok(())
}

/// Check whether a table column exists.
fn has_column(conn: &Connection, table: &str, col: &str) -> bool {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name='{}'", table, col),
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

/// Core initialization: open DB, run migrations, return the Connection.
fn init_db_inner(db_path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(db_path)
        .map_err(|e| format!("open invest.db: {}", e))?;

    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
        .map_err(|e| format!("set pragmas: {}", e))?;

    conn.execute_batch(CREATE_TABLES_SQL)
        .map_err(|e| format!("create tables: {}", e))?;

    // Migration: add stance column to events table if missing
    if !has_column(&conn, "events", "stance") {
        conn.execute_batch("ALTER TABLE events ADD COLUMN stance TEXT DEFAULT 'neutral';")
            .map_err(|e| format!("Failed to add stance column: {}", e))?;
    }

    // Migration: add asset_type column to holdings table if missing
    if !has_column(&conn, "holdings", "asset_type") {
        conn.execute_batch("ALTER TABLE holdings ADD COLUMN asset_type TEXT NOT NULL DEFAULT 'stock';")
            .map_err(|e| format!("Failed to add asset_type column: {}", e))?;
    }

    // Add UNIQUE index on (source, title) for event dedup
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_events_source_title ON events(source, title);"
    )
    .map_err(|e| format!("Failed to add events dedup index: {}", e))?;

    // Migrate trades table: rebuild to include name/trade_date columns and
    // the complete action CHECK constraint (including cash_adjust/add_watch/delete_watch).
    // SQLite doesn't support ALTER CHECK, so we rebuild via trades_new.
    // On an old DB that already has name/trade_date, this is a no-op.
    // On a DB with a mismatched schema, init_with_fallback will wipe and retry.
    conn.execute_batch(
        "BEGIN;
        CREATE TABLE IF NOT EXISTS trades_new (
            id TEXT PRIMARY KEY,
            symbol TEXT NOT NULL,
            currency TEXT NOT NULL DEFAULT 'CNY',
            kind TEXT NOT NULL CHECK (kind IN ('hold', 'watch')),
            action TEXT NOT NULL CHECK (action IN ('buy', 'sell', 'convert_watch_to_hold', 'convert_hold_to_watch', 'cost_edit', 'cash_adjust', 'add_watch', 'delete_watch')),
            shares REAL,
            price REAL,
            amount REAL,
            notes TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            name TEXT,
            trade_date TEXT
        );
        INSERT OR IGNORE INTO trades_new (id, symbol, currency, kind, action, shares, price, amount, notes, created_at, name, trade_date) SELECT id, symbol, currency, kind, action, shares, price, amount, notes, created_at, name, trade_date FROM trades;
        DROP TABLE IF EXISTS trades;
        ALTER TABLE trades_new RENAME TO trades;
        COMMIT;"
    ).map_err(|e| format!("migrate trades table: {}", e))?;

    // Migration: create verdict_reviews table (use local conn, DB not yet in static)
    verdict_reviews::create_table(&conn)?;

    // Migration: create verdict_tracking table (auto-tracking for hit rate)
    verdict_tracking::create_table(&conn)?;

    // Migration: create dream_snapshots table (use local conn, DB not yet in static)
    dream_snapshots::create_table(&conn)?;

    // Migration: create macro_cache table (use local conn, DB not yet in static)
    macro_cache::create_table(&conn)?;

    // Migration: create stock_data_cache table (permanent per-symbol data cache)
    stock_data_cache::create_table(&conn)?;

    // FTS5 virtual table for domain_insights full-text search
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS domain_insights_fts USING fts5(content, symbol, tokenize='unicode61');"
    ).map_err(|e| format!("create domain_insights_fts: {e}"))?;

    // Triggers to keep FTS in sync with domain_insights
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS domain_insights_ai;
         DROP TRIGGER IF EXISTS domain_insights_ad;
         DROP TRIGGER IF EXISTS domain_insights_au;
         CREATE TRIGGER domain_insights_ai AFTER INSERT ON domain_insights BEGIN
             INSERT INTO domain_insights_fts(rowid, content, symbol) VALUES (new.rowid, new.content, new.symbol);
         END;
         CREATE TRIGGER domain_insights_ad AFTER DELETE ON domain_insights BEGIN
             INSERT INTO domain_insights_fts(domain_insights_fts, rowid, content, symbol) VALUES ('delete', old.rowid, old.content, old.symbol);
         END;
         CREATE TRIGGER domain_insights_au AFTER UPDATE ON domain_insights BEGIN
             INSERT INTO domain_insights_fts(domain_insights_fts, rowid, content, symbol) VALUES ('delete', old.rowid, old.content, old.symbol);
             INSERT INTO domain_insights_fts(rowid, content, symbol) VALUES (new.rowid, new.content, new.symbol);
         END;"
    ).map_err(|e| format!("create domain_insights_fts triggers: {e}"))?;

    // Migration: add name column to verdicts table if missing
    if !has_column(&conn, "verdicts", "name") {
        conn.execute_batch("ALTER TABLE verdicts ADD COLUMN name TEXT;")
            .map_err(|e| format!("Failed to add name column to verdicts: {}", e))?;
    }

    // Migration: add initial_balance column to cash table if missing
    if !has_column(&conn, "cash", "initial_balance") {
        conn.execute_batch("ALTER TABLE cash ADD COLUMN initial_balance REAL;")
            .map_err(|e| format!("Failed to add initial_balance column: {}", e))?;
    }

    // Migration: add family_support column to user_profile table if missing
    if !has_column(&conn, "user_profile", "family_support") {
        conn.execute_batch("ALTER TABLE user_profile ADD COLUMN family_support TEXT;")
            .map_err(|e| format!("Failed to add family_support column: {}", e))?;
    }

    // Migration: add name and trade_date columns to trades table if missing.
    // NOTE: the trades rebuild migration above already creates trades with these
    // columns. This block handles DBs where trades was created by CREATE_TABLES_SQL
    // (which already includes name/trade_date) but the rebuild was skipped.
    if !has_column(&conn, "trades", "name") {
        conn.execute_batch("ALTER TABLE trades ADD COLUMN name TEXT;")
            .map_err(|e| format!("Failed to add name column to trades: {}", e))?;
        conn.execute_batch(
            "UPDATE trades SET name = (SELECT h.name FROM holdings h WHERE h.symbol = trades.symbol AND h.currency = trades.currency AND h.kind = trades.kind AND h.name IS NOT NULL) WHERE name IS NULL AND action IN ('buy', 'add_watch', 'convert_watch_to_hold');"
        ).map_err(|e| format!("backfill trade names: {}", e))?;
    }
    if !has_column(&conn, "trades", "trade_date") {
        conn.execute_batch("ALTER TABLE trades ADD COLUMN trade_date TEXT;")
            .map_err(|e| format!("Failed to add trade_date column to trades: {}", e))?;
    }

    log::info!("invest.db initialized at {:?}", db_path);
    Ok(conn)
}

/// Clear all invest tables for data re-initialization.
/// Uses sqlite_master to enumerate all user tables (no hardcoded list).
pub fn clear_all_invest_data() -> Result<(), String> {
    with_conn_mut(|conn| {
        let tx = conn.transaction().map_err(|e| format!("begin tx: {}", e))?;

        // Collect all non-internal tables
        let tables: Vec<String> = {
            let mut stmt = tx
                .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite%'")
                .map_err(|e| format!("prepare: {}", e))?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| format!("query: {}", e))?;
            rows.filter_map(|r| r.ok()).collect()
        };

        // Delete all rows from each table
        for table in &tables {
            tx.execute(&format!("DELETE FROM [{}]", table), [])
                .map_err(|e| format!("delete from {}: {}", table, e))?;
        }
        // Reset all autoincrement counters
        tx.execute("DELETE FROM sqlite_sequence", [])
            .map_err(|e| format!("reset sqlite_sequence: {}", e))?;

        tx.commit().map_err(|e| format!("commit: {}", e))?;
        log::info!("All {} invest tables cleared", tables.len());
        Ok(())
    })
}

pub fn with_conn<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&Connection) -> Result<R, String>,
{
    let mut guard = DB.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        ensure_conn(&mut *guard)?;
    }
    let conn = guard.as_ref().unwrap(); // safe: just ensured Some
    f(conn)
}

pub fn with_conn_mut<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut Connection) -> Result<R, String>,
{
    let mut guard = DB.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        ensure_conn(&mut *guard)?;
    }
    let conn = guard.as_mut().unwrap(); // safe: just ensured Some
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
    asset_type TEXT NOT NULL DEFAULT 'stock',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (symbol, currency, kind)
);

CREATE TABLE IF NOT EXISTS trades (
    id TEXT PRIMARY KEY,
    symbol TEXT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'CNY',
    kind TEXT NOT NULL CHECK (kind IN ('hold', 'watch')),
    action TEXT NOT NULL CHECK (action IN ('buy', 'sell', 'convert_watch_to_hold', 'convert_hold_to_watch', 'cost_edit', 'cash_adjust', 'add_watch', 'delete_watch')),
    shares REAL,
    price REAL,
    amount REAL,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    name TEXT,
    trade_date TEXT
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

CREATE TABLE IF NOT EXISTS strategy (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT 'default',
    targets TEXT NOT NULL DEFAULT '[]',
    max_single_pct REAL,
    min_cash_pct REAL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS user_profile (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    emergency_buffer_cny REAL NOT NULL DEFAULT 100000,
    family_backup_available INTEGER NOT NULL DEFAULT 0,
    account_purpose TEXT NOT NULL DEFAULT 'long_term',
    lifestyle_notes TEXT NOT NULL DEFAULT '',
    display_name TEXT,
    risk_tolerance TEXT,
    exchange_buffer_cny REAL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS daily_reports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    report_date TEXT NOT NULL UNIQUE,
    summary TEXT,
    file_path TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
";
