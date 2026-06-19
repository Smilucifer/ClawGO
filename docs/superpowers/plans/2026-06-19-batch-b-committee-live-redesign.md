# 批次 B — 委员会直播 UI 重设计 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 重设计委员会直播卡片:头部行加入 REGIME chip 并瘦化进度条(任务3),展开体移除独立 REGIME 框、宏观/CIO/判决全宽、内部内容改用结构化字段卡渲染(方案A)、宽度响应式自适应(任务4)。

**Architecture:** 全部改动集中在单文件 `src/lib/components/invest/CommitteeLiveTab.svelte`。采用就地编辑现有 snippet(`pipelineBar`、`stepCard`)与 card-header 模板及 `<style>` 块,不拆分独立组件文件——snippet 已是封装单元,就地改动 diff 更可审、避免 props 穿线。数据来源不变(`SymbolProgress.regimeData` / `RoundOutputSummary.parsed`)。

**Tech Stack:** Svelte 5 runes、CSS(媒体查询 + color-mix + CSS 变量)、项目 i18n(en.json + zh-CN.json)。

## Global Constraints

- 关联 spec:`docs/superpowers/specs/2026-06-19-multi-module-maintenance-design.md`(批次 B 节);视觉参照 `docs/superpowers/specs/committee-live-redesign-mockup.html`(头部布局已确认、内部用方案A、unknown 淡化、响应式断点)。
- **不触碰** tool-strip(`CommitteeLiveTab.svelte:410-417`)及 `toolMap`(`:87-95`)——其移除属批次 C(C1),批次 B 保持原样以隔离改动。
- 任何新增/改动的 UI 文案必须同时更新 `messages/en.json` 与 `zh-CN.json`,并通过 `npm run i18n:check`。
- REGIME `unknown` 或 `regimeData` 为空时:头部 chip **淡化显示**(降不透明度),不隐藏(用户已确认)。
- 进度条保持 7 段(STEP_DEFS 不变),仅改宽度与布局;regime 仍是 pipeline 的一个 step(进度条第 2 段保留)。
- 验证以 `npm run check`(svelte-check)+ `npm run build` + 运行 app 目测为准。
- Conventional Commits。

## File Structure

唯一改动文件:`src/lib/components/invest/CommitteeLiveTab.svelte`,分区域:
- `card-header` 模板(`:297-332`)— 任务 B1:插入 REGIME chip + spacer 重排。
- `pipelineBar` snippet(`:143-152`)+ `.pipeline-bar` CSS(`:563-578`)— 任务 B1:瘦化。
- `stepCard` snippet(`:154-216`)— 任务 B2:删 regime 分支、rawText 分支改方案A。
- flow-grid 模板(`:337-368`)+ `.flow-grid`/`.fw`/`.verdict-block` CSS(`:581-591`, `:656`)— 任务 B2:删 regime 格、全宽块占满。
- `@media` + chip/field CSS — 任务 B3:响应式断点。
- i18n 文件 — 新增字段标签键。

---

## Task 1: 头部行 REGIME chip + 进度条瘦化(任务 B1)

**Files:**
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`(card-header 模板 `:297-332`、`pipelineBar` snippet `:143-152`、CSS `:493-567`)
- Modify: `messages/en.json`、`messages/zh-CN.json`(新增 regime chip 文案键)

**Interfaces:**
- Consumes: `SymbolProgress.regimeData`(类型 `RegimeStepData | null`,含 `regime: string`、`metrics: { rsi14, ma20, ma60, latest, volatilityAnn, priceQuantile2y }`、`strategyHint`)。
- Produces: 新 `regimeChip(p)` snippet;头部行新布局 class `regime-chip`、`.regime-chip.unknown`。

- [ ] **Step 1: 新增 `regimeChip` snippet**

在 `pipelineBar` snippet 之后(`:152` 之后)插入新 snippet。MA20 方向由 `latest` 与 `ma20` 比较得出箭头:

```svelte
{#snippet regimeChip(p: SymbolProgress | undefined)}
  {#if p?.regimeData}
    {@const rd = p.regimeData}
    {@const isUnknown = rd.regime === 'unknown' || rd.regime === ''}
    {@const ma20Dir = rd.metrics.latest >= rd.metrics.ma20 ? '↑' : '↓'}
    <div class="regime-chip" class:unknown={isUnknown} title={rd.strategyHint}>
      <span class="rg-tag">{rd.regime || t('invest_regime_unknown')}</span>
      <span class="rg-metrics">
        <span>RSI {rd.metrics.rsi14.toFixed(0)}</span>
        <span>MA20{ma20Dir}</span>
        <span>{(rd.metrics.priceQuantile2y * 100).toFixed(0)}%</span>
      </span>
    </div>
  {/if}
{/snippet}
```

- [ ] **Step 2: 重排 card-header 模板**

把 `card-header`(`:297-332`)按新顺序改为 `名字 | 持仓/观察 | REGIME chip | spacer | 进度条 | 判断 | 运行 | 展开`。替换 `:298-303`(card-id + badge + pipelineBar 之间):

```svelte
      <div class="card-id">
        <span class="card-name">{asset.name ?? asset.symbol}</span>
        <span class="card-ticker">{asset.symbol}</span>
      </div>
      <span class="badge {asset.kind}">{asset.kind === 'hold' ? 'HOLD' : 'WATCH'}</span>
      {@render regimeChip(p)}
      <span class="header-spacer"></span>
      {@render pipelineBar(p)}
```

(其余 `:304-331`——verdict-badge-sm、abort/run 按钮、expand-arrow——保持不变。)

- [ ] **Step 3: 进度条瘦化 CSS**

替换 `.pipeline-bar` 规则(`:563-567`):

```css
  /* Pipeline bar — fixed slim width, pushed right by header-spacer */
  .pipeline-bar {
    display: flex; height: 14px; width: 148px; flex: 0 0 auto;
    border-radius: var(--radius-sm); overflow: hidden;
    background: var(--bg-input); gap: 2px;
  }
```

(`.seg` 规则 `:568-578` 不变;`seg-pulse` 不变。)

- [ ] **Step 4: 新增 header-spacer + regime-chip CSS**

在 `.card-ticker` 规则之后(`:505` 之后)插入:

```css
  .header-spacer { flex: 1 1 auto; }
  .regime-chip {
    --rg-color: #c084fc;  /* regime 紫,独立语义;不复用 --color-quant(那是 quant 蓝 #3b82f6) */
    display: flex; align-items: center; gap: 7px; flex-shrink: 0;
    padding: 3px 10px; border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--rg-color) 13%, transparent);
    border: 1px solid color-mix(in srgb, var(--rg-color) 30%, transparent);
    white-space: nowrap;
  }
  .regime-chip.unknown { opacity: 0.45; }
  .regime-chip .rg-tag {
    font-size: 11px; font-weight: 700;
    color: var(--rg-color);
    text-transform: uppercase; letter-spacing: 0.3px;
  }
  .regime-chip .rg-metrics {
    display: flex; gap: 8px; font-size: 10.5px;
    font-family: var(--font-mono); color: var(--text-tertiary);
  }
```

- [ ] **Step 5: 新增 i18n 键**

在 `messages/zh-CN.json` 与 `en.json` 各加一个键(若 `invest_regime_unknown` 已存在则复用,先 grep 确认):

zh-CN.json:
```json
  "invest_regime_unknown": "未知",
```
en.json:
```json
  "invest_regime_unknown": "Unknown",
```

- [ ] **Step 6: 验证编译 + i18n**

Run:
```bash
npm run check
npm run i18n:check
```
Expected: svelte-check 0 errors;i18n 校验通过(无缺键)。

- [ ] **Step 7: 运行目测**

`npm run tauri dev` → /invest → 委员会 → 直播。确认:
- 头部行顺序为 `名字|HOLD/WATCH|REGIME chip|空档|瘦进度条|判断|运行|展开`。
- 有 regimeData 的卡片显示紫色 chip(状态名 + RSI/MA20方向/分位);进度条不再撑满中间、固定在右侧。
- `unknown`/无数据卡片 chip 淡化。

- [ ] **Step 8: Commit**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte messages/en.json messages/zh-CN.json
git commit -m "feat(committee): 直播卡头部行重排 — REGIME chip 上移 + 进度条瘦化"
```

---

## Task 2: 展开体移除 REGIME 框 + 全宽块占满(任务 B2 布局部分)

**Files:**
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`(flow-grid 模板 `:337-339`、`stepCard` regime 分支 `:182-194`、`.flow-grid .fw` CSS `:585`、`.verdict-block` CSS `:656`、`@media` `:588-591`)

**Interfaces:**
- Consumes: 无新增。
- Produces: 全宽块(macro/cio/verdict)占满 grid 两列宽度(去除 65% 居中约束)。

- [ ] **Step 1: 从 flow-grid 删除 regime 格**

删除 `:339` 这一行(regime step-card 渲染):

```svelte
            <div class="fw">{@render stepCard('regime', p)}</div>
```

保留 `:338` 的 macro fw 和其后的 connector(`:341-346`)。

- [ ] **Step 2: 从 stepCard 删除 regime 分支**

删除 `stepCard` snippet 中的 regime 分支(`:182-194`,整段 `{:else if stepKey === 'regime' && p?.regimeData} ... </div>`)。删除后该 `{#if}` 链直接从 fallback-message 分支跳到 `{:else if round?.parsed?.rawText}` 分支。

- [ ] **Step 3: 全宽块占满 CSS**

替换 `.flow-grid .fw`(`:585`):

```css
  .flow-grid .fw { grid-column: 1 / -1; width: 100%; }
```

替换 `.verdict-block` 的宽度约束(`:656` 行内 `width: 65%; max-width: 560px; min-width: 320px;`)——把该行改为全宽:

```css
  .verdict-block {
    grid-column: 1 / -1; width: 100%;
    background: var(--bg-base); border: 1px solid var(--accent-muted); border-radius: var(--radius-md);
    padding: 14px 16px; display: flex; flex-direction: column; gap: 10px;
  }
```

(删除原 `justify-self: center; width: 65%; max-width: 560px; min-width: 320px;`。)

- [ ] **Step 4: 更新窄屏媒体查询**

`@media (max-width: 700px)`(`:588-591`)当前含 `.flow-grid .fw { width: 100%; max-width: none; }`——因 Step3 已把 fw 设为 width:100%,该条冗余但无害;保留单列规则即可。改为:

```css
  @media (max-width: 700px) {
    .flow-grid { grid-template-columns: 1fr; }
  }
```

- [ ] **Step 5: 验证 + 目测**

Run: `npm run check`
然后 `npm run tauri dev` 目测:展开卡片后**不再有** REGIME 框;宏观/CIO/最终判决占满整行宽度(与量化+风控两列之和等宽);量化/风控仍两列对半。

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte
git commit -m "feat(committee): 展开体移除 REGIME 框,宏观/CIO/判决全宽占满"
```

---

## Task 3: 卡片内部改用结构化字段卡(方案A)(任务 B2 渲染部分)

**Files:**
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`(`stepCard` 的 rawText 分支 `:195-210`、新增 `roleFields` 辅助、CSS 新增字段表样式、`.step-body` 的 `white-space` `:623`)
- Modify: `messages/en.json`、`zh-CN.json`(字段标签键)

**Interfaces:**
- Consumes: `RoundOutputSummary.parsed`(字段:`signal, strength, verdict, confidence, oneLiner, reasoning, marketPhase, emotionTemperature, marketPhaseReason, buyPointAssessment, valuationAssessment, moneyFlow, concentrationPct, dryPowderCny, pnlPct, stockRiskSummary, catalystTier, catalystSummary, executionMode, firstTrancheCny, rawText, fallbackReason`),`def.key`(角色键:`macro|quant_r1|quant_r2|risk_r1|risk_r2|cio`)。
- Produces: 结构化字段视图;fallback 时退回 `rawText`。

**设计:** 用脚本辅助函数 `roleFields(stepKey, parsed)` 返回 `{label, value}[]`,按角色挑字段;模板渲染 signal/置信度 chip + oneLiner + 字段表 + reasoning。`rawText` 仅当无任何结构化字段可显示时兜底。

- [ ] **Step 1: 新增 `roleFields` 脚本辅助**

在 `<script>` 块内 `segIcon` 函数之后(`:37` 之后)插入。字段值做空值过滤,数字字段带单位:

```ts
  // Import RoundOutputSummary type alongside the existing store imports:
  //   import { ..., type RoundOutputSummary } from '$lib/stores/invest-committee-store.svelte';
  type ParsedFields = RoundOutputSummary['parsed'];

  /** True if a parsed role output has any displayable content (parsed is a
   *  required object, so a truthiness check on it alone is always true). */
  function hasParsedContent(pf: ParsedFields): boolean {
    return !!(pf.signal || pf.verdict || pf.oneLiner || pf.reasoning || pf.rawText
      || pf.strength != null || pf.confidence != null || pf.fallbackReason
      || pf.marketPhase || pf.emotionTemperature || pf.buyPointAssessment
      || pf.valuationAssessment || pf.moneyFlow || pf.concentrationPct != null
      || pf.dryPowderCny != null || pf.pnlPct != null || pf.stockRiskSummary
      || pf.catalystTier || pf.catalystSummary || pf.executionMode || pf.firstTrancheCny != null);
  }

  function roleFields(stepKey: string, pf: ParsedFields): { label: string; value: string }[] {
    const out: { label: string; value: string }[] = [];
    const push = (label: string, v: unknown, suffix = '') => {
      if (v === null || v === undefined || v === '') return;
      out.push({ label, value: `${v}${suffix}` });
    };
    const role = stepKey.startsWith('quant') ? 'quant'
      : stepKey.startsWith('risk') ? 'risk'
      : stepKey;
    if (role === 'macro') {
      push(t('invest_field_market_phase'), pf.marketPhase);
      push(t('invest_field_emotion'), pf.emotionTemperature);
      push(t('invest_field_phase_reason'), pf.marketPhaseReason);
    } else if (role === 'quant') {
      push(t('invest_field_buy_point'), pf.buyPointAssessment);
      push(t('invest_field_valuation'), pf.valuationAssessment);
      push(t('invest_field_money_flow'), pf.moneyFlow);
    } else if (role === 'risk') {
      push(t('invest_field_concentration'), pf.concentrationPct, '%');
      push(t('invest_field_dry_powder'), pf.dryPowderCny != null ? `¥${pf.dryPowderCny}` : null);
      push(t('invest_field_pnl'), pf.pnlPct != null ? `${pf.pnlPct}%` : null);
      push(t('invest_field_stock_risk'), pf.stockRiskSummary);
    } else if (role === 'cio') {
      push(t('invest_field_catalyst_tier'), pf.catalystTier);
      push(t('invest_field_exec_mode'), pf.executionMode);
      push(t('invest_field_first_tranche'), pf.firstTrancheCny != null ? `¥${pf.firstTrancheCny}` : null);
      push(t('invest_field_catalyst'), pf.catalystSummary);
    }
    return out;
  }
```

> 类型别名用 `RoundOutputSummary['parsed']`(该 interface 已在 store 顶层 export),比 `NonNullable<SymbolProgress['result']>['rounds'][number]['parsed']` 更短更稳。记得在 import 里加 `type RoundOutputSummary`。

- [ ] **Step 2: 替换 rawText 分支为结构化渲染**

把 `stepCard` 的 `{:else if round?.parsed?.rawText}` 分支(`:195-210`)替换为方案A 渲染。保留原 chip-row 的 signal/strength/verdict/confidence chips,新增 oneLiner + 字段表 + reasoning,rawText 仅在无字段且无 oneLiner 时兜底:

```svelte
      {:else if round?.parsed && hasParsedContent(round.parsed)}
        {@const pf = round.parsed}
        {@const fields = roleFields(stepKey, pf)}
        <div class="chip-row">
          {#if pf.signal}<span class="chip sig-{pf.signal.toLowerCase()}">{pf.signal}</span>{/if}
          {#if pf.strength != null}<span class="chip neutral">{t('invest_committee_chip_strength')} {pf.strength}</span>{/if}
          {#if pf.verdict}<span class="chip" style={getVerdictBadgeStyle(pf.verdict)}>{pf.verdict}</span>{/if}
          {#if pf.confidence != null}<span class="chip neutral">{normalizeConfidencePct(pf.confidence).toFixed(0)}%</span>{/if}
          {#if pf.fallbackReason}<span class="chip warn" title={pf.fallbackReason}>⚠ {t('invest_committee_chip_fields_partial')}</span>{/if}
        </div>
        {#if pf.confidence != null}
          <div class="confidence-meter"><i style="width:{normalizeConfidencePct(pf.confidence)}%"></i></div>
        {/if}
        {#if pf.oneLiner}<div class="role-oneliner">{pf.oneLiner}</div>{/if}
        {#if fields.length > 0}
          <div class="field-list">
            {#each fields as f}
              <div class="field"><span class="field-k">{f.label}</span><span class="field-v">{f.value}</span></div>
            {/each}
          </div>
        {/if}
        {#if pf.reasoning}<div class="role-reasoning">{pf.reasoning}</div>{/if}
        {#if fields.length === 0 && !pf.oneLiner && !pf.reasoning && pf.rawText}
          <div class="raw-text">{pf.rawText}</div>
        {/if}
      {:else}
        <span class="muted">{t('invest_committee_waiting')}</span>
```

> **关键修正(原 rawText 分支判定失效):** store 类型里 `RoundOutputSummary.parsed` 是**必填对象**(非 optional),因此 `round?.parsed` 在 round 非空时**恒为 truthy**——若直接用它做条件,会让"parsed 存在但所有字段为空"的情况也进入该分支,渲染出只剩空 chip-row 的半截卡片。故改用 `round?.parsed && hasParsedContent(round.parsed)` 守门,无内容时落到末尾 `{:else}` 的 waiting 占位(与原行为一致)。`hasParsedContent` 在 Step1a 定义。

- [ ] **Step 3: 关闭 step-body 的全局 pre-wrap,改为字段表分层**

`.step-body`(`:620-624`)当前有 `white-space: pre-wrap;`,会影响新结构化布局。把它移除,只让 `.raw-text`(兜底)与 `.role-reasoning` 保留换行语义。替换 `.step-body`:

```css
  .step-body {
    padding: 14px; font-size: 12.5px; color: var(--text-secondary); line-height: 1.7;
    max-height: 320px; overflow-y: auto; word-break: break-word;
  }
```

(`.raw-text` 已有 `white-space: pre-wrap`(`:551`),保留。)

- [ ] **Step 4: 新增方案A 字段表/oneLiner/meter CSS**

在 chip 样式之后(`:652` 之后)插入:

```css
  /* Option A: structured field card */
  .confidence-meter { height: 5px; border-radius: 3px; background: var(--bg-input); overflow: hidden; margin-bottom: 10px; }
  .confidence-meter > i { display: block; height: 100%; background: var(--sc); }
  .role-oneliner {
    font-size: 13px; font-weight: 500; color: var(--text-primary);
    padding: 8px 10px; border-radius: var(--radius-sm); margin-bottom: 10px;
    background: color-mix(in srgb, var(--sc) 8%, transparent);
  }
  .field-list { display: flex; flex-direction: column; gap: 8px; margin-bottom: 10px; }
  .field { display: grid; grid-template-columns: 96px 1fr; gap: 10px; align-items: start; }
  .field-k { font-size: 11px; color: var(--text-tertiary); padding-top: 1px; }
  .field-v { font-size: 12.5px; color: var(--text-primary); }
  .role-reasoning { font-size: 12.5px; color: var(--text-secondary); line-height: 1.7; white-space: pre-wrap; }
```

- [ ] **Step 5: 新增字段标签 i18n 键**

`messages/zh-CN.json` 新增:
```json
  "invest_field_market_phase": "市场阶段",
  "invest_field_emotion": "情绪温度",
  "invest_field_phase_reason": "阶段判断",
  "invest_field_buy_point": "买点评估",
  "invest_field_valuation": "估值评估",
  "invest_field_money_flow": "资金流",
  "invest_field_concentration": "集中度",
  "invest_field_dry_powder": "可用现金",
  "invest_field_pnl": "持仓盈亏",
  "invest_field_stock_risk": "个股风险",
  "invest_field_catalyst_tier": "催化等级",
  "invest_field_exec_mode": "执行方式",
  "invest_field_first_tranche": "首笔金额",
  "invest_field_catalyst": "催化说明",
```
`messages/en.json` 新增对应英文:
```json
  "invest_field_market_phase": "Market Phase",
  "invest_field_emotion": "Sentiment",
  "invest_field_phase_reason": "Phase Basis",
  "invest_field_buy_point": "Buy Point",
  "invest_field_valuation": "Valuation",
  "invest_field_money_flow": "Money Flow",
  "invest_field_concentration": "Concentration",
  "invest_field_dry_powder": "Dry Powder",
  "invest_field_pnl": "Position P&L",
  "invest_field_stock_risk": "Stock Risk",
  "invest_field_catalyst_tier": "Catalyst Tier",
  "invest_field_exec_mode": "Execution",
  "invest_field_first_tranche": "First Tranche",
  "invest_field_catalyst": "Catalyst",
```

- [ ] **Step 6: 验证编译 + i18n**

Run:
```bash
npm run check
npm run i18n:check
```
Expected: svelte-check 0 errors;i18n 无缺键。重点确认 `ParsedFields` 类型推导无误(若推导失败,改用从 store 显式导出的 `RoundOutputSummary['parsed']` 类型)。

- [ ] **Step 7: 运行目测**

`npm run tauri dev` → 跑一次委员会(或看已有结果),展开卡片确认:各角色显示 signal/置信度 chip + 置信度迷你条 + 一句话结论 + key-value 字段表 + reasoning;不再是等宽原始文本块;解析失败(hard fallback)时仍显示黄色 fallback 提示;无结构化字段时兜底显示 rawText。

- [ ] **Step 8: Commit**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte messages/en.json messages/zh-CN.json
git commit -m "feat(committee): 直播卡内部改用结构化字段卡渲染(方案A),rawText 兜底"
```

---

## Task 4: 响应式断点完善(任务 B3)

**Files:**
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`(`@media` 块、`.regime-chip` 响应式)

**Interfaces:**
- Consumes: 无。
- Produces: 窄屏下 REGIME chip 仅留状态名、字段表/双列塌单列。

- [ ] **Step 1: 窄屏隐藏 chip 指标 + 中屏过渡**

在现有 `@media (max-width: 700px)` 块(Task2 改过)基础上扩展。在 `.flow-grid` 单列规则旁加 chip 指标隐藏;并加一档中屏断点收窄进度条:

```css
  /* Medium screens: slightly narrower pipeline bar */
  @media (max-width: 1100px) {
    .pipeline-bar { width: 120px; }
  }
  @media (max-width: 700px) {
    .flow-grid { grid-template-columns: 1fr; }
    .regime-chip .rg-metrics { display: none; }
    .pipeline-bar { width: 96px; }
    .field { grid-template-columns: 80px 1fr; }
  }
```

- [ ] **Step 2: 验证 + 多屏宽目测**

Run: `npm run check`
`npm run tauri dev`,拖动窗口在宽(>1100)/中(700-1100)/窄(<700)三档下目测:
- 宽:chip 全指标、进度条 148px、展开双列。
- 中:进度条 120px,布局不溢出。
- 窄:chip 仅状态名、进度条 96px、展开单列、字段表标签列收窄。

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte
git commit -m "feat(committee): 直播卡响应式断点 — chip 指标/进度条/列布局随屏宽自适应"
```

---

## Task 5: 批次 B 收尾验证

**Files:** 无(仅运行验证命令)

- [ ] **Step 1: 前端全量校验**

Run:
```bash
npm run check
npm run lint
npm run i18n:check
npm run build
```
Expected: 全部通过。lint 若可自动修复,运行 `npm run lint:fix`。

- [ ] **Step 2: 回归目测清单**

`npm run tauri dev`,逐项确认:
- 头部:名字|HOLD/WATCH|REGIME chip|空档|瘦进度条|判断|运行|展开;unknown chip 淡化。
- 展开:无 REGIME 框;宏观/CIO/判决全宽;量化/风控两列;内部为方案A 结构化字段;hard fallback 黄框正常。
- 响应式:宽/中/窄三档布局正常。
- tool-strip 仍在(批次 B 未动,属批次 C)。

- [ ] **Step 3: 若有改动,补提交**

```bash
git add -A
git commit -m "chore(committee): 批次 B 收尾 — lint/format 修正"
```

---

## Self-Review 记录

- **Spec 覆盖:** B1 头部行重排+REGIME chip+进度条瘦化 → Task1;B2 移除 REGIME 框 → Task2,全宽块 → Task2,方案A 渲染 → Task3;B3 响应式断点 → Task4。spec 批次 B 全覆盖。
- **类型一致性:** `regimeChip(p)`/`pipelineBar(p)` 参数同为 `SymbolProgress | undefined`;`roleFields(stepKey, pf)` 的 `pf` 取自 `round.parsed`;新增 CSS class(`regime-chip`/`header-spacer`/`confidence-meter`/`field-list`/`field`/`role-oneliner`/`role-reasoning`)在模板与 `<style>` 一致。
- **占位符扫描:** 无 TBD/TODO;每个代码步骤含完整代码。
- **隔离性:** 全程不动 tool-strip 与 toolMap(留给批次 C),避免与 C1 冲突。
- **已知不确定点(实现时核对):** Task3 Step1 的 `ParsedFields` 类型推导若失败,改为在 store 中给 `RoundOutputSummary['parsed']` 显式导出类型别名再 import;Task1 的 `--color-quant` 变量若项目未定义,fallback 已用 `#c084fc`,实现时确认变量名(grep `--color-quant`)。
