# Invest 模块 7 项改进计划

## Context

用户提出 invest 模块的 7 项改进需求：收益率计算、交易记录过滤、删除 UI 刷新、Insights/Dreams 启动、事件中文化、策略注入、UI 全面重写。经过代码探索和用户确认，以下是具体方案。

---

## Task 1: 总收益率改为成本基准收益率

**用户选择**: 方案 B — `(totalAssets - totalCostBasis) / totalCostBasis × 100%`

**改动文件**: `src/lib/stores/invest-store.svelte.ts:77-83`

```typescript
// 改前
totalReturnPct = $derived(
  this.initialCash > 0
    ? ((this.totalAssets - this.initialCash) / this.initialCash) * 100
    : this.totalCostBasis > 0
      ? ((this.totalAssets - this.totalCostBasis) / this.totalCostBasis) * 100
      : 0,
);

// 改后
totalReturnPct = $derived(
  this.totalCostBasis > 0
    ? ((this.totalAssets - this.totalCostBasis) / this.totalCostBasis) * 100
    : 0,
);
```

- `initialCash` 字段保留（其他地方可能用到），但不再参与收益率计算
- Dashboard KPI 卡片和 CommitteeLiveTab 的 portfolio summary 自动跟随 `$derived` 更新，无需额外改动

---

## Task 2: 交易记录过滤非交易类 action

**用户选择**: 默认隐藏 + 可展开的"显示全部"开关

**改动文件**: `src/lib/components/invest/TradeLogTab.svelte`

1. 在过滤逻辑（行 22-28）中，默认排除 `cash_adjust`、`cost_edit`、`add_watch`、`delete_watch`
2. 添加 `showSystemActions` 开关状态，默认 `false`
3. 当开关打开时，显示所有 action 类型
4. 下拉过滤器保持 `buy`/`sell`/`all` 三个选项不变

```typescript
// 新增状态
let showSystemActions = $state(false);

// 过滤逻辑更新
const SYSTEM_ACTIONS = new Set(['cash_adjust', 'cost_edit', 'add_watch', 'delete_watch']);

const filtered = $derived(
  investStore.trades.filter((tr) => {
    if (!showSystemActions && SYSTEM_ACTIONS.has(tr.action)) return false;
    if (symbolFilter && !tr.symbol.includes(symbolFilter.toUpperCase())) return false;
    if (directionFilter !== 'all' && tr.action !== directionFilter) return false;
    return true;
  })
);
```

5. 在过滤器栏添加 toggle 开关 UI

---

## Task 3: 修复删除 UI 更新问题

### 3a: Dashboard watch 持仓删除

**问题**: 用户反馈 `confirm()` 对话框出现前持仓就消失了。

**分析**: `openDeleteWatch`（+page.svelte:115-119）使用同步 `confirm()`，理论上不应提前消失。可能原因：
- Tauri webview 中 `confirm()` 的行为异常
- 或 HoldingsTable 有其他响应式依赖导致重渲染

**方案**: 将 `confirm()` 替换为自定义 modal 对话框组件，确保删除操作只在用户确认后执行。

**改动文件**:
- `src/routes/invest/+page.svelte:115-119` — 替换 confirm() 为 modal
- 新增或复用现有的 ConfirmDialog 组件

### 3b: TradeLogTab 删除后不刷新

**问题**: 删除后需要切换 tab 才更新。

**分析**: `deleteTrade`（invest-store.svelte.ts:487-491）已有乐观更新 `this.trades.filter(...)` + `loadAll()`。`TradeLogTab` 的 `filtered` 是 `$derived`，应该自动响应。

**方案**: 检查 Svelte 5 响应式链是否正常工作。如果 `loadAll()` 竞态覆盖了本地更新，改为在 `loadAll()` 完成后再做一次过滤，或移除乐观更新只依赖 `loadAll()`。

**改动文件**: `src/lib/stores/invest-store.svelte.ts:487-491`

---

## Task 4: Insights & Dreams 启动逻辑说明

**现状**: 两者都已完整实现，但 Dreams 默认 `enabled: false`（scheduler/mod.rs:65）。Insights 数据完全来自 Dream Pipeline。

**已修复**: Dream Pipeline 执行报错 — `domain_insights.rs:101-103` 中 SQLite `json()` 函数语法错误（缺少 key 名）。已修复为 `json('id', id, 'insight_type', insight_type, ...)` 格式。

**使用方式**: 进入 System → Dreams，在 DreamingConfigPanel 中打开开关（或点击"立即运行"）。
3. Dream Pipeline 运行后，Insights 自动填充

---

## Task 5: 事件全面中文化

**改动文件**:
- `src/lib/components/invest/EventWatchTab.svelte`
- `src/lib/components/invest/EventTriggerDialog.svelte`

### 5a: severity/stance 标签翻译

使用 i18n 键保持一致性（severity 复用已有 filter 键，stance 新增）：

```typescript
function severityLabel(severity: string): string {
  switch (severity) {
    case 'high': return t('invest.eventWatch.filterHigh');    // "高"
    case 'medium': return t('invest.eventWatch.filterMedium'); // "中"
    case 'low': return t('invest.eventWatch.filterLow');      // "低"
    default: return severity;
  }
}

function stanceLabel(stance: string): string {
  switch (stance) {
    case 'bullish': return t('invest.eventWatch.stanceBullish'); // 新增: "看涨"
    case 'bearish': return t('invest.eventWatch.stanceBearish'); // 新增: "看跌"
    case 'neutral': return t('invest.eventWatch.stanceNeutral'); // 新增: "中性"
    default: return stance;
  }
}
```

**i18n 新增键**（`messages/zh-CN.json` 和 `messages/en.json`）:
```json
"invest.eventWatch.stanceBullish": "看涨",
"invest.eventWatch.stanceBearish": "看跌",
"invest.eventWatch.stanceNeutral": "中性"
```
en.json:
```json
"invest.eventWatch.stanceBullish": "Bullish",
"invest.eventWatch.stanceBearish": "Bearish",
"invest.eventWatch.stanceNeutral": "Neutral"
```

### 5b: 内容展示优先级调整

当前：`title` 作为主显示，`body` 作为副显示。
改为：`body`（LLM 中文摘要）作为主显示，`title` 作为副显示/tooltip。

**EventWatchTab.svelte 行 149-158**:
```svelte
<!-- 改前 -->
<span class="text-sm text-zinc-200 truncate">{event.title}</span>
<span class="text-[10px] {stanceColor(event.stance)}">{event.stance}</span>
{#if event.body && event.body !== event.title}
  <p class="text-xs ...">{event.body}</p>
{/if}

<!-- 改后：body 优先，title 作为 tooltip -->
<span class="text-sm text-zinc-200 truncate" title={event.title !== (event.body || '') ? event.title : ''}>{event.body || event.title}</span>
<span class="text-[10px] {stanceColor(event.stance)}">{stanceLabel(event.stance)}</span>
{#if event.body && event.title && event.title !== event.body}
  <p class="text-xs text-zinc-500 mt-0.5">{event.title}</p>  <!-- 原始标题作为次要信息 -->
{/if}
```

**EventTriggerDialog.svelte 行 65, 69, 71**:
- 行 65: `{event.body || event.title}` — 已经是 body 优先，无需改动
- 行 69: `{event.severity.toUpperCase()}` → `{severityLabel(event.severity)}`
- 行 71: `{event.stance}` → `{stanceLabel(event.stance)}`

**注意**: 只改展示层，不改数据库存储（保持英文存储，前端翻译显示）。

---

## Task 6: 策略配置注入到 Risk 角色

**用户选择**: 仅给 Risk 注入风控约束

**改动文件**: `src-tauri/src/invest/committee/orchestrator.rs:1121-1128`

将策略注入条件从 `Cio` 扩展到 `Cio + Risk`：

```rust
// 改前
if role == CommitteeRole::Cio {
    let strategy_ctx = build_strategy_context();
    ...
}

// 改后
if role == CommitteeRole::Cio || role == CommitteeRole::Risk {
    let strategy_ctx = build_strategy_context();
    ...
}
```

Risk 角色在 R1 和 R2 都会看到策略约束，有助于在风控分析时考虑集中度和现金比例限制。

---

## Task 7: UI 全面重写

**用户要求**: 全面重写，但重写之前必须先出 demo 到 `docs/ui-demo/` 下。

### 阶段 A: 出 Demo（先做）

创建 `docs/ui-demo/pages/invest-v2.html`，参考 `chat.html` 的设计系统（design-system.css），覆盖以下页面：
1. **Dashboard** — KPI 卡片 + 持仓表格 + PnL 图表 + Regime + Verdicts（参考现有 invest.html）
2. **Committee** — Live/Replay/Archive/Roles/Accuracy/Tools 子标签（新增）
3. **Strategy** — 策略配置 CRUD（新增）
4. **Trades** — 交易记录表格 + 过滤器（新增）
5. **System** — Cron/Regime/Events/Datasource/PnL/Insights/Dreams/Profile 子标签（新增）

Demo 需要体现：
- 左侧 sidebar（icon-rail + content-panel）与 chat 一致
- 暖色暗黑主题（design-system.css）
- 工具卡片风格（参考 chat.html 的 tool-card）
- 表格/卡片/按钮等组件与 chat 风格统一

### 阶段 B: 前端实现（Demo 确认后）

根据确认后的 Demo，逐页改造 Svelte 组件：
- `src/routes/invest/+page.svelte` — 主页面布局
- `src/lib/components/invest/` — 各子组件
- `CommitteeToolsTab.svelte` — 补充 L4Officer（此为 Task 7 的子任务，独立于 UI 重写）

### 阶段 C: L4Officer 补充（可独立于 UI 重写）

**改动文件**: `src/lib/components/invest/CommitteeToolsTab.svelte`

1. `ROLE_ACCESS` 数组添加 L4Officer 条目
2. `roleLabel` 函数添加 `l4_officer` case
3. `roleColor` 函数添加 `l4_officer` case（使用 `#ef4444` 红色，与 DebateBlock 一致）

---

## 执行顺序

1. **Task 1** — 收益率公式（1 行改动，快速验证）
2. **Task 2** — 交易记录过滤（前端改动）
3. **Task 3** — 删除 UI 修复（前端改动）
4. **Task 5** — 事件中文化（前端改动）
5. **Task 6** — 策略注入 Risk（后端改动）
6. **Task 7C** — L4Officer 补充（前端改动，可与 Task 7A 并行）
7. **Task 7A** — UI Demo 出图（HTML 静态页面）
8. **Task 7B** — UI 前端实现（Demo 确认后）

Task 4 无需代码改动。

---

## 验证

- `npm run check` — TypeScript 类型检查
- `npm run lint` — 代码风格
- `npm run build` — 前端构建
- `cargo check --manifest-path src-tauri/Cargo.toml` — Rust 编译（Task 6）
- `npm run i18n:check` — i18n 键完整性
- 手动验证：Dashboard 收益率显示、交易记录过滤开关、删除确认对话框、事件中文化、策略注入效果
