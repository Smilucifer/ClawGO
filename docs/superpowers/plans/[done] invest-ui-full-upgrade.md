# /invest 全模块 UI 升级计划

**日期:** 2026-06-02
**目标:** 将 /invest 路由下全部 27 个 Svelte 组件从旧 Tailwind 风格升级为暖色暗黑设计系统
**参考:** `docs/ui-demo/pages/invest-v2.html` + `docs/ui-demo/design-system.css`

## 设计系统核心变量

```css
--bg-base: #1a1918          --bg-card: #242220
--bg-elevated: #242220       --bg-input: #2e2c29
--bg-hover: rgba(235,232,228,0.04)
--border: rgba(235,232,228,0.06)
--text-primary: #ebe8e4      --text-secondary: #9a9590      --text-tertiary: #6b6660
--accent: #c9a96e            --accent-muted: rgba(201,169,110,0.15)
--color-success: #8a9a76     --color-error: #a87a7a         --color-warning: #b89a6a
--radius-sm/md/lg            --space-1..8                   --font-mono
```

## 升级范围总览

| 状态 | Tab | 组件 | 行数 |
|------|-----|------|------|
| ✅ 已完成 | Dashboard | KpiCard, MacroSnapshotCard, LatestVerdictCard, HoldingsTable, PnlChart | 385 |
| ✅ 已完成 | Committee/Live | CommitteeLiveTab | 373 |
| 🔄 待升级 | Committee | CommitteeReplayTab, CommitteeArchiveTab, CommitteeRolesTab, CommitteeAccuracyTab, CommitteeToolsTab | 1570 |
| 🔄 待升级 | Strategy | StrategyTab | 153 |
| 🔄 待升级 | Trades | TradeLogTab | 148 |
| 🔄 待升级 | System | SchedulerTab, SystemRegimeTab, EventWatchTab, SystemDatasourceTab, SystemPnlHistoryTab, InsightsFeed, SystemDreamsTab, UserProfileSection | 1227 |
| 🔄 待升级 | 通用 | TradeDialog, EventTriggerDialog, DebateBlock, PipelineFlow, DreamingConfigPanel, ProviderConfigPanel | 1145 |
| 🔄 待升级 | 页面框架 | +page.svelte | 258 |

## 升级规则（每个组件统一执行）

1. **颜色映射:**
   - `slate-700/800/900` → `bg-[var(--bg-card)]` / `bg-[var(--bg-input)]`
   - `slate-400/500/600` → `text-[var(--text-secondary)]` / `text-[var(--text-tertiary)]`
   - `green-*` → `#8a9a76` (success)
   - `red-*` → `#a87a7a` (error)
   - `yellow-*` → `#b89a6a` (warning)
   - `blue-*` → `#3b82f6`
   - `purple-*` → `#8b5cf6`
   - `emerald-*` → `#8a9a76`

2. **卡片容器:** `rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)]`

3. **表头文字:** `text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]`

4. **数值:** `font-[var(--font-mono)]`

5. **按钮:** `rounded-[var(--radius-md)] px-[var(--space-3)] py-[var(--space-1)] text-[12px]`

6. **Badge:** `rounded-[var(--radius-full)] px-3 py-1 text-[11px] font-bold` + 角色/状态对应颜色

7. **保持不变:** 所有逻辑、事件处理、store 绑定、props 接口、i18n 调用

## 执行计划

### Phase 1: Committee 子 Tab (5 文件, 1570 行)
- `CommitteeReplayTab.svelte` (508行) — 回放卡片+时间线+verdict badge
- `CommitteeArchiveTab.svelte` (113行) — 归档列表+badge
- `CommitteeRolesTab.svelte` (327行) — 角色配置表单+tool badges
- `CommitteeAccuracyTab.svelte` (250行) — 准确率表格+统计卡片
- `CommitteeToolsTab.svelte` (272行) — 工具面板+角色工具分配

### Phase 2: Strategy + Trades (2 文件, 301 行)
- `StrategyTab.svelte` (153行) — 策略 CRUD 列表
- `TradeLogTab.svelte` (148行) — 交易日志表格+过滤

### Phase 3: System 子 Tab (8 文件, 1227 行)
- `SchedulerTab.svelte` (253行) — Cron 任务列表+状态
- `SystemRegimeTab.svelte` (90行) — Regime 数据展示
- `EventWatchTab.svelte` (218行) — 事件监控列表+过滤
- `SystemDatasourceTab.svelte` (110行) — 数据源状态
- `SystemPnlHistoryTab.svelte` (85行) — PnL 历史表格
- `InsightsFeed.svelte` (189行) — 洞察列表+过滤
- `SystemDreamsTab.svelte` (110行) — Dream 列表
- `UserProfileSection.svelte` (214行) — 用户档案表单

### Phase 4: 通用组件 (6 文件, 1145 行)
- `TradeDialog.svelte` (194行) — 交易对话框
- `EventTriggerDialog.svelte` (130行) — 事件触发对话框
- `DebateBlock.svelte` (105行) — 辩论块
- `PipelineFlow.svelte` (173行) — 管道流程图
- `DreamingConfigPanel.svelte` (331行) — Dreaming 配置面板
- `ProviderConfigPanel.svelte` (209行) — Provider 配置面板

### Phase 5: 页面框架 (1 文件, 258 行)
- `+page.svelte` — 顶部 header + tab 导航 + sub-tab 导航样式

### Phase 6: 验证
- `npm run check`
- `npm run lint`
- `npm run build`
