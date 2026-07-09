pub mod committees;
pub mod domain_insights;
pub mod dream_snapshots;
pub mod events;
pub mod macro_cache;
pub mod macro_verdict;
pub mod premarket_cache;
pub mod portfolio;
pub mod scheduler;
pub mod sentiment;
pub mod stock_industry;
pub mod strategy;
pub mod stock_data_cache;
pub mod user_profile;
pub mod verdict_reviews;
pub mod verdict_tracking;
pub mod verdicts;

use rusqlite::Connection;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

/// SQLite sidecar file extensions (WAL journal + shared memory).
const DB_SIDECAR_EXTS: &[&str] = &["db", "db-wal", "db-shm"];

/// Canonical column list for the trades table.
const TRADES_COLUMNS: &[&str] = &[
    "id", "symbol", "currency", "kind", "action", "shares", "price", "amount",
    "notes", "created_at", "name", "trade_date", "asset_type",
    "commission", "stamp_duty",
];

/// 根据 ts_code/symbol 前缀判断是否为 ETF/基金。
/// 前缀列表与 `TushareClient::daily_api` 保持同步。
pub fn is_etf_symbol(symbol: &str) -> bool {
    let prefix = symbol.split('.').next().unwrap_or("");
    matches!(
        prefix.get(..3).unwrap_or(""),
        "159" | "510" | "512" | "515" | "588" | "150" | "500" | "501"
            | "160" | "161" | "162" | "163" | "164"
    )
}

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

/// Open and migrate the DB; if it fails, backup the file (incl. WAL/SHM)
/// and retry once with a fresh database.
fn init_with_fallback(db_path: &Path) -> Result<Connection, String> {
    match init_db_inner(db_path) {
        Ok(conn) => Ok(conn),
        Err(e) => {
            log::warn!("invest DB init failed ({}), backing up and retrying with fresh DB", e);
            backup_db_files(db_path);
            delete_db_files(db_path);
            init_db_inner(db_path)
                .map_err(|e2| format!("invest DB retry also failed: {}", e2))
        }
    }
}

/// Backup the SQLite database file and its WAL/SHM sidecars.
/// Creates a timestamped backup copy before destructive operations.
fn backup_db_files(db_path: &Path) {
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    for ext in DB_SIDECAR_EXTS {
        let p = db_path.with_extension(ext);
        if p.exists() {
            let backup_name = format!("{}.backup_{}", p.display(), timestamp);
            match std::fs::copy(&p, &backup_name) {
                Ok(_) => log::info!("Backed up {} to {}", p.display(), backup_name),
                Err(e) => log::warn!("Failed to backup {}: {}", p.display(), e),
            }
        }
    }
}

/// Remove the SQLite database file and its WAL/SHM sidecars.
fn delete_db_files(db_path: &Path) {
    for ext in DB_SIDECAR_EXTS {
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

/// Validate that a name is a safe SQL identifier (alphanumeric + underscore).
fn assert_safe_identifier(name: &str) {
    debug_assert!(
        !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_'),
        "unsafe SQL identifier: {:?}",
        name
    );
}

/// Check whether a table column exists.
fn has_column(conn: &Connection, table: &str, col: &str) -> bool {
    assert_safe_identifier(table);
    assert_safe_identifier(col);
    conn.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name='{}'", table, col),
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

/// Get list of column names for a table.
fn get_table_columns(conn: &Connection, table: &str) -> Result<Vec<String>, String> {
    assert_safe_identifier(table);
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info('{}')", table))
        .map_err(|e| format!("prepare table_info for {}: {}", table, e))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("query table_info for {}: {}", table, e))?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Migrate trades table with tolerant column handling.
/// Only copies columns that exist in the old table; missing columns get NULL defaults.
/// This prevents data loss when schema doesn't match exactly.
fn migrate_trades_table(conn: &mut Connection) -> Result<(), String> {
    // Check if trades table exists
    let trades_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='trades'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;

    if !trades_exists {
        // No existing trades table, just create the new one via CREATE_TABLES_SQL
        return Ok(());
    }

    // Get existing columns from old trades table (fail fast on introspection error)
    let old_columns: HashSet<String> = get_table_columns(conn, "trades")?.into_iter().collect();

    // Check if the CHECK constraint includes 'transfer_out' (latest action).
    // If columns match AND CHECK is current, skip redundant table rebuild.
    let current_check: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='trades'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();
    let check_is_current = current_check.contains("transfer_out");

    let expected: HashSet<String> = TRADES_COLUMNS.iter().map(|s| s.to_string()).collect();
    if old_columns == expected && check_is_current {
        log::debug!("Trades table schema already up-to-date, skipping migration");
        return Ok(());
    }

    // Build SELECT clause: use existing column if available, NULL if missing
    let column_list = TRADES_COLUMNS.join(", ");
    let select_clause = TRADES_COLUMNS
        .iter()
        .map(|col| {
            if old_columns.contains(*col) {
                col.to_string()
            } else {
                format!("NULL as {}", col)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    // Use RAII transaction for automatic rollback on any error
    let tx = conn.transaction().map_err(|e| format!("begin trades migration tx: {}", e))?;

    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS trades_new (
            id TEXT PRIMARY KEY,
            symbol TEXT NOT NULL,
            currency TEXT NOT NULL DEFAULT 'CNY',
            kind TEXT NOT NULL CHECK (kind IN ('hold', 'watch', 'cash')),
            action TEXT NOT NULL CHECK (action IN ('buy', 'sell', 'cost_edit', 'cash_adjust', 'transfer_in', 'transfer_out', 'add_watch', 'delete_watch', 'edit_holding', 'unknown')),
            shares REAL,
            price REAL,
            amount REAL,
            notes TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            name TEXT,
            trade_date TEXT,
            asset_type TEXT,
            commission REAL,
            stamp_duty REAL
        );"
    ).map_err(|e| format!("create trades_new: {}", e))?;

    // Convert deprecated actions before copying (avoids CHECK constraint violation).
    // 'convert_hold_to_watch' maps to 'unknown' (not 'cost_edit') because the old action
    // had different semantics (move hold→watch) that cost_edit does not replicate.
    // 'unknown' trades are skipped during replay, which is safer than silently modifying cost.
    tx.execute_batch(
        "UPDATE trades SET action = 'buy' WHERE action = 'convert_watch_to_hold';
         UPDATE trades SET action = 'unknown' WHERE action = 'convert_hold_to_watch';"
    ).map_err(|e| format!("convert deprecated actions: {}", e))?;

    // Copy data with tolerant column handling
    let insert_sql = format!(
        "INSERT OR IGNORE INTO trades_new ({}) SELECT {} FROM trades;",
        column_list, select_clause
    );
    tx.execute_batch(&insert_sql)
        .map_err(|e| format!("copy trades data: {}", e))?;

    // Replace old table with new one
    tx.execute_batch(
        "DROP TABLE IF EXISTS trades;
         ALTER TABLE trades_new RENAME TO trades;"
    ).map_err(|e| format!("finalize trades migration: {}", e))?;

    tx.commit().map_err(|e| format!("commit trades migration: {}", e))?;

    log::info!("Trades table migrated successfully with {} old columns", old_columns.len());
    Ok(())
}

/// Core initialization: open DB, run migrations, return the Connection.
fn init_db_inner(db_path: &Path) -> Result<Connection, String> {
    let mut conn = Connection::open(db_path)
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

    // Migration: add analyzed, analyzed_at, channels columns to events table if missing
    if !has_column(&conn, "events", "analyzed") {
        conn.execute_batch("ALTER TABLE events ADD COLUMN analyzed INTEGER DEFAULT 0;")
            .map_err(|e| format!("Failed to add analyzed column: {}", e))?;
    }
    if !has_column(&conn, "events", "analyzed_at") {
        conn.execute_batch("ALTER TABLE events ADD COLUMN analyzed_at TEXT;")
            .map_err(|e| format!("Failed to add analyzed_at column: {}", e))?;
    }
    if !has_column(&conn, "events", "channels") {
        conn.execute_batch("ALTER TABLE events ADD COLUMN channels TEXT DEFAULT '[]';")
            .map_err(|e| format!("Failed to add channels column: {}", e))?;
    }

    // Migration: add asset_type column to holdings table if missing
    if !has_column(&conn, "holdings", "asset_type") {
        conn.execute_batch("ALTER TABLE holdings ADD COLUMN asset_type TEXT NOT NULL DEFAULT 'stock';")
            .map_err(|e| format!("Failed to add asset_type column: {}", e))?;
    }

    // Migration: add cleared_date column to holdings table if missing
    if !has_column(&conn, "holdings", "cleared_date") {
        conn.execute_batch("ALTER TABLE holdings ADD COLUMN cleared_date TEXT;")
            .map_err(|e| format!("Failed to add cleared_date column: {}", e))?;
    }

    // Add UNIQUE index on (source, title) for event dedup.
    // If the index doesn't exist yet, deduplicate first to avoid UNIQUE constraint failure.
    let has_dedup_index: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_events_source_title'",
            [],
            |row| row.get::<_, i32>(0),
        )
        .unwrap_or(0)
        > 0;
    if !has_dedup_index {
        conn.execute_batch(
            "DELETE FROM events WHERE rowid NOT IN (
                SELECT MIN(rowid) FROM events GROUP BY source, title
            );"
        )
        .map_err(|e| format!("Failed to deduplicate events: {}", e))?;
    }
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_events_source_title ON events(source, title);"
    )
    .map_err(|e| format!("Failed to add events dedup index: {}", e))?;

    // Migrate trades table: rebuild to include name/trade_date/asset_type columns and
    // the complete action CHECK constraint (including cash_adjust/add_watch/delete_watch).
    // SQLite doesn't support ALTER CHECK, so we rebuild via trades_new.
    // Uses tolerant migration: only copies columns that exist in the old table,
    // missing columns get NULL defaults. This prevents data loss on schema mismatch.
    migrate_trades_table(&mut conn)?;

    // Migration: backfill NULL/empty trade_date from created_at's local date.
    // Historically sells and system actions stored trade_date=NULL, which made the UI
    // fall back to a locale-formatted (slash) date. Normalize all rows to YYYY-MM-DD.
    // Idempotent: only touches rows still missing a trade_date.
    conn.execute_batch(
        "UPDATE trades \
         SET trade_date = date(created_at, 'localtime') \
         WHERE (trade_date IS NULL OR trade_date = '') \
           AND created_at IS NOT NULL AND created_at <> '';"
    )
    .map_err(|e| format!("Failed to backfill trades.trade_date: {}", e))?;

    // Migration: create verdict_reviews table (use local conn, DB not yet in static)
    verdict_reviews::create_table(&conn)?;

    // Migration: create verdict_tracking table (auto-tracking for hit rate)
    verdict_tracking::create_table(&conn)?;

    // Migration: create dream_snapshots table (use local conn, DB not yet in static)
    dream_snapshots::create_table(&conn)?;

    // Migration: create macro_cache table (use local conn, DB not yet in static)
    macro_cache::create_table(&conn)?;

    // Migration: create premarket_factor_cache table (盘后 SABC 全市场缓存)
    premarket_cache::create_table(&conn)?;

    // Migration: create macro_verdict table (全局宏观判断单行存储)
    macro_verdict::create_table(&conn)?;

    // Migration: create stock_data_cache table (permanent per-symbol data cache)
    stock_data_cache::create_table(&conn)?;

    // Migration: create sentiment_items table (舆情采集)
    conn.execute_batch(sentiment::CREATE_SENTIMENT_TABLE)
        .map_err(|e| format!("create sentiment_items: {}", e))?;

    // Migration: add summary/sectors/topics columns to events table (Task 5).
    // Aligns events schema with sentiment_items for the shared analyze_pending path.
    sentiment::ensure_column(&conn, "events", "summary", "TEXT")?;
    sentiment::ensure_column(&conn, "events", "sectors", "TEXT")?;
    sentiment::ensure_column(&conn, "events", "topics", "TEXT")?;

    // Migration: create stock_industry table (个股 → 行业映射，每周从 tushare stock_basic 刷新)
    conn.execute_batch(stock_industry::CREATE_STOCK_INDUSTRY_TABLE)
        .map_err(|e| format!("create stock_industry: {}", e))?;

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

    // NOTE: trades.name/trade_date/asset_type are already handled by migrate_trades_table
    // (called above) which rebuilds the table with all 13 columns. No fallback needed.

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
    kind TEXT NOT NULL CHECK (kind IN ('hold', 'watch', 'cash')),
    name TEXT,
    notional REAL NOT NULL DEFAULT 0,
    avg_cost REAL,
    shares REAL,
    entry_date TEXT,
    linked_verdict_id TEXT,
    notes TEXT,
    asset_type TEXT NOT NULL DEFAULT 'stock',
    cleared_date TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (symbol, currency, kind)
);

CREATE TABLE IF NOT EXISTS trades (
    id TEXT PRIMARY KEY,
    symbol TEXT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'CNY',
    kind TEXT NOT NULL CHECK (kind IN ('hold', 'watch', 'cash')),
    action TEXT NOT NULL CHECK (action IN ('buy', 'sell', 'cost_edit', 'cash_adjust', 'transfer_in', 'transfer_out', 'add_watch', 'delete_watch', 'edit_holding', 'unknown')),
    shares REAL,
    price REAL,
    amount REAL,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    name TEXT,
    trade_date TEXT,
    asset_type TEXT,
    commission REAL,
    stamp_duty REAL
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
    stance TEXT DEFAULT 'neutral',
    triggered INTEGER DEFAULT 0,
    trigger_verdict_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    analyzed INTEGER DEFAULT 0,
    analyzed_at TEXT,
    channels TEXT DEFAULT '[]'
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

#[cfg(test)]
mod migration_tests {
    use rusqlite::Connection;

    #[test]
    fn test_ensure_column_adds_missing() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE t (id TEXT)", []).unwrap();
        // 第一次：列不存在，应添加
        crate::storage::invest::sentiment::ensure_column(&conn, "t", "extra", "TEXT").unwrap();
        // 第二次：列已存在，应幂等不报错
        crate::storage::invest::sentiment::ensure_column(&conn, "t", "extra", "TEXT").unwrap();
        let cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('t') WHERE name='extra'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 1);
    }
}
