# openInvest Phase 5: 委员会 LLM 工具 + Prompt 全面升级

## Context

当前 ClawGO invest 委员会存在三个核心问题：
1. `exec_macro_snapshot` 是半成品（只有沪深300，其余 TODO）
2. Macro prompt 引用的工具名（`get_market_overview` 等）不存在于 tools.rs
3. 只有 Macro 角色有工具访问权，Quant/Risk 无法主动查数据

参照 openInvest 的成熟设计，本次升级覆盖：角色精简（去掉 Wealth，R1/R2 改为同一角色的轮次）、宏观数据缓存层、工具分角色开放、REGIME 约束、反幻觉规则。

### Review 决策记录（10 项设计缺口确认）

| # | 问题 | 决策 |
|---|------|------|
| 1 | REGIME 注入时机 | Macro 完成后、Quant R1 之前计算 REGIME，格式化注入 Quant R1 的 user message |
| 2 | Portfolio 数据注入 Risk R1 | 在 Risk R1 之前从 invest.db 查询持仓数据，格式化为 portfolio_summary 注入 Risk 的 user message |
| 3 | `get_history_data` Yahoo 符号 | `exec_history_data` 自动判断：Tushare 格式（`XXXXXX.SH`）走 Tushare，其他（如 `^VIX`）走 Yahoo Finance |
| 4 | `analyze_multi_timeframe` 增强 | 增加 RSI(14) Wilder 计算、价格分位数（2年窗口）、MA60→MA120 升级 |
| 5 | Prompt 占位符渲染 | `load_prompt_for_round(role, round, asset_name, asset_symbol)` 内部做 `str::replace` |
| 6 | CIO prompt "三方" | 改为"刚听完所有前序分析报告"，明确列出 5 个输出来源 |
| 7 | Convergence/Sentinel 数据源 | 保持内存对比（同次运行 R1 vs R2），round_cache 仅用于跨次查询工具 |
| 8 | Risk R1 引用 WealthContextOfficer | 删除 SOLVENCY_BUFFER_LEVEL 引用，改为直接从 portfolio data 中 cash 比例判断流动性 |
| 9 | `debate_rounds` 配置 | 保留配置。默认模式固定 6 步；回放模式支持 1/2/3/4/6/8 轮辩论（R2 prompt 重复使用，累积上下文） |
| 10 | R2 工具分配 | R1 有工具（主动查数据），R2 无工具（上下文已足够做 cross-challenge） |

---

## 管线结构（目标）

```
Step 1: Macro   → 宏观共享 prompt（有工具）
NEW:    REGIME  → 计算 REGIME（调用 regime::compute_regime_for_symbol）
NEW:    Portfolio → 查询 portfolio data（集中度/子弹/浮盈）
Step 2: Quant R1 → 注入 REGIME + Macro 输出 + portfolio → 有工具 → 结果写入 round_cache
Step 3: Risk R1  → 注入 Macro+Q1 + portfolio_summary → 有工具 → 结果写入 round_cache
Step 4: Quant R2 → 注入 Macro+Q1+R1 → 无工具 → cross-challenge
Step 5: Risk R2  → 注入 Macro+Q1+R1+Q2 → 无工具 → cross-challenge
Step 6: CIO      → 注入全部 5 个前序输出 → 无工具 → 最终裁决
```

**4 个角色类型，6 个步骤**。Quant 和 Risk 各跑 2 轮（R1 独立陈述 + R2 cross-challenge）。

**`debate_rounds` 语义**：
- 默认模式（CLI/API）：固定 6 步，`debate_rounds` 忽略
- 回放模式（replay）：`debate_rounds` 控制额外轮数，R2 prompt 重复使用，累积上下文
- 收敛检测在 round 2 后触发，提前退出
- 前端 PipelineFlow 动态渲染步骤数

**REGIME 注入**：Macro 完成后立即计算 REGIME（MA20/MA60/RSI14/波动率），格式化为 REGIME context block 注入 Quant R1 的 user message。

**Portfolio 注入**：Risk R1 之前从 invest.db 查询持仓数据，格式化为 portfolio_summary 注入 Risk 的 user message（集中度、子弹、浮盈、压力测试）。

---

## Phase 0: 角色精简 — 去掉 Wealth，R1/R2 改为轮次

### 0.1 Enum 重构

**文件**：`src-tauri/src/invest/committee/roles.rs`

```rust
// Before: 7 variants
pub enum CommitteeRole { Macro, QuantR1, RiskR1, Wealth, QuantR2, RiskR2, Cio }

// After: 4 variants
pub enum CommitteeRole { Macro, Quant, Risk, Cio }
```

同步修改所有 match arms：
- `label()`: Quant→"量化分析师", Risk→"风控官"
- `prompt_filename()`: Quant→`quant.txt`, Risk→`risk.txt`（不再有 r1/r2 区分）
- `default_prompt()`: 删除 Wealth/R2 分支，每个角色只有一个 prompt
- `max_chars()`: 简化为 4 个分支（Macro 400, Quant 250, Risk 250, CIO 400）
- 测试：role_all_count 从 7→4

### 0.2 轮次 Prompt 机制

**文件**：`src-tauri/src/invest/committee/roles.rs`

新增概念：`Round`（R1 / R2），用于 prompt 加载和 context 构建：

```rust
pub enum Round { R1, R2 }

impl CommitteeRole {
    /// 根据角色+轮次返回 prompt 文件名
    pub fn prompt_filename_for_round(&self, round: Round) -> &'static str {
        match (self, round) {
            (Self::Macro, _) => "macro.txt",
            (Self::Quant, Round::R1) => "quant.txt",        // R1 独立陈述
            (Self::Quant, Round::R2) => "quant_r2.txt",     // R2 cross-challenge
            (Self::Risk, Round::R1) => "risk.txt",           // R1 独立陈述
            (Self::Risk, Round::R2) => "risk_r2.txt",        // R2 cross-challenge
            (Self::Cio, _) => "cio.txt",
        }
    }
}
```

`load_prompt(role)` 改为 `load_prompt_for_round(role, round, asset_name, asset_symbol)`，自动选择 R1 或 R2 prompt 文件，并做占位符替换：

```rust
pub fn load_prompt_for_round(
    role: CommitteeRole,
    round: Round,
    asset_name: &str,
    asset_symbol: &str,
) -> String {
    let filename = role.prompt_filename_for_round(round);
    let path = get_prompt_dir().join(filename);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| role.default_prompt_for_round(round).to_string());
    raw.replace("{{asset_name}}", asset_name)
       .replace("{{asset_symbol}}", asset_symbol)
}
```

### 0.3 Orchestrator 重构

**文件**：`src-tauri/src/invest/committee/orchestrator.rs`

管线从 7 步改为 6 步，去掉 Wealth 独立调用：

```rust
// 6-step pipeline with REGIME + portfolio injection
let macro_out = run_macro_phase(client, symbol, config).await;           // Step 1

// NEW: Compute REGIME after Macro
let regime = compute_regime_for_symbol(client, symbol).await;
let regime_context = format_regime_context(&regime);

// NEW: Query portfolio data for Risk R1
let portfolio_data = query_portfolio_summary(symbol).await;

let q1 = run_role_phase(client, symbol, Quant, Round::R1, [macro, regime], tools=quant_tools); // Step 2
let r1 = run_role_phase(client, symbol, Risk,  Round::R1, [macro, q1, portfolio], tools=risk_tools); // Step 3
let q2 = run_role_phase(client, symbol, Quant, Round::R2, [macro, q1, r1], tools=None); // Step 4
let r2 = run_role_phase(client, symbol, Risk,  Round::R2, [macro, q1, r1, q2], tools=None); // Step 5
let cio = run_role_phase(client, symbol, Cio,  Round::R1, [macro, q1, r1, q2, r2], tools=None); // Step 6
```

**round_cache 读写逻辑**（同次运行内，用于 convergence/sentinel 内存对比）：
- Step 2 (Quant R1) 完成后：`save_round_cache(symbol, "quant_r1", output)`
- Step 3 (Risk R1) 完成后：`save_round_cache(symbol, "risk_r1", output)`
- Step 4 (Quant R2) 开始前：`load_round_cache(symbol, "quant_r1")` → 注入到 Quant R2 的 context（用于对比一致性）
- Step 5 (Risk R2) 开始前：`load_round_cache(symbol, "risk_r1")` → 注入到 Risk R2 的 context（用于对比浓度漂移）

**build_context_messages 改造**：
- Quant R1 的 context 中包含 REGIME context block + Macro 输出
- Risk R1 的 context 中包含 portfolio_summary + Macro + Q1 输出
- Quant R2 的 context 中包含 Q1 的缓存输出（用于对比一致性）+ REGIME
- Risk R2 的 context 中包含 R1 的缓存输出 + Q2 输出
- CIO 的 context 中包含所有 5 个前序输出

**debate_rounds 回放模式**：
- 默认（debate_rounds=2）：固定 6 步管线
- 回放模式（debate_rounds=N）：N>2 时，Step 4/5 循环 N-1 次（R2 prompt 重复使用，累积上下文），每轮后检查收敛

### 0.4 Events 重编号

**文件**：`src-tauri/src/invest/committee/events.rs`

```
Macro => 0, Quant(R1) => 1, Risk(R1) => 2, Quant(R2) => 3, Risk(R2) => 4, Cio => 5
```

删除 Wealth 条目。

### 0.5 Parser 精简

**文件**：`src-tauri/src/invest/committee/parser.rs`

- 从 `ParsedFields` 删除 `wealth_context` 和 `solvency_buffer_level`
- 删除 `parse_wealth()` 和 `parse_role_output` 中的 Wealth 分支
- 简化 Quant/Risk match arms（不再有 QuantR1/QuantR2 区分，统一为 Quant）
- 删除 `test_parse_wealth`

### 0.6 round_cache 存储层

**新建文件**：`src-tauri/src/storage/invest/round_cache.rs`
**修改文件**：`src-tauri/src/storage/invest/mod.rs`

```sql
CREATE TABLE IF NOT EXISTS round_cache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    symbol TEXT NOT NULL,
    round_key TEXT NOT NULL,     -- "quant_r1" | "risk_r1"
    raw_text TEXT NOT NULL,
    parsed_json TEXT,            -- 序列化的 ParsedFields
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_round_cache_symbol ON round_cache(symbol, round_key, created_at);
```

**API**：
- `save_round_cache(conn, symbol, round_key, raw_text, parsed_json)` — 追加写入
- `load_latest_round_cache(conn, symbol, round_key) -> Option<RoundCacheEntry>` — 读最新一条（用于 `get_recent_committee_verdicts` 工具的跨次查询）
- `load_round_cache_for_date(conn, symbol, round_key, date) -> Option<RoundCacheEntry>` — 读指定日期

### 0.7 Analysis 更新

**文件**：`src-tauri/src/invest/committee/analysis.rs`

- `check_convergence`：保持内存对比（同次运行 Q1 vs Q2），round_cache 仅用于跨次查询工具
- `check_sentinel`：保持内存对比（同次运行 R1 vs R2），round_cache 仅用于跨次查询工具
- 新增 Gate 4（CIO）：仓位=0 且 verdict=HOLD → 降 confidence（现金机会成本规则）

### 0.8 Archive 测试更新

**文件**：`src-tauri/src/invest/committee/archive.rs`

- 测试 helper 中删除 `wealth_context: None` 和 `solvency_buffer_level: None`

### 0.9 Tauri Command

**文件**：`src-tauri/src/commands/invest.rs`

- `save_role_prompt`: 删除 `"wealth"`, `"quant_r1"`, `"quant_r2"`, `"risk_r1"`, `"risk_r2"` 分支，改为 `"quant"`, `"risk"`
- `get_role_prompts`: 自然产出 4 个条目

### 0.10 前端

**文件**及改动：
- `PipelineFlow.svelte`: 从 7→6 节点，删除 wealth
- `CommitteeLiveTab.svelte`: `PIPELINE_STEPS` 从 7→6
- `DebateBlock.svelte`: 删除 `wealth` ROLE_COLORS
- `CommitteeRolesTab.svelte`: 删除 Wealth 卡片，Quant/Risk 卡片各保留一个 prompt 编辑器（或改为 R1/R2 双编辑器）

### 0.11 i18n

**文件**：`messages/en.json`, `messages/zh-CN.json`

删除 `invest_pipeline_wealth`, `invest_roles_wealth_cn`, `invest_roles_wealth_desc`。
更新 `settings_profile_*_desc` 中对 Wealth 的引用。

---

## Phase 1: Regime 计算模块提取

**新建文件**：`src-tauri/src/invest/regime.rs`
**修改文件**：
- `src-tauri/src/invest/mod.rs` — 添加 `pub mod regime;`
- `src-tauri/src/commands/invest.rs` — `get_regime_classification` 改为调用 `invest::regime::`
- `src-tauri/src/invest/committee/orchestrator.rs` — 在 Step 1 (Macro) 完成后、Step 2 (Quant R1) 之前调用 `compute_regime_for_symbol()`

**关键逻辑**（已有，需提取 + 补 RSI-14）：
- MA20/MA60 趋势判断 → uptrend/downtrend/range_bound/crash/unknown
- 新增 RSI-14 计算
- 返回 `(regime, brief, metrics_hashmap)`

**REGIME 注入到 Quant R1**：
```rust
// 在 orchestrator::run_committee() 中，Macro 完成后：
let regime_result = compute_regime_for_symbol(client, symbol).await;
let regime_context = format!(
    "REGIME: {}\nREASON: {}\nINPUTS: {}\nSTRATEGY_HINT: {}",
    regime_result.regime, regime_result.reason,
    regime_result.inputs, regime_result.strategy_hint
);
// regime_context 注入到 Quant R1 的 user message
```

---

## Phase 2: Tushare A 股宏观数据接口

**修改文件**：`src-tauri/src/tushare/client.rs`

新增 4 个方法（复用 `call_api()` 模式）：

| 方法 | Tushare API | 返回字段 |
|------|------------|---------|
| `moneyflow_hsgt(start, end)` | `moneyflow_hsgt` | trade_date, north_money, south_money, net_money |
| `margin_detail(start, end)` | `margin_detail` | trade_date, rzye, rzmre, rzche |
| `shibor(start, end)` | `shibor` | date, on, w1, m1, m3 |
| `cn_bond_yield(start, end)` | `cn_bond_yield` | ts_code, yield_10y |

---

## Phase 3: 国际指标 HTTP 客户端 (Yahoo Finance)

**新建文件**：`src-tauri/src/invest/international.rs`

用现有 `reqwest` 调 Yahoo Finance v8 API：

| 指标 | Yahoo Symbol | 用途 |
|------|-------------|------|
| VIX | `^VIX` | 全球恐慌情绪 |
| 美10Y国债 | `^TNX` | 美联储利率风向 |
| 美元指数 | `DX-Y.NYB` | 影响黄金/汇率 |
| 国际金价 | `GC=F` | 避险资产基准 |
| 国际油价 | `CL=F` | 地缘冲突指标 |
| USDCNY | `USDCNY=X` | 汇率风险 |

**`exec_history_data` 复用**：`get_history_data` 工具的 Yahoo Finance fallback 也使用此模块的 HTTP 客户端，拉取任意 Yahoo 符号的历史日线数据。

---

## Phase 4: macro_cache 表 + 存储层

**新建文件**：`src-tauri/src/storage/invest/macro_cache.rs`
**修改文件**：`src-tauri/src/storage/invest/mod.rs`

```sql
CREATE TABLE IF NOT EXISTS macro_cache (
    indicator TEXT PRIMARY KEY,
    value REAL,
    extra_json TEXT,
    source TEXT NOT NULL,
    fetched_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

12 个 indicator 行：csi300_close, csi300_vol20, northbound_net, margin_balance, shibor_on, cgb_10y, vix, tnx, dxy, gold, oil, usdcny。

---

## Phase 5: 调度任务 + exec_macro_snapshot 重写

**新建文件**：`src-tauri/src/invest/macro_refresh.rs`
**修改文件**：
- `src-tauri/src/invest/scheduler/mod.rs` — 添加 `macro_refresh` job
- `src-tauri/src/invest/scheduler/runner.rs` — 添加 dispatch arm
- `src-tauri/src/invest/committee/tools.rs` — 重写 `exec_macro_snapshot()`

调度：`*/15 8-22 * * 1-5`（交易日每 15 分钟）。
工具从 macro_cache 表读取，缓存过期(>30min)时 fallback 实时拉取。

**`exec_history_data` 双数据源改造**：
- 判断符号格式：匹配 `^\d{6}\.(SH|SZ)$` 走 Tushare，其他（如 `^VIX`）走 Yahoo Finance
- Yahoo Finance fallback：复用 `invest/international.rs` 的 HTTP 客户端
- 返回格式统一为：最新收盘/区间涨跌/最高最低/近5日K线

**`exec_multi_timeframe` 增强**：
- 新增 RSI(14) Wilder 计算
- 新增价格分位数（2年窗口）
- MA60→MA120 升级（需要 250+ 交易日数据）
- 输出格式增加：RSI14、价位分位、MA120

---

## Phase 6: 工具分角色开放

**修改文件**：
- `src-tauri/src/invest/committee/tools.rs` — 新增 `role_tool_defs()`
- `src-tauri/src/invest/committee/orchestrator.rs` — 提取工具调用循环为共享函数

**工具分配（R1 有工具，R2 无工具）**：

| 角色 | R1 可用工具 | R2 可用工具 | 理由 |
|------|------------|------------|------|
| Macro | get_macro_snapshot, get_history_data | — | 宏观数据 |
| Quant | analyze_multi_timeframe, get_history_data, get_recent_committee_verdicts | 无 | R1 主动查数据，R2 cross-challenge 上下文已足够 |
| Risk | query_dreaming_insights, get_recent_committee_verdicts | 无 | R1 查行为模式，R2 cross-challenge 上下文已足够 |
| CIO | 无 | 无 | 决策角色，上下文已足够 |

提取 `run_with_tool_loop()` 共享函数，`run_role_phase` 接受可选 `tool_defs`。

---

## Phase 7: 全部 Prompt 替换

**修改文件**：`src-tauri/src/invest/committee/roles.rs`

以下 prompt 基于 `docs/superpowers/plans/ref/prompt.md` 参考文档，针对 ClawGO 实际工具和数据源修改。

### 7.1 Macro Prompt (MACRO_PROMPT)

基于参考文档 Macro 部分，修改工具引用为 ClawGO 实际工具：

```
你是一名全球宏观策略师，给整个投资组合提供宏观环境判断。
**只看宏观指标 + 政策 + 地缘**——不评论单一资产技术面、不评论用户持仓。

**你有工具可调用**：
- `get_macro_snapshot()` → 当前宏观指标快照：沪深300/北向资金/融资余额/Shibor/10Y国债/VIX/TNX/DXY/黄金/油价/USDCNY
- `get_history_data(symbol="000300.SH", days=90)` → 看沪深300趋势
- `get_history_data(symbol="^VIX", days=90)` → 看 VIX 趋势（恐慌情绪是否爬升）

**核心关注**：
1. A股流动性：沪深300 60日分位 / 北向资金流向 / 融资余额变化 / Shibor 隔夜
2. 利率与央行：10Y国债收益率走向 / 美联储利率 / TNX 走向
3. 汇率与大宗商品：USDCNY / 金价(避险) / 油价(地缘)
4. 地缘：战争 / 贸易制裁 / 供应链冲击

**严禁**：在最终输出里抱怨"工具不可用"或"未找到信息" — 用户只想看你的判断

**输出要求**：
- 必须中文回复
- 严格按下列格式，总长度 ≤150 字

SIGNAL: risk_on | risk_off | neutral
STRENGTH: 0-10
SCORE: -5 到 +5
KEY_HEADWIND: <一句话最大利空>
KEY_TAILWIND: <一句话最大利好>
ONE_LINER: <一句话宏观结论，明确给"加仓 / 减仓 / 维持"倾向>

**判定原则**：
- SCORE < -2: 强烈 risk_off，所有资产偏向减仓
- -2 ≤ SCORE ≤ 2: neutral
- SCORE > 2: risk_on，可加仓
```

### 7.2 Quant R1 Prompt (QUANT_PROMPT)

基于参考文档 Quant Round 1，修改工具引用和 REGIME 来源：

```
你是一名量化技术分析师，专注 {{asset_name}} ({{asset_symbol}})。
**只看技术面 / 价量 / 历史模式 + 市场 REGIME 上下文**——不评论宏观、不评论用户持仓。

**你将在 user message 中收到一段 REGIME 上下文**（由系统用确定性规则算出，不是你判断的），
格式如下：
REGIME: uptrend | downtrend | range_bound | crash | unknown
REASON: <为什么判这个 regime 的具体数据依据>
INPUTS: ma20=..., ma60=..., volatility_ann=..., rsi14=...
STRATEGY_HINT: <对应 regime 下的策略偏好>

**REGIME 是事实，不是你的判断**——你必须在它给定的方向偏好内出 SIGNAL。
具体约束:
  - REGIME=uptrend  → SIGNAL 不允许 bearish（顺势市不喊跌）
  - REGIME=downtrend → SIGNAL 不允许 bullish（下跌趋势不抄底）
  - REGIME=range_bound 且价格处于低位区间 → SIGNAL 偏向 bullish
  - REGIME=range_bound 且价格处于高位区间 → SIGNAL 偏向 bearish
  - REGIME=crash → SIGNAL=neutral（崩盘期任何方向都不可执行）
  - REGIME=unknown → 走原判定标准

**你有工具可调用，主动决策需要看什么数据**：
- `analyze_multi_timeframe(symbol="{{asset_symbol}}")` → 多周期 RSI/MA/分位数（**核心**）
- `get_history_data(symbol, days)` → 拉具体周期日线，查异常波动 / 关键 anchor
- `get_recent_committee_verdicts(symbol="{{asset_symbol}}")` → 看上次自己给的 SIGNAL，避免观点漂移

baseline brief 已经在 prompt 里给了基础数据，**如果你需要更深的视角主动调 tool**。
不要不调——一个负责的分析师会去查多周期对照。

**输出要求**：
- 必须中文回复
- 严格按下列格式，总长度 ≤180 字
- 不要 markdown 表格
- **必须把收到的 REGIME 字段原样回填**（用于 audit + verdict_review 归因）

REGIME: <原样回填收到的 regime 值>
SIGNAL: bullish | bearish | neutral
STRENGTH: 0-10
KEY_DATA:
  - <最有说服力的技术数据>
  - <第二条数据>
  - <第三条数据>
ONE_LINER: <一句话技术结论，含支撑/阻力位，明确说 SIGNAL 与 REGIME 的关系>

**判定标准**（在 REGIME 约束之内）：
- bullish: 价位分位 ≤ 30% OR (上升趋势 MA20>MA60 AND RSI 50-70)
- bearish: 价位分位 ≥ 70% AND (RSI > 70 OR 跌破关键均线量增)
- neutral: 中间状态
```

### 7.3 Quant R2 Prompt (QUANT_R2_PROMPT)

基于参考文档 Quant Round 2，修改为接收 round_cache 中 Q1 输出：

```
你是量化技术分析师，刚读完 Risk Officer 关于用户当前持仓状态的报告。
现在做真正的 cross-challenge：**审视自己 Round 1 的判断在用户上下文下是否仍 actionable**。

不是"坚守原判"，也不是"听 Risk 的就改"——而是"基于新信息重新判断，但 REGIME 是底线"。

**REGIME 硬保护规则（禁止违反，违反需在 REASONING 解释为什么）**：
- 如果 Round 1 收到的 REGIME=range_bound 且价格处于低位区间：
  → **不允许**因为 Risk 警告"集中度高 / 子弹少"就把 SIGNAL 从 bullish 改 neutral 或 bearish
  → 集中度问题归 Risk 管（它会喊 TRIM），技术面归 Quant 管，不要互相偷活
- 如果 REGIME=uptrend 且 Quant Round 1 已 bullish：
  → **不允许**因为 Risk 警告就改 neutral；可调 STRENGTH，不可改 SIGNAL 方向
- 如果 REGIME=downtrend：
  → 跟 Risk 同向放大没问题，可改 SIGNAL 到 bearish

**改判 SIGNAL 的合法触发条件**（在 REGIME 允许的范围内）：
- Risk 揭示子弹（dry_powder）≤ 单笔最小 cap 且 Round 1 是 bullish → 可改 neutral
  （加仓 actionability=0，但仅在 REGIME 不是 range_bound 底部时适用）
- 你 STRENGTH 想调整 ≥ 3 档 → 必须重新评估 SIGNAL 方向是否仍然成立

**输出要求**：
- 必须中文回复，严格按下列格式，≤150 字
- 必须引用 Risk Officer 的具体数据（"Risk 提到 X..."）
- 必须显式说明 REGIME 硬保护是否触发

ADJUSTED_SIGNAL: bullish | bearish | neutral
ADJUSTED_STRENGTH: 0-10
REGIME_PROTECTION_TRIGGERED: yes | no
REASONING: <引用 Risk 数据 + REGIME 保护是否触发 + 是否改判 SIGNAL 及原因>
```

### 7.4 Risk R1 Prompt (RISK_PROMPT)

基于参考文档 Risk Round 1，修改工具引用：

```
你是投资委员会的 Risk Officer，专门评估**针对 {{asset_name}} ({{asset_symbol}}) 的本次决策**对用户整体财务的风险影响。
**只看用户上下文**——不重复 Quant 的技术分析，不重复 Macro 的宏观评估。

**你有工具可调用**：
- `query_dreaming_insights(asset_symbol="{{asset_symbol}}", top_k=3)` → 长期行为模式（用户过去类似情境的过度集中持仓 / 情绪化追涨等）
- `get_recent_committee_verdicts(symbol="{{asset_symbol}}", n=5)` → 上次同资产委员会决策，看决策一致性

**核心关注（你独有的视角）**：
1. **集中度**: 该资产已占总资产多少 %？参考 PWM 行业标准（单一资产建议 ≤25-35%，>50% 即为超配）
2. **子弹**: 可用现金还剩多少？是否有钱加仓
3. **成本基础**: 用户成本均价 vs 现价，浮盈/浮亏多少
4. **历史模式**: 主动 query_dreaming_insights 看用户过去是不是情绪化追涨
5. **压力测试**: 如果该资产跌 10% / 20% / -35% 极端，整体浮亏多少 CNY

**严禁**：
- 不要捏造**任何数字**（盈亏 + 集中度 + 现金 + 总资产）。portfolio_summary
  字面写出了每个 asset 的"**集中度 X%**"和"浮盈 ±Y%"，**直接复制粘贴该数字**，
  禁止自算/估算/脑补。
- 如果 portfolio_summary 没给该字段（罕见），写 `N/A` 而不是猜。

**输出要求**：
- 必须中文回复
- 严格按下列格式，总长度 ≤150 字

SIGNAL: ok | concerned | high_risk
STRENGTH: 0-10
CONCENTRATION_PCT: <该资产占总资产 %>
DRY_POWDER_CNY: <可用子弹>
PNL_PCT: <当前浮盈百分比>
WORST_CASE_LOSS_PCT_AT_-20: <如果该资产跌 20%，整体损失百分比>
ONE_LINER: <一句话评估，含"建议建仓比例上限"或"建议减仓比例">
```

### 7.5 Risk R2 Prompt (RISK_R2_PROMPT)

基于参考文档 Risk Round 2，修改为接收 round_cache 中 R1 输出：

```
你是 Risk Officer，刚读完 Quant 对 {{asset_name}} ({{asset_symbol}}) 的技术信号。
现在做真正的 cross-challenge：**Quant 信号是否揭示了你 Round 1 没看到的用户上下文风险？**

不是"坚守原判"，也不是"看到 Quant 提分位 / RSI 就跟着升级"。

⚠️ **核心边界（必读）**：你的职责是评估**用户上下文**（集中度 / 子弹 / 浮盈 / 历史
模式），**不是**重做技术面归因。Quant 已经把 RSI / 分位 / 价位高低 折算成 SIGNAL
+ STRENGTH，你只看 Quant 给出的 *结论*，**不要拿 Quant 的原始数字（分位 / RSI）
再算一遍升级 trigger**——那是 Quant 的活，你二次升级就是放大同一份信号。

## 升级 SIGNAL 的合法规则（仅这两条）

1. **Quant 自己给 bearish 且 STRENGTH ≥ 7**：跟随 Quant 同向放大
2. **用户上下文恶化**（与 Quant 无关，是你独有的视角）：
   - 用户 7 天内多次买入同资产 → 情绪化追涨，给 high_risk
   - DRY_POWDER_CNY < 1000 → 流动性风险升级

## 禁止的升级 trigger

❌ 不要因为 Quant 报告"分位 ≥ 90%" / "RSI > 70" / "价位高位" 就升级
❌ 不要因为"浮盈大就该锁"主动升级

## 输出要求

- 必须中文回复，严格按下列格式，≤120 字
- 必须引用 Quant 的 *SIGNAL/STRENGTH*

ADJUSTED_SIGNAL: ok | concerned | high_risk
ADJUSTED_STOP_LOSS: <新止损线条件>
REASONING: <引用 Quant SIGNAL/STRENGTH + 升级/降级理由>
```

### 7.6 CIO Prompt (CIO_PROMPT)

基于参考文档 CIO 部分，修改为接收所有前序输出（不含 Wealth）：

```
你是首席投资官 (CIO)，刚听完所有前序分析报告：
- 宏观分析师 Macro 的宏观信号
- 量化分析师 Quant R1 的技术分析 + Quant R2 的 cross-challenge
- 风控官 Risk R1 的风险评估 + Risk R2 的 cross-challenge

你的任务：综合所有意见 + 用户上下文 → **直接输出可执行的客户备忘**，不要调用任何工具。

⚠️ **禁止 tool_call**：所有必要信息都在 user message 里。不要尝试调用任何工具。

**Hard Rules**：
- 任何 worker 输出含 `[WORKER_UNAVAILABLE]` 标记 → 你必须 verdict=HOLD + confidence ≤ 0.4
- confidence ≥ 0.95 + verdict=BUY → 系统会自动降级到 ACCUMULATE
- |SUGGESTED_ALLOC_CNY| > 100000 → 系统会 clamp

**裁决原则**：
1. **三方一致**: confidence ≥ 0.85，按一致方向给 verdict
2. **Quant vs Macro 分歧**: 看 Risk Officer 倒向哪边
3. **Risk Officer 给 high_risk**: 即便 Quant + Macro 都看多，也必须降级
4. **CONCENTRATION_PCT > 60%**: 任何加仓金额必须 ≤ 子弹的 10% 且做分批

**🔥 现金仓位机会成本规则（强制，必读）**：
- **CONCENTRATION_PCT < 20%**：**不允许给 HOLD**，默认至少给 ACCUMULATE
- **CONCENTRATION_PCT 20-40%**：HOLD 允许，但需在 PERSONAL_NOTE 说明理由
- **CONCENTRATION_PCT > 40%**：HOLD / TRIM 都可

**Verdict 选项**：
- `BUY` - 一次建满仓（≥ 子弹 50%）
- `ACCUMULATE` - 分批建仓/加仓（**100% 现金时的 default**）
- `HOLD` - 维持现状，**只在已有仓位 20%+ 时合法**
- `TRIM` - 部分减仓
- `SELL` - 全部清仓

**输出要求**：
- 必须中文回复，所有字段必填，没有就写 "N/A"

VERDICT: BUY | ACCUMULATE | HOLD | TRIM | SELL
CONFIDENCE: 0.0-1.0
DOMINANT_VIEW: quant | macro | risk
SUGGESTED_ALLOC_CNY: <具体金额>

EXECUTION_PLAN:
  mode: lump-sum | pyramid | grid | none
  first_tranche_cny: <第一笔金额>
  add_levels:
    - <条件式加仓描述>

RISK_PLAN:
  stop_loss_trigger: <具体条件>
  what_if_wrong:
    worst_case_pnl_cny: <最坏情况浮亏>
    recovery_estimate: <解套估计>

PERSONAL_NOTE:
  - <一句话持仓状态评估>
  - <一句话子弹占比>
  - <一句话操作纪律建议>
```

**字符限制**：Macro 400, Quant R1/R2 250, Risk R1/R2 250, CIO 400

**Portfolio 数据注入**（Risk R1 之前）：
```rust
// 在 orchestrator::run_committee() 中，Risk R1 之前：
let portfolio_summary = query_portfolio_summary(symbol).await?;
// 格式化为：
// "【用户持仓摘要】
//  资产: {{asset_name}} ({{asset_symbol}})
//  集中度: X%（占总资产）
//  可用子弹: ¥N
//  成本均价: ¥X → 现价 ¥Y → 浮盈 ±Z%
//  压力测试: 跌20% → 整体损失 ¥N（X%）"
// 注入到 Risk R1 的 user message
```

---

## Phase 8: Parser + Analysis 更新

**修改文件**：
- `src-tauri/src/invest/committee/parser.rs` — 新增字段解析
- `src-tauri/src/invest/committee/analysis.rs` — CIO Gate 4

**Parser 新增字段**：
- `regime: Option<String>` — REGIME 值（从 Quant R1 输出回填）
- `pnl_pct: Option<f64>` — 浮盈百分比（Risk R1 输出）
- `worst_case_loss_pct_at_-20: Option<f64>` — 压力测试（Risk R1 输出）
- `dominant_view: Option<String>` — 主导观点（CIO 输出）
- `suggested_alloc_cny: Option<f64>` — 建议配置金额（CIO 输出）
- `regime_protection_triggered: Option<bool>` — REGIME 硬保护触发（Quant R2 输出）
- `adjusted_stop_loss: Option<String>` — 调整后止损（Risk R2 输出）

**Analysis 新增 Gate 4（CIO）**：
- 仓位=0 且 verdict=HOLD → 降 confidence 到 0.3（现金机会成本规则）
- CONCENTRATION_PCT < 20% 且 verdict=HOLD → 降 confidence

---

## 依赖关系

```
Phase 0 (角色精简)  ──┐
Phase 1 (Regime)   ──┤
Phase 2 (Tushare)  ──┤
Phase 3 (Yahoo)    ──┼──► Phase 4 (macro_cache 表)
                       │         │
                       │         ▼
                       │    Phase 5 (调度+工具重写)
                       │         │
                       ▼         ▼
                  Phase 6 (工具分角色开放)
                            │
                            ▼
                  Phase 7 (Prompt 替换)
                            │
                            ▼
                  Phase 8 (Parser/Analysis)
```

Phase 0/1/2/3 互相独立，可并行。

---

## 关键文件清单

| 文件 | Phase | 操作 |
|------|-------|------|
| `invest/committee/roles.rs` | 0, 7 | 重构 enum、删除 Wealth、替换 prompt |
| `invest/committee/orchestrator.rs` | 0, 6 | 重构 6 步管线、round_cache 读写、提取工具循环 |
| `invest/committee/tools.rs` | 5, 6 | 重写 macro_snapshot、新增 role_tool_defs |
| `invest/committee/events.rs` | 0 | 重编号 step_index |
| `invest/committee/parser.rs` | 0, 8 | 删除 Wealth、新增字段解析 |
| `invest/committee/analysis.rs` | 0, 8 | 改为从 round_cache 对比、CIO Gate 4 |
| `invest/committee/archive.rs` | 0 | 测试更新 |
| `invest/regime.rs` | 1 | **新建** |
| `invest/international.rs` | 3 | **新建** |
| `invest/macro_refresh.rs` | 5 | **新建** |
| `storage/invest/macro_cache.rs` | 4 | **新建** |
| `storage/invest/round_cache.rs` | 0 | **新建** |
| `storage/invest/mod.rs` | 0, 4 | 添加表定义 |
| `tushare/client.rs` | 2 | 添加 4 个方法 |
| `invest/scheduler/mod.rs` | 5 | 添加 job |
| `invest/scheduler/runner.rs` | 5 | 添加 dispatch |
| `commands/invest.rs` | 0 | 删除 Wealth 分支 |
| 前端 4 个 .svelte | 0 | 删除 Wealth、6 步管线 |
| `messages/en.json` + `zh-CN.json` | 0 | 删除 Wealth 字符串 |

---

## 验证方案

1. `cargo check` — 编译通过
2. `cargo clippy -- -D warnings` — 无警告
3. `npm run build` — 前端构建通过
4. `npm run i18n:check` — 国际化检查通过
5. 手动测试：`npm run tauri dev` → 触发 macro_refresh → 跑委员会 → 检查 6 步管线 + round_cache 有数据

---

## 设计审查补充 + 修复方案（2026-05-31）

> 以下是对计划的系统性审查。每个缺口给出**具体修复指令**（文件、函数、改什么、怎么改），可直接作为实施任务使用。

---

### Gap A: 5 个不存在的函数/模块 — 具体实现设计

#### A1. `invest/regime.rs` — 新建模块

**从 `commands/invest.rs:987-1048` 提取 `get_regime_classification` 逻辑**，扩展为独立模块。

```rust
// src-tauri/src/invest/regime.rs

pub struct RegimeResult {
    pub regime: &'static str,    // uptrend | downtrend | range_bound | crash | unknown
    pub reason: String,          // 人类可读的原因
    pub strategy_hint: &'static str,
    pub metrics: RegimeMetrics,
}

pub struct RegimeMetrics {
    pub latest: f64,
    pub ma20: f64,
    pub ma60: f64,
    pub rsi14: f64,              // NEW: Wilder RSI-14
    pub volatility_ann: f64,
    pub price_quantile_2y: f64,  // NEW: 0.0-1.0，500日窗口
}

/// 从 Tushare 拉 500 日数据，计算 REGIME。
/// commands/invest.rs::get_regime_classification 改为调用此函数。
pub async fn compute_regime_for_symbol(
    client: &TushareClient,
    symbol: &str,
) -> Result<RegimeResult, String> { ... }
```

**数据需求**：拉 500 日日线（Tushare `daily` 接口），计算：
- MA20 / MA60（已有逻辑，从 commands/invest.rs:1005-1010 迁移）
- RSI-14 Wilder：`avg_gain / (avg_gain + avg_loss)` over 14 periods，用指数移动平均
- 价格分位数：当前价格在 500 日 close 中的百分位排名（0.0-1.0）
- 20日年化波动率（已有逻辑，从 commands/invest.rs:1015-1022 迁移）

**分类逻辑扩展**（在 commands/invest.rs:1024-1033 基础上）：
- `crash`：最新价 < MA60 且 5日跌幅 > 15%
- `range_bound`：非 uptrend/downtrend/crash 且 volatility < 0.35
- `unknown`：数据不足（bars < 60）

**`format_regime_context()`**：同文件内函数，输出：
```
REGIME: {regime}
REASON: {reason}
INPUTS: ma20={:.2}, ma60={:.2}, rsi14={:.1}, volatility_ann={:.1}%, price_quantile_2y={:.0}%
STRATEGY_HINT: {strategy_hint}
```

**`invest/mod.rs` 改动**：添加 `pub mod regime;`

**`commands/invest.rs` 改动**：`get_regime_classification` 改为委托 `invest::regime::compute_regime_for_symbol()`，保持 Tauri command 接口不变。

#### A2. `query_portfolio_summary()` — 在 `committee/tools.rs` 中新增

**数据来源**：`storage::invest::portfolio` 模块已有 `get_holdings()` 和相关函数。

```rust
// src-tauri/src/invest/committee/tools.rs — 新增

/// 查询目标资产的持仓摘要，格式化为 Risk R1 的 context 注入。
/// 不走 LLM 工具调用，由 orchestrator 在 Risk R1 之前直接调用。
pub fn query_portfolio_summary(symbol: &str) -> Result<String, String> {
    use crate::storage::invest::{with_conn, portfolio};

    let holdings = portfolio::get_holdings()?;
    let holding = holdings.iter().find(|h| h.symbol == symbol);

    let cash = with_conn(|conn| {
        // SELECT available FROM cash WHERE id = 1
        ...
    })?;

    let total_value = /* cash + sum(holding.market_value for all holdings) */;

    match holding {
        Some(h) => {
            let concentration = h.market_value / total_value * 100.0;
            let pnl_pct = (current_price - h.avg_cost) / h.avg_cost * 100.0;
            let worst_case = concentration * 0.20; // 跌20%的近似损失
            Ok(format!(
                "【用户持仓摘要】\n\
                 资产: {} ({})\n\
                 集中度: {:.1}%（占总资产）\n\
                 可用子弹: ¥{:.0}\n\
                 成本均价: ¥{:.2} → 现价 ¥{:.2} → 浮盈 {:+.1}%\n\
                 压力测试: 跌20% → 整体损失 ¥{:.0}（{:.1}%）",
                h.name, symbol, concentration, cash,
                h.avg_cost, current_price, pnl_pct,
                worst_case * total_value / 100.0, worst_case
            ))
        }
        None => Ok(format!(
            "【用户持仓摘要】\n\
             资产: {} (未持仓)\n\
             集中度: 0%\n\
             可用子弹: ¥{:.0}",
            symbol, cash
        )),
    }
}
```

**关键**：需要调用 Tushare `daily()` 获取现价。使用 `read_tushare_token()` + `TushareClient::new()`，与 `exec_history_data` 模式一致。

#### A3. `run_with_tool_loop()` — 从 `run_macro_phase` 提取

**当前代码**（orchestrator.rs:245-389）：`run_macro_phase` 内含完整的 tool-call loop（首次调用 → 检查 tool_calls → 执行工具 → 第二次调用）。

**提取为通用函数**：

```rust
// orchestrator.rs — 新增

/// 通用的 LLM + 工具循环。R1 有工具时使用，R2/CIO 无工具时直接调用。
async fn run_with_tool_loop(
    client: &dyn InvestLlmClient,
    symbol: &str,
    role: CommitteeRole,
    round: u8,
    system_prompt: &str,
    messages: &[Message],
    tool_defs: Option<Vec<ToolDef>>,
    llm_config: &LlmConfig,
) -> Result<(RoundOutput, u32), String> {
    let governor = global_governor();
    let provider = llm_config.provider;
    let _permit = governor.acquire(provider).await;
    let start = std::time::Instant::now();
    let mut total_tokens: u32 = 0;

    let response1 = match llm_call_with_retry(client, system_prompt, messages, tool_defs.as_deref(), llm_config).await {
        Ok(r) => r,
        Err(e) => return Ok((make_unavailable_output(role, round, start), 0)),
    };
    total_tokens += response1.usage.total_tokens;

    if let Some(ref tools) = tool_defs {
        if !response1.tool_calls.is_empty() {
            // 构建 assistant message + 执行工具 + 第二次调用
            let mut msgs = messages.to_vec();
            msgs.push(build_assistant_tool_message(&response1));
            for tc in &response1.tool_calls {
                let result = execute_tool(&tc.name, &tc.arguments.to_string(), symbol).await;
                msgs.push(tool_result_message(&tc.id, &result.unwrap_or_else(|e| format!("Error: {}", e))));
            }
            let response2 = llm_call_with_retry(client, system_prompt, &msgs, None, llm_config).await;
            // ... 同 run_macro_phase:335-371
        }
    }

    // 无工具调用 — 直接用首次响应
    let (text, truncated) = hard_truncate(&response1.content, role, 0);
    let parsed = parse_role_output(role, &text, truncated);
    Ok((RoundOutput { role, round, parsed, latency_ms: start.elapsed().as_millis() as u64, tokens_used: total_tokens }, total_tokens))
}
```

**`run_macro_phase` 改为调用 `run_with_tool_loop`**，传入 `Some(macro_tool_defs())`。

**`run_role_phase` 改造**：新增 `tool_defs: Option<Vec<ToolDef>>` 参数，内部调用 `run_with_tool_loop`。

#### A4. `role_tool_defs()` — 在 `tools.rs` 中新增

```rust
// src-tauri/src/invest/committee/tools.rs — 新增

/// 按 (role, round) 返回该角色该轮次可用的工具定义。
/// R1 有工具，R2 无工具，CIO 无工具。
pub fn role_tool_defs(role: CommitteeRole, round: u8) -> Option<Vec<ToolDef>> {
    if round > 1 {
        return None; // R2 及以后无工具
    }
    match role {
        CommitteeRole::Macro => Some(macro_tool_defs()),
        CommitteeRole::Quant => Some(vec![
            get_history_data_def(),
            analyze_multi_timeframe_def(),
            get_recent_committee_verdicts_def(),
        ]),
        CommitteeRole::Risk => Some(vec![
            query_dreaming_insights_def(),
            get_recent_committee_verdicts_def(),
        ]),
        CommitteeRole::Cio => None,
    }
}
```

**改造**：将现有 `macro_tool_defs()` 中的 5 个工具定义拆分为独立的 `fn get_history_data_def() -> ToolDef` 等，然后 `macro_tool_defs()` 组合它们，`role_tool_defs()` 按角色选取子集。

---

### Gap B: 现有函数改造 — 具体代码变更

#### B1. `run_debate_rounds()` (orchestrator.rs:469-529)

**当前**：按 `round` 变量选择 `QuantR1/RiskR1` 或 `QuantR2/RiskR2`。

**改造**：
```rust
// Before:
let roles = if round == 1 {
    vec![CommitteeRole::QuantR1, CommitteeRole::RiskR1]
} else {
    vec![CommitteeRole::QuantR2, CommitteeRole::RiskR2]
};

// After:
let roles = vec![CommitteeRole::Quant, CommitteeRole::Risk];
```

循环体中 `run_role_phase` 调用改为传入 `(role, round)` 和 `role_tool_defs(role, round)`。

**回放模式支持**（debate_rounds > 2）：
```rust
for round_num in 1..=max_rounds {
    let actual_round = if round_num <= 2 { round_num } else { 2 }; // 3+ 复用 R2 prompt
    for role in &[CommitteeRole::Quant, CommitteeRole::Risk] {
        let output = run_role_phase(client, symbol, *role, actual_round, ...).await?;
        round_outputs.push(output);
    }
    if round_num >= 2 && check_convergence(round_outputs) {
        converged = true;
        break;
    }
}
```

#### B2. `check_convergence()` (analysis.rs:11-57)

**当前**：过滤 `QuantR1/QuantR2` 和 `RiskR1/RiskR2`，比较 `quant_view`/`risk_view`。

**改造**：
```rust
// Before:
let quant_rounds: Vec<&RoundOutput> = round_outputs
    .iter()
    .filter(|o| matches!(o.role, CommitteeRole::QuantR1 | CommitteeRole::QuantR2))
    .collect();

// After:
let quant_rounds: Vec<&RoundOutput> = round_outputs
    .iter()
    .filter(|o| o.role == CommitteeRole::Quant)
    .collect();
```

匹配逻辑改为：
```rust
// Before: q_views_match = q1.parsed.quant_view == q2.parsed.quant_view
// After:
let q_signals_match = match (&q1.parsed.signal, &q2.parsed.signal) {
    (Some(a), Some(b)) => a == b,  // R1 输出 signal, R2 输出 adjusted_signal
    _ => false,
};
// 需要在 ParsedFields 中统一：R2 的 ADJUSTED_SIGNAL 也存入 signal 字段
```

**关键决策**：R2 的 `ADJUSTED_SIGNAL` 解析后存入 `ParsedFields.signal`（统一字段），而不是新建 `adjusted_signal` 字段。这样 `check_convergence` 无需区分 R1/R2 的字段名。同理 Risk R2 的 `ADJUSTED_SIGNAL` 存入 `signal`。

#### B3. `hard_truncate()` max_chars 更新 (roles.rs:58-63)

```rust
// Before:
Self::QuantR1 | Self::QuantR2 | Self::RiskR1 | Self::RiskR2 | Self::Wealth => 200,
Self::Cio => 300,

// After:
Self::Quant => 250,
Self::Risk => 250,
Self::Cio => 400,
```

#### B4. `format_decision_markdown()` (archive.rs:160-250)

- 删除 "Wealth Context" 段落
- Round Outputs 表从 7→6 行
- Pipeline 描述更新为 "Macro → Quant R1 → Risk R1 → Quant R2 → Risk R2 → CIO"

#### B5. `run_role_phase` 签名改造 (orchestrator.rs:395-461)

**当前签名**：`fn run_role_phase(client, symbol, role, config, round_outputs, macro_signal, emergency_buffer_cny)`
- `round` 从 role variant 推断（QuantR2→2, 其他→1）
- prompt 从 `load_prompt(role)` 加载（无轮次区分）
- 无工具调用

**改造后签名**：
```rust
async fn run_role_phase(
    client: &dyn InvestLlmClient,
    symbol: &str,
    role: CommitteeRole,
    round: u8,                              // NEW: 显式传入
    config: &CommitteeConfig,
    round_outputs: &[RoundOutput],
    macro_signal: &str,
    emergency_buffer_cny: f64,
    extra_context: Option<String>,           // NEW: REGIME context 或 portfolio_summary
) -> Result<RoundOutput, String>
```

内部：
- `load_prompt(role)` → `load_prompt_for_round(role, round, asset_name, asset_symbol)`
- `build_context_messages` 中注入 `extra_context`
- 调用 `run_with_tool_loop` 传入 `role_tool_defs(role, round)`

#### B6. `events.rs` `step_index_for_role` 改造

```rust
// Before (7 variants):
pub fn step_index_for_role(role: CommitteeRole, round: u8) -> usize {
    match role {
        CommitteeRole::Macro => 0,
        CommitteeRole::QuantR1 => 1,
        ...
        CommitteeRole::Cio => 6,
    }
}

// After (4 variants + round):
pub fn step_index_for_role(role: CommitteeRole, round: u8) -> usize {
    match (role, round) {
        (CommitteeRole::Macro, _) => 0,
        (CommitteeRole::Quant, 1) => 1,
        (CommitteeRole::Risk, 1) => 2,
        (CommitteeRole::Quant, _) => 3,  // R2+
        (CommitteeRole::Risk, _) => 4,   // R2+
        (CommitteeRole::Cio, _) => 5,
    }
}
```

---

### Gap C: Parser 全面改造 — 具体字段映射

#### C1. `ParsedFields` 改造 (parser.rs:7-40)

```rust
// 删除:
- pub quant_view: Option<String>,
- pub risk_view: Option<String>,
- pub wealth_context: Option<String>,
- pub solvency_buffer_level: Option<String>,

// 新增:
+ pub regime: Option<String>,                    // Quant R1: REGIME 回填
+ pub key_data: Option<Vec<String>>,             // Quant R1: KEY_DATA 列表
+ pub one_liner: Option<String>,                 // Quant R1/Risk R1: ONE_LINER
+ pub pnl_pct: Option<f64>,                      // Risk R1: PNL_PCT
+ pub worst_case_loss_pct: Option<f64>,          // Risk R1: WORST_CASE_LOSS_PCT_AT_-20
+ pub regime_protection_triggered: Option<bool>,  // Quant R2: REGIME_PROTECTION_TRIGGERED
+ pub adjusted_stop_loss: Option<String>,         // Risk R2: ADJUSTED_STOP_LOSS
+ pub reasoning: Option<String>,                  // Quant R2/Risk R2: REASONING
+ pub dominant_view: Option<String>,              // CIO: DOMINANT_VIEW
+ pub suggested_alloc_cny: Option<f64>,           // CIO: SUGGESTED_ALLOC_CNY

// 保留（不变）:
  pub signal: Option<String>,       // 统一存放：R1 的 SIGNAL 和 R2 的 ADJUSTED_SIGNAL
  pub strength: Option<f64>,        // 统一存放：R1 的 STRENGTH 和 R2 的 ADJUSTED_STRENGTH
  pub concentration_pct: Option<f64>,
  pub dry_powder_cny: Option<f64>,
  pub verdict: Option<String>,
  pub confidence: Option<f64>,
  pub personal_note: Option<String>,
  pub execution_plan: Option<String>,
  pub risk_plan: Option<String>,
  pub truncated: bool,
  pub raw_text: String,
```

#### C2. Parser 函数改造

**`parse_role_output` dispatch** (parser.rs:47-63)：
```rust
// Before:
match role {
    CommitteeRole::QuantR1 | CommitteeRole::QuantR2 => parse_quant(text, &mut parsed),
    CommitteeRole::RiskR1 | CommitteeRole::RiskR2 => parse_risk(text, &mut parsed),
    CommitteeRole::Wealth => parse_wealth(text, &mut parsed),
    ...
}

// After:
match role {
    CommitteeRole::Quant => parse_quant(text, &mut parsed, round),
    CommitteeRole::Risk => parse_risk(text, &mut parsed, round),
    // 删除 Wealth 分支
    ...
}
```

**`parse_quant(text, parsed, round)`**：
```rust
fn parse_quant(text: &str, parsed: &mut ParsedFields, round: u8) {
    if round == 1 {
        // R1: REGIME, SIGNAL, STRENGTH, KEY_DATA, ONE_LINER
        parsed.regime = extract_field(text, "REGIME");
        parsed.signal = extract_field(text, "SIGNAL").map(normalize_signal);
        parsed.strength = extract_f64(text, "STRENGTH");
        parsed.key_data = extract_list_field(text, "KEY_DATA");
        parsed.one_liner = extract_field(text, "ONE_LINER");
    } else {
        // R2: ADJUSTED_SIGNAL, ADJUSTED_STRENGTH, REGIME_PROTECTION_TRIGGERED, REASONING
        parsed.signal = extract_field(text, "ADJUSTED_SIGNAL").map(normalize_signal);
        parsed.strength = extract_f64(text, "ADJUSTED_STRENGTH");
        parsed.regime_protection_triggered = extract_field(text, "REGIME_PROTECTION_TRIGGERED")
            .map(|s| s.to_lowercase() == "yes");
        parsed.reasoning = extract_field(text, "REASONING");
    }
}
```

**`parse_risk(text, parsed, round)`**：
```rust
fn parse_risk(text: &str, parsed: &mut ParsedFields, round: u8) {
    parsed.concentration_pct = extract_f64(text, "CONCENTRATION_PCT");
    parsed.dry_powder_cny = extract_f64(text, "DRY_POWDER_CNY");
    if round == 1 {
        // R1: SIGNAL, STRENGTH, PNL_PCT, WORST_CASE_LOSS_PCT_AT_-20, ONE_LINER
        parsed.signal = extract_field(text, "SIGNAL").map(normalize_risk_signal);
        parsed.strength = extract_f64(text, "STRENGTH");
        parsed.pnl_pct = extract_f64(text, "PNL_PCT");
        parsed.worst_case_loss_pct = extract_f64(text, "WORST_CASE_LOSS_PCT_AT_-20");
        parsed.one_liner = extract_field(text, "ONE_LINER");
    } else {
        // R2: ADJUSTED_SIGNAL, ADJUSTED_STOP_LOSS, REASONING
        parsed.signal = extract_field(text, "ADJUSTED_SIGNAL").map(normalize_risk_signal);
        parsed.adjusted_stop_loss = extract_field(text, "ADJUSTED_STOP_LOSS");
        parsed.reasoning = extract_field(text, "REASONING");
    }
}
```

**新增 `extract_list_field`**：解析 `KEY_DATA:` 后的多行 `- item` 列表。

**`normalize_signal(s)`**：统一 bullish/bearish/neutral。
**`normalize_risk_signal(s)`**：统一 ok/concerned/high_risk。

---

### Gap D: round_cache 双重语义 — 代码注释方案

在 orchestrator.rs 的 `run_committee` 函数开头添加注释块：

```rust
// ── round_cache 语义说明 ──────────────────────────────────────────────
// 本管线中 "round cache" 有两种用途，请勿混淆：
//
// 1. 同次运行内存对比（convergence / sentinel）：
//    - `round_outputs: Vec<RoundOutput>` 是 run_committee 的局部变量
//    - check_convergence() 比较 Q1 vs Q2（signal + strength）
//    - check_sentinel() 比较 R1 vs R2（concentration_pct）
//    - 不落盘，管线结束即销毁
//
// 2. 跨次持久化查询（工具用）：
//    - `round_cache` SQLite 表（Phase 0.6 新建）
//    - save_round_cache() 在每步完成后写入
//    - get_recent_committee_verdicts 等工具读取历史数据
//    - 用于 LLM 查看自己上次的观点，避免漂移
// ──────────────────────────────────────────────────────────────────────
```

---

### Gap E: `exec_history_data` 双数据源 — 已确认无遗漏

Phase 7 的 Macro prompt 已包含 `get_history_data(symbol="^VIX", days=90)` 和 `get_history_data(symbol="000300.SH", days=90)` 示例。LLM 可以正确使用 Yahoo 符号。

**Phase 5 中 `exec_history_data` 的改造**（tools.rs:130-183）：
```rust
// 新增判断：
let is_tushare = regex::Regex::new(r"^\d{6}\.(SH|SZ)$").unwrap().is_match(symbol);
if is_tushare {
    // 现有 Tushare daily 逻辑
} else {
    // 调用 invest::international::fetch_yahoo_history(symbol, days)
}
```

---

### Gap F: `macro_cache` 部分失败策略 — 实现方案

在 `invest/macro_refresh.rs` 中：

```rust
pub async fn refresh_macro_cache(client: &TushareClient) -> Result<(), String> {
    let indicators = vec![
        ("csi300_close", fetch_csi300_close(client)),
        ("northbound_net", fetch_northbound(client)),
        ("vix", fetch_yahoo("^VIX")),
        // ... 12 个指标
    ];

    let results = futures::future::join_all(indicators).await;

    for (name, result) in results {
        match result {
            Ok((value, source)) => {
                save_macro_cache(name, value, source)?;  // UPSERT
            }
            Err(e) => {
                log::warn!("macro_refresh: {} failed: {} (keeping stale)", name, e);
                // 不更新 fetched_at → exec_macro_snapshot 可检测 stale
            }
        }
    }
    Ok(())
}
```

`exec_macro_snapshot` 输出中对 `fetched_at > 30min` 的指标追加 `(stale)` 标注。

---

### Gap G: `trade_calendar` 集成 — 改动点

**`scheduler/mod.rs`**：`macro_refresh` job 的 `requires_trading_day: true`。

**`scheduler/runner.rs`**：已有 `is_trading_day()` 检查（在 `start()` 循环中，line ~120）。`requires_trading_day: true` 的 job 在非交易日自动 skip。无需额外代码。

---

### Gap H: `analyze_multi_timeframe` 数据量 — 改动点

**当前**（tools.rs:185-250）：拉 180 日数据，计算 MA5/MA20/MA60 + HV20。

**改造**：
```rust
// Before:
let start_date = (chrono::Local::now() - chrono::Duration::days(180)).format("%Y%m%d").to_string();

// After:
let start_date = (chrono::Local::now() - chrono::Duration::days(750)).format("%Y%m%d").to_string();
// 750 日 ≈ 500 交易日（考虑节假日）
```

新增计算：
```rust
// RSI-14 (Wilder)
let gains: Vec<f64> = ...;
let losses: Vec<f64> = ...;
let avg_gain = gains.iter().take(14).sum::<f64>() / 14.0;
let avg_loss = losses.iter().take(14).sum::<f64>() / 14.0;
// Wilder 平滑：后续 avg_gain = (prev_avg_gain * 13 + current_gain) / 14
let rsi14 = if avg_loss == 0.0 { 100.0 } else { 100.0 - 100.0 / (1.0 + avg_gain / avg_loss) };

// 价格分位数（2年窗口）
let all_closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
let mut sorted = all_closes.clone();
sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
let rank = sorted.iter().position(|&v| v >= latest_close).unwrap_or(sorted.len());
let price_quantile = rank as f64 / sorted.len() as f64;

// MA120
let ma120 = if bars.len() >= 120 {
    bars.iter().take(120).map(|b| b.close).sum::<f64>() / 120.0
} else { /* fallback */ };
```

输出格式增加：
```
RSI14: {:.1}
价位分位(2Y): {:.0}%
MA120: {:.2}
```

---

### Gap I: Frontend Prompt 保存 R1/R2 — 接口方案

**决策**：保持 Tauri command 接口简单，使用 6 个 prompt 文件名作为 key。

**`save_role_prompt` 改造**（commands/invest.rs:725-740）：
```rust
#[tauri::command]
pub fn save_role_prompt(role: String, content: String) -> Result<(), String> {
    use crate::invest::committee::roles::{save_prompt_by_filename, CommitteeRole};
    // role 参数现在直接是文件名：macro, quant, quant_r2, risk, risk_r2, cio
    save_prompt_by_filename(&role, &content)
}
```

**`get_role_prompts` 改造**（commands/invest.rs:711-723）：
```rust
#[tauri::command]
pub fn get_role_prompts() -> Result<HashMap<String, String>, String> {
    let files = ["macro", "quant", "quant_r2", "risk", "risk_r2", "cio"];
    let mut map = HashMap::new();
    for f in files {
        map.insert(f.to_string(), load_prompt_by_filename(f));
    }
    Ok(map)
}
```

**roles.rs 新增**：
```rust
pub fn save_prompt_by_filename(name: &str, content: &str) -> Result<(), String> {
    let dir = get_prompt_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create prompt dir: {e}"))?;
    std::fs::write(dir.join(format!("{}.txt", name)), content)
        .map_err(|e| format!("write prompt: {e}"))
}

pub fn load_prompt_by_filename(name: &str) -> String {
    let path = get_prompt_dir().join(format!("{}.txt", name));
    std::fs::read_to_string(&path).unwrap_or_else(|_| {
        // fallback to default
        match name {
            "macro" => MACRO_PROMPT,
            "quant" => QUANT_PROMPT,
            "quant_r2" => QUANT_R2_PROMPT,
            "risk" => RISK_PROMPT,
            "risk_r2" => RISK_R2_PROMPT,
            "cio" => CIO_PROMPT,
            _ => "",
        }.to_string()
    })
}
```

**前端 `CommitteeRolesTab`**：每个角色卡片用 Tab 切换 R1/R2（Quant 和 Risk 各两个编辑器，Macro 和 CIO 各一个）。

---

### Gap J: Batch 模式 — 确认无额外改动

`run_committee_batch` 和 `run_committee_batch_stream` 直接调用 `run_committee`，管线内部改动自动生效。`step_index_for_role` 的返回值范围从 0-6 变为 0-5，前端 `PipelineFlow` 通过 `PIPELINE_STEPS` 常量同步更新即可。

---

### Gap K: Archive markdown 模板 — 具体改动

`format_decision_markdown()` (archive.rs:160-250)：

1. 删除 "### Wealth Context" 段落
2. Round Outputs 表改为 6 行：
   ```
   | Step | Role | Signal | Strength |
   |------|------|--------|----------|
   | 1 | Macro | risk_on | 7 |
   | 2 | Quant R1 | bullish | 8 |
   | 3 | Risk R1 | ok | 3 |
   | 4 | Quant R2 | bullish | 7 |
   | 5 | Risk R2 | ok | 3 |
   | 6 | CIO | ACCUMULATE | 0.7 |
   ```
3. Pipeline 描述改为 "Macro → REGIME → Portfolio → Quant R1 → Risk R1 → Quant R2 → Risk R2 → CIO"
4. 测试 helper `make_test_result()` 中删除 wealth 相关字段

---

### Gap L: REGIME = range_bound 阈值 — 已内化

`compute_regime_for_symbol()` 返回的 `RegimeMetrics.price_quantile_2y` 会包含在 `format_regime_context()` 输出中。

Quant R1 prompt 中写："REGIME=range_bound 且价格处于低位区间 → SIGNAL 偏向 bullish"。

**LLM 自行判断**：收到 `INPUTS: ... price_quantile_2y=15%` 后，会理解为"低位区间"。无需硬编码阈值在 Rust 代码中。prompt 中的 "低位/高位" 是给 LLM 的模糊指引，price_quantile 数字提供精确参考。

---

### Gap M: Tauri Command — 已有，无需新增

`get_regime_classification` Tauri command 已存在（commands/invest.rs:987-1048）。Phase 1 只需将其内部实现委托给 `invest::regime::compute_regime_for_symbol()`。

REGIME 信息通过 `CommitteeEvent::RoleComplete(Quant, 1)` 的 `summary.parsed.regime` 字段传递给前端，无需独立 command。

`format_decision_markdown()` 中包含 "Wealth Context" 段落和 7 步 pipeline 表。需同步删除 Wealth 段落、更新 pipeline 为 6 步。

### L. REGIME = range_bound 的低位/高位判定

prompt 中写"价格处于低位区间 → bullish"、"高位区间 → bearish"，但未定义阈值。

**建议**：与 `analyze_multi_timeframe` 的价格分位数对齐——`price_quantile_2y ≤ 0.20` = 低位，`≥ 0.80` = 高位。在 REGIME context block 中直接输出 `PRICE_QUANTILE: 0.XX`，让 Quant LLM 自行判断。或者在 `format_regime_context` 中硬编码 hint："价格处于低位（分位 X%）"。

### M. Tauri Command 新增

Phase 1 新建 `regime.rs`，但未提及是否需要新增 Tauri command 给前端展示 REGIME 信息。

**建议**：暂不需要独立 command。REGIME 仅在委员会管线内部使用，前端通过 `CommitteeEvent::RoleComplete(Quant R1)` 的 summary 间接获取 REGIME 值。
