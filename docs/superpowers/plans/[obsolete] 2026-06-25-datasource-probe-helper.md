# Extract Generic Probe Helper for `get_datasource_health`

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace 13 copy-pasted match/push probe blocks in `get_datasource_health` with a single generic `probe` helper, eliminating ~150 lines of boilerplate.

**Architecture:** A synchronous `probe(name, now, result, log_err) -> DataSourceStatus` helper inside the function. Callers pre-format `Ok(sample)` and `Err(e)` strings, then pass `Result<String, String>` to the helper. This keeps each probe's unique formatting logic at the call site while centralizing the struct construction, logging, and error handling.

**Tech Stack:** Rust, Tauri, serde

## Global Constraints

- Preserve exact runtime behavior: same `name`, `ok`, `last_success`, `sample_value` for every probe
- Tushare 行情's error path must NOT log (original behavior) — controlled via `log_err: false`
- All other error paths log via `log::warn!`
- Compile check: `cargo check --manifest-path src-tauri/Cargo.toml`

---

### Task 1: Add `probe` helper and refactor all 13 probes

**Files:**
- Modify: `src-tauri/src/commands/invest.rs:960-1288`

**Interfaces:**
- Produces: `fn probe(name: &str, now: &str, result: Result<String, String>, log_err: bool) -> DataSourceStatus` (local helper inside `get_datasource_health`)

- [ ] **Step 1: Add the `probe` helper function**

Add this as the first item inside `get_datasource_health`, after the `let mut sources = Vec::new();` line:

```rust
    // Generic probe helper: wraps Result<String, String> into DataSourceStatus.
    // Callers pre-format Ok(sample) and Err(e) before passing.
    let probe = |name: &str, now: &str, result: Result<String, String>, log_err: bool| -> DataSourceStatus {
        match result {
            Ok(sample) => DataSourceStatus {
                name: name.into(),
                ok: true,
                last_success: Some(now.into()),
                sample_value: Some(sample),
            },
            Err(e) => {
                if log_err {
                    log::warn!("[datasource] {} probe failed: {}", name, e);
                }
                DataSourceStatus {
                    name: name.into(),
                    ok: false,
                    last_success: None,
                    sample_value: Some(e),
                }
            }
        }
    };
```

- [ ] **Step 2: Refactor Tushare 行情 probe (lines 972-990)**

Replace the existing match block:

```rust
        // Tushare 行情 (stock API)
        sources.push(probe(
            "Tushare 行情",
            &now_str,
            client
                .get_latest_price("000001.SZ")
                .await
                .map(|p| format!("000001.SZ = {:.2}", p))
                .map_err(|e| format!("{e}")),
            false, // no log — token/permission issues are expected
        ));
```

- [ ] **Step 3: Refactor Tushare 新闻 probe (lines 992-1017)**

Replace the existing match block:

```rust
        // Tushare 新闻 (major_news API — different permission tier than quote)
        let today = Local::now().format("%Y%m%d").to_string();
        sources.push(probe(
            "Tushare 新闻",
            &now_str,
            client
                .major_news("sina", &today, &today)
                .await
                .map(|items| {
                    items
                        .first()
                        .map(|i| i.title.chars().take(20).collect::<String>())
                        .unwrap_or_else(|| "(empty)".into())
                }),
            true,
        ));
```

- [ ] **Step 4: Refactor Tushare no-token fallback (lines 1019-1032)**

Replace the else block:

```rust
    } else {
        sources.push(probe("Tushare 行情", &now_str, Err("no token configured".into()), false));
        sources.push(probe("Tushare 新闻", &now_str, Err("no token configured".into()), false));
    }
```

- [ ] **Step 5: Refactor 腾讯 实时行情 probe (lines 1034-1058)**

Replace the existing match block:

```rust
    // Tencent realtime quote (does not require a token; HTTP qt.gtimg.cn)
    let tencent_http = reqwest::Client::new();
    sources.push(probe(
        "腾讯 实时行情",
        &now_str,
        crate::tencent_quotes::fetch_quotes(&tencent_http, &["000001.SZ"])
            .await
            .and_then(|quotes| {
                if quotes.is_empty() {
                    Err("(empty)".into())
                } else {
                    Ok(format!("{} = {:.2}", quotes[0].ts_code, quotes[0].close))
                }
            }),
        true,
    ));
```

- [ ] **Step 6: Refactor 腾讯 上证指数 K线 probe (lines 1060-1083)**

Replace the existing match block:

```rust
    // Tencent Shanghai Composite K-line (used by macro_refresh fallback)
    sources.push(probe(
        "腾讯 上证指数 K线",
        &now_str,
        crate::tencent_quotes::fetch_index_kline(&tencent_http, "sh000001", 25)
            .await
            .map(|k| {
                format!(
                    "close = {:.2}, vol20 = {}",
                    k.close,
                    k.vol20.map(|v| format!("{:.2}", v)).unwrap_or_else(|| "n/a".into())
                )
            }),
        true,
    ));
```

- [ ] **Step 7: Refactor invest.db probe (lines 1085-1106)**

Replace the existing match block:

```rust
    // Check invest.db (light schema check — confirms the holdings table exists)
    sources.push(probe(
        "invest.db",
        &now_str,
        crate::storage::invest::with_conn(|conn| {
            conn.query_row("SELECT 1 FROM holdings LIMIT 1", [], |_| Ok(()))
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(()),
                    other => Err(format!("{other}")),
                })
        })
        .map(|()| "connected, schema ok".into())
        .map_err(|e| format!("schema check failed: {e}")),
        true,
    ));
```

- [ ] **Step 8: Refactor Python 运行时 probe (lines 1108-1122)**

Replace the existing match block:

```rust
    // Python runtime — root dependency for AkShare / Jin10 / Yahoo (yfinance)
    sources.push(probe(
        "Python 运行时",
        &now_str,
        crate::python::require().map(|_| "ready".into()).map_err(|e| format!("{e}")),
        true,
    ));
```

- [ ] **Step 9: Refactor miniQMT probe (lines 1127-1153)**

Replace the existing match block:

```rust
    // miniQMT (xtdata) — depends on Python runtime + QMT client
    sources.push(probe(
        "miniQMT 行情",
        &now_str,
        intl_client
            .fetch_xtdata_health()
            .await
            .map(|h| {
                if h.available {
                    "QMT 客户端在线".into()
                } else if h.reason.is_empty() {
                    "QMT 客户端离线".into()
                } else {
                    h.reason
                }
            }),
        true,
    ));
```

- [ ] **Step 10: Remove `probe_news` helper and refactor its call sites (lines 1155-1193)**

Delete the `probe_news` async function (lines 1155-1190) and replace its two call sites:

```rust
    // AkShare stock news
    sources.push(probe(
        "AkShare 个股",
        &now_str,
        intl_client
            .fetch_akshare_stock_news("000001", 1)
            .await
            .map(|items| {
                items
                    .first()
                    .map(|i| i.title.chars().take(20).collect::<String>())
                    .unwrap_or_else(|| "(empty)".into())
            }),
        true,
    ));

    // Jin10 news
    sources.push(probe(
        "金十数据",
        &now_str,
        intl_client
            .fetch_jinshi_news("", 1, None)
            .await
            .map(|items| {
                items
                    .first()
                    .map(|i| i.title.chars().take(20).collect::<String>())
                    .unwrap_or_else(|| "(empty)".into())
            }),
        true,
    ));
```

- [ ] **Step 11: Refactor AkShare 10Y国债 probe (lines 1195-1214)**

Replace the existing match block:

```rust
    // AkShare 10Y bond yield (used by macro_refresh fallback)
    sources.push(probe(
        "AkShare 10Y国债",
        &now_str,
        intl_client
            .fetch_akshare_bond_yield()
            .await
            .map(|b| format!("yield = {:.3} ({})", b.yield_10y, b.date)),
        true,
    ));
```

- [ ] **Step 12: Refactor AkShare 涨跌停 probe (lines 1216-1239)**

Replace the existing match block:

```rust
    // AkShare market stats (limit-up / limit-down counts)
    let today_compact = Local::now().format("%Y%m%d").to_string();
    sources.push(probe(
        "AkShare 涨跌停",
        &now_str,
        intl_client
            .fetch_akshare_market_stats(&today_compact)
            .await
            .map(|s| format!("up = {}, down = {} ({})", s.limit_up_count, s.limit_down_count, s.date)),
        true,
    ));
```

- [ ] **Step 13: Refactor Yahoo Finance quote probe (lines 1241-1260)**

Replace the existing match block:

```rust
    // Yahoo Finance (VIX, TNX, DXY, Gold, Oil, USDCNY)
    sources.push(probe(
        "Yahoo Finance",
        &now_str,
        intl_client
            .fetch_yahoo_quote("^VIX")
            .await
            .map(|q| format!("VIX = {:.2}", q.price)),
        true,
    ));
```

- [ ] **Step 14: Refactor Yahoo Finance 历史 probe (lines 1262-1285)**

Replace the existing match block:

```rust
    // Yahoo Finance history (distinct from quote — uses yfinance.history endpoint)
    sources.push(probe(
        "Yahoo Finance 历史",
        &now_str,
        intl_client
            .fetch_yahoo_history("^VIX", 5)
            .await
            .and_then(|bars| {
                if bars.is_empty() {
                    Err("(empty)".into())
                } else {
                    Ok(bars
                        .last()
                        .map(|b| format!("{} {} close = {:.2}", b.symbol, b.date, b.close))
                        .unwrap_or_default())
                }
            }),
        true,
    ));
```

- [ ] **Step 15: Compile check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: `Finished` with no errors

- [ ] **Step 16: Verify runtime behavior**

Run: `cd src-tauri/python-runtime && python/python.exe scripts/test_xtdata.py`
Expected: all 5 tests pass (miniQMT health check still works)

- [ ] **Step 17: Commit**

```bash
git add src-tauri/src/commands/invest.rs
git commit -m "refactor(invest): extract generic probe helper for datasource health checks"
```
