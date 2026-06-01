# [wip] 委员会 Prompt L1-L4 策略框架升级

> 状态：5 角色架构 + 数据注入层确认，Prompt 模板已缓存待审阅
> 创建：2026-06-01
> 范围：Prompt + Context + Parser + Tools + 数据注入层 + L4 Officer + macro_cache
> Prompt 模板：`docs/superpowers/plans/committee-prompts-v2.md`

---

## 背景

当前委员会 4 个角色（Macro / Quant / Risk / CIO）的 prompt 缺少 L1-L4 四层嵌套决策体系、催化剂层级框架、L4 行为红灯评分、Tier-1 退出机制等核心策略概念。

Macro 角色存在架构矛盾：prompt 写"不评论单一资产"，但系统逐股调用，导致同一宏观环境对不同标的产生不一致的信号。

---

## 核心设计决策

| 决策 | 结论 |
|---|---|
| 宏观信号 | 全局市场底色统一，标的敏感度分化（敏感度在 Macro） |
| L2 催化剂归属 | CIO 负责标的催化剂判定 |
| 个股资金流 | 注入 Quant，context + tool 双路径 |
| L4 行为红灯 | 新增 L4 Officer 角色，独立于 Risk |
| L4 评分 | Rust 端规则引擎计算，不依赖 LLM |
| macro_cache 扩展 | 涨跌停家数 + 两市成交额 |
| 输出格式 | 全部扁平 key-value，禁止嵌套 |
| CIO 定位 | 纯综合裁决，只读其他角色输出，不接收原始数据注入 |
| 数据注入 | stock-data-report 数据源（PE/PB/ROE/财务增速/评级/风险新闻）补全注入 |

---

## 架构：5 角色 + REGIME

```
Macro (LLM)
  ↓
Regime (Rust 确定性计算 — MA/RSI/波动率，不是 LLM 调用)
  ↓
Quant R1 + Risk R1 (并行 LLM)
  ↓
Quant R2 + Risk R2 (交叉辩论 LLM)
  ↓
L4 Officer (LLM — 新增)
  ↓
CIO (LLM — 纯综合裁决，无数据注入)
```

| 步骤 | 类型 | 说明 |
|---|---|---|
| Macro | LLM | L1 全局底色 + 敏感度 + 情绪温度 |
| Regime | Rust | MA20/MA60/RSI-14/波动率 → uptrend/downtrend/range_bound/crash |
| Quant R1 | LLM | 技术执行（受 REGIME 约束）+ 资金流 + 估值评估 |
| Risk R1 | LLM | 用户财务风险 + 标的风险（估值/财务/评级/利空） |
| Quant R2 | LLM | 交叉挑战 + REGIME 硬保护 |
| Risk R2 | LLM | 交叉挑战 + 风险复核 |
| L4 Officer | LLM (新增) | 卫语句 + 情绪评估 + 行为红灯 + 买点合理性 |
| CIO | LLM | L2 催化剂 + 综合裁决（只读报告，无数据注入） |

---

## 角色职责

```
┌──────────────┬───────────────────────────────────────────────────────┐
│ Macro        │ L1 市场环境（阶段判断 + 全局信号 + 标的敏感度）        │
│              │ 宏观催化剂感知（降准/地缘/美联储等，不分类 Tier）       │
│              │ 行业敏感度（基于标的所属行业判断影响方向）              │
├──────────────┼───────────────────────────────────────────────────────┤
│ Quant        │ L3 技术执行（REGIME + 买点 + 盈亏比 + 支撑阻力）        │
│              │ 资金流向分析（context + tool 双路径）                   │
│              │ 估值评估（PE/PB 分位数 + ROE 质量）                    │
├──────────────┼───────────────────────────────────────────────────────┤
│ Risk         │ 用户财务风险（集中度/子弹/浮盈/回撤）                   │
│              │ 标的风险（估值泡沫/财务恶化/评级下调/个股利空）         │
│              │ 情绪评估（emotional_state 输出给 L4 Officer）           │
├──────────────┼───────────────────────────────────────────────────────┤
│ L4 Officer   │ L4 卫语句（Macro+Quant+Risk 三重恶化 → 强制止损）      │
│ (新增)       │ L4 情绪校准（冲动交易/报复交易/恐慌性抛售检测）         │
│              │ 行为红灯评分（情绪+仓位+方向 → 0-30 分）               │
│              │ 买点合理性检查                                         │
├──────────────┼───────────────────────────────────────────────────────┤
│ CIO          │ L2 标的催化剂识别 + Tier 判定                           │
│              │ 综合裁决（读 Macro/Quant/Risk/L4 Officer 报告）         │
│              │ L4 执行检查（4 项 bool 通过数）                         │
│              │ Tier-1 退出条件设定                                     │
│              │ ⚠️ 不接收原始数据注入，只读其他角色输出                  │
└──────────────┴───────────────────────────────────────────────────────┘
```

---

## 数据注入层（从 stock-data-report 补全）

### 数据→角色映射

| 数据 | 来源 | 注入到 | 分析理由 |
|---|---|---|---|
| PE/PB/总市值/流通市值 | tushare daily_basic | **Quant** | 估值分位数，判断价格是否合理 |
| ROE | tushare fina_indicator | **Quant** | 盈利质量，支撑买点评估 |
| 营收增速/净利增速 | tushare fina_indicator | **Risk** | 财务恶化风险（增速转负 = 高风险） |
| 机构评级 (买/持/减/卖) | tushare report_rc | **Risk** | 评级下调 = 风险信号 |
| 个股风险新闻 | mx-finance search_financial_news | **Risk** | 利空/减持/诉讼等事件风险 |
| 换手率 | tushare daily_basic | **Quant** | 量价分析补充 |
| 行业/赛道 | tushare index_member_all | **Macro** | 行业敏感度分析（L1 对不同行业影响不同） |
| 宏观指标 | tushare/yahoo macro APIs | **Macro** | 已有，不变 |
| 均线/RSI/波动率 | REGIME 计算 | **Quant** | 已有，不变 |
| 资金流 | tushare moneyflow_dc | **Quant** | 计划新增（context + tool） |

### AssetContext 结构

```rust
pub struct AssetContext {
    pub asset_type: String,              // "stock" | "etf"
    pub industry: Option<String>,        // tushare stock_basic → 申万二级行业
    pub money_flow_summary: Option<String>, // tushare moneyflow_dc → 近5日主力/散户净流入
    // 新增（从 stock-data-report 补全）
    pub pe_ttm: Option<f64>,             // tushare daily_basic
    pub pb: Option<f64>,                 // tushare daily_basic
    pub total_mv_yi: Option<f64>,        // tushare daily_basic（亿元）
    pub roe: Option<f64>,                // tushare fina_indicator（最新季度）
    pub or_yoy: Option<f64>,             // tushare fina_indicator（营收增速%）
    pub np_yoy: Option<f64>,             // tushare fina_indicator（净利增速%）
    pub rating_summary: Option<String>,  // tushare report_rc → "买入15/增持3/中性1/减持0/卖出0"
    pub risk_news: Option<String>,       // mx-finance search_financial_news → 最多5条
    pub turnover_rate: Option<f64>,      // tushare daily_basic
}
```

### 注入位置

| 角色 | 注入方式 |
|---|---|
| Macro | `build_context_messages` 注入 industry（行业敏感度） |
| Quant | `build_context_messages` 注入 money_flow_summary + PE/PB/ROE/换手率（估值评估） |
| Risk | `build_context_messages` 注入 or_yoy/np_yoy + rating_summary + risk_news + max_drawdown（标的风险） |
| L4 Officer | `build_context_messages` 注入 Macro/Quant/R2/Risk/R2 全部输出 + portfolio_summary |
| CIO | `build_context_messages` 注入 Macro/Quant/R2/Risk/R2/L4 Officer 全部输出（无原始数据） |

### 最大回撤预计算

Risk prompt 中的"最大回撤"由 Rust 端预计算（基于 holdings 当前价格 × -20% 跌幅 → 整体浮亏），注入 Risk context，不依赖 LLM 自算。

---

## 输出格式设计（全部扁平 key-value）

### Macro（8 行，字符 600）

```
信号: risk_on | risk_off | neutral
强度: 0-10
市场阶段: 主升 | 分歧 | 退潮 | 冰点 | 混沌
敏感度: positive | negative | neutral
敏感度原因: <一句话≤20字>
情绪温度: 乐观 | 中性 | 谨慎 | 恐慌
宏观催化剂: <当前最重要的宏观事件，无则写"无">
一句话: <加仓/减仓/维持倾向>
```

### Quant R1（9 行，字符 350）

```
市场状态: <原样回填 regime>
信号: bullish | bearish | neutral
强度: 0-10
资金流向: 主力净流入|流出 <金额>，散户净流入|流出 <金额>
估值评估: PE <值> 分位<百分位>，PB <值>，ROE <值>%
关键数据:
  - <最有说服力的技术数据>
  - <第二条数据>
买点评估: 低吸 | 突破 | 回踩 | 追高 | 不可交易
一句话: <技术结论含支撑/阻力位>
```

### Quant R2（5 行，字符 350）

```
调整信号: bullish | bearish | neutral
调整强度: 0-10
调整买点: 低吸 | 突破 | 回踩 | 追高 | 不可交易
保护触发: yes | no
推理: <引用 Risk 数据 + REGIME 保护是否触发 + 是否改判 SIGNAL 及原因>
```

### Risk R1（10 行，字符 350）

```
信号: ok | concerned | high_risk
强度: 0-10
集中度: <该资产占总资产 %>
可用子弹: <可用子弹>
盈亏比: <当前浮盈百分比>
最大回撤: <系统预计算，直接引用>
标的风险: <估值/财务/评级/利空综合一句话>
情绪状态: stable | warning | danger
L4否决: true | false
否决原因: <卫语句触发条件，未触发写 N/A>
```

### Risk R2（4 行，字符 350）

```
调整信号: ok | concerned | high_risk
调整止损: <新止损线条件>
情绪重校准: stable | warning | danger
推理: <引用 Quant SIGNAL/STRENGTH + 情绪变化>
```

### L4 Officer（6 行，字符 250）— 新增

```
卫语句: true | false
卫语句原因: <三重恶化判定，未触发写 N/A>
情绪评估: stable | warning | danger
行为红灯: green | yellow | red
买点合理: yes | no
推理: <基于 Macro/Quant/Risk 三方信号的执行控制判断>
```

### CIO（17 行，字符 600，全部扁平）

```
裁决: BUY | ACCUMULATE | HOLD | TRIM | SELL
置信度: 0.0-1.0
催化剂层级: Tier1 | Tier2 | Tier3 | 无
催化剂摘要: <一句话>
主流观点: quant | macro | risk
建议配置: <金额>
执行模式: lump-sum | pyramid | grid | none
首笔金额: <第一笔金额>
止损条件: <具体条件>
止损明确: yes | no
仓位合理: yes | no
情绪稳定: yes | no
买点合理: yes | no
is_tier1: yes | no
tier1_watch_hours: 72（仅 Tier1 标的填写）
个人备注: <一句话持仓状态评估 + 子弹占比 + 操作纪律建议>
```

**行为红灯约束**：当 L4 Officer 报告行为红灯=red 时，CIO 裁决 ≤ HOLD。
**安全阀**：L4 卫语句=true 或行为红灯=red 或 worker unavailable → 覆盖现金机会成本规则，HOLD 合法。

---

## L4 Officer 详细设计

### 职责边界

L4 Officer **不做**：
- 不做技术分析（Quant 的活）
- 不做财务风险评估（Risk 的活）
- 不做宏观判断（Macro 的活）
- 不做催化剂判定（CIO 的活）

L4 Officer **只做**：
1. 卫语句判定：Macro risk_off(≥7) + Quant bearish(≥7) + 浮亏≥15% → 同时满足=true
2. 情绪评估：基于 Risk 的 emotional_state + 用户交易行为模式
3. 行为红灯评分：情绪+仓位+方向 → green/yellow/red
4. 买点合理性：基于 Quant 的买点评估 + REGIME 状态

### 数据输入

- Macro/Quant R1/R2/Risk R1/R2 的全部输出文本
- portfolio_summary（集中度/子弹/浮盈）
- recent_trade_count_7d（Rust 端查询）
- dreaming_insights（用户行为模式）

### 行为红灯评分（Rust 端计算）

```
c_score = match emotional_state { "stable" => 0, "warning" => 5, "danger" => 10, _ => 3 }
k_score = if concentration_pct > 60.0 { 10 } else if concentration_pct > 40.0 { 6 } else if dry_powder_cny < 1000.0 { 8 } else { 2 }
l_score = if recent_trade_count_7d >= 5 { 10 } else if recent_trade_count_7d >= 3 { 5 } else { 0 }
execution_red_light_score = c_score + k_score + l_score  // 0-30
→ green (0-10) / yellow (11-20) / red (21-30)
```

### L4 执行检查（Rust 端计算）

```
l4_execution_checks_passed = count(止损明确, 仓位合理, 情绪稳定, 买点合理)  // 0-4
```

---

## Parser 升级（ParsedFields 新增字段）

```rust
// -- Macro 新增 --
pub market_phase: Option<String>,        // "主升" | "分歧" | "退潮" | "冰点" | "混沌"
pub sensitivity: Option<String>,         // "positive" | "negative" | "neutral"
pub sensitivity_reason: Option<String>,  // 一句话原因
pub emotion_temperature: Option<String>, // "乐观" | "中性" | "谨慎" | "恐慌"

// -- Quant 新增 --
pub money_flow: Option<String>,          // 资金流向原始文本
pub buy_point_assessment: Option<String>, // "低吸" | "突破" | "回踩" | "追高" | "不可交易"
pub valuation_assessment: Option<String>, // 估值评估原始文本

// -- Risk 新增 --
pub l4_veto: Option<bool>,               // true=卫语句触发, false=未触发
pub l4_veto_reason: Option<String>,      // 卫语句触发原因
pub emotional_state: Option<String>,     // "stable" | "warning" | "danger"
pub stock_risk_summary: Option<String>,  // 标的风险综合

// -- Risk R2 新增 --
pub l4_veto_r2: Option<bool>,            // R2 复检结果 (true/false)
pub emotion_recalibrated: Option<String>, // R2 情绪重校准

// -- L4 Officer 新增 (新角色) --
pub l4_guard_clause: Option<bool>,       // 卫语句判定 (true/false)
pub l4_guard_reason: Option<String>,     // 卫语句原因
pub l4_emotion_assessment: Option<String>, // "stable" | "warning" | "danger"
pub l4_red_light: Option<String>,        // "green" | "yellow" | "red"
pub l4_buy_point_ok: Option<bool>,       // 买点合理性

// -- CIO 新增 --
pub catalyst_tier: Option<String>,       // "Tier1" | "Tier2" | "Tier3" | "无"
pub catalyst_summary: Option<String>,    // 一句话催化剂摘要
pub l4_check_stop_loss: Option<bool>,
pub l4_check_position: Option<bool>,
pub l4_check_emotion: Option<bool>,
pub l4_check_buy_point: Option<bool>,
pub l4_execution_checks_passed: Option<f64>, // Rust 端计算 (0-4)
pub is_tier1: Option<bool>,
pub tier1_watch_hours: Option<f64>,
pub personal_note: Option<String>,

// -- L4 行为红灯 (Rust 端计算) --
pub execution_red_light_score: Option<f64>, // 0-30
pub red_light: Option<bool>,              // score >= 20
```

### Parser 函数修改

| 函数 | 新增解析 |
|---|---|
| `parse_macro` | +market_phase, +sensitivity, +sensitivity_reason, +emotion_temperature |
| `parse_quant` | +money_flow, +buy_point_assessment, +valuation_assessment |
| `parse_risk` | +l4_veto, +l4_veto_reason, +emotional_state, +stock_risk_summary |
| `parse_risk_r2` | +l4_veto_r2, +emotion_recalibrated |
| `parse_l4_officer` (新增) | +l4_guard_clause, +l4_guard_reason, +l4_emotion_assessment, +l4_red_light, +l4_buy_point_ok |
| `parse_cio` | +catalyst_tier, +catalyst_summary, +l4_check_*, +is_tier1, +tier1_watch_hours, +personal_note |

---

## 新增工具

| 工具 | 归属角色 | 数据源 | 说明 |
|---|---|---|---|
| get_moneyflow | Quant | tushare moneyflow_dc | 个股近5日主力/散户资金流向 |
| get_company_info | ~~CIO~~ Quant/Risk | tushare stock_basic + daily_basic | PE/PB/ROE/市值 |
| get_company_news | Risk | mx-finance search_financial_news | 个股风险新闻 |
| get_recent_events | Macro | event_scanner 存储 | 最近事件列表 |

> 注：get_company_info 不给 CIO（CIO 不接收原始数据），给 Quant（估值评估）和 Risk（标的风险）。

---

## 字符限制变更

| 角色 | 旧值 | 新值 |
|---|---|---|
| Macro | 400 | 600 |
| Quant | 250 | 350 |
| Risk | 250 | 350 |
| L4 Officer | N/A | 250 (新增) |
| CIO | 400 | 600 |

---

## 实施步骤

### 第一批：数据层 + Parser 基础

1. **Tushare moneyflow 客户端方法** — `client.rs`
2. **Tushare daily_basic 扩展** — `client.rs`（PE/PB/ROE/换手率/市值）
3. **Tushare fina_indicator 扩展** — `client.rs`（营收/净利增速）
4. **Tushare report_rc 扩展** — `client.rs`（机构评级聚合）
5. **mx-finance 风险新闻** — `client.rs` 或 tool 层
6. **macro_cache 扩展** — `macro_cache.rs`（涨跌停家数/两市成交额）
7. **AssetContext 构建** — `orchestrator.rs`
8. **count_recent_trades** — `portfolio.rs`
9. **max_drawdown 预计算** — `orchestrator.rs`
10. **ParsedFields 新增字段** — `parser.rs`
11. **hard_truncate 改进** — `roles.rs`（最后一个完整行截断）

### 第二批：Prompt + Parser + Tools

12. **重写全部 7 个 Prompt** — `roles.rs`（含新增 L4 Officer）
13. **新增 L4 Officer 角色** — `roles.rs`（CommitteeRole 枚举、prompt_filename、max_chars）
14. **新增工具** — `tools.rs`（get_moneyflow/get_company_info/get_company_news/get_recent_events）
15. **更新 parser 解析函数** — `parser.rs`（含新增 parse_l4_officer）
16. **扩展 build_context_messages** — `orchestrator.rs`
17. **L4 Officer pipeline 集成** — `orchestrator.rs`（在 Risk R2 之后、CIO 之前）

### 第三批：前端改动

18. **PipelineFlow.svelte** — NODES 数组添加 L4 Officer 节点（risk_r2 和 cio 之间）
19. **DebateBlock.svelte** — ROLE_COLORS 添加 `l4_officer` 颜色
20. **invest-committee-store.svelte.ts** — SymbolProgress 处理 L4 Officer 事件
21. **CommitteeReplayTab.svelte** — ROLES 数组添加 L4 Officer + roleToBackendIdx 更新
22. **CommitteeLiveTab.svelte** — ROLES 数组添加 L4 Officer + roleToBackendIdx 更新
23. **CommitteeRolesTab.svelte** — 角色卡片列表添加 L4 Officer
24. **i18n** — `messages/en.json` + `messages/zh-CN.json` 添加 L4 Officer 相关翻译

### 第四批：验证

27. **编译验证** — `cargo check`
28. **Parser tests** — 各角色新格式解析测试
29. **Context tests** — AssetContext 构建、max_drawdown、count_recent_trades
30. **Tool tests** — 新增工具接口测试
31. **L4 红灯 tests** — emotional_state → 分数映射、红灯阈值
32. **更新 changelog** — `docs/changelog.md`

---

## 前端改动详情

### 1. PipelineFlow.svelte

文件：`src/lib/components/invest/PipelineFlow.svelte`

NODES 数组（`$derived`）添加 L4 Officer 节点：

```typescript
{ role: 'l4_officer', label: t('invest_pipeline_l4_officer'), color: '#ef4444', icon: 'L4' },
```

位置：在 `risk_r2` 和 `cio` 之间。

stepIndex 映射更新：
- L4 Officer → stepIndex 6
- CIO → stepIndex 7（原为 6）

### 2. DebateBlock.svelte

文件：`src/lib/components/invest/DebateBlock.svelte`

ROLE_COLORS 添加：

```typescript
l4_officer: '#ef4444',
```

颜色选择：红色系（与 Risk 的橙色 `#f97316` 区分，L4 是安全阀角色，用红色更直观）。

### 3. events.rs (后端，影响前端)

文件：`src-tauri/src/invest/committee/events.rs`

step_index_for_role 更新：

```rust
(Some(CommitteeRole::L4Officer), false) => 6,
(Some(CommitteeRole::Cio), false) => 7,  // 原来是 6
```

### 4. invest-committee-store.svelte.ts

文件：`src/lib/stores/invest-committee-store.svelte.ts`

SymbolProgress.completedSteps 需要处理 stepIndex=6 (L4 Officer) 和 stepIndex=7 (CIO)。

### 5. CommitteeReplayTab.svelte

文件：`src/lib/components/invest/CommitteeReplayTab.svelte`

ROLES 数组添加：

```typescript
{ key: 'l4_officer', labelKey: 'invest_pipeline_l4_officer' as const, color: '#ef4444', backendIdx: 6 },
```

CIO 的 backendIdx 从 6 改为 7。

roleToBackendIdx 函数添加：

```typescript
if (role === 'l4_officer') return 6;
```

CIO 的 return 从 6 改为 7。

### 6. CommitteeLiveTab.svelte

文件：`src/lib/components/invest/CommitteeLiveTab.svelte`

ROLES 数组添加：

```typescript
{ key: 'l4_officer', labelKey: 'invest_pipeline_l4_officer' as const, color: '#ef4444', backendIdx: 6 },
```

CIO 的 backendIdx 从 6 改为 7。

roleToBackendIdx 函数添加：

```typescript
if (role === 'l4_officer') return 6;
```

CIO 的 return 从 6 改为 7。

### 7. CommitteeRolesTab.svelte

文件：`src/lib/components/invest/CommitteeRolesTab.svelte`

在 `risk` 和 `cio` 之间添加 L4 Officer 角色卡片：

```typescript
{
  key: 'l4_officer',
  color: 'red',
  badge: 'L4',
  nameCn: t('invest_roles_l4_officer_cn'),
  nameEn: 'L4 Execution Officer',
  desc: t('invest_roles_l4_officer_desc'),
  meta: 'temp 0.3 · tools ✗',
  prompts: [{ key: 'l4_officer', label: t('invest_roles_prompt_full') }],
},
```

颜色映射：`red` → `#ef4444`

hard_rules 添加：

```typescript
role.key === 'l4_officer' ? [
  t('invest_roles_hard_l4_1'),
  t('invest_roles_hard_l4_2'),
  t('invest_roles_hard_l4_3'),
] :
```

### 6. i18n

文件：`messages/en.json` 和 `messages/zh-CN.json`

新增键：

```json
"invest_pipeline_l4_officer": "L4 Officer",
"invest_roles_l4_officer_cn": "L4 执行控制官",
"invest_roles_l4_officer_desc": "执行前最后安全阀：guard clause + 情绪评估 + 红灯评分 + 买点合理性检查",
"invest_roles_hard_l4_1": "guard clause 任一触发 → verdict 不高于 HOLD",
"invest_roles_hard_l4_2": "execution_red_light = red → 红灯锁定",
"invest_roles_hard_l4_3": "买点合理性 check 3 项：价格位置/止损空间/赔率"
```

---

## 已确认决定

1. ✅ 标的敏感度 → 保留在 Macro
2. ✅ L4 行为红灯评分 → 完整实现（新增 L4 Officer 角色）
3. ✅ macro_cache 扩展 → 直接做
4. ✅ Tool + Context 双路径 → 两个都做
5. ✅ Prompt 模板 → 已缓存到 `docs/superpowers/plans/committee-prompts-v2.md`
6. ✅ CIO → 纯综合裁决，不接收原始数据注入
7. ✅ 数据注入层 → 从 stock-data-report 补全（PE/PB/ROE/增速/评级/风险新闻）
8. ✅ L4 Officer → 新增角色，在 Risk R2 之后、CIO 之前
