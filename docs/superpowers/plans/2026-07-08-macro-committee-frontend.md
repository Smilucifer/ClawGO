# 委员会宏观改版 · 前端实现计划(计划 2/2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking。
>
> **依赖:** 本计划消费计划 1(后端)的命令 `get_macro_snapshot`(Task 1 新增)、`get_macro_verdict`、`refresh_macro_verdict`。执行前计划 1 应已完成。

**Goal:** 宏观快照卡片从 per-symbol 移到持仓摘要下方全局展示一次(两区+赚钱效应带),per-symbol verdict 块加行业敏感度小条;全局红涨绿跌配色;四态状态机 + i18n。

**Architecture:** 新增 `globalMacro` store 状态(读 `get_macro_snapshot` 纯数据 + `get_macro_verdict` 判断 + 手动刷新);新建 `GlobalMacroCard.svelte`(对齐 demo);`CommitteeLiveTab` 移除 per-symbol `MacroSnapshotCard`、verdict 块加 `.sens-strip`;全局配色新增 `--up/--down/--flat` 语义变量,审计替换方向/盈亏色,状态/危险色不动。

**Tech Stack:** SvelteKit + Svelte 5 runes(`$state`/`$derived`)、Tauri transport、paraglide-style i18n(`$messages/*.json`)。

## Global Constraints

- 响应/注释/UI 文案一律简体中文;技术标识符保留原文。
- Conventional Commits。
- i18n:新 key 必须同时加 `messages/en.json` + `messages/zh-CN.json`,类型从 en.json 自动推导;UI 文案一律走 `t('key')`,不硬编中文。
- 前端验证:`pnpm check`(svelte-check)+ `pnpm lint`(eslint,零 error);构建 `pnpm build`。
- 配色红涨绿跌**只反转方向/涨跌/盈亏语义**:涨/盈利/bullish/risk_on/BUY → `--up`(红 #c56f62);跌/亏损/bearish/risk_off/SELL → `--down`(绿 #7f9d6d);平 → `--flat`。Gate 通过/失败、danger/error 按钮**保持** `--color-success`/`--color-error` 不反转。
- demo 基准:`docs/ui-demo/macro-card-demo.html`(:root 变量 + `.me-badge`/`.sens-badge`/`.apd`/`.macro-grid` 等 class 为对齐目标)。
- 赚钱效应 4 态 class:`hot`(--up)/`active`(--accent)/`calm`(--flat)/`cold`(--down);敏感度 2 态:`pos`(--up)/`neg`(--down)/中性用 `--flat`。
- money_effect 枚举闭集(与后端一致):hot/active/calm/cold;缺失显示"数据不足"。

---

## 文件结构

**新建:**
- `src/lib/components/invest/GlobalMacroCard.svelte` — 全局宏观快照卡片(两区 + 赚钱效应带)。

**修改:**
- `src-tauri/src/commands/invest.rs` + `src-tauri/src/lib.rs` — 新增 `get_macro_snapshot` 命令(Task 1)。
- `src/app.css` — 新增 `--up`/`--down`/`--flat` 语义变量(:root + invest-scope)。
- `src/lib/utils/invest-verdict.ts` — BUY/SELL 方向色改走语义变量。
- `src/lib/stores/invest-committee-store.svelte.ts` — `globalMacro` 状态 + action + 类型。
- `src/lib/components/invest/CommitteeLiveTab.svelte` — 挂全局卡片、移除 per-symbol 宏观卡、加敏感度小条、配色审计。
- `src/lib/components/invest/MacroSnapshotCard.svelte` — 废弃(Task 7 删除引用后删文件)。
- `messages/en.json` + `messages/zh-CN.json` — 新增 i18n key。

---

### Task 1: 后端补 `get_macro_snapshot` 命令(纯数据源)

**Files:**
- Modify: `src-tauri/src/commands/invest.rs`(末尾加命令)
- Modify: `src-tauri/src/lib.rs`(注册)

**Interfaces:**
- Consumes: `storage::invest::macro_cache::build_macro_snapshot()`(已存在,含计划 1 Task 3 新增的 upOver3pctCount/flatCount)。
- Produces: `#[tauri::command] get_macro_snapshot() -> Result<Option<MacroSnapshot>, String>`。

- [ ] **Step 1: 加命令**

```rust
/// 读 macro_cache 的宏观快照(纯数据,前端全局卡片用)。
#[tauri::command]
pub fn get_macro_snapshot() -> Result<Option<crate::storage::invest::macro_cache::MacroSnapshot>, String> {
    Ok(crate::storage::invest::macro_cache::build_macro_snapshot())
}
```

- [ ] **Step 2: lib.rs 注册**

`invoke_handler!` 内(紧邻计划 1 注册的 `get_macro_verdict` 后)加:
```rust
            commands::invest::get_macro_snapshot,
```

- [ ] **Step 3: check + commit**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。
```bash
git add src-tauri/src/commands/invest.rs src-tauri/src/lib.rs
git commit -m "feat(invest): get_macro_snapshot 命令(前端全局卡片纯数据源)"
```

---

### Task 2: 全局配色语义变量 `--up` / `--down` / `--flat`

**Files:**
- Modify: `src/app.css`(:root line ~56-61 加三变量;`[data-invest-scope]` line ~91-112 保持一致)

**Interfaces:**
- Produces: CSS 变量 `--up: #c56f62`(涨/红)、`--down: #7f9d6d`(跌/绿)、`--flat: hsl(24 7% 58%)`(平/灰),值取自 demo :root。
- 说明:**新增独立变量,不动** `--color-success`/`--color-error`(继续表示状态/危险语义)。

- [ ] **Step 1: :root 加三变量**

`src/app.css` 的 `--color-warning: #b89a6a;`(line 61)之后加:
```css
  /* 红涨绿跌:方向/涨跌/盈亏语义专用(状态/危险语义仍用 --color-success/error) */
  --up: #c56f62;
  --down: #7f9d6d;
  --flat: hsl(24 7% 58%);
```

- [ ] **Step 2: invest-scope 一致(如需微调 warm 色调)**

`[data-invest-scope]`(line 91-112)内保持继承 :root 三变量即可(demo 用同值),无需覆盖。确认该 scope 未意外覆盖 `--up/--down/--flat`。

- [ ] **Step 3: 验证 + commit**

Run: `pnpm check`
Expected: 无新增错误。
```bash
git add src/app.css
git commit -m "style(invest): 新增 --up/--down/--flat 红涨绿跌语义变量"
```

---

### Task 3: 红涨绿跌配色审计替换

**Files:**
- Modify: `src/lib/utils/invest-verdict.ts`(BUY/SELL,line 13-19)
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`(`.chip.sig-*` line 765-769;持仓摘要硬编 hex line 349/353/359)

**Interfaces:**
- Consumes: Task 2 的 `--up`/`--down`/`--flat`。
- 原则:BUY/risk_on/bullish → --up;SELL/risk_off/bearish → --down;HOLD/neutral → --accent;TRIM → --color-warning(不变);Gate/danger 不变。

- [ ] **Step 1: invest-verdict.ts BUY/SELL 反转(line 13-19)**

```ts
  if (verdict === 'BUY') return 'background:rgba(197,111,98,0.2); color:var(--up);';
  if (verdict === 'ACCUMULATE') return 'background:rgba(197,111,98,0.14); color:var(--up);';
  if (verdict === 'HOLD' || verdict === 'WATCH')
    return 'background:var(--accent-muted); color:var(--accent);';
  if (verdict === 'TRIM') return 'background:rgba(127,157,109,0.16); color:var(--down);';
  if (verdict === 'SELL') return 'background:rgba(127,157,109,0.2); color:var(--down);';
  return 'background:var(--bg-input); color:var(--text-tertiary);';
```
(注:ACCUMULATE=温和看多→浅 up;TRIM=温和看空→浅 down,与 SELL 同向不同深浅。)

- [ ] **Step 2: .chip.sig-* 方向色(CommitteeLiveTab line 765-769)**

```css
  .chip.sig-risk_on, .chip.sig-buy, .chip.sig-bullish { background: rgba(197,111,98,0.15); color: var(--up); }
  .chip.sig-accumulate { background: rgba(197,111,98,0.12); color: var(--up); }
  .chip.sig-hold, .chip.sig-neutral { background: rgba(196,169,110,0.15); color: var(--accent); }
  .chip.sig-trim { background: rgba(127,157,109,0.15); color: var(--down); }
  .chip.sig-risk_off, .chip.sig-sell, .chip.sig-bearish, .chip.sig-high_risk { background: rgba(127,157,109,0.2); color: var(--down); }
```

- [ ] **Step 3: 持仓摘要盈亏硬编 hex(CommitteeLiveTab line 349/353/359)**

- line 349 现金:中性数值,改 `text-[var(--flat)]`(现金非涨跌语义,用灰)。
- line 353 集中度超标:保持 `text-[#b89a6a]` 或改 `text-[var(--color-warning)]`(风险提示语义,**不反转**)。
- line 359 总收益:正 `text-[var(--up)]`、负 `text-[var(--down)]`(盈亏方向,反转)。

- [ ] **Step 4: 验证 + commit**

Run: `pnpm check && pnpm lint`
Expected: 零 error。
```bash
git add src/lib/utils/invest-verdict.ts src/lib/components/invest/CommitteeLiveTab.svelte
git commit -m "style(invest): 红涨绿跌配色审计(BUY/SELL/chip/盈亏,状态色不动)"
```

---

### Task 4: i18n key(赚钱效应 / 市场广度 / 敏感度 / 四态)

**Files:**
- Modify: `messages/en.json` + `messages/zh-CN.json`(同步加 key)

**Interfaces:**
- Produces:新 MessageKey(en.json keyof 自动推导),供 Task 6/7 的 `t('...')` 使用。

- [ ] **Step 1: 两文件同步加 key(现有 invest_macro_* 段之后)**

`messages/zh-CN.json`:
```json
  "invest_macro_breadth": "市场广度",
  "invest_macro_indicators": "宏观指标",
  "invest_macro_up_over_3pct": "涨幅>3%",
  "invest_macro_flat_count": "平盘",
  "invest_macro_sentiment_pct": "大盘情绪",
  "invest_macro_apd": "涨-平-跌",
  "invest_money_effect": "赚钱效应",
  "invest_money_effect_hot": "火热",
  "invest_money_effect_active": "活跃",
  "invest_money_effect_calm": "平淡",
  "invest_money_effect_cold": "冰点",
  "invest_money_effect_nodata": "数据不足",
  "invest_sensitivity_title": "行业敏感度",
  "invest_sensitivity_pos": "正向",
  "invest_sensitivity_neg": "负向",
  "invest_sensitivity_neutral": "中性",
  "invest_macro_verdict_analyzing": "分析中…",
  "invest_macro_verdict_stale": "数据已更新,判断待刷新",
  "invest_macro_verdict_waiting": "等待开盘",
  "invest_macro_refresh": "刷新判断"
```

`messages/en.json`(键一致,值英文):
```json
  "invest_macro_breadth": "Market Breadth",
  "invest_macro_indicators": "Macro Indicators",
  "invest_macro_up_over_3pct": "Up >3%",
  "invest_macro_flat_count": "Flat",
  "invest_macro_sentiment_pct": "Sentiment",
  "invest_macro_apd": "Adv-Flat-Dec",
  "invest_money_effect": "Money Effect",
  "invest_money_effect_hot": "Hot",
  "invest_money_effect_active": "Active",
  "invest_money_effect_calm": "Calm",
  "invest_money_effect_cold": "Cold",
  "invest_money_effect_nodata": "No data",
  "invest_sensitivity_title": "Industry Sensitivity",
  "invest_sensitivity_pos": "Positive",
  "invest_sensitivity_neg": "Negative",
  "invest_sensitivity_neutral": "Neutral",
  "invest_macro_verdict_analyzing": "Analyzing…",
  "invest_macro_verdict_stale": "Data updated, verdict pending refresh",
  "invest_macro_verdict_waiting": "Awaiting market open",
  "invest_macro_refresh": "Refresh verdict"
```

- [ ] **Step 2: 验证 + commit**

Run: `pnpm check`(MessageKey 类型应重新推导通过,无 unknown key)。
```bash
git add messages/en.json messages/zh-CN.json
git commit -m "i18n(invest): 宏观广度/赚钱效应/敏感度/四态 key(中英)"
```

---

### Task 5: store `globalMacro` 状态 + action + 类型

**Files:**
- Modify: `src/lib/stores/invest-committee-store.svelte.ts`

**Interfaces:**
- Consumes: `get_macro_snapshot` / `get_macro_verdict` / `refresh_macro_verdict`(计划 1 + Task 1);现有 `invoke`/`getTransport`。
- Produces:
  - 类型 `MacroVerdict`(与后端 camelCase 对齐)、`MacroVerdictView { verdict: MacroVerdict | null; isCurrent: boolean }`、`GlobalMacroState { snapshot, verdict, isCurrent, status, refreshing }`。
  - `status: 'analyzing' | 'stale' | 'ready' | 'waiting' | 'empty'`(四态 + 空态,§8.2-I)。
  - class 字段 `globalMacro = $state<GlobalMacroState>(...)`。
  - 方法 `loadGlobalMacro()`(并发拉 snapshot+verdict,派生 status)、`refreshGlobalMacro()`(调 refresh 后重载)。

- [ ] **Step 1: 加类型(文件类型区,MacroSnapshot interface 之后 line ~93)**

```ts
export interface MacroVerdict {
  signal: string | null;
  strength: number | null;
  marketPhase: string | null;
  moneyEffect: string | null;
  moneyEffectReason: string | null;
  signalReason: string | null;
  marketPhaseReason: string | null;
  basedOnDataVersion: string;
  updatedAt: string;
}
export interface MacroVerdictView { verdict: MacroVerdict | null; isCurrent: boolean; }
export type GlobalMacroStatus = 'analyzing' | 'stale' | 'ready' | 'waiting' | 'empty';
export interface GlobalMacroState {
  snapshot: MacroSnapshot | null;
  verdict: MacroVerdict | null;
  isCurrent: boolean;
  status: GlobalMacroStatus;
  refreshing: boolean;
}
```

- [ ] **Step 2: 加 class 字段 + 方法(class 内,`modeOverrides` 后 line ~230)**

```ts
  globalMacro = $state<GlobalMacroState>({
    snapshot: null, verdict: null, isCurrent: false, status: 'empty', refreshing: false,
  });

  async loadGlobalMacro() {
    const [snapshot, view] = await Promise.all([
      invoke<MacroSnapshot | null>('get_macro_snapshot').catch(() => null),
      invoke<MacroVerdictView>('get_macro_verdict').catch(() => ({ verdict: null, isCurrent: false })),
    ]);
    this.globalMacro.snapshot = snapshot;
    this.globalMacro.verdict = view.verdict;
    this.globalMacro.isCurrent = view.isCurrent;
    this.globalMacro.status = this._deriveMacroStatus(snapshot, view);
  }

  private _deriveMacroStatus(snap: MacroSnapshot | null, view: MacroVerdictView): GlobalMacroStatus {
    if (!snap && !view.verdict) return 'empty';        // 全新用户/无数据
    if (!view.verdict) return 'analyzing';             // 有数据无判断 → 生成中
    if (!view.isCurrent) return 'stale';               // 数据已更新,判断待刷新
    return 'ready';
  }

  async refreshGlobalMacro() {
    this.globalMacro.refreshing = true;
    try {
      const msg = await invoke<string>('refresh_macro_verdict');
      if (typeof msg === 'string' && msg.startsWith('skipped')) {
        this.globalMacro.status = 'waiting';           // 非交易时段
      }
      await this.loadGlobalMacro();
    } finally {
      this.globalMacro.refreshing = false;
    }
  }
```

- [ ] **Step 3: 更新 `RoundOutputSummary.parsed` 类型(敏感度小条的类型地基)**

现状:`parsed`(line 31-56)有旧 `emotionTemperature?`,**无** `sensitivity`。后端(计划 1 Task 7/12)已产出 `sensitivity`/`sensitivityReason` 并把 `emotion_temperature` 改名 `money_effect_reason`。同步 TS:

`// Macro`(line 41-43)段改为:
```ts
    // Macro
    marketPhase?: string;
    signalReason?: string;
    marketPhaseReason?: string;
    sensitivity?: string;          // positive | negative | neutral(per-symbol 敏感度)
    sensitivityReason?: string;
    moneyEffectReason?: string;    // 原 emotionTemperature 语义替换
```
删除 `emotionTemperature?: string;`(若 grep 全仓仍有引用——如旧渲染处——一并清理,应仅此一处类型 + 可能的展示处)。

- [ ] **Step 4: 验证 + commit**

Run: `pnpm check`
Expected: 零 error。若报 `emotionTemperature` 仍被引用,定位并改为 `moneyEffectReason` 或移除。
```bash
git add src/lib/stores/invest-committee-store.svelte.ts
git commit -m "feat(invest): store globalMacro 状态 + parsed 敏感度字段 + 四态派生"
```

---

### Task 6: GlobalMacroCard 组件(两区 + 赚钱效应带 + 四态)

**Files:**
- Create: `src/lib/components/invest/GlobalMacroCard.svelte`

**Interfaces:**
- Consumes: prop `macro: GlobalMacroState`(Task 5)、`refreshing`;回调 prop `onRefresh: () => void`;`t()`。
- Produces: 对齐 demo 的卡片:标题带(signal chip + 强度 + 刷新按钮)、宏观指标 5 格、市场广度 5 格(APD/涨停/跌停/涨幅>3%/情绪%)、赚钱效应带(me-badge + 理由)。
- 派生:大盘情绪% = 上涨/(上涨+下跌)×100(前端算,§4.1);APD = 涨/平/跌。

- [ ] **Step 1: script 段 + 标题带(卡片顶部)**

创建 `src/lib/components/invest/GlobalMacroCard.svelte`:

```svelte
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import type { GlobalMacroState } from '$lib/stores/invest-committee-store.svelte';

  let { macro, onRefresh }: { macro: GlobalMacroState; onRefresh: () => void } = $props();

  const s = $derived(macro.snapshot);
  const v = $derived(macro.verdict);
  const meClass = $derived(v?.moneyEffect ?? '');            // hot|active|calm|cold|''
  const adv = $derived(s?.advanceCount ?? 0);
  const dec = $derived(s?.declineCount ?? 0);
  const sentiment = $derived(adv + dec > 0 ? (adv / (adv + dec)) * 100 : null);
  const meLabel = $derived(
    macro.status !== 'ready' || !v?.moneyEffect
      ? t('invest_money_effect_nodata')
      : t(`invest_money_effect_${v.moneyEffect}` as never),
  );
  const num = (x: number | null | undefined, d = 0) =>
    x == null ? '—' : x.toLocaleString(undefined, { maximumFractionDigits: d });
</script>

<div class="macro-snapshot" data-invest-scope>
  <div class="macro-header">
    <span class="macro-title">
      {t('invest_macro_snapshot')}
      {#if v?.signal}· <span class="chip sig-{v.signal}">{v.signal}</span>{/if}
      {#if v?.strength != null}· {t('invest_macro_strength')} {v.strength.toFixed(1)}{/if}
    </span>
    <button class="macro-refresh" onclick={onRefresh} disabled={macro.refreshing}>
      {macro.refreshing ? t('invest_macro_verdict_analyzing') : t('invest_macro_refresh')}
    </button>
  </div>
```

- [ ] **Step 2: 两区网格 + 赚钱效应带(接 script 段之后)**

```svelte
  {#if macro.status === 'empty'}
    <p class="macro-empty">{t('invest_macro_no_data')} · {t('invest_macro_refresh')}</p>
  {:else}
    <!-- 宏观指标 5 格 -->
    <div class="macro-sub">{t('invest_macro_indicators')}</div>
    <div class="macro-grid">
      <div class="macro-cell"><span>{t('invest_macro_sh_composite_close')}</span><b>{num(s?.shCompositeClose, 2)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_northbound_net')}</span><b>{num(s?.northboundNet, 1)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_vix')}</span><b>{num(s?.vix, 2)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_gold')}</span><b>{num(s?.gold, 1)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_two_market_volume')}</span><b>{num(s?.twoMarketVolume, 0)}</b></div>
    </div>
    <!-- 市场广度 5 格 -->
    <div class="macro-sub">{t('invest_macro_breadth')}</div>
    <div class="macro-grid">
      <div class="macro-cell"><span>{t('invest_macro_apd')}</span>
        <span class="apd"><span class="u">{num(adv)}</span><span class="sep">-</span><span class="p">{num(s?.flatCount)}</span><span class="sep">-</span><span class="d">{num(dec)}</span></span></div>
      <div class="macro-cell"><span>{t('invest_macro_limit_up_count')}</span><b class="u">{num(s?.limitUpCount)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_limit_down_count')}</span><b class="d">{num(s?.limitDownCount)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_up_over_3pct')}</span><b class="u">{num(s?.upOver3pctCount)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_sentiment_pct')}</span><b>{sentiment == null ? '—' : sentiment.toFixed(0) + '%'}</b></div>
    </div>
    <!-- 赚钱效应带 -->
    <div class="money-strip">
      <span class="me-title">{t('invest_money_effect')}</span>
      <span class="me-badge {meClass}">{meLabel}</span>
      <span class="me-reason">
        {#if macro.status === 'stale'}{t('invest_macro_verdict_stale')}
        {:else if macro.status === 'analyzing'}{t('invest_macro_verdict_analyzing')}
        {:else}{v?.moneyEffectReason ?? ''}{/if}
      </span>
    </div>
  {/if}
</div>
```

- [ ] **Step 3: 样式段(照 demo class,用语义变量)**

```svelte
<style>
  .macro-snapshot { border: 1px solid var(--border); background: var(--bg-card);
    border-radius: var(--radius-lg); padding: var(--space-4); display: flex; flex-direction: column; gap: var(--space-2); }
  .macro-header { display: flex; justify-content: space-between; align-items: center; }
  .macro-title { font-size: 13px; color: var(--text-secondary); }
  .macro-refresh { font-size: 11px; padding: 2px 10px; border-radius: var(--radius-sm);
    border: 1px solid var(--border); background: var(--bg-input); color: var(--text-secondary); cursor: pointer; }
  .macro-refresh:disabled { opacity: 0.5; cursor: default; }
  .macro-sub { font-size: 10px; text-transform: uppercase; color: var(--text-tertiary); margin-top: var(--space-1); }
  .macro-grid { display: grid; grid-template-columns: repeat(5, 1fr); gap: var(--space-2); }
  .macro-cell { display: flex; flex-direction: column; gap: 2px; padding: var(--space-2);
    background: var(--bg-hover); border-radius: var(--radius-sm); }
  .macro-cell span { font-size: 10px; color: var(--text-tertiary); }
  .macro-cell b { font-family: var(--font-mono); font-size: 14px; color: var(--text-primary); }
  .macro-cell b.u, .apd .u { color: var(--up); }
  .macro-cell b.d, .apd .d { color: var(--down); }
  .apd { display: inline-flex; gap: 2px; font-family: var(--font-mono); font-size: 13px; font-weight: 600; }
  .apd .p { color: var(--flat); } .apd .sep { color: var(--text-tertiary); }
  .money-strip { display: flex; align-items: center; gap: 10px; padding-top: var(--space-2);
    border-top: 1px solid var(--border); margin-top: var(--space-1); }
  .me-title { font-size: 10px; text-transform: uppercase; color: var(--text-tertiary); }
  .me-badge { font-size: 12px; font-weight: 700; padding: 2px 10px; border-radius: var(--radius-sm); }
  .me-badge.hot { color: var(--up); background: rgba(197,111,98,0.18); }
  .me-badge.active { color: var(--accent); background: var(--accent-subtle); }
  .me-badge.calm { color: var(--flat); background: rgba(158,154,150,0.14); }
  .me-badge.cold { color: var(--down); background: rgba(127,157,109,0.18); }
  .me-reason { font-size: 12px; color: var(--text-secondary); }
  .macro-empty { font-size: 12px; color: var(--text-tertiary); }
  .chip { font-size: 11px; padding: 1px 8px; border-radius: var(--radius-sm); }
  .chip.sig-risk_on { background: rgba(197,111,98,0.15); color: var(--up); }
  .chip.sig-risk_off { background: rgba(127,157,109,0.2); color: var(--down); }
  .chip.sig-neutral { background: rgba(196,169,110,0.15); color: var(--accent); }
</style>
```

- [ ] **Step 4: 验证 + commit**

Run: `pnpm check`
Expected: 零 error(注意 `t(\`...${}\` as never)` 动态 key 若报类型错,改为 switch 映射固定 key)。
```bash
git add src/lib/components/invest/GlobalMacroCard.svelte
git commit -m "feat(invest): GlobalMacroCard 组件(两区+赚钱效应带+四态,对齐demo)"
```

---

### Task 7: CommitteeLiveTab 接线(挂全局卡片 + 移除 per-symbol 宏观卡 + 敏感度小条)

**Files:**
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`
- Delete: `src/lib/components/invest/MacroSnapshotCard.svelte`(引用移除后)

**Interfaces:**
- Consumes: `GlobalMacroCard`(Task 6);store `globalMacro` + `loadGlobalMacro`/`refreshGlobalMacro`(Task 5);macro round 的 `parsed.sensitivity`/`parsed.sensitivityReason`(step_index=0)。
- 布局:持仓摘要(line 333-365)之后插 `<GlobalMacroCard>`;verdict 块(line 485)移除 `<MacroSnapshotCard>`,改为 `.sens-strip`。

- [ ] **Step 1: import + 挂全局卡片(持仓摘要 div 之后 line 365)**

```svelte
  import GlobalMacroCard from './GlobalMacroCard.svelte';
```
持仓摘要块结束后插:
```svelte
  <GlobalMacroCard macro={store.globalMacro} onRefresh={() => store.refreshGlobalMacro()} />
```

- [ ] **Step 2: onMount 加载(现有 onMount 内或新增)**

```svelte
  onMount(() => { store.loadGlobalMacro(); });
```
(若已有 onMount,把 `store.loadGlobalMacro();` 并入,勿重复注册。)

- [ ] **Step 3: verdict 块移除 per-symbol 宏观卡 + 加敏感度小条(line 485)**

删除:
```svelte
  <MacroSnapshotCard snapshot={result.macroSnapshot} />
```
及其 `import MacroSnapshotCard`。改为(从 macro round 取敏感度;`macroRound` = `result.rounds.find(r => r.parsed && r.role === 'Macro')` 或按 step_index=0):
```svelte
  {#if macroRound?.parsed?.sensitivity}
    {@const sens = macroRound.parsed.sensitivity}
    <div class="sens-strip">
      <span class="sens-title">{t('invest_sensitivity_title')}</span>
      <span class="sens-badge {sens === 'positive' ? 'pos' : sens === 'negative' ? 'neg' : 'neu'}">
        {sens === 'positive' ? t('invest_sensitivity_pos') : sens === 'negative' ? t('invest_sensitivity_neg') : t('invest_sensitivity_neutral')}
      </span>
      <span class="sens-reason">{macroRound.parsed.sensitivityReason ?? ''}</span>
    </div>
  {/if}
```
其中 `macroRound` 在该 symbol 渲染作用域内派生(参照现有 `result.rounds` 访问方式;字段名以实际 CommitteeResult 类型为准,pnpm check 会校验)。

- [ ] **Step 4: 加 sens-strip 样式(CommitteeLiveTab `<style>`,照 demo line 162-168)**

```css
  .sens-strip { display: flex; align-items: center; gap: 10px; padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm); background: var(--bg-hover); border: 1px solid var(--border); margin-top: var(--space-2); }
  .sens-title { font-size: 10px; color: var(--text-tertiary); text-transform: uppercase; }
  .sens-reason { font-size: 12px; color: var(--text-secondary); }
  .sens-badge { font-size: 11px; font-weight: 700; padding: 2px 9px; border-radius: var(--radius-sm); }
  .sens-badge.pos { color: var(--up); background: rgba(197,111,98,0.18); }
  .sens-badge.neg { color: var(--down); background: rgba(127,157,109,0.18); }
  .sens-badge.neu { color: var(--flat); background: rgba(158,154,150,0.14); }
```

- [ ] **Step 4-bis: 清理 emotionTemperature 引用(改名后的孤儿)**

`CommitteeLiveTab.svelte` 有两处引用旧字段(grep 确认):
- line 49:`|| pf.marketPhase || pf.emotionTemperature || pf.buyPointAssessment` → 把 `pf.emotionTemperature` 改为 `pf.moneyEffectReason`(保持"有内容即展开"的判空语义)。
- line 66:`push(t('invest_field_emotion'), pf.emotionTemperature);` → 改为 `push(t('invest_money_effect'), pf.moneyEffectReason);`(复用 Task 4 的 `invest_money_effect` key;旧 `invest_field_emotion` key 若无其它引用可保留不删)。

- [ ] **Step 5: 删除 MacroSnapshotCard.svelte + 清 i18n 孤儿**

确认全仓无 `MacroSnapshotCard` 引用后删除该文件。旧 `invest_macro_sh_composite_vol20` key 若无其它引用可保留(不删,避免误伤)。
Run: `pnpm check && pnpm lint`
Expected: 零 error,无未使用 import。

- [ ] **Step 6: Commit**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte
git rm src/lib/components/invest/MacroSnapshotCard.svelte
git commit -m "feat(invest): 全局宏观卡片上移 + per-symbol 敏感度小条 + 移除旧宏观卡"
```

---

### Task 8: 前端全链验证

**Files:** 无新增。

- [ ] **Step 1: 类型 + lint + 构建**

Run: `pnpm check && pnpm lint && pnpm build`
Expected: 全通过,零 error。

- [ ] **Step 2: dev app 视觉走查(对照 demo)**

启动 dev app → 委员会实况页:
- 持仓摘要下方出现全局宏观卡片(仅一张,非 per-symbol)。
- 宏观指标 5 格 + 市场广度 5 格(APD 涨红/跌绿/平灰、涨停红、跌停绿、涨幅>3%、情绪%)。
- 赚钱效应带:档位徽标(火热红/活跃金/平淡灰/冰点绿)+ 理由。
- 展开某 symbol → verdict 块底部有行业敏感度小条(正向红/负向绿/中性灰)。
- 全局红涨绿跌:BUY 裁决红、SELL 绿、总收益正红负绿;Gate 通过仍绿、失败仍红(未反转)。

- [ ] **Step 3: 四态 + 空态走查**

- 无数据(全新)→ 卡片空态 + 引导刷新,不崩。
- 数据有、判断无 → 赚钱效应"数据不足" + "分析中"。
- 点刷新 → 非交易时段显示"等待开盘";交易时段刷新后 ready。
- 后端数据更新但判断未刷 → "数据已更新,判断待刷新"(stale)。

- [ ] **Step 4: i18n 切换**

切 en/zh,确认新 key 均有译文,无 raw key 泄漏。

- [ ] **Step 5: 更新 memory**

新增 memory:委员会宏观卡片改为全局单卡(GlobalMacroCard),per-symbol 仅敏感度小条;红涨绿跌语义变量 `--up/--down/--flat`(方向色)与 `--color-success/error`(状态色)分离。链 [[macro-committee-redesign]]。

---

## 自检结论(spec → plan 覆盖)

- §4.4 卡片层(上移/两区/赚钱效应带/生成中占位):Task 6 + Task 7 + 四态(Task 5 派生 + Task 8-3)。✅
- §4.4 per-symbol 敏感度小条:Task 7-3。✅
- §4.4 store globalMacro:Task 5。✅
- §4.5 红涨绿跌(方向反转 + BUY/SELL 跟涨跌 + Gate/danger 不动):Task 2 + Task 3。✅
- §4.1 卡片撤 vol20:Task 6 指标区不含 vol20(改列成交额)。✅(B3 展示层落地)
- §8.2-I 四态状态机(analyzing/stale/ready/waiting + empty 空态):Task 5 `_deriveMacroStatus` + Task 6 渲染 + Task 8-3。✅
- i18n 中英:Task 4 + Task 8-4。✅

**依赖回看:** 全局卡片纯数据来自 Task 1 新增 `get_macro_snapshot`(含计划 1 Task 3 的 upOver3pctCount/flatCount);判断来自计划 1 `get_macro_verdict`;敏感度来自计划 1 Task 12 写入 macro round 的 `parsed.sensitivity`。三条数据链闭合。
