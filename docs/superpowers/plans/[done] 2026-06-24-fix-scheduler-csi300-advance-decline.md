# Fix Scheduler Cron + Replace CSI300 + Add Advance/Decline

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the broken cron scheduler (jobs never auto-fire), replace CSI 300 with Shanghai Composite in macro indicators, and add market-wide advance/decline counts.

**Architecture:** Three independent fixes in the invest subsystem. Task 1 patches the scheduler's `next_run` persistence gap so cron jobs actually fire. Task 2 renames CSI 300 indicators to Shanghai Composite across the full data pipeline (macro cache, committee prompts, datasource probe). Task 3 adds a new AkShare data source for advance/decline counts and injects them into the macro cache.

**Design decisions:**
- `roles.rs` Macro prompt market stage rules ("沪深300站上MA60…") are updated to "上证指数" — the benchmark index change is system-wide, not just data-layer.
- `parser.rs` test fixtures containing "沪深300" are NOT changed — they test regex extraction from arbitrary LLM output, and the LLM may still mention CSI 300.
- `roles.rs` test fixtures using "沪深300ETF" / "000300.SH" as asset names are NOT changed — they test prompt generation for a specific asset.
- Non-trading day behavior for new `advance_decline` API: Python returns `{}` → Rust `rpc_call` returns `Err` → `macro_refresh` logs warning and preserves stale cache. This matches the existing `market_stats` pattern.

**Tech Stack:** Rust (Tauri backend), Python (AkShare RPC bridge), SQLite (macro_cache table)

## Global Constraints

- Windows-first: no WSL/macOS assumptions.
- Conventional Commits (`feat:`, `fix:`, `chore:`).
- Run `cargo check --manifest-path src-tauri/Cargo.toml` after each task (no `cargo test` due to MSVC runtime issue §11).
- Frontend tests: `npm test` where applicable.
- Update both `en.json` and `zh-CN.json` for any new UI text.

---

## Task 1: Fix Scheduler Cron — `next_run` Persistence

**Problem:** Every tick, `config::load_jobs()` recomputes `next_run` via `compute_next_run_for_job()`, which calls `schedule.after(&now).next()` — always returning a strictly future time. Then `should_fire(now)` checks `now >= next_run`, which is always false. Compounding this, `JobOverride` (the disk schema) has no `next_run` field, so even though `persist_job_status` writes `next_run` back to the in-memory job, it is silently dropped on save.

**Files:**
- Modify: `src-tauri/src/invest/scheduler/config.rs:17-33` (add `next_run` to `JobOverride`)
- Modify: `src-tauri/src/invest/scheduler/config.rs:48-53` (change `load_jobs` to preserve persisted `next_run`)
- Modify: `src-tauri/src/invest/scheduler/config.rs:158-188` (update `save_jobs` diff logic)
- Test: `src-tauri/src/invest/scheduler/config.rs` (existing tests + new test)

**Interfaces:**
- Consumes: `CronJob.next_run`, `JobOverride` struct, `load_jobs()`, `save_jobs()`, `should_fire()`
- Produces: Same public API; internal behavior change — `next_run` now persists across ticks

- [ ] **Step 1: Add `next_run` field to `JobOverride`**

In `src-tauri/src/invest/scheduler/config.rs`, add the field to the `JobOverride` struct (around line 19):

```rust
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
    #[serde(default)]
    last_run: Option<String>,
    #[serde(default)]
    last_status: Option<String>,
    #[serde(default)]
    next_run: Option<String>,   // NEW: persist computed next_run to disk
}
```

- [ ] **Step 2: Overlay `next_run` from disk in `load_jobs_base`**

In the `load_jobs_base` function, add the `next_run` overlay alongside the existing field overlays (after line 89, inside the `if let Some(job) = jobs.iter_mut().find(...)` block):

```rust
            if let Some(ls) = ov.last_status {
                job.last_status = Some(ls);
            }
            if let Some(nr) = ov.next_run {
                job.next_run = Some(nr);
            }
```

- [ ] **Step 3: Change `load_jobs` to skip recomputation when `next_run` is already set**

Replace the current `load_jobs` function (lines 48-53) so it only computes `next_run` for jobs that don't already have one persisted:

```rust
pub fn load_jobs() -> Vec<CronJob> {
    let mut jobs = load_jobs_base();
    for job in &mut jobs {
        // Only compute next_run if not already persisted from disk.
        // Previously, this always recomputed, which produced a strictly-future
        // time that caused should_fire() to always return false.
        if job.next_run.is_none() {
            job.next_run = compute_next_run_for_job(job);
        }
    }
    jobs
}
```

- [ ] **Step 4: Include `next_run` in `save_jobs` diff and serialization**

In `save_jobs`, update the `changed` check (around line 166) to include `next_run`:

```rust
            let changed = def.map_or(true, |d| {
                d.cron_expr != job.cron_expr
                    || d.interval_min != job.interval_min
                    || d.enabled != job.enabled
                    || d.requires_trading_day != job.requires_trading_day
                    || d.last_run != job.last_run
                    || d.last_status != job.last_status
                    || d.next_run != job.next_run
            });
```

And add `next_run` to the `JobOverride` construction (around line 175):

```rust
                Some(JobOverride {
                    id: job.id.clone(),
                    cron_expr: Some(job.cron_expr.clone()),
                    interval_min: job.interval_min,
                    enabled: Some(job.enabled),
                    requires_trading_day: Some(job.requires_trading_day),
                    last_run: job.last_run.clone(),
                    last_status: job.last_status.clone(),
                    next_run: job.next_run.clone(),
                })
```

- [ ] **Step 5: Add a test for next_run persistence round-trip**

Add this test to the `config.rs` test module (after the existing `save_then_load_base_roundtrips_cron_override` test):

```rust
    #[test]
    fn next_run_persists_through_save_load_cycle() {
        let _t = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().expect("tempdir");
        let _env = EnvGuard {
            userprofile: std::env::var_os("USERPROFILE"),
            home: std::env::var_os("HOME"),
        };
        std::env::set_var("USERPROFILE", tmp.path());
        std::env::set_var("HOME", tmp.path());

        assert!(
            config_path().starts_with(tmp.path()),
            "test isolation failed"
        );

        let mut jobs = super::super::default_jobs();
        let pnl = jobs.iter_mut().find(|j| j.id == "pnl_snapshot").unwrap();
        pnl.next_run = Some("2026-06-25T09:30:00".into());
        save_jobs(&jobs).expect("save_jobs ok");

        let reloaded = load_jobs();
        let rp = reloaded.iter().find(|j| j.id == "pnl_snapshot").unwrap();
        assert_eq!(
            rp.next_run.as_deref(),
            Some("2026-06-25T09:30:00"),
            "next_run should survive save→load round-trip"
        );
    }
```

- [ ] **Step 6: Verify with cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: No compile errors.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/invest/scheduler/config.rs
git commit -m "fix(scheduler): persist next_run to disk so cron jobs actually fire

load_jobs() was recomputing next_run every tick via schedule.after(&now),
producing a strictly-future time that made should_fire() always return false.
JobOverride lacked a next_run field, so persist_job_status writes were silently
dropped on save. Now next_run is persisted and load_jobs only computes it when
absent."
```

**Note on `save_jobs` diff behavior:** Once a job has a persisted non-`None` `next_run`, it will always be included in the `save_jobs` diff (since the default `CronJob` has `next_run: None`). This means every fired job always serializes to disk. This is correct — `next_run` changes every time a job fires, so it must be re-persisted. The scheduler.json file will be slightly larger than before but this is negligible.

**Note on pre-existing race condition:** `persist_job_status` calls `load_jobs_base()` then `save_jobs()`. If a dedicated loop (jin10/event_analyzer) persists a status update while the main loop is dispatching, the main loop's stale snapshot could overwrite it on the next save. This is pre-existing and not worsened by this fix — the dedicated loops pass `compute_next: false` so they don't touch `next_run`.

---

## Task 2: Replace CSI 300 with Shanghai Composite

**Files:**
- Modify: `src-tauri/src/storage/invest/macro_cache.rs:14-30` (rename indicator keys)
- Modify: `src-tauri/src/invest/macro_refresh.rs:75-122` (change fetch logic)
- Modify: `src-tauri/src/tencent_quotes.rs:197-270` (generalize K-line fetcher)
- Modify: `src-tauri/src/invest/committee/tools.rs:25-27` (update labels)
- Modify: `src-tauri/src/invest/committee/cli_executor.rs:269` (update prompt text)
- Modify: `src-tauri/src/invest/committee/roles.rs:255` (update market stage rules prompt)
- Modify: `src-tauri/src/commands/invest.rs:1060-1077` (update datasource probe)
- Modify: `src-tauri/src/invest/macro_refresh.rs:1` (update doc comment count)
- Modify: `src-tauri/src/invest/macro_refresh.rs:353-361` (update test)

**Interfaces:**
- Consumes: `TushareClient::daily()`, `tencent_quotes::fetch_index_kline()`
- Produces: New indicator keys `sh_composite_close` and `sh_composite_vol20` replacing `csi300_close` and `csi300_vol20`

- [ ] **Step 1: Rename indicators in `macro_cache.rs`**

In `src-tauri/src/storage/invest/macro_cache.rs`, update `ALL_INDICATORS` (lines 14-30):

```rust
pub const ALL_INDICATORS: &[&str] = &[
    "sh_composite_close",
    "sh_composite_vol20",
    "northbound_net",
    "margin_balance",
    "shibor_on",
    "cgb_10y",
    "vix",
    "tnx",
    "dxy",
    "gold",
    "oil",
    "usdcny",
    "limit_up_count",
    "limit_down_count",
    "two_market_volume",
    "advance_count",
    "decline_count",
];
```

Note: `advance_count` and `decline_count` are added here preemptively for Task 3. The test `test_all_indicators_count` will need updating.

- [ ] **Step 2: Generalize `fetch_csi300_kline` to `fetch_index_kline` in `tencent_quotes.rs`**

Replace the CSI300-specific function with a generic one. Rename `Csi300KlineResult` to `IndexKlineResult` and `fetch_csi300_kline` to `fetch_index_kline`, parameterized by symbol.

Replace lines 197-270 in `src-tauri/src/tencent_quotes.rs`:

```rust
// ---------------------------------------------------------------------------
// Index K-line + 20-day volatility
// ---------------------------------------------------------------------------

/// Index K-line result with latest close and 20-day annualized volatility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexKlineResult {
    pub close: f64,
    /// 20-day annualized volatility (percent), e.g. 19.96.
    pub vol20: Option<f64>,
}

/// Fetch any index daily K-line from Tencent Finance and compute 20-day volatility.
///
/// Uses the `web.ifzq.gtimg.cn` K-line API (same endpoint used by the web chart).
/// `symbol` is the Tencent format, e.g. `"sh000001"` for Shanghai Composite.
/// `days` controls the lookback window (25 is enough for vol20 computation).
pub async fn fetch_index_kline(
    client: &reqwest::Client,
    symbol: &str,
    days: u32,
) -> Result<IndexKlineResult, String> {
    let url = format!(
        "https://web.ifzq.gtimg.cn/appstock/app/fqkline/get?param={symbol},day,,,{days},qfq"
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("tencent kline request failed: {e}"))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("tencent kline parse failed: {e}"))?;

    // Response: {"data":{"sh000001":{"day":[[date,open,close,high,low,vol],...], "qfqday":[...]}}}
    let day_data = body
        .get("data")
        .and_then(|d| d.get(symbol))
        .and_then(|s| {
            s.get("day")
                .or_else(|| s.get("qfqday"))
                .and_then(|v| v.as_array())
        })
        .ok_or(format!("tencent kline: missing data.{symbol}.day"))?;

    if day_data.is_empty() {
        return Err("tencent kline: empty day data".into());
    }

    // Parse closing prices (index 2 in each [date, open, close, high, low, vol] array)
    let closes: Vec<f64> = day_data
        .iter()
        .filter_map(|bar| {
            bar.as_array()
                .and_then(|arr| arr.get(2))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
        })
        .filter(|&c| c > 0.0)
        .collect();

    if closes.is_empty() {
        return Err("tencent kline: no valid closing prices".into());
    }

    let latest_close = closes[0]; // newest first

    Ok(IndexKlineResult {
        close: latest_close,
        vol20: compute_vol20(&closes),
    })
}

/// Backward-compatible alias for CSI300 K-line fetch.
#[deprecated(note = "Use fetch_index_kline with symbol=\"sh000300\"")]
pub async fn fetch_csi300_kline(
    client: &reqwest::Client,
    days: u32,
) -> Result<IndexKlineResult, String> {
    fetch_index_kline(client, "sh000300", days).await
}
```

- [ ] **Step 3: Update `macro_refresh.rs` to fetch Shanghai Composite**

Replace `fetch_csi300` and `csi300_tencent_fallback` (lines 75-122) with:

```rust
/// sh_composite_close + sh_composite_vol20 from Tushare daily bars.
///
/// Falls back to Tencent Finance K-line API when Tushare fails or returns empty.
async fn fetch_sh_composite(
    client: TushareClient,
    start_date: String,
    end_date: String,
) -> MacroResult {
    match client.daily("000001.SH", &start_date, &end_date).await {
        Ok(bars) if !bars.is_empty() => {
            let latest_close = bars[0].close;
            let closes: Vec<f64> = bars.iter().take(21).map(|b| b.close).collect();
            let vol20 = crate::tencent_quotes::compute_vol20(&closes);

            let mut entries = vec![
                ("sh_composite_close".to_string(), Some(latest_close), None),
            ];
            if let Some(v) = vol20 {
                entries.push(("sh_composite_vol20".to_string(), Some(v), None));
            }
            Ok(entries)
        }
        Ok(_) => {
            log::info!("macro_refresh: sh_composite Tushare returned empty, trying Tencent fallback");
            sh_composite_tencent_fallback().await
        }
        Err(e) => {
            log::warn!("macro_refresh: sh_composite Tushare error: {e}, falling back to Tencent");
            sh_composite_tencent_fallback().await
        }
    }
}

/// Tencent Finance fallback for Shanghai Composite close + vol20.
async fn sh_composite_tencent_fallback() -> MacroResult {
    let http = reqwest::Client::new();
    let kline = crate::tencent_quotes::fetch_index_kline(&http, "sh000001", 25).await
        .map_err(|e| format!("sh_composite tencent fallback: {e}"))?;

    let mut entries = vec![
        ("sh_composite_close".to_string(), Some(kline.close), None),
    ];
    if let Some(v) = kline.vol20 {
        entries.push(("sh_composite_vol20".to_string(), Some(v), None));
    }
    Ok(entries)
}
```

Update the `tasks` vec in `refresh_macro_cache` (line 30) to call the new function:

```rust
        Box::pin(fetch_sh_composite(client.clone(), start_date.clone(), end_date.clone())),
```

Also update the module doc comment at line 6 to mention Shanghai Composite instead of CSI300.

- [ ] **Step 4: Update committee labels in `tools.rs`**

In `src-tauri/src/invest/committee/tools.rs`, update the match arms (lines 25-27):

```rust
                "sh_composite_close" => "上证指数",
                "sh_composite_vol20" => "上证指数 20日波动率",
```

- [ ] **Step 5: Update CLI executor prompt text**

In `src-tauri/src/invest/committee/cli_executor.rs`, change line 269:

```rust
         市场阶段判定中，MA60/MA20 等均线数据无法直接获取——请根据上证指数点位、\n\
```

- [ ] **Step 5b: Update Macro role market stage rules in `roles.rs`**

In `src-tauri/src/invest/committee/roles.rs`, change line 255:

```rust
- 主升：上证指数站上MA60且MA20>MA60，北向持续流入，两市成交额>1.2万亿
```

This is a semantic reference to the benchmark index in the Macro role's prompt. The market stage judgment rules should reference the same index as the data source.

- [ ] **Step 6: Update module doc comment in `macro_refresh.rs`**

Update line 1 to reflect the new indicator count:

```rust
//! Scheduler job: refresh the 17 canonical macro indicators in macro_cache.
```

Also update the data sources comment at line 6 to mention Shanghai Composite instead of CSI300.

- [ ] **Step 7: Update datasource probe in `commands/invest.rs`**

In `src-tauri/src/commands/invest.rs`, update lines 1060-1077:

```rust
    // Tencent Shanghai Composite K-line (used by macro_refresh fallback)
    match crate::tencent_quotes::fetch_index_kline(&tencent_http, "sh000001", 25).await {
        Ok(kline) => {
            sources.push(DataSourceStatus {
                name: "腾讯 上证指数 K线".into(),
                ok: true,
                last_success: Some(now_str.clone()),
                sample_value: Some(format!(
                    "close = {:.2}, vol20 = {}",
                    kline.close,
                    kline.vol20.map(|v| format!("{:.2}", v)).unwrap_or_else(|| "n/a".into())
                )),
```

Also update any error branch label similarly.

- [ ] **Step 8: Update test in `macro_refresh.rs`**

Update `test_all_indicators_count` (line 358-360) to reflect the new count (17 indicators after Task 3 adds 2):

```rust
    #[test]
    fn test_all_indicators_count() {
        // 15 original - 2 (csi300) + 2 (sh_composite) + 2 (advance/decline) = 17
        assert_eq!(macro_cache::ALL_INDICATORS.len(), 17);
    }
```

- [ ] **Step 9: Verify with cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: No compile errors. If `fetch_csi300_kline` is still referenced elsewhere (it shouldn't be after the deprecated alias), the `#[deprecated]` will produce warnings but not errors.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/storage/invest/macro_cache.rs \
        src-tauri/src/invest/macro_refresh.rs \
        src-tauri/src/tencent_quotes.rs \
        src-tauri/src/invest/committee/tools.rs \
        src-tauri/src/invest/committee/cli_executor.rs \
        src-tauri/src/invest/committee/roles.rs \
        src-tauri/src/commands/invest.rs
git commit -m "feat(invest): replace CSI 300 with Shanghai Composite in macro indicators

Rename csi300_close/csi300_vol20 to sh_composite_close/sh_composite_vol20.
Generalize fetch_csi300_kline to fetch_index_kline(symbol, days) for reuse.
Update all consumers: macro_refresh, committee prompts, CLI executor, datasource probe."
```

---

## Task 3: Add Market-Wide Advance/Decline Counts

**Files:**
- Modify: `src-tauri/python-runtime/scripts/providers/akshare_market.py` (add `market_advance_decline` function)
- Modify: `src-tauri/src/invest/international.rs` (add `AdvanceDecline` struct + fetch method)
- Modify: `src-tauri/src/invest/macro_refresh.rs` (add `fetch_advance_decline` task)
- Modify: `src-tauri/src/invest/committee/tools.rs` (add labels for new indicators)
- Modify: `src-tauri/src/invest/macro_refresh.rs:353-361` (update test count)

**Interfaces:**
- Consumes: `InternationalClient::rpc_call()`, AkShare `stock_market_activity_legu` API
- Produces: Two new macro indicators: `advance_count`, `decline_count`

- [ ] **Step 1: Add `market_advance_decline` to AkShare Python provider**

Append to `src-tauri/python-runtime/scripts/providers/akshare_market.py`:

```python
def market_advance_decline(date: str = "") -> dict:
    """Fetch market-wide advance/decline stock counts.

    Uses AkShare's stock_market_activity_legu which returns the daily
    market breadth: advancing, declining, and unchanged counts.

    Args:
        date: Trading date in "YYYYMMDD" format. Empty string = today.

    Returns: {"advance_count": int, "decline_count": int, "date": str}
    or {} on failure.
    """
    try:
        import akshare as ak
    except ImportError:
        return {}

    if not date:
        date = datetime.now().strftime("%Y%m%d")

    try:
        df = ak.stock_market_activity_legu()
        if df is None or df.empty:
            return {}

        # The returned DataFrame has columns like: 日期, 上涨家数, 下跌家数, ...
        # Filter to the requested date (format may vary; try YYYY-MM-DD and YYYYMMDD)
        date_dash = f"{date[:4]}-{date[4:6]}-{date[6:]}"
        row = df[df.iloc[:, 0].astype(str).str.contains(date) | df.iloc[:, 0].astype(str).str.contains(date_dash)]

        if row.empty:
            # If no exact match, take the latest row
            row = df.tail(1)

        r = row.iloc[0]

        # Find columns by name pattern (robust to minor naming variations)
        advance = 0
        decline = 0
        for col in df.columns:
            col_str = str(col)
            if "上涨" in col_str:
                advance = int(float(r[col]))
            elif "下跌" in col_str:
                decline = int(float(r[col]))

        if advance == 0 and decline == 0:
            return {}

        return {
            "advance_count": advance,
            "decline_count": decline,
            "date": date,
        }
    except Exception as e:
        return {}
```

**Note:** The exact column names from `ak.stock_market_activity_legu()` should be verified at implementation time. The function uses pattern matching ("上涨"/"下跌") for robustness. If the API returns empty or the columns don't match, the function returns `{}`, which causes the Rust side to return `Err` — `macro_refresh` handles this gracefully by logging a warning and preserving stale cache data (same pattern as `market_stats` on non-trading days).

- [ ] **Step 2: Add `AdvanceDecline` struct and fetch method in `international.rs`**

In `src-tauri/src/invest/international.rs`, add the struct after `MarketStats` (around line 66):

```rust
/// A-share market-wide advance/decline counts from AkShare.
#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct AdvanceDecline {
    pub advance_count: u32,
    pub decline_count: u32,
    pub date: String,
}
```

Add the fetch method after `fetch_akshare_market_stats` (around line 218):

```rust
    /// Fetch A-share market-wide advance/decline counts from AkShare.
    ///
    /// `date` is `"YYYYMMDD"` format; empty string defaults to today.
    pub async fn fetch_akshare_advance_decline(&self, date: &str) -> Result<AdvanceDecline, String> {
        self.rpc_call(
            "akshare_market.market_advance_decline",
            serde_json::json!({"date": date}),
        )
        .await
    }
```

- [ ] **Step 3: Add `fetch_advance_decline` task in `macro_refresh.rs`**

Add a new async function (after `fetch_market_stats`, around line 325):

```rust
/// Fetch market-wide advance/decline stock counts from AkShare.
///
/// Uses Python RPC bridge to call `akshare_market.market_advance_decline`.
/// Returns advance_count and decline_count as two separate entries.
async fn fetch_advance_decline() -> MacroResult {
    let client = crate::invest::international::InternationalClient::from_settings();
    let today = chrono::Local::now().format("%Y%m%d").to_string();

    let ad = client.fetch_akshare_advance_decline(&today).await
        .map_err(|e| format!("advance_decline: {e}"))?;

    Ok(vec![
        ("advance_count".to_string(), Some(ad.advance_count as f64), None),
        ("decline_count".to_string(), Some(ad.decline_count as f64), None),
    ])
}
```

Add this task to the `tasks` vec in `refresh_macro_cache` (line 37, before the closing `];`):

```rust
        Box::pin(fetch_advance_decline()),
```

- [ ] **Step 4: Add labels for new indicators in `tools.rs`**

In `src-tauri/src/invest/committee/tools.rs`, add two new match arms after `"two_market_volume"`:

```rust
                "advance_count" => "上涨家数",
                "decline_count" => "下跌家数",
```

- [ ] **Step 5: Update indicator count test**

The test in `macro_refresh.rs` should already be correct from Task 2 Step 7 (expecting 17). Verify it matches.

Also update the `test_all_indicators_count` in `macro_cache.rs` if it exists separately (it does, at line 358). The count should be 17.

- [ ] **Step 6: Verify with cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: No compile errors.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/python-runtime/scripts/providers/akshare_market.py \
        src-tauri/src/invest/international.rs \
        src-tauri/src/invest/macro_refresh.rs \
        src-tauri/src/invest/committee/tools.rs \
        src-tauri/src/storage/invest/macro_cache.rs
git commit -m "feat(invest): add market-wide advance/decline counts to macro indicators

New AkShare data source (stock_market_activity_legu) provides full-market
上涨家数/下跌家数, injected as advance_count/decline_count into macro_cache.
Committee prompts now include these in the macro snapshot."
```

---

## Task 4: Final Verification & Docs

**Files:**
- Modify: `docs/changelog.md` (add entry for this release)

- [ ] **Step 1: Run full cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: No errors.

- [ ] **Step 2: Run frontend tests**

Run: `npm test`

Expected: All tests pass.

- [ ] **Step 3: Update changelog**

Add a new version entry at the top of `docs/changelog.md` with the three changes:
1. Fix: cron scheduler jobs now auto-fire (next_run persistence)
2. Feat: replace CSI 300 with Shanghai Composite in macro indicators
3. Feat: add market-wide advance/decline counts (上涨家数/下跌家数)

- [ ] **Step 4: Commit**

```bash
git add docs/changelog.md
git commit -m "docs(changelog): scheduler fix, Shanghai Composite, advance/decline"
```
