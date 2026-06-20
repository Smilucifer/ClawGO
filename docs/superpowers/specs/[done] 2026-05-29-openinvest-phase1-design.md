# openInvest Phase 1 Design Spec

**Date:** 2026-05-29
**Scope:** Data layer + sidebar/route refactor + memory scope migration
**Status:** Approved

## Overview

Phase 1 is the foundation for porting the openInvest investment committee system into ClawGO. It establishes the data layer (invest.db), refactors the memory system to support 3 scopes (global/project/invest), adds new sidebar entries and route skeletons, and removes the character memory UI.

## Modules

### Module A: Memory Scope Refactor

**Goal:** Add `scope` and `project_id` fields to `memories` table, enabling 3-scope isolation (global/project/invest).

**Schema change:**
```sql
ALTER TABLE memories ADD COLUMN scope TEXT NOT NULL DEFAULT 'global';
ALTER TABLE memories ADD COLUMN project_id TEXT;
```

**Migration:** Application startup auto-migration. All existing rows get `scope='global'`.

**Storage changes (`storage/memos.rs`):**
- `save_memo()` accepts optional `scope` and `project_id` params
- `list_memos()` filters by scope
- `get_memos_by_project()` new function for project-scoped queries

**Command changes (`commands/memos.rs`):**
- `list_memos(scope: Option<String>)` — filter by scope
- `save_memo(content, scope, project_id)` — accept scope params

**UI removal:**
- Settings page: remove "Memory" tab (character memory config)
- AiCharacter detail: remove memory injection UI

### Module B: invest.db Data Layer

**Goal:** Create independent invest.db with full schema for portfolio management, verdicts, events, and scheduler.

**Database location:** `~/.claw-go/invest/invest.db`

**Schema (10 tables):**

```sql
-- Portfolio
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

-- Committee
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

-- Events
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

-- Domain insights (Dreaming Path B)
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

-- Scheduler
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
```

**Storage modules:**

| Module | Path | Responsibility |
|--------|------|----------------|
| `mod.rs` | `storage/invest/mod.rs` | DB init, WAL mode, migration |
| `portfolio.rs` | `storage/invest/portfolio.rs` | holdings, trades, cash CRUD |
| `verdicts.rs` | `storage/invest/verdicts.rs` | verdicts, pnl_snapshots |
| `events.rs` | `storage/invest/events.rs` | events, event_sources |
| `scheduler.rs` | `storage/invest/scheduler.rs` | scheduler_logs, trade_calendar |

**Tauri commands (`commands/invest.rs`):**
- `get_holdings()` / `add_holding()` / `update_holding()` / `delete_holding()`
- `record_trade()` — atomic transaction with holdings update
- `get_cash()` / `update_cash()`
- `get_verdicts(symbol, limit)` / `save_verdict()`
- `get_pnl_snapshots(limit)` / `save_pnl_snapshot()`
- `get_events(source, limit)` / `save_event()` / `mark_event_triggered()`
- `is_trading_day(date)` — check trade_calendar

**Initialization:**
- `invest_db()` singleton in `storage/invest/mod.rs` using `std::sync::OnceLock`
- Auto-create tables on first access (no separate migration step)
- WAL mode enabled at init

### Module C: Sidebar + Routes + UI

**Sidebar changes (`+layout.svelte`):**
- Add `/invest` entry between `/plugins` and `/memory`: icon `trending-up`, label from i18n
- Add `/memory-mgmt` entry between `/settings` and `/history`: icon `database`, label from i18n
- Remove `/memory` entry (if present)

**Route `/invest/+page.svelte`:**
- 6 tabs: Dashboard / 委员会 / 策略 / 交易记录 / 事件监控 / 定时任务
- Each tab shows placeholder content for now
- Dashboard tab: placeholder KPI cards + holdings table skeleton

**Route `/memory-mgmt/+page.svelte`:**
- 2 tabs: 用户记忆 / 提取配置
- 用户记忆 tab: scope filter (global/project/invest) + memory list
- 提取配置 tab: extraction config form + "应用并重载" button

**Title bar `[···]` dropdown:**
- New `MoreMenu` component in top bar
- Entries: Doctor 诊断, 版本更新日志, 全部设置

**i18n updates:**
- `messages/en.json`: new keys for invest, memory-mgmt, more menu
- `messages/zh-CN.json`: corresponding Chinese translations

## File Change Summary

| Action | File | Description |
|--------|------|-------------|
| Create | `src-tauri/src/storage/invest/mod.rs` | invest.db init + migration |
| Create | `src-tauri/src/storage/invest/portfolio.rs` | holdings/trades/cash |
| Create | `src-tauri/src/storage/invest/verdicts.rs` | verdicts + pnl |
| Create | `src-tauri/src/storage/invest/events.rs` | events + sources |
| Create | `src-tauri/src/storage/invest/scheduler.rs` | scheduler + calendar |
| Create | `src-tauri/src/commands/invest.rs` | Tauri commands for invest |
| Modify | `src-tauri/src/commands/mod.rs` | register invest commands |
| Modify | `src-tauri/src/storage/memos.rs` | add scope/project_id |
| Modify | `src-tauri/src/commands/memos.rs` | scope-aware CRUD |
| Create | `src/routes/invest/+page.svelte` | invest page skeleton |
| Create | `src/routes/memory-mgmt/+page.svelte` | memory mgmt page |
| Modify | `src/routes/+layout.svelte` | sidebar + more menu |
| Create | `src/lib/components/MoreMenu.svelte` | title bar dropdown |
| Modify | `messages/en.json` | i18n keys |
| Modify | `messages/zh-CN.json` | i18n keys |

## Dependencies

- No new npm/cargo dependencies for Phase 1
- Uses existing `rusqlite` with `bundled` feature
- Uses existing `uuid` crate for ID generation

## Risks

| Risk | Mitigation |
|------|------------|
| Memory scope migration breaks existing queries | Default `scope='global'` preserves all existing behavior |
| invest.db init fails on first run | Graceful fallback + error toast |
| Sidebar layout breaks with new entries | Test responsive layout at multiple widths |

## Out of Scope (Phase 2+)

- Dashboard KPI logic and PnL chart rendering
- Holdings table with real data binding
- Buy/sell/watch dialog implementations
- Committee orchestrator
- Event monitoring logic
- Dreaming integration
