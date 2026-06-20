# 委员会直播修复 + invest Dashboard 优化 — 设计文档

**日期:** 2026-06-18
**模块:** invest(委员会直播 / Dashboard)
**状态:** 设计已确认,待 review

## 背景

本次迭代横跨 invest 模块两个独立 UI 区域,合并为一份 spec 统一 review,实现阶段拆为有序的两个 plan:

- **Part A — 委员会直播(CommitteeLiveTab)**:6 项修复,解决切页面丢数据、操作交互冗余、卡片排版缺失、解析误报、输出过长等问题。
- **Part B — invest Dashboard**:4 项优化,KPI 卡重构 + 持仓明细表扩展(含 A 股 T+1 冻结)+ 委员会评级预测列 + 归档文件名带名称。

所有字段取值(signal / verdict 等枚举)保持英文原样显示,与 `docs/demo-committee-live.html` 对齐。

---

# Part A — 委员会直播修复

涉及文件:
- 前端组件 `src/lib/components/invest/CommitteeLiveTab.svelte`
- 前端配置 `src/lib/components/invest/pipeline-config.ts`
- 前端 store `src/lib/stores/invest-committee-store.svelte.ts`
- 后端队列 `src-tauri/src/invest/committee/queue.rs`
- 后端 parser `src-tauri/src/invest/committee/parser.rs`
- 后端 prompts `src-tauri/src/invest/committee/roles.rs`
- 参考 `docs/demo-committee-live.html`

## A1. 跨重启恢复完整进度

### 问题根因

`loadQueue()` 在组件每次 `onMount` 时,用磁盘队列重建一份**空的** `SymbolProgress`,覆盖内存里已跑完的结果。磁盘只存了 `status`(done/failed),没存卡片内容。因此切回页面后 status 仍是 done(只剩重试按钮),但 `result` / `completedRounds` / `regimeData` 全空。

### 设计(方案 A:queue.rs 哑存储,前端 store 拥有 schema)

**存盘扩展**:`CommitteeQueueState.items` 的每个 `QueueItem` 增加可选字段 `progress`。`PersistedProgress` 是 `SymbolProgress` 的可序列化子集:
- `completedRounds`(各角色 `RoundOutputSummary`)
- `result`(`CommitteeResult`)
- `regimeData`(`RegimeStepData`)
- `failedSteps`(Set → 数组)
- `completedSteps`(number)
- 运行中的瞬时字段 `activeStep` **不存**。

**后端**:`queue.rs` 的 `QueueItem` 增加 `progress: Option<serde_json::Value>`,纯透传,后端不解析结构。符合"哑存储 + 前端拥有 schema"的边界。

**写入时机**:复用现有 300ms debounce 的 `_persistQueue()`。`role_complete` / `symbol_complete` / `regime_step` 已触发 store 更新的地方,顺带把 progress 一起序列化。不新增写盘调用点。

**加载恢复**:`loadQueue()` 改为——若 item 带 `progress`,用它重建带内容的 `SymbolProgress`(`failedSteps` 数组转回 Set);没有则退回当前空进度逻辑。`running` 状态的 item 仍降级为 `queued`(进程重启保护),但已有 progress 内容保留,断点续跑时旧卡片不闪空。

**边界情况**:
- 旧 queue.json 无 `progress` 字段 → serde 默认 `None`,自动退回空进度,无需迁移。
- result 内 `rawText` 可能较长 → 单标的几十 KB,5–10 标的共几百 KB,单用户桌面应用可接受。

## A2. 操作栏改造 + 卡片独立运行按钮

### 决定

取消多选框、运行所选、全选;每张卡片加独立"运行/中止"切换按钮。

### 设计

**顶部操作栏 — 删除**:
- `▶ 运行选中` 按钮及 `runSelected()`
- `全选` 复选框及 `toggleAll()`
- 卡片头部复选框及 `toggleSel()`、`selectedSymbols` 状态

**顶部操作栏 — 保留**:
- `⏵ 全部运行`(`runAll()`)
- `⏹ 中止全部`(运行中显示)
- 并发选择器(1/2/3/5/8/10)
- Include Watch 复选框
- 进度文字

**卡片头部按钮(状态机)**:每张卡片右侧按 queue 状态显示**单个**主操作按钮:
- 未运行 / queued / done / failed / aborted → **运行按钮(▶)**,点击 `store.addToQueue([symbol], buildSnapshot())`。对已完成标的天然即"重新运行",原重试按钮(↻)合并进来,不再单列。
- running → **中止按钮(⏹)**,点击 `store.abortSymbol(symbol)`。

**点击区域**:卡片头部整体仍是展开/收起触发区;运行/中止按钮 `stopPropagation` 不触发展开(沿用现有写法)。

**快照语义**:`addToQueue` 仅在 `!this.portfolioSnapshot` 时存第一次快照,后续运行复用——符合"一次委员会会议共享同一组合状态"语义,保持不变。

## A3. 解析错误处理 — 弱化 + 增强

### 问题根因

`raw_text` 一直保留,但 fallback 来自 `detect_fallback_reason` 检测到关键字段(signal/regime/verdict)提取失败。当 LLM 把信号写进句子、或字段名变体未覆盖时,提取返回 `None`,整张卡片被标成 fallback 黄条,即使原文完全可读。

### 设计

**A. 弱化(前端展示层)** — 改 `stepCard` snippet 渲染优先级:
- 仅 `worker_unavailable` / `empty_text` / `cli_error` / `cli_executor_none` 这类**真正无内容**的 fallback,才显示警告条。
- `missing_critical_fields`(有原文、仅未提到字段)→ **正常渲染 rawText**,只在 step-meta 区附一个不显眼的 `⚠` icon + tooltip("部分字段未识别"),不再用黄条盖住内容。

**B. 增强(后端 parser 层)** — 完整对账,降低误判:
对每个角色做 prompt 输出字段名 ↔ parser 提取键 ↔ 前端展示三方对齐,补齐缺口。已知需对齐项(读 roles.rs / parser.rs 发现):
- **Macro**:prompt 输出"信号理由/市场阶段理由"等字段,parser 需补 `signal_reason` / `market_phase_reason` 提取(或确认归入现有字段);确保 `signal` 命中"信号"行。
- **Quant**:prompt 输出"市场状态/资金流向/估值评估/买点评估/关键数据/一句话",parser 已覆盖,核对键名一致。
- **Risk**:prompt 输出"风险信号/集中度/可用子弹/盈亏比/最大回撤/标的风险/L4否决",parser 已覆盖,核对 R2"调整风险信号"等变体。
- **CIO**:prompt 输出"执行模式/首笔金额",parser **未覆盖** → 补 `execution_mode` / `first_tranche_cny` 提取(供 A6 关键 chip 与归档使用)。

对账原则:三个决定 fallback 的关键字段(signal/regime/verdict)提取必须稳;其余字段补齐供 A6 chip 使用。新增字段需补单元测试(中英文键名 + 格式变体)。

## A4. 精简输出 — prompt 改造

### 要求

每条理由一句话结束;一个卡片里理由不超过 3 条。

### 设计

**A. 修改内置 prompt 模板**(`roles.rs`:`MACRO_PROMPT` / `QUANT_PROMPT` / `QUANT_R2_PROMPT` / `RISK_PROMPT` / `RISK_R2_PROMPT` / `CIO_PROMPT`):
在每个 prompt 的"输出要求"段加入统一硬约束:
- 每个字段值必须一句话结束,不分点、不展开、不换行续写。
- 理由类**自由文本**字段(reasoning / 一句话 / 各 *_理由)最多 3 条要点,每条一句话。
- 统一现有零散措辞(如 Macro"一句话≤20字")到所有角色。

**约束范围澄清**:"一句话 / ≤3 条"针对 reasoning 这类自由文本理由字段。结构化列表字段(如 Quant `KEY_DATA` 现为"最多 2 条")保持其原有条数约束不变,与 demo 一致。两类约束不冲突。

直接改内置常量(用户不自定义),顺带减少 token 占用。`length_constraint_suffix` 路径**不改**(决定:不走 suffix)。

**B. 不做硬截断**:v5.4.1 已移除 `hard_truncate` / `max_chars`,后端不重新引入字符级截断,纯靠 prompt 约束。

**C. 前端兜底(配合 A5)**:`step-body` 已有 `max-height: 320px` + `overflow-y: auto`;原文完整保留可滚动,不加截断。

## A5. 卡片排版修复

### 问题根因

`.step-body` 走 `renderMarkdown` 渲染,但 LLM 输出是多行 `KEY: 值`(非真 markdown),换行被折叠成一坨。

### 设计(纯前端)

确保渲染结果保留换行:渲染前把裸换行转为合适的块级结构(`<br>` / 段落),或给容器补 `white-space` 处理。A4 让输出更短更规整后,排版压力同步减小。

## A6. 关键 chip + demo 视觉对齐

### 前提

后端 `RoundOutputSummary.parsed` 类型即完整 `ParsedFields`,且带 `#[serde(rename_all = "camelCase")]`——**所有字段已序列化发到前端**,前端仅 TS 接口声明了 9 个、组件未渲染。本项基本是纯前端,后端结构不动(仅 A3 补的字段除外)。

### 设计(纯前端)

**扩展前端 `RoundOutputSummary.parsed` TS 接口**,声明后端已发来的字段,按角色在 step-body 顶部渲染 chip:
- **Macro**:signal(risk_on/off chip)、strength、市场阶段、情绪温度
- **Quant R1/R2**:signal + strength chip、买点评估、估值评估、资金流向
- **Risk R1/R2**:风险信号 chip、集中度、可用子弹、盈亏比、标的风险
- **Regime**:已有(RSI/MA/Vol/分位),对齐 demo 的 3 格 metric 网格样式
- **CIO**:verdict chip、置信度、催化剂层级(完整 verdict 仍在底部 verdict-block)

chip 下接精简后的 `rawText` 正文。

**demo 视觉细节对齐**(参照 `docs/demo-committee-live.html` 现成 CSS):signal chip 配色(sig-buy/sig-sell 等)、regime metric 网格、tool-strip、verdict-block。

---

# Part B — invest Dashboard 优化

涉及文件:
- 前端页面 `src/routes/invest/+page.svelte`(dashboard tab,约 163–195 行)
- 前端组件 `src/lib/components/invest/HoldingsTable.svelte`
- 前端 store `src/lib/stores/invest-store.svelte.ts`
- 后端持仓 `src-tauri/src/storage/invest/portfolio.rs`
- 后端裁决 `src-tauri/src/storage/invest/verdicts.rs`
- 后端归档 `src-tauri/src/invest/committee/archive.rs`

## B1. KPI 卡重构

### 设计(`src/routes/invest/+page.svelte` dashboard tab)

- **保留不变**:总资产、持仓市值、现金余额、持仓数量
- **改造**:总收益率 → **总收益**(大字显示金额 `holdingsMarketValue - totalCostBasis`,小字显示百分比 `totalReturnPct`)
- **新增**:**当日收益**(大字金额 + 小字百分比)
- **删除**:宏观快照卡(`MacroSnapshotCard`)、最新裁决卡(`LatestVerdictCard`)

**当日收益数据源**:前端实时聚合,**不读 pnlSnapshots 快照**(快照定时/手动触发,不实时):
- 金额 = `Σ (priceMap[sym].change × shares)`(仅 hold 持仓;`change` 为现成派生值 = close − preClose)
- 百分比 = 当日收益金额 / `Σ (priceMap[sym].close × shares − priceMap[sym].change × shares)` × 100,即金额 / 昨收总市值。`preClose` 不直接暴露,用 `close − change` 还原昨收,避免依赖未暴露字段。

store 现成派生值已确认可用:`holdingsMarketValue` / `totalAssets` / `totalCostBasis` / `totalReturnPct`,priceMap 的 `change` / `pctChg`。总收益金额 = `holdingsMarketValue − totalCostBasis`,百分比复用 `totalReturnPct`。

被删除组件 `MacroSnapshotCard` / `LatestVerdictCard` 若无其他引用,可保留文件但移除 dashboard 引用(避免误删被复用组件;实现阶段确认引用计数)。

## B2. HoldingsTable 扩展为持仓明细表

### 字段清单与数据来源

当前 7 列扩展为完整明细。纯前端计算项(数据已在 store):

| 字段 | 来源 / 计算 |
|------|------------|
| 标的名称 / 代码 | `h.name` / `h.symbol`(现成) |
| 资产类型 | `h.assetType`(现成,stock/etf badge) |
| 股票/ETF 余额(持仓数量) | `h.shares`(现成) |
| 可用余额(可卖数量) | `h.shares - frozen_shares`(见 B3) |
| 冻结数量 | `frozen_shares`(见 B3,后端新增) |
| 成本价 | `h.avgCost`(现成) |
| 市价 | `priceMap[sym].close`(现成) |
| 盈亏 | `(close - avgCost) × shares` |
| 盈亏比例 | `(close - avgCost) / avgCost × 100` |
| 当日盈亏 | `priceMap[sym].change × shares` |
| 当日盈亏比 | `priceMap[sym].pctChg`(现成) |
| 市值 | `close × shares`(回退 `h.notional`) |
| 仓位占比 | `市值 / totalAssets × 100` |
| 当日买入(数量) | trades 聚合:`action=buy ∧ trade_date=今日` 的 shares 之和 |
| 当日卖出(数量) | trades 聚合:`action=sell ∧ trade_date=今日` 的 shares 之和 |
| 评级预测 | 见 B4 |

**当日买入/卖出聚合**:前端从 `investStore.trades`(已加载最近 200 条)按 `trade_date ?? createdAt[:10] === 今日交易日` 过滤,按 symbol reduce 求和。今日交易日用现有前端日期工具(对齐 `getInvestDate` 5AM 截止规则)。

**布局**:列数较多,表格需横向可读性处理(分组表头或紧凑列宽);实现阶段按 demo 暖色暗黑设计系统([data-invest-scope])定列样式。watch 类持仓无 shares/成本,相关列显示 `—`。

## B3. T+1 冻结(后端真字段)

### 设计

**数据结构**:`Holding` 结构体(`portfolio.rs`)新增 `frozen_shares: Option<f64>`(默认 0),DB 迁移加列(默认 0,宽容迁移)。

**结算逻辑**:
- `record_trade` 处理 `buy` 时,把本次买入股数累加到该持仓的 `frozen_shares`。
- 可卖数量 = `shares - frozen_shares`(前端展示用,不单独存)。

**解冻时机(惰性解冻,不新增定时任务)**:
- 冻结记录关联其来源交易日。读取持仓时(如 `list_holdings`),若 `frozen_shares` 的来源交易日 < 今日交易日(用 `date_utils::get_invest_date`),视为已解冻 → 归零。
- 实现:在 Holding 上记录冻结来源日期(可复用 `updated_at` 或新增 `frozen_date` 字段);读取时比对今日交易日。新增 `frozen_date: Option<String>` 更清晰,避免 `updated_at` 被其他写入污染。
- 决定:新增 `frozen_date` 字段,惰性解冻在读取路径判断。

**边界**:
- 当日多笔买入累加;当日卖出**不**减少 frozen(卖的是 T+1 之前的可用持仓,A 股规则)。
- 跨交易日后首次读取触发归零,并持久化回写(避免每次读都重算)。
- 旧数据无 `frozen_shares` / `frozen_date` → 迁移默认 NULL/0,视为无冻结。

## B4. 委员会评级预测列

### 设计

HoldingsTable 新增"评级"列,显示该 symbol 最近一条委员会 verdict,**仅当其新鲜(近一个交易日内)时显示,过期不显示**。

**数据源**:`verdicts` 表已有 `symbol/name/verdict/confidence/created_at`,`list_verdicts(symbol, 1)` 现成可查最新一条。

**新鲜度过滤**:`created_at >= 上一交易日起点`。"上一交易日"用现有交易日历 / `date_utils` 推算;过期(更早)则该列显示 `—`,不展示陈旧评级。

**前端获取**:
- 方案:页面加载时批量取各持仓 symbol 的最新 verdict。可新增一个 Tauri 命令 `get_latest_verdicts(symbols)` 返回 map,或前端对持仓列表逐个调 `list_verdicts(symbol, 1)`(持仓数通常 < 20,可接受)。
- 决定:新增 `get_latest_verdicts(symbols: Vec<String>)` 批量命令,一次返回 `symbol → 最新 Verdict`,避免 N 次 IPC;前端按新鲜度过滤后渲染 verdict chip(复用 `getVerdictBadgeStyle`)。

**展示**:新鲜则显示 verdict chip(BUY/ACCUMULATE/HOLD/TRIM/SELL,英文原样 + 现有配色);过期或无记录显示 `—`。

## B5. 归档文件名带股票名称

### 问题

`archive.rs` 用 `format!("{symbol}.md")` 命名归档,文件名只有代码无名称,不易辨识。

### 设计

**文件名格式**:`{symbol}_{safe_name}.md`(如 `600519.SH_贵州茅台.md`)。

**名称安全处理**:`validate_symbol` 仅允许 ASCII,但中文名合法存在于文件名中(Windows NTFS 支持)。新增 `sanitize_name_for_filename`:去除路径分隔符 `/ \ : * ? " < > |` 等文件系统非法字符,保留中文;name 为空时回退纯 `{symbol}.md`。

**读取兼容**:`load_archive` 当前按精确 `{symbol}.md` 查找,需改为匹配 `{symbol}_*.md` **和** 旧格式 `{symbol}.md`(向后兼容历史归档)。实现:扫描日期目录,匹配以 `{symbol}.md` 或 `{symbol}_` 开头的 .md 文件。

**name 来源**:`archive_decision_full` 调用处传入 symbol 对应的 name(从持仓 / verdict / CommitteeResult 取);若拿不到 name 则回退纯 symbol 文件名。实现阶段确认 orchestrator 归档调用点能否拿到 name。

---

## 实现顺序建议(供 writing-plans 拆分)

**Plan 1 — 委员会直播(Part A)**:A1(持久化)→ A2(操作栏)→ A3(解析对账)→ A4(prompt 精简)→ A5(排版)→ A6(chip + 视觉)。A3 的字段对账为 A6 chip 提供数据,需先行。

**Plan 2 — Dashboard(Part B)**:B3(T+1 后端字段)→ B2(明细表,依赖 B3 的可卖/冻结)→ B1(KPI 卡)→ B4(评级列)→ B5(归档文件名)。B3 是 B2 的前置。

每个 plan 完成后按 CLAUDE.md 标准工作流:simplify 审查 → 修复 → commit → `npm run build` / `npm run i18n:check` / 相关测试验证。新增 UI 文案需同步 `messages/en.json` 与 `messages/zh-CN.json`。
