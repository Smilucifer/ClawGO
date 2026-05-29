pub mod domain_insights;
pub mod dream_snapshots;
pub mod events;
pub mod portfolio;
pub mod scheduler;
pub mod strategy;
pub mod verdict_reviews;
pub mod verdicts;

use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

static DB: Mutex<Option<Connection>> = Mutex::new(None);

pub fn init_db(data_dir: &Path) -> Result<(), String> {
    let invest_dir = data_dir.join("invest");
    crate::storage::ensure_dir(&invest_dir).map_err(|e| format!("create invest dir: {}", e))?;
    let db_path = invest_dir.join("invest.db");

    let conn = Connection::open(&db_path)
        .map_err(|e| format!("open invest.db: {}", e))?;

    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
        .map_err(|e| format!("set pragmas: {}", e))?;

    conn.execute_batch(CREATE_TABLES_SQL)
        .map_err(|e| format!("create tables: {}", e))?;

    // Migration: add stance column to events table if missing
    {
        let has_stance: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('events') WHERE name='stance'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if has_stance == 0 {
            conn.execute_batch("ALTER TABLE events ADD COLUMN stance TEXT DEFAULT 'neutral';")
                .map_err(|e| format!("Failed to add stance column: {}", e))?;
        }
    }

    // Add UNIQUE index on (source, title) for event dedup
    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_events_source_title ON events(source, title);"
    )
    .map_err(|e| format!("Failed to add events dedup index: {}", e))?;

    // Migrate trades table to include 'cash_adjust' action.
    // SQLite doesn't support ALTER CHECK, so we rebuild the table.
    conn.execute_batch(
        "BEGIN;
        CREATE TABLE IF NOT EXISTS trades_new (
            id TEXT PRIMARY KEY,
            symbol TEXT NOT NULL,
            currency TEXT NOT NULL DEFAULT 'CNY',
            kind TEXT NOT NULL CHECK (kind IN ('hold', 'watch')),
            action TEXT NOT NULL CHECK (action IN ('buy', 'sell', 'convert_watch_to_hold', 'convert_hold_to_watch', 'cost_edit', 'cash_adjust')),
            shares REAL,
            price REAL,
            amount REAL,
            notes TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        INSERT OR IGNORE INTO trades_new SELECT * FROM trades;
        DROP TABLE IF EXISTS trades;
        ALTER TABLE trades_new RENAME TO trades;
        COMMIT;"
    ).map_err(|e| format!("migrate trades table: {}", e))?;

    // Migration: create verdict_reviews table (use local conn, DB not yet in static)
    verdict_reviews::create_table(&conn)?;

    // Migration: create dream_snapshots table (use local conn, DB not yet in static)
    dream_snapshots::create_table(&conn)?;

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

CREATE TABLE IF NOT EXISTS strategy (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT 'default',
    targets TEXT NOT NULL DEFAULT '[]',
    max_single_pct REAL,
    min_cash_pct REAL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
";
