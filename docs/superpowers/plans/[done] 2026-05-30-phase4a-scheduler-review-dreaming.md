# Phase 4a: Scheduler + Verdict Review + Dreaming + FTS5 + Archived View

> **Status:** All 10 tasks completed (2026-05-30). See individual task status below.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace hardcoded background loops with a configurable Scheduler, add Verdict Review accuracy pipeline, implement Dreaming invest pipeline with snapshots/rollback, upgrade domain_insights to FTS5, and add archived memory view.

**Architecture:** Storage layer (`storage/invest/`) owns schema + CRUD. Pipeline logic (`invest/verdict_review.rs`, `invest/dreaming.rs`) owns business logic. Tauri commands (`commands/invest.rs`) bridge frontend. Frontend components + stores render UI. The Scheduler runner replaces 3 inline `tokio::spawn` loops in `lib.rs`.

**Tech Stack:** Rust (rusqlite, serde, chrono, cron crate), Svelte 5 runes, Chart.js, Tauri IPC, SQLite FTS5

---

## File Structure

### New Files

| File | Responsibility |
|------|---------------|
| `src-tauri/src/invest/scheduler/mod.rs` | CronJob struct, DEFAULT_JOBS, re-exports |
| `src-tauri/src/invest/scheduler/config.rs` | scheduler.json read/write, merge with defaults |
| `src-tauri/src/invest/scheduler/runner.rs` | tokio scheduling loop, job dispatch |
| `src-tauri/src/invest/verdict_review.rs` | run_verdict_review pipeline, ATR threshold, aggregation |
| `src-tauri/src/invest/dreaming/mod.rs` | DreamConfig, DreamResult, DreamTrace types |
| `src-tauri/src/invest/dreaming/pipeline.rs` | 3-stage pipeline: Light→REM→Deep |
| `src-tauri/src/invest/dreaming/snapshot.rs` | dream_snapshots CRUD, rollback logic |
| `src-tauri/src/storage/invest/verdict_reviews.rs` | verdict_reviews table CRUD |
| `src-tauri/src/storage/invest/domain_insights.rs` | domain_insights CRUD + FTS5 search |
| `src-tauri/src/storage/invest/dream_snapshots.rs` | dream_snapshots table CRUD |
| `src/lib/components/invest/SchedulerTab.svelte` | Scheduler UI: job list, toggle, logs |
| `src/lib/components/invest/CommitteeAccuracyTab.svelte` | **Replace** placeholder with full accuracy UI |
| `src/lib/components/invest/DreamingConfigPanel.svelte` | Dreaming config + trigger + trace list |
| `src/lib/components/invest/ArchivedMemoriesTab.svelte` | Archived memory list with restore/delete |

### Modified Files

| File | Changes |
|------|---------|
| `src-tauri/Cargo.toml` | Add `cron = "0.15"` dependency |
| `src-tauri/src/invest/mod.rs` | Add `pub mod scheduler; pub mod verdict_review; pub mod dreaming;` |
| `src-tauri/src/storage/invest/mod.rs` | Add `pub mod verdict_reviews; pub mod domain_insights; pub mod dream_snapshots;`, add FTS5 migration SQL |
| `src-tauri/src/commands/invest.rs` | Add ~12 new Tauri commands |
| `src-tauri/src/commands/memos.rs` | Add `restore_memory` command |
| `src-tauri/src/lib.rs` | Register new commands, replace hardcoded cron loops with scheduler::start() |
| `src-tauri/src/invest/committee/tools.rs` | Upgrade exec_dreaming_insights to FTS5 MATCH |
| `src/routes/invest/+page.svelte` | Import SchedulerTab, wire it to scheduler tab slot |
| `src/routes/memory-mgmt/+page.svelte` | Add "已归档" tab with ArchivedMemoriesTab |
| `src/lib/stores/invest-store.svelte.ts` | Add scheduler + verdict review + dreaming state/methods |
| `messages/en.json` | Add ~40 i18n keys |
| `messages/zh-CN.json` | Add ~40 i18n keys |

---

## Task 1: Scheduler Framework — Backend Types + Config

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/invest/scheduler/mod.rs`
- Create: `src-tauri/src/invest/scheduler/config.rs`
- Modify: `src-tauri/src/invest/mod.rs`

### Step 1.1: Add `cron` crate dependency

Add to `[dependencies]` in `src-tauri/Cargo.toml`:

```toml
cron = "0.15"
```

### Step 1.2: Create `src-tauri/src/invest/scheduler/mod.rs`

```rust
pub mod config;
pub mod runner;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJob {
    pub id: String,
    pub name: String,
    pub cron_expr: String,
    #[serde(default)]
    pub interval_min: Option<i64>,
    pub enabled: bool,
    #[serde(default)]
    pub requires_trading_day: bool,
    #[serde(default)]
    pub last_run: Option<String>,
    #[serde(default)]
    pub next_run: Option<String>,
    #[serde(default)]
    pub last_status: Option<String>,
    #[serde(default)]
    pub description: String,
}

pub const DEFAULT_JOBS: &[CronJob] = &[
    CronJob {
        id: String::new(), // populated by const fn workaround below
        name: String::new(),
        cron_expr: String::new(),
        interval_min: None,
        enabled: false,
        requires_trading_day: false,
        last_run: None,
        next_run: None,
        last_status: None,
        description: String::new(),
    },
];
```

Note: Rust `const` doesn't allow `String::new()` in const context. Use a `default_jobs()` function instead:

```rust
pub fn default_jobs() -> Vec<CronJob> {
    vec![
        CronJob {
            id: "pnl_snapshot".into(),
            name: "PnL 快照".into(),
            cron_expr: "30 9,11 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "定时记录持仓市值快照".into(),
        },
        CronJob {
            id: "verdict_review".into(),
            name: "Verdict Review".into(),
            cron_expr: "0 17 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "回溯验证裁决命中率".into(),
        },
        CronJob {
            id: "event_scan".into(),
            name: "Event Watch 扫描".into(),
            cron_expr: "*/30 8-22 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "扫描财经新闻和公告".into(),
        },
        CronJob {
            id: "dream_user".into(),
            name: "Dreaming (用户记忆)".into(),
            cron_expr: String::new(),
            interval_min: Some(120),
            enabled: false,
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "用户记忆衰减与归档".into(),
        },
        CronJob {
            id: "dream_invest".into(),
            name: "Dreaming (投资)".into(),
            cron_expr: "0 3 * * *".into(),
            interval_min: None,
            enabled: false,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "投资洞察管线: Light→REM→Deep".into(),
        },
    ]
}
```

### Step 1.3: Create `src-tauri/src/invest/scheduler/config.rs`

```rust
use super::CronJob;
use std::path::PathBuf;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchedulerConfig {
    jobs: Vec<JobOverride>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobOverride {
    id: String,
    #[serde(default)]
    cron_expr: Option<String>,
    #[serde(default)]
    interval_min: Option<i64>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    requires_trading_day: Option<bool>,
}

fn config_path() -> PathBuf {
    let data = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    data.join("claw-go").join("invest").join("scheduler.json")
}

/// Load jobs: start from defaults, overlay user overrides from scheduler.json.
pub fn load_jobs() -> Vec<CronJob> {
    let mut jobs = super::default_jobs();
    let path = config_path();
    if !path.exists() {
        return jobs;
    }
    let Ok(content) = std::fs::read_to_string(&path) else {
        return jobs;
    };
    let Ok(config) = serde_json::from_str::<SchedulerConfig>(&content) else {
        return jobs;
    };
    for ov in config.jobs {
        if let Some(job) = jobs.iter_mut().find(|j| j.id == ov.id) {
            if let Some(c) = ov.cron_expr {
                job.cron_expr = c;
            }
            if let Some(i) = ov.interval_min {
                job.interval_min = Some(i);
            }
            if let Some(e) = ov.enabled {
                job.enabled = e;
            }
            if let Some(r) = ov.requires_trading_day {
                job.requires_trading_day = r;
            }
        }
    }
    jobs
}

/// Save user overrides (only changed fields) to scheduler.json.
pub fn save_jobs(jobs: &[CronJob]) -> Result<(), String> {
    let defaults = super::default_jobs();
    let overrides: Vec<JobOverride> = jobs
        .iter()
        .filter_map(|job| {
            let def = defaults.iter().find(|d| d.id == job.id);
            let changed = def.map_or(true, |d| {
                d.cron_expr != job.cron_expr
                    || d.interval_min != job.interval_min
                    || d.enabled != job.enabled
                    || d.requires_trading_day != job.requires_trading_day
            });
            if changed {
                Some(JobOverride {
                    id: job.id.clone(),
                    cron_expr: Some(job.cron_expr.clone()),
                    interval_min: job.interval_min,
                    enabled: Some(job.enabled),
                    requires_trading_day: Some(job.requires_trading_day),
                })
            } else {
                None
            }
        })
        .collect();
    let config = SchedulerConfig { jobs: overrides };
    let json = serde_json::to_string_pretty(&config).map_err(|e| format!("{e}"))?;
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{e}"))?;
    }
    std::fs::write(&path, json).map_err(|e| format!("{e}"))
}

/// Toggle a single job's enabled state and persist.
pub fn toggle_job(id: &str, enabled: bool) -> Result<(), String> {
    let mut jobs = load_jobs();
    let job = jobs
        .iter_mut()
        .find(|j| j.id == id)
        .ok_or_else(|| format!("Job '{}' not found", id))?;
    job.enabled = enabled;
    save_jobs(&jobs)
}

/// Update a single job's cron expression and persist.
pub fn update_cron(id: &str, cron_expr: &str) -> Result<(), String> {
    // Validate cron expression
    cron::Schedule::from_str(cron_expr).map_err(|e| format!("Invalid cron: {e}"))?;
    let mut jobs = load_jobs();
    let job = jobs
        .iter_mut()
        .find(|j| j.id == id)
        .ok_or_else(|| format!("Job '{}' not found", id))?;
    job.cron_expr = cron_expr.to_string();
    save_jobs(&jobs)
}
```

Note: `update_cron` needs `use std::str::FromStr;` and `use cron;` at the top.

### Step 1.4: Register module in `src-tauri/src/invest/mod.rs`

Add at the top alongside existing modules:

```rust
pub mod scheduler;
```

### Step 1.5: Verify compilation

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

Expected: compiles without errors (warnings about unused code are OK at this stage).

### Step 1.6: Commit

```bash
git add src-tauri/Cargo.toml src-tauri/src/invest/scheduler/ src-tauri/src/invest/mod.rs
git commit -m "feat(invest): add Scheduler framework types and config (Task 1)"
```

---

## Task 2: Scheduler Runner + Tauri Commands + SchedulerTab UI

**Files:**
- Create: `src-tauri/src/invest/scheduler/runner.rs`
- Modify: `src-tauri/src/commands/invest.rs` (add 5 commands)
- Modify: `src-tauri/src/lib.rs` (register commands, replace cron loops)
- Create: `src/lib/components/invest/SchedulerTab.svelte`
- Modify: `src/routes/invest/+page.svelte` (wire SchedulerTab)
- Modify: `src/lib/stores/invest-store.svelte.ts` (add scheduler state)

### Step 2.1: Create `src-tauri/src/invest/scheduler/runner.rs`

```rust
use super::config;
use crate::storage::invest::scheduler::{is_trading_day, log_task_end, log_task_start};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Start the scheduler loop. Call once from lib.rs setup.
/// The `dispatch` callback maps job_id to the async function to execute.
pub fn start<F, Fut>(dispatch: F)
where
    F: Fn(String) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
{
    if RUNNING.swap(true, Ordering::SeqCst) {
        return; // already running
    }
    let dispatch = Arc::new(dispatch);
    tauri::async_runtime::spawn(async move {
        // Initial delay to let app finish setup
        sleep(Duration::from_secs(10)).await;
        loop {
            let jobs = config::load_jobs();
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();

            for job in jobs {
                if !job.enabled {
                    continue;
                }

                // Check if it's time to fire
                let should_fire = if let Some(interval) = job.interval_min {
                    // Interval-based: next_fire = last_run + interval
                    match &job.last_run {
                        Some(last) => {
                            let Ok(last_dt) = chrono::NaiveDateTime::parse_from_str(
                                last, "%Y-%m-%dT%H:%M:%S",
                            ) else {
                                continue;
                            };
                            let next = last_dt + chrono::Duration::minutes(interval);
                            chrono::Local::now().naive_local() >= next
                        }
                        None => true, // never run → fire now
                    }
                } else {
                    // Cron-based
                    let Ok(schedule) = cron::Schedule::from_str(&job.cron_expr) else {
                        continue;
                    };
                    let after = match &job.last_run {
                        Some(last) => chrono::DateTime::parse_from_str(
                            &format!("{}+08:00", last), "%Y-%m-%dT%H:%M:%S%z",
                        )
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Local)),
                        None => None,
                    };
                    match after {
                        Some(after) => {
                            if let Some(next) = schedule.after(&after).next() {
                                chrono::Local::now() >= next
                            } else {
                                false
                            }
                        }
                        None => true, // never run → fire now
                    }
                };

                if !should_fire {
                    continue;
                }

                // Trading day guard
                if job.requires_trading_day && !is_trading_day(&today).unwrap_or(false) {
                    let _ = log_task_start(&job.id);
                    // Log skip (the log_task_start already created the row)
                    continue;
                }

                // Execute
                let log_id = log_task_start(&job.id).ok();
                let start = std::time::Instant::now();
                let result = (dispatch)(job.id.clone()).await;
                let elapsed = start.elapsed().as_millis() as i64;

                match result {
                    Ok(msg) => {
                        if let Some(id) = log_id {
                            let _ = log_task_end(id, "ok", Some(&msg));
                        }
                    }
                    Err(err) => {
                        if let Some(id) = log_id {
                            let _ = log_task_end(id, "error", Some(&err));
                        }
                    }
                }

                // Update last_run in config
                let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
                let mut jobs_mut = config::load_jobs();
                if let Some(j) = jobs_mut.iter_mut().find(|j| j.id == job.id) {
                    j.last_run = Some(now.clone());
                    j.last_status = Some(if result.is_ok() { "ok" } else { "error" }.into());
                    let _ = config::save_jobs(&jobs_mut);
                }
            }

            sleep(Duration::from_secs(60)).await;
        }
    });
}
```

### Step 2.2: Add Tauri commands to `src-tauri/src/commands/invest.rs`

Append at the end of the file:

```rust
// ── Scheduler commands ──────────────────────────────────────────────

#[tauri::command]
pub fn list_cron_jobs() -> Result<Vec<crate::invest::scheduler::CronJob>, String> {
    Ok(crate::invest::scheduler::config::load_jobs())
}

#[tauri::command]
pub fn toggle_cron_job(id: String, enabled: bool) -> Result<(), String> {
    crate::invest::scheduler::config::toggle_job(&id, enabled)
}

#[tauri::command]
pub fn update_cron_schedule(id: String, cron_expr: String) -> Result<(), String> {
    crate::invest::scheduler::config::update_cron(&id, &cron_expr)
}

#[tauri::command]
pub async fn trigger_cron_job(id: String) -> Result<(), String> {
    // Execute the job immediately by dispatching to the same functions the runner uses
    match id.as_str() {
        "pnl_snapshot" => {
            // Reuse the existing run_pnl_snapshot logic
            crate::invest::scheduler::runner::run_job_now(&id).await
        }
        "event_scan" => {
            crate::invest::scheduler::runner::run_job_now(&id).await
        }
        "verdict_review" => {
            crate::invest::scheduler::runner::run_job_now(&id).await
        }
        _ => Err(format!("Unknown job id: {}", id)),
    }
}
```

Note: `trigger_cron_job` needs a `run_job_now` helper in runner.rs. Let me add that to Step 2.1.

Actually, a simpler approach: `trigger_cron_job` should call the pipeline functions directly. The runner dispatches via a callback; the command should do the same. Let me restructure — the runner holds no state, so `trigger_cron_job` just calls the pipeline functions inline.

Revised Step 2.2 — `trigger_cron_job`:

```rust
#[tauri::command]
pub async fn trigger_cron_job(id: String) -> Result<String, String> {
    use crate::storage::invest::scheduler::{log_task_end, log_task_start};
    let log_id = log_task_start(&id)?;
    let result = match id.as_str() {
        "pnl_snapshot" => {
            // Reuse existing PnL snapshot logic
            let settings = crate::storage::settings::get_user_settings();
            let token = settings.tushare_token.ok_or("No Tushare token")?;
            let client = crate::tushare::TushareClient::new(token);
            // ... call run_pnl_snapshot equivalent
            Ok("PnL snapshot saved".to_string())
        }
        "event_scan" => {
            let (tushare, llm_client, llm_config) = build_scan_clients()?;
            let result = crate::invest::event_scanner::scan_events(
                &tushare, &*llm_client, &llm_config, None,
            )
            .await?;
            Ok(format!("Scanned: {} fetched, {} saved", result.fetched, result.saved))
        }
        "verdict_review" => {
            let settings = crate::storage::settings::get_user_settings();
            let token = settings.tushare_token.ok_or("No Tushare token")?;
            let summary = crate::invest::verdict_review::run_verdict_review(&token).await?;
            Ok(format!("Reviewed {} verdicts", summary.total_verdicts))
        }
        "dream_invest" => {
            let settings = crate::storage::settings::get_user_settings();
            let token = settings.tushare_token.ok_or("No Tushare token")?;
            let result = crate::invest::dreaming::trigger_dream("invest", &token).await?;
            Ok(format!("Dream complete: {} insights", result.insights_written))
        }
        _ => Err(format!("Unknown job: {}", id)),
    };
    let status = if result.is_ok() { "ok" } else { "error" };
    let msg = match &result {
        Ok(m) => Some(m.as_str()),
        Err(e) => Some(e.as_str()),
    };
    let _ = log_task_end(log_id, status, msg);
    result
}
```

This is cleaner — each job maps to its pipeline function inline.

### Step 2.3: Register commands in `src-tauri/src/lib.rs`

Add to the `invoke_handler` macro (after line 460):

```rust
commands::invest::list_cron_jobs,
commands::invest::toggle_cron_job,
commands::invest::update_cron_schedule,
commands::invest::trigger_cron_job,
commands::invest::get_cron_job_logs,
```

### Step 2.4: Replace hardcoded cron loops in `lib.rs`

In `lib.rs`, replace the event scanner cron spawn (around line 682) and PnL snapshot cron spawn (around line 612) with a single scheduler start call. Keep the trade calendar sync as a one-shot startup task (it runs once, not on a schedule).

```rust
// In the setup section, after invest DB init:
crate::invest::scheduler::runner::start(|job_id| async move {
    match job_id.as_str() {
        "pnl_snapshot" => { /* existing pnl logic */ }
        "event_scan" => {
            let (tushare, llm_client, llm_config) = build_scan_clients()?;
            let r = crate::invest::event_scanner::scan_events(&tushare, &*llm_client, &llm_config, None).await?;
            Ok(format!("{} fetched, {} saved", r.fetched, r.saved))
        }
        "verdict_review" => {
            let settings = crate::storage::settings::get_user_settings();
            let token = settings.tushare_token.ok_or("No Tushare token")?;
            let s = crate::invest::verdict_review::run_verdict_review(&token).await?;
            Ok(format!("{} verdicts reviewed", s.total_verdicts))
        }
        "dream_invest" => {
            let settings = crate::storage::settings::get_user_settings();
            let token = settings.tushare_token.ok_or("No Tushare token")?;
            let r = crate::invest::dreaming::trigger_dream("invest", &token).await?;
            Ok(format!("{} insights", r.insights_written))
        }
        "dream_user" => {
            // Reuse existing dream cycle logic
            Ok("User dream complete".to_string())
        }
        _ => Err(format!("Unknown job: {}", job_id)),
    }
});
```

Note: Remove or comment out the old `spawn_event_scanner_cron()` call and the inline PnL snapshot cron block.

### Step 2.5: Create `src/lib/components/invest/SchedulerTab.svelte`

```svelte
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { invoke } from '$lib/transport';

  interface CronJob {
    id: string;
    name: string;
    cronExpr: string;
    intervalMin?: number;
    enabled: boolean;
    requiresTradingDay: boolean;
    lastRun?: string;
    nextRun?: string;
    lastStatus?: string;
    description: string;
  }

  interface SchedulerLog {
    id: number;
    taskName: string;
    status: string;
    message?: string;
    startedAt: string;
    finishedAt?: string;
    durationMs?: number;
  }

  let jobs: CronJob[] = $state([]);
  let logs: SchedulerLog[] = $state([]);
  let loading = $state(false);
  let expandedJob = $state<string | null>(null);
  let editingJob = $state<string | null>(null);
  let editCronValue = $state('');
  let triggering = $state<string | null>(null);

  async function loadJobs() {
    loading = true;
    try {
      jobs = await invoke<CronJob[]>('list_cron_jobs');
    } finally {
      loading = false;
    }
  }

  async function loadLogs(jobId: string) {
    expandedJob = expandedJob === jobId ? null : jobId;
    if (expandedJob) {
      logs = await invoke<SchedulerLog[]>('get_cron_job_logs', { taskName: jobId, limit: 20 });
    }
  }

  async function toggle(job: CronJob) {
    await invoke('toggle_cron_job', { id: job.id, enabled: !job.enabled });
    await loadJobs();
  }

  async function runNow(jobId: string) {
    triggering = jobId;
    try {
      await invoke('trigger_cron_job', { id: jobId });
      await loadJobs();
    } finally {
      triggering = null;
    }
  }

  async function saveCron(jobId: string) {
    await invoke('update_cron_schedule', { id: jobId, cronExpr: editCronValue });
    editingJob = null;
    await loadJobs();
  }

  function humanCron(expr: string): string {
    // Simple human-readable conversion
    const map: Record<string, string> = {
      '30 9,11 * * 1-5': 'Weekdays 9:30, 11:00',
      '0 13,15 * * 1-5': 'Weekdays 13:00, 15:00',
      '0 17 * * 1-5': 'Weekdays 17:00',
      '*/30 8-22 * * 1-5': 'Weekdays every 30min (8-22h)',
      '0 3 * * *': 'Daily 03:00',
    };
    return map[expr] || expr;
  }

  function statusColor(status?: string): string {
    if (status === 'ok') return 'text-green-500';
    if (status === 'error') return 'text-red-500';
    if (status === 'skipped') return 'text-muted-foreground';
    return 'text-muted-foreground';
  }

  $effect(() => { loadJobs(); });
</script>

<div class="space-y-4">
  <h3 class="text-lg font-semibold">{t('invest.scheduler.title')}</h3>

  {#if loading}
    <p class="text-muted-foreground">{t('common.loading')}</p>
  {:else}
    <div class="rounded-lg border">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b bg-muted/50 text-left">
            <th class="p-3">{t('invest.scheduler.jobName')}</th>
            <th class="p-3">{t('invest.scheduler.cronExpr')}</th>
            <th class="p-3 text-center">Status</th>
            <th class="p-3">{t('invest.scheduler.lastRun')}</th>
            <th class="p-3 text-right">Actions</th>
          </tr>
        </thead>
        <tbody>
          {#each jobs as job}
            <tr class="border-b last:border-0">
              <td class="p-3">
                <div class="font-medium">{job.name}</div>
                <div class="text-xs text-muted-foreground">{job.description}</div>
              </td>
              <td class="p-3">
                {#if editingJob === job.id}
                  <div class="flex items-center gap-2">
                    <input
                      class="w-40 rounded border bg-background px-2 py-1 text-xs"
                      bind:value={editCronValue}
                    />
                    <button class="text-xs text-primary" onclick={() => saveCron(job.id)}>Save</button>
                    <button class="text-xs text-muted-foreground" onclick={() => editingJob = null}>Cancel</button>
                  </div>
                {:else}
                  <span class="text-xs font-mono">{humanCron(job.cronExpr)}</span>
                  {#if job.intervalMin}
                    <span class="ml-1 text-xs text-muted-foreground">(every {job.intervalMin}min)</span>
                  {/if}
                  <button
                    class="ml-2 text-xs text-muted-foreground hover:text-foreground"
                    onclick={() => { editingJob = job.id; editCronValue = job.cronExpr; }}
                  >Edit</button>
                {/if}
              </td>
              <td class="p-3 text-center">
                <button
                  class="relative inline-flex h-5 w-9 items-center rounded-full transition-colors {job.enabled ? 'bg-primary' : 'bg-muted'}"
                  onclick={() => toggle(job)}
                >
                  <span class="inline-block h-3.5 w-3.5 rounded-full bg-white transition-transform {job.enabled ? 'translate-x-4' : 'translate-x-1'}"></span>
                </button>
              </td>
              <td class="p-3">
                {#if job.lastRun}
                  <div class="text-xs">{new Date(job.lastRun).toLocaleString()}</div>
                  <div class="text-xs {statusColor(job.lastStatus)}">{job.lastStatus || '-'}</div>
                {:else}
                  <span class="text-xs text-muted-foreground">-</span>
                {/if}
              </td>
              <td class="p-3 text-right">
                <div class="flex items-center justify-end gap-2">
                  <button
                    class="rounded px-2 py-1 text-xs hover:bg-muted disabled:opacity-50"
                    disabled={triggering === job.id}
                    onclick={() => runNow(job.id)}
                  >
                    {triggering === job.id ? '...' : t('invest.scheduler.runNow')}
                  </button>
                  <button
                    class="rounded px-2 py-1 text-xs hover:bg-muted"
                    onclick={() => loadLogs(job.id)}
                  >
                    {t('invest.scheduler.viewLogs')}
                  </button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    {#if expandedJob}
      <div class="rounded-lg border p-4">
        <h4 class="mb-2 text-sm font-medium">Logs: {expandedJob}</h4>
        {#if logs.length === 0}
          <p class="text-xs text-muted-foreground">No logs yet</p>
        {:else}
          <div class="max-h-60 overflow-y-auto space-y-1">
            {#each logs as log}
              <div class="flex items-center gap-3 text-xs">
                <span class="text-muted-foreground">{new Date(log.startedAt).toLocaleString()}</span>
                <span class={statusColor(log.status)}>{log.status}</span>
                <span class="text-muted-foreground">{log.durationMs ? `${log.durationMs}ms` : ''}</span>
                <span class="truncate">{log.message || ''}</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</div>
```

### Step 2.6: Wire SchedulerTab in `src/routes/invest/+page.svelte`

Add import:

```typescript
import SchedulerTab from '$lib/components/invest/SchedulerTab.svelte';
```

Replace the placeholder at line ~180:

```svelte
{:else if activeTab === 'scheduler'}
    <SchedulerTab />
```

### Step 2.7: Commit

```bash
git add src-tauri/src/invest/scheduler/runner.rs src-tauri/src/commands/invest.rs src-tauri/src/lib.rs src/lib/components/invest/SchedulerTab.svelte src/routes/invest/+page.svelte
git commit -m "feat(invest): add Scheduler runner, commands, and SchedulerTab UI (Task 2)"
```

---

## Task 3: Verdict Review Pipeline — Backend

**Files:**
- Create: `src-tauri/src/invest/verdict_review.rs`
- Create: `src-tauri/src/storage/invest/verdict_reviews.rs`
- Modify: `src-tauri/src/storage/invest/mod.rs` (add module + migration SQL)
- Modify: `src-tauri/src/invest/mod.rs` (add module)

### Step 3.1: Create `src-tauri/src/storage/invest/verdict_reviews.rs`

```rust
use crate::storage::invest::with_conn;
use crate::storage::invest::with_conn_mut;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictReviewEntry {
    pub id: i64,
    pub verdict_id: String,
    pub symbol: String,
    pub verdict_type: String,
    pub verdict_date: String,
    pub window_days: i64,
    pub price_at_verdict: Option<f64>,
    pub price_after: Option<f64>,
    pub return_pct: Option<f64>,
    pub hit: bool,
    pub flat_threshold: Option<f64>,
    pub created_at: String,
}

pub fn create_table_if_not_exists() -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS verdict_reviews (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                verdict_id      TEXT NOT NULL,
                symbol          TEXT NOT NULL,
                verdict_type    TEXT NOT NULL,
                verdict_date    TEXT NOT NULL,
                window_days     INTEGER NOT NULL,
                price_at_verdict REAL,
                price_after     REAL,
                return_pct      REAL,
                hit             INTEGER NOT NULL,
                flat_threshold  REAL,
                created_at      TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_vr_verdict ON verdict_reviews(verdict_id);
            CREATE INDEX IF NOT EXISTS idx_vr_symbol ON verdict_reviews(symbol);
            CREATE INDEX IF NOT EXISTS idx_vr_date ON verdict_reviews(verdict_date);"
        )?;
        Ok(())
    })
}

pub fn upsert_review(
    verdict_id: &str,
    symbol: &str,
    verdict_type: &str,
    verdict_date: &str,
    window_days: i64,
    price_at_verdict: Option<f64>,
    price_after: Option<f64>,
    return_pct: Option<f64>,
    hit: bool,
    flat_threshold: Option<f64>,
) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO verdict_reviews (verdict_id, symbol, verdict_type, verdict_date, window_days, price_at_verdict, price_after, return_pct, hit, flat_threshold, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, datetime('now'))
             ON CONFLICT(verdict_id, window_days) DO UPDATE SET
               price_at_verdict = excluded.price_at_verdict,
               price_after = excluded.price_after,
               return_pct = excluded.return_pct,
               hit = excluded.hit,
               flat_threshold = excluded.flat_threshold",
            rusqlite::params![verdict_id, symbol, verdict_type, verdict_date, window_days,
                price_at_verdict, price_after, return_pct, hit as i32, flat_threshold],
        )?;
        Ok(())
    })
}

pub fn list_reviews(symbol: Option<&str>, limit: Option<i64>) -> Result<Vec<VerdictReviewEntry>, String> {
    with_conn(|conn| {
        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match symbol {
            Some(s) => (
                "SELECT * FROM verdict_reviews WHERE symbol = ?1 ORDER BY verdict_date DESC LIMIT ?2".into(),
                vec![Box::new(s.to_string()), Box::new(limit.unwrap_or(200))],
            ),
            None => (
                "SELECT * FROM verdict_reviews ORDER BY verdict_date DESC LIMIT ?1".into(),
                vec![Box::new(limit.unwrap_or(200))],
            ),
        };
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok(VerdictReviewEntry {
                id: row.get(0)?,
                verdict_id: row.get(1)?,
                symbol: row.get(2)?,
                verdict_type: row.get(3)?,
                verdict_date: row.get(4)?,
                window_days: row.get(5)?,
                price_at_verdict: row.get(6)?,
                price_after: row.get(7)?,
                return_pct: row.get(8)?,
                hit: row.get::<_, i32>(9)? != 0,
                flat_threshold: row.get(10)?,
                created_at: row.get(11)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    })
}

pub fn clear_reviews() -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute("DELETE FROM verdict_reviews", [])?;
        Ok(())
    })
}
```

Note: The `ON CONFLICT(verdict_id, window_days)` requires a UNIQUE constraint. Add to the CREATE TABLE:

```sql
UNIQUE(verdict_id, window_days)
```

### Step 3.2: Add migration in `src-tauri/src/storage/invest/mod.rs`

In the `init_db` function, after existing migrations, add:

```rust
// Create verdict_reviews table
if let Err(e) = crate::storage::invest::verdict_reviews::create_table_if_not_exists() {
    log::warn!("Failed to create verdict_reviews: {}", e);
}
```

Add module declaration:

```rust
pub mod verdict_reviews;
```

### Step 3.3: Create `src-tauri/src/invest/verdict_review.rs`

```rust
use crate::storage::invest::verdicts;
use crate::storage::invest::verdict_reviews;
use crate::tushare::TushareClient;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictReviewSummary {
    pub total_verdicts: usize,
    pub overall_hit_rate: f64,
    pub directional_hit_rate: f64,
    pub by_window: Vec<WindowStats>,
    pub by_verdict: Vec<VerdictStats>,
    pub last_review_at: Option<String>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowStats {
    pub window_days: i64,
    pub sample_count: usize,
    pub hit_rate: f64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictStats {
    pub verdict_type: String,
    pub sample_count: usize,
    pub avg_confidence: f64,
    pub hit_rate_1d: f64,
    pub hit_rate_7d: f64,
    pub hit_rate_30d: f64,
}

const K_FLAT: f64 = 1.0;
const WINDOWS: &[i64] = &[1, 7, 30];

fn flat_threshold(atr_pct: f64, days: i64) -> f64 {
    (K_FLAT * atr_pct * (days as f64).sqrt()).min(0.08)
}

fn is_hit(verdict_type: &str, return_pct: f64, threshold: f64) -> bool {
    match verdict_type {
        "BUY" | "ACCUMULATE" => return_pct > 0.0,
        "SELL" | "TRIM" => return_pct < 0.0,
        "HOLD" => return_pct.abs() < threshold,
        _ => return_pct > 0.0,
    }
}

/// Run the full verdict review pipeline.
pub async fn run_verdict_review(tushare_token: &str) -> Result<VerdictReviewSummary, String> {
    let client = TushareClient::new(tushare_token.to_string());
    let verdicts = verdicts::list_verdicts(None, None)?;

    // Clear old reviews for fresh calculation
    verdict_reviews::clear_reviews()?;

    let mut all_reviews: Vec<(String, i64, bool)> = Vec::new(); // (verdict_type, window, hit)

    for v in &verdicts {
        // Fetch prices around verdict date
        let date_str = &v.verdict_date();
        let start = date_from_offset(date_str, -1);
        let end = date_from_offset(date_str, 31);

        let bars = match client.daily(&v.symbol, &start, &end).await {
            Ok(b) => b,
            Err(_) => continue,
        };

        if bars.is_empty() {
            continue;
        }

        // Find verdict date price
        let verdict_bar = bars.iter().find(|b| b.trade_date == *date_str);
        let Some(vbar) = verdict_bar else { continue };
        let price_at = vbar.close;

        // Calculate ATR14 from available bars
        let atr_pct = calc_atr_pct(&bars, price_at);

        for &window in WINDOWS {
            // Find the bar closest to verdict_date + window days
            let target_date = date_from_offset(date_str, window);
            let after_bar = bars.iter().find(|b| b.trade_date >= target_date);
            let Some(abar) = after_bar else { continue };
            let return_pct = (abar.close - price_at) / price_at;
            let threshold = flat_threshold(atr_pct, window);
            let hit = is_hit(&v.verdict, return_pct, threshold);

            verdict_reviews::upsert_review(
                &v.id,
                &v.symbol,
                &v.verdict,
                date_str,
                window,
                Some(price_at),
                Some(abar.close),
                Some(return_pct),
                hit,
                Some(threshold),
            )?;

            all_reviews.push((v.verdict.clone(), window, hit));
        }
    }

    // Aggregate
    let summary = aggregate(&all_reviews, &verdicts);
    Ok(summary)
}

fn aggregate(reviews: &[(String, i64, bool)], verdicts: &[verdicts::Verdict]) -> VerdictReviewSummary {
    let total = verdicts.len();
    let total_30d: usize = reviews.iter().filter(|(_, w, _)| *w == 30).count();
    let hits_30d: usize = reviews.iter().filter(|(_, w, h)| *w == 30 && *h).count();

    // Directional = exclude HOLD
    let directional: Vec<_> = reviews
        .iter()
        .filter(|(vt, _, _)| vt != "HOLD")
        .collect();
    let dir_total = directional.len();
    let dir_hits = directional.iter().filter(|(_, _, h)| **h).count();

    let by_window: Vec<WindowStats> = WINDOWS
        .iter()
        .map(|&w| {
            let w_reviews: Vec<_> = reviews.iter().filter(|(_, ww, _)| *ww == w).collect();
            let count = w_reviews.len();
            let hits = w_reviews.iter().filter(|(_, _, h)| **h).count();
            WindowStats {
                window_days: w,
                sample_count: count,
                hit_rate: if count > 0 { hits as f64 / count as f64 } else { 0.0 },
            }
        })
        .collect();

    let verdict_types = ["BUY", "ACCUMULATE", "HOLD", "TRIM", "SELL"];
    let by_verdict: Vec<VerdictStats> = verdict_types
        .iter()
        .map(|&vt| {
            let vt_verdicts: Vec<_> = verdicts.iter().filter(|v| v.verdict == vt).collect();
            let count = vt_verdicts.len();
            let avg_conf = if count > 0 {
                vt_verdicts.iter().filter_map(|v| v.confidence).sum::<f64>() / count as f64
            } else {
                0.0
            };
            let rate = |w: i64| {
                let w_reviews: Vec<_> = reviews.iter().filter(|(vvt, ww, _)| vvt == vt && *ww == w).collect();
                let c = w_reviews.len();
                let h = w_reviews.iter().filter(|(_, _, h)| **h).count();
                if c > 0 { h as f64 / c as f64 } else { 0.0 }
            };
            VerdictStats {
                verdict_type: vt.to_string(),
                sample_count: count,
                avg_confidence: avg_conf,
                hit_rate_1d: rate(1),
                hit_rate_7d: rate(7),
                hit_rate_30d: rate(30),
            }
        })
        .collect();

    VerdictReviewSummary {
        total_verdicts: total,
        overall_hit_rate: if total_30d > 0 { hits_30d as f64 / total_30d as f64 } else { 0.0 },
        directional_hit_rate: if dir_total > 0 { dir_hits as f64 / dir_total as f64 } else { 0.0 },
        by_window,
        by_verdict,
        last_review_at: Some(chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()),
    }
}

fn date_from_offset(date: &str, days: i64) -> String {
    let Ok(dt) = chrono::NaiveDate::parse_from_str(date, "%Y%m%d") else {
        return date.to_string();
    };
    (dt + chrono::Duration::days(days)).format("%Y%m%d").to_string()
}

fn calc_atr_pct(bars: &[crate::tushare::DailyBar], price: f64) -> f64 {
    if bars.len() < 15 || price <= 0.0 {
        return 0.03; // default 3%
    }
    let mut trs = Vec::new();
    for i in 1..bars.len() {
        let high = bars[i].high;
        let low = bars[i].low;
        let prev_close = bars[i - 1].close;
        let tr = (high - low).max((high - prev_close).abs()).max((low - prev_close).abs());
        trs.push(tr);
    }
    let atr14: f64 = trs.iter().rev().take(14).sum::<f64>() / 14.0;
    atr14 / price
}
```

Note: The `Verdict` struct needs a `verdict_date()` method. The current `Verdict` struct doesn't have a `verdict_date` field — it uses `created_at`. Extract the date part from `created_at`:

```rust
// Add to verdicts.rs or use inline:
fn verdict_date(&self) -> String {
    self.created_at[..10].replace('-', "")
}
```

### Step 3.4: Register module in `src-tauri/src/invest/mod.rs`

```rust
pub mod verdict_review;
```

### Step 3.5: Verify compilation

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

### Step 3.6: Commit

```bash
git add src-tauri/src/invest/verdict_review.rs src-tauri/src/storage/invest/verdict_reviews.rs src-tauri/src/storage/invest/mod.rs src-tauri/src/invest/mod.rs
git commit -m "feat(invest): add Verdict Review pipeline and storage (Task 3)"
```

---

## Task 4: Verdict Review — Tauri Commands + AccuracyTab UI

**Files:**
- Modify: `src-tauri/src/commands/invest.rs` (add 3 commands)
- Modify: `src/lib/components/invest/CommitteeAccuracyTab.svelte` (replace placeholder)
- Modify: `src/lib/stores/invest-store.svelte.ts` (add verdict review state)

### Step 4.1: Add Tauri commands to `src-tauri/src/commands/invest.rs`

```rust
// ── Verdict Review commands ──────────────────────────────────────────

#[tauri::command]
pub async fn run_verdict_review(tushare_token: String) -> Result<crate::invest::verdict_review::VerdictReviewSummary, String> {
    crate::invest::verdict_review::run_verdict_review(&tushare_token).await
}

#[tauri::command]
pub fn get_verdict_review_summary() -> Result<crate::invest::verdict_review::VerdictReviewSummary, String> {
    // Reconstruct summary from stored reviews
    use crate::storage::invest::verdict_reviews;
    let reviews = verdict_reviews::list_reviews(None, None)?;
    if reviews.is_empty() {
        return Ok(crate::invest::verdict_review::VerdictReviewSummary {
            total_verdicts: 0,
            overall_hit_rate: 0.0,
            directional_hit_rate: 0.0,
            by_window: vec![],
            by_verdict: vec![],
            last_review_at: None,
        });
    }
    // Aggregate from stored reviews (same logic as pipeline but from DB)
    let mut by_window = std::collections::HashMap::new();
    let mut by_verdict_type = std::collections::HashMap::new();
    let mut total_30d = 0usize;
    let mut hits_30d = 0usize;
    let mut dir_total = 0usize;
    let mut dir_hits = 0usize;

    for r in &reviews {
        let entry = by_window.entry(r.window_days).or_insert((0usize, 0usize));
        entry.0 += 1;
        if r.hit { entry.1 += 1; }

        let v_entry = by_verdict_type.entry(r.verdict_type.clone())
            .or_insert((0usize, [0usize; 3], [0usize; 3])); // count, hits_per_window, total_per_window
        v_entry.0 += 1;
        let w_idx = match r.window_days { 1 => 0, 7 => 1, _ => 2 };
        v_entry.2[w_idx] += 1;
        if r.hit { v_entry.1[w_idx] += 1; }

        if r.window_days == 30 {
            total_30d += 1;
            if r.hit { hits_30d += 1; }
        }
        if r.verdict_type != "HOLD" && r.window_days == 30 {
            dir_total += 1;
            if r.hit { dir_hits += 1; }
        }
    }

    Ok(crate::invest::verdict_review::VerdictReviewSummary {
        total_verdicts: reviews.iter().map(|r| &r.verdict_id).collect::<std::collections::HashSet<_>>().len(),
        overall_hit_rate: if total_30d > 0 { hits_30d as f64 / total_30d as f64 } else { 0.0 },
        directional_hit_rate: if dir_total > 0 { dir_hits as f64 / dir_total as f64 } else { 0.0 },
        by_window: by_window.into_iter().map(|(days, (total, hits))| {
            crate::invest::verdict_review::WindowStats {
                window_days: days,
                sample_count: total,
                hit_rate: if total > 0 { hits as f64 / total as f64 } else { 0.0 },
            }
        }).collect(),
        by_verdict: by_verdict_type.into_iter().map(|(vt, (count, hits, totals))| {
            let rate = |i: usize| if totals[i] > 0 { hits[i] as f64 / totals[i] as f64 } else { 0.0 };
            crate::invest::verdict_review::VerdictStats {
                verdict_type: vt,
                sample_count: count,
                avg_confidence: 0.0, // not stored in reviews
                hit_rate_1d: rate(0),
                hit_rate_7d: rate(1),
                hit_rate_30d: rate(2),
            }
        }).collect(),
        last_review_at: reviews.first().map(|r| r.created_at.clone()),
    })
}

#[tauri::command]
pub fn get_verdict_review_detail(symbol: Option<String>) -> Result<Vec<crate::storage::invest::verdict_reviews::VerdictReviewEntry>, String> {
    crate::storage::invest::verdict_reviews::list_reviews(symbol.as_deref(), Some(200))
}
```

### Step 4.2: Register commands in `lib.rs`

Add to invoke_handler:

```rust
commands::invest::run_verdict_review,
commands::invest::get_verdict_review_summary,
commands::invest::get_verdict_review_detail,
```

### Step 4.3: Replace `src/lib/components/invest/CommitteeAccuracyTab.svelte`

```svelte
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { invoke } from '$lib/transport';

  interface WindowStats {
    windowDays: number;
    sampleCount: number;
    hitRate: number;
  }
  interface VerdictStats {
    verdictType: string;
    sampleCount: number;
    avgConfidence: number;
    hitRate1d: number;
    hitRate7d: number;
    hitRate30d: number;
  }
  interface ReviewSummary {
    totalVerdicts: number;
    overallHitRate: number;
    directionalHitRate: number;
    byWindow: WindowStats[];
    byVerdict: VerdictStats[];
    lastReviewAt?: string;
  }
  interface ReviewEntry {
    id: number;
    verdictId: string;
    symbol: string;
    verdictType: string;
    verdictDate: string;
    windowDays: number;
    priceAtVerdict?: number;
    priceAfter?: number;
    returnPct?: number;
    hit: boolean;
    flatThreshold?: number;
  }

  let summary = $state<ReviewSummary | null>(null);
  let detail = $state<ReviewEntry[]>([]);
  let loading = $state(false);
  let running = $state(false);
  let showDetail = $state(false);
  let error = $state<string | null>(null);

  async function loadSummary() {
    loading = true;
    try {
      summary = await invoke<ReviewSummary>('get_verdict_review_summary');
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function runReview() {
    running = true;
    error = null;
    try {
      // Get tushare token from settings
      const settings = await invoke<{ tushareToken?: string }>('get_user_settings');
      if (!settings.tushareToken) {
        error = 'No Tushare token configured';
        return;
      }
      summary = await invoke<ReviewSummary>('run_verdict_review', { tushareToken: settings.tushareToken });
    } catch (e) {
      error = String(e);
    } finally {
      running = false;
    }
  }

  async function loadDetail() {
    showDetail = !showDetail;
    if (showDetail && detail.length === 0) {
      detail = await invoke<ReviewEntry[]>('get_verdict_review_detail', {});
    }
  }

  function pct(n: number): string {
    return (n * 100).toFixed(1) + '%';
  }

  function hitColor(rate: number): string {
    if (rate >= 0.6) return 'text-green-500';
    if (rate >= 0.4) return 'text-yellow-500';
    return 'text-red-500';
  }

  $effect(() => { loadSummary(); });
</script>

<div class="space-y-4">
  <div class="flex items-center justify-between">
    <h3 class="text-lg font-semibold">{t('invest.accuracy.title')}</h3>
    <button
      class="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
      disabled={running}
      onclick={runReview}
    >
      {running ? '...' : t('invest.accuracy.runReview')}
    </button>
  </div>

  {#if error}
    <div class="rounded border border-red-300 bg-red-50 p-3 text-sm text-red-700 dark:bg-red-950 dark:text-red-300">
      {error}
    </div>
  {/if}

  {#if loading}
    <p class="text-muted-foreground">{t('common.loading')}</p>
  {:else if !summary || summary.totalVerdicts === 0}
    <div class="flex h-32 items-center justify-center">
      <p class="text-muted-foreground">{t('invest.accuracy.noData')}</p>
    </div>
  {:else}
    <!-- KPI Cards -->
    <div class="grid grid-cols-3 gap-4">
      <div class="rounded-lg border p-4 text-center">
        <div class="text-2xl font-bold">{summary.totalVerdicts}</div>
        <div class="text-xs text-muted-foreground">{t('invest.accuracy.totalVerdicts')}</div>
      </div>
      <div class="rounded-lg border p-4 text-center">
        <div class="text-2xl font-bold {hitColor(summary.overallHitRate)}">{pct(summary.overallHitRate)}</div>
        <div class="text-xs text-muted-foreground">{t('invest.accuracy.overallHitRate')}</div>
      </div>
      <div class="rounded-lg border p-4 text-center border-primary/30">
        <div class="text-2xl font-bold {hitColor(summary.directionalHitRate)}">{pct(summary.directionalHitRate)}</div>
        <div class="text-xs text-muted-foreground">{t('invest.accuracy.directionalHitRate')}</div>
      </div>
    </div>

    <!-- Honesty banner -->
    {#if summary.directionalHitRate < 0.5}
      <div class="rounded border border-yellow-300 bg-yellow-50 p-3 text-sm text-yellow-800 dark:bg-yellow-950 dark:text-yellow-200">
        {t('invest.accuracy.honestyBanner')}
      </div>
    {/if}

    <!-- By Window -->
    <div class="rounded-lg border">
      <div class="border-b bg-muted/50 px-4 py-2 text-sm font-medium">{t('invest.accuracy.byWindow')}</div>
      <div class="p-4 space-y-3">
        {#each summary.byWindow.sort((a, b) => a.windowDays - b.windowDays) as w}
          <div class="flex items-center gap-3">
            <span class="w-12 text-sm font-medium">{w.windowDays}d</span>
            <div class="flex-1">
              <div class="h-3 rounded-full bg-muted overflow-hidden">
                <div class="h-full rounded-full bg-primary transition-all" style="width: {w.hitRate * 100}%"></div>
              </div>
            </div>
            <span class="w-16 text-right text-sm font-mono {hitColor(w.hitRate)}">{pct(w.hitRate)}</span>
            <span class="w-16 text-right text-xs text-muted-foreground">{w.sampleCount} samples</span>
          </div>
        {/each}
      </div>
    </div>

    <!-- By Verdict Type -->
    <div class="rounded-lg border">
      <div class="border-b bg-muted/50 px-4 py-2 text-sm font-medium">{t('invest.accuracy.byVerdict')}</div>
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b text-left text-xs text-muted-foreground">
            <th class="px-4 py-2">Type</th>
            <th class="px-4 py-2 text-right">Count</th>
            <th class="px-4 py-2 text-right">1d</th>
            <th class="px-4 py-2 text-right">7d</th>
            <th class="px-4 py-2 text-right">30d</th>
          </tr>
        </thead>
        <tbody>
          {#each summary.byVerdict.sort((a, b) => b.sampleCount - a.sampleCount) as v}
            <tr class="border-b last:border-0">
              <td class="px-4 py-2 font-medium">{v.verdictType}</td>
              <td class="px-4 py-2 text-right">{v.sampleCount}</td>
              <td class="px-4 py-2 text-right {hitColor(v.hitRate1d)}">{pct(v.hitRate1d)}</td>
              <td class="px-4 py-2 text-right {hitColor(v.hitRate7d)}">{pct(v.hitRate7d)}</td>
              <td class="px-4 py-2 text-right {hitColor(v.hitRate30d)}">{pct(v.hitRate30d)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <!-- Detail toggle -->
    <button class="text-sm text-muted-foreground hover:text-foreground" onclick={loadDetail}>
      {showDetail ? 'Hide' : 'Show'} detail ({detail.length} entries)
    </button>

    {#if showDetail && detail.length > 0}
      <div class="max-h-80 overflow-y-auto rounded-lg border">
        <table class="w-full text-xs">
          <thead>
            <tr class="border-b bg-muted/50 text-left">
              <th class="px-3 py-2">Symbol</th>
              <th class="px-3 py-2">Date</th>
              <th class="px-3 py-2">Verdict</th>
              <th class="px-3 py-2 text-right">Window</th>
              <th class="px-3 py-2 text-right">Return%</th>
              <th class="px-3 py-2 text-center">Hit</th>
            </tr>
          </thead>
          <tbody>
            {#each detail as d}
              <tr class="border-b last:border-0">
                <td class="px-3 py-2 font-mono">{d.symbol}</td>
                <td class="px-3 py-2">{d.verdictDate}</td>
                <td class="px-3 py-2">{d.verdictType}</td>
                <td class="px-3 py-2 text-right">{d.windowDays}d</td>
                <td class="px-3 py-2 text-right {(d.returnPct ?? 0) >= 0 ? 'text-green-500' : 'text-red-500'}">
                  {d.returnPct != null ? pct(d.returnPct) : '-'}
                </td>
                <td class="px-3 py-2 text-center">
                  <span class="inline-block w-5 rounded text-center {d.hit ? 'bg-green-100 text-green-700' : 'bg-red-100 text-red-700'}">
                    {d.hit ? '✓' : '✗'}
                  </span>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}

    {#if summary.lastReviewAt}
      <p class="text-xs text-muted-foreground">Last review: {new Date(summary.lastReviewAt).toLocaleString()}</p>
    {/if}
  {/if}
</div>
```

### Step 4.4: Commit

```bash
git add src-tauri/src/commands/invest.rs src-tauri/src/lib.rs src/lib/components/invest/CommitteeAccuracyTab.svelte
git commit -m "feat(invest): add Verdict Review commands and AccuracyTab UI (Task 4)"
```

---

## Task 5: Dreaming Invest Pipeline — Backend

**Files:**
- Create: `src-tauri/src/invest/dreaming/mod.rs`
- Create: `src-tauri/src/invest/dreaming/pipeline.rs`
- Create: `src-tauri/src/storage/invest/domain_insights.rs`
- Modify: `src-tauri/src/invest/mod.rs`
- Modify: `src-tauri/src/storage/invest/mod.rs`

### Step 5.1: Create `src-tauri/src/storage/invest/domain_insights.rs`

```rust
use crate::storage::invest::{with_conn, with_conn_mut};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainInsight {
    pub id: String,
    pub insight_type: String,
    pub symbol: Option<String>,
    pub content: String,
    pub confidence: Option<f64>,
    pub source_verdict_ids: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

pub fn upsert_insight(insight: &DomainInsight) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO domain_insights (id, insight_type, symbol, content, confidence, source_verdict_ids, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               content = excluded.content,
               confidence = excluded.confidence,
               source_verdict_ids = excluded.source_verdict_ids,
               updated_at = excluded.updated_at",
            rusqlite::params![
                insight.id, insight.insight_type, insight.symbol, insight.content,
                insight.confidence, insight.source_verdict_ids, insight.status,
                insight.created_at, insight.updated_at
            ],
        )?;
        Ok(())
    })
}

pub fn list_insights(
    status: Option<&str>,
    insight_type: Option<&str>,
    symbol: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<DomainInsight>, String> {
    with_conn(|conn| {
        let mut sql = String::from("SELECT id, insight_type, symbol, content, confidence, source_verdict_ids, status, created_at, updated_at FROM domain_insights WHERE 1=1");
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(s) = status {
            sql.push_str(&format!(" AND status = ?{idx}"));
            params.push(Box::new(s.to_string()));
            idx += 1;
        }
        if let Some(t) = insight_type {
            sql.push_str(&format!(" AND insight_type = ?{idx}"));
            params.push(Box::new(t.to_string()));
            idx += 1;
        }
        if let Some(sym) = symbol {
            sql.push_str(&format!(" AND symbol = ?{idx}"));
            params.push(Box::new(sym.to_string()));
            idx += 1;
        }
        sql.push_str(" ORDER BY updated_at DESC");
        if let Some(l) = limit {
            sql.push_str(&format!(" LIMIT {l}"));
        }

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok(DomainInsight {
                id: row.get(0)?,
                insight_type: row.get(1)?,
                symbol: row.get(2)?,
                content: row.get(3)?,
                confidence: row.get(4)?,
                source_verdict_ids: row.get(5)?,
                status: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    })
}

pub fn get_active_insights_json() -> Result<String, String> {
    with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT json_group_array(json(id, insight_type, symbol, content, confidence, source_verdict_ids, status))
             FROM domain_insights WHERE status = 'active'"
        )?;
        let json: String = stmt.query_row([], |row| row.get(0))?;
        Ok(json)
    })
}

pub fn restore_insight_snapshot(json: &str) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute("DELETE FROM domain_insights WHERE status = 'active'", [])?;
        conn.execute(
            "INSERT INTO domain_insights (id, insight_type, symbol, content, confidence, source_verdict_ids, status, created_at, updated_at)
             SELECT value->>'id', value->>'insight_type', value->>'symbol', value->>'content',
                    value->>'confidence', value->>'source_verdict_ids', value->>'status',
                    datetime('now'), datetime('now')
             FROM json_each(?1)",
            [json],
        )?;
        Ok(())
    })
}
```

### Step 5.2: Create `src-tauri/src/invest/dreaming/mod.rs`

```rust
pub mod pipeline;
pub mod snapshot;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DreamConfig {
    pub invest_enabled: bool,
    pub invest_cron: String,
    pub user_memory_enabled: bool,
    pub user_memory_interval_min: i64,
    pub lookback_days: i64,
    pub min_score: f64,
    pub min_count: i64,
}

impl Default for DreamConfig {
    fn default() -> Self {
        Self {
            invest_enabled: false,
            invest_cron: "0 3 * * *".into(),
            user_memory_enabled: false,
            user_memory_interval_min: 120,
            lookback_days: 30,
            min_score: 0.8,
            min_count: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DreamResult {
    pub insights_written: usize,
    pub insights_updated: usize,
    pub insights_archived: usize,
    pub pipeline_duration_ms: i64,
    pub stages: Vec<StageResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageResult {
    pub stage: String,
    pub duration_ms: i64,
    pub items_processed: usize,
    pub items_output: usize,
}

/// Trigger the invest dreaming pipeline.
pub async fn trigger_dream(mode: &str, tushare_token: &str) -> Result<DreamResult, String> {
    match mode {
        "invest" => pipeline::run_invest_pipeline(tushare_token).await,
        _ => Err(format!("Unknown dream mode: {}", mode)),
    }
}
```

### Step 5.3: Create `src-tauri/src/invest/dreaming/pipeline.rs`

```rust
use super::{DreamResult, StageResult};
use crate::storage::invest::domain_insights::{self, DomainInsight};
use crate::storage::invest::verdicts;
use crate::tushare::TushareClient;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug)]
struct DreamCandidate {
    symbol: String,
    verdict: String,
    regime: String,
    hit_rate: f64,
    count: usize,
    avg_return_1d: f64,
    avg_return_7d: f64,
    avg_return_30d: f64,
    source_ids: Vec<String>,
}

pub async fn run_invest_pipeline(tushare_token: &str) -> Result<DreamResult, String> {
    let pipeline_start = Instant::now();
    let client = TushareClient::new(tushare_token.to_string());
    let config = super::DreamConfig::default();

    // Snapshot before (for rollback)
    let before_json = domain_insights::get_active_insights_json()?;

    // ── Light Sleep: Extract tuples ──────────────────────────────────
    let light_start = Instant::now();
    let verdicts = verdicts::list_verdicts(None, Some(200))?;
    let recent: Vec<_> = verdicts
        .iter()
        .filter(|v| {
            let Ok(dt) = chrono::NaiveDate::parse_from_str(&v.created_at[..10].replace('-', ""), "%Y%m%d") else {
                return false;
            };
            let cutoff = chrono::Local::now().naive_local().date() - chrono::Duration::days(config.lookback_days);
            dt >= cutoff
        })
        .collect();

    let tuples: Vec<(String, String, String, String, String)> = recent
        .iter()
        .map(|v| {
            let regime = v.macro_signal.clone().unwrap_or_else(|| "unknown".into());
            (v.symbol.clone(), v.verdict.clone(), v.confidence.unwrap_or(0.5).to_string(), v.created_at[..10].to_string(), regime)
        })
        .collect();

    let light_dur = light_start.elapsed().as_millis() as i64;

    // ── REM Sleep: Aggregate hit rates ───────────────────────────────
    let rem_start = Instant::now();
    let mut groups: HashMap<(String, String, String), Vec<&(String, String, String, String, String)>> = HashMap::new();
    for t in &tuples {
        let key = (t.0.clone(), t.1.clone(), t.4.clone());
        groups.entry(key).or_default().push(t);
    }

    let mut candidates: Vec<DreamCandidate> = Vec::new();

    for ((symbol, verdict, regime), group) in &groups {
        if group.len() < config.min_count as usize {
            continue;
        }

        // Fetch prices for this symbol
        let first_date = group.iter().map(|t| &t.3).min().cloned().unwrap_or_default();
        let end_date = chrono::Local::now().format("%Y%m%d").to_string();
        let Ok(bars) = client.daily(symbol, &first_date.replace('-', ""), &end_date).await else {
            continue;
        };
        if bars.is_empty() {
            continue;
        }

        let mut returns_1d = Vec::new();
        let mut returns_7d = Vec::new();
        let mut returns_30d = Vec::new();

        for t in group {
            let date_str = t.3.replace('-', "");
            let Some(vbar) = bars.iter().find(|b| b.trade_date == date_str) else {
                continue;
            };
            let price = vbar.close;

            for (days, returns) in [(1, &mut returns_1d), (7, &mut returns_7d), (30, &mut returns_30d)] {
                let target = (chrono::NaiveDate::parse_from_str(&date_str, "%Y%m%d").unwrap()
                    + chrono::Duration::days(days))
                    .format("%Y%m%d")
                    .to_string();
                if let Some(abar) = bars.iter().find(|b| b.trade_date >= target) {
                    returns.push((abar.close - price) / price);
                }
            }
        }

        // Simple hit rate: return > 0 for BUY/ACCUMULATE, < 0 for SELL/TRIM
        let is_bullish = matches!(verdict.as_str(), "BUY" | "ACCUMULATE");
        let hit_count = returns_30d.iter().filter(|r| if is_bullish { **r > 0.0 } else { **r < 0.0 }).count();
        let total = returns_30d.len();
        if total == 0 {
            continue;
        }
        let hit_rate = hit_count as f64 / total as f64;
        let score = hit_rate * 0.7 + (total.min(10) as f64 / 10.0) * 0.3;

        if score >= config.min_score {
            let source_ids: Vec<String> = group.iter().filter_map(|t| {
                recent.iter().find(|v| v.symbol == t.0 && v.created_at.starts_with(&t.3)).map(|v| v.id.clone())
            }).collect();

            candidates.push(DreamCandidate {
                symbol: symbol.clone(),
                verdict: verdict.clone(),
                regime: regime.clone(),
                hit_rate,
                count: total,
                avg_return_1d: returns_1d.iter().sum::<f64>() / returns_1d.len().max(1) as f64,
                avg_return_7d: returns_7d.iter().sum::<f64>() / returns_7d.len().max(1) as f64,
                avg_return_30d: returns_30d.iter().sum::<f64>() / returns_30d.len().max(1) as f64,
                source_ids,
            });
        }
    }
    let rem_dur = rem_start.elapsed().as_millis() as i64;

    // ── Deep Sleep: Write insights ───────────────────────────────────
    let deep_start = Instant::now();
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let mut written = 0usize;

    for c in &candidates {
        let content = format!(
            "{} {} in {}: {:.0}% hit rate over {} samples (1d/7d/30d: {:.1}%/{:.1}%/{:.1}%)",
            c.symbol, c.verdict, c.regime,
            c.hit_rate * 100.0, c.count,
            c.avg_return_1d * 100.0, c.avg_return_7d * 100.0, c.avg_return_30d * 100.0,
        );
        let insight = DomainInsight {
            id: format!("dream_{}_{}_{}", c.symbol, c.verdict, c.regime).replace(' ', "_"),
            insight_type: "pattern".into(),
            symbol: Some(c.symbol.clone()),
            content,
            confidence: Some(c.hit_rate),
            source_verdict_ids: Some(c.source_ids.join(",")),
            status: "active".into(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        domain_insights::upsert_insight(&insight)?;
        written += 1;
    }

    // Snapshot after
    let after_json = domain_insights::get_active_insights_json()?;
    let summary = format!("{} insights written, {} candidates found", written, candidates.len());

    // Save snapshot record
    super::snapshot::save_snapshot("invest", "manual", &before_json, &after_json, &summary)?;

    let deep_dur = deep_start.elapsed().as_millis() as i64;
    let total_dur = pipeline_start.elapsed().as_millis() as i64;

    Ok(DreamResult {
        insights_written: written,
        insights_updated: 0,
        insights_archived: 0,
        pipeline_duration_ms: total_dur,
        stages: vec![
            StageResult { stage: "light".into(), duration_ms: light_dur, items_processed: recent.len(), items_output: tuples.len() },
            StageResult { stage: "rem".into(), duration_ms: rem_dur, items_processed: groups.len(), items_output: candidates.len() },
            StageResult { stage: "deep".into(), duration_ms: deep_dur, items_processed: candidates.len(), items_output: written },
        ],
    })
}
```

### Step 5.4: Register modules

In `src-tauri/src/invest/mod.rs`:

```rust
pub mod dreaming;
```

In `src-tauri/src/storage/invest/mod.rs`:

```rust
pub mod domain_insights;
```

### Step 5.5: Verify compilation

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30
```

### Step 5.6: Commit

```bash
git add src-tauri/src/invest/dreaming/ src-tauri/src/storage/invest/domain_insights.rs src-tauri/src/invest/mod.rs src-tauri/src/storage/invest/mod.rs
git commit -m "feat(invest): add Dreaming invest pipeline (Light→REM→Deep) (Task 5)"
```

---

## Task 6: Dreaming Snapshots + Rollback Storage

**Files:**
- Create: `src-tauri/src/invest/dreaming/snapshot.rs`
- Create: `src-tauri/src/storage/invest/dream_snapshots.rs`
- Modify: `src-tauri/src/storage/invest/mod.rs` (add migration)

### Step 6.1: Create `src-tauri/src/storage/invest/dream_snapshots.rs`

```rust
use crate::storage::invest::{with_conn, with_conn_mut};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DreamSnapshot {
    pub id: i64,
    pub dream_type: String,
    pub trigger_type: String,
    pub before_json: String,
    pub after_json: Option<String>,
    pub status: String,
    pub summary: Option<String>,
    pub rollback_ready: bool,
    pub created_at: String,
}

pub fn create_table_if_not_exists() -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS dream_snapshots (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                dream_type      TEXT NOT NULL,
                trigger_type    TEXT NOT NULL,
                before_json     TEXT NOT NULL,
                after_json      TEXT,
                status          TEXT NOT NULL DEFAULT 'pending',
                summary         TEXT,
                rollback_ready  INTEGER NOT NULL DEFAULT 0,
                created_at      TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_ds_type ON dream_snapshots(dream_type);
            CREATE INDEX IF NOT EXISTS idx_ds_status ON dream_snapshots(status);"
        )?;
        Ok(())
    })
}

pub fn insert_pending(dream_type: &str, trigger_type: &str, before_json: &str) -> Result<i64, String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO dream_snapshots (dream_type, trigger_type, before_json, status, created_at)
             VALUES (?1, ?2, ?3, 'pending', datetime('now'))",
            rusqlite::params![dream_type, trigger_type, before_json],
        )?;
        Ok(conn.last_insert_rowid())
    })
}

pub fn complete_snapshot(id: i64, after_json: &str, summary: &str) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "UPDATE dream_snapshots SET after_json = ?1, status = 'completed', summary = ?2, rollback_ready = 1 WHERE id = ?3",
            rusqlite::params![after_json, summary, id],
        )?;
        Ok(())
    })
}

pub fn list_snapshots(dream_type: Option<&str>, limit: Option<i64>) -> Result<Vec<DreamSnapshot>, String> {
    with_conn(|conn| {
        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match dream_type {
            Some(t) => (
                "SELECT * FROM dream_snapshots WHERE dream_type = ?1 ORDER BY created_at DESC LIMIT ?2".into(),
                vec![Box::new(t.to_string()), Box::new(limit.unwrap_or(20))],
            ),
            None => (
                "SELECT * FROM dream_snapshots ORDER BY created_at DESC LIMIT ?1".into(),
                vec![Box::new(limit.unwrap_or(20))],
            ),
        };
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok(DreamSnapshot {
                id: row.get(0)?,
                dream_type: row.get(1)?,
                trigger_type: row.get(2)?,
                before_json: row.get(3)?,
                after_json: row.get(4)?,
                status: row.get(5)?,
                summary: row.get(6)?,
                rollback_ready: row.get::<_, i32>(7)? != 0,
                created_at: row.get(8)?,
            })
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    })
}

pub fn mark_rolled_back(id: i64) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "UPDATE dream_snapshots SET status = 'rolled_back', rollback_ready = 0 WHERE id = ?1",
            [id],
        )?;
        Ok(())
    })
}
```

### Step 6.2: Create `src-tauri/src/invest/dreaming/snapshot.rs`

```rust
use crate::storage::invest::dream_snapshots;

/// Save a dream snapshot record. Called at start (pending) and end (completed).
pub fn save_snapshot(
    dream_type: &str,
    trigger_type: &str,
    before_json: &str,
    after_json: &str,
    summary: &str,
) -> Result<i64, String> {
    let id = dream_snapshots::insert_pending(dream_type, trigger_type, before_json)?;
    dream_snapshots::complete_snapshot(id, after_json, summary)?;
    Ok(id)
}

/// Rollback a dream snapshot: restore domain_insights to before state.
pub fn rollback_snapshot(snapshot_id: i64) -> Result<(), String> {
    let snapshots = dream_snapshots::list_snapshots(None, Some(100))?;
    let snapshot = snapshots
        .iter()
        .find(|s| s.id == snapshot_id)
        .ok_or("Snapshot not found")?;

    if !snapshot.rollback_ready {
        return Err("Snapshot is not rollback-ready".into());
    }

    // Verify current state matches after_json (if available)
    if let Some(after) = &snapshot.after_json {
        let current = crate::storage::invest::domain_insights::get_active_insights_json()?;
        if &current != after {
            // State has been modified since this dream — invalidate
            dream_snapshots::mark_rolled_back(snapshot_id)?;
            return Err("Current domain_insights state has changed since this dream. Rollback aborted.".into());
        }
    }

    // Restore
    crate::storage::invest::domain_insights::restore_insight_snapshot(&snapshot.before_json)?;
    dream_snapshots::mark_rolled_back(snapshot_id)?;
    Ok(())
}
```

### Step 6.3: Add migration in `src-tauri/src/storage/invest/mod.rs`

In `init_db`, after existing migrations:

```rust
if let Err(e) = crate::storage::invest::dream_snapshots::create_table_if_not_exists() {
    log::warn!("Failed to create dream_snapshots: {}", e);
}
```

Add module:

```rust
pub mod dream_snapshots;
```

### Step 6.4: Commit

```bash
git add src-tauri/src/invest/dreaming/snapshot.rs src-tauri/src/storage/invest/dream_snapshots.rs src-tauri/src/storage/invest/mod.rs
git commit -m "feat(invest): add Dreaming snapshot + rollback storage (Task 6)"
```

---

## Task 7: Dreaming — Tauri Commands + Config UI

**Files:**
- Modify: `src-tauri/src/commands/invest.rs` (add 5 commands)
- Modify: `src-tauri/src/lib.rs` (register commands)
- Create: `src/lib/components/invest/DreamingConfigPanel.svelte`
- Modify: `src/lib/stores/invest-store.svelte.ts` (add dreaming state)

### Step 7.1: Add Tauri commands to `src-tauri/src/commands/invest.rs`

```rust
// ── Dreaming commands ────────────────────────────────────────────────

#[tauri::command]
pub async fn trigger_dream(mode: String, tushare_token: String) -> Result<crate::invest::dreaming::DreamResult, String> {
    crate::invest::dreaming::trigger_dream(&mode, &tushare_token).await
}

#[tauri::command]
pub fn get_dream_config() -> Result<crate::invest::dreaming::DreamConfig, String> {
    // Read from scheduler.json dream section or use defaults
    Ok(crate::invest::dreaming::DreamConfig::default())
}

#[tauri::command]
pub fn save_dream_config(config: crate::invest::dreaming::DreamConfig) -> Result<(), String> {
    // Persist to scheduler.json dream section
    // For now, update the dream_user and dream_invest job configs
    let mut jobs = crate::invest::scheduler::config::load_jobs();
    if let Some(j) = jobs.iter_mut().find(|j| j.id == "dream_user") {
        j.enabled = config.user_memory_enabled;
        j.interval_min = Some(config.user_memory_interval_min);
    }
    if let Some(j) = jobs.iter_mut().find(|j| j.id == "dream_invest") {
        j.enabled = config.invest_enabled;
        j.cron_expr = config.invest_cron.clone();
    }
    crate::invest::scheduler::config::save_jobs(&jobs)
}

#[tauri::command]
pub fn list_dream_traces(dream_type: Option<String>, limit: Option<i64>) -> Result<Vec<crate::storage::invest::dream_snapshots::DreamSnapshot>, String> {
    crate::storage::invest::dream_snapshots::list_snapshots(dream_type.as_deref(), limit)
}

#[tauri::command]
pub fn rollback_dream(snapshot_id: i64) -> Result<(), String> {
    crate::invest::dreaming::snapshot::rollback_snapshot(snapshot_id)
}
```

### Step 7.2: Register commands in `lib.rs`

```rust
commands::invest::trigger_dream,
commands::invest::get_dream_config,
commands::invest::save_dream_config,
commands::invest::list_dream_traces,
commands::invest::rollback_dream,
```

### Step 7.3: Create `src/lib/components/invest/DreamingConfigPanel.svelte`

```svelte
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { invoke } from '$lib/transport';

  interface DreamConfig {
    investEnabled: boolean;
    investCron: string;
    userMemoryEnabled: boolean;
    userMemoryIntervalMin: number;
    lookbackDays: number;
    minScore: number;
    minCount: number;
  }

  interface DreamTrace {
    id: number;
    dreamType: string;
    triggerType: string;
    status: string;
    summary?: string;
    rollbackReady: boolean;
    createdAt: string;
  }

  interface DreamResult {
    insightsWritten: number;
    pipelineDurationMs: number;
    stages: { stage: string; durationMs: number; itemsProcessed: number; itemsOutput: number }[];
  }

  let config = $state<DreamConfig | null>(null);
  let traces = $state<DreamTrace[]>([]);
  let loading = $state(false);
  let running = $state(false);
  let lastResult = $state<DreamResult | null>(null);
  let error = $state<string | null>(null);

  // Which path: 'invest' or 'user_memory'
  let { path = 'invest' }: { path: 'invest' | 'user_memory' } = $props();

  async function loadConfig() {
    config = await invoke<DreamConfig>('get_dream_config');
  }

  async function loadTraces() {
    traces = await invoke<DreamTrace[]>('list_dream_traces', { dreamType: path, limit: 10 });
  }

  async function saveConfig() {
    if (!config) return;
    await invoke('save_dream_config', { config });
  }

  async function triggerDream() {
    running = true;
    error = null;
    try {
      const settings = await invoke<{ tushareToken?: string }>('get_user_settings');
      if (!settings.tushareToken) {
        error = 'No Tushare token';
        return;
      }
      lastResult = await invoke<DreamResult>('trigger_dream', { mode: path, tushareToken: settings.tushareToken });
      await loadTraces();
    } catch (e) {
      error = String(e);
    } finally {
      running = false;
    }
  }

  async function rollback(trace: DreamTrace) {
    if (!confirm(t('invest.dreaming.rollbackConfirm'))) return;
    try {
      await invoke('rollback_dream', { snapshotId: trace.id });
      await loadTraces();
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => { loadConfig(); loadTraces(); });
</script>

<div class="space-y-4">
  {#if error}
    <div class="rounded border border-red-300 bg-red-50 p-3 text-sm text-red-700 dark:bg-red-950 dark:text-red-300">
      {error}
    </div>
  {/if}

  <!-- Config -->
  {#if config}
    <div class="rounded-lg border p-4 space-y-3">
      <h4 class="text-sm font-medium">{t('invest.dreaming.config')}</h4>
      {#if path === 'invest'}
        <label class="flex items-center gap-2 text-sm">
          <input type="checkbox" bind:checked={config.investEnabled} onchange={saveConfig} />
          {t('invest.dreaming.investPath')} enabled
        </label>
        <div class="flex items-center gap-2 text-sm">
          <span>Cron:</span>
          <input class="w-40 rounded border bg-background px-2 py-1 text-xs font-mono"
            bind:value={config.investCron} onchange={saveConfig} />
        </div>
      {:else}
        <label class="flex items-center gap-2 text-sm">
          <input type="checkbox" bind:checked={config.userMemoryEnabled} onchange={saveConfig} />
          {t('invest.dreaming.userMemoryPath')} enabled
        </label>
        <div class="flex items-center gap-2 text-sm">
          <span>Interval (min):</span>
          <input type="number" class="w-20 rounded border bg-background px-2 py-1 text-xs"
            bind:value={config.userMemoryIntervalMin} onchange={saveConfig} />
        </div>
      {/if}
      <div class="flex items-center gap-2 text-sm">
        <span>Lookback days:</span>
        <input type="number" class="w-20 rounded border bg-background px-2 py-1 text-xs"
          bind:value={config.lookbackDays} onchange={saveConfig} />
      </div>
      <div class="flex items-center gap-2 text-sm">
        <span>Min score:</span>
        <input type="number" step="0.05" class="w-20 rounded border bg-background px-2 py-1 text-xs"
          bind:value={config.minScore} onchange={saveConfig} />
      </div>
    </div>
  {/if}

  <!-- Trigger -->
  <button
    class="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
    disabled={running}
    onclick={triggerDream}
  >
    {running ? '...' : t('invest.dreaming.trigger')}
  </button>

  {#if lastResult}
    <div class="rounded border p-3 text-xs space-y-1">
      <div>Written: {lastResult.insightsWritten} insights in {lastResult.pipelineDurationMs}ms</div>
      {#each lastResult.stages as s}
        <div class="text-muted-foreground">{s.stage}: {s.itemsProcessed}→{s.itemsOutput} ({s.durationMs}ms)</div>
      {/each}
    </div>
  {/if}

  <!-- Traces -->
  <div class="rounded-lg border">
    <div class="border-b bg-muted/50 px-4 py-2 text-sm font-medium">{t('invest.dreaming.trace')}</div>
    {#if traces.length === 0}
      <p class="p-4 text-xs text-muted-foreground">No traces yet</p>
    {:else}
      <div class="divide-y">
        {#each traces as tr}
          <div class="flex items-center gap-3 px-4 py-2 text-xs">
            <span class="text-muted-foreground">{new Date(tr.createdAt).toLocaleString()}</span>
            <span class="rounded px-1.5 py-0.5 text-xs {tr.status === 'completed' ? 'bg-green-100 text-green-700' : tr.status === 'rolled_back' ? 'bg-yellow-100 text-yellow-700' : 'bg-muted'}">{tr.status}</span>
            <span class="truncate flex-1">{tr.summary || ''}</span>
            {#if tr.rollbackReady}
              <button class="text-xs text-red-500 hover:underline" onclick={() => rollback(tr)}>
                {t('invest.dreaming.rollback')}
              </button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
```

### Step 7.4: Commit

```bash
git add src-tauri/src/commands/invest.rs src-tauri/src/lib.rs src/lib/components/invest/DreamingConfigPanel.svelte
git commit -m "feat(invest): add Dreaming commands and config UI (Task 7)"
```

---

## Task 8: FTS5 Upgrade for domain_insights

**Files:**
- Modify: `src-tauri/src/storage/invest/mod.rs` (add FTS5 migration)
- Modify: `src-tauri/src/invest/committee/tools.rs` (upgrade exec_dreaming_insights)

### Step 8.1: Add FTS5 migration in `src-tauri/src/storage/invest/mod.rs`

In the `init_db` function, add after existing table creations:

```rust
// FTS5 for domain_insights
conn.execute_batch(
    "CREATE VIRTUAL TABLE IF NOT EXISTS domain_insights_fts USING fts5(
        content, symbol, insight_type,
        content='domain_insights',
        content_rowid='rowid'
    );

    CREATE TRIGGER IF NOT EXISTS domain_insights_ai AFTER INSERT ON domain_insights BEGIN
        INSERT INTO domain_insights_fts(rowid, content, symbol, insight_type)
        VALUES (new.rowid, new.content, new.symbol, new.insight_type);
    END;

    CREATE TRIGGER IF NOT EXISTS domain_insights_ad AFTER DELETE ON domain_insights BEGIN
        INSERT INTO domain_insights_fts(domain_insights_fts, rowid, content, symbol, insight_type)
        VALUES ('delete', old.rowid, old.content, old.symbol, old.insight_type);
    END;

    CREATE TRIGGER IF NOT EXISTS domain_insights_au AFTER UPDATE ON domain_insights BEGIN
        INSERT INTO domain_insights_fts(domain_insights_fts, rowid, content, symbol, insight_type)
        VALUES ('delete', old.rowid, old.content, old.symbol, old.insight_type);
        INSERT INTO domain_insights_fts(rowid, content, symbol, insight_type)
        VALUES (new.rowid, new.content, new.symbol, new.insight_type);
    END;"
).map_err(|e| format!("FTS5 migration: {e}"))?;

// Rebuild FTS index from existing data
conn.execute(
    "INSERT INTO domain_insights_fts(domain_insights_fts) VALUES('rebuild')",
    [],
).map_err(|e| format!("FTS5 rebuild: {e}"))?;
```

Note: The `rebuild` command populates the FTS index from existing rows. Only needed once (idempotent).

### Step 8.2: Upgrade `exec_dreaming_insights` in `src-tauri/src/invest/committee/tools.rs`

Replace the LIKE query (around line 298) with FTS5 MATCH:

```rust
fn exec_dreaming_insights(query: &str, limit: usize) -> Result<String, String> {
    use crate::storage::invest::with_conn;
    let results: Vec<String> = with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT di.content, di.created_at FROM domain_insights di
             JOIN domain_insights_fts fts ON di.rowid = fts.rowid
             WHERE domain_insights_fts MATCH ?1 AND di.status = 'active'
             ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![query, limit as i64], |row| {
            let content: String = row.get(0)?;
            let created: String = row.get(1)?;
            Ok(format!("[{}] {}", created, content))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })?;

    if results.is_empty() {
        Ok("No matching insights found.".into())
    } else {
        Ok(results.join("\n"))
    }
}
```

Key changes:
- `LIKE ?1` → `JOIN domain_insights_fts ... WHERE domain_insights_fts MATCH ?1`
- Added `AND di.status = 'active'` filter (was missing before)
- Removed `format!("%{}%", query)` — FTS5 MATCH uses the query directly

### Step 8.3: Verify compilation

```bash
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -20
```

### Step 8.4: Commit

```bash
git add src-tauri/src/storage/invest/mod.rs src-tauri/src/invest/committee/tools.rs
git commit -m "feat(invest): upgrade domain_insights to FTS5 full-text search (Task 8)"
```

---

## Task 9: Archived Memory View

**Files:**
- Modify: `src-tauri/src/commands/memos.rs` (add restore_memory)
- Create: `src/lib/components/invest/ArchivedMemoriesTab.svelte`
- Modify: `src/routes/memory-mgmt/+page.svelte` (add archived tab)

### Step 9.1: Add `restore_memory` command to `src-tauri/src/commands/memos.rs`

```rust
#[tauri::command]
pub fn restore_memory(id: String) -> Result<(), String> {
    crate::storage::memory_store::with_conn_mut(|conn| {
        let updated = conn.execute(
            "UPDATE memories SET status = 'approved', confidence = 1.0, updated_at = datetime('now') WHERE id = ?1 AND status = 'archived'",
            [&id],
        )?;
        if updated == 0 {
            return Err("Memory not found or not archived".to_string());
        }
        Ok(())
    })
}
```

### Step 9.2: Register in `lib.rs`

```rust
commands::memos::restore_memory,
```

### Step 9.3: Create `src/lib/components/invest/ArchivedMemoriesTab.svelte`

```svelte
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { invoke } from '$lib/transport';

  interface Memory {
    id: string;
    content: string;
    memoryType: string;
    confidence: number;
    scope: string;
    status: string;
    updatedAt: string;
  }

  let memories = $state<Memory[]>([]);
  let scopeFilter = $state<string>('all');
  let loading = $state(false);

  async function load() {
    loading = true;
    try {
      const scope = scopeFilter === 'all' ? undefined : scopeFilter;
      memories = await invoke<Memory[]>('list_memories', {
        statusFilter: 'archived',
        memoryTypeFilter: undefined,
        scopeFilter: scope,
        limit: 100,
        offset: 0,
      });
    } finally {
      loading = false;
    }
  }

  async function restore(id: string) {
    await invoke('restore_memory', { id });
    await load();
  }

  async function deletePermanent(id: string) {
    // Set status to 'deleted' — not a hard delete
    await invoke('delete_memory', { id });
    await load();
  }

  function scopeLabel(scope: string): string {
    return scope === 'global' ? 'G' : scope === 'invest' ? 'I' : scope === 'project' ? 'P' : scope[0].toUpperCase();
  }

  function scopeColor(scope: string): string {
    return scope === 'global' ? 'bg-blue-100 text-blue-700' : scope === 'invest' ? 'bg-purple-100 text-purple-700' : 'bg-gray-100 text-gray-700';
  }

  $effect(() => { load(); });
</script>

<div class="space-y-4">
  <div class="flex items-center justify-between">
    <h3 class="text-lg font-semibold">{t('memory_mgmt.archived.title')}</h3>
    <div class="flex gap-1">
      {#each ['all', 'global', 'project', 'invest'] as s}
        <button
          class="rounded px-2 py-1 text-xs {scopeFilter === s ? 'bg-primary text-primary-foreground' : 'bg-muted hover:bg-muted/80'}"
          onclick={() => { scopeFilter = s; load(); }}
        >{s === 'all' ? t('common.all') : s}</button>
      {/each}
    </div>
  </div>

  {#if loading}
    <p class="text-muted-foreground">{t('common.loading')}</p>
  {:else if memories.length === 0}
    <div class="flex h-32 items-center justify-center">
      <p class="text-muted-foreground">{t('memory_mgmt.archived.empty')}</p>
    </div>
  {:else}
    <div class="space-y-2">
      {#each memories as mem}
        <div class="rounded-lg border p-3">
          <div class="flex items-start justify-between gap-2">
            <div class="flex-1">
              <div class="flex items-center gap-2 mb-1">
                <span class="rounded px-1.5 py-0.5 text-xs {scopeColor(mem.scope)}">{scopeLabel(mem.scope)}</span>
                <span class="rounded bg-muted px-1.5 py-0.5 text-xs">{mem.memoryType}</span>
                <span class="text-xs text-muted-foreground">{(mem.confidence * 100).toFixed(0)}%</span>
              </div>
              <p class="text-sm">{mem.content}</p>
              <p class="mt-1 text-xs text-muted-foreground">
                Archived: {new Date(mem.updatedAt).toLocaleString()}
              </p>
            </div>
            <div class="flex gap-1">
              <button
                class="rounded px-2 py-1 text-xs text-green-600 hover:bg-green-50"
                onclick={() => restore(mem.id)}
              >{t('memory_mgmt.archived.restore')}</button>
              <button
                class="rounded px-2 py-1 text-xs text-red-600 hover:bg-red-50"
                onclick={() => deletePermanent(mem.id)}
              >{t('memory_mgmt.archived.deletePermanent')}</button>
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
```

### Step 9.4: Add "已归档" tab to `src/routes/memory-mgmt/+page.svelte`

Update the MemTab type and tabs array:

```typescript
type MemTab = "userMemory" | "extractionConfig" | "archived";
const tabs: { id: MemTab; label: string }[] = $derived([
    { id: "userMemory", label: t("memoryMgmt_tab_userMemory") },
    { id: "extractionConfig", label: t("memoryMgmt_tab_extractionConfig") },
    { id: "archived", label: t("memory_mgmt.archived.title") },
]);
```

Add the archived tab content (after existing tab content blocks):

```svelte
{:else if activeTab === 'archived'}
    <ArchivedMemoriesTab />
```

Import at the top:

```typescript
import ArchivedMemoriesTab from '$lib/components/invest/ArchivedMemoriesTab.svelte';
```

### Step 9.5: Commit

```bash
git add src-tauri/src/commands/memos.rs src-tauri/src/lib.rs src/lib/components/invest/ArchivedMemoriesTab.svelte src/routes/memory-mgmt/+page.svelte
git commit -m "feat: add archived memory view with restore/delete (Task 9)"
```

---

## Task 10: i18n Keys + Integration Polish

**Files:**
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`
- Modify: `src-tauri/src/lib.rs` (wire scheduler start with dispatch)

### Step 10.1: Add i18n keys to `messages/en.json`

Add these keys (within the existing JSON object, near other `invest.*` keys):

```json
"invest.scheduler.title": "Scheduled Tasks",
"invest.scheduler.jobName": "Job",
"invest.scheduler.cronExpr": "Schedule",
"invest.scheduler.lastRun": "Last Run",
"invest.scheduler.nextRun": "Next Run",
"invest.scheduler.runNow": "Run Now",
"invest.scheduler.viewLogs": "Logs",
"invest.scheduler.editSchedule": "Edit",
"invest.scheduler.status.ok": "OK",
"invest.scheduler.status.error": "Error",
"invest.scheduler.status.skipped": "Skipped",
"invest.scheduler.status.running": "Running",

"invest.accuracy.title": "Verdict Accuracy",
"invest.accuracy.totalVerdicts": "Total Verdicts",
"invest.accuracy.overallHitRate": "Overall Hit Rate",
"invest.accuracy.directionalHitRate": "Directional Hit Rate",
"invest.accuracy.honestyBanner": "HOLD verdicts naturally inflate hit rates. Directional accuracy (BUY/SELL only) is the real alpha metric.",
"invest.accuracy.byWindow": "By Time Window",
"invest.accuracy.byVerdict": "By Verdict Type",
"invest.accuracy.runReview": "Run Review",
"invest.accuracy.noData": "No verdict data yet. Run the committee to generate verdicts.",

"invest.dreaming.title": "Dreaming Pipeline",
"invest.dreaming.investPath": "Invest Path",
"invest.dreaming.userMemoryPath": "User Memory Path",
"invest.dreaming.trigger": "Run Dream",
"invest.dreaming.rollback": "Rollback",
"invest.dreaming.rollbackConfirm": "This will restore domain_insights to the state before this dream. Continue?",
"invest.dreaming.config": "Configuration",
"invest.dreaming.trace": "Dream Traces",
"invest.dreaming.score": "Score",
"invest.dreaming.count": "Samples",

"memory_mgmt.archived.title": "Archived",
"memory_mgmt.archived.restore": "Restore",
"memory_mgmt.archived.deletePermanent": "Delete",
"memory_mgmt.archived.empty": "No archived memories"
```

### Step 10.2: Add i18n keys to `messages/zh-CN.json`

```json
"invest.scheduler.title": "定时任务",
"invest.scheduler.jobName": "任务",
"invest.scheduler.cronExpr": "调度",
"invest.scheduler.lastRun": "上次运行",
"invest.scheduler.nextRun": "下次运行",
"invest.scheduler.runNow": "立即运行",
"invest.scheduler.viewLogs": "日志",
"invest.scheduler.editSchedule": "编辑",
"invest.scheduler.status.ok": "成功",
"invest.scheduler.status.error": "失败",
"invest.scheduler.status.skipped": "跳过",
"invest.scheduler.status.running": "运行中",

"invest.accuracy.title": "裁决命中率",
"invest.accuracy.totalVerdicts": "总裁决数",
"invest.accuracy.overallHitRate": "总体命中率",
"invest.accuracy.directionalHitRate": "方向命中率",
"invest.accuracy.honestyBanner": "HOLD 裁决天然有高命中率。方向命中率（仅 BUY/SELL）才是真正的 alpha 指标。",
"invest.accuracy.byWindow": "按时间窗口",
"invest.accuracy.byVerdict": "按裁决类型",
"invest.accuracy.runReview": "运行审查",
"invest.accuracy.noData": "暂无裁决数据。运行委员会生成裁决。",

"invest.dreaming.title": "Dreaming 管线",
"invest.dreaming.investPath": "投资路径",
"invest.dreaming.userMemoryPath": "用户记忆路径",
"invest.dreaming.trigger": "运行 Dream",
"invest.dreaming.rollback": "回滚",
"invest.dreaming.rollbackConfirm": "将 domain_insights 恢复到此次 dream 之前的状态。继续？",
"invest.dreaming.config": "配置",
"invest.dreaming.trace": "Dream 记录",
"invest.dreaming.score": "得分",
"invest.dreaming.count": "样本数",

"memory_mgmt.archived.title": "已归档",
"memory_mgmt.archived.restore": "恢复",
"memory_mgmt.archived.deletePermanent": "删除",
"memory_mgmt.archived.empty": "没有已归档的记忆"
```

### Step 10.3: Wire scheduler dispatch in `lib.rs`

Ensure the `scheduler::runner::start()` call in lib.rs dispatches to the correct pipeline functions. This replaces the old `spawn_event_scanner_cron()` and inline PnL snapshot cron. The dispatch closure should match the pattern shown in Task 2 Step 2.4.

### Step 10.4: Run verification

```bash
npm run i18n:check
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5
npm run check
```

Expected: all pass.

### Step 10.5: Commit

```bash
git add messages/en.json messages/zh-CN.json src-tauri/src/lib.rs
git commit -m "feat(invest): add i18n keys and wire scheduler dispatch (Task 10)"
```

---

## Self-Review Checklist

- [x] **Spec coverage:** All 7 features from the spec have corresponding tasks (Scheduler → Tasks 1-2, Verdict Review → Tasks 3-4, Dreaming → Tasks 5-7, FTS5 → Task 8, Archived View → Task 9, i18n → Task 10)
- [x] **No placeholders:** All steps contain actual code blocks or exact commands
- [x] **Type consistency:** CronJob, VerdictReviewSummary, DreamConfig, DreamResult, DreamSnapshot types are defined once and referenced consistently
- [x] **File paths:** All paths are exact and match existing repo structure
- [x] **Commands registered:** All new Tauri commands are added to `invoke_handler` in Tasks 2, 4, 7, 9
- [x] **Migrations run:** verdict_reviews, dream_snapshots, FTS5 tables created in init_db
- [x] **i18n complete:** Both en.json and zh-CN.json get all new keys

---

## Completion Status

All 10 tasks completed on 2026-05-30.

| Task | Description | Commits |
|------|-------------|---------|
| 1 | Scheduler Framework — Backend Types + Config | `feat(invest): add Scheduler framework types and config` |
| 2 | Scheduler Runner + Tauri Commands + SchedulerTab UI | `feat(invest): add Scheduler runner, commands, and SchedulerTab UI` |
| 3 | Verdict Review Pipeline — Backend | `feat(invest): add Verdict Review pipeline and storage` |
| 4 | Verdict Review — Tauri Commands + AccuracyTab UI | `feat(invest): add Verdict Review commands and AccuracyTab UI` |
| 5 | Dreaming Invest Pipeline — Backend | `feat(invest): add Dreaming invest pipeline (Light→REM→Deep)` |
| 6 | Dreaming Snapshots + Rollback Storage | `feat(invest): add Dreaming snapshot + rollback storage` |
| 7 | Dreaming — Tauri Commands + Config UI | `feat(invest): add Dreaming commands and config UI` |
| 8 | FTS5 Upgrade for domain_insights | `feat(invest): upgrade domain_insights to FTS5 full-text search` |
| 9 | Archived Memory View | `feat: add archived memory view with restore/delete` |
| 10 | i18n Keys + Integration Polish | `feat(invest): i18n keys, insights feed, pipeline notifications` |

### Initial review fixes (same day):
- Fix C1: `.sort()` on reactive arrays in CommitteeAccuracyTab
- Fix C2: Extract `aggregate_from_stored` in verdict_review.rs
- Fix I1: Add i18n keys for hardcoded English strings
- Fix M1: Add MAX_DETAIL_ENTRIES constant
- FTS5: Implement full-text search for domain_insights

### Comprehensive code review (commit `8f2ebe8`):

14 findings across 6 severity levels, all fixed:

| # | Severity | File | Finding | Fix |
|---|----------|------|---------|-----|
| 1 | CRITICAL | `dreaming/pipeline.rs:130` | Missing `to_ts_code()` conversion before `client.daily()` — Tushare expects "600519.SH" but raw symbol "600519" was passed | Added `to_ts_code(symbol)` call |
| 2 | CRITICAL | `dreaming/pipeline.rs:118` | `config.min_count as usize` on negative i64 wraps to ~18 quintillion, silently skipping all groups | Changed to `(group.len() as i64) < config.min_count` |
| 3 | CRITICAL | `scheduler/config.rs` | `JobOverride` omitted `last_run`/`last_status` — scheduler run history lost on reload | Added `last_run`/`last_status` fields with `#[serde(default)]` |
| 4 | HIGH | `storage/invest/domain_insights.rs` | ROLLBACK failure swallowed by `.ok()` — poisons connection for subsequent operations | Added `log::error!` on ROLLBACK failure |
| 5 | HIGH | `dreaming/snapshot.rs` | `mark_rolled_back()` called on divergence branch — snapshot status changed to "rolled_back" even when rollback was aborted, destroying the rollback option | Removed `mark_rolled_back` call from divergence branch |
| 6 | MEDIUM | `dreaming/pipeline.rs:99-106` | `HashMap<(&str, &str), &str>` drops duplicate verdicts for same (symbol, date) — only last ID kept | Changed to `Vec<&str>` values with `flat_map` collection |
| 7 | MEDIUM | `CommitteeAccuracyTab.svelte` | `null` hitRate renders as "0.0%" — misleading for verdicts without price data | Added `?? null` guards with "—" fallback display |
| 8 | MEDIUM | `scheduler/runner.rs` | `save_jobs` error discarded silently — job state changes lost without trace | Wrapped in `if let Err(e) = ... { log::error!(...) }` |
| 9 | MEDIUM | `SchedulerTab.svelte` | `loadJobs`/`loadLogs` had no error handling — UI shows stale data on failure | Added try/catch blocks with error state display |
| 10 | LOW | `SchedulerTab.svelte` | `$effect` fetch can resolve after component unmounts — sets state on disposed component | Added `disposed` boolean flag with cleanup guard |
| 11 | LOW | `InsightsFeed.svelte` | FTS5 `search_domain_insights` command existed but UI used client-side filtering | Wired to FTS5-backed command with 300ms debounce |
| 12 | LOW | `DreamingConfigPanel.svelte` | Hardcoded English strings ("Invest Dream", "Save Config", etc.) not i18n'd | Replaced with `t('invest_dreaming.*')` keys |
| 13 | LOW | `SchedulerTab.svelte` | Hardcoded English strings ("Last run:", "Loading jobs...", etc.) not i18n'd | Replaced with `t('invest_scheduler.*')` keys |
| 14 | LOW | `messages/*.json` | 42 missing i18n keys for dreaming and scheduler UI | Added ~30 `invest_dreaming.*` + ~12 `invest_scheduler.*` keys to both en.json and zh-CN.json |

Total: 19 commits (10 tasks + 5 initial review fixes + 1 comprehensive review fix).
