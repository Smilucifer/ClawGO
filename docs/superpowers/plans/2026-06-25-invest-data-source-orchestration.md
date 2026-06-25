# 数据源编排层 + miniQMT 接入 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 openInvest 取数建立统一的数据源编排层，实现 tushare 优先、判空降级 AkShare 的源链，接入 miniQMT 作为可选行情源，并修复北向资金永远为 0 的 BUG。

**Architecture:** 新建 `invest/data_source/` 编排层（类型+编排器 / 源链注册表 / 判空函数三文件），在现有 `TushareClient` 和 Python RPC 桥之上做"按指标分类选源 + 判空降级 + 命中源记录"。miniQMT 作为新 Python provider（`xtdata.py`）挂到现有 `python/bridge.rs`，与 AkShare 同一条 RPC 腿，软依赖、客户端离线无缝降级。

**Tech Stack:** Rust（Tauri 后端，rusqlite、reqwest、futures-util、chrono）、Python（JSON-RPC provider，xtquant 软依赖）、Svelte 5（设置项 UI）。

## Global Constraints

- 设计基线 v5.5.11，spec：`docs/superpowers/specs/2026-06-25-invest-data-source-orchestration-design.md`。
- 北向单位换算：tushare `moneyflow_hsgt` 金额字段单位为**百万元**，换算到亿元 **÷ 100**（1 亿元 = 100 百万元）。常量带注释，定义在 `macro_refresh::fetch_northbound`。
- 北向口径统一称 **"北向资金净流入"**（单位：亿元），写入 `northbound_net`。
- 编排层只管"选源 + 降级 + 记录"，不碰各源字段解析与单位换算（解析在 `client.rs` / 各 `.py` provider，单位换算在 `macro_refresh`）。
- 默认优先级：tushare 优先；取不到/为 0/NaN → 降级 AkShare。miniQMT 开关开启时行情类源链为 `MiniQmt → Tushare → Akshare`。
- 第一期 miniQMT 仅接行情类（K线/实时报价/health），**不**接财务六表、板块成分、Level2。
- 个股级数据（daily_basic/fina_indicator/moneyflow_dc/report_rc）**不**纳入编排层。
- Yahoo 海外指标取数保持原样。
- 设置项 `invest_miniqmt_enabled: bool`，默认 `false`，user 作用域。
- 受 CLAUDE.md §11 限制，Rust 验证用 `cargo check`（不跑二进制测试）；纯逻辑单测可写但以 `cargo check` 通过为准。
- UI 文案需同步 `messages/en.json` + `messages/zh-CN.json`。
- Conventional Commits（`feat:`/`fix:`/`chore:`）。

---

## 文件结构

| 文件 | 职责 |
|------|------|
| `src-tauri/src/invest/data_source/mod.rs` | 新建：`SourceId` 枚举、`Fetched<T>`、`fetch_with_chain` 编排器 |
| `src-tauri/src/invest/data_source/registry.rs` | 新建：`Category` 枚举、`chain_for` 源链登记表 |
| `src-tauri/src/invest/data_source/validity.rs` | 新建：`is_valid_number` 判空函数 |
| `src-tauri/src/invest/mod.rs` | 修改：注册 `data_source` 模块 |
| `src-tauri/src/tushare/client.rs` | 修改：`moneyflow_hsgt` 解析 `net_money` 缺失时回退 `north_money` |
| `src-tauri/src/invest/macro_refresh.rs` | 修改：北向单位换算修复；`MacroEntry` 扩展带 source；`fetch_*` 走编排层（北向/国债先行） |
| `src-tauri/src/invest/international.rs` | 修改：新增 `fetch_xtdata_kline`/`fetch_xtdata_health` RPC 包装 |
| `python-runtime/scripts/providers/xtdata.py` | 新建：miniQMT provider（kline/realtime_quote/health） |
| `python-runtime/scripts/server.py` | 修改：注册 xtdata provider |
| `src-tauri/src/models.rs` | 修改：`User` 新增 `invest_miniqmt_enabled` 字段 |
| `src-tauri/src/storage/settings.rs` | 修改：patch 注册 `invest_miniqmt_enabled` |
| `messages/en.json` + `zh-CN.json` | 修改：miniQMT 开关 UI 文案 |

---

## Task 1: 北向资金 BUG 修复（解析回退 + 单位换算）

**Files:**
- Modify: `src-tauri/src/tushare/client.rs:128-130`（`moneyflow_hsgt` 解析）
- Modify: `src-tauri/src/invest/macro_refresh.rs:124-150`（`fetch_northbound`）

**Interfaces:**
- Consumes: `MoneyflowHsgt { trade_date, north_money, south_money, net_money }`（现有结构）
- Produces: `fetch_northbound` 返回的 `northbound_net` 值 = `north_money ÷ 100`（亿元）

**背景：** tushare `moneyflow_hsgt` 的 `net_money` 字段 2024-08 停更，现行接口字段为 `trade_date, ggt_ss, ggt_sz, hgt, sgt, north_money, south_money`，无 `net_money`，解析 `.unwrap_or_default()` → 永远 0。实测 `north_money` = `hgt`+`sgt`，自洽。

- [ ] **Step 1: 修改 `client.rs` 解析，net_money 缺失时回退 north_money**

`src-tauri/src/tushare/client.rs`，将 `moneyflow_hsgt` 中 `net_money` 字段的解析（约 1128-1130 行）改为：

```rust
net_money: net_money_idx
    .and_then(|i| get_f64(row, i))
    // net_money 字段自 2024-08 交易所停更后已废弃，现行接口不返回。
    // 缺失时回退到 north_money（= hgt + sgt，实测自洽）。
    .or_else(|| north_money_idx.and_then(|i| get_f64(row, i)))
    .unwrap_or_default(),
```

- [ ] **Step 2: 修改 `fetch_northbound`，改用 north_money 并换算单位**

`src-tauri/src/invest/macro_refresh.rs:124-150`，将函数体改为：

```rust
/// northbound_net from Tushare moneyflow_hsgt.
///
/// tushare 金额字段单位为百万元；换算到亿元需 ÷ 100（1 亿元 = 100 百万元）。
/// net_money 字段已于 2024-08 停更，改用 north_money（= 沪股通 + 深股通）。
async fn fetch_northbound(
    client: TushareClient,
    start_date: String,
    end_date: String,
) -> MacroResult {
    /// tushare 百万元 → 亿元换算系数。
    const MILLION_TO_YI: f64 = 100.0;

    let flows = client
        .moneyflow_hsgt(&start_date, &end_date)
        .await
        .map_err(|e| format!("northbound moneyflow_hsgt: {e}"))?;

    let latest = flows
        .iter()
        .max_by_key(|f| &f.trade_date)
        .ok_or("northbound: no data")?;

    // north_money 单位百万元，转亿元。
    let northbound_yi = latest.north_money / MILLION_TO_YI;

    Ok(vec![(
        "northbound_net".to_string(),
        Some(northbound_yi),
        Some(serde_json::json!({
            "trade_date": latest.trade_date,
            "north_money_yi": northbound_yi,
            "south_money_yi": latest.south_money / MILLION_TO_YI,
        }).to_string()),
    )])
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过，无 error。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/tushare/client.rs src-tauri/src/invest/macro_refresh.rs
git commit -m "fix(invest): 修复北向资金永远为0，改用 north_money 并换算百万元→亿元"
```

---

## Task 2: 判空函数 validity.rs

**Files:**
- Create: `src-tauri/src/invest/data_source/validity.rs`

**Interfaces:**
- Produces: `pub fn is_valid_number(v: &Option<f64>) -> bool`

- [ ] **Step 1: 创建 validity.rs 含函数与单测**

Create `src-tauri/src/invest/data_source/validity.rs`:

```rust
//! 数据源取数结果的判空规则。
//!
//! 用户要求："实测取不到或为 0 则降级"。判空函数返回 false 即触发源链降级。

/// 默认数值判空：None / 0.0 / 非有限（NaN/Inf）视为无效。
pub fn is_valid_number(v: &Option<f64>) -> bool {
    matches!(v, Some(x) if *x != 0.0 && x.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_is_invalid() {
        assert!(!is_valid_number(&None));
    }

    #[test]
    fn zero_is_invalid() {
        assert!(!is_valid_number(&Some(0.0)));
    }

    #[test]
    fn nan_is_invalid() {
        assert!(!is_valid_number(&Some(f64::NAN)));
    }

    #[test]
    fn inf_is_invalid() {
        assert!(!is_valid_number(&Some(f64::INFINITY)));
    }

    #[test]
    fn positive_is_valid() {
        assert!(is_valid_number(&Some(1.408)));
    }

    #[test]
    fn negative_is_valid() {
        // 负值（如净流出）是有效数据，不应降级。
        assert!(is_valid_number(&Some(-42.0)));
    }
}
```

- [ ] **Step 2: 验证编译（模块尚未注册，单独 check 会因未引用而 warn，先跳过到 Task 5 注册后统一验证）**

本步仅确认文件语法。等 Task 5 注册模块后统一 `cargo check`。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/invest/data_source/validity.rs
git commit -m "feat(invest): 新增数据源判空函数 is_valid_number"
```

---

## Task 3: 源链注册表 registry.rs

**Files:**
- Create: `src-tauri/src/invest/data_source/registry.rs`

**Interfaces:**
- Consumes: `SourceId`（Task 4 定义；本任务先在文件内 `use super::SourceId`，Task 4 完成后可编译）
- Produces: `pub enum Category`、`pub fn chain_for(category: Category, miniqmt_on: bool) -> Vec<SourceId>`

> **执行顺序提示：** Task 3 与 Task 4 互相引用，应作为一组提交后统一编译。建议先写 Task 4（定义 SourceId），再写本任务。

- [ ] **Step 1: 创建 registry.rs**

Create `src-tauri/src/invest/data_source/registry.rs`:

```rust
//! 指标类别 → 源链优先级登记表。
//!
//! 这是**唯一**决定取数源优先级的地方。调整顺序或新增源只改这里。

use super::SourceId;

/// 指标类别。决定源链构成。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// 行情类（K线/实时报价/指数行情）。miniQMT 开关影响此类。
    Quote,
    /// 资金面（北向/两融）。
    Capital,
    /// 宏观面（shibor/国债）。
    Macro,
    /// tushare 独有（moneyflow_dc/report_rc），无降级意义。
    TushareOnly,
    /// 海外（VIX/美债/黄金/原油/汇率）。Yahoo 专属。
    Overseas,
}

/// 返回某类别的有序源链。`miniqmt_on` 仅影响 Quote 类。
pub fn chain_for(category: Category, miniqmt_on: bool) -> Vec<SourceId> {
    use SourceId::*;
    match category {
        Category::Quote => {
            if miniqmt_on {
                vec![MiniQmt, Tushare, Tencent]
            } else {
                vec![Tushare, Tencent]
            }
        }
        Category::Capital => vec![Tushare, Akshare],
        Category::Macro => vec![Tushare, Akshare],
        Category::TushareOnly => vec![Tushare],
        Category::Overseas => vec![Yahoo],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invest::data_source::SourceId;

    #[test]
    fn quote_prepends_miniqmt_when_on() {
        assert_eq!(
            chain_for(Category::Quote, true),
            vec![SourceId::MiniQmt, SourceId::Tushare, SourceId::Akshare]
        );
    }

    #[test]
    fn quote_omits_miniqmt_when_off() {
        assert_eq!(
            chain_for(Category::Quote, false),
            vec![SourceId::Tushare, SourceId::Akshare]
        );
    }

    #[test]
    fn capital_ignores_miniqmt_flag() {
        assert_eq!(chain_for(Category::Capital, true), vec![SourceId::Tushare, SourceId::Akshare]);
        assert_eq!(chain_for(Category::Capital, false), vec![SourceId::Tushare, SourceId::Akshare]);
    }

    #[test]
    fn tushare_only_is_single_source() {
        assert_eq!(chain_for(Category::TushareOnly, true), vec![SourceId::Tushare]);
    }

    #[test]
    fn overseas_is_yahoo_only() {
        assert_eq!(chain_for(Category::Overseas, true), vec![SourceId::Yahoo]);
    }
}
```

- [ ] **Step 2: Commit（与 Task 4 一并编译验证）**

```bash
git add src-tauri/src/invest/data_source/registry.rs
git commit -m "feat(invest): 新增源链注册表 chain_for"
```

---

## Task 4: 编排器 mod.rs

**Files:**
- Create: `src-tauri/src/invest/data_source/mod.rs`

**Interfaces:**
- Produces:
  - `pub enum SourceId { MiniQmt, Tushare, Akshare, Yahoo }`（含 `as_str()` 供 macro_cache.source 记录）
  - `pub struct Fetched<T> { pub value: T, pub source: SourceId }`
  - `pub async fn fetch_with_chain<T, F, Fut>(chain, is_valid, try_source) -> Result<Fetched<T>, String>`
  - 重导出 `validity`、`registry` 子模块

- [ ] **Step 1: 创建 mod.rs**

Create `src-tauri/src/invest/data_source/mod.rs`:

```rust
//! 数据源编排层：按指标类别选源、判空降级、记录命中源。
//!
//! 只管"选源 + 降级 + 记录"，不碰各源内部字段解析与单位换算。

pub mod registry;
pub mod validity;

use std::future::Future;

/// 数据源标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceId {
    MiniQmt,
    Tushare,
    Akshare,
    Tencent,
    Yahoo,
}

impl SourceId {
    /// 写入 macro_cache.source 列的字符串标识。
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceId::MiniQmt => "miniqmt",
            SourceId::Tushare => "tushare",
            SourceId::Akshare => "akshare",
            SourceId::Tencent => "tencent",
            SourceId::Yahoo => "yahoo",
        }
    }
}

/// 一次取数的结果，带命中源信息。
#[derive(Debug, Clone)]
pub struct Fetched<T> {
    pub value: T,
    pub source: SourceId,
}

/// 按源链依次尝试取数：成功且通过判空则返回；否则降级到下一个源。
///
/// - `chain`: 有序源链（来自 `registry::chain_for`）。
/// - `is_valid`: 判空函数，返回 false 触发降级。
/// - `try_source`: 对某个源发起取数的异步闭包。
pub async fn fetch_with_chain<T, F, Fut>(
    chain: &[SourceId],
    is_valid: impl Fn(&T) -> bool,
    try_source: F,
) -> Result<Fetched<T>, String>
where
    F: Fn(SourceId) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let mut last_err = String::from("empty source chain");
    for &source in chain {
        match try_source(source).await {
            Ok(value) if is_valid(&value) => {
                return Ok(Fetched { value, source });
            }
            Ok(_) => {
                log::debug!("data_source: {} returned invalid value, falling back", source.as_str());
                last_err = format!("{} returned invalid value", source.as_str());
            }
            Err(e) => {
                log::debug!("data_source: {} failed: {e}, falling back", source.as_str());
                last_err = format!("{}: {e}", source.as_str());
            }
        }
    }
    Err(format!("all sources exhausted: {last_err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn run(chain: Vec<SourceId>, behavior: impl Fn(SourceId) -> Result<f64, String>)
        -> Result<Fetched<f64>, String>
    {
        fetch_with_chain(
            &chain,
            |v: &f64| *v != 0.0,
            |s| {
                let r = behavior(s);
                async move { r }
            },
        ).await
    }

    #[tokio::test]
    async fn first_source_success() {
        let got = run(vec![SourceId::Tushare, SourceId::Akshare], |_| Ok(1.4)).await.unwrap();
        assert_eq!(got.source, SourceId::Tushare);
        assert_eq!(got.value, 1.4);
    }

    #[tokio::test]
    async fn falls_back_on_invalid() {
        let got = run(vec![SourceId::Tushare, SourceId::Akshare], |s| {
            if s == SourceId::Tushare { Ok(0.0) } else { Ok(1.7) }
        }).await.unwrap();
        assert_eq!(got.source, SourceId::Akshare);
        assert_eq!(got.value, 1.7);
    }

    #[tokio::test]
    async fn falls_back_on_error() {
        let got = run(vec![SourceId::Tushare, SourceId::Akshare], |s| {
            if s == SourceId::Tushare { Err("boom".into()) } else { Ok(2.2) }
        }).await.unwrap();
        assert_eq!(got.source, SourceId::Akshare);
    }

    #[tokio::test]
    async fn all_fail_returns_err() {
        let r = run(vec![SourceId::Tushare, SourceId::Akshare], |_| Err("x".into())).await;
        assert!(r.is_err());
    }
}
```

- [ ] **Step 2: Commit（与 Task 3 一并）**

```bash
git add src-tauri/src/invest/data_source/mod.rs
git commit -m "feat(invest): 新增数据源编排器 fetch_with_chain"
```

---

## Task 5: 注册 data_source 模块并验证编排层编译

**Files:**
- Modify: `src-tauri/src/invest/mod.rs`

**Interfaces:**
- Consumes: Task 2/3/4 的 `data_source` 子模块
- Produces: `crate::invest::data_source` 可被全 crate 引用

- [ ] **Step 1: 在 invest/mod.rs 注册模块**

`src-tauri/src/invest/mod.rs`，在模块声明区（与 `pub mod macro_refresh;` 等并列）加入：

```rust
pub mod data_source;
```

（按文件内现有 `pub mod` 的字母/分组顺序插入，保持风格一致。）

- [ ] **Step 2: 验证整个编排层编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过。`data_source::{mod, registry, validity}` 全部被引用，无 dead_code warning（若有 warning 因 `chain_for`/`fetch_with_chain` 暂未被调用，可接受，Task 7 接入后消除）。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/invest/mod.rs
git commit -m "feat(invest): 注册 data_source 编排层模块"
```

---

## Task 6: miniQMT Python provider（xtdata.py）+ 注册

**Files:**
- Create: `python-runtime/scripts/providers/xtdata.py`
- Modify: `python-runtime/scripts/server.py:94-98`（provider 注册区）

**Interfaces:**
- Produces（JSON-RPC method）：
  - `xtdata.health() -> {"available": bool, "reason": str}`
  - `xtdata.kline(symbol, period="1d", count=25) -> {"items": [{trade_date, open, high, low, close, vol}], "source": "miniqmt"}`
  - `xtdata.realtime_quote(symbols) -> {symbol: {last, ...}}`

**背景：** 沿用现有 provider 模式（`akshare_market.py` 的 `def func() -> dict` + `server.py` 的 `register_provider`）。xtquant 软依赖，未安装/客户端离线时 health 返回 available=false，不崩溃。

- [ ] **Step 1: 创建 xtdata.py**

Create `python-runtime/scripts/providers/xtdata.py`:

```python
# -*- coding: utf-8 -*-
"""miniQMT (xtquant.xtdata) 行情 provider。

软依赖：xtquant 未安装或 QMT 客户端未运行时，health() 返回 available=false，
其余函数抛异常由 RPC 层捕获，上层编排器据此降级到 tushare。
仅行情类（第一期）：kline / realtime_quote / health。
"""

_xtdata = None
_import_error = None


def _get_xtdata():
    """Lazy import xtquant.xtdata；失败记录原因。"""
    global _xtdata, _import_error
    if _xtdata is not None:
        return _xtdata
    if _import_error is not None:
        raise RuntimeError(_import_error)
    try:
        from xtquant import xtdata as xt
        _xtdata = xt
        return xt
    except Exception as e:  # noqa: BLE001
        _import_error = f"xtquant import failed: {e}"
        raise RuntimeError(_import_error)


def health() -> dict:
    """探测 miniQMT 是否可用。不抛异常。"""
    try:
        xt = _get_xtdata()
        # get_market_data_ex 对空列表应快速返回而不报错，作为客户端连通性探针。
        xt.get_sector_list()
        return {"available": True, "reason": ""}
    except Exception as e:  # noqa: BLE001
        return {"available": False, "reason": str(e)}


def kline(symbol: str = "", period: str = "1d", count: int = 25) -> dict:
    """获取历史 K线，字段名归一化为 tushare 风格。"""
    xt = _get_xtdata()
    fields = ["time", "open", "high", "low", "close", "volume"]
    # 触发本地下载后读取缓存。
    xt.download_history_data(symbol, period, "", "")
    data = xt.get_market_data_ex(fields, [symbol], period=period, count=count)
    df = data.get(symbol)
    items = []
    if df is not None:
        for idx in range(len(df)):
            row = df.iloc[idx]
            # time 为毫秒时间戳或 yyyymmdd，统一转 yyyymmdd 字符串。
            t = row["time"]
            trade_date = _to_yyyymmdd(t)
            items.append({
                "trade_date": trade_date,
                "open": float(row["open"]),
                "high": float(row["high"]),
                "low": float(row["low"]),
                "close": float(row["close"]),
                "vol": float(row["volume"]),
            })
    return {"items": items, "source": "miniqmt"}


def realtime_quote(symbols=None) -> dict:
    """获取实时快照。symbols 为代码列表。"""
    xt = _get_xtdata()
    if symbols is None:
        symbols = []
    if isinstance(symbols, str):
        symbols = [symbols]
    ticks = xt.get_full_tick(symbols)
    out = {}
    for code, t in (ticks or {}).items():
        out[code] = {
            "last": float(t.get("lastPrice", 0.0)),
            "volume": float(t.get("volume", 0.0)),
            "amount": float(t.get("amount", 0.0)),
        }
    return out


def _to_yyyymmdd(t) -> str:
    """xtdata time 字段（ms 时间戳 / yyyymmdd / yyyymmddHHMMSS）→ yyyymmdd。"""
    s = str(int(t)) if not isinstance(t, str) else t
    if len(s) >= 8 and s[:8].isdigit() and s[:4].startswith(("19", "20")):
        return s[:8]
    # 毫秒时间戳
    try:
        import datetime
        dt = datetime.datetime.fromtimestamp(int(t) / 1000)
        return dt.strftime("%Y%m%d")
    except Exception:  # noqa: BLE001
        return s[:8]
```

- [ ] **Step 2: 在 server.py 注册 provider**

`python-runtime/scripts/server.py`，在 `register_provider` 区（约 94-98 行，`akshare_market` 之后）加入：

```python
    register_provider("xtdata", "xtdata")
```

- [ ] **Step 3: 验证 xtdata.py 语法**

Run: `./src-tauri/python-runtime/python/python.exe -c "import ast; ast.parse(open('python-runtime/scripts/providers/xtdata.py', encoding='utf-8').read()); print('OK')"`
Expected: 输出 `OK`。

- [ ] **Step 4: 验证 health 软降级（未装 xtquant 时返回 available=false 而非崩溃）**

Run:
```bash
./src-tauri/python-runtime/python/python.exe -c "import sys; sys.path.insert(0,'python-runtime/scripts/providers'); import xtdata; print(xtdata.health())"
```
Expected: 输出 `{'available': False, 'reason': '...'}`（未安装 xtquant），不抛异常退出。

- [ ] **Step 5: Commit**

```bash
git add python-runtime/scripts/providers/xtdata.py python-runtime/scripts/server.py
git commit -m "feat(invest): 新增 miniQMT xtdata provider（kline/realtime_quote/health，软依赖）"
```

---

## Task 7: InternationalClient 增加 xtdata RPC 包装

**Files:**
- Modify: `src-tauri/src/invest/international.rs`（在 AkShare 包装方法附近，约 213-244 行后）

**Interfaces:**
- Consumes: `InternationalClient::rpc_call`（现有私有方法，约 114 行）、Task 6 的 `xtdata.*` RPC method
- Produces:
  - `pub async fn fetch_xtdata_health(&self) -> Result<XtdataHealth, String>`
  - `pub async fn fetch_xtdata_kline(&self, symbol: &str, period: &str, count: u32) -> Result<Vec<XtdataKlineBar>, String>`
  - 新结构 `XtdataHealth { available: bool, reason: String }`、`XtdataKlineBar { trade_date, open, high, low, close, vol }`

- [ ] **Step 1: 新增 xtdata 响应结构**

`src-tauri/src/invest/international.rs`，在文件结构体定义区（如 `BondYield10y` 附近）新增：

```rust
/// miniQMT 客户端健康状态。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct XtdataHealth {
    pub available: bool,
    #[serde(default)]
    pub reason: String,
}

/// miniQMT 单根 K线（字段已由 xtdata.py 归一化为 tushare 风格）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct XtdataKlineBar {
    pub trade_date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub vol: f64,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct XtdataKlineResp {
    items: Vec<XtdataKlineBar>,
}
```

- [ ] **Step 2: 新增 RPC 包装方法**

在 `impl InternationalClient` 块内（AkShare 方法后）新增：

```rust
    // -- miniQMT provider (xtdata) --------------------------------------------

    /// 探测 miniQMT 客户端是否在线。不在线返回 available=false（不报错）。
    pub async fn fetch_xtdata_health(&self) -> Result<XtdataHealth, String> {
        self.rpc_call("xtdata.health", serde_json::json!({})).await
    }

    /// 获取 miniQMT 历史 K线。
    pub async fn fetch_xtdata_kline(
        &self,
        symbol: &str,
        period: &str,
        count: u32,
    ) -> Result<Vec<XtdataKlineBar>, String> {
        let resp: XtdataKlineResp = self
            .rpc_call(
                "xtdata.kline",
                serde_json::json!({ "symbol": symbol, "period": period, "count": count }),
            )
            .await?;
        Ok(resp.items)
    }
```

- [ ] **Step 3: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过。新方法暂未被调用，可能有 dead_code warning（Task 8 接入后消除，或暂加 `#[allow(dead_code)]`）。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/invest/international.rs
git commit -m "feat(invest): InternationalClient 新增 xtdata RPC 包装（health/kline）"
```

---

## Task 8: 设置项 invest_miniqmt_enabled

**Files:**
- Modify: `src-tauri/src/models.rs:379-382`（`User` 结构）
- Modify: `src-tauri/src/storage/settings.rs:574`（patch 注册）

**Interfaces:**
- Produces: `User.invest_miniqmt_enabled: bool`（默认 false），可通过设置 patch 读写

- [ ] **Step 1: User 结构新增字段**

`src-tauri/src/models.rs`，在 `tushare_proxy_url` 字段之后（约 379 行后）新增：

```rust
    /// 启用 miniQMT（xtdata）作为行情类数据的优先源。默认关闭。
    #[serde(default)]
    pub invest_miniqmt_enabled: bool,
```

- [ ] **Step 2: 默认值初始化**

`src-tauri/src/models.rs` 中 `User` 的 `Default`/构造区（与 `tushare_proxy_url: None` 并列，约 571 行），新增：

```rust
            invest_miniqmt_enabled: false,
```

> 注：若 `User` 用 `#[derive(Default)]` 而非手写默认值，则 `#[serde(default)]` 已足够，本步可跳过——执行时先确认 571 行附近是手写默认还是 derive。

- [ ] **Step 3: patch 注册**

`src-tauri/src/storage/settings.rs`，在 `tushare_proxy_url` 的 apply 行之后（约 574 行）新增：

```rust
    apply_bool_field(&mut all.user.invest_miniqmt_enabled, &patch, "invest_miniqmt_enabled");
```

- [ ] **Step 4: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models.rs src-tauri/src/storage/settings.rs
git commit -m "feat(invest): 新增 invest_miniqmt_enabled 设置项"
```

---

## Task 9: macro_refresh 接入编排层（北向/国债先行 + 命中源记录）

**Files:**
- Modify: `src-tauri/src/invest/macro_refresh.rs`（`MacroEntry` 类型、`refresh_macro_cache` 保存逻辑、`fetch_northbound`、`fetch_cgb_10y`）

**Interfaces:**
- Consumes: `data_source::{fetch_with_chain, SourceId, registry::{Category, chain_for}, validity::is_valid_number}`、`User.invest_miniqmt_enabled`
- Produces: `MacroEntry` 扩展为四元组 `(indicator, value, extra, source)`；`save_macro_cache` 传入真实命中源

**说明：** 本任务把"命中源记录"与"两个代表性指标（北向走 Capital 链、国债走 Macro 链）接入编排层"一起落地，作为编排层的首个真实接入与端到端验证。其余指标的接入留作后续增量（同模式）。

- [ ] **Step 1: 扩展 MacroEntry 为四元组并默认 source**

`src-tauri/src/invest/macro_refresh.rs:14`，将类型与保存逻辑改为携带 source。把：

```rust
type MacroEntry = (String, Option<f64>, Option<String>);
```

改为：

```rust
/// (indicator, value, extra_json, source)
type MacroEntry = (String, Option<f64>, Option<String>, &'static str);
```

- [ ] **Step 2: 更新 refresh_macro_cache 保存逻辑**

`src-tauri/src/invest/macro_refresh.rs:48-58`，将解构与保存改为：

```rust
            Ok(entries) => {
                for (indicator, value, extra, source) in entries {
                    if let Err(e) =
                        macro_cache::save_macro_cache(&indicator, value, extra.as_deref(), source)
                    {
                        log::warn!("macro_refresh: failed to save {indicator}: {e}");
                        fail_count += 1;
                    } else {
                        ok_count += 1;
                    }
                }
            }
```

- [ ] **Step 3: 现有所有 fetch_* 的返回值补 source 字段**

对未接入编排层的 `fetch_*`（sh_composite/margin/shibor/international/market_stats/two_market_volume/advance_decline 及各 fallback），其 `Ok(vec![(...)])` 的每个元组末尾补默认 source 字符串。例如 `fetch_shibor` 改为：

```rust
    Ok(vec![(
        "shibor_on".to_string(),
        Some(latest.on),
        Some(serde_json::json!({ /* 原有字段 */ }).to_string()),
        "tushare",   // 新增
    )])
```

- `fetch_sh_composite` 主体 `"tushare"`，`sh_composite_tencent_fallback` 内 `"tencent"`。
- `fetch_margin` → `"tushare"`。
- `fetch_shibor` → `"tushare"`。
- `fetch_international` 各条 → `"yahoo"`。
- `fetch_market_stats`/`fetch_advance_decline` → `"akshare"`。
- `fetch_two_market_volume` → `"tencent"`。
- `cgb_10y_akshare_fallback` → `"akshare"`。

> 这些保持各自原有单一源；source 字符串需与 `SourceId::as_str()` 输出一致（`"tushare"/"akshare"/"yahoo"`），新增 `"tencent"` 为非编排源的直接标识。

- [ ] **Step 4: fetch_northbound 接入编排层（Capital 链）**

将 Task 1 修复后的 `fetch_northbound` 改为通过编排层，签名增加 `miniqmt_on`（实际 Capital 链不受影响，但统一传参）。改为：

```rust
async fn fetch_northbound(
    client: TushareClient,
    start_date: String,
    end_date: String,
) -> MacroResult {
    use crate::invest::data_source::{fetch_with_chain, registry::{chain_for, Category}, validity::is_valid_number, SourceId};

    const MILLION_TO_YI: f64 = 100.0;

    let chain = chain_for(Category::Capital, false); // [Tushare, Akshare]
    let fetched = fetch_with_chain(
        &chain,
        |v: &Option<f64>| is_valid_number(v),
        |source| {
            let client = client.clone();
            let (sd, ed) = (start_date.clone(), end_date.clone());
            async move {
                match source {
                    SourceId::Tushare => {
                        let flows = client.moneyflow_hsgt(&sd, &ed).await?;
                        let latest = flows.iter().max_by_key(|f| &f.trade_date)
                            .ok_or("northbound: no data")?;
                        Ok(Some(latest.north_money / MILLION_TO_YI))
                    }
                    SourceId::Akshare => {
                        let intl = crate::invest::international::InternationalClient::from_settings();
                        let s = intl.fetch_akshare_north_money().await?;
                        Ok(Some(s))  // akshare 已是亿元口径
                    }
                    _ => Err("northbound: unsupported source".into()),
                }
            }
        },
    ).await?;

    Ok(vec![(
        "northbound_net".to_string(),
        fetched.value,
        Some(serde_json::json!({ "unit": "亿元" }).to_string()),
        fetched.source.as_str(),
    )])
}
```

> **依赖：** 需在 `international.rs` 新增 `fetch_akshare_north_money` 包装 `akshare_market.north_money_today`（或复用现有 `stock_hsgt_fund_flow_summary_em`）。若 AkShare provider 暂无此函数，本步 AkShare 分支可先返回 `Err("akshare northbound not implemented")`，使链等价于仅 tushare——降级路径预留但不强求第一期实现。**执行时确认 akshare_market.py 是否已有对应函数；没有则走 Err 预留。**

- [ ] **Step 5: fetch_cgb_10y 接入编排层（Macro 链）**

将现有 `fetch_cgb_10y`（tushare `cn_bond_yield` 已知 502，本就靠 AkShare）改为编排层 Macro 链。tushare 分支保留（未来代理可能恢复），失败/为 0 自动降级 AkShare：

```rust
async fn fetch_cgb_10y(
    client: TushareClient,
    start_date: String,
    end_date: String,
) -> MacroResult {
    use crate::invest::data_source::{fetch_with_chain, registry::{chain_for, Category}, validity::is_valid_number, SourceId};

    let chain = chain_for(Category::Macro, false); // [Tushare, Akshare]
    let fetched = fetch_with_chain(
        &chain,
        |v: &Option<f64>| is_valid_number(v),
        |source| {
            let client = client.clone();
            let (sd, ed) = (start_date.clone(), end_date.clone());
            async move {
                match source {
                    SourceId::Tushare => {
                        let rows = client.cn_bond_yield(&sd, &ed).await?;
                        let latest = rows.iter().max_by_key(|r| r.date.clone())
                            .ok_or("cgb_10y: no data")?;
                        Ok(Some(latest.yield_10y))
                    }
                    SourceId::Akshare => {
                        let intl = crate::invest::international::InternationalClient::from_settings();
                        let bond = intl.fetch_akshare_bond_yield().await?;
                        if bond.yield_10y <= 0.0 {
                            return Err("cgb_10y akshare: invalid".into());
                        }
                        Ok(Some(bond.yield_10y))
                    }
                    _ => Err("cgb_10y: unsupported source".into()),
                }
            }
        },
    ).await?;

    Ok(vec![(
        "cgb_10y".to_string(),
        fetched.value,
        Some(serde_json::json!({ "unit": "%" }).to_string()),
        fetched.source.as_str(),
    )])
}
```

> **依赖：** 需确认 `client.cn_bond_yield` 返回结构的字段名（`date`、`yield_10y` 等）。执行时读 `client.rs` 对应结构核对，调整字段访问。删除原 `cgb_10y_akshare_fallback` 独立函数（逻辑已并入 Akshare 分支）。

- [ ] **Step 6: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过。确认无未消除的 dead_code（`fetch_with_chain`/`chain_for` 现已被调用）。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/invest/macro_refresh.rs src-tauri/src/invest/international.rs
git commit -m "feat(invest): macro_refresh 接入编排层（北向/国债），记录真实命中源"
```

---

## Task 10: miniQMT 开关串联 + 行情类编排接入

**Files:**
- Modify: `src-tauri/src/invest/macro_refresh.rs`（`fetch_sh_composite` 接入 Quote 链 + 读取 miniqmt 开关）
- 参考: `src-tauri/src/storage/settings.rs`（读取 `invest_miniqmt_enabled`）

**Interfaces:**
- Consumes: `User.invest_miniqmt_enabled`、`InternationalClient::{fetch_xtdata_health, fetch_xtdata_kline}`、`Category::Quote`
- Produces: `fetch_sh_composite` 在 miniqmt_on 时优先尝试 miniQMT，health 不可用则降级

**说明：** 这是 miniQMT 开关的端到端串联。以上证指数 K线（`fetch_sh_composite`）作为行情类的代表接入 Quote 链。

- [ ] **Step 1: 读取 miniqmt 开关并传入 refresh_macro_cache**

`refresh_macro_cache` 开头读取设置：

```rust
    let miniqmt_on = crate::storage::settings::load()
        .map(|s| s.user.invest_miniqmt_enabled)
        .unwrap_or(false);
```

> **依赖：** 确认 `storage::settings` 读取整体设置的函数名（可能是 `load()`/`get_all()`/`read()`）。执行时读 `settings.rs` 公开函数核对。将 `miniqmt_on` 传入需要的 `fetch_*`。

- [ ] **Step 2: fetch_sh_composite 接入 Quote 链（保留 close + vol20 双指标）**

改造 `fetch_sh_composite`，签名加 `miniqmt_on: bool`，在 tasks 列表调用处传入。原函数产出两个指标（`sh_composite_close` + `sh_composite_vol20`），降级源是**腾讯**（非 AkShare）。编排闭包返回 `(close, Option<vol20>)` 元组以同时携带两值；判空只校验 close。函数体改为：

```rust
async fn fetch_sh_composite(
    client: TushareClient,
    start_date: String,
    end_date: String,
    miniqmt_on: bool,
) -> MacroResult {
    use crate::invest::data_source::{fetch_with_chain, registry::{chain_for, Category}, SourceId};

    let chain = chain_for(Category::Quote, miniqmt_on); // on: [MiniQmt,Tushare,Tencent] / off: [Tushare,Tencent]

    // 返回 (close, Option<vol20>)；判空只看 close 有效（!=0 且有限）。
    let fetched = fetch_with_chain(
        &chain,
        |(close, _): &(f64, Option<f64>)| *close != 0.0 && close.is_finite(),
        |source| {
            let client = client.clone();
            let (sd, ed) = (start_date.clone(), end_date.clone());
            async move {
                match source {
                    SourceId::MiniQmt => {
                        let intl = crate::invest::international::InternationalClient::from_settings();
                        let h = intl.fetch_xtdata_health().await?;
                        if !h.available {
                            return Err(format!("miniqmt offline: {}", h.reason));
                        }
                        let bars = intl.fetch_xtdata_kline("000001.SH", "1d", 25).await?;
                        if bars.is_empty() {
                            return Err("miniqmt: empty kline".into());
                        }
                        let closes: Vec<f64> = bars.iter().rev().take(21).map(|b| b.close).collect();
                        let vol20 = crate::tencent_quotes::compute_vol20(&closes);
                        let latest_close = bars.last().unwrap().close;
                        Ok((latest_close, vol20))
                    }
                    SourceId::Tushare => {
                        let bars = client.daily("000001.SH", &sd, &ed).await?;
                        if bars.is_empty() {
                            return Err("sh_composite: tushare empty".into());
                        }
                        let latest_close = bars[0].close;
                        let closes: Vec<f64> = bars.iter().take(21).map(|b| b.close).collect();
                        let vol20 = crate::tencent_quotes::compute_vol20(&closes);
                        Ok((latest_close, vol20))
                    }
                    SourceId::Tencent => {
                        let http = reqwest::Client::new();
                        let kline = crate::tencent_quotes::fetch_index_kline(&http, "sh000001", 25).await?;
                        Ok((kline.close, kline.vol20))
                    }
                    _ => Err("sh_composite: unsupported source".into()),
                }
            }
        },
    ).await?;

    let (close, vol20) = fetched.value;
    let source = fetched.source.as_str();
    let mut entries = vec![
        ("sh_composite_close".to_string(), Some(close), None, source),
    ];
    if let Some(v) = vol20 {
        entries.push(("sh_composite_vol20".to_string(), Some(v), None, source));
    }
    Ok(entries)
}
```

删除原 `sh_composite_tencent_fallback` 独立函数（逻辑已并入 Tencent 分支）。

> **依赖核对（执行时读源确认，调整字段访问）：**
> 1. `client.daily(ts_code, start, end)` 的真实签名与返回 bar 结构字段名（`close` 等），见 `client.rs`。原函数用 `bars[0].close`（tushare 日线降序，最新在前），保持不变。
> 2. `tencent_quotes::compute_vol20(&[f64]) -> Option<f64>` 与 `fetch_index_kline` 返回结构的 `close`/`vol20` 字段，见原 `sh_composite_tencent_fallback`（已确认存在）。
> 3. miniQMT 的 K线为升序（最新在末），故用 `.rev().take(21)` 取最近窗口；tushare 降序用 `.take(21)`。

- [ ] **Step 3: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过。

- [ ] **Step 4: 验证开关默认关闭时行为不变（手动核对源链）**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`（无运行时测试，靠逻辑核对）
确认 `miniqmt_on=false` 时 Quote 链为 `[Tushare, Akshare]`，与改造前行为一致。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/macro_refresh.rs
git commit -m "feat(invest): 串联 miniQMT 开关，sh_composite 行情接入 Quote 编排链"
```

---

## Task 11: 设置项 UI 控件 + i18n 文案

**Files:**
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`
- Modify: `src/routes/settings/+page.svelte`（tushare proxy 设置项附近，约 1988-2008 行）

**Interfaces:**
- Consumes: `settings.invest_miniqmt_enabled`（Task 8 字段）、现有 `saveGeneralPatch`、`t()` i18n 函数
- Produces: 设置面板中可切换 miniQMT 开关，写入 `invest_miniqmt_enabled`

> i18n 键命名跟随现有风格：`settings_tushare_proxy_url` 用下划线 snake_case（非驼峰），新键照此模式。

- [ ] **Step 1: en.json 新增文案**

`messages/en.json`，在 `settings_tushare_proxy_url`/`settings_tushare_proxy_hint` 附近新增：

```json
"settings_invest_miniqmt_enabled": "Enable miniQMT as priority market data source",
"settings_invest_miniqmt_hint": "When on, quote data (K-line, real-time) is fetched from miniQMT first, falling back to Tushare then Tencent. Requires a running miniQMT client logged into a broker account."
```

- [ ] **Step 2: zh-CN.json 新增对应文案**

`messages/zh-CN.json`，同位置新增：

```json
"settings_invest_miniqmt_enabled": "启用 miniQMT 作为行情优先数据源",
"settings_invest_miniqmt_hint": "开启后，行情数据（K线、实时报价）优先从 miniQMT 获取，取不到时降级 Tushare 再降级腾讯。需本机运行已登录券商账户的 miniQMT 客户端。"
```

- [ ] **Step 3: 在设置面板新增开关控件**

`src/routes/settings/+page.svelte`，在 tushare proxy 设置项的 `</div>` 之后（约 2008 行，`</Card>` 之前）新增一个开关行，沿用同区块的 `flex items-center justify-between` 布局与 `saveGeneralPatch` 模式：

```svelte
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm font-medium">{t("settings_invest_miniqmt_enabled")}</p>
              <p class="text-xs text-muted-foreground">{t("settings_invest_miniqmt_hint")}</p>
            </div>
            <input
              type="checkbox"
              class="rounded"
              checked={settings?.invest_miniqmt_enabled ?? false}
              onchange={async (e) => {
                const val = (e.currentTarget as HTMLInputElement).checked;
                await saveGeneralPatch({ invest_miniqmt_enabled: val });
              }}
            />
          </div>
```

> 执行时确认：`settings` 对象上的字段访问名为 snake_case `invest_miniqmt_enabled`（与后端 serde 一致，后端 `User` 未加 `rename_all = "camelCase"`，前端类型若由后端生成则为 snake_case；若前端有独立的 camelCase 类型映射，则用 `investMiniqmtEnabled`——读 `saveGeneralPatch` 现有 `tushare_proxy_url` 用法，它是 snake_case，照此即可）。

- [ ] **Step 4: 验证 i18n 一致性与前端构建**

Run:
```bash
npm run i18n:check
npm run build
```
Expected: i18n 键集一致；构建成功。

- [ ] **Step 5: Commit**

```bash
git add messages/en.json messages/zh-CN.json src/routes/settings/+page.svelte
git commit -m "feat(invest): 设置面板新增 miniQMT 开关 + i18n 文案"
```

---

## Task 12: 全量验证

**Files:** 无（验证任务）

- [ ] **Step 1: Rust 编译与 clippy**

Run:
```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```
Expected: 全部通过，无 error/warning。

- [ ] **Step 2: i18n 检查**

Run: `npm run i18n:check`
Expected: 通过。

- [ ] **Step 3: 前端构建（确认无破坏）**

Run: `npm run build`
Expected: 构建成功。

- [ ] **Step 4: Python provider 烟测**

Run:
```bash
./src-tauri/python-runtime/python/python.exe -c "import sys; sys.path.insert(0,'python-runtime/scripts/providers'); import xtdata; print(xtdata.health())"
```
Expected: `{'available': False, ...}`（无 xtquant 环境），不崩溃。

- [ ] **Step 5: 最终提交（若有 fmt 修正等）**

```bash
git add -A
git commit -m "chore(invest): 数据源编排层 + miniQMT 接入 全量验证修正"
```

---

## 自检对照（spec 覆盖）

- 编排层三文件（mod/registry/validity）→ Task 2/3/4/5 ✓
- tushare 优先、判空降级 → `is_valid_number` + `fetch_with_chain` + `chain_for`，Task 9/10 接入 ✓
- miniQMT 开关 → Task 8（设置项）+ Task 10（串联）✓
- miniQMT provider 软依赖 → Task 6 ✓
- 北向修复 + 单位 ÷100 + "净流入"口径 → Task 1 + Task 9 ✓
- macro_cache 记真实命中源 → Task 9 ✓
- 第一期仅行情类、个股级不纳入 → 范围限定，Task 9/10 仅接 4 个代表指标 ✓
- Yahoo 保持原样 → Task 9 Step 3 标 `"yahoo"`，不改逻辑 ✓
- i18n + 设置面板开关控件 → Task 11 ✓
- 验证（cargo check 为主）→ Task 12 ✓
