# 委员会宏观改版 · 后端实现计划(计划 1/2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让全局宏观判断成为与 macro_cache 平级、绑数据版本、按交易时段定时刷新的独立持久化对象;委员会各 symbol 只读它做轻量行业敏感度,不再重算(修复 B1)。

**Architecture:** 数据层 miniQMT 一次性取全 A 市场广度(涨幅>3%/涨-平-跌)写入 macro_cache 并盖批次戳;新增 `macro_verdict` 存储对象 + cron job,运行时读齐广度+大盘数据 → MACRO_GLOBAL_PROMPT 产判断,赚钱效应档位由固定阈值规则算(LLM 只写理由),绑 `_verdict_input_batch` 统一戳;委员会编排去除 per-symbol 宏观重算,改读全局判断 + per-symbol SENSITIVITY_PROMPT(行业+regime 单次运行内存缓存)。

**Tech Stack:** Rust(rusqlite/tokio/chrono)、Python(xtquant/JSON-RPC over server.py)、Claude CLI 执行器。

## Global Constraints

- 响应/注释/文档一律简体中文;技术标识符保留原文。
- Conventional Commits(`feat:`/`fix:`/`chore:`)。
- Rust 单测本机从 Git Bash 跑会 `STATUS_ENTRYPOINT_NOT_FOUND`;验证优先 `cargo check --manifest-path src-tauri/Cargo.toml`,需跑测试用:`cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- <filter> --nocapture"`。
- clippy 零警告:`cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`。
- miniQMT 是可选源(`UserSettings.invest_miniqmt_enabled`,默认关);广度指标**同源优先**,关闭时赚钱效应显示"数据不足",绝不混用 miniQMT+tushare。
- 赚钱效应 4 档为**固定阈值规则**(占比口径,来自 2 年校准),存为常量,非 LLM 判定:冰点 ≤6% / 平淡 6–9% / 活跃 9–21% / 火热 >21%(占比 = 涨幅>3%家数 / 当日有效家数 × 100)。
- 枚举闭集:signal ∈ {risk_on, risk_off, neutral};money_effect ∈ {hot, active, calm, cold};regime 沿用 {uptrend, downtrend, range_bound, crash, unknown}。
- 版本戳规则:全局判断有效 ⟺ 其 `based_on_data_version` == 当前 `_verdict_input_batch`(绑广度批次戳 + macro_refresh 批次戳的组合);任一上游刷新则失效。
- 手动刷新与自动统一时段逻辑:非交易时段不跑真广度,复用最近收盘定版。

---

## 文件结构

**新建:**
- `src-tauri/src/storage/invest/macro_verdict.rs` — 全局宏观判断对象表 + 批次戳读写 + is_valid。
- `src-tauri/src/invest/macro_verdict.rs` — `macro_verdict` job 主逻辑(取数→prompt→写判断)+ 赚钱效应分档常量 + 时段门禁。
- `src-tauri/src/invest/committee/sensitivity.rs` — SENSITIVITY_PROMPT + 行业敏感度(行业+regime 内存缓存)。

**修改:**
- `src-tauri/python-runtime/scripts/providers/xtdata.py` — 新增 `market_breadth()`。
- `src-tauri/src/invest/international.rs` — 新增 `fetch_xtdata_breadth()`。
- `src-tauri/src/invest/macro_refresh.rs` — 广度改走 miniQMT 同源 + 写 `_breadth_batch` 戳 + 新指标。
- `src-tauri/src/storage/invest/macro_cache.rs` — `ALL_INDICATORS` 增 `up_over_3pct_count`/`flat_count`;`MacroSnapshot` 加对应字段。
- `src-tauri/src/storage/invest/mod.rs` — 声明 `pub mod macro_verdict;` + `init_db` 调 `create_table`。
- `src-tauri/src/invest/scheduler/mod.rs` — `default_jobs()` 增 `macro_verdict`。
- `src-tauri/src/invest/scheduler/runner.rs` — `dispatch_job` 增 `macro_verdict` 分支。
- `src-tauri/src/invest/committee/roles.rs` — 新增 `MACRO_GLOBAL_PROMPT` 常量。
- `src-tauri/src/invest/committee/parser.rs` — `emotion_temperature` → `money_effect` + `money_effect_reason`。
- `src-tauri/src/invest/committee/orchestrator.rs` — 去除 per-symbol 宏观重算,接全局判断 + 敏感度。
- `src-tauri/src/commands/invest.rs` — 新增 `get_macro_verdict` / `refresh_macro_verdict` 命令。
- `src-tauri/src/lib.rs` — 注册两个新命令。
- `src-tauri/src/invest/mod.rs` — 声明 `pub mod macro_verdict;`。

---

### Task 1: Python `market_breadth()` provider 方法

**Files:**
- Modify: `src-tauri/python-runtime/scripts/providers/xtdata.py`(在 `realtime_quote` 后、`_to_yyyymmdd` 前插入)

**Interfaces:**
- Produces: JSON-RPC 方法 `xtdata.market_breadth`,无参数,返回 dict:
  `{"available": bool, "reason": str, "up": int, "flat": int, "down": int, "limit_up": int, "limit_down": int, "up_over_3pct": int, "valid": int}`。
- 说明:`server.py` 通过 `getattr(provider, func_name)` 自动暴露模块顶层函数,**无需注册表**;新增顶层 `def market_breadth()` 即可被 `xtdata.market_breadth` 调用。

- [ ] **Step 1: 写函数(照 xtdata.py 现有 lazy-import + 不抛异常风格)**

在 `realtime_quote` 函数之后插入:

```python
def market_breadth() -> dict:
    """一次性全 A 市场广度快照。QMT 离线返回 available=false，不抛异常。"""
    try:
        xt = _get_xtdata()
        codes = xt.get_stock_list_in_sector("沪深A股") or []
        codes = [c for c in codes
                 if c.partition(".")[0][:2] in ("60", "00", "30", "68")]
        ticks = xt.get_full_tick(codes) or {}
        up = flat = down = up3 = lu = ld = valid = 0
        for _code, t in ticks.items():
            last = float(t.get("lastPrice", 0.0) or 0.0)
            prev = float(t.get("lastClose", 0.0) or 0.0)
            if prev <= 0.0 or last <= 0.0:
                continue
            valid += 1
            chg = (last - prev) / prev * 100.0
            if chg > 0.01:
                up += 1
            elif chg < -0.01:
                down += 1
            else:
                flat += 1
            if chg > 3.0:
                up3 += 1
            if chg >= 9.9:
                lu += 1
            elif chg <= -9.9:
                ld += 1
        return {"available": True, "reason": "", "up": up, "flat": flat,
                "down": down, "limit_up": lu, "limit_down": ld,
                "up_over_3pct": up3, "valid": valid}
    except Exception as e:  # noqa: BLE001
        return {"available": False, "reason": str(e), "up": 0, "flat": 0,
                "down": 0, "limit_up": 0, "limit_down": 0,
                "up_over_3pct": 0, "valid": 0}
```

- [ ] **Step 2: 手动烟测(QMT 在线时)**

Run: `cd src-tauri/python-runtime && echo '{"jsonrpc":"2.0","method":"xtdata.market_breadth","params":{},"id":1}' | python/python.exe scripts/server.py`
Expected: 一行 JSON,`result.available=true` 且 `valid` 约 5000+;QMT 离线时 `available=false` 且 `reason` 非空(不崩)。

**关键前置校验(地基):** 现有 `realtime_quote` 只用了 `lastPrice`/`volume`/`amount`,**从未用过 `lastClose`**。本函数涨跌幅依赖 `t["lastClose"]`。若烟测返回 `valid≈5000` 但 `up/down/up_over_3pct` 全为 0 或异常小,说明 `get_full_tick` 的 tick **无 `lastClose` 字段**(memory 记录称有,但需实测坐实)。届时降级方案:改用 `t.get("open")` 或读日线 `lastClose`,或从 tick 的 `askPrice`/`preClose` 等实际字段名取(先 `print(list(next(iter(ticks.values())).keys()))` 打印真实字段名再定)。**此步必须在 Task 2 之前确认通过,否则整条广度链失效。**

- [ ] **Step 3: Commit**

```bash
git add src-tauri/python-runtime/scripts/providers/xtdata.py
git commit -m "feat(invest): miniQMT market_breadth provider 方法(全A广度一次取全)"
```

---

### Task 2: Rust `fetch_xtdata_breadth()` 客户端方法 + struct

**Files:**
- Modify: `src-tauri/src/invest/international.rs`(struct 加在 `XtdataKlineResp` 后 line ~98;方法加在 impl 块内 `fetch_xtdata_kline` 之后、闭合 `}` line 285 前)

**Interfaces:**
- Consumes: `xtdata.market_breadth`(Task 1);现有 `self.rpc_call::<T>("method", json) -> Result<T,String>`。
- Produces: `pub struct MarketBreadth { available, reason, up, flat, down, limit_up, limit_down, up_over_3pct, valid }`(全 `u32` 除 available:bool/reason:String);`InternationalClient::fetch_xtdata_breadth(&self) -> Result<MarketBreadth, String>`;`InternationalClient::from_settings()`(已存在)。

- [ ] **Step 1: 加 struct(照 XtdataHealth 的 #[serde(default)] 风格)**

在 `XtdataKlineResp`(line 98)之后插入:

```rust
/// miniQMT 全 A 市场广度快照。
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MarketBreadth {
    pub available: bool,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub up: u32,
    #[serde(default)]
    pub flat: u32,
    #[serde(default)]
    pub down: u32,
    #[serde(default)]
    pub limit_up: u32,
    #[serde(default)]
    pub limit_down: u32,
    #[serde(default)]
    pub up_over_3pct: u32,
    #[serde(default)]
    pub valid: u32,
}
```

- [ ] **Step 2: 加方法(照 fetch_xtdata_health 风格)**

在 impl 块内 `fetch_xtdata_kline` 之后插入:

```rust
    /// 获取 miniQMT 全 A 市场广度快照。QMT 离线时返回 available=false。
    pub async fn fetch_xtdata_breadth(&self) -> Result<MarketBreadth, String> {
        self.rpc_call("xtdata.market_breadth", serde_json::json!({})).await
    }
```

- [ ] **Step 3: cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过(可能有 `MarketBreadth` 未使用 warning,下个 Task 消费,暂忽略)。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/invest/international.rs
git commit -m "feat(invest): InternationalClient.fetch_xtdata_breadth 客户端方法"
```

---

### Task 3: macro_cache 新增广度指标 + 批次版本戳纯函数

**Files:**
- Modify: `src-tauri/src/storage/invest/macro_cache.rs`

**Interfaces:**
- Consumes: 现有 `save_macro_cache`/`load_macro_cache`/`load_all_macro_cache`。
- Produces:
  - `ALL_INDICATORS` 增两项 `up_over_3pct_count`、`flat_count`。
  - `MacroSnapshot` 增 `up_over_3pct_count: Option<f64>`、`flat_count: Option<f64>`(camelCase 自动 → upOver3pctCount/flatCount)。
  - `pub fn compose_data_version(breadth: Option<&str>, macro_batch: Option<&str>) -> String`(纯函数,拼两个批次戳)。
  - `pub fn current_data_version() -> Result<String, String>`(读 `_breadth_batch` + `_macro_batch` 两个标记行的 fetched_at,调 compose)。
  - 标记行写入沿用 `save_macro_cache("_breadth_batch", None, None, source)`(fetched_at 自动为当刻)。

- [ ] **Step 1: 写 compose_data_version 的失败测试**

在文件末尾 `mod tests` 内加:

```rust
    #[test]
    fn test_compose_data_version() {
        assert_eq!(
            compose_data_version(Some("2026-07-08 01:35:00"), Some("2026-07-08 01:30:00")),
            "b:2026-07-08 01:35:00|m:2026-07-08 01:30:00"
        );
        // 任一缺失 → 该段为 none，整体仍确定性可比
        assert_eq!(compose_data_version(None, Some("2026-07-08 01:30:00")), "b:none|m:2026-07-08 01:30:00");
        assert_eq!(compose_data_version(None, None), "b:none|m:none");
    }
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- macro_cache::tests::test_compose_data_version --nocapture"`
Expected: FAIL(`compose_data_version` 未定义,编译错误)。

- [ ] **Step 3: 加指标 + 字段 + 函数**

3a. `ALL_INDICATORS`(line 14-32)末尾 `"decline_count",` 后加:
```rust
    "up_over_3pct_count",
    "flat_count",
```

3b. `MacroSnapshot`(line 139-160)在 `limit_down_count` 后加:
```rust
    /// 涨幅 > 3% 家数（赚钱效应基础）
    pub up_over_3pct_count: Option<f64>,
    /// 平盘家数
    pub flat_count: Option<f64>,
```

3c. `build_macro_snapshot`(line 166-177)在 `limit_down_count: get(...)` 后加:
```rust
        up_over_3pct_count: get("up_over_3pct_count"),
        flat_count: get("flat_count"),
```

3c-bis. **补 prompt 中文标签**(否则宏观 prompt 里新指标显示英文 key):`src-tauri/src/invest/committee/tools.rs` 的 `format_macro_entries` label match(line 41-42,`decline_count => "下跌家数",` 后)加:
```rust
                "up_over_3pct_count" => "涨幅>3%家数",
                "flat_count" => "平盘家数",
```
说明:`format_macro_entries` 遍历 `ALL_INDICATORS` 白名单,新加两项会进循环;哨兵行 `_breadth_batch`/`_macro_batch` **不在**白名单,不会污染 prompt(已核实)。

3d. 在 `is_stale` 函数前加两个新函数:
```rust
/// 拼接广度批次戳 + macro_refresh 批次戳为确定性版本串。
pub fn compose_data_version(breadth: Option<&str>, macro_batch: Option<&str>) -> String {
    format!("b:{}|m:{}", breadth.unwrap_or("none"), macro_batch.unwrap_or("none"))
}

/// 读两个批次标记行的 fetched_at，组合为当前数据版本串。
pub fn current_data_version() -> Result<String, String> {
    let b = load_macro_cache("_breadth_batch")?.map(|e| e.fetched_at);
    let m = load_macro_cache("_macro_batch")?.map(|e| e.fetched_at);
    Ok(compose_data_version(b.as_deref(), m.as_deref()))
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- macro_cache::tests::test_compose_data_version --nocapture"`
Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/storage/invest/macro_cache.rs
git commit -m "feat(invest): macro_cache 增广度指标 + 批次版本戳纯函数"
```

---

### Task 4: macro_refresh 广度同源(miniQMT)+ 降级 + 批次戳写入

**Files:**
- Modify: `src-tauri/src/invest/macro_refresh.rs`

**Interfaces:**
- Consumes: `MarketBreadth` + `fetch_xtdata_breadth`(Task 2);`compose/current_data_version` 无关(此处只写标记行);现有 `fetch_akshare_market_stats`/`fetch_akshare_advance_decline`(降级路径)。
- Produces: 新 `async fn fetch_breadth(miniqmt_on: bool) -> MacroResult`;替换 `tasks` 里的 `fetch_market_stats` + `fetch_advance_decline` 两项为单个 `fetch_breadth(miniqmt_on)`;`refresh_macro_cache` 末尾写 `_breadth_batch` + `_macro_batch` 标记行。

- [ ] **Step 1: 新增 fetch_breadth(替换旧的 market_stats + advance_decline 语义,同源优先)**

在 `fetch_advance_decline`(line 447)之后插入:

```rust
/// 市场广度同源采集：miniQMT 开启且在线 → 一次取全(涨/平/跌/涨跌停/涨幅>3%)；
/// 否则降级 akshare(仅涨跌家数 + 涨跌停，flat/up_over_3pct 缺失 = "数据不足")。
async fn fetch_breadth(miniqmt_on: bool) -> MacroResult {
    let client = crate::invest::international::InternationalClient::from_settings();
    if miniqmt_on {
        match client.fetch_xtdata_breadth().await {
            Ok(b) if b.available && b.valid > 0 => {
                return Ok(vec![
                    ("advance_count".into(), Some(b.up as f64), None, "miniqmt"),
                    ("decline_count".into(), Some(b.down as f64), None, "miniqmt"),
                    ("flat_count".into(), Some(b.flat as f64), None, "miniqmt"),
                    ("limit_up_count".into(), Some(b.limit_up as f64), None, "miniqmt"),
                    ("limit_down_count".into(), Some(b.limit_down as f64), None, "miniqmt"),
                    ("up_over_3pct_count".into(), Some(b.up_over_3pct as f64), None, "miniqmt"),
                ]);
            }
            Ok(b) => log::warn!("breadth: miniqmt unavailable ({}), 降级 akshare", b.reason),
            Err(e) => log::warn!("breadth: miniqmt error ({e}), 降级 akshare"),
        }
    }
    // 降级：akshare 仅有涨跌家数 + 涨跌停；flat/up_over_3pct 不写(保留旧值/缺失)。
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let stats = client.fetch_akshare_market_stats(&today).await.map_err(|e| format!("market_stats: {e}"))?;
    let ad = client.fetch_akshare_advance_decline(&today).await.map_err(|e| format!("advance_decline: {e}"))?;
    Ok(vec![
        ("advance_count".into(), Some(ad.advance_count as f64), None, "akshare"),
        ("decline_count".into(), Some(ad.decline_count as f64), None, "akshare"),
        ("limit_up_count".into(), Some(stats.limit_up_count as f64), None, "akshare"),
        ("limit_down_count".into(), Some(stats.limit_down_count as f64), None, "akshare"),
    ])
}
```

- [ ] **Step 2: tasks 列表替换 + 删旧函数**

2a. `refresh_macro_cache` 的 `tasks` vec(line 33-48):把 `Box::pin(fetch_market_stats())` 和 `Box::pin(fetch_advance_decline())` 两行**替换为一行**:
```rust
        Box::pin(fetch_breadth(miniqmt_on)),
```

2b. 删除现已无引用的 `fetch_market_stats`(line 419-430)与 `fetch_advance_decline`(line 436-447)两个函数(避免 dead_code 警告)。

- [ ] **Step 3: refresh 末尾写批次标记行**

在 `refresh_macro_cache` 的 `Ok(format!(...))`(line 76)之前插入:
```rust
    // 批次戳：供全局宏观判断的 based_on_data_version 比对(§8.2-G)。
    // 广度与大盘数据同批刷新，两行 fetched_at 均为当刻。
    let breadth_source = if miniqmt_on { "miniqmt" } else { "akshare" };
    let _ = macro_cache::save_macro_cache("_breadth_batch", None, None, breadth_source);
    let _ = macro_cache::save_macro_cache("_macro_batch", None, None, "macro_refresh");
```

- [ ] **Step 4: 修 test_all_indicators_count(17 → 19)**

`mod tests`(line 480-484):
```rust
    #[test]
    fn test_all_indicators_count() {
        // 17 + up_over_3pct_count + flat_count = 19
        assert_eq!(macro_cache::ALL_INDICATORS.len(), 19);
    }
```

- [ ] **Step 5: cargo check + 测试**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过,无 dead_code(旧函数已删)。
Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- macro_refresh::tests --nocapture"`
Expected: PASS。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/invest/macro_refresh.rs
git commit -m "feat(invest): 市场广度 miniQMT 同源采集 + 批次戳"
```

---

### Task 5: 全局宏观判断对象存储表

**Files:**
- Create: `src-tauri/src/storage/invest/macro_verdict.rs`
- Modify: `src-tauri/src/storage/invest/mod.rs`(声明 mod + init_db 建表)

**Interfaces:**
- Consumes: 现有 `with_conn`/`with_conn_mut`;`macro_cache::current_data_version()`(Task 3)。
- Produces:
  - `pub struct MacroVerdict { signal, strength, market_phase, money_effect, money_effect_reason, signal_reason, market_phase_reason, based_on_data_version, updated_at }`(camelCase 序列化)。
  - `pub fn create_table(conn: &Connection) -> Result<(), String>`。
  - `pub fn save_verdict(v: &MacroVerdict) -> Result<(), String>`(id=1 单行 upsert)。
  - `pub fn load_verdict() -> Result<Option<MacroVerdict>, String>`。
  - `pub fn is_current(v: &MacroVerdict, current_version: &str) -> bool`(纯函数:`v.based_on_data_version == current_version`)。

- [ ] **Step 1: 建文件(表 + struct + create_table)**

创建 `src-tauri/src/storage/invest/macro_verdict.rs`:

```rust
use crate::storage::invest::{with_conn, with_conn_mut};
use rusqlite::Connection;

const CREATE_TABLE_SQL: &str = "
CREATE TABLE IF NOT EXISTS macro_verdict (
    id                    INTEGER PRIMARY KEY CHECK (id = 1),
    signal                TEXT,
    strength              REAL,
    market_phase          TEXT,
    money_effect          TEXT,
    money_effect_reason   TEXT,
    signal_reason         TEXT,
    market_phase_reason   TEXT,
    based_on_data_version TEXT NOT NULL,
    updated_at            TEXT NOT NULL DEFAULT (datetime('now'))
);";

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroVerdict {
    pub signal: Option<String>,
    pub strength: Option<f64>,
    pub market_phase: Option<String>,
    pub money_effect: Option<String>,
    pub money_effect_reason: Option<String>,
    pub signal_reason: Option<String>,
    pub market_phase_reason: Option<String>,
    pub based_on_data_version: String,
    pub updated_at: String,
}

pub fn create_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(CREATE_TABLE_SQL)
        .map_err(|e| format!("create macro_verdict table: {e}"))
}

/// 判断是否仍对应当前数据版本(纯函数,可测)。
pub fn is_current(v: &MacroVerdict, current_version: &str) -> bool {
    v.based_on_data_version == current_version
}
```

- [ ] **Step 2: 加 save/load(接文件尾)**

```rust
pub fn save_verdict(v: &MacroVerdict) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO macro_verdict (id, signal, strength, market_phase, money_effect,
                 money_effect_reason, signal_reason, market_phase_reason, based_on_data_version, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                 signal=excluded.signal, strength=excluded.strength, market_phase=excluded.market_phase,
                 money_effect=excluded.money_effect, money_effect_reason=excluded.money_effect_reason,
                 signal_reason=excluded.signal_reason, market_phase_reason=excluded.market_phase_reason,
                 based_on_data_version=excluded.based_on_data_version, updated_at=excluded.updated_at",
            rusqlite::params![v.signal, v.strength, v.market_phase, v.money_effect,
                v.money_effect_reason, v.signal_reason, v.market_phase_reason, v.based_on_data_version],
        ).map_err(|e| format!("save macro_verdict: {e}"))?;
        Ok(())
    })
}

pub fn load_verdict() -> Result<Option<MacroVerdict>, String> {
    with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT signal, strength, market_phase, money_effect, money_effect_reason,
                    signal_reason, market_phase_reason, based_on_data_version, updated_at
             FROM macro_verdict WHERE id = 1",
        ).map_err(|e| format!("prepare load_verdict: {e}"))?;
        let mut rows = stmt.query_map([], |r| Ok(MacroVerdict {
            signal: r.get(0)?, strength: r.get(1)?, market_phase: r.get(2)?,
            money_effect: r.get(3)?, money_effect_reason: r.get(4)?, signal_reason: r.get(5)?,
            market_phase_reason: r.get(6)?, based_on_data_version: r.get(7)?, updated_at: r.get(8)?,
        })).map_err(|e| format!("query load_verdict: {e}"))?;
        rows.next().transpose().map_err(|e| format!("read load_verdict: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_is_current() {
        let v = MacroVerdict { based_on_data_version: "b:X|m:Y".into(), ..Default::default() };
        assert!(is_current(&v, "b:X|m:Y"));
        assert!(!is_current(&v, "b:X2|m:Y"));
    }
}
```

- [ ] **Step 3: 注册 mod + 建表**

3a. `src-tauri/src/storage/invest/mod.rs` 顶部模块声明区(line 1-13,按字母序在 `pub mod macro_cache;` 后)加:
```rust
pub mod macro_verdict;
```
3b. `init_db` 内 `macro_cache::create_table(&conn)?;`(line 346)后加:
```rust
    macro_verdict::create_table(&conn)?;
```

- [ ] **Step 4: cargo check + 测试**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。
Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- macro_verdict::tests::test_is_current --nocapture"`
Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/storage/invest/macro_verdict.rs src-tauri/src/storage/invest/mod.rs
git commit -m "feat(invest): 全局宏观判断对象存储表(单行 + 版本戳 is_current)"
```

---

### Task 6: 赚钱效应分档 + 交易时段门禁(纯函数,新建 invest/macro_verdict.rs 骨架)

**Files:**
- Create: `src-tauri/src/invest/macro_verdict.rs`
- Modify: `src-tauri/src/invest/mod.rs`(声明 `pub mod macro_verdict;`)

**Interfaces:**
- Produces:
  - `pub fn money_effect_tier(ratio_pct: f64) -> &'static str`:占比→档位(冰点/平淡/活跃/火热 的英文枚举 cold/calm/active/hot),阈值 6/9/21。
  - `pub fn is_trading_session(now_cst: chrono::NaiveTime, weekday: chrono::Weekday) -> bool`:是否 A 股交易时段(工作日 9:30–11:30 或 13:00–15:00);用于 §8.2-H 手动/自动统一门禁。
  - 常量 `MONEY_EFFECT_COLD/CALM/ACTIVE/HOT` = "cold"/"calm"/"active"/"hot"。

- [ ] **Step 1: 写失败测试**

创建 `src-tauri/src/invest/macro_verdict.rs`,先只放测试与签名占位:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveTime, Weekday};

    #[test]
    fn test_money_effect_tier() {
        assert_eq!(money_effect_tier(4.0), "cold");   // ≤6
        assert_eq!(money_effect_tier(6.0), "cold");   // 边界含在冰点
        assert_eq!(money_effect_tier(7.5), "calm");   // 6–9
        assert_eq!(money_effect_tier(15.0), "active"); // 9–21
        assert_eq!(money_effect_tier(25.0), "hot");   // >21
    }

    #[test]
    fn test_is_trading_session() {
        let wed = Weekday::Wed;
        assert!(is_trading_session(NaiveTime::from_hms_opt(10, 0, 0).unwrap(), wed));
        assert!(is_trading_session(NaiveTime::from_hms_opt(14, 30, 0).unwrap(), wed));
        assert!(!is_trading_session(NaiveTime::from_hms_opt(12, 0, 0).unwrap(), wed)); // 午休
        assert!(!is_trading_session(NaiveTime::from_hms_opt(8, 0, 0).unwrap(), wed));  // 盘前
        assert!(!is_trading_session(NaiveTime::from_hms_opt(16, 0, 0).unwrap(), wed)); // 盘后
        assert!(!is_trading_session(NaiveTime::from_hms_opt(10, 0, 0).unwrap(), Weekday::Sat)); // 周末
    }
}
```

- [ ] **Step 2: 跑测试确认失败(未定义)**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- invest::macro_verdict::tests --nocapture"`
Expected: FAIL(编译错误,函数未定义)。注:需先在 `invest/mod.rs` 声明 `pub mod macro_verdict;`,否则测试不被编译。

- [ ] **Step 3: 实现纯函数(插到文件顶部,测试之前)**

```rust
//! 全局宏观判断 job:取数 → MACRO_GLOBAL_PROMPT → 写 macro_verdict 对象。
//! 赚钱效应档位由固定阈值规则确定性计算(§8.1 校准),LLM 仅写理由。

use chrono::{Timelike, Weekday};

pub const MONEY_EFFECT_COLD: &str = "cold";
pub const MONEY_EFFECT_CALM: &str = "calm";
pub const MONEY_EFFECT_ACTIVE: &str = "active";
pub const MONEY_EFFECT_HOT: &str = "hot";

/// 占比(涨幅>3%家数/有效家数 ×100)→ 赚钱效应档位。阈值来自 2 年校准(§8.1)。
pub fn money_effect_tier(ratio_pct: f64) -> &'static str {
    if ratio_pct <= 6.0 {
        MONEY_EFFECT_COLD
    } else if ratio_pct <= 9.0 {
        MONEY_EFFECT_CALM
    } else if ratio_pct <= 21.0 {
        MONEY_EFFECT_ACTIVE
    } else {
        MONEY_EFFECT_HOT
    }
}

/// 是否处于 A 股连续竞价交易时段(9:30–11:30 或 13:00–15:00 的工作日)。
pub fn is_trading_session(now_cst: chrono::NaiveTime, weekday: Weekday) -> bool {
    if matches!(weekday, Weekday::Sat | Weekday::Sun) {
        return false;
    }
    let mins = now_cst.hour() * 60 + now_cst.minute();
    (570..=690).contains(&mins) || (780..=900).contains(&mins) // 9:30-11:30 / 13:00-15:00
}
```

3b. `src-tauri/src/invest/mod.rs` 加声明(按现有模块顺序):
```rust
pub mod macro_verdict;
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- invest::macro_verdict::tests --nocapture"`
Expected: PASS(两个测试)。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/macro_verdict.rs src-tauri/src/invest/mod.rs
git commit -m "feat(invest): 赚钱效应分档 + 交易时段门禁纯函数"
```

---

### Task 7: MACRO_GLOBAL_PROMPT 常量 + parser `emotion_temperature`→`money_effect_reason`

**Files:**
- Modify: `src-tauri/src/invest/committee/roles.rs`(新增 `MACRO_GLOBAL_PROMPT`,`pub`)
- Modify: `src-tauri/src/invest/committee/parser.rs`(字段改名 + 提取键 + 两个单测)

**Interfaces:**
- Produces:
  - `pub const MACRO_GLOBAL_PROMPT: &str`:纯全局判断,**无 symbol 上下文、无敏感度**;含占位 `{{ma20}}`/`{{ma60}}`/`{{vol20_trend}}`/`{{breadth}}`(Task 8 填充);输出字段 signal/强度/信号理由/市场阶段/市场阶段理由/赚钱效应理由。
  - `ParsedFields.money_effect_reason: Option<String>`(替换 `emotion_temperature`)。
- Consumes(下一 Task):Task 8 用 `MACRO_GLOBAL_PROMPT` + `parse_role_output(Macro, ...)`。

- [ ] **Step 1: 加 MACRO_GLOBAL_PROMPT 常量(roles.rs,MACRO_PROMPT 之后 line 288)**

```rust
/// 全局宏观判断专用 prompt：不含任何 symbol 上下文与敏感度。
/// 注入真实 MA20/MA60 + 波动率趋势(修复 B2)+ 市场广度作为市场阶段/赚钱效应判据。
pub const MACRO_GLOBAL_PROMPT: &str = r#"你是投资委员会的宏观分析师，为整个 A 股市场提供全局环境判断（不针对任何个股）。

**你的职责（只输出以下内容）**：
1. 全局市场底色信号（risk_on/risk_off/neutral）
2. 信号强度（0-10）
3. 市场环境阶段判断（主升/分歧/退潮/冰点/混沌）
4. 赚钱效应理由——用市场广度解释当前赚钱难易（档位由系统按占比规则确定，你只写理由）

**系统注入的真实数据**（据此判断，不要编造均线数值）：
- 上证 MA20 / MA60：{{ma20}} / {{ma60}}
- 20 日波动率趋势：{{vol20_trend}}
- 市场广度：{{breadth}}

**市场阶段判定规则**：
- 主升：上证站上 MA60 且 MA20>MA60，两市成交额 >1.2 万亿，涨幅>3%占比高
- 分歧：指数高位震荡，涨跌家数接近，广度中性
- 退潮：指数跌破 MA20，成交额萎缩，涨幅>3%占比下降
- 冰点：指数跌破 MA60，跌停家数 > 涨停，涨幅>3%占比极低
- 混沌：特征不明显或信号矛盾

**输出要求**：
- 必须中文；严格按下列格式，每项换行；每个字段值一句话结束，不分点
- 严禁个股技术面分析、严禁操作建议、严禁抱怨工具不可用

信号: risk_on | risk_off | neutral
强度: 0-10
信号理由: <一句话说明信号判断依据>
市场阶段: 主升 | 分歧 | 退潮 | 冰点 | 混沌
市场阶段理由: <一句话说明阶段判断依据>
赚钱效应理由: <一句话，基于涨幅>3%占比与涨跌停对比说明赚钱难易>"#;
```

- [ ] **Step 2: parser 字段改名 + 提取键**

2a. `parser.rs` line 47-48:
```rust
    /// 赚钱效应理由(档位由规则算,此处仅 LLM 理由句)
    pub money_effect_reason: Option<String>,
```
2b. `parse_macro`(line 584-586):
```rust
    // 赚钱效应理由(替代原情绪温度)
    parsed.money_effect_reason =
        extract_field_any(text, &["MONEY_EFFECT_REASON", "赚钱效应理由"]);
```

- [ ] **Step 3: 改两个 parser 单测(line 921-936)**

```rust
        let text = "SIGNAL: risk_on\nSTRENGTH: 7\n市场阶段: 主升\n敏感度: positive\n敏感度原因: 北向资金持续流入\n赚钱效应理由: 涨幅超3%个股占比高";
```
对应断言(line 926):
```rust
        assert_eq!(parsed.money_effect_reason.as_deref(), Some("涨幅超3%个股占比高"));
```
第二个测试(line 931 / 936):
```rust
        let text = "SIGNAL: risk_off\nSTRENGTH: 3\nMARKET_PHASE: 退潮\nSENSITIVITY: negative\nSENSITIVITY_REASON: trade war\nMONEY_EFFECT_REASON: 跌停多于涨停";
```
```rust
        assert_eq!(parsed.money_effect_reason.as_deref(), Some("跌停多于涨停"));
```

- [ ] **Step 4: cargo check + parser 测试**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 若有其它文件引用 `emotion_temperature` 会报错——本仓库 grep 确认仅 parser.rs 内部引用,check 应通过。
Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- committee::parser::tests --nocapture"`
Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/committee/roles.rs src-tauri/src/invest/committee/parser.rs
git commit -m "feat(invest): MACRO_GLOBAL_PROMPT + parser money_effect_reason 字段"
```

---

### Task 8: macro_verdict job 主体(取数→prompt→CLI→分档→写对象)

**Files:**
- Modify: `src-tauri/src/invest/macro_verdict.rs`(在纯函数之后追加 `run_macro_verdict`)

**Interfaces:**
- Consumes: `roles::MACRO_GLOBAL_PROMPT`;`CliCommitteeExecutor::global()` + `run_role`;`parser::parse_role_output`;`macro_cache`(current_data_version/load/int'l MA);`macro_verdict`(存储 save_verdict);`money_effect_tier`/`is_trading_session`(Task 6);`cli_executor::write_committee_settings_json`。
- Produces: `pub async fn run_macro_verdict(manual: bool) -> Result<String, String>`(scheduler + 手动命令共用;`manual` 仅影响日志)。

- [ ] **Step 1: 加取数+prompt 助手(纯拼装,便于复用)**

在 `is_trading_session` 之后加:

```rust
use crate::storage::invest::{macro_cache, macro_verdict as store};

/// 组装 MACRO_GLOBAL_PROMPT 的占位填充值(MA20/MA60/波动率趋势/广度串)。
fn fill_global_prompt(ma20: Option<f64>, ma60: Option<f64>, vol20: Option<f64>,
    up: f64, flat: f64, down: f64, lu: f64, ld: f64, up3: f64, valid: f64) -> String {
    let fmt = |o: Option<f64>| o.map(|v| format!("{v:.2}")).unwrap_or_else(|| "N/A".into());
    // vol20 为年化波动率(×√252,见 spec B3)。25% 为经验分界,非严格校准;
    // 仅作 prompt 的粗趋势提示("偏高/平稳"),不参与档位规则计算,魔数可接受。
    let vol_trend = match vol20 {
        Some(v) if v > 25.0 => "偏高",
        Some(_) => "平稳",
        None => "N/A",
    };
    let ratio = if valid > 0.0 { up3 / valid * 100.0 } else { 0.0 };
    let breadth = format!(
        "涨{up:.0}/平{flat:.0}/跌{down:.0}，涨停{lu:.0}/跌停{ld:.0}，涨幅>3% {up3:.0}只(占比{ratio:.1}%)",
    );
    crate::invest::committee::roles::MACRO_GLOBAL_PROMPT
        .replace("{{ma20}}", &fmt(ma20))
        .replace("{{ma60}}", &fmt(ma60))
        .replace("{{vol20_trend}}", vol_trend)
        .replace("{{breadth}}", &breadth)
}

/// 读 committee_tuning.json → 生成 CLI --settings 路径(provider 路由)。
fn resolve_settings_path() -> Option<std::path::PathBuf> {
    let p = crate::storage::data_dir().join("invest").join("committee_tuning.json");
    let (provider, model) = std::fs::read_to_string(&p).ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| (v["selectedProvider"].as_str().unwrap_or("default").to_string(),
                  v["model"].as_str().unwrap_or("").to_string()))
        .unwrap_or_else(|| ("default".into(), String::new()));
    let model_opt = if model.is_empty() { None } else { Some(model.as_str()) };
    crate::invest::committee::cli_executor::write_committee_settings_json(&provider, model_opt)
        .ok().flatten()
}
```

- [ ] **Step 2: 加 MA20/MA60 计算(复用 miniQMT/international kline)**

```rust
/// 取上证 25 日线算 MA20/MA60(数据不足返回 None)。走 international kline(miniQMT 优先)。
async fn fetch_sh_ma() -> (Option<f64>, Option<f64>) {
    let client = crate::invest::international::InternationalClient::from_settings();
    let bars = match client.fetch_xtdata_kline("000001.SH", "1d", 60).await {
        Ok(b) if b.len() >= 20 => b,
        _ => return (None, None),
    };
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let ma = |n: usize| if closes.len() >= n {
        Some(closes.iter().rev().take(n).sum::<f64>() / n as f64)
    } else { None };
    (ma(20), ma(60))
}
```

注:此处 `fetch_xtdata_kline` 在 miniQMT 关闭/离线时会 Err → MA=None，prompt 显示 N/A，与 §8.2-C 降级一致(不崩)。

- [ ] **Step 3: 加主函数 run_macro_verdict**

```rust
/// 全局宏观判断主流程。scheduler 与手动命令共用。
/// 非交易时段:不跑真广度,直接复用最近收盘定版(§8.2-H)。
pub async fn run_macro_verdict(manual: bool) -> Result<String, String> {
    use chrono::{Datelike, Timelike};
    // 时段门禁:非交易时段不重算,保留最近收盘定版。
    let cst = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let now = chrono::Utc::now().with_timezone(&cst);
    let t = chrono::NaiveTime::from_hms_opt(now.hour(), now.minute(), 0).unwrap();
    if !is_trading_session(t, now.weekday()) {
        log::info!("macro_verdict: 非交易时段(manual={manual}),复用最近收盘定版");
        return Ok("skipped: non-trading session, reused last verdict".into());
    }

    // 取广度(同源)。miniQMT 关/离线 → 广度缺失,赚钱效应"数据不足"。
    let client = crate::invest::international::InternationalClient::from_settings();
    let miniqmt_on = crate::storage::settings::load().user.invest_miniqmt_enabled;
    let breadth = if miniqmt_on { client.fetch_xtdata_breadth().await.ok() } else { None };
    let b = breadth.filter(|b| b.available && b.valid > 0);

    let (ma20, ma60) = fetch_sh_ma().await;
    let vol20 = macro_cache::load_macro_cache("sh_composite_vol20").ok().flatten().and_then(|e| e.value);

    let (up, flat, down, lu, ld, up3, valid) = match &b {
        Some(x) => (x.up as f64, x.flat as f64, x.down as f64,
                    x.limit_up as f64, x.limit_down as f64, x.up_over_3pct as f64, x.valid as f64),
        None => (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
    };

    let sys_prompt = fill_global_prompt(ma20, ma60, vol20, up, flat, down, lu, ld, up3, valid);
    let cli = crate::invest::committee::cli_executor::CliCommitteeExecutor::global()
        .ok_or("claude CLI not found")?;
    let settings = resolve_settings_path();
    let raw = cli.run_role(&sys_prompt, "请给出当前 A 股全局宏观判断。", 120,
        settings.as_deref(), None).await?;

    let parsed = crate::invest::committee::parser::parse_role_output(
        crate::invest::committee::roles::CommitteeRole::Macro, &raw, false);

    // 赚钱效应档位:规则确定性算(数据不足时 None)。
    let money_effect = if valid > 0.0 {
        Some(money_effect_tier(up3 / valid * 100.0).to_string())
    } else { None };

    let verdict = store::MacroVerdict {
        signal: parsed.signal,
        strength: parsed.strength,
        market_phase: parsed.market_phase,
        money_effect,
        money_effect_reason: parsed.money_effect_reason,
        signal_reason: parsed.signal_reason,
        market_phase_reason: parsed.market_phase_reason,
        based_on_data_version: macro_cache::current_data_version()?,
        updated_at: String::new(), // save 时 DB 填 datetime('now')
    };
    store::save_verdict(&verdict)?;
    Ok(format!("macro_verdict updated: signal={:?} money_effect={:?}",
        verdict.signal, verdict.money_effect))
}
```

- [ ] **Step 4: cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。若报 `parse_role_output` 参数不符,核对 parser.rs line 116 真实签名并对齐(应为 `(role, text, is_retry)`)。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/macro_verdict.rs
git commit -m "feat(invest): macro_verdict job 主体(取数→全局prompt→分档→写对象)"
```

---

### Task 9: 调度层 `macro_verdict` job + dispatch 分支

**Files:**
- Modify: `src-tauri/src/invest/scheduler/mod.rs`(`default_jobs()` 尾部加 job)
- Modify: `src-tauri/src/invest/scheduler/runner.rs`(`dispatch_job` match 加分支)

**Interfaces:**
- Consumes: `macro_verdict::run_macro_verdict(bool)`(Task 8)。
- Produces: id=`macro_verdict` 的 CronJob;dispatch 分支返回其 `Result<String,String>`。
- 说明:cron 6 字段(秒 分 时 日 月 周)。macro_refresh 在 `*/15`(:00/:15/:30/:45),本 job 错峰到 **:05/:35/:55**,仅开盘→收盘:`0 5,35,55 9,10,11,13,14 * * 1-5`(用户定案:含 11:05 上午读数 + 14:55 收盘定版)。门禁 `is_trading_session`(9:30-11:30/13:00-15:00)挡掉 9:05/11:35/11:55 废触发,时段外真正跳过由 `run_macro_verdict` 内部门禁兜底(§8.2-H),cron 只做粗过滤。

- [ ] **Step 1: default_jobs() 加 job(在 clearance_convert 之后、闭合 `]` 前)**

`src-tauri/src/invest/scheduler/mod.rs` line 150(`clearance_convert` 的 `},` 后):

```rust
        CronJob {
            id: "macro_verdict".into(),
            name: "全局宏观判断".into(),
            // 开盘→收盘每 30 分钟(错峰 macro_refresh 的 */15),排除午休。
            cron_expr: "0 5,35,55 9,10,11,13,14 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "开盘时段每30分钟产出全局宏观判断(赚钱效应/市场阶段/signal)".into(),
            dedicated: false,
        },
```

- [ ] **Step 2: dispatch_job 加分支(runner.rs,`clearance_convert` 分支后 line 121)**

```rust
        "macro_verdict" => {
            crate::invest::macro_verdict::run_macro_verdict(false).await
        }
```

- [ ] **Step 3: cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。

- [ ] **Step 4: 验证 job 注册(临时测试确认 default_jobs 含 macro_verdict)**

在 `scheduler/mod.rs` 的 `#[cfg(test)] mod tests`(若无则新建)加:
```rust
    #[test]
    fn test_macro_verdict_job_registered() {
        assert!(default_jobs().iter().any(|j| j.id == "macro_verdict" && j.requires_trading_day));
    }
```
Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- scheduler::tests::test_macro_verdict_job_registered --nocapture"`
Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/scheduler/mod.rs src-tauri/src/invest/scheduler/runner.rs
git commit -m "feat(invest): macro_verdict cron job + dispatch 分支(错峰+排除午休)"
```

---

### Task 10: Tauri 命令 `get_macro_verdict` / `refresh_macro_verdict` + 注册

**Files:**
- Modify: `src-tauri/src/commands/invest.rs`(文件末尾加两个命令 + 包装 struct)
- Modify: `src-tauri/src/lib.rs`(`invoke_handler!` 注册)

**Interfaces:**
- Consumes: `storage::invest::macro_verdict::{load_verdict, is_current, MacroVerdict}`;`macro_cache::current_data_version`;`invest::macro_verdict::run_macro_verdict`。
- Produces:
  - `pub struct MacroVerdictView { verdict: Option<MacroVerdict>, is_current: bool }`(camelCase)。
  - `#[tauri::command] get_macro_verdict() -> Result<MacroVerdictView, String>`。
  - `#[tauri::command] async fn refresh_macro_verdict() -> Result<String, String>`(手动入口,调 `run_macro_verdict(true)`,内部时段门禁保证非交易时段不产假判断)。

- [ ] **Step 1: 加命令(commands/invest.rs 末尾)**

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroVerdictView {
    pub verdict: Option<crate::storage::invest::macro_verdict::MacroVerdict>,
    pub is_current: bool,
}

/// 读全局宏观判断 + 新鲜度(是否对应当前数据版本)。
#[tauri::command]
pub fn get_macro_verdict() -> Result<MacroVerdictView, String> {
    use crate::storage::invest::{macro_cache, macro_verdict};
    let verdict = macro_verdict::load_verdict()?;
    let is_current = match &verdict {
        Some(v) => macro_verdict::is_current(v, &macro_cache::current_data_version()?),
        None => false,
    };
    Ok(MacroVerdictView { verdict, is_current })
}

/// 手动刷新全局宏观判断(非交易时段内部会跳过真跑,复用收盘定版)。
#[tauri::command]
pub async fn refresh_macro_verdict() -> Result<String, String> {
    crate::invest::macro_verdict::run_macro_verdict(true).await
}
```

- [ ] **Step 2: lib.rs 注册**

`src-tauri/src/lib.rs` 的 `invoke_handler!` 内,`commands::invest::trigger_cron_job,`(line 464)后加:
```rust
            commands::invest::get_macro_verdict,
            commands::invest::refresh_macro_verdict,
```

- [ ] **Step 3: cargo check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/invest.rs src-tauri/src/lib.rs
git commit -m "feat(invest): get_macro_verdict / refresh_macro_verdict 命令 + 注册"
```

---

### Task 11: 行业敏感度模块(SENSITIVITY_PROMPT + 行业|signal 内存缓存)

**Files:**
- Create: `src-tauri/src/invest/committee/sensitivity.rs`
- Modify: `src-tauri/src/invest/committee/mod.rs`(声明 `pub mod sensitivity;`)

**Interfaces:**
- Consumes: `CliCommitteeExecutor`;全局 signal(regime)+ industry 字符串。
- Produces:
  - `pub const SENSITIVITY_PROMPT: &str`(极简:输入=全局 signal + 行业,输出 sensitivity + 一句理由)。
  - `pub fn cache_key(industry: &str, signal: &str) -> String`(纯函数,`"industry|signal"`)。
  - `pub async fn analyze(industry: &str, signal: &str, settings_path: Option<&std::path::Path>, cancel: Option<&CancellationToken>) -> (Option<String>, Option<String>)`:返回 `(sensitivity, reason)`;命中缓存直接返回;signal 变更时清空缓存(§8.2-K)。

- [ ] **Step 1: 写 cache_key 失败测试 + 建文件骨架**

创建 `src-tauri/src/invest/committee/sensitivity.rs`:

```rust
//! 行业敏感度:全局 regime × 行业 → positive/negative/neutral。
//! 单次运行进程内存缓存(§8.2-K):key=行业|signal;signal 变更清空,不持久化。

use std::collections::HashMap;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub const SENSITIVITY_PROMPT: &str = r#"你是宏观敏感度分析助手。给定当前 A 股全局信号与某行业,判断该行业在此环境下的敏感度。

全局信号: {{signal}}
所属行业: {{industry}}

只输出两行,中文,不解释多余内容:
敏感度: positive | negative | neutral
敏感度理由: <一句话≤20字>"#;

/// 缓存键:行业|signal(纯函数,可测)。
pub fn cache_key(industry: &str, signal: &str) -> String {
    format!("{industry}|{signal}")
}

static CACHE: Mutex<Option<(String, HashMap<String, (Option<String>, Option<String>)>)>> = Mutex::new(None);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_cache_key() {
        assert_eq!(cache_key("白酒", "risk_on"), "白酒|risk_on");
        assert_ne!(cache_key("白酒", "risk_on"), cache_key("白酒", "risk_off"));
    }
}
```

- [ ] **Step 2: 跑测试确认失败(mod 未声明 → 不编译)**

先在 `src-tauri/src/invest/committee/mod.rs` 加 `pub mod sensitivity;`(按现有顺序)。
Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- committee::sensitivity::tests::test_cache_key --nocapture"`
Expected: 首次因 `analyze` 未实现只有 warning,测试应能编译并 PASS;若因 CACHE 未用报 dead_code,继续 Step 3 补 analyze。

- [ ] **Step 3: 实现 analyze(带缓存 + regime 变更清空)**

在 `static CACHE` 之后、`mod tests` 之前加:

```rust
/// 分析行业敏感度。命中缓存直接返回;signal 变更(regime 切换)清空整表。
pub async fn analyze(
    industry: &str,
    signal: &str,
    settings_path: Option<&std::path::Path>,
    cancel: Option<&CancellationToken>,
) -> (Option<String>, Option<String>) {
    if industry.is_empty() || industry == "N/A" {
        return (None, None);
    }
    let key = cache_key(industry, signal);
    {
        let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        match &*guard {
            Some((sig, map)) if sig == signal => {
                if let Some(hit) = map.get(&key) {
                    return hit.clone();
                }
            }
            _ => *guard = Some((signal.to_string(), HashMap::new())), // regime 变更 → 清空
        }
    }
    let cli = match super::cli_executor::CliCommitteeExecutor::global() {
        Some(c) => c,
        None => return (None, None),
    };
    let sys = SENSITIVITY_PROMPT.replace("{{signal}}", signal).replace("{{industry}}", industry);
    let raw = match cli.run_role(&sys, "请判断该行业敏感度。", 60, settings_path, cancel).await {
        Ok(t) => t,
        Err(_) => return (None, None),
    };
    let parsed = super::parser::parse_role_output(super::roles::CommitteeRole::Macro, &raw, false);
    let result = (parsed.sensitivity.clone(), parsed.sensitivity_reason.clone());
    if let Some((sig, map)) = &mut *CACHE.lock().unwrap_or_else(|e| e.into_inner()) {
        if sig == signal {
            map.insert(key, result.clone());
        }
    }
    result
}
```

- [ ] **Step 4: cargo check + 测试**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。
Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- committee::sensitivity::tests --nocapture"`
Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/committee/sensitivity.rs src-tauri/src/invest/committee/mod.rs
git commit -m "feat(invest): 行业敏感度模块(SENSITIVITY_PROMPT + 行业|signal 内存缓存)"
```

---

### Task 12: 编排 B1 修复 — run_macro_phase 改读全局判断 + 敏感度

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs`(重写 `run_macro_phase` 函数体 line 1113-1180)

**Interfaces:**
- Consumes: `storage::invest::macro_verdict::load_verdict`;`sensitivity::analyze`;`AssetContext.industry`;`ParsedFields`(signal/strength/market_phase/sensitivity/sensitivity_reason/money_effect_reason)。
- Produces:`run_macro_phase` 不变的签名/返回 `(RoundOutput, u32)`;`round_outputs[0].parsed.signal/strength` 语义不变(下游无需改)。
- 行为变更:不再 per-symbol 调宏观 LLM(修复 B1);全 symbol 共享同一全局 signal;仅敏感度 per-symbol(带缓存)。

- [ ] **Step 1: 重写 run_macro_phase 函数体(整体替换 line 1113-1180)**

```rust
async fn run_macro_phase(
    symbol: &str,
    config: &CommitteeConfig,
    _portfolio_summary: &str,
    _emitter: &Option<EventEmitter>,
    asset_context: &AssetContext,
    _verdicts: &str,
    cancel: Option<&CancellationToken>,
) -> Result<(RoundOutput, u32), String> {
    let role = CommitteeRole::Macro;
    // B1 修复:读全局宏观判断对象(不再 per-symbol 调 LLM 重算)。
    let verdict = crate::storage::invest::macro_verdict::load_verdict().ok().flatten();

    let mut parsed = super::parser::ParsedFields::default();
    match &verdict {
        Some(v) => {
            parsed.signal = v.signal.clone();
            parsed.strength = v.strength;
            parsed.market_phase = v.market_phase.clone();
            parsed.market_phase_reason = v.market_phase_reason.clone();
            parsed.signal_reason = v.signal_reason.clone();
            parsed.money_effect_reason = v.money_effect_reason.clone();
            parsed.raw_text = format!("[全局宏观判断] signal={:?} strength={:?} phase={:?}",
                v.signal, v.strength, v.market_phase);
        }
        None => {
            parsed.signal = Some("neutral".into());
            parsed.raw_text = "[全局宏观判断未生成] 降级 neutral,请手动刷新宏观判断".into();
            parsed.fallback_reason = Some("macro_verdict_missing".into());
        }
    }

    // per-symbol 行业敏感度(带缓存,§8.2-K)。
    let signal = parsed.signal.clone().unwrap_or_else(|| "neutral".into());
    let industry = asset_context.industry.clone().unwrap_or_default();
    let (sens, reason) = super::sensitivity::analyze(
        &industry, &signal, config.settings_path.as_deref(), cancel).await;
    parsed.sensitivity = sens;
    parsed.sensitivity_reason = reason;

    Ok((RoundOutput { role, round: 1, parsed, latency_ms: 0, tokens_used: 0 }, 0))
}
```

- [ ] **Step 2: 处理 build_cli_macro_prompt / build_macro_user_msg 变为未使用**

`run_macro_phase` 不再调 `build_cli_macro_prompt`(cli_executor.rs)与 `build_macro_user_msg`(orchestrator.rs)。
Run: `cargo check --manifest-path src-tauri/Cargo.toml`
- 若 `build_macro_user_msg` 报 dead_code:删除该私有函数(orchestrator.rs 内,grep 定位)。
- `build_cli_macro_prompt` 若仍被 archive/其它路径引用则保留;若报 dead_code 则加 `#[allow(dead_code)]` 并注释"保留供未来 per-symbol 宏观回归",不删(降低回滚成本)。

- [ ] **Step 3: 全量 check + committee 测试回归**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: 零警告(dead_code 已按 Step 2 处理)。
Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- committee --nocapture"`
Expected: 现有 committee 测试全 PASS(signal/strength 语义未变)。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/invest/committee/orchestrator.rs
git commit -m "fix(invest): B1 委员会不再 per-symbol 重算宏观,改读全局判断+敏感度"
```

---

### Task 13: 后端全链集成验证

**Files:** 无新增(仅验证 + 记录)。

**说明:** `CommitteeResult.macro_snapshot`(orchestrator line 1722)**保留不动**——前端(计划 2)自行停止 per-symbol 渲染并改读全局卡片;后端继续挂载无害,降低耦合。

- [ ] **Step 1: 全量编译 + clippy**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
Expected: 零错误零警告。

- [ ] **Step 2: 全量单测**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib --nocapture"`
Expected: 全 PASS(重点:macro_cache/macro_verdict/macro_refresh/sensitivity/scheduler/committee::parser)。

- [ ] **Step 3: 手动烟测(QMT 在线,dev app)**

启动 dev app,开发者控制台调:
```js
await window.__TAURI__.core.invoke('trigger_cron_job', { jobId: 'macro_refresh' });
await window.__TAURI__.core.invoke('refresh_macro_verdict');
await window.__TAURI__.core.invoke('get_macro_verdict');
```
Expected:第三条返回 `{ verdict: {signal, strength, moneyEffect, ...}, isCurrent: true }`;`moneyEffect ∈ {hot,active,calm,cold}`;`basedOnDataVersion` 形如 `b:...|m:...`。

- [ ] **Step 4: 版本戳失效验证**

再次单独 `trigger_cron_job('macro_refresh')`(刷新广度批次戳,不刷 verdict),然后 `get_macro_verdict`。
Expected:`isCurrent: false`(§8.2-G:上游数据一刷新,旧判断即失效)。

- [ ] **Step 5: 非交易时段门禁验证**

非交易时段(或本机时间调到周末)调 `refresh_macro_verdict`。
Expected:返回 `"skipped: non-trading session, reused last verdict"`,不产生新判断、不覆盖收盘定版(§8.2-H)。

- [ ] **Step 6: 更新 memory**

更新 `[[miniqmt-market-breadth]]`:实现已落地,provider 方法为 `xtdata.market_breadth`(经 server.py `getattr` 自动暴露,无需注册表);Rust 侧 `InternationalClient::fetch_xtdata_breadth`。新增 memory 记录 `_verdict_input_batch` 版本戳机制(§8.2-G)与手动刷新时段门禁(§8.2-H)的落地位置。

---

## 自检结论(spec → plan 覆盖)

- §4.1 数据层(up_over_3pct/flat + 移除 vol20 展示):Task 3(指标)+ 计划 2(卡片撤 vol20)。vol20 仍留 cache 供 MA 波动率趋势(Task 8)。✅
- §4.2 全局判断层:Task 5(存储)+ Task 8(job)+ Task 9(调度)。✅
- §4.3 prompt 拆分 + money_effect:Task 7(global prompt + parser)+ Task 11(sensitivity prompt)。✅
- §4.4/4.5 卡片/配色:**计划 2**(前端)。
- §7.1 miniQMT 广度源:Task 1/2/4。✅
- §7.2 + §8.2-G 版本戳:Task 3(compose/current)+ Task 4(写标记行)+ Task 5(based_on)+ Task 13-4(失效验证)。✅
- §8-A 有效期(收盘定版/非交易时段):Task 6(门禁)+ Task 8(复用)+ Task 13-5。✅
- §8.1 + §8-B 分档阈值(规则算):Task 6(money_effect_tier)。✅
- §8-C miniQMT 降级"数据不足":Task 4(降级)+ Task 8(money_effect=None)。✅
- §8-D + §8.2-K 敏感度缓存:Task 11(行业|signal 内存缓存,regime 变更清空)。✅
- §8.2-H 手动刷新门禁:Task 6 + Task 8 + Task 10(refresh 命令走同一门禁)。✅
- §8.2-J 枚举闭集:money_effect(Task 6 常量)、signal/regime(沿用);前端 chip 对齐在计划 2。✅
- B1 修复:Task 12。✅ / B2(MA/波动率真实注入):Task 8 fill_global_prompt。✅ / B3(vol20 不进卡片):计划 2。

**遗留交界(交计划 2):** i18n 新 key、前端四态状态机(§8.2-I)、红涨绿跌配色审计、全局卡片/敏感度小条渲染。
