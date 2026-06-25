# 数据源编排层 + miniQMT 接入 设计文档

- **日期**：2026-06-25
- **版本基线**：v5.5.11
- **范围**：openInvest 取数子系统（`src-tauri/src/invest/`）
- **状态**：设计已确认，待写实现计划

---

## 1. 背景与动机

openInvest 当前取数分散在三条腿：

1. **tushare**（`tushare/client.rs`，Rust HTTP，走自定义代理 `tushare_proxy_url`）
2. **AkShare / EastMoney / Jinshi / Yahoo**（Python RPC，经 `python/bridge.rs`）
3. **腾讯行情**（`tencent_quotes.rs`）

存在的问题：

- **取数源硬编码、降级散乱。** 每个 `macro_refresh.rs::fetch_*` 直连单一源，只有国债一项手写了 AkShare fallback。优先级和判空逻辑无法集中管理或测试。
- **北向资金永远为 0（确诊 BUG）。** tushare `moneyflow_hsgt` 的 `net_money` 字段在 2024 年 8 月交易所停更后已废弃，现行接口不再返回该字段；代码 `macro_refresh.rs:143` 取 `latest.net_money`，解析 `.unwrap_or_default()` → 永远 `0.0`。实测代理仍返回 `north_money`（=`hgt`+`sgt`，自洽）。
- **`macro_cache.source` 列写死 `scheduler`**，无法反映真实命中源。
- **miniQMT（xtdata）未接入。** 用户希望对有券商账户的场景，行情类数据可优先走 miniQMT 本地行情库（无限频、本地缓存、Level2）。

### 取数源能力实测（走生产代理 + AkShare，2026-06-24 数据）

| 指标 | tushare（代理） | AkShare | 结论 |
|------|------|------|------|
| 北向 `northbound_net` | 接口有数（`north_money`=424355.82 百万元），但代码取废弃的 `net_money` → 0 | `stock_hsgt_fund_flow_summary_em` 当日实时 | 修字段后 tushare 可用 |
| 融资融券 `margin_balance` | `margin_detail` 个股级 6000 行 | `stock_margin_sse` 仅市场汇总 | tushare 更全 |
| shibor `shibor_on` | 1.408（全期限） | `rate_interbank` 1.408（一致） | 两者一致 |
| 国债 10Y `cgb_10y` | 接口 `cn_bond_yield` 不存在（代理 502） | `bond_zh_us_rate` 1.7454 | AkShare 为唯一可用源 |

---

## 2. 目标与非目标

### 目标

1. 新增统一的**数据源编排层** `invest/data_source/`，集中管理"源链优先级 + 判空降级 + 命中源记录"。
2. 默认优先级：**tushare 优先；实测取不到或返回 0/无效 → 降级 AkShare**。
3. 新增 **miniQMT 开关**：开启时，行情类数据取数优先链为 `miniQMT → tushare → AkShare`；客户端离线时无缝降级。
4. **修复北向资金 BUG**：改用 `north_money`，并修正单位/语义标注。
5. `macro_cache.source` 记录真实命中源。

### 非目标（YAGNI / 第二期）

- **不**用 miniQMT 财务六表（`get_financial_data`）替代 tushare `fina_indicator`——miniQMT 给原始报表，比率指标需自行加工，tushare 已做好。
- **不**接 miniQMT 板块成分（`get_sector_list`/`get_stock_list_in_sector`）——当前行业字段够用。
- **不**接 Level2 逐笔。
- **不**把个股级数据（`daily_basic`/`fina_indicator`/`moneyflow_dc`/`report_rc`）纳入编排层——它们是 tushare 独有，无降级意义。
- **不**改动 Yahoo 海外指标（VIX/美债/黄金/原油/汇率）取数。

> 以上第二期项在编排层注册表中天然可扩展：行情类跑通并确认 miniQMT 在目标机器稳定后，按需新增指标类别即可。

---

## 3. 整体架构

```
调用方 (macro_refresh / committee / event_scanner …)
        │  按"指标"请求，不再直接 new TushareClient
        ▼
┌─────────────────────────────────────────────┐
│  invest/data_source/  (编排层)                │
│   • registry: 每个指标登记 [源链] + 判空函数   │
│   • orchestrator: 按链尝试→判空→降级→记命中源  │
│   • miniqmt 开关注入(行情类源链头部)          │
└───────┬───────────────┬───────────────┬───────┘
        ▼               ▼               ▼
   TushareClient    Python RPC      Python RPC
   (Rust HTTP,代理)  akshare_*       xtdata (新增)
```

### 边界约定

- 编排层**只管"选源 + 降级 + 记录"**，不碰各源内部字段解析（解析仍在 `client.rs` / 各 `.py` provider）。
- `macro_refresh.rs` 的 `fetch_*` 从"直连单源"改为"向编排层请求某指标"，降级逻辑从函数体移到注册表。
- miniQMT 作为新 provider 挂到现有 `python/bridge.rs`（与 AkShare 同一条 RPC 腿），**不引入新进程模型**。

---

## 4. 编排层内部结构

模块 `invest/data_source/` 由三个单一职责文件组成。

### 4.1 `mod.rs` — 类型与编排器

```rust
/// 数据源标识
pub enum SourceId { MiniQmt, Tushare, Akshare, Yahoo }

/// 一次取数的结果，带命中源信息（供 macro_cache.source 记录）
pub struct Fetched<T> {
    pub value: T,
    pub source: SourceId,
}

/// 核心编排：按源链依次尝试，判空则降级
/// - chain: 有序源链
/// - is_valid: 判空函数（取不到/为0/NaN → false → 降级）
/// - try_source: 对某个源发起取数
pub async fn fetch_with_chain<T>(
    chain: &[SourceId],
    is_valid: impl Fn(&T) -> bool,
    try_source: impl Fn(SourceId) -> /* Future<Output = Result<T, String>> */,
) -> Result<Fetched<T>, String>
```

行为：遍历 `chain`，对每个源调 `try_source`；成功且 `is_valid` 为真则返回 `Fetched{value, source}`；否则（Err 或无效）记 debug 日志并尝试下一个源；全链耗尽返回 `Err`。

### 4.2 `registry.rs` — 指标→源链登记表

```rust
pub enum Category { Quote, Capital, Macro, TushareOnly, Overseas }

pub fn chain_for(category: Category, miniqmt_on: bool) -> Vec<SourceId> {
    match category {
        Category::Quote   => if miniqmt_on { vec![MiniQmt, Tushare, Akshare] }
                             else          { vec![Tushare, Akshare] },
        Category::Capital => vec![Tushare, Akshare],   // 北向 / 两融
        Category::Macro   => vec![Tushare, Akshare],   // shibor / 国债
        Category::TushareOnly => vec![Tushare],        // moneyflow_dc / report_rc
        Category::Overseas    => vec![Yahoo],          // VIX / 美债 …保持原样
    }
}
```

注册表是**唯一**决定优先级的地方——调整顺序 / 加源只改这里。`miniqmt_on` 由设置项注入。

### 4.3 `validity.rs` — 判空规则

用户要求"实测取不到或为 0 则降级"的落地点。

```rust
/// 默认：None / Err / 0.0 / 非有限 视为无效 → 触发降级
pub fn is_valid_number(v: &Option<f64>) -> bool {
    matches!(v, Some(x) if *x != 0.0 && x.is_finite())
}
```

判空函数按指标可定制（如海外指标若允许 0，可传不同规则）。源链长度为 1 的指标（Yahoo、tushare 独有）复用同一编排器，等价于"不降级"，无特例代码。

---

## 5. miniQMT provider 接入

### 5.1 新增 `python-runtime/scripts/providers/xtdata.py`

第一期仅行情类函数：

```python
def kline(symbol, period="1d", count=25) -> dict      # 包装 get_market_data_ex
def realtime_quote(symbols) -> dict                   # 包装 get_full_tick
def health() -> dict   # 探测 QMT 客户端是否在线 → {"available": bool, "reason": str}
```

注册（`server.py`，与 `akshare_market` 并列）：

```python
register_provider("xtdata", "xtdata")
```

调用走现有 `method = "xtdata.<func>"` 路由，**无需改 `bridge.rs` 进程模型**。

### 5.2 关键设计点

1. **软依赖。** `import xtquant.xtdata` 用 lazy import + try/except 包裹（沿用现有 provider 的 `LazySession` 风格）。未装 xtquant 或客户端未启动时，`health()` 返回 `{"available": false, ...}`，不崩溃。
2. **健康探测决定开关生效。** 编排层在 miniQMT 开关打开时先看 health；客户端离线则该次取数直接跳到链中下一个源（tushare），用户无感。
3. **格式归一化。** xtdata 返回 QMT 自有结构（`{symbol: DataFrame}`），`xtdata.py` 负责转成与 tushare K线**相同的 JSON 字段名**（`trade_date/open/high/low/close/vol`），上层无感知。
4. **符号格式。** 库内用 tushare 风格（`600000.SH`），xtdata 同用 `.SH/.SZ` 后缀；ETF/指数差异在 `xtdata.py` 内部处理。

### 5.3 设置项

新增 `settings.json` 字段（user 作用域）：`invest_miniqmt_enabled: bool`（默认 `false`）。注入编排层 `chain_for(_, miniqmt_on)`。

---

## 6. 北向资金修复

### 根因

tushare `moneyflow_hsgt` 的 `net_money` 字段已于 2024-08 停更（交易所停止披露），现行接口字段为 `trade_date, ggt_ss, ggt_sz, hgt, sgt, north_money, south_money`，**无 `net_money`**。代码取该字段 → 永远 0。

### 修复

1. **解析**（`client.rs::moneyflow_hsgt`）：`net_money` 字段缺失时，以 `north_money`（=`hgt`+`sgt`，实测自洽）作为北向值。保留向后兼容：若未来字段恢复则仍可用。
2. **取数**（`macro_refresh.rs::fetch_northbound`）：北向值改用 `north_money`。
3. **单位换算（关键）。** tushare `moneyflow_hsgt` 金额字段单位为**百万元**（官方文档确认）。换算到亿元：**÷ 100**（1 亿元 = 100 百万元）。
   - 验证：`424355.82` 百万元 ÷ 100 = `4243.56` 亿元，量级合理。
   - 换算属于北向指标的解析职责（非编排层），系数在 `macro_refresh::fetch_northbound` 定义为带注释的常量，避免单位回归。与第 3 节边界一致：编排层只管选源/降级，不碰字段解析与单位换算。
4. **语义标注。** `north_money` 为累计/余额口径，而下游 `committee/tools.rs:28` 现标注"北向资金净流入(亿)"。修正为与实际口径一致的标注（具体口径文案在写实现计划时定稿）。

---

## 7. 数据流

```
macro_refresh 并发跑各指标
   → 每指标向编排层 fetch_with_chain 请求
   → 编排层按 registry 源链尝试 + validity 判空
   → 第一个有效结果 → Fetched{value, source}
   → 写入 macro_cache，source 列记录真实命中源（修正写死 scheduler）
```

---

## 8. 错误处理

- **单指标全链失败**：沿用现有"保留旧缓存"策略（`macro_refresh.rs` 已具备），不覆盖、记 warn。
- **miniQMT 客户端离线**：health 探测失败 → 跳过该源，链继续，用户无感。
- **单源异常**：被编排器捕获为"该源无效"→ 降级，不向上抛断整个刷新。

---

## 9. 测试

| 单元 | 测试内容 | 依赖 |
|------|------|------|
| `validity.rs` | 判空函数（0/None/NaN/正常值） | 纯逻辑，无网络 |
| `registry.rs` | `chain_for` 在 miniqmt_on true/false 下的链顺序 | 纯逻辑 |
| `orchestrator` | mock 闭包：首源失败→降级→次源成功 / 全失败返回 Err | 纯逻辑 |
| 北向解析 | 实测 JSON 样本（无 net_money）回退到 north_money + 单位换算 | 纯逻辑 |

受 CLAUDE.md §11 Rust 测试运行时限制（`STATUS_ENTRYPOINT_NOT_FOUND`），以 `cargo check` 验证编译 + 上述纯逻辑单测为主，避免依赖网络/真实客户端的集成测试进 CI。

---

## 10. 影响文件清单

| 文件 | 改动 |
|------|------|
| `src-tauri/src/invest/data_source/mod.rs` | 新建：类型 + 编排器 |
| `src-tauri/src/invest/data_source/registry.rs` | 新建：源链登记表 |
| `src-tauri/src/invest/data_source/validity.rs` | 新建：判空函数 |
| `src-tauri/src/invest/mod.rs` | 注册新模块 |
| `src-tauri/src/invest/macro_refresh.rs` | 改造 `fetch_*` 走编排层；北向修复；单位换算常量 |
| `src-tauri/src/tushare/client.rs` | `moneyflow_hsgt` 解析回退 north_money |
| `src-tauri/src/invest/committee/tools.rs` | 北向口径标注修正 |
| `src-tauri/src/storage/invest/macro_cache.rs` | source 列写真实命中源 |
| `python-runtime/scripts/providers/xtdata.py` | 新建：miniQMT provider（kline/realtime_quote/health） |
| `python-runtime/scripts/server.py` | 注册 xtdata provider |
| `settings`（user 作用域） | 新增 `invest_miniqmt_enabled` |
| `messages/en.json` + `zh-CN.json` | miniQMT 开关 UI 文案 |

---

## 11. 分期

- **第一期（本设计）**：编排层 + 优先级降级 + 北向修复 + miniQMT 行情类接入 + 开关。
- **第二期（按需）**：miniQMT 财务六表 / 板块成分 / Level2，纳入编排层注册表新增类别。
