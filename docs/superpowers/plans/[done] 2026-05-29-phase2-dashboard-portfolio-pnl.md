# Phase 2: Dashboard + Portfolio + Trading + PnL Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the /invest skeleton into a working dashboard with live A-share prices, portfolio CRUD, trade log, strategy config, and PnL chart — all backed by the existing 19 Tauri commands + invest.db.

**Architecture:** Rust Tushare HTTP client fetches market data → invest-store (Svelte 5 runes) orchestrates multi-step Tauri commands for buy/sell → Dashboard renders KPI cards + HoldingsTable + Chart.js PnL chart. Strategy CRUD adds a new `strategy` table. PnL snapshot cron runs in a background tokio task.

**Tech Stack:** Svelte 5 runes, Chart.js, reqwest (already in Cargo.toml), Tushare Pro HTTP API, SQLite (invest.db), Tauri IPC.

---

## File Structure

```
src/lib/types/invest.ts                          — TS interfaces (Holding, Trade, PnlSnapshot, Verdict, Strategy)
src/lib/stores/invest-store.svelte.ts            — Main store ($state runes, wraps 19+ invoke calls)
src/routes/invest/+page.svelte                   — Replace placeholders with real tabs
src/lib/components/invest/KpiCard.svelte         — Single KPI number card
src/lib/components/invest/HoldingsTable.svelte   — HOLD + WATCH grouped table
src/lib/components/invest/TradeDialog.svelte     — Buy / Sell / Cash adjust dialog
src/lib/components/invest/TradeLogTab.svelte     — Trade log tab with filtering
src/lib/components/invest/StrategyTab.svelte     — Strategy CRUD tab
src/lib/components/invest/PnlChart.svelte        — Chart.js line chart
src-tauri/src/tushare/mod.rs                     — Module exports
src-tauri/src/tushare/client.rs                  — Tushare HTTP client (reqwest)
src-tauri/src/storage/invest/strategy.rs         — Strategy table storage
src-tauri/src/commands/invest.rs                 — Add sync_trade_calendar, migrate_legacy, save/get strategy
src-tauri/src/models.rs                          — Add tushare_token to UserSettings
src-tauri/src/lib.rs                             — Spawn PnL cron + trade calendar sync on startup
src-tauri/src/storage/invest/mod.rs              — ALTER TABLE trades CHECK to include cash_adjust
messages/en.json + messages/zh-CN.json           — i18n keys
```

---

## Task 1: Frontend Types

**Files:**
- Create: `src/lib/types/invest.ts`

- [ ] **Step 1: Create invest types file**

```ts
// src/lib/types/invest.ts

export interface Holding {
  symbol: string;
  currency: string;
  kind: 'hold' | 'watch';
  name: string | null;
  notional: number;
  avgCost: number | null;
  shares: number | null;
  entryDate: string | null;
  linkedVerdictId: string | null;
  notes: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface Trade {
  id: string;
  symbol: string;
  currency: string;
  kind: string;
  action: string;
  shares: number | null;
  price: number | null;
  amount: number | null;
  notes: string | null;
  createdAt: string;
}

export interface PnlSnapshot {
  id: number;
  snapshotDate: string;
  totalValue: number;
  cash: number;
  holdingsValue: number;
  dailyPnl: number | null;
  dailyPnlPct: number | null;
}

export interface Verdict {
  id: string;
  symbol: string;
  verdict: string;
  confidence: number | null;
  macroSignal: string | null;
  macroStrength: number | null;
  reasoning: string | null;
  model: string | null;
  provider: string | null;
  tokensUsed: number | null;
  latencyMs: number | null;
  createdAt: string;
}

export interface CashBalance {
  available: number;
}

export interface Strategy {
  id: string;
  name: string;
  targets: StrategyTarget[];
  maxSinglePct: number | null;
  minCashPct: number | null;
  updatedAt: string;
}

export interface StrategyTarget {
  symbol: string;
  name: string;
  targetPct: number;
}

/** Real-time price quote from Tushare. */
export interface PriceQuote {
  tsCode: string;
  name: string;
  close: number;
  change: number;
  pctChg: number;
  vol: number;
  amount: number;
}
```

- [ ] **Step 2: Verify types compile**

Run: `npx svelte-kit sync && npm run check`
Expected: No errors in `src/lib/types/invest.ts`

- [ ] **Step 3: Commit**

```bash
git add src/lib/types/invest.ts
git commit -m "feat(invest): add frontend types for Phase 2"
```

---

## Task 2: Tushare Rust Client

**Files:**
- Create: `src-tauri/src/tushare/mod.rs`
- Create: `src-tauri/src/tushare/client.rs`

- [ ] **Step 1: Create tushare module export**

```rust
// src-tauri/src/tushare/mod.rs
pub mod client;
pub use client::TushareClient;
```

- [ ] **Step 2: Create TushareClient**

```rust
// src-tauri/src/tushare/client.rs
use serde::{Deserialize, Serialize};
use serde_json::json;

const TUSHARE_API_URL: &str = "https://api.tushare.pro";

#[derive(Debug, Clone)]
pub struct TushareClient {
    token: String,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct TushareResponse {
    code: i32,
    msg: Option<String>,
    data: Option<TushareData>,
}

#[derive(Debug, Deserialize)]
struct TushareData {
    fields: Vec<String>,
    items: Vec<Vec<serde_json::Value>>,
}

/// A single daily bar from Tushare `daily` API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBar {
    pub ts_code: String,
    pub trade_date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub pre_close: f64,
    pub change: f64,
    pub pct_chg: f64,
    pub vol: f64,
    pub amount: f64,
}

/// Basic stock info from `stock_basic`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockBasic {
    pub ts_code: String,
    pub symbol: String,
    pub name: String,
    pub area: String,
    pub industry: String,
    pub market: String,
    pub list_date: String,
}

/// Trade calendar entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeCal {
    pub exchange: String,
    pub cal_date: String,
    pub is_open: i32,
    pub pretrade_date: Option<String>,
}

impl TushareClient {
    pub fn new(token: String) -> Self {
        Self {
            token,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build reqwest client"),
        }
    }

    /// Generic Tushare API call. Returns rows as Vec<HashMap>.
    async fn call_api(
        &self,
        api_name: &str,
        params: serde_json::Value,
        fields: Option<&str>,
    ) -> Result<(Vec<String>, Vec<Vec<serde_json::Value>>), String> {
        let mut body = json!({
            "api_name": api_name,
            "token": self.token,
            "params": params,
        });
        if let Some(f) = fields {
            body["fields"] = json!(f);
        }

        let mut last_err = String::new();
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            let resp = self
                .client
                .post(TUSHARE_API_URL)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("tushare request: {}", e))?;

            if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                || resp.status().is_server_error()
            {
                last_err = format!("HTTP {}", resp.status());
                continue;
            }

            let parsed: TushareResponse = resp
                .json()
                .await
                .map_err(|e| format!("tushare parse: {}", e))?;

            if parsed.code != 0 {
                return Err(format!(
                    "tushare error {}: {}",
                    parsed.code,
                    parsed.msg.unwrap_or_default()
                ));
            }

            let data = parsed.data.ok_or("tushare: missing data")?;
            return Ok((data.fields, data.items));
        }
        Err(format!("tushare failed after 3 attempts: {}", last_err))
    }

    /// Fetch daily bars for a stock.
    pub async fn daily(
        &self,
        ts_code: &str,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<DailyBar>, String> {
        let params = json!({
            "ts_code": ts_code,
            "start_date": start_date,
            "end_date": end_date,
        });
        let (fields, items) = self
            .call_api("daily", params, Some("ts_code,trade_date,open,high,low,close,pre_close,change,pct_chg,vol,amount"))
            .await?;

        let idx = |name: &str| -> Option<usize> { fields.iter().position(|f| f == name) };

        let mut bars = Vec::new();
        for row in &items {
            let bar = DailyBar {
                ts_code: get_str(row, idx("ts_code")),
                trade_date: get_str(row, idx("trade_date")),
                open: get_f64(row, idx("open")),
                high: get_f64(row, idx("high")),
                low: get_f64(row, idx("low")),
                close: get_f64(row, idx("close")),
                pre_close: get_f64(row, idx("pre_close")),
                change: get_f64(row, idx("change")),
                pct_chg: get_f64(row, idx("pct_chg")),
                vol: get_f64(row, idx("vol")),
                amount: get_f64(row, idx("amount")),
            };
            bars.push(bar);
        }
        Ok(bars)
    }

    /// Fetch stock basic info by name (fuzzy) or code.
    pub async fn stock_basic(&self, name: Option<&str>) -> Result<Vec<StockBasic>, String> {
        let mut params = json!({});
        if let Some(n) = name {
            params["name"] = json!(n);
        }
        let (fields, items) = self
            .call_api("stock_basic", params, Some("ts_code,symbol,name,area,industry,market,list_date"))
            .await?;

        let idx = |name: &str| -> Option<usize> { fields.iter().position(|f| f == name) };

        let mut stocks = Vec::new();
        for row in &items {
            stocks.push(StockBasic {
                ts_code: get_str(row, idx("ts_code")),
                symbol: get_str(row, idx("symbol")),
                name: get_str(row, idx("name")),
                area: get_str(row, idx("area")),
                industry: get_str(row, idx("industry")),
                market: get_str(row, idx("market")),
                list_date: get_str(row, idx("list_date")),
            });
        }
        Ok(stocks)
    }

    /// Get latest close price for a stock (today or most recent trading day).
    pub async fn get_latest_price(&self, ts_code: &str) -> Result<f64, String> {
        let today = chrono::Local::now().format("%Y%m%d").to_string();
        let params = json!({
            "ts_code": ts_code,
            "end_date": today,
        });
        let (fields, items) = self
            .call_api("daily", params, Some("ts_code,trade_date,close"))
            .await?;

        if items.is_empty() {
            return Err(format!("no daily data for {}", ts_code));
        }
        let close_idx = fields
            .iter()
            .position(|f| f == "close")
            .ok_or("missing close field")?;
        let val = &items[0][close_idx];
        match val {
            serde_json::Value::Number(n) => n
                .as_f64()
                .ok_or_else(|| "close not a number".to_string()),
            serde_json::Value::String(s) => s
                .parse::<f64>()
                .map_err(|e| format!("parse close: {}", e)),
            _ => Err("unexpected close type".to_string()),
        }
    }

    /// Fetch trade calendar.
    pub async fn trade_cal(
        &self,
        exchange: &str,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<TradeCal>, String> {
        let params = json!({
            "exchange": exchange,
            "start_date": start_date,
            "end_date": end_date,
        });
        let (fields, items) = self
            .call_api("trade_cal", params, Some("exchange,cal_date,is_open,pretrade_date"))
            .await?;

        let idx = |name: &str| -> Option<usize> { fields.iter().position(|f| f == name) };

        let mut cals = Vec::new();
        for row in &items {
            cals.push(TradeCal {
                exchange: get_str(row, idx("exchange")),
                cal_date: get_str(row, idx("cal_date")),
                is_open: get_i64(row, idx("is_open")) as i32,
                pretrade_date: idx("pretrade_date").and_then(|i| {
                    if row[i].is_null() { None } else { Some(get_str(row, Some(i))) }
                }),
            });
        }
        Ok(cals)
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn get_str(row: &[serde_json::Value], idx: Option<usize>) -> String {
    idx.and_then(|i| row.get(i))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn get_f64(row: &[serde_json::Value], idx: Option<usize>) -> f64 {
    idx.and_then(|i| row.get(i))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
}

fn get_i64(row: &[serde_json::Value], idx: Option<usize>) -> i64 {
    idx.and_then(|i| row.get(i))
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_str() {
        let row = vec![json!("600519.SH"), json!(null)];
        assert_eq!(get_str(&row, Some(0)), "600519.SH");
        assert_eq!(get_str(&row, Some(1)), "");
        assert_eq!(get_str(&row, None), "");
    }

    #[test]
    fn test_get_f64() {
        let row = vec![json!(1800.5), json!("1800.5"), json!(null)];
        assert_eq!(get_f64(&row, Some(0)), 1800.5);
        assert_eq!(get_f64(&row, Some(1)), 1800.5);
        assert_eq!(get_f64(&row, Some(2)), 0.0);
    }
}
```

- [ ] **Step 3: Register tushare module in lib.rs**

In `src-tauri/src/lib.rs`, add after the existing `pub mod` declarations:

```rust
pub mod tushare;
```

- [ ] **Step 4: Run Rust checks**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```
Expected: Compiles successfully.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tushare/
git commit -m "feat(invest): add Tushare HTTP client"
```

---

## Task 3: Strategy Storage + Commands

**Files:**
- Create: `src-tauri/src/storage/invest/strategy.rs`
- Modify: `src-tauri/src/storage/invest/mod.rs`
- Modify: `src-tauri/src/commands/invest.rs`

- [ ] **Step 1: Add strategy table SQL to mod.rs**

In `src-tauri/src/storage/invest/mod.rs`, append to `CREATE_TABLES_SQL`:

```rust
CREATE TABLE IF NOT EXISTS strategy (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT 'default',
    targets TEXT NOT NULL DEFAULT '[]',
    max_single_pct REAL,
    min_cash_pct REAL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

- [ ] **Step 2: Create strategy.rs storage module**

```rust
// src-tauri/src/storage/invest/strategy.rs
use super::with_conn;
use rusqlite::params;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Strategy {
    pub id: String,
    pub name: String,
    /// JSON array of StrategyTarget objects.
    pub targets: String,
    pub max_single_pct: Option<f64>,
    pub min_cash_pct: Option<f64>,
    pub updated_at: String,
}

pub fn get_strategy(id: &str) -> Result<Option<Strategy>, String> {
    with_conn(|conn| {
        let result = conn.query_row(
            "SELECT id, name, targets, max_single_pct, min_cash_pct, updated_at FROM strategy WHERE id = ?1",
            params![id],
            |row| {
                Ok(Strategy {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    targets: row.get(2)?,
                    max_single_pct: row.get(3)?,
                    min_cash_pct: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            },
        );
        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("get strategy: {}", e)),
        }
    })
}

pub fn list_strategies() -> Result<Vec<Strategy>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare("SELECT id, name, targets, max_single_pct, min_cash_pct, updated_at FROM strategy ORDER BY name")
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Strategy {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    targets: row.get(2)?,
                    max_single_pct: row.get(3)?,
                    min_cash_pct: row.get(4)?,
                    updated_at: row.get(5)?,
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

pub fn save_strategy(s: &Strategy) -> Result<(), String> {
    with_conn(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        let updated = if s.updated_at.is_empty() { &now } else { &s.updated_at };
        conn.execute(
            "INSERT INTO strategy (id, name, targets, max_single_pct, min_cash_pct, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET name=?2, targets=?3, max_single_pct=?4, min_cash_pct=?5, updated_at=?6",
            params![s.id, s.name, s.targets, s.max_single_pct, s.min_cash_pct, updated],
        )
        .map_err(|e| format!("save strategy: {}", e))?;
        Ok(())
    })
}

pub fn delete_strategy(id: &str) -> Result<(), String> {
    with_conn(|conn| {
        let changed = conn
            .execute("DELETE FROM strategy WHERE id = ?1", params![id])
            .map_err(|e| format!("delete strategy: {}", e))?;
        if changed == 0 {
            Err("Strategy not found".to_string())
        } else {
            Ok(())
        }
    })
}
```

- [ ] **Step 3: Register strategy module in invest/mod.rs**

In `src-tauri/src/storage/invest/mod.rs`, add after existing `pub mod` lines:

```rust
pub mod strategy;
```

- [ ] **Step 4: Add strategy commands to invest.rs**

Append to `src-tauri/src/commands/invest.rs`:

```rust
use crate::storage::invest::strategy;

// ── Strategy ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_strategy(id: String) -> Result<Option<strategy::Strategy>, String> {
    strategy::get_strategy(&id)
}

#[tauri::command]
pub fn list_strategies() -> Result<Vec<strategy::Strategy>, String> {
    strategy::list_strategies()
}

#[tauri::command]
pub fn save_strategy(
    id: Option<String>,
    name: String,
    targets: String,
    max_single_pct: Option<f64>,
    min_cash_pct: Option<f64>,
) -> Result<(), String> {
    let sid = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let now = chrono::Utc::now().to_rfc3339();
    let s = strategy::Strategy {
        id: sid,
        name,
        targets,
        max_single_pct,
        min_cash_pct,
        updated_at: now,
    };
    strategy::save_strategy(&s)
}

#[tauri::command]
pub fn delete_strategy(id: String) -> Result<(), String> {
    strategy::delete_strategy(&id)
}
```

- [ ] **Step 5: Register new commands in lib.rs**

In `src-tauri/src/lib.rs`, find the `.invoke_handler(tauri::generate_handler![...])` block for invest commands and add:

```rust
commands::invest::get_strategy,
commands::invest::list_strategies,
commands::invest::save_strategy,
commands::invest::delete_strategy,
```

- [ ] **Step 6: Run Rust checks**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/storage/invest/strategy.rs src-tauri/src/storage/invest/mod.rs src-tauri/src/commands/invest.rs
git commit -m "feat(invest): add strategy storage and commands"
```

---

## Task 4: Add tushare_token to UserSettings

**Files:**
- Modify: `src-tauri/src/models.rs`

- [ ] **Step 1: Add tushare_token field to UserSettings**

In `src-tauri/src/models.rs`, find the `UserSettings` struct (around line 294) and add before `updated_at`:

```rust
    /// Tushare Pro API token for market data fetching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tushare_token: Option<String>,
```

- [ ] **Step 2: Run Rust check**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```
Expected: Compiles. The field is `Option<String>` with `skip_serializing_if`, so existing settings files without this field will deserialize fine (defaults to `None`).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/models.rs
git commit -m "feat(invest): add tushare_token to UserSettings"
```

---

## Task 5: Fix trades CHECK constraint for cash_adjust

**Files:**
- Modify: `src-tauri/src/storage/invest/mod.rs`

- [ ] **Step 1: Add ALTER TABLE migration for cash_adjust**

In `src-tauri/src/storage/invest/mod.rs`, inside `init_db()`, after `CREATE_TABLES_SQL`:

```rust
    // Migrate: allow 'cash_adjust' action in trades table.
    // SQLite doesn't support ALTER CHECK constraints, so we use a workaround:
    // Check if cash_adjust rows can be inserted by attempting a no-op insert.
    // The constraint is text-based so we just document the intent.
    // Actual enforcement happens at the application layer.
```

Since SQLite doesn't support modifying CHECK constraints after table creation, we need to recreate the table. Add this migration after `CREATE_TABLES_SQL`:

```rust
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
```

- [ ] **Step 2: Run Rust check**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/storage/invest/mod.rs
git commit -m "fix(invest): migrate trades table to allow cash_adjust action"
```

---

## Task 6: Tushare Tauri Commands + Trade Calendar Sync

**Files:**
- Modify: `src-tauri/src/commands/invest.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add Tushare Tauri commands**

Append to `src-tauri/src/commands/invest.rs`:

```rust
// ── Tushare Market Data ───────────────────────────────────────────────────

#[tauri::command]
pub async fn search_stocks(
    name: String,
    token: String,
) -> Result<Vec<crate::tushare::client::StockBasic>, String> {
    let client = crate::tushare::TushareClient::new(token);
    client.stock_basic(Some(&name)).await
}

#[tauri::command]
pub async fn get_latest_price(
    ts_code: String,
    token: String,
) -> Result<f64, String> {
    let client = crate::tushare::TushareClient::new(token);
    client.get_latest_price(&ts_code).await
}

#[tauri::command]
pub async fn get_daily_bars(
    ts_code: String,
    start_date: String,
    end_date: String,
    token: String,
) -> Result<Vec<crate::tushare::client::DailyBar>, String> {
    let client = crate::tushare::TushareClient::new(token);
    client.daily(&ts_code, &start_date, &end_date).await
}

// ── Trade Calendar Sync ──────────────────────────────────────────────────

#[tauri::command]
pub async fn sync_trade_calendar(token: String) -> Result<usize, String> {
    let client = crate::tushare::TushareClient::new(token);
    let today = chrono::Local::now();
    let start = today.format("%Y%m%d").to_string();
    let end = (today + chrono::Duration::days(730)).format("%Y%m%d").to_string();

    let cals = client.trade_cal("SSE", &start, &end).await?;
    let count = cals.len();
    for cal in &cals {
        scheduler::upsert_trade_calendar(&cal.cal_date, cal.is_open != 0, cal.pretrade_date.as_deref())?;
    }
    Ok(count)
}

// ── Legacy Migration ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn migrate_legacy_portfolio(app_handle: tauri::AppHandle) -> Result<String, String> {
    let home = dirs::home_dir().ok_or("cannot find home dir")?;
    let legacy_path = home.join(".claw-go").join("invest").join("portfolio.json");
    if !legacy_path.exists() {
        return Ok("no_legacy".to_string());
    }

    let content = std::fs::read_to_string(&legacy_path)
        .map_err(|e| format!("read legacy: {}", e))?;
    let data: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("parse legacy: {}", e))?;

    let mut migrated = 0;

    // Migrate cash balance
    if let Some(cash) = data.get("cash").and_then(|v| v.as_f64()) {
        portfolio::set_cash(cash)?;
        migrated += 1;
    }

    // Migrate holdings
    if let Some(holdings) = data.get("holdings").and_then(|v| v.as_array()) {
        for h in holdings {
            let symbol = h.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
            let name = h.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
            let shares = h.get("shares").and_then(|v| v.as_f64());
            let avg_cost = h.get("avg_cost").or(h.get("cost")).and_then(|v| v.as_f64());
            let kind = h.get("kind").and_then(|v| v.as_str()).unwrap_or("hold");

            if symbol.is_empty() { continue; }

            let holding = portfolio::Holding {
                symbol: symbol.to_string(),
                currency: "CNY".to_string(),
                kind: kind.to_string(),
                name,
                notional: 0.0,
                avg_cost,
                shares,
                entry_date: None,
                linked_verdict_id: None,
                notes: Some("migrated from legacy".to_string()),
                created_at: String::new(),
                updated_at: String::new(),
            };
            portfolio::upsert_holding(&holding)?;
            migrated += 1;
        }
    }

    // Rename legacy file
    let backup = legacy_path.with_extension("legacy");
    let _ = std::fs::rename(&legacy_path, &backup);

    Ok(format!("migrated {} items", migrated))
}
```

- [ ] **Step 2: Register new commands in lib.rs**

Add to the invest command handler block in `src-tauri/src/lib.rs`:

```rust
commands::invest::search_stocks,
commands::invest::get_latest_price,
commands::invest::get_daily_bars,
commands::invest::sync_trade_calendar,
commands::invest::migrate_legacy_portfolio,
```

- [ ] **Step 3: Add startup trade calendar sync**

In `src-tauri/src/lib.rs`, after the invest DB init block, add:

```rust
    // Sync trade calendar on startup (non-blocking).
    {
        let cal_data_dir = data_dir.clone();
        tauri::async_runtime::spawn(async move {
            let settings = crate::storage::settings::get_user_settings();
            if let Some(token) = settings.tushare_token {
                let client = crate::tushare::TushareClient::new(token);
                let today = chrono::Local::now();
                let start = today.format("%Y%m%d").to_string();
                let end = (today + chrono::Duration::days(730)).format("%Y%m%d").to_string();
                match client.trade_cal("SSE", &start, &end).await {
                    Ok(cals) => {
                        let mut count = 0;
                        for cal in &cals {
                            if crate::storage::invest::scheduler::upsert_trade_calendar(
                                &cal.cal_date,
                                cal.is_open != 0,
                                cal.pretrade_date.as_deref(),
                            ).is_ok() {
                                count += 1;
                            }
                        }
                        log::info!("[invest] synced {} trade calendar entries", count);
                    }
                    Err(e) => log::warn!("[invest] trade calendar sync failed: {}", e),
                }
            } else {
                log::debug!("[invest] no tushare_token, skipping calendar sync");
            }
        });
    }
```

- [ ] **Step 4: Run Rust check**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/invest.rs src-tauri/src/lib.rs
git commit -m "feat(invest): add Tushare commands, trade calendar sync, legacy migration"
```

---

## Task 7: Invest Store

**Files:**
- Create: `src/lib/stores/invest-store.svelte.ts`

- [ ] **Step 1: Create the invest store**

This is the central frontend state manager. It wraps all Tauri `invoke` calls and exposes reactive `$state` runes.

```ts
// src/lib/stores/invest-store.svelte.ts
import { invoke } from '$lib/transport';
import type {
  Holding,
  Trade,
  PnlSnapshot,
  Verdict,
  Strategy,
  PriceQuote,
} from '$lib/types/invest';

class InvestStore {
  // ── State ────────────────────────────────────────────────────────────
  holdings = $state<Holding[]>([]);
  trades = $state<Trade[]>([]);
  pnlSnapshots = $state<PnlSnapshot[]>([]);
  verdicts = $state<Verdict[]>([]);
  cash = $state<number>(0);
  strategies = $state<Strategy[]>([]);

  loading = $state<boolean>(false);
  error = $state<string | null>(null);

  /** Live price cache: tsCode → PriceQuote */
  priceMap = $state<Record<string, PriceQuote>>({});

  // ── Derived ──────────────────────────────────────────────────────────
  holdHoldings = $derived(this.holdings.filter((h) => h.kind === 'hold'));
  watchHoldings = $derived(this.holdings.filter((h) => h.kind === 'watch'));
  holdCount = $derived(this.holdHoldings.length);
  watchCount = $derived(this.watchHoldings.length);

  holdingsMarketValue = $derived(
    this.holdHoldings.reduce((sum, h) => {
      const price = this.priceMap[h.symbol]?.close;
      if (price && h.shares) return sum + price * h.shares;
      return sum + (h.notional || 0);
    }, 0)
  );

  totalAssets = $derived(this.cash + this.holdingsMarketValue);

  totalCostBasis = $derived(
    this.holdHoldings.reduce((sum, h) => {
      if (h.avgCost && h.shares) return sum + h.avgCost * h.shares;
      return sum + (h.notional || 0);
    }, 0)
  );

  totalReturnPct = $derived(
    this.totalCostBasis > 0
      ? ((this.totalAssets - this.totalCostBasis) / this.totalCostBasis) * 100
      : 0
  );

  // ── Actions ──────────────────────────────────────────────────────────

  async loadAll(): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      const [holdings, trades, snapshots, verdicts, cash, strategies] =
        await Promise.all([
          invoke<Holding[]>('get_holdings'),
          invoke<Trade[]>('get_trades', { symbol: null, limit: 200 }),
          invoke<PnlSnapshot[]>('get_pnl_snapshots', { limit: 80 }),
          invoke<Verdict[]>('get_verdicts', { symbol: null, limit: 50 }),
          invoke<number>('get_cash'),
          invoke<Strategy[]>('list_strategies').catch(() => []),
        ]);
      this.holdings = holdings;
      this.trades = trades;
      this.pnlSnapshots = snapshots;
      this.verdicts = verdicts;
      this.cash = cash;
      this.strategies = strategies;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }

  async refreshPrices(tushareToken: string): Promise<void> {
    const holdSyms = this.holdHoldings.map((h) => h.symbol);
    if (holdSyms.length === 0 || !tushareToken) return;

    const entries: [string, PriceQuote][] = [];
    for (const sym of holdSyms) {
      try {
        const bars = await invoke<
          Array<{
            tsCode: string;
            close: number;
            change: number;
            pctChg: number;
            vol: number;
            amount: number;
          }>
        >('get_daily_bars', {
          tsCode: sym,
          startDate: '',
          endDate: '',
          token: tushareToken,
        });
        if (bars.length > 0) {
          const latest = bars[0];
          entries.push([
            sym,
            {
              tsCode: latest.tsCode,
              name: '',
              close: latest.close,
              change: latest.change,
              pctChg: latest.pctChg,
              vol: latest.vol,
              amount: latest.amount,
            },
          ]);
        }
      } catch {
        // Skip failed stocks silently
      }
    }
    this.priceMap = Object.fromEntries(entries);
  }

  /** Get live price for stock search (buy dialog). */
  async searchStocks(
    name: string,
    tushareToken: string
  ): Promise<
    Array<{ tsCode: string; name: string; symbol: string; industry: string }>
  > {
    return invoke('search_stocks', { name, token: tushareToken });
  }

  async getLatestPrice(
    tsCode: string,
    tushareToken: string
  ): Promise<number> {
    return invoke('get_latest_price', { tsCode, token: tushareToken });
  }

  // ── Portfolio Operations ─────────────────────────────────────────────

  async buyStock(
    symbol: string,
    name: string,
    qty: number,
    price: number,
    tushareToken: string
  ): Promise<void> {
    const amount = qty * price;
    const now = new Date().toISOString();

    // 1. Record trade
    await invoke('record_trade', {
      id: null,
      symbol,
      currency: 'CNY',
      kind: 'hold',
      action: 'buy',
      shares: qty,
      price,
      amount,
      notes: null,
    });

    // 2. Upsert holding (add shares, weighted avg cost)
    const existing = this.holdHoldings.find((h) => h.symbol === symbol);
    if (existing && existing.shares && existing.avgCost) {
      const newShares = existing.shares + qty;
      const newAvgCost =
        (existing.avgCost * existing.shares + price * qty) / newShares;
      await invoke('update_holding', {
        symbol,
        currency: 'CNY',
        kind: 'hold',
        name: name || existing.name,
        notional: 0,
        avgCost: newAvgCost,
        shares: newShares,
        entryDate: existing.entryDate,
        linkedVerdictId: existing.linkedVerdictId,
        notes: existing.notes,
      });
    } else {
      await invoke('add_holding', {
        symbol,
        currency: 'CNY',
        kind: 'hold',
        name,
        notional: 0,
        avgCost: price,
        shares: qty,
        entryDate: now.split('T')[0],
        linkedVerdictId: null,
        notes: null,
      });
    }

    // 3. Deduct cash
    await invoke('update_cash', { available: this.cash - amount });
    this.cash = this.cash - amount;

    // Refresh
    await this.loadAll();
  }

  async sellStock(
    symbol: string,
    qty: number,
    price: number
  ): Promise<void> {
    const amount = qty * price;
    const existing = this.holdHoldings.find((h) => h.symbol === symbol);
    if (!existing) throw new Error('Holding not found');

    // 1. Record trade
    await invoke('record_trade', {
      id: null,
      symbol,
      currency: 'CNY',
      kind: 'hold',
      action: 'sell',
      shares: qty,
      price,
      amount,
      notes: null,
    });

    // 2. Update or delete holding
    const remaining = (existing.shares || 0) - qty;
    if (remaining <= 0.0001) {
      await invoke('delete_holding', {
        symbol,
        currency: 'CNY',
        kind: 'hold',
      });
    } else {
      await invoke('update_holding', {
        symbol,
        currency: 'CNY',
        kind: 'hold',
        name: existing.name,
        notional: 0,
        avgCost: existing.avgCost,
        shares: remaining,
        entryDate: existing.entryDate,
        linkedVerdictId: existing.linkedVerdictId,
        notes: existing.notes,
      });
    }

    // 3. Add cash
    await invoke('update_cash', { available: this.cash + amount });
    this.cash = this.cash + amount;

    await this.loadAll();
  }

  async updateCash(newBalance: number, reason?: string): Promise<void> {
    await invoke('update_cash', { available: newBalance });
    await invoke('record_trade', {
      id: null,
      symbol: 'CASH',
      currency: 'CNY',
      kind: 'hold',
      action: 'cash_adjust',
      shares: null,
      price: null,
      amount: newBalance,
      notes: reason || null,
    });
    this.cash = newBalance;
    await this.loadAll();
  }

  async convertWatchToHold(
    symbol: string,
    name: string,
    qty: number,
    price: number
  ): Promise<void> {
    const amount = qty * price;

    // Delete watch entry
    await invoke('delete_holding', {
      symbol,
      currency: 'CNY',
      kind: 'watch',
    });

    // Add as hold
    await invoke('add_holding', {
      symbol,
      currency: 'CNY',
      kind: 'hold',
      name,
      notional: 0,
      avgCost: price,
      shares: qty,
      entryDate: new Date().toISOString().split('T')[0],
      linkedVerdictId: null,
      notes: 'converted from watchlist',
    });

    // Record conversion trade
    await invoke('record_trade', {
      id: null,
      symbol,
      currency: 'CNY',
      kind: 'hold',
      action: 'convert_watch_to_hold',
      shares: qty,
      price,
      amount,
      notes: null,
    });

    // Deduct cash
    await invoke('update_cash', { available: this.cash - amount });
    this.cash = this.cash - amount;

    await this.loadAll();
  }

  // ── Strategy ─────────────────────────────────────────────────────────

  async saveStrategy(
    id: string | null,
    name: string,
    targets: Array<{ symbol: string; name: string; targetPct: number }>,
    maxSinglePct: number | null,
    minCashPct: number | null
  ): Promise<void> {
    await invoke('save_strategy', {
      id,
      name,
      targets: JSON.stringify(targets),
      maxSinglePct,
      minCashPct,
    });
    await this.loadAll();
  }

  async deleteStrategy(id: string): Promise<void> {
    await invoke('delete_strategy', { id });
    await this.loadAll();
  }
}

export const investStore = new InvestStore();
```

- [ ] **Step 2: Verify types**

Run: `npx svelte-kit sync && npm run check`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add src/lib/stores/invest-store.svelte.ts
git commit -m "feat(invest): add invest store with Svelte 5 runes"
```

---

## Task 8: PnL Snapshot Cron Job

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add PnL snapshot background task**

In `src-tauri/src/lib.rs`, after the trade calendar sync block, add:

```rust
    // Start PnL snapshot cron job.
    // Runs at 9:30, 11:00, 13:00, 15:00 Beijing time on weekdays.
    {
        tauri::async_runtime::spawn(async loop {
            // Calculate next run time
            let now = chrono::Local::now();
            let target_hours = [9, 11, 13, 15]; // hours, minute offset handled below
            let target_minutes = [30, 0, 0, 0]; // 9:30, 11:00, 13:00, 15:00

            let mut next_run = None;
            for (i, &hour) in target_hours.iter().enumerate() {
                let candidate = now
                    .date_naive()
                    .and_hms_opt(hour, target_minutes[i], 0)
                    .unwrap();
                let candidate = chrono::Local.from_local_datetime(&candidate).unwrap();
                if candidate > now {
                    next_run = Some(candidate);
                    break;
                }
            }
            // If all today's times passed, sleep until tomorrow 9:30
            let next = next_run.unwrap_or_else(|| {
                let tomorrow = now.date_naive() + chrono::Duration::days(1);
                let t = tomorrow.and_hms_opt(9, 30, 0).unwrap();
                chrono::Local.from_local_datetime(&t).unwrap()
            });

            let sleep_duration = (next - now).to_std().unwrap_or(std::time::Duration::from_secs(3600));
            tokio::time::sleep(sleep_duration).await;

            // Guard: only run on trading days
            let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
            match crate::storage::invest::scheduler::is_trading_day(&date_str) {
                Ok(true) => { /* proceed */ }
                _ => {
                    log::debug!("[invest-pnl] skipping non-trading day");
                    continue;
                }
            }

            let task_id = match crate::storage::invest::scheduler::log_task_start("pnl_snapshot") {
                Ok(id) => id,
                Err(e) => {
                    log::warn!("[invest-pnl] failed to log start: {}", e);
                    continue;
                }
            };

            let result = run_pnl_snapshot();
            match &result {
                Ok(msg) => {
                    let _ = crate::storage::invest::scheduler::log_task_end(task_id, "success", Some(msg));
                    log::info!("[invest-pnl] snapshot: {}", msg);
                }
                Err(e) => {
                    let _ = crate::storage::invest::scheduler::log_task_end(task_id, "error", Some(e));
                    log::warn!("[invest-pnl] failed: {}", e);
                }
            }
        });
    }
```

- [ ] **Step 2: Add run_pnl_snapshot helper function**

In `src-tauri/src/lib.rs` (or a new file `src-tauri/src/invest_cron.rs`), add:

```rust
/// Run a PnL snapshot: get current holdings, calculate total value, save snapshot.
fn run_pnl_snapshot() -> Result<String, String> {
    use crate::storage::invest::{portfolio, verdicts};

    let settings = crate::storage::settings::get_user_settings();
    let token = settings
        .tushare_token
        .ok_or("no tushare_token configured")?;

    let holdings = portfolio::list_holdings()?;
    let hold_items: Vec<_> = holdings.iter().filter(|h| h.kind == "hold").collect();
    if hold_items.is_empty() {
        return Ok("no holdings, skipping".to_string());
    }

    let cash = portfolio::get_cash()?;
    let rt = tokio::runtime::Handle::current();

    let mut holdings_value = 0.0;
    for h in &hold_items {
        if let (Some(shares), Some(_cost)) = (h.shares, h.avgCost) {
            // Use blocking call inside spawn_blocking for async Tushare
            let sym = h.symbol.clone();
            let tok = token.clone();
            let price = rt.block_on(async {
                let client = crate::tushare::TushareClient::new(tok);
                client.get_latest_price(&sym).await
            });
            match price {
                Ok(p) => holdings_value += p * shares,
                Err(e) => log::warn!("[invest-pnl] price fetch failed for {}: {}", h.symbol, e),
            }
        }
    }

    let total_value = cash + holdings_value;

    // Calculate daily PnL vs previous snapshot
    let prev = verdicts::list_pnl_snapshots(Some(1))?;
    let (daily_pnl, daily_pnl_pct) = if let Some(last) = prev.first() {
        let pnl = total_value - last.total_value;
        let pct = if last.total_value > 0.0 {
            (pnl / last.total_value) * 100.0
        } else {
            0.0
        };
        (Some(pnl), Some(pct))
    } else {
        (None, None)
    };

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let snapshot = verdicts::PnlSnapshot {
        id: 0,
        snapshot_date: today,
        total_value,
        cash,
        holdings_value,
        daily_pnl,
        daily_pnl_pct,
        created_at: String::new(),
    };
    let id = verdicts::save_pnl_snapshot(&snapshot)?;
    Ok(format!("saved snapshot #{}: total={:.2}", id, total_value))
}
```

- [ ] **Step 3: Run Rust check**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat(invest): add PnL snapshot cron job"
```

---

## Task 9: i18n Keys

**Files:**
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

- [ ] **Step 1: Add English i18n keys**

Add to `messages/en.json` under the invest section (or create if absent):

```json
{
  "invest_dashboard": "Dashboard",
  "invest_total_assets": "Total Assets",
  "invest_holdings_value": "Holdings Value",
  "invest_cash": "Available Cash",
  "invest_total_return": "Total Return",
  "invest_position_count": "Positions",
  "invest_hold": "HOLD",
  "invest_watch": "WATCH",
  "invest_buy": "Buy",
  "invest_sell": "Sell",
  "invest_edit_cash": "Edit Cash",
  "invest_stock_search": "Search by name or code...",
  "invest_quantity": "Quantity",
  "invest_price": "Price",
  "invest_market_price": "Market Price",
  "invest_confirm_buy": "Confirm Buy",
  "invest_confirm_sell": "Confirm Sell",
  "invest_cash_new_balance": "New Balance",
  "invest_cash_reason": "Reason (optional)",
  "invest_no_holdings": "No holdings yet. Click Buy to add.",
  "invest_no_pnl": "No PnL data yet. First snapshot will be generated on the next trading day.",
  "invest_pnl_chart": "PnL Trend",
  "invest_trade_log": "Trade Log",
  "invest_strategy": "Strategy",
  "invest_strategy_targets": "Target Allocation",
  "invest_strategy_add_target": "Add Target",
  "invest_strategy_max_single": "Max Single Stock %",
  "invest_strategy_min_cash": "Min Cash %",
  "invest_strategy_save": "Save Strategy",
  "invest_strategy_empty": "No strategy configured yet.",
  "invest_trade_date": "Date",
  "invest_trade_stock": "Stock",
  "invest_trade_direction": "Direction",
  "invest_trade_amount": "Amount",
  "invest_trade_notes": "Notes",
  "invest_convert_to_hold": "Convert to HOLD",
  "invest_legacy_migrate": "Legacy data detected. Click to migrate.",
  "invest_legacy_done": "Migration complete: {count} items",
  "invest_no_token": "Set Tushare token in Settings to enable live prices.",
  "invest_refresh_prices": "Refresh Prices"
}
```

- [ ] **Step 2: Add Chinese i18n keys**

Add to `messages/zh-CN.json`:

```json
{
  "invest_dashboard": "仪表盘",
  "invest_total_assets": "总资产",
  "invest_holdings_value": "持仓市值",
  "invest_cash": "可用现金",
  "invest_total_return": "总收益率",
  "invest_position_count": "持仓数量",
  "invest_hold": "持仓",
  "invest_watch": "观察",
  "invest_buy": "买入",
  "invest_sell": "卖出",
  "invest_edit_cash": "编辑现金",
  "invest_stock_search": "输入代码或名称搜索...",
  "invest_quantity": "数量",
  "invest_price": "价格",
  "invest_market_price": "市价",
  "invest_confirm_buy": "确认买入",
  "invest_confirm_sell": "确认卖出",
  "invest_cash_new_balance": "新余额",
  "invest_cash_reason": "原因（可选）",
  "invest_no_holdings": "暂无持仓，点击买入添加",
  "invest_no_pnl": "暂无 PnL 数据，首个快照将在下一个交易日生成",
  "invest_pnl_chart": "PnL 趋势",
  "invest_trade_log": "交易记录",
  "invest_strategy": "策略配置",
  "invest_strategy_targets": "目标配置",
  "invest_strategy_add_target": "添加目标",
  "invest_strategy_max_single": "最大单股集中度 %",
  "invest_strategy_min_cash": "最小现金比例 %",
  "invest_strategy_save": "保存策略",
  "invest_strategy_empty": "暂未配置策略",
  "invest_trade_date": "日期",
  "invest_trade_stock": "股票",
  "invest_trade_direction": "方向",
  "invest_trade_amount": "金额",
  "invest_trade_notes": "备注",
  "invest_convert_to_hold": "转为持仓",
  "invest_legacy_migrate": "检测到旧版投资数据，点击迁移",
  "invest_legacy_done": "迁移完成：{count} 条",
  "invest_no_token": "请在设置中配置 Tushare Token 以启用实时行情",
  "invest_refresh_prices": "刷新行情"
}
```

- [ ] **Step 3: Run i18n check**

```bash
npm run i18n:check
```
Expected: No missing/extra keys.

- [ ] **Step 4: Commit**

```bash
git add messages/en.json messages/zh-CN.json
git commit -m "feat(invest): add i18n keys for Phase 2"
```

---

## Task 10: Settings UI — Tushare Token Input

**Files:**
- Modify: `src/routes/settings/+page.svelte` (or the relevant settings component)

- [ ] **Step 1: Find the settings page structure**

Run: `grep -n "tushare\|mcp_server\|embedding_config" src/routes/settings/+page.svelte | head -20`

Identify where connection/API settings are rendered. Add a tushare_token input in the appropriate section.

- [ ] **Step 2: Add tushare token input field**

In the settings page, in the connection/API section, add:

```svelte
<div class="flex items-center gap-2">
  <label class="w-32 text-sm">{t('settings_tushare_token')}</label>
  <input
    type="password"
    class="flex-1 rounded border bg-background px-3 py-1.5 text-sm"
    placeholder="Tushare Pro API Token"
    value={settings.tushareToken ?? ''}
    oninput={(e) => {
      const val = e.currentTarget.value || null;
      updateSettings({ tushareToken: val });
    }}
  />
</div>
```

- [ ] **Step 3: Add i18n keys for settings**

Add to both `messages/en.json` and `messages/zh-CN.json`:

```json
"settings_tushare_token": "Tushare Token"
```

```json
"settings_tushare_token": "Tushare Token"
```

- [ ] **Step 4: Commit**

```bash
git add src/routes/settings/+page.svelte messages/en.json messages/zh-CN.json
git commit -m "feat(invest): add tushare token setting input"
```

---

## Task 11: Dashboard UI Components

**Files:**
- Create: `src/lib/components/invest/KpiCard.svelte`
- Create: `src/lib/components/invest/HoldingsTable.svelte`
- Create: `src/lib/components/invest/TradeDialog.svelte`
- Modify: `src/routes/invest/+page.svelte`

- [ ] **Step 1: Create KpiCard component**

```svelte
<!-- src/lib/components/invest/KpiCard.svelte -->
<script lang="ts">
  let { label, value, sub, trend }: {
    label: string;
    value: string;
    sub?: string;
    trend?: 'up' | 'down' | 'neutral';
  } = $props();
</script>

<div class="rounded-lg border bg-card p-4">
  <p class="text-xs text-muted-foreground">{label}</p>
  <p class="mt-1 text-2xl font-bold tabular-nums {trend === 'up' ? 'text-green-600' : trend === 'down' ? 'text-red-600' : ''}">
    {value}
  </p>
  {#if sub}
    <p class="mt-0.5 text-xs text-muted-foreground">{sub}</p>
  {/if}
</div>
```

- [ ] **Step 2: Create HoldingsTable component**

```svelte
<!-- src/lib/components/invest/HoldingsTable.svelte -->
<script lang="ts">
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import type { Holding } from '$lib/types/invest';

  let { onSell, onConvert, tushareToken }: {
    onSell: (h: Holding) => void;
    onConvert: (h: Holding) => void;
    tushareToken: string;
  } = $props();

  function getPrice(sym: string): number | null {
    return investStore.priceMap[sym]?.close ?? null;
  }

  function getPnlPct(h: Holding): number | null {
    const price = getPrice(h.symbol);
    if (!price || !h.avgCost || !h.avgCost) return null;
    return ((price - h.avgCost) / h.avgCost) * 100;
  }
</script>

<div class="space-y-4">
  <!-- HOLD section -->
  <div>
    <h3 class="mb-2 text-sm font-medium text-muted-foreground">
      {t('invest_hold')} ({investStore.holdCount})
    </h3>
    {#if investStore.holdHoldings.length === 0}
      <p class="py-4 text-center text-sm text-muted-foreground">{t('invest_no_holdings')}</p>
    {:else}
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b text-left text-xs text-muted-foreground">
              <th class="pb-2 pr-4">{t('invest_trade_stock')}</th>
              <th class="pb-2 pr-4">{t('invest_quantity')}</th>
              <th class="pb-2 pr-4">成本价</th>
              <th class="pb-2 pr-4">现价</th>
              <th class="pb-2 pr-4">盈亏%</th>
              <th class="pb-2"></th>
            </tr>
          </thead>
          <tbody>
            {#each investStore.holdHoldings as h}
              <tr class="border-b border-border/50">
                <td class="py-2 pr-4">
                  <span class="font-medium">{h.name ?? h.symbol}</span>
                  <span class="ml-1 text-xs text-muted-foreground">{h.symbol}</span>
                </td>
                <td class="py-2 pr-4 tabular-nums">{h.shares ?? '-'}</td>
                <td class="py-2 pr-4 tabular-nums">{h.avgCost?.toFixed(2) ?? '-'}</td>
                <td class="py-2 pr-4 tabular-nums">{getPrice(h.symbol)?.toFixed(2) ?? '-'}</td>
                <td class="py-2 pr-4 tabular-nums">
                  {#if getPnlPct(h) !== null}
                    <span class={getPnlPct(h)! >= 0 ? 'text-green-600' : 'text-red-600'}>
                      {getPnlPct(h)!.toFixed(2)}%
                    </span>
                  {:else}
                    -
                  {/if}
                </td>
                <td class="py-2">
                  <button
                    class="rounded px-2 py-0.5 text-xs hover:bg-muted"
                    onclick={() => onSell(h)}
                  >{t('invest_sell')}</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>

  <!-- WATCH section -->
  {#if investStore.watchHoldings.length > 0}
    <div>
      <h3 class="mb-2 text-sm font-medium text-muted-foreground">
        {t('invest_watch')} ({investStore.watchCount})
      </h3>
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b text-left text-xs text-muted-foreground">
              <th class="pb-2 pr-4">{t('invest_trade_stock')}</th>
              <th class="pb-2 pr-4">现价</th>
              <th class="pb-2"></th>
            </tr>
          </thead>
          <tbody>
            {#each investStore.watchHoldings as h}
              <tr class="border-b border-border/50">
                <td class="py-2 pr-4">
                  <span class="font-medium">{h.name ?? h.symbol}</span>
                  <span class="ml-1 text-xs text-muted-foreground">{h.symbol}</span>
                </td>
                <td class="py-2 pr-4 tabular-nums">{getPrice(h.symbol)?.toFixed(2) ?? '-'}</td>
                <td class="py-2">
                  <button
                    class="rounded px-2 py-0.5 text-xs hover:bg-muted"
                    onclick={() => onConvert(h)}
                  >{t('invest_convert_to_hold')}</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  {/if}
</div>
```

- [ ] **Step 3: Create TradeDialog component**

```svelte
<!-- src/lib/components/invest/TradeDialog.svelte -->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import type { Holding } from '$lib/types/invest';

  let { mode, prefill, tushareToken, onClose }: {
    mode: 'buy' | 'sell' | 'cash';
    prefill?: { symbol?: string; name?: string; holding?: Holding };
    tushareToken: string;
    onClose: () => void;
  } = $props();

  let symbol = $state(prefill?.symbol ?? '');
  let name = $state(prefill?.name ?? '');
  let quantity = $state(0);
  let price = $state(0);
  let cashBalance = $state(investStore.cash);
  let cashReason = $state('');
  let loading = $state(false);
  let error = $state<string | null>(null);
  let searchResults = $state<Array<{ tsCode: string; name: string }>>([]);
  let searchQuery = $state('');

  async function doSearch() {
    if (!searchQuery || !tushareToken) return;
    try {
      searchResults = await investStore.searchStocks(searchQuery, tushareToken);
    } catch (e) {
      error = String(e);
    }
  }

  async function fillMarketPrice() {
    if (!symbol || !tushareToken) return;
    try {
      price = await investStore.getLatestPrice(symbol, tushareToken);
    } catch (e) {
      error = String(e);
    }
  }

  async function handleSubmit() {
    loading = true;
    error = null;
    try {
      if (mode === 'buy') {
        await investStore.buyStock(symbol, name, quantity, price, tushareToken);
      } else if (mode === 'sell') {
        await investStore.sellStock(symbol, quantity, price);
      } else if (mode === 'cash') {
        await investStore.updateCash(cashBalance, cashReason);
      }
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
  <div class="w-full max-w-md rounded-lg border bg-background p-6 shadow-lg">
    <h2 class="mb-4 text-lg font-semibold">
      {mode === 'buy' ? t('invest_confirm_buy') : mode === 'sell' ? t('invest_confirm_sell') : t('invest_edit_cash')}
    </h2>

    {#if error}
      <p class="mb-3 rounded bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p>
    {/if}

    {#if mode === 'buy'}
      <!-- Stock search -->
      <div class="mb-3">
        <label class="mb-1 block text-sm">{t('invest_trade_stock')}</label>
        <div class="flex gap-2">
          <input
            class="flex-1 rounded border bg-background px-3 py-1.5 text-sm"
            placeholder={t('invest_stock_search')}
            bind:value={searchQuery}
          />
          <button class="rounded bg-muted px-3 py-1.5 text-sm" onclick={doSearch}>搜索</button>
        </div>
        {#if searchResults.length > 0}
          <div class="mt-1 max-h-32 overflow-auto rounded border">
            {#each searchResults as s}
              <button
                class="w-full px-3 py-1.5 text-left text-sm hover:bg-muted"
                onclick={() => { symbol = s.tsCode; name = s.name; searchResults = []; }}
              >
                {s.name} ({s.tsCode})
              </button>
            {/each}
          </div>
        {/if}
        {#if symbol}
          <p class="mt-1 text-xs text-muted-foreground">已选: {name} ({symbol})</p>
        {/if}
      </div>
    {/if}

    {#if mode !== 'cash'}
      <div class="mb-3 grid grid-cols-2 gap-3">
        <div>
          <label class="mb-1 block text-sm">{t('invest_quantity')}</label>
          <input
            type="number"
            class="w-full rounded border bg-background px-3 py-1.5 text-sm"
            step="100"
            min="0"
            bind:value={quantity}
          />
        </div>
        <div>
          <label class="mb-1 block text-sm">{t('invest_price')}</label>
          <div class="flex gap-1">
            <input
              type="number"
              class="flex-1 rounded border bg-background px-3 py-1.5 text-sm"
              step="0.01"
              bind:value={price}
            />
            <button
              class="rounded bg-muted px-2 py-1.5 text-xs"
              onclick={fillMarketPrice}
            >{t('invest_market_price')}</button>
          </div>
        </div>
      </div>
      <p class="mb-3 text-sm text-muted-foreground">
        金额: ¥{(quantity * price).toLocaleString(undefined, { minimumFractionDigits: 2 })}
      </p>
    {:else}
      <div class="mb-3">
        <label class="mb-1 block text-sm">{t('invest_cash_new_balance')}</label>
        <input
          type="number"
          class="w-full rounded border bg-background px-3 py-1.5 text-sm"
          step="0.01"
          bind:value={cashBalance}
        />
      </div>
      <div class="mb-3">
        <label class="mb-1 block text-sm">{t('invest_cash_reason')}</label>
        <textarea
          class="w-full rounded border bg-background px-3 py-1.5 text-sm"
          rows="2"
          bind:value={cashReason}
        ></textarea>
      </div>
    {/if}

    <div class="flex justify-end gap-2">
      <button
        class="rounded px-4 py-1.5 text-sm hover:bg-muted"
        onclick={onClose}
      >Cancel</button>
      <button
        class="rounded bg-primary px-4 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
        disabled={loading || (mode !== 'cash' && (!symbol || quantity <= 0 || price <= 0))}
        onclick={handleSubmit}
      >
        {loading ? '...' : mode === 'buy' ? t('invest_confirm_buy') : mode === 'sell' ? t('invest_confirm_sell') : t('invest_strategy_save')}
      </button>
    </div>
  </div>
</div>
```

- [ ] **Step 4: Update invest/+page.svelte to wire Dashboard tab**

Replace the `src/routes/invest/+page.svelte` file. The dashboard tab renders KPI cards + HoldingsTable + buy/sell/cash dialogs + PnL chart placeholder. Other tabs delegate to their components.

```svelte
<!-- src/routes/invest/+page.svelte -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { invoke } from '$lib/transport';
  import KpiCard from '$lib/components/invest/KpiCard.svelte';
  import HoldingsTable from '$lib/components/invest/HoldingsTable.svelte';
  import TradeDialog from '$lib/components/invest/TradeDialog.svelte';
  import TradeLogTab from '$lib/components/invest/TradeLogTab.svelte';
  import StrategyTab from '$lib/components/invest/StrategyTab.svelte';
  import PnlChart from '$lib/components/invest/PnlChart.svelte';
  import type { Holding } from '$lib/types/invest';

  type InvestTab = 'dashboard' | 'committee' | 'strategy' | 'trades' | 'events' | 'scheduler';
  let activeTab: InvestTab = $state('dashboard');

  const tabs: { id: InvestTab; label: string }[] = $derived([
    { id: 'dashboard', label: t('invest_tab_dashboard') },
    { id: 'committee', label: t('invest_tab_committee') },
    { id: 'strategy', label: t('invest_strategy') },
    { id: 'trades', label: t('invest_trade_log') },
    { id: 'events', label: t('invest_tab_events') },
    { id: 'scheduler', label: t('invest_tab_scheduler') },
  ]);

  let tushareToken = $state<string>('');
  let dialogMode = $state<'buy' | 'sell' | 'cash' | null>(null);
  let dialogPrefill = $state<{ symbol?: string; name?: string; holding?: Holding } | undefined>();
  let refreshInterval = $state<ReturnType<typeof setInterval> | null>(null);

  onMount(async () => {
    // Load settings for token
    try {
      const settings = await invoke<{ tushareToken?: string }>('get_user_settings');
      tushareToken = settings.tushareToken ?? '';
    } catch {}

    await investStore.loadAll();

    // Check for legacy data
    try {
      const result = await invoke<string>('migrate_legacy_portfolio');
      if (result !== 'no_legacy') {
        // Show toast or notification
        console.log('[invest] legacy migration:', result);
      }
    } catch {}

    // Start price refresh if we have a token and holdings
    if (tushareToken) {
      investStore.refreshPrices(tushareToken);
      refreshInterval = setInterval(() => {
        investStore.refreshPrices(tushareToken);
      }, 60_000);
    }

    return () => {
      if (refreshInterval) clearInterval(refreshInterval);
    };
  });

  function openBuy() {
    dialogMode = 'buy';
    dialogPrefill = undefined;
  }
  function openSell(h: Holding) {
    dialogMode = 'sell';
    dialogPrefill = { symbol: h.symbol, name: h.name ?? undefined, holding: h };
  }
  function openCash() {
    dialogMode = 'cash';
    dialogPrefill = undefined;
  }
  function openConvert(h: Holding) {
    dialogMode = 'buy';
    dialogPrefill = { symbol: h.symbol, name: h.name ?? undefined };
  }
  function closeDialog() {
    dialogMode = null;
    dialogPrefill = undefined;
  }
</script>

<div class="flex h-full flex-col">
  <div class="border-b border-border px-4 pt-3">
    <h1 class="mb-3 text-lg font-semibold">{t('nav_invest')}</h1>
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
    {#if activeTab === 'dashboard'}
      {#if !tushareToken}
        <div class="mb-4 rounded-lg border border-dashed p-4 text-center text-sm text-muted-foreground">
          {t('invest_no_token')}
        </div>
      {/if}

      <!-- KPI Cards -->
      <div class="mb-6 grid grid-cols-2 gap-3 sm:grid-cols-5">
        <KpiCard
          label={t('invest_total_assets')}
          value={'¥' + investStore.totalAssets.toLocaleString(undefined, { minimumFractionDigits: 2 })}
        />
        <KpiCard
          label={t('invest_holdings_value')}
          value={'¥' + investStore.holdingsMarketValue.toLocaleString(undefined, { minimumFractionDigits: 2 })}
        />
        <KpiCard
          label={t('invest_cash')}
          value={'¥' + investStore.cash.toLocaleString(undefined, { minimumFractionDigits: 2 })}
          sub="✎"
        />
        <KpiCard
          label={t('invest_total_return')}
          value={investStore.totalReturnPct.toFixed(2) + '%'}
          trend={investStore.totalReturnPct >= 0 ? 'up' : 'down'}
        />
        <KpiCard
          label={t('invest_position_count')}
          value={t('invest_hold') + ' ' + investStore.holdCount + ' + ' + t('invest_watch') + ' ' + investStore.watchCount}
        />
      </div>

      <!-- Action buttons -->
      <div class="mb-4 flex gap-2">
        <button class="rounded bg-primary px-4 py-1.5 text-sm text-primary-foreground" onclick={openBuy}>
          {t('invest_buy')}
        </button>
        <button class="rounded bg-muted px-4 py-1.5 text-sm" onclick={openCash}>
          {t('invest_edit_cash')}
        </button>
        <button class="rounded bg-muted px-4 py-1.5 text-sm" onclick={() => investStore.refreshPrices(tushareToken)}>
          {t('invest_refresh_prices')}
        </button>
      </div>

      <!-- Holdings Table -->
      <HoldingsTable
        onSell={openSell}
        onConvert={openConvert}
        {tushareToken}
      />

      <!-- PnL Chart -->
      <div class="mt-6">
        <PnlChart />
      </div>

    {:else if activeTab === 'trades'}
      <TradeLogTab />

    {:else if activeTab === 'strategy'}
      <StrategyTab {tushareToken} />

    {:else if activeTab === 'committee'}
      <div class="text-muted-foreground">Committee — coming in Phase 3</div>
    {:else if activeTab === 'events'}
      <div class="text-muted-foreground">Event Monitor — coming in Phase 3c</div>
    {:else if activeTab === 'scheduler'}
      <div class="text-muted-foreground">Scheduled Tasks — coming in Phase 4</div>
    {/if}
  </div>
</div>

{#if dialogMode}
  <TradeDialog
    mode={dialogMode}
    prefill={dialogPrefill}
    {tushareToken}
    onClose={closeDialog}
  />
{/if}
```

- [ ] **Step 5: Verify build**

```bash
npx svelte-kit sync && npm run check
```

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/invest/ src/routes/invest/+page.svelte
git commit -m "feat(invest): add Dashboard UI with KPI cards, holdings table, trade dialogs"
```

---

## Task 12: TradeLogTab + StrategyTab + PnlChart Components

**Files:**
- Create: `src/lib/components/invest/TradeLogTab.svelte`
- Create: `src/lib/components/invest/StrategyTab.svelte`
- Create: `src/lib/components/invest/PnlChart.svelte`

- [ ] **Step 1: Create TradeLogTab**

```svelte
<!-- src/lib/components/invest/TradeLogTab.svelte -->
<script lang="ts">
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { t } from '$lib/i18n/index.svelte';

  let filterSymbol = $state('');
  let filterAction = $state('');

  const filteredTrades = $derived(
    investStore.trades.filter((tr) => {
      if (filterSymbol && !tr.symbol.includes(filterSymbol.toUpperCase())) return false;
      if (filterAction && tr.action !== filterAction) return false;
      return true;
    })
  );

  function actionLabel(action: string): string {
    const map: Record<string, string> = {
      buy: '买入',
      sell: '卖出',
      convert_watch_to_hold: '转持仓',
      convert_hold_to_watch: '转观察',
      cost_edit: '成本编辑',
      cash_adjust: '现金调整',
    };
    return map[action] ?? action;
  }

  function exportCsv() {
    const header = 'Date,Stock,Direction,Shares,Price,Amount,Notes\n';
    const rows = filteredTrades.map((tr) =>
      [tr.createdAt.split('T')[0], tr.symbol, actionLabel(tr.action), tr.shares ?? '', tr.price ?? '', tr.amount ?? '', tr.notes ?? ''].join(',')
    ).join('\n');
    const blob = new Blob([header + rows], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'trades.csv';
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<div class="space-y-4">
  <!-- Filters -->
  <div class="flex flex-wrap items-end gap-3">
    <div>
      <label class="mb-1 block text-xs text-muted-foreground">{t('invest_trade_stock')}</label>
      <input
        class="w-40 rounded border bg-background px-3 py-1.5 text-sm"
        placeholder="600519"
        bind:value={filterSymbol}
      />
    </div>
    <div>
      <label class="mb-1 block text-xs text-muted-foreground">{t('invest_trade_direction')}</label>
      <select class="rounded border bg-background px-3 py-1.5 text-sm" bind:value={filterAction}>
        <option value="">全部</option>
        <option value="buy">买入</option>
        <option value="sell">卖出</option>
        <option value="cash_adjust">现金调整</option>
        <option value="convert_watch_to_hold">转持仓</option>
        <option value="cost_edit">成本编辑</option>
      </select>
    </div>
    <button class="rounded bg-muted px-3 py-1.5 text-sm" onclick={exportCsv}>导出 CSV</button>
  </div>

  <!-- Table -->
  {#if filteredTrades.length === 0}
    <p class="py-8 text-center text-sm text-muted-foreground">暂无交易记录</p>
  {:else}
    <div class="overflow-x-auto">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b text-left text-xs text-muted-foreground">
            <th class="pb-2 pr-4">{t('invest_trade_date')}</th>
            <th class="pb-2 pr-4">{t('invest_trade_stock')}</th>
            <th class="pb-2 pr-4">{t('invest_trade_direction')}</th>
            <th class="pb-2 pr-4">{t('invest_quantity')}</th>
            <th class="pb-2 pr-4">{t('invest_price')}</th>
            <th class="pb-2 pr-4">{t('invest_trade_amount')}</th>
            <th class="pb-2">{t('invest_trade_notes')}</th>
          </tr>
        </thead>
        <tbody>
          {#each filteredTrades as tr}
            <tr class="border-b border-border/50">
              <td class="py-2 pr-4 text-xs">{tr.createdAt.split('T')[0]}</td>
              <td class="py-2 pr-4">{tr.symbol}</td>
              <td class="py-2 pr-4">
                <span class="rounded px-1.5 py-0.5 text-xs
                  {tr.action === 'buy' ? 'bg-green-100 text-green-700' :
                   tr.action === 'sell' ? 'bg-red-100 text-red-700' :
                   'bg-gray-100 text-gray-700'}">
                  {actionLabel(tr.action)}
                </span>
              </td>
              <td class="py-2 pr-4 tabular-nums">{tr.shares?.toFixed(0) ?? '-'}</td>
              <td class="py-2 pr-4 tabular-nums">{tr.price?.toFixed(2) ?? '-'}</td>
              <td class="py-2 pr-4 tabular-nums">{tr.amount?.toFixed(2) ?? '-'}</td>
              <td class="py-2 text-xs text-muted-foreground">{tr.notes ?? ''}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>
```

- [ ] **Step 2: Create StrategyTab**

```svelte
<!-- src/lib/components/invest/StrategyTab.svelte -->
<script lang="ts">
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { t } from '$lib/i18n/index.svelte';

  let { tushareToken }: { tushareToken: string } = $props();

  let strategy = $derived(investStore.strategies[0] ?? null);
  let targets = $state<Array<{ symbol: string; name: string; targetPct: number }>>([]);
  let maxSinglePct = $state<number | null>(null);
  let minCashPct = $state<number | null>(null);
  let searchQuery = $state('');
  let searchResults = $state<Array<{ tsCode: string; name: string }>>([]);
  let saving = $state(false);

  // Load existing strategy into editable state
  $effect(() => {
    if (strategy) {
      try {
        targets = JSON.parse(strategy.targets);
      } catch { targets = []; }
      maxSinglePct = strategy.maxSinglePct;
      minCashPct = strategy.minCashPct;
    }
  });

  async function doSearch() {
    if (!searchQuery || !tushareToken) return;
    try {
      searchResults = await investStore.searchStocks(searchQuery, tushareToken);
    } catch {}
  }

  function addTarget(tsCode: string, name: string) {
    if (targets.find((t) => t.symbol === tsCode)) return;
    targets = [...targets, { symbol: tsCode, name, targetPct: 0 }];
    searchResults = [];
    searchQuery = '';
  }

  function removeTarget(idx: number) {
    targets = targets.filter((_, i) => i !== idx);
  }

  async function save() {
    saving = true;
    try {
      await investStore.saveStrategy(
        strategy?.id ?? null,
        'default',
        targets,
        maxSinglePct,
        minCashPct
      );
    } finally {
      saving = false;
    }
  }
</script>

<div class="space-y-6">
  <div>
    <h3 class="mb-3 text-sm font-medium">{t('invest_strategy_targets')}</h3>

    <!-- Search -->
    <div class="mb-3 flex gap-2">
      <input
        class="flex-1 rounded border bg-background px-3 py-1.5 text-sm"
        placeholder={t('invest_stock_search')}
        bind:value={searchQuery}
      />
      <button class="rounded bg-muted px-3 py-1.5 text-sm" onclick={doSearch}>搜索</button>
    </div>
    {#if searchResults.length > 0}
      <div class="mb-3 max-h-32 overflow-auto rounded border">
        {#each searchResults as s}
          <button
            class="w-full px-3 py-1.5 text-left text-sm hover:bg-muted"
            onclick={() => addTarget(s.tsCode, s.name)}
          >
            {s.name} ({s.tsCode})
          </button>
        {/each}
      </div>
    {/if}

    <!-- Targets list -->
    {#if targets.length === 0}
      <p class="py-4 text-center text-sm text-muted-foreground">{t('invest_strategy_empty')}</p>
    {:else}
      <div class="space-y-2">
        {#each targets as target, i}
          <div class="flex items-center gap-3 rounded border p-2">
            <span class="flex-1 text-sm font-medium">{target.name} ({target.symbol})</span>
            <input
              type="number"
              class="w-20 rounded border bg-background px-2 py-1 text-sm text-right"
              step="1"
              min="0"
              max="100"
              bind:value={target.targetPct}
            />
            <span class="text-sm text-muted-foreground">%</span>
            <button
              class="rounded px-2 py-1 text-xs text-destructive hover:bg-destructive/10"
              onclick={() => removeTarget(i)}
            >✕</button>
          </div>
        {/each}
      </div>
    {/if}

    <button
      class="mt-3 rounded bg-muted px-3 py-1.5 text-sm"
      onclick={() => { targets = [...targets, { symbol: '', name: '', targetPct: 0 }]; }}
    >{t('invest_strategy_add_target')}</button>
  </div>

  <!-- Constraints -->
  <div class="grid grid-cols-2 gap-4">
    <div>
      <label class="mb-1 block text-sm">{t('invest_strategy_max_single')}</label>
      <input
        type="number"
        class="w-full rounded border bg-background px-3 py-1.5 text-sm"
        step="1"
        min="0"
        max="100"
        bind:value={maxSinglePct}
      />
    </div>
    <div>
      <label class="mb-1 block text-sm">{t('invest_strategy_min_cash')}</label>
      <input
        type="number"
        class="w-full rounded border bg-background px-3 py-1.5 text-sm"
        step="1"
        min="0"
        max="100"
        bind:value={minCashPct}
      />
    </div>
  </div>

  <button
    class="rounded bg-primary px-6 py-2 text-sm text-primary-foreground disabled:opacity-50"
    disabled={saving}
    onclick={save}
  >
    {saving ? '...' : t('invest_strategy_save')}
  </button>
</div>
```

- [ ] **Step 3: Create PnlChart (Chart.js)**

First install Chart.js:

```bash
npm install chart.js
```

Then create the component:

```svelte
<!-- src/lib/components/invest/PnlChart.svelte -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { Chart, LineController, LineElement, PointElement, LinearScale, CategoryScale, Tooltip, Legend } from 'chart.js';

  Chart.register(LineController, LineElement, PointElement, LinearScale, CategoryScale, Tooltip, Legend);

  let canvas: HTMLCanvasElement;
  let chart: Chart | null = null;

  function buildChart() {
    if (!canvas) return;
    const snapshots = [...investStore.pnlSnapshots].reverse();
    if (snapshots.length === 0) return;

    const labels = snapshots.map((s) => s.snapshotDate);
    const totalPnlPct = snapshots.map((s) => s.dailyPnlPct ?? 0);

    // Cumulative PnL from first snapshot
    const firstTotal = snapshots[0]?.totalValue ?? 1;
    const cumulative = snapshots.map((s) => ((s.totalValue - firstTotal) / firstTotal) * 100);

    if (chart) chart.destroy();

    chart = new Chart(canvas, {
      type: 'line',
      data: {
        labels,
        datasets: [
          {
            label: '总资产 PnL%',
            data: cumulative,
            borderColor: 'rgb(59, 130, 246)',
            backgroundColor: 'rgba(59, 130, 246, 0.1)',
            fill: true,
            tension: 0.3,
            pointRadius: 2,
          },
        ],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          y: {
            ticks: {
              callback: (v) => v + '%',
            },
          },
        },
        plugins: {
          tooltip: {
            callbacks: {
              label: (ctx) => ctx.parsed.y.toFixed(2) + '%',
            },
          },
        },
      },
    });
  }

  $effect(() => {
    // Rebuild chart when snapshots change
    investStore.pnlSnapshots;
    if (canvas) buildChart();
  });

  onMount(() => {
    buildChart();
    return () => { chart?.destroy(); };
  });
</script>

<div>
  <h3 class="mb-3 text-sm font-medium">{t('invest_pnl_chart')}</h3>
  {#if investStore.pnlSnapshots.length === 0}
    <p class="py-8 text-center text-sm text-muted-foreground">{t('invest_no_pnl')}</p>
  {:else}
    <div class="h-64">
      <canvas bind:this={canvas}></canvas>
    </div>
  {/if}
</div>
```

- [ ] **Step 4: Verify build**

```bash
npx svelte-kit sync && npm run check && npm run build
```

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/invest/TradeLogTab.svelte src/lib/components/invest/StrategyTab.svelte src/lib/components/invest/PnlChart.svelte package.json package-lock.json
git commit -m "feat(invest): add TradeLog, Strategy, PnL chart components"
```

---

## Task 13: Final Verification

- [ ] **Step 1: Run full verification suite**

```bash
npm run lint
npm run check
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
npm run i18n:check
```

- [ ] **Step 2: Fix any issues found**

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "chore(invest): Phase 2 verification fixes"
```

---

## Post-Implementation Code Review (2026-05-29)

9 finder angles × 8 candidates, 1-vote verify, sweep. 13 confirmed, 1 refuted.

### Critical (4)

| # | File | Finding | Fix |
|---|------|---------|-----|
| 1 | `storage/settings.rs:544` | `update_user_settings` never persists `tushare_token` — all Tushare ops silently fail | Added `apply_optional_string_empty_as_none` call |
| 2 | `lib.rs:95` | `Handle::block_on()` inside tokio async context panics on every PnL cron tick | Made `run_pnl_snapshot` async, use `.await` |
| 3 | `storage/invest/strategy.rs:10` | `targets: String` serializes as quoted JSON string, frontend expects `StrategyTarget[]` | Custom serde deserialize + `parse_targets` helper for SQLite reads |
| 4 | `tushare/tushare-api-reference.py:7` | Hardcoded API token in committed reference file | Replaced with placeholder |

### High (3)

| # | File | Finding | Fix |
|---|------|---------|-----|
| 5 | `tushare/client.rs:23,39` | DailyBar/StockBasic missing `#[serde(rename_all = "camelCase")]` — frontend gets snake_case | Added serde rename attribute |
| 6 | `invest-store.svelte.ts:221` | sellStock no oversell validation — credits phantom cash | Added `qty > currentShares` guard |
| 7 | `routes/invest/+page.svelte:66` | openConvert creates duplicate holdings (watch + hold) | New `convert` dialog mode calling `convertWatchToHold` |

### Medium (4)

| # | File | Finding | Fix |
|---|------|---------|-----|
| 8 | `storage/invest/verdicts.rs:90` | PnL snapshot no dedup — duplicates on app restart | Upsert on `snapshot_date` |
| 9 | `invest-store.svelte.ts:128` | refreshPrices replaces entire priceMap — cached prices lost on failure | Merge instead of replace |
| 10 | `PnlChart.svelte:55,61` | Double `buildChart()` call ($effect + onMount) | Removed redundant onMount |
| 11 | `HoldingsTable.svelte:18` | `!price` treats 0 as null | Explicit `== null` check |

### Refuted (1)

| # | File | Finding | Reason |
|---|------|---------|--------|
| 12 | `storage/invest/mod.rs:29` | Trades migration fragile (DROP between rename) | Wrapped in SQLite transaction — atomic |

**Commits:** `a1a41f6` (fixes), `3acd188` (docs)
