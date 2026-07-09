# 盘前观察报告后端改造 实现计划(Plan A)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把盘前报告后端从"生成时临时拉数据"改为"盘后定时批量缓存全市场 → 生成只读缓存",并沿用委员会 provider、舆情定时采集、删 Yahoo 改东财/akshare。

**Architecture:** 新增盘后 `premarket_cache` job 批量拉全市场(tushare 代理单次 5500+行)→ 粗筛 ≤200 候选 → 逐候选算四因子 → 落 `premarket_factor_cache` 表。交易日经"从基准日往前回退找最近有数据日"确定(盘前/节后兜底也能命中最近收盘日)。生成 job 读 `MAX(trade_date)` 缓存 + 新鲜度守卫,缺失走兜底。舆情 `sentiment_collector` job 每小时采集+归一化(collect_all_sentiment 内部已串联)。海外 6 指标 akshare 4 + 东财直连 2,删 Yahoo/yfinance。

**Tech Stack:** Rust(Tauri 后端)、SQLite(rusqlite)、Python(akshare/requests provider,JSON-RPC bridge)、tokio、chrono。

## Global Constraints

- 不改 SABC 四因子打分公式(`scoring.rs::score` / `grade_of` 不动)。
- 不改委员会/宏观判断链路本体。
- tushare 走第三方代理:`TushareClient::with_token_and_proxy` / `from_settings`(已内置 proxy 解析),不用官方直连。
- 判"最近交易日"不用代理 `trade_cal`(滞后):查 `daily(目标日)` 返回 0 行即非交易日/数据未出。
- 缓存新鲜度守卫:读缓存时最新 `trade_date` 距今 > 4 自然日视为缺失,走兜底。
- 后端单测命令:`cd src-tauri && cargo test`(非 workspace,单 package)。
- 编译检查:`cd src-tauri && cargo build`。
- 每个 provider 方法经 JSON-RPC `"provider.method"` 路由(server.py `getattr`),Rust 侧 `rpc_call(method, params)`。
- 每步小步提交,TDD 优先(纯函数先写测试)。

---
## 文件结构总览

**新建:**
- `src-tauri/src/storage/invest/premarket_cache.rs` — `premarket_factor_cache` 表:建表、写入(批量 upsert)、读最新交易日整批、新鲜度判断。
- `src-tauri/src/invest/premarket/cache_builder.rs` — 盘后缓存构建:批量拉全市场、粗筛 ≤200、逐候选算四因子、写表。粗筛+打分逻辑供 cache job 与生成兜底共享。

**修改:**
- `src-tauri/src/tushare/client.rs` — 加 `daily_market`(按 trade_date 全市场)、`moneyflow_dc_market`(按 trade_date 全市场)两个批量方法。
- `src-tauri/src/invest/premarket/report.rs` — `collect_pool` → 读缓存;`compute_capital` 批量版查表;`ai_commentary` 传 committee settings;`build_news_block_for_ai` 放宽上限。
- `src-tauri/src/invest/premarket/mod.rs` — 挂 `cache_builder` 子模块。
- `src-tauri/src/storage/invest/stock_industry.rs` — 加 `names_of` 批量查名(cache_builder 填 CachedFactor.name 用,查不到回退代码)。
- `src-tauri/src/storage/invest/mod.rs` — init 里挂 `premarket_cache::create_table`。
- `src-tauri/src/invest/scheduler/mod.rs` — `default_jobs()` 加 `premarket_cache`、`sentiment_collector` 两 job。
- `src-tauri/src/invest/scheduler/runner.rs` — dispatch match 加两 job 分支。
- `src-tauri/src/invest/event_analyzer.rs` — 加 `cli_complete_with_settings`。
- `src-tauri/src/invest/macro_refresh.rs` — `fetch_international` 改调 akshare + 东财;删 Yahoo 符号表。
- `src-tauri/src/invest/international.rs` — 加 `fetch_overseas_indicator`(akshare/东财);删 `fetch_yahoo_quote`/`fetch_yahoo_history`/`YahooQuote`。
- `src-tauri/src/commands/invest.rs` — `get_datasource_health`:删 Yahoo 两探针 + Tushare 新闻探针,加"东财海外指标"探针。
- `src-tauri/python-runtime/scripts/providers/akshare_market.py` — 加 `overseas_vix/gold/oil/usdcny` 函数。
- `src-tauri/python-runtime/scripts/providers/eastmoney.py` — 加 `overseas_indicator(secid)` 函数(DXY/US10Y)。
- `src-tauri/python-runtime/scripts/server.py` — 删 yahoo provider 注册;`yfinance.version` builtin 处理。
- 删 `src-tauri/python-runtime/scripts/providers/yahoo.py`。

**测试:**
- `premarket_cache.rs` / `cache_builder.rs` / `client.rs` 内联 `#[cfg(test)] mod tests`(照现有 scoring.rs 模式)。

---
## Task 1: tushare 全市场批量方法

**Files:**
- Modify: `src-tauri/src/tushare/client.rs`(在 `daily`(L513)与 `moneyflow_dc`(L1418)之后各加一个 `_market` 变体;测试加到文件底部 `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `self.call_api(api_name, params, fields)`、`get_str`(L275)、`get_f64`(L289)、`DailyBar`(L25)、`MoneyflowDc`(L197)。
- Produces:
  - `pub async fn daily_market(&self, trade_date: &str) -> Result<Vec<DailyBar>, String>`
  - `pub async fn moneyflow_dc_market(&self, trade_date: &str) -> Result<Vec<MoneyflowDc>, String>`
  - `pub(crate) fn parse_daily_rows(fields: &[String], items: &[Vec<serde_json::Value>]) -> Vec<DailyBar>`(纯函数,供单测)

- [ ] **Step 1: 写失败测试(纯解析函数)**

在 `client.rs` 底部 `mod tests` 内加:

```rust
    #[test]
    fn parse_daily_rows_maps_fields_by_name() {
        let fields = vec![
            "ts_code".to_string(),
            "trade_date".to_string(),
            "close".to_string(),
            "pct_chg".to_string(),
            "amount".to_string(),
        ];
        let items = vec![
            vec![
                serde_json::json!("600519.SH"),
                serde_json::json!("20260708"),
                serde_json::json!(1680.5),
                serde_json::json!(2.31),
                serde_json::json!(123456.0),
            ],
            vec![
                serde_json::json!("000001.SZ"),
                serde_json::json!("20260708"),
                serde_json::json!(11.2),
                serde_json::json!(-1.1),
                serde_json::json!(98765.0),
            ],
        ];
        let bars = super::TushareClient::parse_daily_rows(&fields, &items);
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].ts_code, "600519.SH");
        assert_eq!(bars[0].pct_chg, 2.31);
        assert_eq!(bars[1].ts_code, "000001.SZ");
        assert_eq!(bars[1].pct_chg, -1.1);
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && cargo test parse_daily_rows_maps_fields_by_name`
Expected: 编译失败 `no function parse_daily_rows`。

- [ ] **Step 3: 抽出纯解析函数 + 批量方法**

在 `impl TushareClient` 内(紧接 `daily` 方法后)加。`parse_daily_rows` 抄 `daily`(L528-566)的字段定位+组装逻辑,去掉 `resolve_close_idx` 的 ETF 分支(全市场只有股票 `daily`,固定用 `close`):

```rust
    /// 按 trade_date 拉全市场日线(单次覆盖全 A ~5500 行,无需分页)。
    /// 用于盘后缓存,固定走股票 `daily` API(非 ETF)。
    pub async fn daily_market(&self, trade_date: &str) -> Result<Vec<DailyBar>, String> {
        let params = serde_json::json!({ "trade_date": trade_date });
        let resp = self.call_api("daily", params, "").await?;
        Ok(Self::parse_daily_rows(&resp.data.fields, &resp.data.items))
    }

    /// 纯解析:tushare daily 响应行 → Vec<DailyBar>。全市场固定用 `close` 列。
    pub(crate) fn parse_daily_rows(
        fields: &[String],
        items: &[Vec<serde_json::Value>],
    ) -> Vec<DailyBar> {
        let idx = |name: &str| fields.iter().position(|f| f == name);
        let (ts_i, td_i) = (idx("ts_code"), idx("trade_date"));
        let (open_i, high_i, low_i, close_i) =
            (idx("open"), idx("high"), idx("low"), idx("close"));
        let (pre_i, chg_i, pct_i, vol_i, amt_i) = (
            idx("pre_close"), idx("change"), idx("pct_chg"), idx("vol"), idx("amount"),
        );
        let mut bars = Vec::with_capacity(items.len());
        for row in items {
            let g = |i: Option<usize>| i.and_then(|i| get_f64(row, i)).unwrap_or_default();
            bars.push(DailyBar {
                ts_code: ts_i.and_then(|i| get_str(row, i)).unwrap_or_default(),
                trade_date: td_i.and_then(|i| get_str(row, i)).unwrap_or_default(),
                open: g(open_i), high: g(high_i), low: g(low_i), close: g(close_i),
                pre_close: g(pre_i), change: g(chg_i), pct_chg: g(pct_i),
                vol: g(vol_i), amount: g(amt_i),
            });
        }
        bars
    }
```

> 注意:`DailyBar` 字段(L25-37)为 `ts_code, trade_date, open, high, low, close, pre_close, change, pct_chg, vol, amount` 全 `f64`(除前两个 String)。若实际字段名不符,以 client.rs L25 定义为准调整。

- [ ] **Step 4: 跑测试确认通过**

Run: `cd src-tauri && cargo test parse_daily_rows_maps_fields_by_name`
Expected: PASS。

- [ ] **Step 5: 加 moneyflow_dc_market(照抄 moneyflow_dc 解析,params 换 trade_date)**

在 `moneyflow_dc`(L1418)后加:

```rust
    /// 按 trade_date 拉全市场东财资金流(单次 ~5900 行)。
    pub async fn moneyflow_dc_market(&self, trade_date: &str) -> Result<Vec<MoneyflowDc>, String> {
        let params = serde_json::json!({ "trade_date": trade_date });
        let resp = self.call_api("moneyflow_dc", params, "").await?;
        let fields = &resp.data.fields;
        let idx = |name: &str| fields.iter().position(|f| f == name);
        let (ts_i, td_i, net_i) = (idx("ts_code"), idx("trade_date"), idx("net_amount"));
        let mut items = Vec::with_capacity(resp.data.items.len());
        for row in &resp.data.items {
            items.push(MoneyflowDc {
                ts_code: ts_i.and_then(|i| get_str(row, i)).unwrap_or_default(),
                trade_date: td_i.and_then(|i| get_str(row, i)).unwrap_or_default(),
                buy_sm_amount: None,
                buy_md_amount: None,
                buy_lg_amount: None,
                buy_elg_amount: None,
                net_amount: net_i.and_then(|i| get_f64(row, i)),
            });
        }
        Ok(items)
    }
```

- [ ] **Step 6: 编译**

Run: `cd src-tauri && cargo build`
Expected: 编译通过(可能有 `daily_market`/`moneyflow_dc_market` unused 警告,Task 3 会用到,可暂忽略)。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/tushare/client.rs
git commit -m "feat(tushare): 加 daily_market/moneyflow_dc_market 全市场批量方法"
```

---
## Task 2: premarket_factor_cache 表(storage 层)

**Files:**
- Create: `src-tauri/src/storage/invest/premarket_cache.rs`
- Modify: `src-tauri/src/storage/invest/mod.rs`(init_db_inner L339-357 区块加 `premarket_cache::create_table(&conn)?;`;文件顶部 `mod` 声明区加 `pub mod premarket_cache;`)

**Interfaces:**
- Consumes: `with_conn`(mod.rs L450)、`with_conn_mut`、`rusqlite::{params, Connection}`。
- Produces:
  - `pub struct CachedFactor { symbol, name: String; change_pct, amount, sentiment, capital, technical, catalyst: f64; missing: Vec<String> }`
  - `pub fn create_table(conn: &Connection) -> Result<(), String>`
  - `pub fn save_factor_cache(trade_date: &str, rows: &[CachedFactor]) -> Result<(), String>`(批量 upsert)
  - `pub fn load_latest_cache() -> Result<Option<(String, Vec<CachedFactor>)>, String>`(返回 (最新trade_date, 整批);表空→None)
  - `pub fn is_fresh(trade_date: &str, today: &str, max_age_days: i64) -> bool`(纯函数,可单测)

- [ ] **Step 1: 建文件写建表+结构体+纯函数 is_fresh**

创建 `src-tauri/src/storage/invest/premarket_cache.rs`:

```rust
use crate::storage::invest::{with_conn, with_conn_mut};
use rusqlite::{params, Connection};

const CREATE_TABLE_SQL: &str = "
CREATE TABLE IF NOT EXISTS premarket_factor_cache (
    trade_date  TEXT NOT NULL,
    symbol      TEXT NOT NULL,
    name        TEXT NOT NULL,
    change_pct  REAL NOT NULL DEFAULT 0,
    amount      REAL NOT NULL DEFAULT 0,
    sentiment   REAL NOT NULL DEFAULT 50,
    capital     REAL NOT NULL DEFAULT 50,
    technical   REAL NOT NULL DEFAULT 50,
    catalyst    REAL NOT NULL DEFAULT 50,
    missing     TEXT NOT NULL DEFAULT '',
    cached_at   TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (trade_date, symbol)
);
CREATE INDEX IF NOT EXISTS idx_pmcache_date ON premarket_factor_cache(trade_date);";

#[derive(Debug, Clone)]
pub struct CachedFactor {
    pub symbol: String,
    pub name: String,
    pub change_pct: f64,
    pub amount: f64,
    pub sentiment: f64,
    pub capital: f64,
    pub technical: f64,
    pub catalyst: f64,
    pub missing: Vec<String>,
}

pub fn create_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(CREATE_TABLE_SQL)
        .map_err(|e| format!("create premarket_factor_cache: {e}"))
}

/// 缓存新鲜度:trade_date 与 today(均 "YYYY-MM-DD")相差 <= max_age_days 自然日视为新鲜。
/// 解析失败一律视为不新鲜(保守走兜底)。
pub fn is_fresh(trade_date: &str, today: &str, max_age_days: i64) -> bool {
    let parse = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    match (parse(trade_date), parse(today)) {
        (Some(td), Some(now)) => {
            let diff = (now - td).num_days();
            (0..=max_age_days).contains(&diff)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_fresh_within_window() {
        assert!(is_fresh("2026-07-06", "2026-07-09", 4)); // 周五缓存,周一读,差3天
        assert!(is_fresh("2026-07-09", "2026-07-09", 4)); // 同日
    }

    #[test]
    fn is_fresh_rejects_stale_and_future() {
        assert!(!is_fresh("2026-07-01", "2026-07-09", 4)); // 差8天,过期
        assert!(!is_fresh("2026-07-10", "2026-07-09", 4)); // 未来日期
        assert!(!is_fresh("garbage", "2026-07-09", 4));    // 解析失败
    }
}
```

- [ ] **Step 2: 跑纯函数测试确认通过(建表未接 init 也能测纯函数)**

Run: `cd src-tauri && cargo test is_fresh_`
Expected: 两个测试 PASS(注意需先完成 Step 4 的 mod 声明才能编译整包;若此步编译不过,先做 Step 4 再回来)。

- [ ] **Step 3: 加批量写入 + 读取最新交易日整批**

在 `is_fresh` 后、`#[cfg(test)]` 前加:

```rust
/// 批量 upsert 一个交易日的整批候选。空 rows 直接返回 Ok。
pub fn save_factor_cache(trade_date: &str, rows: &[CachedFactor]) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    with_conn_mut(|conn| {
        let tx = conn.transaction().map_err(|e| format!("tx begin: {e}"))?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO premarket_factor_cache
                     (trade_date, symbol, name, change_pct, amount, sentiment, capital, technical, catalyst, missing, cached_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10, datetime('now'))
                     ON CONFLICT(trade_date, symbol) DO UPDATE SET
                        name=excluded.name, change_pct=excluded.change_pct, amount=excluded.amount,
                        sentiment=excluded.sentiment, capital=excluded.capital, technical=excluded.technical,
                        catalyst=excluded.catalyst, missing=excluded.missing, cached_at=excluded.cached_at",
                )
                .map_err(|e| format!("prepare upsert: {e}"))?;
            for r in rows {
                stmt.execute(params![
                    trade_date, r.symbol, r.name, r.change_pct, r.amount,
                    r.sentiment, r.capital, r.technical, r.catalyst, r.missing.join(","),
                ])
                .map_err(|e| format!("upsert row {}: {e}", r.symbol))?;
            }
        }
        tx.commit().map_err(|e| format!("tx commit: {e}"))
    })
}

/// 读缓存表中最新 trade_date 的整批候选。表空返回 Ok(None)。
pub fn load_latest_cache() -> Result<Option<(String, Vec<CachedFactor>)>, String> {
    with_conn(|conn| {
        let latest: Option<String> = conn
            .query_row(
                "SELECT MAX(trade_date) FROM premarket_factor_cache",
                [],
                |row| row.get(0),
            )
            .map_err(|e| format!("query max trade_date: {e}"))?;
        let Some(td) = latest else { return Ok(None) };
        let mut stmt = conn
            .prepare(
                "SELECT symbol, name, change_pct, amount, sentiment, capital, technical, catalyst, missing
                 FROM premarket_factor_cache WHERE trade_date = ?1 ORDER BY change_pct DESC",
            )
            .map_err(|e| format!("prepare load latest: {e}"))?;
        let rows = stmt
            .query_map([&td], |row| {
                let missing_str: String = row.get(8)?;
                Ok(CachedFactor {
                    symbol: row.get(0)?,
                    name: row.get(1)?,
                    change_pct: row.get(2)?,
                    amount: row.get(3)?,
                    sentiment: row.get(4)?,
                    capital: row.get(5)?,
                    technical: row.get(6)?,
                    catalyst: row.get(7)?,
                    missing: if missing_str.is_empty() {
                        vec![]
                    } else {
                        missing_str.split(',').map(|s| s.to_string()).collect()
                    },
                })
            })
            .map_err(|e| format!("query load latest: {e}"))?;
        let out: Vec<CachedFactor> = rows
            .collect::<Result<_, _>>()
            .map_err(|e| format!("read latest rows: {e}"))?;
        Ok(Some((td, out)))
    })
}
```

- [ ] **Step 4: 接入 init 迁移 + mod 声明**

在 `src-tauri/src/storage/invest/mod.rs` 顶部 `mod` 声明区(与 `pub mod macro_cache;` 同处)加:

```rust
pub mod premarket_cache;
```

在 `init_db_inner`(mod.rs L339-357)`macro_cache::create_table(&conn)?;` 那行后加:

```rust
    // Migration: create premarket_factor_cache table (盘后 SABC 全市场缓存)
    premarket_cache::create_table(&conn)?;
```

- [ ] **Step 5: 编译 + 跑测试**

Run: `cd src-tauri && cargo build && cargo test premarket_cache`
Expected: 编译通过;`is_fresh_*` 测试 PASS。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/storage/invest/premarket_cache.rs src-tauri/src/storage/invest/mod.rs
git commit -m "feat(storage): 加 premarket_factor_cache 表 + 批量写入/读最新/新鲜度守卫"
```

---
## Task 3: cache_builder — 盘后缓存构建(核心)

**Files:**
- Create: `src-tauri/src/invest/premarket/cache_builder.rs`
- Modify: `src-tauri/src/invest/premarket/mod.rs`(加 `pub mod cache_builder;`)
- Modify: `src-tauri/src/invest/premarket/report.rs`(把 `compute_sentiment_and_catalyst` L117、`compute_technical` L217 由 `fn`→`pub(crate) fn`,供 cache_builder 复用)

**Interfaces:**
- Consumes: `TushareClient::{from_settings, daily_market, moneyflow_dc_market}`(Task 1)、`storage::invest::premarket_cache::{CachedFactor, save_factor_cache}`(Task 2)、`report::{compute_sentiment_and_catalyst, compute_technical}`、`storage::invest::sentiment::list_recent_sentiment`、`date_utils::{get_invest_date, get_invest_date_compact}`、`DailyBar`、`MoneyflowDc`。
- Produces:
  - `pub fn select_candidates(daily: &[DailyBar], sentiment_symbols: &HashSet<String>, cap: usize) -> Vec<(String, f64, f64)>`(纯函数:返回 (ts_code, pct_chg, amount);舆情命中优先全保留,剩余涨幅降序补齐到 cap)
  - `pub fn capital_score_from_net(net_amount_wan: Option<f64>) -> f64`(纯函数:单日 net_amount 万元 → 0-100,tanh 归一)
  - `pub async fn build_cache() -> Result<(String, usize), String>`(顶层:返回 (trade_date, 写入行数))
  - `pub async fn build_cache_for_generation() -> Result<(), String>`(生成兜底调用,内部即 build_cache 忽略返回)

- [ ] **Step 1: 建文件写两个纯函数 + 失败测试**

创建 `src-tauri/src/invest/premarket/cache_builder.rs`:

```rust
//! 盘后 SABC 全市场缓存构建。批量拉全市场 → 粗筛 ≤200 → 逐候选四因子 → 落 premarket_factor_cache。
//! 粗筛+打分逻辑供 cache job 与生成兜底共享,避免两份实现漂移。

use std::collections::HashSet;
use crate::storage::invest::premarket_cache::{save_factor_cache, CachedFactor};
use crate::tushare::client::{DailyBar, MoneyflowDc, TushareClient};

/// 候选粗筛(纯函数):
/// - 舆情命中股(在 sentiment_symbols 内)优先全保留;
/// - 剩余名额按 pct_chg 降序补齐到 cap;
/// - 返回 (ts_code, pct_chg, amount)。
pub fn select_candidates(
    daily: &[DailyBar],
    sentiment_symbols: &HashSet<String>,
    cap: usize,
) -> Vec<(String, f64, f64)> {
    let code6 = |ts: &str| ts.split('.').next().unwrap_or(ts).to_string();
    let mut hit: Vec<&DailyBar> = Vec::new();
    let mut rest: Vec<&DailyBar> = Vec::new();
    for b in daily {
        if sentiment_symbols.contains(&code6(&b.ts_code)) {
            hit.push(b);
        } else {
            rest.push(b);
        }
    }
    rest.sort_by(|a, b| b.pct_chg.partial_cmp(&a.pct_chg).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(String, f64, f64)> = Vec::with_capacity(cap);
    for b in hit.iter().chain(rest.iter()) {
        if out.len() >= cap {
            break;
        }
        out.push((b.ts_code.clone(), b.pct_chg, b.amount));
    }
    out
}

/// capital 因子(纯函数):**单日** net_amount(万元)tanh 归一到 0-100。
/// ⚠️ 与旧 compute_capital(近5日求和 /5e5)不同,此处是单日值,分母重标为 1e5
/// (=10 亿单日主力净流入,对个股已属很强);tanh(1)=0.76 → ~88 分。None → 50(中性)。
/// 注:旧版还混 30% 北向(moneyflow_hsgt,市场级),批量版按个股摊无意义,已去掉。
pub fn capital_score_from_net(net_amount_wan: Option<f64>) -> f64 {
    match net_amount_wan {
        Some(v) => ((v / 1.0e5).tanh() + 1.0) / 2.0 * 100.0,
        None => 50.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(ts: &str, pct: f64) -> DailyBar {
        DailyBar {
            ts_code: ts.into(), trade_date: "20260708".into(),
            open: 0.0, high: 0.0, low: 0.0, close: 0.0, pre_close: 0.0,
            change: 0.0, pct_chg: pct, vol: 0.0, amount: 0.0,
        }
    }

    #[test]
    fn select_prioritizes_sentiment_hits_then_fills_by_pct() {
        let daily = vec![bar("600000.SH", 1.0), bar("600001.SH", 9.0), bar("600002.SH", 5.0)];
        let mut hits = HashSet::new();
        hits.insert("600000".to_string()); // 低涨幅但命中舆情,须保留
        let out = select_candidates(&daily, &hits, 2);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "600000.SH");     // 命中优先
        assert_eq!(out[1].0, "600001.SH");     // 剩余按涨幅降序 → 9.0 先
    }

    #[test]
    fn capital_score_maps_net_to_range() {
        assert_eq!(capital_score_from_net(None), 50.0);
        assert!((capital_score_from_net(Some(0.0)) - 50.0).abs() < 0.01);
        assert!(capital_score_from_net(Some(1.0e5)) > 85.0);   // 10亿单日 → ~88
        assert!(capital_score_from_net(Some(-1.0e5)) < 15.0);  // 对称
    }
}
```

- [ ] **Step 2: 跑纯函数测试确认失败(mod 未声明先跳编译)**

先做 Step 3 的 mod 声明,再 Run: `cd src-tauri && cargo test cache_builder::`
Expected: 两个纯函数测试可编译后 PASS(此步只验纯函数,build_cache 还没写)。

- [ ] **Step 3: 挂子模块 + 放开 report.rs 两函数可见性**

在 `src-tauri/src/invest/premarket/mod.rs` 加:

```rust
pub mod cache_builder;
```

在 `src-tauri/src/invest/premarket/report.rs`:
- L117 `fn compute_sentiment_and_catalyst(code: &str)` → `pub(crate) fn compute_sentiment_and_catalyst(code: &str)`
- L217 `async fn compute_technical(symbol: &str)` → `pub(crate) async fn compute_technical(symbol: &str)`

- [ ] **Step 4: 跑纯函数测试确认通过**

Run: `cd src-tauri && cargo test cache_builder::`
Expected: `select_prioritizes_*`、`capital_score_*` PASS。

- [ ] **Step 5: 提交(纯函数先落地)**

```bash
git add src-tauri/src/invest/premarket/cache_builder.rs src-tauri/src/invest/premarket/mod.rs src-tauri/src/invest/premarket/report.rs
git commit -m "feat(premarket): cache_builder 粗筛/capital 纯函数 + 复用 compute_* 可见性"
```

---
## Task 3b: cache_builder — build_cache 顶层编排

**Files:**
- Modify: `src-tauri/src/storage/invest/stock_industry.rs`(加 `names_of` 批量查名)
- Modify: `src-tauri/src/invest/premarket/cache_builder.rs`(在纯函数后、`#[cfg(test)]` 前加 `resolve_recent_trade_date` + `build_cache`)

**Interfaces:**
- Consumes: `stock_industry::names_of`、`TushareClient::{from_settings, daily_market, moneyflow_dc_market}`、`get_invest_naive_date()`(date_utils,返回 `NaiveDate`)。
- Produces: `pub async fn build_cache() -> Result<(String, usize), String>`、`pub async fn build_cache_for_generation() -> Result<(), String>`

- [ ] **Step 1: stock_industry 加 names_of 批量查名**

在 `src-tauri/src/storage/invest/stock_industry.rs` 的 `industry_of`(L33)后加:

```rust
/// 批量查名:codes 为 6 位裸码列表 → {code: name}。未收录的 code 不出现在结果里。
/// 供盘前缓存填 CachedFactor.name;查不到时调用方回退代码。
pub fn names_of(codes: &[String]) -> Result<std::collections::HashMap<String, String>, String> {
    use std::collections::HashMap;
    if codes.is_empty() {
        return Ok(HashMap::new());
    }
    with_conn(|conn| {
        let placeholders = std::iter::repeat("?").take(codes.len()).collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT code, name FROM stock_industry WHERE code IN ({placeholders}) AND name IS NOT NULL AND name != ''"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare names_of: {e}"))?;
        let params = rusqlite::params_from_iter(codes.iter());
        let rows = stmt
            .query_map(params, |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| format!("query names_of: {e}"))?;
        let mut out = HashMap::new();
        for r in rows {
            let (c, n) = r.map_err(|e| format!("row names_of: {e}"))?;
            out.insert(c, n);
        }
        Ok(out)
    })
}
```

- [ ] **Step 2: 写 build_cache 编排(含交易日回退循环)**

在 `cache_builder.rs` 的 `capital_score_from_net` 之后加。**关键:目标交易日不能硬用今天——盘前/节后兜底时今天无数据,需从 `get_invest_naive_date()` 往前逐日回退,找到 `daily_market` 返回非空的最近交易日**:

```rust
use crate::invest::premarket::report::{compute_sentiment_and_catalyst, compute_technical};
use futures::stream::StreamExt;

const CANDIDATE_CAP: usize = 200;
const TECH_CONCURRENCY: usize = 3;
const MAX_LOOKBACK_DAYS: i64 = 7;

/// 从 invest 基准日往前逐日回退,返回首个 daily_market 非空的 (compact日, dash日, daily行)。
/// 盘前/节后今天无数据时靠此回退到最近已收盘交易日。全部为空 → Err。
async fn resolve_recent_trade_date(
    client: &TushareClient,
) -> Result<(String, String, Vec<DailyBar>), String> {
    let base = crate::invest::date_utils::get_invest_naive_date();
    for back in 0..=MAX_LOOKBACK_DAYS {
        let day = base - chrono::Duration::days(back);
        let compact = day.format("%Y%m%d").to_string();
        let daily = client.daily_market(&compact).await?;
        if !daily.is_empty() {
            let dash = day.format("%Y-%m-%d").to_string();
            return Ok((compact, dash, daily));
        }
    }
    Err(format!(
        "resolve_recent_trade_date: 近 {MAX_LOOKBACK_DAYS} 日均无 daily 数据"
    ))
}

/// 盘后缓存构建:批量拉全市场 → 粗筛 ≤200 → 逐候选四因子 → 落表。
/// 返回 (trade_date, 写入行数)。交易日经 resolve_recent_trade_date 回退确定。
pub async fn build_cache() -> Result<(String, usize), String> {
    let client = TushareClient::from_settings()?;

    // 1. 确定最近有数据的交易日 + 全市场 daily(带回退)
    let (td_compact, td_dash, daily) = resolve_recent_trade_date(&client).await?;

    // 2. 批量拉全市场 moneyflow_dc(同一交易日)→ 按 6 位裸码建 net_amount map
    let flow: Vec<MoneyflowDc> = client
        .moneyflow_dc_market(&td_compact)
        .await
        .unwrap_or_else(|e| {
            log::warn!("[cache_builder] moneyflow_dc_market failed: {e}; capital 全缺省");
            vec![]
        });
    let code6 = |ts: &str| ts.split('.').next().unwrap_or(ts).to_string();
    let net_map: std::collections::HashMap<String, Option<f64>> =
        flow.iter().map(|f| (code6(&f.ts_code), f.net_amount)).collect();

    // 3. 近 3 日舆情命中股集合(6 位裸码)
    let since = (chrono::Local::now() - chrono::Duration::days(3))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let sent_items = crate::storage::invest::sentiment::list_recent_sentiment(&since, 500)
        .unwrap_or_default();
    let mut sentiment_symbols: HashSet<String> = HashSet::new();
    for it in &sent_items {
        if let Some(sym) = &it.symbol {
            sentiment_symbols.insert(code6(sym));
        }
    }

    // 4. 粗筛候选
    let candidates = select_candidates(&daily, &sentiment_symbols, CANDIDATE_CAP);
    log::info!("[cache_builder] {} 候选(全市场 {} 行, 舆情命中 {})",
        candidates.len(), daily.len(), sentiment_symbols.len());

    // 5. 批量查名(6 位裸码 → 中文名;查不到回退代码)
    let code_list: Vec<String> = candidates.iter().map(|(ts, _, _)| code6(ts)).collect();
    let name_map = crate::storage::invest::stock_industry::names_of(&code_list)
        .unwrap_or_default();

    // 6. technical(K线)限速慢拉;sentiment/catalyst(本地库)+ capital(查 net_map)同步
    let tech_results: Vec<(String, Option<f64>)> = futures::stream::iter(
        candidates.iter().map(|(ts, _, _)| {
            let ts = ts.clone();
            async move { (ts.clone(), compute_technical(&ts).await) }
        }),
    )
    .buffer_unordered(TECH_CONCURRENCY)
    .collect()
    .await;
    let tech_map: std::collections::HashMap<String, Option<f64>> =
        tech_results.into_iter().collect();

    let mut rows: Vec<CachedFactor> = Vec::with_capacity(candidates.len());
    for (ts, pct, amount) in &candidates {
        let c6 = code6(ts);
        let (sent_opt, cat_opt) = compute_sentiment_and_catalyst(&c6);
        let mut missing = Vec::new();
        let sentiment = sent_opt.unwrap_or_else(|| { missing.push("sentiment".into()); 50.0 });
        let catalyst = cat_opt.unwrap_or_else(|| { missing.push("catalyst".into()); 50.0 });
        let capital = match net_map.get(&c6) {
            Some(net) => capital_score_from_net(*net),
            None => { missing.push("capital".into()); 50.0 }
        };
        let technical = match tech_map.get(ts).and_then(|o| *o) {
            Some(t) => t,
            None => { missing.push("technical".into()); 50.0 }
        };
        // 名称:stock_industry 查表,查不到回退代码
        let name = name_map.get(&c6).cloned().unwrap_or_else(|| ts.clone());
        rows.push(CachedFactor {
            symbol: ts.clone(),
            name,
            change_pct: *pct,
            amount: *amount,
            sentiment, capital, technical, catalyst,
            missing,
        });
    }

    // 7. 落表(缓存 key = td_dash,与生成侧读取口径一致)
    save_factor_cache(&td_dash, &rows)?;
    Ok((td_dash, rows.len()))
}

/// 生成兜底:缓存缺失/过期时现场构建一次。忽略返回值,失败不阻断报告。
pub async fn build_cache_for_generation() -> Result<(), String> {
    build_cache().await.map(|(td, n)| {
        log::info!("[cache_builder] 兜底构建完成 {td}: {n} 行");
    })
}
```

> `td_dash` 是回退命中的真实收盘交易日(如周一盘前触发 → 命中上周五),与生成侧 `load_latest_cache` 的 `MAX(trade_date)` 口径天然一致。`scoring::score` 的组装在生成侧(Task 4)完成,cache 只存四因子分与 missing,故此处不 import score/cfg。

- [ ] **Step 3: 加 futures 依赖确认**

Run: `cd src-tauri && grep -q '^futures' Cargo.toml && echo HAS_FUTURES || echo NEED_FUTURES`
Expected: `HAS_FUTURES`(orchestrator.rs 已用 `buffer_unordered`,依赖应已在)。若 `NEED_FUTURES`,则 `cargo add futures`。

- [ ] **Step 4: 编译**

Run: `cd src-tauri && cargo build`
Expected: 编译通过。若报 `compute_technical`/`compute_sentiment_and_catalyst` 私有,回 Task 3 Step 3 确认可见性已改;若报 `names_of` 未找到,确认 Step 1 已落地。

- [ ] **Step 5: 手动冒烟(可选,需真 token)**

写一个临时 `#[tokio::test] #[ignore]` 调 `build_cache().await`,或等 Task 5 挂 job 后用 `trigger_cron_job` 触发。此处至少确认编译+纯函数测试通过。

Run: `cd src-tauri && cargo test cache_builder::`
Expected: 纯函数测试 PASS。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/invest/premarket/cache_builder.rs src-tauri/src/storage/invest/stock_industry.rs
git commit -m "feat(premarket): build_cache 全市场批量→粗筛→四因子(含交易日回退+批量查名)→落表"
```

---
## Task 4: 生成侧改读缓存 + AI 点评用委员会 provider

**Files:**
- Modify: `src-tauri/src/invest/premarket/report.rs`(`generate_premarket_report` L675 的打分段 L691-699 改读缓存;`ai_commentary` L327 改用 settings;`build_news_block_for_ai` L358 放宽上限)
- Modify: `src-tauri/src/invest/event_analyzer.rs`(加 `cli_complete_with_settings`)

**Interfaces:**
- Consumes: `premarket_cache::{load_latest_cache, is_fresh, CachedFactor}`(Task 2)、`cache_builder::build_cache_for_generation`(Task 3b)、`scoring::{score, FactorBreakdown, get_premarket_config}`、`macro_verdict::resolve_settings_path`(需放开可见性)。
- Produces:
  - `pub async fn cli_complete_with_settings(system_prompt: &str, user_message: &str, settings_path: Option<&std::path::Path>) -> Result<String, String>`(event_analyzer.rs)
  - report.rs 私有 `async fn collect_scores_from_cache(cfg: &PremarketConfig) -> Vec<SymbolScore>`

- [ ] **Step 1: 加 cli_complete_with_settings(event_analyzer.rs)**

在 `cli_complete`(L23-30)后加:

```rust
/// 同 cli_complete,但显式传 --settings 路径(委员会 provider 路由)。
/// settings_path=None 等价于 cli_complete(默认 Claude provider)。
pub async fn cli_complete_with_settings(
    system_prompt: &str,
    user_message: &str,
    settings_path: Option<&std::path::Path>,
) -> Result<String, String> {
    let exec = crate::invest::committee::cli_executor::CliCommitteeExecutor::global()
        .ok_or("claude CLI not available")?;
    exec.run_role(system_prompt, user_message, 0, settings_path, None).await
}
```

- [ ] **Step 2: 放开 resolve_settings_path 可见性(macro_verdict.rs)**

`src-tauri/src/invest/macro_verdict.rs` L65:
`fn resolve_settings_path()` → `pub(crate) fn resolve_settings_path()`

- [ ] **Step 3: ai_commentary 改用委员会 settings**

`report.rs` L336-341,把 `cli_complete(system, prompt)` 调用替换为:

```rust
    let settings = crate::invest::macro_verdict::resolve_settings_path();
    let resp = crate::invest::event_analyzer::cli_complete_with_settings(
        "你是严谨的金融分析师，只输出JSON。",
        &prompt,
        settings.as_deref(),
    )
    .await
    .ok()?;
```

- [ ] **Step 4: build_news_block_for_ai 放宽上限(report.rs L358)**

把 L362 的 `list_recent_sentiment(&since, 40)` 改 `list_recent_sentiment(&since, 120)`,L365 的 `.take(40)` 改 `.take(120)`,并把 L359 时间窗口 `days(1)` 改 `days(2)`(缓存后舆情量更大,放宽覆盖)。截断保持 80 字不变。

- [ ] **Step 5: 写 collect_scores_from_cache + 改生成流程**

在 `report.rs` 的 `collect_pool`(L28)后加:

```rust
/// 从盘后缓存读最新交易日整批 → 组装 SymbolScore。缓存缺失/过期时先兜底构建一次再读。
async fn collect_scores_from_cache(cfg: &PremarketConfig) -> Vec<SymbolScore> {
    use crate::storage::invest::premarket_cache::{is_fresh, load_latest_cache};
    let today = crate::invest::date_utils::get_invest_date();

    let fresh_cache = match load_latest_cache() {
        Ok(Some((td, rows))) if is_fresh(&td, &today, 4) && !rows.is_empty() => Some(rows),
        _ => None,
    };
    let rows = match fresh_cache {
        Some(r) => r,
        None => {
            log::warn!("[premarket] 缓存缺失/过期,兜底现场构建");
            let _ = crate::invest::premarket::cache_builder::build_cache_for_generation().await;
            match load_latest_cache() {
                Ok(Some((_, r))) => r,
                _ => {
                    log::warn!("[premarket] 兜底后仍无缓存,观察池为空");
                    return vec![];
                }
            }
        }
    };
    rows.into_iter()
        .map(|c| {
            let factors = crate::invest::premarket::scoring::FactorBreakdown {
                sentiment: c.sentiment,
                capital: c.capital,
                technical: c.technical,
                catalyst: c.catalyst,
            };
            score(&c.symbol, &c.name, factors, c.missing, cfg)
        })
        .collect()
}
```

在 `generate_premarket_report`(L675),把 L691-699 的打分段:

```rust
    let cfg: PremarketConfig = get_premarket_config();
    let pool = collect_pool();
    let mut scores: Vec<SymbolScore> = Vec::with_capacity(pool.len());
    for (symbol, name) in &pool {
        let (factors, missing) = compute_factors(symbol).await;
        scores.push(score(symbol, name, factors, missing, &cfg));
    }
```

替换为:

```rust
    let cfg: PremarketConfig = get_premarket_config();
    let scores: Vec<SymbolScore> = collect_scores_from_cache(&cfg).await;
```

> `collect_pool`、`compute_factors`、`compute_capital` 变为未使用。`compute_sentiment_and_catalyst`/`compute_technical` 仍被 cache_builder 用(保留)。删除 `collect_pool`(L28)、`compute_factors`(L268)、`compute_capital`(L155)三个死函数,避免 unused 警告。

- [ ] **Step 6: 编译 + 现有测试**

Run: `cd src-tauri && cargo build && cargo test premarket`
Expected: 编译通过;scoring.rs 现有档位测试仍 PASS。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/invest/premarket/report.rs src-tauri/src/invest/event_analyzer.rs src-tauri/src/invest/macro_verdict.rs
git commit -m "feat(premarket): 生成侧改读缓存+兜底; AI点评沿用委员会provider; 放宽舆情上限"
```

---
## Task 5: 调度器加 premarket_cache + sentiment_collector 两 job

**Files:**
- Modify: `src-tauri/src/invest/scheduler/mod.rs`(`default_jobs()` 返回的 vec L179 前加两个 CronJob)
- Modify: `src-tauri/src/invest/scheduler/runner.rs`(`dispatch_job` match L130 `_ =>` 前加两分支)

**Interfaces:**
- Consumes: `CronJob`(mod.rs L6 全字段:`id,name,cron_expr:String; interval_min:Option<i64>; enabled,requires_trading_day:bool; last_run,next_run,last_status:Option<String>; description:String; dedicated:bool`)、`cache_builder::build_cache`(Task 3b)、`sentiment::collect_all_sentiment`(内部已含归一化)。

- [ ] **Step 1: default_jobs() 加两 job(mod.rs,在 macro_verdict job L164 之后、`]` L179 之前)**

```rust
        CronJob {
            id: "premarket_cache".into(),
            name: "盘后SABC缓存".into(),
            // 盘后 16:30 工作日:收盘后拉全市场,粗筛≤200,算四因子落表
            cron_expr: "0 30 16 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "盘后批量拉全市场→粗筛→四因子→premarket_factor_cache".into(),
            dedicated: false,
        },
        CronJob {
            id: "sentiment_collector".into(),
            name: "舆情定时采集".into(),
            // 每小时采集外部舆情 + 串联归一化打标(非交易日也采,覆盖周末消息)
            cron_expr: "0 0 * * * *".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "每小时采集外部舆情并归一化打标(全市场口径)".into(),
            dedicated: false,
        },
```

- [ ] **Step 2: dispatch_job 加两分支(runner.rs,在 `_ =>` L130 之前)**

```rust
        "premarket_cache" => {
            let (td, n) = crate::invest::premarket::cache_builder::build_cache().await?;
            Ok(format!("盘后缓存: {} 共 {} 候选", td, n))
        }
        "sentiment_collector" => {
            // collect_all_sentiment 内部已 loop analyze_pending 到 total_pending==0
            // (sentiment.rs L154-172),故采集即含归一化打标,无需再单独调 analyze_pending。
            let r = crate::invest::sentiment::collect_all_sentiment(None, 30).await?;
            Ok(format!(
                "舆情采集+归一化: 待处理 {}, 打标 {}, 跳过 {}",
                r.total_pending, r.analyzed, r.skipped
            ))
        }
```

> 已核实:`collect_all_sentiment(symbol: Option<&str>, limit: u32) -> Result<AnalyzerResult, String>`(sentiment.rs L139),返回 `AnalyzerResult { total_pending, analyzed, skipped: usize; errors: Vec<String> }`,且**内部循环调用 `analyze_pending` 直到 pending 清零**——采集与归一化在此一步完成,再调 `analyze_pending` 是冗余(第二次立即返回 0)。spec 第8条"同 job 内串联归一化"由此满足。

- [ ] **Step 3: 编译**

Run: `cd src-tauri && cargo build`
Expected: 编译通过。

- [ ] **Step 4: 验证 job 注册(现有 default_jobs 测试)**

Run: `cd src-tauri && cargo test scheduler`
Expected: 现有调度器测试 PASS;若有"job 数量"断言,更新为 +2。

- [ ] **Step 5: 手动触发冒烟(需真 token + claude CLI)**

启动 app 后于数据源页触发 `premarket_cache`(或 `trigger_cron_job("premarket_cache")`),确认日志出现"盘后缓存: YYYY-MM-DD 共 N 候选"且 N>0。此步为集成验证,计划文档记录预期,执行时人工确认。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/invest/scheduler/mod.rs src-tauri/src/invest/scheduler/runner.rs
git commit -m "feat(scheduler): 加 premarket_cache(盘后16:30)+sentiment_collector(每小时)两 job"
```

---
## Task 6: Python provider — 海外指标(东财 DXY/US10Y + akshare VIX/金/油/汇率)

**Files:**
- Modify: `src-tauri/python-runtime/scripts/providers/eastmoney.py`(加 `overseas_indicator(secid)`)
- Modify: `src-tauri/python-runtime/scripts/providers/akshare_market.py`(加 `overseas_vix / overseas_gold / overseas_oil / overseas_usdcny`)

**Interfaces(JSON-RPC 方法名):**
- `eastmoney.overseas_indicator` — params `{"secid": "100.UDI"}`(DXY)/`{"secid":"171.US10Y"}`(美10Y),返回 `{"value": f64, "name": str, "change_pct": f64}`
- `akshare_market.overseas_vix / overseas_gold / overseas_oil / overseas_usdcny` — 无参,各返回 `{"value": f64}` 或 `{}`

- [ ] **Step 1: eastmoney.overseas_indicator(按 f59 小数位解码)**

在 `eastmoney.py` 的 `quote`(L118)后加。**注意:与 A股 quote 固定 /100 不同,海外指标必须用 f59 动态小数位**:

```python
def overseas_indicator(secid: str) -> dict:
    """Fetch an overseas indicator (DXY / US10Y) from EastMoney push2.

    secid examples: "100.UDI" (美元指数), "171.US10Y" (美国10年期国债收益率).
    Decodes value = f43 / 10**f59 (dynamic decimals — NOT the /100 used for A-shares).
    Returns {"value": float, "name": str, "change_pct": float} or {} on failure.
    """
    url = "https://push2.eastmoney.com/api/qt/stock/get"
    params = {
        "secid": secid,
        "fields": "f43,f57,f58,f59,f170",
        "ut": "fa5fd1943c7b386f172d6893dbbd1d0c",
    }
    session = _session.get()
    if session is None:
        return {}
    try:
        resp = session.get(url, params=params, timeout=10)
        resp.raise_for_status()
        data = resp.json().get("data", {})
    except Exception:
        return {}
    if not data:
        return {}
    raw = data.get("f43")
    dec = data.get("f59")
    if raw is None or dec is None:
        return {}
    try:
        value = float(raw) / (10 ** int(dec))
    except (ValueError, TypeError):
        return {}
    # f170 change_pct 同样按 2 位小数编码(整数百分比*100)
    chg_raw = data.get("f170")
    change_pct = (float(chg_raw) / 100.0) if chg_raw is not None else 0.0
    return {
        "value": round(value, 4),
        "name": data.get("f58", secid),
        "change_pct": round(change_pct, 2),
    }
```

- [ ] **Step 2: akshare_market 加 4 个海外指标函数**

在 `akshare_market.py` 的 `bond_yield_10y`(L16)后加。VIX/金/油走 `futures_foreign_hist`,汇率走 `fx_spot_quote`。**实现时以实测的确切 akshare 接口名/参数为准(见 spec 待验证假设1),下方为骨架**:

```python
def _latest_futures_foreign(symbol: str) -> dict:
    """Helper: 取外盘期货最新收盘 (VIX=VX, 金=GC, 油=CL)。"""
    try:
        import akshare as ak
    except ImportError:
        return {}
    try:
        df = ak.futures_foreign_hist(symbol=symbol)
    except Exception:
        return {}
    if df is None or df.empty:
        return {}
    df = df.dropna(subset=[df.columns[-1]])
    if df.empty:
        return {}
    return {"value": round(float(df.iloc[-1]["close"]), 4)}


def overseas_vix() -> dict:
    return _latest_futures_foreign("VX")


def overseas_gold() -> dict:
    return _latest_futures_foreign("GC")


def overseas_oil() -> dict:
    return _latest_futures_foreign("CL")


def overseas_usdcny() -> dict:
    """离岸/在岸人民币汇率 (akshare fx_spot_quote)。"""
    try:
        import akshare as ak
    except ImportError:
        return {}
    try:
        df = ak.fx_spot_quote()
    except Exception:
        return {}
    if df is None or df.empty:
        return {}
    row = df[df["ccy_pair"].astype(str).str.contains("USD/CNY", na=False)]
    if row.empty:
        return {}
    try:
        return {"value": round(float(row.iloc[0]["bid"]), 4)}
    except (ValueError, KeyError, TypeError):
        return {}
```

> ⚠️ akshare 接口的列名/参数在不同版本可能不同。执行此步时先在 bundled python-runtime 里 `python -c "import akshare as ak; print(ak.futures_foreign_hist(symbol='VX').tail())"` 确认列名(尤其 `close`)与 `fx_spot_quote` 的列名(`ccy_pair`/`bid`),按实测调整。spec 假设1已实证这些源有新鲜数据。

- [ ] **Step 3: 语法检查两 python 文件**

Run: `cd src-tauri/python-runtime/scripts && python -m py_compile providers/eastmoney.py providers/akshare_market.py`
Expected: 无输出(编译通过)。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/python-runtime/scripts/providers/eastmoney.py src-tauri/python-runtime/scripts/providers/akshare_market.py
git commit -m "feat(python): 海外指标 provider——东财DXY/US10Y(f59解码)+akshare VIX/金/油/汇率"
```

---
## Task 7: Rust 海外链路改 akshare/东财 + 删 Yahoo

**Files:**
- Modify: `src-tauri/src/invest/international.rs`(加海外指标方法;删 `fetch_yahoo_quote` L173/`fetch_yahoo_history`/`YahooQuote` L12)
- Modify: `src-tauri/src/invest/macro_refresh.rs`(`fetch_international` L378 改调新方法)
- Modify: `src-tauri/python-runtime/scripts/server.py`(删 `register_provider("yahoo", "yahoo")` L93)
- Delete: `src-tauri/python-runtime/scripts/providers/yahoo.py`

**Interfaces:**
- Consumes: `InternationalClient::rpc_call`(L162,`async fn rpc_call<T: DeserializeOwned>(&self, method: &str, params: Value) -> Result<T, String>`)。
- Produces:
  - `pub async fn fetch_eastmoney_overseas(&self, secid: &str) -> Result<OverseasIndicator, String>`
  - `pub async fn fetch_akshare_overseas(&self, method: &str) -> Result<OverseasValue, String>`
  - 结构体 `OverseasIndicator { value: f64, name: String, change_pct: f64 }`、`OverseasValue { value: f64 }`

- [ ] **Step 1: international.rs 加结构体 + 两方法**

在 `YahooQuote`(L12)附近加结构体,在 `fetch_akshare_bond_yield`(L259)后加方法:

```rust
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverseasIndicator {
    pub value: f64,
    #[serde(default)]
    pub name: String,
    #[serde(alias = "change_pct", default)]
    pub change_pct: f64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OverseasValue {
    pub value: f64,
}
```

```rust
    /// 东财直连海外指标(DXY secid=100.UDI / 美10Y secid=171.US10Y)。
    pub async fn fetch_eastmoney_overseas(&self, secid: &str) -> Result<OverseasIndicator, String> {
        self.rpc_call("eastmoney.overseas_indicator", serde_json::json!({ "secid": secid }))
            .await
    }

    /// akshare 海外标量指标(method 取 "overseas_vix"/"overseas_gold"/"overseas_oil"/"overseas_usdcny")。
    pub async fn fetch_akshare_overseas(&self, method: &str) -> Result<OverseasValue, String> {
        self.rpc_call(&format!("akshare_market.{method}"), serde_json::json!({}))
            .await
    }
```

- [ ] **Step 2: macro_refresh.rs 重写 fetch_international(L378-420)**

把整个 `fetch_international`(L378 起,到 L420 的 `}`)替换为:

```rust
/// Fetch VIX / 美10Y / DXY / Gold / Oil / USDCNY —— akshare(4) + 东财直连(2)。
/// Yahoo 已弃用。任一失败只 warn 跳过,不阻断其余。
async fn fetch_international() -> MacroResult {
    let client = crate::invest::international::InternationalClient::from_settings();
    let mut entries = Vec::new();

    // 东财直连:DXY / 美10Y(按 f59 解码,value 已是真值)
    for (secid, indicator) in [("100.UDI", "dxy"), ("171.US10Y", "tnx")] {
        match client.fetch_eastmoney_overseas(secid).await {
            Ok(q) => entries.push((
                indicator.to_string(),
                Some(q.value),
                Some(serde_json::json!({ "change_pct": q.change_pct }).to_string()),
                "eastmoney",
            )),
            Err(e) => log::warn!("macro_refresh: eastmoney {secid}: {e}"),
        }
    }

    // akshare:VIX / 金 / 油 / 汇率
    for (method, indicator) in [
        ("overseas_vix", "vix"),
        ("overseas_gold", "gold"),
        ("overseas_oil", "oil"),
        ("overseas_usdcny", "usdcny"),
    ] {
        match client.fetch_akshare_overseas(method).await {
            Ok(v) => entries.push((indicator.to_string(), Some(v.value), None, "akshare")),
            Err(e) => log::warn!("macro_refresh: akshare {method}: {e}"),
        }
    }

    if entries.is_empty() {
        return Err("international: all overseas fetches failed".into());
    }
    Ok(entries)
}
```

> `tnx` 指标名沿用(美10Y),`MacroSnapshot` 无 tnx/dxy/oil/usdcny 字段也无妨——macro_cache 表按 indicator 存全部,快照只挑展示字段。

- [ ] **Step 3: 删 Yahoo 代码**

- `international.rs`:删 `YahooQuote` 结构体(L12-25)、`fetch_yahoo_quote`(L173-177)、`fetch_yahoo_history`(若存在,grep `fetch_yahoo_history` 定位后删整个方法)。
- `server.py`:删 L93 `register_provider("yahoo", "yahoo")`。
- 删文件 `providers/yahoo.py`。
- grep 全仓 `fetch_yahoo` / `YahooQuote` / `yahoo\.` 确认无残留引用(Task 8 会处理 datasource_health 里的引用)。

Run: `cd src-tauri && grep -rn "fetch_yahoo\|YahooQuote" src/ | grep -v "datasource\|invest.rs"`
Expected: 无输出(除 commands/invest.rs 的健康检查,Task 8 处理)。

- [ ] **Step 4: 编译**

Run: `cd src-tauri && cargo build`
Expected: 编译通过(commands/invest.rs 若仍引用 fetch_yahoo 会报错 → 直接进 Task 8 修复后再编译)。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/invest/international.rs src-tauri/src/invest/macro_refresh.rs src-tauri/python-runtime/scripts/server.py
git rm src-tauri/python-runtime/scripts/providers/yahoo.py
git commit -m "feat(macro): 海外指标改 akshare+东财直连; 删 Yahoo provider/quote/history"
```

---
## Task 8: 数据源健康检查更新(删 Yahoo/Tushare新闻探针,加东财海外探针)

**Files:**
- Modify: `src-tauri/src/commands/invest.rs`(`get_datasource_health` L989)

**Interfaces:**
- Consumes: `DataSourceStatus { name: String, ok: bool, last_success: Option<String>, sample_value: Option<String> }`、`InternationalClient::{fetch_eastmoney_overseas, fetch_akshare_overseas}`(Task 7)。

- [ ] **Step 1: 删 Tushare 新闻探针(L1021-1046 整段)**

删除 `// Tushare 新闻 ...` 到该 match 块结束的整段(约 L1021-1046)。理由:实证 tushare 代理无 major_news 权限,探针恒失败误报。

- [ ] **Step 2: 删 Yahoo 两探针(L1270-1314 整段),换东财海外探针**

把 L1270-1314(Yahoo Finance quote + Yahoo Finance 历史两段)整体替换为:

```rust
    // 东财海外指标 (DXY / 美10Y — f59 解码直连)
    match intl_client.fetch_eastmoney_overseas("100.UDI").await {
        Ok(q) => sources.push(DataSourceStatus {
            name: "东财海外指标".into(),
            ok: true,
            last_success: Some(now_str.clone()),
            sample_value: Some(format!("DXY = {:.2}", q.value)),
        }),
        Err(e) => {
            log::warn!("[datasource] 东财海外指标 probe failed: {}", e);
            sources.push(DataSourceStatus {
                name: "东财海外指标".into(),
                ok: false,
                last_success: None,
                sample_value: Some(e),
            });
        }
    }

    // AkShare 海外标量 (VIX)
    match intl_client.fetch_akshare_overseas("overseas_vix").await {
        Ok(v) => sources.push(DataSourceStatus {
            name: "AkShare 海外指标".into(),
            ok: true,
            last_success: Some(now_str.clone()),
            sample_value: Some(format!("VIX = {:.2}", v.value)),
        }),
        Err(e) => {
            log::warn!("[datasource] AkShare 海外指标 probe failed: {}", e);
            sources.push(DataSourceStatus {
                name: "AkShare 海外指标".into(),
                ok: false,
                last_success: None,
                sample_value: Some(e),
            });
        }
    }
```

- [ ] **Step 3: Python runtime 探针里的 yfinance 处理**

grep 健康检查里是否探测 `yfinance.version`(server.py builtin):

Run: `cd src-tauri && grep -rn "yfinance" src/commands/invest.rs src/invest/`
Expected: 若有对 `yfinance.version` 的探针调用,删除或改探 `sys.version`。Python runtime 探针本身(检测 runtime 存活)保留。server.py 的 `yfinance.version` builtin 可保留(无害)或一并删,执行时二选一并记录。

- [ ] **Step 4: 编译 + 确认无 Yahoo 残留**

Run: `cd src-tauri && cargo build && grep -rn "fetch_yahoo\|YahooQuote\|Yahoo Finance" src/`
Expected: 编译通过;grep 无输出。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/commands/invest.rs
git commit -m "feat(datasource): 健康检查删 Yahoo/Tushare新闻探针,加东财+akshare海外指标探针"
```

---
## Task 9: 集成验证 + 全量回归

**Files:** 无新增改动,纯验证。

- [ ] **Step 1: 全量编译 + 单测**

Run: `cd src-tauri && cargo build && cargo test`
Expected: 编译 0 error;所有单测 PASS(尤其 `is_fresh_*`、`select_prioritizes_*`、`capital_score_*`、`parse_daily_rows_*`、scoring 档位测试)。

- [ ] **Step 2: clippy(可选,项目若用)**

Run: `cd src-tauri && cargo clippy --all-targets 2>&1 | grep -E "warning|error" | head -30`
Expected: 无新增 error;死函数(collect_pool/compute_factors/compute_capital)已删,无相关 unused 警告。

- [ ] **Step 3: 端到端冒烟(人工,需真 token + claude CLI + 启动 app)**

按序验证:
1. 触发 `premarket_cache` job → 日志"盘后缓存: YYYY-MM-DD 共 N 候选",N 在 100-200,查 DB `SELECT COUNT(*) FROM premarket_factor_cache` > 0。
2. 触发 `premarket_report` job → 报告秒级生成(读缓存),观察池标的数 = 缓存候选数(非旧的 Hold+Watch 数)。
3. 触发 `sentiment_collector` job → 日志"舆情采集: X 采集, 归一化 Y/Z 打标"。
4. 触发 `macro_refresh` → 查 DB `SELECT indicator,value,source FROM macro_cache WHERE indicator IN ('vix','dxy','tnx','gold','oil','usdcny')`,source 为 eastmoney/akshare,value 非空且合理(DXY~100、VIX~15-25)。
5. 打开数据源健康页 → "东财海外指标"、"AkShare 海外指标" 绿,无"Yahoo Finance"/"Tushare 新闻"条目。
6. AI 点评段:确认走委员会 provider(日志有 `--settings` 注入;非默认 Claude)。

- [ ] **Step 4: 缓存缺失兜底验证(人工)**

清空缓存表 `DELETE FROM premarket_factor_cache;` → 直接触发 `premarket_report` → 应看到"缓存缺失/过期,兜底现场构建"日志,报告仍能出(耗时较长),观察池非空。

- [ ] **Step 5: 最终提交(若前面步骤有微调)**

```bash
git add -A
git commit -m "test(premarket): 后端改造集成验证通过——缓存架构/海外指标/舆情定时全链路"
```

---

## 验证清单(spec 覆盖对照)

- [x] 第6条 A:tushare 全市场批量(Task1)+ 盘后缓存 job(Task5)+ cache_builder(Task3)
- [x] 第6条 粗筛≤200 舆情优先(Task3 select_candidates)
- [x] 第6条 capital 重写查表(Task3 capital_score_from_net,单日口径 /1e5)
- [x] 第6条 缓存 key=MAX(trade_date)+新鲜度守卫(Task2 is_fresh + Task4 collect_scores_from_cache)
- [x] 第6条 交易日回退(Task3b resolve_recent_trade_date,盘前/节后兜底不拉空)
- [x] 第6条 名称解析 stock_industry 批量查名回退代码(Task3b names_of)
- [x] 第6条 兜底共享逻辑(Task3b build_cache_for_generation + Task4 调用)
- [x] 第7条 AI 点评沿用委员会 provider(Task4 cli_complete_with_settings + resolve_settings_path)
- [x] 第8条 舆情定时采集+归一化(Task5 sentiment_collector,collect_all_sentiment 内部串联)
- [x] 第8条 放宽舆情上限(Task4 build_news_block_for_ai 120条/2天)
- [x] 第9条 删 Yahoo(Task7)+ akshare/东财海外指标(Task6/7)+ 健康检查更新(Task8)
