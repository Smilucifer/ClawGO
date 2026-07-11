# 每日盈记（Fortune Journal）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 openInvest 下实现"每日盈记"娱乐子功能——按日干支记录每日股市收益率，展示今日/明日干支评分、干支排行、数据统计与可选 AI 解读。

**Architecture:** 后端复用 `invest.db`（新增两表），核心是 `aggregate.rs` 的 point-in-time 单趟聚合（一趟 O(N) 同时算每日预测分/盘后分）；层分/综合分算法已在 `stats.rs` 定稿落地。前端在 invest 页新增 `fortune` 顶级 tab，含 3 子 tab + 日历 + 录入弹窗，遵循现有 Svelte 5 runes + Tailwind/CSS 变量模式。

**Tech Stack:** Rust + Tauri 2（rusqlite、tokio）、Svelte 5（runes）、TypeScript、Tailwind + CSS 变量。

## Global Constraints

- 语言：**只做中文**。i18n key 扁平化（`fortune_xxx`），加在 `messages/zh-CN.json` 与 `messages/en.json` **两个文件**（en 放同名中文值作 fallback，满足 `i18n:check`）。
- 序列化：所有返回前端的 struct 用 `#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]` + `#[serde(rename_all = "camelCase")]`；Rust 字段 snake_case。
- 错误类型：后端统一 `Result<T, String>`（`.map_err(|e| format!(...))`）。
- 数据库连接：不经参数传入；storage 函数内部用 `crate::storage::invest::with_conn` / `with_conn_mut`。建表函数例外，接 `&Connection` 参数。
- 胜率口径（钉死）：`收益 > 0` 才算赢，平盘（=0）不算赢。
- 层分/综合分：**不要改** `stats.rs`（已定稿测试全绿）；只调用其公共 API。
- 本机跑 Rust 测试：**必须走** `node scripts/rust-test.mjs --lib <filter>`，裸 `cargo test` 会因 DLL manifest 崩（0xc0000139）。
- 前端跑测试：`npm test`（vitest）。
- 组件样式：Tailwind 内联 + CSS 变量（`var(--space-*)`、`var(--radius-*)`、`var(--up)`/`var(--down)`/`var(--flat)`、`var(--accent)`、`var(--font-mono)`），根节点需在 `data-invest-scope` 作用域内（invest 页根 div 已有）。**红涨绿跌**。
- Tauri invoke：组件内 `const invoke = <T,>(cmd, args?) => getTransport().invoke<T>(cmd, args)`；store 内同样用 `getTransport().invoke`。

## File Structure

**后端（Rust）**
- `src-tauri/src/storage/invest/fortune.rs`（新建）— 两表建表 + CRUD：`fortune_daily_returns`、`fortune_ai_readings`。
- `src-tauri/src/storage/invest/mod.rs`（改）— 顶部 `pub mod fortune;`；`init_db_inner` 尾部 `fortune::create_table(&conn)?;`。
- `src-tauri/src/invest/fortune/aggregate.rs`（新建）— point-in-time 单趟聚合：产出今日/明日卡、日历全量、排行/风险、预告、KPI、月度。
- `src-tauri/src/invest/fortune/reading.rs`（新建）— AI 解读 executor（独立信号量，复用 committee `run_role`）。
- `src-tauri/src/invest/fortune/mod.rs`（改）— 加 `pub mod aggregate;`、`pub mod reading;`。
- `src-tauri/src/commands/fortune.rs`（新建）— 8 个 Tauri 命令边界。
- `src-tauri/src/commands/mod.rs`（改）— 加 `pub mod fortune;`。
- `src-tauri/src/lib.rs`（改）— `generate_handler![]` 注册 8 命令。

**前端（Svelte/TS）**
- `src/lib/stores/fortune-store.svelte.ts`（新建）— runes class 单例：状态 + 命令调用 + 缓存失效。
- `src/lib/components/invest/FortuneAnalysisTab.svelte`（新建）— 今日/明日卡 + AI 解读 + 日历。
- `src/lib/components/invest/FortuneStemBranchTab.svelte`（新建）— 天干表 + 地支表 + 6 路预告。
- `src/lib/components/invest/FortuneDataTab.svelte`（新建）— KPI + Top3 排行/风险 + 月度。
- `src/lib/components/invest/FortuneCalendar.svelte`（新建）— 干支日历，三态视觉。
- `src/lib/components/invest/FortuneRecordDialog.svelte`（新建）— 录入弹窗（单日 + 按月批量）。
- `src/routes/invest/+page.svelte`（改）— `InvestTab` 加 `'fortune'` + tab 项 + 渲染分支。
- `messages/zh-CN.json` + `messages/en.json`（改）— `fortune_*` key。

---

### Task 1: 存储层 — 建表 + CRUD

**Files:**
- Create: `src-tauri/src/storage/invest/fortune.rs`
- Modify: `src-tauri/src/storage/invest/mod.rs`（顶部模块声明区 + `init_db_inner` 第 361 行 `stock_data_cache::create_table(&conn)?;` 之后）

**Interfaces:**
- Produces（供 aggregate.rs 与 commands 使用）：
  - `pub struct DailyReturn { pub date: String, pub return_pct: f64, pub note: String, pub created_at: String, pub updated_at: String }`
  - `pub fn create_table(conn: &rusqlite::Connection) -> Result<(), String>`
  - `pub fn upsert_return(date: &str, return_pct: f64, note: &str) -> Result<(), String>`
  - `pub fn delete_return(date: &str) -> Result<(), String>`
  - `pub fn list_returns() -> Result<Vec<DailyReturn>, String>`（按 date 升序，供单趟聚合）
  - `pub fn insert_reading(date: &str, content: &str) -> Result<i64, String>`
  - `pub fn get_latest_reading(date: &str) -> Result<Option<String>, String>`

- [ ] **Step 1: 写建表 + CRUD（含测试模块骨架）**

创建 `src-tauri/src/storage/invest/fortune.rs`：

```rust
//! 每日盈记存储：手动录入的每日收益率 + AI 解读。复用 invest.db。
use rusqlite::Connection;
use super::{with_conn, with_conn_mut};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReturn {
    pub date: String,
    pub return_pct: f64,
    pub note: String,
    pub created_at: String,
    pub updated_at: String,
}

pub fn create_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS fortune_daily_returns (
            date TEXT PRIMARY KEY, return_pct REAL NOT NULL,
            note TEXT DEFAULT '', created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS fortune_ai_readings (
            id INTEGER PRIMARY KEY AUTOINCREMENT, date TEXT NOT NULL,
            content TEXT NOT NULL, created_at TEXT NOT NULL);",
    )
    .map_err(|e| format!("建 fortune 表失败: {e}"))
}
```

继续在同文件追加 CRUD（`now()` 用北京时间，复用 scheduler 的 `beijing_today` 不含时分，改用 chrono 直接取；项目已依赖 chrono）：

```rust
fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn upsert_return(date: &str, return_pct: f64, note: &str) -> Result<(), String> {
    let ts = now_iso();
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO fortune_daily_returns (date, return_pct, note, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(date) DO UPDATE SET return_pct=?2, note=?3, updated_at=?4",
            rusqlite::params![date, return_pct, note, ts],
        )
        .map_err(|e| format!("upsert 收益失败: {e}"))?;
        Ok(())
    })
}

pub fn delete_return(date: &str) -> Result<(), String> {
    with_conn_mut(|c| {
        c.execute("DELETE FROM fortune_daily_returns WHERE date=?1", [date])
            .map_err(|e| format!("删除收益失败: {e}"))?;
        Ok(())
    })
}

pub fn list_returns() -> Result<Vec<DailyReturn>, String> {
    with_conn(|c| {
        let mut stmt = c
            .prepare("SELECT date, return_pct, note, created_at, updated_at
                      FROM fortune_daily_returns ORDER BY date ASC")
            .map_err(|e| format!("查询收益失败: {e}"))?;
        let rows = stmt
            .query_map([], |r| Ok(DailyReturn {
                date: r.get(0)?, return_pct: r.get(1)?, note: r.get(2)?,
                created_at: r.get(3)?, updated_at: r.get(4)?,
            }))
            .map_err(|e| format!("映射收益失败: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("收集收益失败: {e}"))
    })
}
```

AI 解读的两个函数：

```rust
pub fn insert_reading(date: &str, content: &str) -> Result<i64, String> {
    let ts = now_iso();
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO fortune_ai_readings (date, content, created_at) VALUES (?1,?2,?3)",
            rusqlite::params![date, content, ts],
        )
        .map_err(|e| format!("插入解读失败: {e}"))?;
        Ok(c.last_insert_rowid())
    })
}

pub fn get_latest_reading(date: &str) -> Result<Option<String>, String> {
    with_conn(|c| {
        c.query_row(
            "SELECT content FROM fortune_ai_readings WHERE date=?1
             ORDER BY id DESC LIMIT 1",
            [date],
            |r| r.get::<_, String>(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(format!("查询解读失败: {other}")),
        })
    })
}
```

- [ ] **Step 2: 注册模块与建表**

在 `src-tauri/src/storage/invest/mod.rs` 顶部模块声明区（第 1-19 行那组 `pub mod` 中，按字母序插在 `pub mod events;` 后）加：

```rust
pub mod fortune;
```

在 `init_db_inner` 第 361 行 `stock_data_cache::create_table(&conn)?;` 之后加：

```rust
    fortune::create_table(&conn)?;
```

- [ ] **Step 3: 写 CRUD round-trip 测试**

在 `fortune.rs` 尾部加测试模块（用内存库直接建表，绕开全局 DB——但 CRUD 走 `with_conn`，故测试聚焦纯 SQL 正确性，用独立 in-memory conn 验证建表与 SQL 语义）：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_table_and_upsert_semantics() {
        let conn = Connection::open_in_memory().unwrap();
        create_table(&conn).unwrap();
        // upsert 两次同日期：第二次应覆盖 return_pct + note，行数保持 1
        conn.execute("INSERT INTO fortune_daily_returns (date,return_pct,note,created_at,updated_at) VALUES ('2026-07-01',1.5,'a','t','t')", []).unwrap();
        conn.execute("INSERT INTO fortune_daily_returns (date,return_pct,note,created_at,updated_at) VALUES ('2026-07-01',2.5,'b','t','t2') ON CONFLICT(date) DO UPDATE SET return_pct=2.5, note='b', updated_at='t2'", []).unwrap();
        let (n, ret, note): (i64, f64, String) = conn.query_row(
            "SELECT COUNT(*), MAX(return_pct), MAX(note) FROM fortune_daily_returns",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!(n, 1);
        assert_eq!(ret, 2.5);
        assert_eq!(note, "b");
    }

    #[test]
    fn readings_latest_is_highest_id() {
        let conn = Connection::open_in_memory().unwrap();
        create_table(&conn).unwrap();
        conn.execute("INSERT INTO fortune_ai_readings (date,content,created_at) VALUES ('2026-07-01','old','t1')", []).unwrap();
        conn.execute("INSERT INTO fortune_ai_readings (date,content,created_at) VALUES ('2026-07-01','new','t2')", []).unwrap();
        let latest: String = conn.query_row(
            "SELECT content FROM fortune_ai_readings WHERE date='2026-07-01' ORDER BY id DESC LIMIT 1",
            [], |r| r.get(0)).unwrap();
        assert_eq!(latest, "new");
    }
}
```

- [ ] **Step 4: 编译 + 跑测试**

Run: `node scripts/rust-test.mjs --lib storage::invest::fortune`
Expected: 2 passed（`create_table_and_upsert_semantics`、`readings_latest_is_highest_id`）

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/storage/invest/fortune.rs src-tauri/src/storage/invest/mod.rs
git commit -m "feat(fortune): storage layer — daily returns + AI readings tables + CRUD"
```

---

### Task 2: 聚合层 — point-in-time 单趟聚合

**Files:**
- Create: `src-tauri/src/invest/fortune/aggregate.rs`
- Modify: `src-tauri/src/invest/fortune/mod.rs`（加 `pub mod aggregate;`）

**Interfaces:**
- Consumes：
  - `crate::invest::fortune::calendar::{ganzhi, ganzhi_index, STEMS, BRANCHES}`
  - `crate::invest::fortune::stats::{LayerStat, layer_score, composite_from_layers, composite_score, fortune_level, FortuneLevel}`
  - `crate::storage::invest::fortune::{DailyReturn, list_returns}`
  - `crate::storage::invest::scheduler::{is_trading_day, beijing_today, next_trading_day}`
- Produces（供 commands 调用，全部 struct camelCase）：
  - `pub fn compute_analysis() -> Result<Analysis, String>`
  - `pub fn compute_overview() -> Result<Overview, String>`
  - `pub fn compute_data_summary() -> Result<DataSummary, String>`
  - 核心内部：`fn aggregate_pit(returns: &[DailyReturn]) -> Aggregation`（纯函数，可单测）

**核心数据结构（写在文件顶部）：**

```rust
use std::collections::HashMap;
use crate::invest::fortune::calendar::{ganzhi, ganzhi_index, STEMS, BRANCHES};
use crate::invest::fortune::stats::{LayerStat, layer_score, composite_from_layers, fortune_level, FortuneLevel};
use crate::storage::invest::fortune::{DailyReturn, list_returns};

/// 单个层级累加器（天干或地支的一个值）。
#[derive(Default, Clone, Copy)]
struct Acc { sum: f64, days: u32, wins: u32 }
impl Acc {
    fn push(&mut self, ret: f64) {
        self.sum += ret; self.days += 1;
        if ret > 0.0 { self.wins += 1; }   // 胜率口径：仅正收益算赢
    }
    fn to_layer_stat(self) -> LayerStat {
        if self.days == 0 { return LayerStat { avg_return_pct: 0.0, win_rate: 0.5, sample: 0 }; }
        LayerStat {
            avg_return_pct: self.sum / self.days as f64,
            win_rate: self.wins as f64 / self.days as f64,
            sample: self.days,
        }
    }
}
```

**响应类型（camelCase，供前端）：**

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayScore {
    pub date: String,
    pub stem: String,
    pub branch: String,
    pub predict_score: f64,
    pub predict_level: FortuneLevel,
    pub actual_return: Option<f64>,       // None = 未录（预测态）
    pub post_score: Option<f64>,          // None = 未录
    pub post_level: Option<FortuneLevel>,
    pub is_trading_day: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerRow {
    pub name: String,                     // 干支中文名
    pub avg_return: f64, pub win_rate: f64, pub sample: u32, pub score: f64,
    pub level: FortuneLevel,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub today: Option<DayScore>,          // None = 无任何数据（空状态）
    pub tomorrow: Option<DayScore>,
    pub calendar: Vec<DayScore>,          // 全量历史 + 当月未来预告格
}

/// 全量聚合结果（扫完所有记录后的终态累加器 + 每日预测/盘后分）。
struct Aggregation {
    stem_final: [Acc; 10],
    branch_final: [Acc; 12],
    day_scores: Vec<DayScore>,   // 每条记录一天，含预测/盘后
}
```

- [ ] **Step 1: 写单趟聚合纯函数 + 测试**

在 `aggregate.rs` 追加：

```rust
fn parse_ymd(date: &str) -> Option<(i64, i64, i64)> {
    let p: Vec<&str> = date.split('-').collect();
    if p.len() != 3 { return None; }
    Some((p[0].parse().ok()?, p[1].parse().ok()?, p[2].parse().ok()?))
}

fn score_for(stems: &[Acc; 10], branches: &[Acc; 12], si: usize, bi: usize) -> f64 {
    let s = layer_score(&stems[si].to_layer_stat());
    let b = layer_score(&branches[bi].to_layer_stat());
    composite_from_layers(s, b)
}

/// 单趟 point-in-time：records 按 date 升序。扫到 D 先拍预测快照，再并入 D → 盘后。
fn aggregate_pit(records: &[DailyReturn]) -> Aggregation {
    let mut stems = [Acc::default(); 10];
    let mut branches = [Acc::default(); 12];
    let mut day_scores = Vec::with_capacity(records.len());
    for rec in records {
        let Some((y, m, d)) = parse_ymd(&rec.date) else { continue };
        let idx = ganzhi_index(y, m, d);
        let (si, bi) = (idx % 10, idx % 12);
        let (stem, branch) = ganzhi(y, m, d);
        let predict = score_for(&stems, &branches, si, bi);
        stems[si].push(rec.return_pct);
        branches[bi].push(rec.return_pct);
        let post = score_for(&stems, &branches, si, bi);
        day_scores.push(DayScore {
            date: rec.date.clone(), stem: stem.to_string(), branch: branch.to_string(),
            predict_score: predict, predict_level: fortune_level(predict),
            actual_return: Some(rec.return_pct),
            post_score: Some(post), post_level: Some(fortune_level(post)),
            is_trading_day: true,
        });
    }
    Aggregation { stem_final: stems, branch_final: branches, day_scores }
}
```

测试（放 `aggregate.rs` 尾部 `#[cfg(test)] mod tests`）：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn dr(date: &str, ret: f64) -> DailyReturn {
        DailyReturn { date: date.into(), return_pct: ret, note: String::new(),
            created_at: String::new(), updated_at: String::new() }
    }

    #[test]
    fn pit_predict_then_post_for_same_ganzhi() {
        // 同一干支两天：第一天预测=中性锚（无历史），第二天预测=纳入第一天后的分。
        // 2026-07-01 与 2026-08-30 都是乙酉（间隔 60 天）。构造两条乙酉记录。
        let recs = vec![dr("2026-07-01", 1.0), dr("2026-08-30", -0.5)];
        let agg = aggregate_pit(&recs);
        // 第一条：预测用空累加器 → 综合分 = 中性锚 56.9（层分均 50）
        assert!((agg.day_scores[0].predict_score - 56.9).abs() < 0.5);
        // 第一条盘后并入 +1.0 后，同干支层分升高 → 第二条预测应高于中性
        assert!(agg.day_scores[1].predict_score > 56.9);
    }

    #[test]
    fn win_rate_excludes_flat() {
        // 平盘(=0)不算赢：一条 0.0 收益 → wins=0, days=1
        let mut a = Acc::default();
        a.push(0.0);
        assert_eq!(a.wins, 0);
        assert_eq!(a.days, 1);
        a.push(0.1);
        assert_eq!(a.wins, 1);
    }

    #[test]
    fn empty_returns_neutral() {
        let s = Acc::default().to_layer_stat();
        assert_eq!(s.win_rate, 0.5);
        assert_eq!(s.sample, 0);
    }
}
```

- [ ] **Step 2: 跑纯函数测试**

Run: `node scripts/rust-test.mjs --lib invest::fortune::aggregate`
Expected: 3 passed（`pit_predict_then_post_for_same_ganzhi`、`win_rate_excludes_flat`、`empty_returns_neutral`）

> 注：`aggregate.rs` 依赖 `stats`/`calendar`/`storage`，需先在 `mod.rs` 加 `pub mod aggregate;` 才能编译。若测试报未解析模块，先做 Step 3 的 mod.rs 改动再跑。

- [ ] **Step 3: 注册模块 + 写 `compute_analysis`**

在 `src-tauri/src/invest/fortune/mod.rs` 加（放在 `pub mod calendar;` 后）：

```rust
pub mod aggregate;
pub mod reading;
```

> `reading` 模块在 Task 4 创建；若此刻编译，先只加 `pub mod aggregate;`，Task 4 再加 `pub mod reading;`。

在 `aggregate.rs` 追加 `compute_analysis`（用 scheduler 的交易日/日期工具）：

```rust
use crate::storage::invest::scheduler::{beijing_today, next_trading_day, is_trading_day};

/// 用当前全量累加器给任意日期（含未来）算预测态 DayScore。
fn predict_day(agg: &Aggregation, date: &str) -> Option<DayScore> {
    let (y, m, d) = parse_ymd(date)?;
    let idx = ganzhi_index(y, m, d);
    let (stem, branch) = ganzhi(y, m, d);
    let sc = score_for(&agg.stem_final, &agg.branch_final, idx % 10, idx % 12);
    Some(DayScore {
        date: date.to_string(), stem: stem.to_string(), branch: branch.to_string(),
        predict_score: sc, predict_level: fortune_level(sc),
        actual_return: None, post_score: None, post_level: None,
        is_trading_day: is_trading_day(date).unwrap_or(true),
    })
}
```

`compute_analysis` 主体：

> **日历数据模型（关键，避免三态缺口）**：`calendar` 不能只给已录天——那样月历网格排不出、且「预测态」永无数据。它需覆盖**一段连续日期区间**的每个自然日：过去已录交易日→盘后态（`actualReturn=Some`）；过去未录/未来交易日→预测态（`actualReturn=None`，虚线）；周末/休市→休市态（`isTradingDay=false`）。区间取 `[首条记录所在月 1 号, 当月末]`（无记录时取当月）。已录天直接用 `day_scores` 里那条，其余用 `predict_day` 生成。

```rust
use std::collections::HashMap;

/// 枚举 [start_ym, end_ym] 每个自然日，产出完整日历（三态齐全）。
fn build_calendar(agg: &Aggregation, first_date: &str, today: &str) -> Vec<DayScore> {
    let recorded: HashMap<&str, &DayScore> =
        agg.day_scores.iter().map(|d| (d.date.as_str(), d)).collect();
    let (fy, fm, _) = parse_ymd(first_date).unwrap_or_else(|| parse_ymd(today).unwrap());
    let (ty, tm, _) = parse_ymd(today).unwrap();
    let mut out = Vec::new();
    let (mut y, mut m) = (fy, fm);
    // 逐月推进到当月（含）
    while (y, m) <= (ty, tm) {
        let days_in = chrono::NaiveDate::from_ymd_opt((y + (m == 12) as i64) as i32,
            (m % 12 + 1) as u32, 1).unwrap()
            .pred_opt().unwrap().day();
        for d in 1..=days_in {
            let date = format!("{y:04}-{m:02}-{d:02}");
            if let Some(rec) = recorded.get(date.as_str()) {
                out.push((*rec).clone());               // 盘后态
            } else if let Some(p) = predict_day(agg, &date) {
                out.push(p);                            // 预测/休市态
            }
        }
        if m == 12 { y += 1; m = 1; } else { m += 1; }
    }
    out
}
```

（`build_calendar` 需 `use chrono::Datelike;`。`predict_day` 已给休市态设 `isTradingDay=false`——前端据此渲染灰底；预测态 `actualReturn=None` → 虚线；盘后态来自 `day_scores` → 实心。三态数据齐。）

`compute_analysis` 组装：

```rust
pub fn compute_analysis() -> Result<Analysis, String> {
    let returns = list_returns()?;
    if returns.is_empty() {
        return Ok(Analysis { today: None, tomorrow: None, calendar: vec![] });
    }
    let agg = aggregate_pit(&returns);
    let today = beijing_today();
    let today_card = agg.day_scores.iter().find(|d| d.date == today).cloned()
        .or_else(|| predict_day(&agg, &today));
    let tomorrow_card = next_trading_day(&today).ok().and_then(|nd| predict_day(&agg, &nd));
    // returns 已按 date 升序 → 首条即最早
    let first = returns.first().map(|r| r.date.as_str()).unwrap_or(today.as_str());
    let calendar = build_calendar(&agg, first, &today);
    Ok(Analysis { today: today_card, tomorrow: tomorrow_card, calendar })
}
```


- [ ] **Step 4: 写 `compute_overview` + `compute_data_summary` 的类型与函数**

在 `aggregate.rs` 顶部类型区追加：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForecastItem {
    pub label: String,          // "最强天干" 等
    pub date: String, pub weekday: String,
    pub ganzhi: String, pub score: f64, pub level: FortuneLevel,
    pub is_strong: bool,        // true 红左边，false 绿左边
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub stems: Vec<LayerRow>,   // 10 天干
    pub branches: Vec<LayerRow>,// 12 地支
    pub forecasts: Vec<ForecastItem>,  // 4 路（最强/最弱 天干/地支）
}
```

> **与 spec §6「6 路预告」的偏差（已定）**：spec 原写 6 路含「最强/最弱组合」，但组合已从综合分消融剔除（无组合分可排序）。故预告改为 **4 路**：最强天干、最弱天干、最强地支、最弱地支。前端 `FortuneStemBranchTab` 相应渲染 4 张预告卡。

`compute_overview` 主体（`layer_row` 辅助 + 排名找未来日）：

```rust
fn layer_row(name: &str, acc: Acc) -> LayerRow {
    let stat = acc.to_layer_stat();
    let score = layer_score(&stat);
    LayerRow { name: name.to_string(), avg_return: stat.avg_return_pct,
        win_rate: stat.win_rate, sample: stat.sample, score, level: fortune_level(score) }
}

/// 从 today+1 起向后扫最多 60 个自然日，找首个 stem_idx(或 branch_idx) 命中且为交易日的日期。
fn next_date_with(is_stem: bool, target_idx: usize, from: &str) -> Option<(String, String)> {
    let (mut y, mut m, mut d) = parse_ymd(from)?;
    for _ in 0..60 {
        // 前进一天（借 chrono）
        let nd = chrono::NaiveDate::from_ymd_opt(y as i32, m as u32, d as u32)?
            .succ_opt()?;
        y = nd.year() as i64; m = nd.month() as i64; d = nd.day() as i64;
        let date = format!("{y:04}-{m:02}-{d:02}");
        let idx = ganzhi_index(y, m, d);
        let hit = if is_stem { idx % 10 == target_idx } else { idx % 12 == target_idx };
        if hit && is_trading_day(&date).unwrap_or(true) {
            let wd = ["周一","周二","周三","周四","周五","周六","周日"]
                [nd.weekday().num_days_from_monday() as usize];
            return Some((date, wd.to_string()));
        }
    }
    None
}
```

（`next_date_with` 需 `use chrono::Datelike;`）

`compute_overview` 组装：

```rust
pub fn compute_overview() -> Result<Overview, String> {
    let returns = list_returns()?;
    let agg = aggregate_pit(&returns);
    let stems: Vec<LayerRow> = (0..10).map(|i| layer_row(STEMS[i], agg.stem_final[i])).collect();
    let branches: Vec<LayerRow> = (0..12).map(|i| layer_row(BRANCHES[i], agg.branch_final[i])).collect();

    let today = beijing_today();
    let mut forecasts = Vec::new();
    // 仅在有数据时给预告；用分数排名找最强/最弱 idx
    let pick = |rows: &[LayerRow]| -> (usize, usize) {
        let mut hi = 0; let mut lo = 0;
        for (i, r) in rows.iter().enumerate() {
            if r.score > rows[hi].score { hi = i; }
            if r.score < rows[lo].score { lo = i; }
        }
        (hi, lo)
    };
    if !returns.is_empty() {
        let (sh, sl) = pick(&stems);
        let (bh, bl) = pick(&branches);
        let specs: [(&str, bool, usize, bool); 4] = [
            ("最强天干", true, sh, true), ("最弱天干", true, sl, false),
            ("最强地支", false, bh, true), ("最弱地支", false, bl, false),
        ];
        for (label, is_stem, idx, strong) in specs {
            if let Some((date, wd)) = next_date_with(is_stem, idx, &today) {
                let (y, m, d) = parse_ymd(&date).unwrap();
                let gi = ganzhi_index(y, m, d);
                let sc = score_for(&agg.stem_final, &agg.branch_final, gi % 10, gi % 12);
                forecasts.push(ForecastItem {
                    label: label.into(), date, weekday: wd,
                    ganzhi: format!("{}{}", STEMS[gi % 10], BRANCHES[gi % 12]),
                    score: sc, level: fortune_level(sc), is_strong: strong,
                });
            }
        }
    }
    Ok(Overview { stems, branches, forecasts })
}
```

- [ ] **Step 5: 写 `compute_data_summary` 类型与函数**

类型区追加：

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthStat { pub month: String, pub avg_return: f64 }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSummary {
    pub total_days: u32, pub win_days: u32, pub win_rate: f64,
    pub cumulative_return: f64, pub avg_daily_return: f64,
    pub top_stems: Vec<LayerRow>,   // Top3 排行（分数降序）
    pub top_branches: Vec<LayerRow>,
    pub risk_stems: Vec<LayerRow>,  // Top3 风险（分数升序）
    pub risk_branches: Vec<LayerRow>,
    pub monthly: Vec<MonthStat>,
}
```

函数：

```rust
pub fn compute_data_summary() -> Result<DataSummary, String> {
    let returns = list_returns()?;
    let agg = aggregate_pit(&returns);
    let total_days = returns.len() as u32;
    let win_days = returns.iter().filter(|r| r.return_pct > 0.0).count() as u32;
    let win_rate = if total_days > 0 { win_days as f64 / total_days as f64 } else { 0.0 };
    let cumulative_return: f64 = returns.iter().map(|r| r.return_pct).sum();
    let avg_daily_return = if total_days > 0 { cumulative_return / total_days as f64 } else { 0.0 };

    let mut stems: Vec<LayerRow> = (0..10).map(|i| layer_row(STEMS[i], agg.stem_final[i]))
        .filter(|r| r.sample > 0).collect();
    let mut branches: Vec<LayerRow> = (0..12).map(|i| layer_row(BRANCHES[i], agg.branch_final[i]))
        .filter(|r| r.sample > 0).collect();
    stems.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    branches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    let top_stems = stems.iter().take(3).cloned().collect();
    let top_branches = branches.iter().take(3).cloned().collect();
    let risk_stems = stems.iter().rev().take(3).cloned().collect();
    let risk_branches = branches.iter().rev().take(3).cloned().collect();

    // 月度：按 "YYYY-MM" 分组平均
    let mut by_month: std::collections::BTreeMap<String, (f64, u32)> = Default::default();
    for r in &returns {
        if r.date.len() >= 7 {
            let e = by_month.entry(r.date[..7].to_string()).or_default();
            e.0 += r.return_pct; e.1 += 1;
        }
    }
    let monthly = by_month.into_iter()
        .map(|(month, (sum, n))| MonthStat { month, avg_return: sum / n as f64 })
        .collect();

    Ok(DataSummary { total_days, win_days, win_rate, cumulative_return, avg_daily_return,
        top_stems, top_branches, risk_stems, risk_branches, monthly })
}
```

- [ ] **Step 6: 编译 + 跑全部 aggregate 测试 + clippy**

Run: `node scripts/rust-test.mjs --lib invest::fortune::aggregate`
Expected: 3 passed

Run: `npm run rust:clippy`
Expected: 无 aggregate.rs 相关 warning（`-D warnings` 下必须零告警）

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/invest/fortune/aggregate.rs src-tauri/src/invest/fortune/mod.rs
git commit -m "feat(fortune): point-in-time single-pass aggregation — analysis/overview/summary"
```

---

### Task 3: 命令层 — 数据 CRUD + 查询命令（不含 AI）

**Files:**
- Create: `src-tauri/src/commands/fortune.rs`
- Modify: `src-tauri/src/commands/mod.rs`（加 `pub mod fortune;`）
- Modify: `src-tauri/src/lib.rs`（`generate_handler![]` 注册，invest 块尾部）

**Interfaces:**
- Consumes：`crate::storage::invest::fortune`、`crate::invest::fortune::aggregate`
- Produces（前端 invoke 目标）：
  - `fortune_upsert_return(date: String, returnPct: f64, note: String)` → `Result<(), String>`
  - `fortune_batch_upsert(entries: Vec<BatchEntry>)` → `Result<(), String>`
  - `fortune_delete_return(date: String)` → `Result<(), String>`
  - `fortune_get_analysis()` → `Result<Analysis, String>`
  - `fortune_get_overview()` → `Result<Overview, String>`
  - `fortune_get_data_summary()` → `Result<DataSummary, String>`
  - （AI 两命令在 Task 4）

- [ ] **Step 1: 写 6 个命令**

创建 `src-tauri/src/commands/fortune.rs`：

```rust
//! 每日盈记 Tauri 命令边界。薄封装：委托 storage + aggregate。
use crate::invest::fortune::aggregate::{self, Analysis, Overview, DataSummary};
use crate::storage::invest::fortune as store;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchEntry { pub date: String, pub return_pct: f64, pub note: String }

#[tauri::command]
pub fn fortune_upsert_return(date: String, return_pct: f64, note: String) -> Result<(), String> {
    store::upsert_return(&date, return_pct, &note)
}

#[tauri::command]
pub fn fortune_batch_upsert(entries: Vec<BatchEntry>) -> Result<(), String> {
    for e in &entries {
        store::upsert_return(&e.date, e.return_pct, &e.note)?;
    }
    Ok(())
}

#[tauri::command]
pub fn fortune_delete_return(date: String) -> Result<(), String> {
    store::delete_return(&date)
}

#[tauri::command]
pub fn fortune_get_analysis() -> Result<Analysis, String> { aggregate::compute_analysis() }

#[tauri::command]
pub fn fortune_get_overview() -> Result<Overview, String> { aggregate::compute_overview() }

#[tauri::command]
pub fn fortune_get_data_summary() -> Result<DataSummary, String> { aggregate::compute_data_summary() }
```

> Tauri 命令参数名：Rust `return_pct` → 前端传 `returnPct`（Tauri 自动 camelCase 转换参数键）。

- [ ] **Step 2: 注册模块与 handler**

在 `src-tauri/src/commands/mod.rs` 加（按字母序，`pub mod files;` 后）：

```rust
pub mod fortune;
```

在 `src-tauri/src/lib.rs` 的 `generate_handler![]` 内，invest 命令块尾部（`commands::invest_cleanup::invest_cleanup_apply,` 附近）追加：

```rust
            commands::fortune::fortune_upsert_return,
            commands::fortune::fortune_batch_upsert,
            commands::fortune::fortune_delete_return,
            commands::fortune::fortune_get_analysis,
            commands::fortune::fortune_get_overview,
            commands::fortune::fortune_get_data_summary,
```

- [ ] **Step 3: 编译验证**

Run: `npm run rust:clippy`
Expected: 编译通过，无 fortune 相关 warning。

- [ ] **Step 4: 手动冒烟（可选，起 app 后 devtools console）**

在 app devtools 里：
```js
await __TAURI__.core.invoke('fortune_upsert_return', { date: '2026-07-01', returnPct: 1.5, note: 't' });
await __TAURI__.core.invoke('fortune_get_analysis');  // 应返回 today 卡非 null
```
Expected: 无报错，analysis.today.stem/branch 有值。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/fortune.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(fortune): Tauri commands — upsert/batch/delete/analysis/overview/summary"
```

---

### Task 4: AI 解读模块 — 复用 committee CLI executor（独立信号量）

**Files:**
- Create: `src-tauri/src/invest/fortune/reading.rs`
- Modify: `src-tauri/src/invest/fortune/mod.rs`（`pub mod reading;` 若未加）
- Modify: `src-tauri/src/commands/fortune.rs`（加 2 命令）
- Modify: `src-tauri/src/lib.rs`（注册 2 命令）

**Interfaces:**
- Consumes：`crate::invest::committee::cli_executor::CliCommitteeExecutor`（`global()` + `run_role(...)`）、`crate::invest::fortune::aggregate::compute_analysis`、`crate::storage::invest::fortune::{insert_reading, get_latest_reading}`
- Produces：
  - `pub async fn generate_reading(date: &str) -> Result<String, String>`（生成并持久化，返回内容）

- [ ] **Step 1: 写 reading 模块（独立信号量 + 极简 prompt）**

创建 `src-tauri/src/invest/fortune/reading.rs`：

> **信号量语义（勘误）**：`CliCommitteeExecutor` derive `Clone`，其 `semaphore: Arc<Semaphore>` 在 `global()` 的所有 clone 间**共享**。所以 `run_role` 内部已排队进委员会那把 `MAX_CLI_CONCURRENT` 闸——fortune 解读**并非**独立于委员会，二者共用同一并发池。下面的 `READING_SEM(permits=1)` 作用是**防用户连点**（串行化 fortune 自己的解读请求），不是"避免与委员会抢占"。若无此需求可直接删掉、裸调 `run_role`（它自带排队）。

```rust
//! 每日盈记 AI 解读。复用 committee 的 CliCommitteeExecutor::run_role
//! （其内部信号量与委员会共享）；此处额外加 permits=1 闸做防连点串行化。
use std::sync::Arc;
use tokio::sync::Semaphore;
use crate::invest::fortune::aggregate::compute_analysis;
use crate::storage::invest::fortune::insert_reading;

/// 防连点闸：同一时刻最多 1 个 fortune 解读排队（注：CLI 并发仍受委员会共享闸约束）。
static READING_SEM: std::sync::OnceLock<Arc<Semaphore>> = std::sync::OnceLock::new();
fn sem() -> Arc<Semaphore> {
    READING_SEM.get_or_init(|| Arc::new(Semaphore::new(1))).clone()
}

pub async fn generate_reading(date: &str) -> Result<String, String> {
    // 取当天卡的干支 + 分数作 prompt 素材
    let analysis = compute_analysis()?;
    let card = analysis.today.as_ref()
        .filter(|c| c.date == date)
        .or(analysis.tomorrow.as_ref())
        .ok_or_else(|| "无当日数据，无法生成解读".to_string())?;
    let sys = "你是一个轻松的股市『每日盈记』解读助手。基于用户给的当日干支与历史统计，\
        用 2-3 句中文给出偏娱乐的吉凶点评，口吻轻松，结尾提醒仅供参考娱乐。不要免责长篇。";
    let user = format!(
        "日期 {}，干支「{}{}」，综合评分 {:.0}。请给一句话点评。",
        card.date, card.stem, card.branch, card.predict_score);

    let _permit = sem().acquire().await.map_err(|e| format!("信号量获取失败: {e}"))?;
    let exec = crate::invest::committee::cli_executor::CliCommitteeExecutor::global()
        .ok_or_else(|| "未找到 claude CLI，无法生成解读".to_string())?;
    let content = exec.run_role(sys, &user, 60, None, None).await?;
    insert_reading(date, &content)?;
    Ok(content)
}
```

> `_permit` 在函数作用域末尾释放。若 `global()` 返回 None（无 claude CLI），返回明确错误供前端展示。注意 `run_role` 第 4/5 参数 `settings_path`/`cancel` 均传 `None`（本功能不需第三方 settings 或取消）。

- [ ] **Step 2: 加 2 命令 + 注册**

在 `src-tauri/src/invest/fortune/mod.rs` 确认有 `pub mod reading;`（Task 2 Step 3 若已加则跳过）。

在 `src-tauri/src/commands/fortune.rs` 追加：

```rust
#[tauri::command]
pub async fn fortune_generate_reading(date: String) -> Result<String, String> {
    crate::invest::fortune::reading::generate_reading(&date).await
}

#[tauri::command]
pub fn fortune_get_reading(date: String) -> Result<Option<String>, String> {
    store::get_latest_reading(&date)
}
```

在 `src-tauri/src/lib.rs` 的 `generate_handler![]` fortune 块追加：

```rust
            commands::fortune::fortune_generate_reading,
            commands::fortune::fortune_get_reading,
```

- [ ] **Step 3: 编译验证**

Run: `npm run rust:clippy`
Expected: 编译通过，无 fortune/reading 相关 warning。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/invest/fortune/reading.rs src-tauri/src/invest/fortune/mod.rs src-tauri/src/commands/fortune.rs src-tauri/src/lib.rs
git commit -m "feat(fortune): AI reading via committee CLI executor (independent semaphore)"
```

---

### Task 5: 前端 store + TS 类型 + i18n key

**Files:**
- Create: `src/lib/stores/fortune-store.svelte.ts`
- Modify: `messages/zh-CN.json`、`messages/en.json`（加 `fortune_*` key）

**Interfaces:**
- Produces（供组件）：`fortuneStore` 单例，含 `$state` 字段 + `loadAll()`、`upsert()`、`batchUpsert()`、`deleteReturn()`、`generateReading()`、`getReading()`。

- [ ] **Step 1: 加 i18n key（两文件同名，en 用中文值 fallback）**

在 `messages/zh-CN.json` 与 `messages/en.json` **都**加以下 key（**两文件值都填中文**——满足 `i18n:check` 的键对齐，且本功能只做中文）：

```json
  "invest_tab_fortune": "每日盈记",
  "fortune_sub_analysis": "分析",
  "fortune_sub_stembranch": "干支总览",
  "fortune_sub_data": "数据总览",
  "fortune_record_btn": "＋ 录入收益",
  "fortune_predict": "预测",
  "fortune_post": "盘后",
  "fortune_empty_hint": "录入第一笔收益后即可查看干支评分",
  "fortune_insufficient": "数据不足",
  "fortune_generate_reading": "生成解读",
  "fortune_generating": "生成中…",
  "fortune_reading_failed": "解读生成失败，可重试",
  "fortune_kpi_total": "总记录天数",
  "fortune_kpi_win": "盈利天数",
  "fortune_kpi_winrate": "胜率",
  "fortune_kpi_cumulative": "累计收益",
  "fortune_kpi_avg": "平均日收益",
  "fortune_top_rank": "排行 Top3",
  "fortune_top_risk": "风险 Top3",
  "fortune_monthly": "月度统计"
```

> `invest_tab_fortune` 在两文件都用中文「每日盈记」。运行 `npm run i18n:check` 应通过（键集一致）。

- [ ] **Step 2: 写 store（TS 类型 inline，与后端 camelCase 对齐）**

创建 `src/lib/stores/fortune-store.svelte.ts`：

```ts
import { getTransport } from "$lib/transport";

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return getTransport().invoke<T>(cmd, args);
}

export type FortuneLevel =
  | "great_fortune" | "fortune" | "neutral" | "misfortune" | "great_misfortune";

export interface DayScore {
  date: string; stem: string; branch: string;
  predictScore: number; predictLevel: FortuneLevel;
  actualReturn: number | null; postScore: number | null; postLevel: FortuneLevel | null;
  isTradingDay: boolean;
}
export interface LayerRow {
  name: string; avgReturn: number; winRate: number; sample: number;
  score: number; level: FortuneLevel;
}
export interface ForecastItem {
  label: string; date: string; weekday: string; ganzhi: string;
  score: number; level: FortuneLevel; isStrong: boolean;
}
export interface Analysis { today: DayScore | null; tomorrow: DayScore | null; calendar: DayScore[]; }
export interface Overview { stems: LayerRow[]; branches: LayerRow[]; forecasts: ForecastItem[]; }
export interface MonthStat { month: string; avgReturn: number; }
export interface DataSummary {
  totalDays: number; winDays: number; winRate: number;
  cumulativeReturn: number; avgDailyReturn: number;
  topStems: LayerRow[]; topBranches: LayerRow[];
  riskStems: LayerRow[]; riskBranches: LayerRow[]; monthly: MonthStat[];
}
export interface BatchEntry { date: string; returnPct: number; note: string; }
```

store class 主体（续同文件）：

```ts
class FortuneStore {
  analysis = $state<Analysis | null>(null);
  overview = $state<Overview | null>(null);
  summary = $state<DataSummary | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);
  readingBusy = $state(false);

  async loadAll(): Promise<void> {
    this.loading = true; this.error = null;
    try {
      const [analysis, overview, summary] = await Promise.all([
        invoke<Analysis>("fortune_get_analysis"),
        invoke<Overview>("fortune_get_overview"),
        invoke<DataSummary>("fortune_get_data_summary"),
      ]);
      this.analysis = analysis; this.overview = overview; this.summary = summary;
    } catch (e) { this.error = String(e); }
    finally { this.loading = false; }
  }

  async upsert(date: string, returnPct: number, note = ""): Promise<void> {
    await invoke("fortune_upsert_return", { date, returnPct, note });
    await this.loadAll();   // invalidate：预测↔盘后自动切换
  }

  async batchUpsert(entries: BatchEntry[]): Promise<void> {
    await invoke("fortune_batch_upsert", { entries });
    await this.loadAll();
  }

  async deleteReturn(date: string): Promise<void> {
    await invoke("fortune_delete_return", { date });
    await this.loadAll();
  }

  async generateReading(date: string): Promise<string> {
    this.readingBusy = true;
    try { return await invoke<string>("fortune_generate_reading", { date }); }
    finally { this.readingBusy = false; }
  }

  async getReading(date: string): Promise<string | null> {
    return await invoke<string | null>("fortune_get_reading", { date });
  }
}

export const fortuneStore = new FortuneStore();
```

- [ ] **Step 3: 写 store 单测（Vitest，mock transport）**

创建 `src/lib/stores/fortune-store.test.ts`（与被测文件同目录，参照 `invest-committee-store.test.ts` 的 `vi.mock('$lib/transport', ...)` 写法）：

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("$lib/transport", () => ({ getTransport: () => ({ invoke: invokeMock }) }));

describe("fortuneStore", () => {
  beforeEach(() => { invokeMock.mockReset(); });

  it("upsert 后重新 loadAll", async () => {
    const { fortuneStore } = await import("../fortune-store.svelte");
    invokeMock.mockResolvedValue({ today: null, tomorrow: null, calendar: [] });
    invokeMock.mockResolvedValueOnce(undefined);  // upsert 调用
    await fortuneStore.upsert("2026-07-01", 1.5);
    // upsert(1) + loadAll 的 3 个查询 = 4 次
    expect(invokeMock).toHaveBeenCalledWith("fortune_upsert_return",
      { date: "2026-07-01", returnPct: 1.5, note: "" });
  });
});
```

- [ ] **Step 4: 跑测试**

Run: `npm test -- fortune-store`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/lib/stores/fortune-store.svelte.ts src/lib/stores/fortune-store.test.ts messages/zh-CN.json messages/en.json
git commit -m "feat(fortune): frontend store + TS types + i18n keys"
```

---

### Task 6: 接入 invest 页 — 新增 fortune 顶级 tab

**Files:**
- Modify: `src/routes/invest/+page.svelte`

**Interfaces:**
- Consumes：Task 7/8 的组件（此步先建骨架占位，逐个填充）。
- 本 task 先创建 4 个组件的**最小骨架**（挂 store.loadAll），保证 tab 可切换、页面可编译。

- [ ] **Step 1: 建 4 个组件骨架**

创建 `src/lib/components/invest/FortuneAnalysisTab.svelte`（骨架，Task 7 填充）：

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { fortuneStore } from '$lib/stores/fortune-store.svelte';
  onMount(() => { fortuneStore.loadAll(); });
</script>

<div class="text-[var(--text-tertiary)]">分析（占位）</div>
```

同样建 `FortuneStemBranchTab.svelte`、`FortuneDataTab.svelte`（各自 `<div>占位</div>`，Task 8 填充）。`FortuneRecordDialog.svelte`、`FortuneCalendar.svelte` 在 Task 7/9 建。

- [ ] **Step 2: 接入 +page.svelte**

在 `src/routes/invest/+page.svelte` 第 29 行 `type InvestTab` 加 `'fortune'`：

```ts
  type InvestTab = 'dashboard' | 'committee' | 'strategy' | 'trades' | 'system' | 'fortune';
  type FortuneSubTab = 'analysis' | 'stembranch' | 'data';
```

在第 33 行附近加子 tab 状态：

```ts
  let fortuneSubTab: FortuneSubTab = $state('analysis');
```

在 `tabs` 数组（第 37-43 行）`system` 后加：

```ts
    { id: 'fortune', label: t('invest_tab_fortune') },
```

加子 tab 定义（参照 `systemSubTabs`）：

```ts
  const fortuneSubTabs: { id: FortuneSubTab; label: string }[] = $derived([
    { id: 'analysis', label: t('fortune_sub_analysis') },
    { id: 'stembranch', label: t('fortune_sub_stembranch') },
    { id: 'data', label: t('fortune_sub_data') },
  ]);
```

在 `<script>` 顶部 import 区加：

```ts
  import FortuneAnalysisTab from '$lib/components/invest/FortuneAnalysisTab.svelte';
  import FortuneStemBranchTab from '$lib/components/invest/FortuneStemBranchTab.svelte';
  import FortuneDataTab from '$lib/components/invest/FortuneDataTab.svelte';
```

在渲染区（`{:else if activeTab === 'system'}` 分支之后、内容 `</div>` 之前）加 fortune 分支，含子 tab 切换（复用 system 子 tab 的胶囊样式 class）：

```svelte
    {:else if activeTab === 'fortune'}
      <div class="mb-[var(--space-4)] flex flex-wrap items-center gap-[var(--space-2)]">
        {#each fortuneSubTabs as subTab}
          <button
            class="rounded-full px-[var(--space-3)] py-[var(--space-1)] text-[12px] font-medium transition-colors"
            class:bg-[var(--accent-muted)]={fortuneSubTab === subTab.id}
            class:text-[var(--accent)]={fortuneSubTab === subTab.id}
            class:text-[var(--text-tertiary)]={fortuneSubTab !== subTab.id}
            onclick={() => (fortuneSubTab = subTab.id)}
          >{subTab.label}</button>
        {/each}
      </div>
      {#if fortuneSubTab === 'analysis'}
        <FortuneAnalysisTab />
      {:else if fortuneSubTab === 'stembranch'}
        <FortuneStemBranchTab />
      {:else if fortuneSubTab === 'data'}
        <FortuneDataTab />
      {/if}
```

- [ ] **Step 3: 编译 + 目视**

Run: `npm run build`（或 `npm run check` 做 svelte-check）
Expected: 无类型错误。

- [ ] **Step 4: Commit**

```bash
git add src/routes/invest/+page.svelte src/lib/components/invest/FortuneAnalysisTab.svelte src/lib/components/invest/FortuneStemBranchTab.svelte src/lib/components/invest/FortuneDataTab.svelte
git commit -m "feat(fortune): wire fortune tab into invest page with sub-tabs + skeletons"
```

---

### Task 7: 分析 tab — 今日/明日卡 + AI 解读 + 日历

> **视觉基线**：`fortune_demo.html`（328 行，已按真实 token 对齐）。本 task 组件按 demo 还原布局；下方给出**数据绑定逻辑**与**三态日历逻辑**（demo 的 JS 是纯静态数据，需替换为 store 真实数据）。

**Files:**
- Create: `src/lib/components/invest/fortune-helpers.ts`（共享：level→颜色/文案、收益格式）
- Modify: `src/lib/components/invest/FortuneAnalysisTab.svelte`（填充骨架）
- Create: `src/lib/components/invest/FortuneCalendar.svelte`

**Interfaces:**
- Produces：`fortune-helpers.ts` 导出 `levelLabel(level)`、`levelColor(level)`、`fmtReturn(pct)`、`fmtScore(n)`。

- [ ] **Step 1: 写共享 helper**

创建 `src/lib/components/invest/fortune-helpers.ts`：

```ts
import type { FortuneLevel } from "$lib/stores/fortune-store.svelte";

const LABELS: Record<FortuneLevel, string> = {
  great_fortune: "大吉", fortune: "吉", neutral: "平",
  misfortune: "凶", great_misfortune: "大凶",
};
// 吉凶色纳入暖色系：大吉/吉=红→琥珀，平=灰，凶/大凶=浅绿→深绿（红涨绿跌一致）
const COLORS: Record<FortuneLevel, string> = {
  great_fortune: "var(--up)", fortune: "#c99a5e", neutral: "var(--flat)",
  misfortune: "#7f9d6d", great_misfortune: "#5f7a52",
};
export function levelLabel(l: FortuneLevel): string { return LABELS[l]; }
export function levelColor(l: FortuneLevel): string { return COLORS[l]; }
export function fmtScore(n: number): string { return n.toFixed(0); }
export function fmtReturn(pct: number): string {
  const s = pct >= 0 ? "+" : "";
  return `${s}${pct.toFixed(2)}%`;
}
/** 收益正负 → 红涨绿跌颜色 */
export function returnColor(pct: number): string {
  if (pct > 0) return "var(--up)";
  if (pct < 0) return "var(--down)";
  return "var(--flat)";
}
```

- [ ] **Step 2: 填充 FortuneAnalysisTab（今日/明日卡 + 解读卡）**

改写 `FortuneAnalysisTab.svelte`。核心逻辑（布局照 `fortune_demo.html` 的 `renderAnalysis`，line 165-196）：

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { fortuneStore } from '$lib/stores/fortune-store.svelte';
  import type { DayScore } from '$lib/stores/fortune-store.svelte';
  import { levelLabel, levelColor, fmtScore, fmtReturn, returnColor } from './fortune-helpers';
  import FortuneCalendar from './FortuneCalendar.svelte';
  import FortuneRecordDialog from './FortuneRecordDialog.svelte';

  let showDialog = $state(false);
  let reading = $state<string | null>(null);
  let readingError = $state<string | null>(null);

  const analysis = $derived(fortuneStore.analysis);
  const today = $derived(analysis?.today ?? null);

  onMount(async () => {
    await fortuneStore.loadAll();
    if (today) reading = await fortuneStore.getReading(today.date).catch(() => null);
  });

  // 卡副标签：已录=盘后，未录=预测
  function cardLabel(d: DayScore): string {
    return d.actualReturn != null ? t('fortune_post') : t('fortune_predict');
  }
  function cardScore(d: DayScore): number {
    return d.postScore ?? d.predictScore;
  }

  async function genReading() {
    if (!today) return;
    readingError = null;
    try { reading = await fortuneStore.generateReading(today.date); }
    catch (e) { readingError = String(e); }
  }
</script>
```

模板（结构照 demo，空状态用 `fortune_empty_hint`）：

```svelte
{#if !analysis || analysis.calendar.length === 0}
  <div class="rounded-[var(--radius-lg)] border border-dashed border-border bg-[var(--bg-card)] p-[var(--space-6)] text-center text-[13px] text-[var(--text-tertiary)]">
    {t('fortune_empty_hint')}
  </div>
{:else}
  <div class="grid grid-cols-1 gap-[var(--space-4)] lg:grid-cols-[1fr_1.4fr]">
    <div class="flex flex-col gap-[var(--space-4)]">
      {#if today}
        {@const sc = cardScore(today)}
        {@const lvl = today.postLevel ?? today.predictLevel}
        <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
          <div class="mb-[var(--space-3)] flex justify-between text-[12px] text-[var(--text-secondary)]">
            <span>今日 {today.date}</span><span>{cardLabel(today)}</span>
          </div>
          <div class="flex items-center gap-[var(--space-4)]">
            <div class="font-mono text-[44px] font-bold" style="color:var(--accent)">{today.stem}{today.branch}</div>
            <div class="font-mono text-[52px] font-extrabold" style="color:{levelColor(lvl)}">{fmtScore(sc)}</div>
            <span class="rounded-[var(--radius-sm)] px-[var(--space-3)] py-[var(--space-1)] text-[15px] font-semibold"
              style="color:{levelColor(lvl)}">{levelLabel(lvl)}</span>
          </div>
          {#if today.actualReturn != null}
            <div class="mt-[var(--space-2)] font-mono text-[14px]" style="color:{returnColor(today.actualReturn)}">
              实测 {fmtReturn(today.actualReturn)}
            </div>
          {/if}
        </div>
      {/if}
      <!-- 明日卡：同结构，用 analysis.tomorrow，副标签恒为「预测」。省略重复代码，按今日卡改 today→tomorrow、去掉实测行。 -->
    </div>

    <!-- 右侧 AI 解读卡 -->
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
      <div class="mb-[var(--space-3)] flex items-center justify-between">
        <span class="text-[13px] text-[var(--text-secondary)]">AI 解读</span>
        <button class="rounded-[var(--radius-sm)] bg-[var(--accent)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] font-semibold text-[var(--bg-base)] disabled:opacity-50"
          disabled={fortuneStore.readingBusy || !today} onclick={genReading}>
          {fortuneStore.readingBusy ? t('fortune_generating') : t('fortune_generate_reading')}
        </button>
      </div>
      {#if fortuneStore.readingBusy}
        <div class="h-16 animate-pulse rounded bg-[var(--bg-input)]"></div>
      {:else if readingError}
        <div class="text-[13px]" style="color:var(--color-error)">{t('fortune_reading_failed')}</div>
      {:else if reading}
        <p class="text-[13px] leading-relaxed text-[var(--text-primary)]">{reading}</p>
      {/if}
    </div>
  </div>

  <div class="mt-[var(--space-4)]"><FortuneCalendar /></div>
{/if}

<div class="mt-[var(--space-4)]">
  <button class="rounded-[var(--radius-sm)] bg-[var(--accent)] px-[var(--space-4)] py-[var(--space-2)] text-[13px] font-semibold text-[var(--bg-base)]"
    onclick={() => (showDialog = true)}>{t('fortune_record_btn')}</button>
</div>
{#if showDialog}
  <FortuneRecordDialog onclose={() => (showDialog = false)} />
{/if}
```

- [ ] **Step 3: 写 FortuneCalendar（三态视觉）**

创建 `FortuneCalendar.svelte`。**三态逻辑**（demo line 100-159 的通道分离：吉凶=颜色 hue，预测/盘后=填充样式）：
- **盘后**（`actualReturn != null`）：实心色块 + 实线同色边 + 分数加粗。
- **预测**（`actualReturn == null`）：透明底 + **虚线**同色边 + "预测"字样。
- **休市**（`isTradingDay == false`）：灰底。
- **今日**：金框（`var(--accent)`）。

```svelte
<script lang="ts">
  import { fortuneStore } from '$lib/stores/fortune-store.svelte';
  import type { DayScore } from '$lib/stores/fortune-store.svelte';
  import { levelColor, fmtScore } from './fortune-helpers';

  // 后端已产出完整月历（三态齐）；这里按 "YYYY-MM" 分组，默认展示最后一个月，可翻页。
  const all = $derived(fortuneStore.analysis?.calendar ?? []);
  const months = $derived([...new Set(all.map((d) => d.date.slice(0, 7)))]);  // 升序
  let monthIdx = $state(0);   // 相对 months 末尾的偏移，0=最新月
  const curMonth = $derived(months[months.length - 1 - monthIdx] ?? '');
  const cells = $derived(all.filter((d) => d.date.startsWith(curMonth)));

  // 单格样式：三态 = 休市/预测/盘后
  function cellStyle(d: DayScore): { border: string; bg: string; dashed: boolean } {
    if (!d.isTradingDay) return { border: 'var(--border)', bg: 'var(--bg-input)', dashed: false };
    const lvl = d.postLevel ?? d.predictLevel;
    const color = levelColor(lvl);
    const isPost = d.actualReturn != null;
    return { border: color, bg: isPost ? color + '22' : 'transparent', dashed: !isPost };
  }
</script>
```

（模板：`cells` 按周几对齐排入 7 列网格（首格用 `new Date(curMonth+'-01').getDay()` 补空位），每格 `style="border:1px {dashed?'dashed':'solid'} {border};background:{bg}"`，格内显示 日/干支/分数；顶部 `‹ {curMonth} ›` 翻页（`monthIdx` 加减，边界禁用）；底部三态图例。布局照 demo `calendarHtml()` line 100-159。虚实边框不依赖颜色深浅，色盲可辨。）

- [ ] **Step 4: 编译 + 目视验证**

Run: `npm run check`
Expected: 无类型错误。手动起 app（`npm run tauri dev`），录入几笔收益，确认今日卡显示干支+分数、日历格三态可辨、生成解读按钮可点。

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/invest/fortune-helpers.ts src/lib/components/invest/FortuneAnalysisTab.svelte src/lib/components/invest/FortuneCalendar.svelte
git commit -m "feat(fortune): analysis tab — today/tomorrow cards, AI reading, three-state calendar"
```

---

### Task 8: 干支总览 tab + 数据总览 tab

**Files:**
- Modify: `src/lib/components/invest/FortuneStemBranchTab.svelte`
- Modify: `src/lib/components/invest/FortuneDataTab.svelte`

**Interfaces:**
- Consumes：`fortuneStore.overview`（stems/branches/forecasts）、`fortuneStore.summary`。

- [ ] **Step 1: FortuneStemBranchTab（天干表10 + 地支表12 + 4 预告卡）**

改写。布局照 demo `renderStemBranch`（line 197-220）。数据来自 `fortuneStore.overview`：

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { fortuneStore } from '$lib/stores/fortune-store.svelte';
  import { levelLabel, levelColor, fmtScore, fmtReturn } from './fortune-helpers';
  const ov = $derived(fortuneStore.overview);
  onMount(() => { if (!ov) fortuneStore.loadAll(); });
</script>

{#if !ov || ov.stems.every((s) => s.sample === 0)}
  <div class="text-[var(--text-tertiary)]">{t('fortune_insufficient')}</div>
{:else}
  <!-- 天干表：ov.stems（10 行），每行 名/均收益/胜率/次数/分数/吉凶 chip -->
  <!-- 地支表：ov.branches（12 行），同结构 -->
  <!-- 预告：ov.forecasts（4 张卡），isStrong ? 红左边 : 绿左边 -->
  <div class="grid grid-cols-2 gap-[var(--space-3)] sm:grid-cols-4">
    {#each ov.forecasts as f}
      <div class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-card)] p-[var(--space-3)]"
        style="border-left:3px solid {f.isStrong ? 'var(--up)' : 'var(--down)'}">
        <div class="text-[11px] text-[var(--text-tertiary)]">{f.label}</div>
        <div class="font-mono text-[18px]" style="color:var(--accent)">{f.ganzhi}</div>
        <div class="text-[12px] text-[var(--text-secondary)]">{f.date} {f.weekday}</div>
        <div class="text-[12px]" style="color:{levelColor(f.level)}">{fmtScore(f.score)} {levelLabel(f.level)}</div>
      </div>
    {/each}
  </div>
{/if}
```

（表格行的完整 HTML 照 demo 还原：`<table>` 或 grid，列头 干支/均收益/胜率/次数/分数/吉凶；均收益用 `returnColor` 上色，分数用 `levelColor`。）

- [ ] **Step 2: FortuneDataTab（KPI + Top3 排行/风险 + 月度柱状）**

改写。布局照 demo `renderData`（line 221-230）。数据来自 `fortuneStore.summary`：

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { fortuneStore } from '$lib/stores/fortune-store.svelte';
  import { levelColor, fmtReturn } from './fortune-helpers';
  const s = $derived(fortuneStore.summary);
  onMount(() => { if (!s) fortuneStore.loadAll(); });
  const kpis = $derived(s ? [
    { label: t('fortune_kpi_total'), val: `${s.totalDays}` },
    { label: t('fortune_kpi_win'), val: `${s.winDays}` },
    { label: t('fortune_kpi_winrate'), val: `${(s.winRate * 100).toFixed(0)}%` },
    { label: t('fortune_kpi_cumulative'), val: fmtReturn(s.cumulativeReturn) },
    { label: t('fortune_kpi_avg'), val: fmtReturn(s.avgDailyReturn) },
  ] : []);
</script>

{#if !s || s.totalDays === 0}
  <div class="text-[var(--text-tertiary)]">{t('fortune_insufficient')}</div>
{:else}
  <div class="mb-[var(--space-4)] grid grid-cols-2 gap-[var(--space-3)] sm:grid-cols-5">
    {#each kpis as k}
      <div class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-card)] p-[var(--space-3)]">
        <div class="text-[11px] text-[var(--text-tertiary)]">{k.label}</div>
        <div class="font-mono text-[20px] font-bold text-[var(--text-primary)]">{k.val}</div>
      </div>
    {/each}
  </div>
  <!-- Top3 排行：s.topStems + s.topBranches；Top3 风险：s.riskStems + s.riskBranches -->
  <!-- 月度柱状：s.monthly，每根柱 height ∝ |avgReturn|，红涨绿跌上色 -->
  <div class="flex items-end gap-[var(--space-2)]">
    {#each s.monthly as m}
      <div class="flex flex-col items-center">
        <div style="height:{Math.min(Math.abs(m.avgReturn) * 20, 80)}px;width:20px;
          background:{m.avgReturn >= 0 ? 'var(--up)' : 'var(--down)'}"></div>
        <div class="mt-1 text-[10px] text-[var(--text-tertiary)]">{m.month.slice(5)}</div>
      </div>
    {/each}
  </div>
{/if}
```

（Top3 排行/风险的表格照 demo 还原：两列 排行/风险，各列 3 行 干支+分数。）

- [ ] **Step 3: 编译 + 目视**

Run: `npm run check`
Expected: 无类型错误。起 app 切到干支总览/数据总览，确认表格、预告卡、KPI、月度柱状显示正确（红涨绿跌）。

- [ ] **Step 4: Commit**

```bash
git add src/lib/components/invest/FortuneStemBranchTab.svelte src/lib/components/invest/FortuneDataTab.svelte
git commit -m "feat(fortune): stem-branch overview + data summary tabs"
```

---

### Task 9: 录入弹窗 — 单日 + 按月批量

**Files:**
- Create: `src/lib/components/invest/FortuneRecordDialog.svelte`

**Interfaces:**
- Props：`onclose: () => void`
- Consumes：`fortuneStore.upsert`、`fortuneStore.batchUpsert`、`fortuneStore.analysis`（拿已录日期预填）。

**关键逻辑（按月工作日生成 + 翻页约束）：**
- 每页 = 一个自然月的工作日（周一~周五，排除周末）。
- 默认页 = 当前月；**不能翻未来月**（下一月按钮到当前月即禁用）。
- 当前月只列到**今天（含）**为止的工作日；历史月列该月全部工作日。
- 已录入日期预填旧值（可覆盖 upsert）+ 红点标记；空行留白。

- [ ] **Step 1: 写弹窗组件（含工作日生成纯函数）**

创建 `FortuneRecordDialog.svelte`：

```svelte
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { fortuneStore } from '$lib/stores/fortune-store.svelte';
  import { getInvestDate } from '$lib/i18n/format';

  let { onclose }: { onclose: () => void } = $props();
  let mode = $state<'single' | 'batch'>('single');

  // ── 单日 ──
  let singleDate = $state(getInvestDate());   // 默认今天
  let singleVal = $state('');
  let singleNote = $state('');

  // ── 批量：按月 ──
  const today = getInvestDate();              // "YYYY-MM-DD"
  const [ty, tm] = today.split('-').map(Number);
  let year = $state(ty);
  let month = $state(tm);                      // 1-12
  const isCurrentMonth = $derived(year === ty && month === tm);

  // 该月工作日列表（当前月截到今天）
  function workdays(y: number, m: number): string[] {
    const out: string[] = [];
    const last = new Date(y, m, 0).getDate();   // 该月天数
    for (let d = 1; d <= last; d++) {
      const dow = new Date(y, m - 1, d).getDay();  // 0=日,6=六
      if (dow === 0 || dow === 6) continue;
      const ds = `${y}-${String(m).padStart(2,'0')}-${String(d).padStart(2,'0')}`;
      if (y === ty && m === tm && ds > today) break;  // 当前月不列未来
      out.push(ds);
    }
    return out;
  }
  const dates = $derived(workdays(year, month));
</script>
```

续 script（预填旧值 + 翻月 + 提交）：

```svelte
<script lang="ts">
  // 已录入映射：date → return_pct（从 analysis.calendar 拿）
  const recorded = $derived.by(() => {
    const m = new Map<string, number>();
    for (const d of fortuneStore.analysis?.calendar ?? []) {
      if (d.actualReturn != null) m.set(d.date, d.actualReturn);
    }
    return m;
  });

  // 批量输入缓存：date → string
  let batchVals = $state<Record<string, string>>({});
  // 进入某月时用已录旧值预填空白项
  $effect(() => {
    for (const ds of dates) {
      if (batchVals[ds] === undefined) {
        const old = recorded.get(ds);
        batchVals[ds] = old != null ? String(old) : '';
      }
    }
  });

  function prevMonth() {
    if (month === 1) { year -= 1; month = 12; } else { month -= 1; }
  }
  function nextMonth() {
    if (isCurrentMonth) return;   // 不可翻未来
    if (month === 12) { year += 1; month = 1; } else { month += 1; }
  }

  async function submitSingle() {
    const v = parseFloat(singleVal);
    if (Number.isNaN(v)) return;   // 前端拦截非法
    await fortuneStore.upsert(singleDate, v, singleNote);
    onclose();
  }
  async function submitBatch() {
    const entries = dates
      .filter((ds) => batchVals[ds]?.trim() !== '')
      .map((ds) => ({ date: ds, returnPct: parseFloat(batchVals[ds]), note: '' }))
      .filter((e) => !Number.isNaN(e.returnPct));   // 跳过非法行
    if (entries.length) await fortuneStore.batchUpsert(entries);
    onclose();
  }
</script>
```

模板（模态遮罩 + 单日/批量切换；批量左固定日期行、右填收益，已录行红点）：

```svelte
<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={onclose}>
  <div class="max-h-[80vh] w-[520px] overflow-auto rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-5)]"
    onclick={(e) => e.stopPropagation()}>
    <div class="mb-[var(--space-4)] flex gap-[var(--space-2)]">
      <button class:text-[var(--accent)]={mode === 'single'} onclick={() => (mode = 'single')}>单日</button>
      <button class:text-[var(--accent)]={mode === 'batch'} onclick={() => (mode = 'batch')}>批量</button>
    </div>

    {#if mode === 'single'}
      <input type="date" bind:value={singleDate} class="..." />
      <input type="number" step="0.01" bind:value={singleVal} placeholder="收益率 %" class="..." />
      <input bind:value={singleNote} placeholder="备注（可选）" class="..." />
      <button onclick={submitSingle}>保存</button>
    {:else}
      <div class="mb-[var(--space-3)] flex items-center justify-between">
        <button onclick={prevMonth}>‹ 上一月</button>
        <span>{year}年{month}月</span>
        <button onclick={nextMonth} disabled={isCurrentMonth}>下一月 ›</button>
      </div>
      {#each dates as ds}
        <div class="flex items-center gap-[var(--space-3)]">
          <span class="font-mono text-[12px] text-[var(--text-secondary)]">
            {ds}{#if recorded.has(ds)}<span style="color:var(--up)"> ●</span>{/if}
          </span>
          <input type="number" step="0.01" bind:value={batchVals[ds]} placeholder="—" class="..." />
        </div>
      {/each}
      <button onclick={submitBatch}>批量保存</button>
    {/if}
  </div>
</div>
```

（`class="..."` 处套用现有 invest 输入框样式：`rounded-[var(--radius-sm)] border border-border bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-[13px]`。参照 `TradeDialog.svelte` 的输入框写法。）

- [ ] **Step 2: 编译 + 目视验证**

Run: `npm run check`
Expected: 无类型错误。起 app：
- 单日模式默认今天，填收益保存后今日卡切换为「盘后」。
- 批量模式默认当前月、只到今天；「下一月」在当前月禁用；翻到上月列全月工作日；已录行有红点且预填旧值。

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/invest/FortuneRecordDialog.svelte
git commit -m "feat(fortune): record dialog — single-day + by-month batch entry"
```

---

### Task 10: 端到端验证 + 收尾

**Files:** 无新增；跑全套检查。

- [ ] **Step 1: 全套后端检查**

Run: `node scripts/rust-test.mjs --lib fortune`
Expected: storage + aggregate 全部测试 passed。

Run: `npm run rust:clippy`
Expected: 零 warning（`-D warnings`）。

- [ ] **Step 2: 全套前端检查**

Run: `npm run check && npm test && npm run i18n:check`
Expected: svelte-check 无错、vitest 全绿、i18n 键集一致。

- [ ] **Step 3: 端到端手动验证（起真实 app）**

Run: `npm run tauri dev`

验证清单（对齐 spec §3 的 point-in-time 语义）：
- 录入一笔今日收益 → 今日卡副标签从「预测」变「盘后」，分数相应变化。
- 批量录入一个月历史数据 → 日历历史格显示预测态（虚线边）；干支总览表格、数据总览 KPI 填充。
- point-in-time 抽查：某干支首次出现日的预测分 ≈ 中性锚（56.9 附近），说明用的是"该日之前"数据。
- 生成 AI 解读 → 按钮禁用显示「生成中…」→ 完成后展示文字；无 claude 时展示错误态。
- 空状态：清空数据后分析 tab 显示引导文案。

- [ ] **Step 4: 更新 spec 状态 + changelog**

将 spec 头部状态改为「已实现」；按项目惯例补 changelog（参照 git log 里 `docs: vX.Y.Z changelog` 格式）。

- [ ] **Step 5: 最终 commit**

```bash
git add docs/superpowers/specs/2026-07-10-fortune-journal-design.md
git commit -m "docs(fortune): mark spec implemented + changelog"
```

---

## 自检对照（Self-Review）

**Spec 覆盖**：
- §2 算法 → 已落地（`stats.rs`），本计划只调用不改（Task 1-2）。✅
- §3 point-in-time 单趟聚合 → Task 2 `aggregate_pit`。✅
- §4 数据模型两表 → Task 1。✅
- §5 后端模块（aggregate/storage/commands）→ Task 1-4。✅
- §5 AI 解读独立信号量 → Task 4。✅
- §6 前端 3 子 tab + 日历 + 弹窗 + store → Task 5-9。✅
- §6 今日/明日卡预测↔盘后切换 → Task 7（`cardLabel` + store invalidate）。✅
- §6 三态日历 → Task 2 `build_calendar`（产出完整月历三态数据）+ Task 7 `cellStyle`。✅（review 修正：原 `calendar` 只给已录天，预测态无数据源、月历排不出，已改为逐月枚举）
- §6 6 路预告 → **改 4 路**（组合无分，已在 Task 2 注明偏差）。⚠️已记录
- §5 AI 解读信号量 → **勘误**：executor 内部信号量与委员会共享（`Arc<Semaphore>` + `Clone`），fortune 解读非独立池；`READING_SEM` 仅防连点。已在 Task 4 更正说明。⚠️已记录
- §6 批量按月翻页 → Task 9。✅
- §6 KPI/Top3/月度 → Task 2 + Task 8。✅
- §7 错误处理（前端拦截/逐行跳过/解读错误态/空数据中性）→ Task 2/7/9。✅
- §8 测试策略 → Task 1（CRUD）、Task 2（聚合）、Task 5（store）。✅
- i18n 只中文 + en fallback → Task 5。✅

**类型一致性**：`DayScore`/`LayerRow`/`Analysis`/`Overview`/`DataSummary`/`ForecastItem`/`MonthStat`/`BatchEntry` 在后端（Task 2/3）与前端 TS（Task 5）字段名 camelCase 对齐；`FortuneLevel` 用 snake_case（`stats.rs` 既有 serde 约定），前端 helper 的 `LABELS`/`COLORS` 键与之匹配。✅

**已核实的假设（review 阶段用真实代码确认）**：
- `stats.rs` 全部签名对上：`LayerStat{avg_return_pct,win_rate,sample}`、`layer_score(&LayerStat)`、`composite_from_layers(f64,f64)`、`fortune_level(f64)`、`FortuneLevel`(serde `snake_case`)、中性锚 `COMP_BASE=56.9`（测试断言正确）。✅
- `calendar.rs`：`STEMS[10]`/`BRANCHES[12]`(`&str`)、`ganzhi(y,m,d)->(&str,&str)`、`ganzhi_index(y,m,d)->usize`。✅
- `scheduler.rs` 在 `crate::storage::invest::scheduler`：`is_trading_day(&str)->Result<bool,String>`、`next_trading_day(&str)->Result<String,String>`、`beijing_today()->String`。✅
- `with_conn(FnOnce(&Connection))` / `with_conn_mut(FnOnce(&mut Connection))`。✅
- `CliCommitteeExecutor::global()->Option<Self>`(owned clone)、`run_role(&self, sys:&str, user:&str, timeout:u64, settings:Option<&Path>, cancel:Option<&CancellationToken>)`。✅
- Tauri v2 参数自动 camelCase↔snake_case（`update_cron_schedule(cron_expr)` 前端传 `{cronExpr}` 佐证）→ `returnPct` 写法正确。✅
- 项目已依赖 `chrono`（`features=["serde"]`，default feature 含 `NaiveDate`/`Datelike`）。✅
- 现有 store 测试在 `src/lib/stores/*.test.ts`（非 `__tests__/`），mock 用 `vi.mock('$lib/transport', () => ({ getTransport: () => ({ invoke: invokeMock }) }))`。✅

**实现时仍需现场确认**：
- `getInvestDate()` 返回值格式（invest-store 已用，假定 "YYYY-MM-DD"；若含时区差异需在 Task 9 微调）。
- `is_trading_day`/`next_trading_day` 对**未来日期**依赖交易日历是否已灌数据；未灌时 `is_trading_day` 报错、计划用 `.unwrap_or(true)` 降级为"按交易日处理"，`next_trading_day` 报错则明日卡为 None（可接受降级，起 app 时留意）。


















