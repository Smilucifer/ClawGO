# Phase 4a Design: Scheduler + Verdict Review + Dreaming + FTS5 + Archived View

> Status: review
> Created: 2026-05-30
> Scope: openInvest Phase 4a — first sub-phase of Phase 4

## Overview

Phase 4a delivers 7 interconnected features for the openInvest system:

1. **Scheduler framework** — configurable cron jobs replacing hardcoded background loops
2. **Verdict Review** — retrospective accuracy pipeline (1d/7d/30d hit rates)
3. **Dreaming invest pipeline** — 3-stage statistical pipeline (Light→REM→Deep) writing to domain_insights
4. **Dreaming snapshots + rollback** — dual-path independent snapshots with rollback safety
5. **Dream Trace audit** — trigger/status/change records for each Dream run
6. **Risk Officer FTS5** — upgrade domain_insights from LIKE to full-text search
7. **Archived memory view** — `/memory-mgmt` new tab for archived user memories

### Dependencies

```
Scheduler framework ← Verdict Review cron job
                   ← Dreaming invest cron job
                   ← Dreaming user memory cron job

Verdict Review → uses verdicts table + Tushare daily prices
Dreaming       → uses verdicts + trades + events + prices
FTS5           → domain_insights table must exist (already does)
Archived View  → memories table (already exists)
```

### Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Cron parsing | `cron` crate | Mature, 5+7 field support, no heavy deps |
| Snapshot storage | SQLite table in invest.db | Transactional safety, single source of truth |
| Dreaming config UI | Path A in /memory-mgmt, Path B in SchedulerTab | Each path lives where its data is |
| Review trigger | Manual + cron (17:00 weekdays) | Scheduler in 4a enables both |
| Dream input | Verdict + price + event correlation | Full retrospective, not just metadata |

---

## 1. Scheduler Framework

### 1.1 Architecture

```
src-tauri/src/invest/scheduler/
├── mod.rs          — CronJob struct + re-exports
├── config.rs       — ~/.claw-go/invest/scheduler.json CRUD
└── runner.rs       — tokio::spawn scheduling loop
```

### 1.2 CronJob Struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJob {
    pub id: String,                    // "pnl_snapshot", "verdict_review", etc.
    pub name: String,                  // Display name
    pub cron_expr: String,             // "30 9,11 * * 1-5" (ignored when interval_min is set)
    pub interval_min: Option<i64>,     // For interval-based jobs (e.g. dream_user: 120)
    pub enabled: bool,
    pub requires_trading_day: bool,    // Skip if not trading day
    pub last_run: Option<String>,      // ISO timestamp
    pub next_run: Option<String>,      // ISO timestamp
    pub last_status: Option<String>,   // "ok" | "error" | "skipped"
    pub description: String,           // Human-readable purpose
}
```

### 1.3 Default Jobs

| ID | Name | Cron | Trading Day | Default |
|----|------|------|-------------|---------|
| `pnl_snapshot` | PnL 快照 | `30 9,11 * * 1-5` + `0 13,15 * * 1-5` | yes | enabled |
| `verdict_review` | Verdict Review | `0 17 * * 1-5` | yes | enabled |
| `event_scan` | Event Watch 扫描 | `*/30 8-22 * * 1-5` | no | enabled |
| `dream_user` | Dreaming (用户记忆) | interval 120min (see 3.5) | no | disabled |
| `dream_invest` | Dreaming (投资) | `0 3 * * *` | yes | disabled |

### 1.4 Runner Logic

```
tokio::spawn loop (every 60 seconds):
  1. Load scheduler.json
  2. For each enabled job:
     a. If job has `interval_min` field (e.g. dream_user):
        - next_fire_time = last_run + interval_min minutes
     b. Else (standard cron):
        - Parse cron_expr with `cron` crate
        - Compute next_fire_time from last_run
     c. If now >= next_fire_time:
        - If requires_trading_day && !is_trading_day(today): skip, log "skipped"
        - Else: execute job function, log result
  3. Sleep until next minute boundary
```

Note: `dream_user` uses interval-based scheduling (not cron) because "every N minutes" doesn't map cleanly to standard cron. The `CronJob` struct has an optional `interval_min` field for this case. Standard cron jobs leave it `None`.

### 1.5 Config File

`~/.claw-go/invest/scheduler.json`:
```json
{
  "jobs": [
    {
      "id": "pnl_snapshot",
      "cronExpr": "30 9,11 * * 1-5",
      "enabled": true,
      "requiresTradingDay": true
    }
  ]
}
```

Only stores overrides from defaults. Missing jobs use built-in defaults.

### 1.6 Tauri Commands

```rust
// src-tauri/src/commands/invest.rs (additions)

#[tauri::command]
pub fn list_cron_jobs() -> Result<Vec<CronJob>, String>

#[tauri::command]
pub fn toggle_cron_job(id: String, enabled: bool) -> Result<(), String>

#[tauri::command]
pub fn update_cron_schedule(id: String, cron_expr: String) -> Result<(), String>

#[tauri::command]
pub async fn trigger_cron_job(id: String) -> Result<(), String>  // immediate execution, async

#[tauri::command]
pub fn get_cron_job_logs(id: String, limit: Option<i64>) -> Result<Vec<SchedulerLog>, String>
```

### 1.7 SchedulerTab UI

```
SchedulerTab
├── Job list (table)
│   ├── Columns: Name | Cron (human-readable) | Status toggle | Last run | Next run | Actions
│   ├── Actions: Run now | View logs | Edit schedule
│   └── Status indicator: running (spinner) / idle / error (red) / skipped (gray)
│
└── Log panel (expandable per job)
    └── Recent N runs: timestamp | status | duration | message
```

---

## 2. Verdict Review (Accuracy Pipeline)

### 2.1 Architecture

```
src-tauri/src/invest/verdict_review.rs  — core pipeline
```

### 2.2 Pipeline Logic

```
run_verdict_review():
  1. Load all verdicts from invest.db (ordered by created_at)
  2. For each verdict:
     a. Get verdict date + symbol + verdict type (BUY/SELL/HOLD/etc.)
     b. Fetch actual prices via Tushare daily: [verdict_date + 1d, +7d, +30d]
     c. Calculate return_pct for each window
     d. Determine hit:
        - BUY/ACCUMULATE: hit = return_pct > 0
        - SELL/TRIM: hit = return_pct < 0
        - HOLD: hit = abs(return_pct) < flat_threshold
        - flat_threshold = K_FLAT × atr% × √(days), cap at 8%
     e. Write to verdict_reviews table
  3. Aggregate summary: overall, by window, by verdict type
  4. Return VerdictReviewSummary
```

### 2.3 ATR-Based HOLD Threshold

```rust
fn flat_threshold(atr_pct: f64, days: i64) -> f64 {
    const K_FLAT: f64 = 1.0;
    (K_FLAT * atr_pct * (days as f64).sqrt()).min(0.08)
}
```

ATR% comes from Tushare daily data: `ATR14 / close_price`.

### 2.4 Directional Hit Rate

The "honest" metric excludes HOLD verdicts:

```
directional_hit_rate = hits(BUY/ACCUMULATE/SELL/TRIM) / total(BUY/ACCUMULATE/SELL/TRIM)
```

When directional_hit_rate < 50%, the UI shows an explanatory banner.

### 2.5 Storage Schema

New table in invest.db:

```sql
CREATE TABLE IF NOT EXISTS verdict_reviews (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    verdict_id      TEXT NOT NULL,
    symbol          TEXT NOT NULL,
    verdict_type    TEXT NOT NULL,        -- BUY/ACCUMULATE/HOLD/TRIM/SELL
    verdict_date    TEXT NOT NULL,
    window_days     INTEGER NOT NULL,     -- 1, 7, or 30
    price_at_verdict REAL,
    price_after     REAL,
    return_pct      REAL,
    hit             INTEGER NOT NULL,     -- 0 or 1
    flat_threshold  REAL,
    created_at      TEXT NOT NULL
);

CREATE INDEX idx_verdict_reviews_verdict ON verdict_reviews(verdict_id);
CREATE INDEX idx_verdict_reviews_symbol ON verdict_reviews(symbol);
CREATE INDEX idx_verdict_reviews_date ON verdict_reviews(verdict_date);
```

### 2.6 Tauri Commands

```rust
#[tauri::command]
pub async fn run_verdict_review(tushare_token: String) -> Result<VerdictReviewSummary, String>

#[tauri::command]
pub fn get_verdict_review_summary() -> Result<VerdictReviewSummary, String>

#[tauri::command]
pub fn get_verdict_review_detail(symbol: Option<String>) -> Result<Vec<VerdictReviewEntry>, String>
```

### 2.7 Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictReviewSummary {
    pub total_verdicts: usize,
    pub overall_hit_rate: f64,          // 30d window
    pub directional_hit_rate: f64,      // excluding HOLD
    pub by_window: Vec<WindowStats>,    // 1d, 7d, 30d
    pub by_verdict: Vec<VerdictStats>,  // per verdict type
    pub last_review_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowStats {
    pub window_days: i64,
    pub sample_count: usize,
    pub hit_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictStats {
    pub verdict_type: String,
    pub sample_count: usize,
    pub avg_confidence: f64,
    pub hit_rate_1d: f64,
    pub hit_rate_7d: f64,
    pub hit_rate_30d: f64,
}
```

### 2.8 AccuracyTab UI

Replace current placeholder `CommitteeAccuracyTab.svelte`:

```
AccuracyTab
├── KPI cards (3)
│   ├── Total verdicts
│   ├── 30d hit rate (overall)
│   └── Directional hit rate (excl. HOLD) — highlighted
│
├── Honesty banner (conditional: directional < 50%)
│   └── "HOLD verdicts naturally have high hit rates. Directional accuracy is the real alpha metric."
│
├── By time window table
│   └── 1d / 7d / 30d: sample count + hit rate + progress bar
│
├── By verdict type table
│   └── BUY / ACCUMULATE / HOLD / TRIM / SELL: count + avg confidence + 1d/7d/30d hit rates
│
├── Manual trigger button
│   └── [Run Review] → calls run_verdict_review
│
└── Detail list (collapsible)
    └── Per-verdict: symbol + date + verdict + return% + hit/miss badge
```

---

## 3. Dreaming Invest Pipeline

### 3.1 Architecture

```
src-tauri/src/invest/dreaming.rs      — 3-stage pipeline
src-tauri/src/storage/invest/dreaming.rs — snapshots + traces
```

### 3.2 Three-Stage Pipeline

**Light Sleep** — Extract tuples:
```
Input: verdicts table (last N days, default 30)
Output: Vec<(symbol, verdict, confidence, date, macro_signal)>
```

**REM Sleep** — Aggregate hit rates:
```
For each unique (symbol, verdict, macro_signal_as_regime):
  1. Fetch actual prices via Tushare daily (same as verdict_review)
  2. Calculate 1d/7d/30d returns
  3. Compute hit_rate per window
  4. Compute score = hit_rate × 0.7 + min(count/10, 1.0) × 0.3
Output: Vec<DreamCandidate> where score >= 0.8 AND count >= 3
```

**Deep Sleep** — Write insights:
```
For each DreamCandidate:
  INSERT OR REPLACE INTO domain_insights (
    id, insight_type, symbol, content, confidence,
    source_verdict_ids, status, created_at, updated_at
  )
```

### 3.3 Insight Content Format

```
"{symbol} {verdict} in {regime}: {hit_rate}% hit rate over {count} samples (1d/7d/30d: {r1}%/{r7}%/{r30}%)"
```

### 3.4 Tauri Commands

```rust
#[tauri::command]
pub async fn trigger_dream(
    mode: String,           // "invest" | "user_memory"
    tushare_token: String,
) -> Result<DreamResult, String>

#[tauri::command]
pub fn get_dream_config() -> Result<DreamConfig, String>

#[tauri::command]
pub fn save_dream_config(config: DreamConfig) -> Result<(), String>

#[tauri::command]
pub fn list_dream_traces(limit: Option<i64>) -> Result<Vec<DreamTrace>, String>

#[tauri::command]
pub fn rollback_dream(snapshot_id: i64) -> Result<(), String>
```

### 3.5 DreamConfig

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DreamConfig {
    pub invest_enabled: bool,          // default false
    pub invest_cron: String,           // "0 3 * * *"
    pub user_memory_enabled: bool,     // default false
    pub user_memory_interval_min: i64, // default 120
    pub lookback_days: i64,            // default 30
    pub min_score: f64,                // default 0.8
    pub min_count: i64,                // default 3
}
```

---

## 4. Dreaming Snapshots + Rollback

### 4.1 Storage Schema

New table in invest.db:

```sql
CREATE TABLE IF NOT EXISTS dream_snapshots (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    dream_type      TEXT NOT NULL,        -- "invest" | "user_memory"
    trigger_type    TEXT NOT NULL,        -- "manual" | "scheduled"
    before_json     TEXT NOT NULL,        -- JSON array of domain_insights rows
    after_json      TEXT,                 -- NULL until dream completes
    status          TEXT NOT NULL,        -- "pending" | "completed" | "rolled_back"
    summary         TEXT,                 -- human-readable change summary
    rollback_ready  INTEGER NOT NULL DEFAULT 0,  -- 1 if current state matches after_json
    created_at      TEXT NOT NULL
);

CREATE INDEX idx_dream_snapshots_type ON dream_snapshots(dream_type);
CREATE INDEX idx_dream_snapshots_status ON dream_snapshots(status);
```

### 4.2 Snapshot Flow

```
Dream execution:
  1. BEGIN IMMEDIATE
  2. SELECT json_group_array(json(id, insight_type, symbol, content, confidence, ...))
     FROM domain_insights WHERE status = 'active' AND dream_type = ?
  3. INSERT INTO dream_snapshots (dream_type, trigger_type, before_json, status, created_at)
  4. COMMIT
  5. Execute pipeline (Light→REM→Deep)
  6. BEGIN IMMEDIATE
  7. SELECT json_group_array(...) FROM domain_insights WHERE status = 'active' AND dream_type = ?
  8. UPDATE dream_snapshots SET after_json = ?, status = 'completed', summary = ?, rollback_ready = 1
  9. COMMIT

Rollback:
  1. Load snapshot by id
  2. Verify: current domain_insights == after_json → proceed; else → rollback_ready = 0, abort
  3. BEGIN IMMEDIATE
  4. DELETE FROM domain_insights WHERE insight_type IN (SELECT insight_type FROM json_each(before_json))
  5. INSERT INTO domain_insights SELECT ... FROM json_each(before_json)
  6. UPDATE dream_snapshots SET status = 'rolled_back', rollback_ready = 0
  7. COMMIT
```

### 4.3 Safety

- If user manually edits domain_insights after dream → `rollback_ready = 0` (detected by comparing current state to after_json)
- Only the most recent completed dream per type is rollback-ready
- Rollback confirmation dialog: "This will restore domain_insights to before the last dream. Continue?"

---

## 5. Dream Trace Audit

### 5.1 Storage

Reuses `dream_snapshots` table — each row IS a trace record. Additional fields:

```
trigger_type: "manual" | "scheduled" | "rollback"
summary: "3 insights added, 2 updated, 1 archived"
```

### 5.2 Trace List UI

In both Dreaming config panels (Path A in /memory-mgmt, Path B in SchedulerTab):

```
Dream Trace list:
  ├── Each row: trigger icon + time + status badge + summary
  ├── Click to expand: step timeline (Light → REM → Deep with durations)
  └── Rollback button (only on most recent completed, rollback_ready=1)
```

---

## 6. FTS5 Upgrade for domain_insights

### 6.1 Migration

Add FTS5 virtual table for domain_insights:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS domain_insights_fts USING fts5(
    content, symbol, insight_type,
    content='domain_insights',
    content_rowid='rowid'
);

-- Triggers for sync
CREATE TRIGGER domain_insights_ai AFTER INSERT ON domain_insights BEGIN
    INSERT INTO domain_insights_fts(rowid, content, symbol, insight_type)
    VALUES (new.rowid, new.content, new.symbol, new.insight_type);
END;

CREATE TRIGGER domain_insights_ad AFTER DELETE ON domain_insights BEGIN
    INSERT INTO domain_insights_fts(domain_insights_fts, rowid, content, symbol, insight_type)
    VALUES ('delete', old.rowid, old.content, old.symbol, old.insight_type);
END;

CREATE TRIGGER domain_insights_au AFTER UPDATE ON domain_insights BEGIN
    INSERT INTO domain_insights_fts(domain_insights_fts, rowid, content, symbol, insight_type)
    VALUES ('delete', old.rowid, old.content, old.symbol, old.insight_type);
    INSERT INTO domain_insights_fts(rowid, content, symbol, insight_type)
    VALUES (new.rowid, new.content, new.symbol, new.insight_type);
END;
```

### 6.2 Query Upgrade

`exec_dreaming_insights()` in `committee/tools.rs` changes from:
```sql
SELECT * FROM domain_insights WHERE content LIKE '%' || ? || '%'
```
To:
```sql
SELECT di.* FROM domain_insights di
JOIN domain_insights_fts fts ON di.rowid = fts.rowid
WHERE domain_insights_fts MATCH ?
ORDER BY rank
```

---

## 7. Archived Memory View

### 7.1 Location

`/memory-mgmt` page — new 3rd tab: "已归档" (Archived)

### 7.2 Data

Query: `SELECT * FROM memories WHERE status = 'archived' ORDER BY updated_at DESC`

With scope filter: [全部] [global] [project] [invest]

### 7.3 UI

```
Archived Tab
├── Scope filter: [全部] [global] [project] [invest]
├── Memory card list
│   ├── Each: scope badge + type badge + content + archived_at + original confidence
│   ├── Action: [Restore] → set status = 'active', confidence = 1.0
│   └── Action: [Delete permanently] → set status = 'deleted'
└── Empty state: "No archived memories"
```

### 7.4 Tauri Command

```rust
#[tauri::command]
pub fn restore_memory(id: String) -> Result<(), String>  // archived → active
```

---

## 8. i18n Keys

New keys for `messages/en.json` and `messages/zh-CN.json`:

```
invest.scheduler.title
invest.scheduler.jobName
invest.scheduler.cronExpr
invest.scheduler.lastRun
invest.scheduler.nextRun
invest.scheduler.runNow
invest.scheduler.viewLogs
invest.scheduler.editSchedule
invest.scheduler.status.ok
invest.scheduler.status.error
invest.scheduler.status.skipped
invest.scheduler.status.running

invest.accuracy.title
invest.accuracy.totalVerdicts
invest.accuracy.overallHitRate
invest.accuracy.directionalHitRate
invest.accuracy.honestyBanner
invest.accuracy.byWindow
invest.accuracy.byVerdict
invest.accuracy.runReview
invest.accuracy.noData

invest.dreaming.title
invest.dreaming.investPath
invest.dreaming.userMemoryPath
invest.dreaming.trigger
invest.dreaming.rollback
invest.dreaming.config
invest.dreaming.trace
invest.dreaming.score
invest.dreaming.count

memory_mgmt.archived.title
memory_mgmt.archived.restore
memory_mgmt.archived.deletePermanent
memory_mgmt.archived.empty
```

---

## 9. Implementation Order

```
Step 1: Scheduler framework (backend + config + runner)
Step 2: Scheduler Tauri commands + SchedulerTab UI
Step 3: Verdict Review pipeline (backend)
Step 4: Verdict Review Tauri commands + AccuracyTab UI
Step 5: Dreaming pipeline (backend, 3 stages)
Step 6: Dreaming snapshots + rollback (storage)
Step 7: Dreaming Tauri commands + config UI (both paths)
Step 8: FTS5 upgrade for domain_insights
Step 9: Archived memory view (/memory-mgmt new tab)
Step 10: i18n + integration testing
```

Steps 1-2 can be tested independently. Steps 3-4 depend on Step 1 (for cron scheduling). Steps 5-7 depend on Steps 3-4 (Dreaming uses similar price-fetching logic). Steps 8-9 are independent.
