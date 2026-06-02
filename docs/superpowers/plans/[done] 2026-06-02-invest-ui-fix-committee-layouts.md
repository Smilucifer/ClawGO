# [done] 2026-06-02 — Invest UI 修复 + 委员会子页布局重构

## Context

v5.2.2（commit 6093d80）声称完成 /invest 全模块 UI 设计系统统一，但实际有 4 类破坏性问题：

1. **CSS token 语义冲突（最严重）**：`src/app.css` 把 shadcn 的 `--accent` 定义为 `#2a2827`（暗灰块，shadcn 内部用作 secondary 表面色），而金色品牌色挂在 `--accent-color` 和 `--primary` 上。但 v5.2.2 的 28 个 invest 组件都把 `var(--accent)` 当成"金色"在用 —— 例如 `bg-[var(--accent)] text-[var(--bg-base)]` 导致按钮变成"暗灰底+几乎一样的暗色字"，用户原话"字都看不见"。
2. **Committee → Replay 没按 mockup 重构布局**：当前还是旧的 mode toggle + 单栏式，mockup（`docs/ui-demo/pages/invest-v2.html:849-880`）设计是 250px 标的列表 + 历史日期 / 1fr 报告内容 的双栏。
3. **Committee → Archive 没按 mockup 重构布局**：双栏架子在，但查询按钮文字不可见、归档项缺 verdict badge。
4. **Committee → Tools 没有"角色-工具访问矩阵"真表格**：当前是 5 个角色卡片堆叠展示 tools 数组，看不出"哪个工具谁能调"，且工具列表只有 5 个（后端 `tools.rs` 实际注册了 9 个）。

## Authoritative Facts

### 后端工具/角色映射（以 `src-tauri/src/invest/committee/tools.rs:184-206` 为准）

Pipeline 8 步：Macro → REGIME(计算步，非 LLM) → Quant R1 → Risk R1 → Quant R2 → Risk R2 → L4 Officer → CIO。**Macro 只跑 R1**，R2 round 返回 `None`。Quant/Risk 在 R1+R2 用同一组工具。CIO 始终无工具。

| 工具 | Macro | Quant | Risk | L4 | CIO |
|---|---|---|---|---|---|
| `get_history_data` | ✓ R1 | ✓ R1+R2 | ✗ | ✗ | ✗ |
| `analyze_multi_timeframe` | ✓ R1 | ✓ R1+R2 | ✗ | ✗ | ✗ |
| `get_macro_snapshot` | ✓ R1 | ✗ | ✗ | ✗ | ✗ |
| `query_dreaming_insights` | ✓ R1 | ✗ | ✓ R1+R2 | ✓ | ✗ |
| `get_recent_committee_verdicts` | ✓ R1 | ✓ R1+R2 | ✓ R1+R2 | ✗ | ✗ |
| `get_recent_events` | ✓ R1 | ✗ | ✗ | ✗ | ✗ |
| `get_moneyflow` | ✗ | ✓ R1+R2 | ✗ | ✗ | ✗ |
| `get_company_info` | ✗ | ✓ R1+R2 | ✗ | ✗ | ✗ |
| `get_company_news` | ✗ | ✗ | ✓ R1+R2 | ✗ | ✗ |

### CSS Token 现状

- `src/app.css:16` `--accent: 20 7% 16%` → `#2a2827`（shadcn secondary 暗灰）
- `src/app.css:49` `--accent-color: hsl(var(--primary))` → `#c9a96e`（金色）
- mockup `docs/ui-demo/design-system.css:24` `--accent: #c9a96e` ← 命名跟 shadcn 直接冲突，是这次出错的导火索

## Plan

### Task 1：修复 CSS token 冲突（在 app.css 加 alias，零侵入）

**文件**：`src/app.css`

**做法**：保留 shadcn 原 `--accent`/`--accent-foreground` 不动（避免破坏 shadcn 组件 hover/secondary 表面），新增一组面向 invest 的金色 alias。最简方案：直接给 `--accent` 在 invest 期望的语义下加覆盖。但因为 shadcn 组件也用同名变量，所以正确做法是 **不改 `--accent`，而是在 `src/app.css` 第 49 行附近确认 `--accent-color` 是金色后**，让 invest 组件改用 `--accent-color`。

**两种实现选其一**：

**采用作用域覆盖方案**：在 `src/app.css` 的 `@layer base :root {}` 块末尾新增一段 `[data-invest-scope]` 选择器，只在 invest 路由子树覆盖关键 token；然后在 `src/routes/invest/+page.svelte` 的根 `<div>` 上加 `data-invest-scope` 属性。

```css
/* invest module: align with design-system.css demo palette */
[data-invest-scope] {
  /* Brand gold accent (vs shadcn neutral) */
  --accent: hsl(var(--primary));
  --accent-foreground: var(--bg-base);

  /* Warm muted error (vs shadcn vivid red) — affects SELL badge, danger buttons, error state */
  --color-error: #a87a7a;
  --color-error-bg: rgba(168, 122, 122, 0.12);

  /* Input one shade lighter than card — restores form layering */
  --bg-input: #2e2c29;

  /* Exact demo bg values (1-2% lightness alignment) */
  --bg-card: #242220;
  --bg-elevated: #242220;
  --bg-sidebar: #1e1d1b;
}
```

这样：
- `var(--accent)` 在 invest 子树返回金色，shadcn 在 `/chat`/`/settings`/`/history` 保持原暗灰 secondary 语义
- SELL badge、错误提示、危险按钮回到 demo 的暖灰红 `#a87a7a`，跟整体暖色调对齐
- HoldingsTable / TradeDialog / 各表单的输入框比卡片亮一档，恢复视觉层次
- card / elevated / sidebar 精确对齐 demo（之前偏差 1–2% lightness）

**不覆盖**的项：`--text-tertiary`（计算值偏差小于 5% lightness，肉眼难辨）、`--border`（实色 vs 半透明，视觉相近）、`--color-success`/`--color-warning`（已经一致）。

验证：`/invest` 下金色按钮、active 状态、tab underline 恢复；SELL badge 暖灰红；input 框跟 card 有层次。`/chat`、`/settings` 视觉不变。

### Task 2：重写 CommitteeReplayTab 双栏布局

**文件**：`src/lib/components/invest/CommitteeReplayTab.svelte`

**保留**：mode toggle（replay/simulate）逻辑、symbol selector、simulate 模式整段流程、`STEP_DEFS`/`getStepState`/`getRoundForStep` 工具函数、`investCommitteeStore.loadArchive` 调用。

**重构 replay 模式 UI**：参照 mockup `invest-v2.html:849-880`：
- 顶部：mode toggle pills（保留）
- 主体改为 grid `250px 1fr`：
  - 左 sidebar：标的列表（`allHoldings`，HOLD+WATCH 合并），点击切换 `symbol`；下方分隔线后是历史日期列表（来自 `archives` 数组），点击切换 `selectedDate`
  - 右 content：选中归档的报告内容（`selectedArchive.content`，monospace pre-wrap）
- 移除当前的"单栏 + 顶部下拉 + 日期 chip 行"
- 响应式：窗口窄时降为单栏（参考 mockup `@media`）

**simulate 模式**：保持当前实现（rounds 选择 + 流式 step cards + CIO verdict），不动。

### Task 3：重写 CommitteeArchiveTab 布局

**文件**：`src/lib/components/invest/CommitteeArchiveTab.svelte`

参照 mockup `invest-v2.html:883-907` 双栏：
- 左 250px：select（symbol）+ "查询" 按钮（修复按钮文字对比度，背景 `--accent` 在 Task 1 后会变金色，文字用 `var(--bg-base)` 即可可见）+ 日期列表（每项显示 `日期 + verdict badge`，需要从 archive 内容里 parse 出 verdict 字段，或扩展 `ArchivedDecision` 类型）
- 右 1fr：归档详情（标题 + 元信息 + Markdown 内容）

**verdict badge 数据来源**：`ArchivedDecision` 当前只有 `{date, symbol, content}`。两种处理：
- 若 content 是 markdown 且包含可解析的"判决: BUY"行 → 用正则 parse 出来（前端纯展示，不改后端）
- 若不可靠 → 这一版先不显示 verdict badge，只显示日期

**采用方案**：先尝试在 content 里 regex match `/(?:CIO 判决|verdict|裁决)[:：]?\s*\*?\*?(BUY|ACCUMULATE|HOLD|TRIM|SELL|WATCH)\*?\*?/i`，匹配到才渲染 badge。

### Task 4：重写 CommitteeToolsTab 添加访问矩阵真表格

**文件**：`src/lib/components/invest/CommitteeToolsTab.svelte`

**保留**：3 KPI 卡（总调用/成功率/平均延迟）、tool call history 列表、role 颜色函数、过滤下拉。

**替换**：把当前 `ROLE_ACCESS` 数组（5 项 × tools[] 列表）替换为 9×5 的真表格。

**新数据结构**（位于组件内 `<script>`，与后端 `tools.rs` 一一对应）：
```typescript
interface ToolMatrixCell { macro: string; quant: string; risk: string; l4: string; cio: string; }
const TOOLS_MATRIX: { name: string; descKey: string; access: ToolMatrixCell }[] = [
  { name: 'get_history_data', descKey: 'invest_tool_history_desc',
    access: { macro: 'R1', quant: 'R1+R2', risk: '', l4: '', cio: '' } },
  { name: 'analyze_multi_timeframe', descKey: 'invest_tool_mtf_desc',
    access: { macro: 'R1', quant: 'R1+R2', risk: '', l4: '', cio: '' } },
  { name: 'get_macro_snapshot', descKey: 'invest_tool_macro_desc',
    access: { macro: 'R1', quant: '', risk: '', l4: '', cio: '' } },
  { name: 'query_dreaming_insights', descKey: 'invest_tool_dreaming_desc',
    access: { macro: 'R1', quant: '', risk: 'R1+R2', l4: '✓', cio: '' } },
  { name: 'get_recent_committee_verdicts', descKey: 'invest_tool_verdicts_desc',
    access: { macro: 'R1', quant: 'R1+R2', risk: 'R1+R2', l4: '', cio: '' } },
  { name: 'get_recent_events', descKey: 'invest_tool_events_desc',
    access: { macro: 'R1', quant: '', risk: '', l4: '', cio: '' } },
  { name: 'get_moneyflow', descKey: 'invest_tool_moneyflow_desc',
    access: { macro: '', quant: 'R1+R2', risk: '', l4: '', cio: '' } },
  { name: 'get_company_info', descKey: 'invest_tool_company_info_desc',
    access: { macro: '', quant: 'R1+R2', risk: '', l4: '', cio: '' } },
  { name: 'get_company_news', descKey: 'invest_tool_company_news_desc',
    access: { macro: '', quant: '', risk: '', l4: 'R1+R2', cio: '' } }, // 注：Risk 列, mockup 误填
];
```
**修正上方笔误**：最后一行 `risk: 'R1+R2'`，不是 l4。

**渲染**：`<table>` 9 行 × 6 列（工具名 + 5 角色），单元格规则：
- 空字符串 → 渲染 `✗` 灰色
- 非空 → 渲染 `✓ {value}` 绿色（`#8a9a76`）
- 表头使用 `--text-tertiary` 小写 uppercase

**i18n**：新增 9 个 `invest_tool_*_desc` key（en/zh-CN 各一），desc 用作行 hover tooltip。

### Task 5：i18n 补齐

**文件**：`messages/zh-CN.json` 和 `messages/en.json`

新增 keys：
- `invest_tools_matrix_title` / "角色-工具访问矩阵" / "Role-Tool Access Matrix"
- `invest_tools_col_tool` / "工具" / "Tool"
- `invest_tool_history_desc` 等 9 个工具描述（中文用 `tools.rs` 里的 `description` 字段直译）

### Task 6：验证

```bash
npm run check                # svelte-check 类型
npm run lint                 # ESLint
npm run i18n:check           # i18n key 完整性
npm run build                # 生产构建
cargo check --manifest-path src-tauri/Cargo.toml  # Rust 编译（不改后端但确认无连带）
```

手工回归（`npm run tauri dev`）：
1. **CSS 修复验证**：进 /invest，所有 tab 的金色按钮/标题/active 状态可见；切到 /chat 和 /settings，shadcn secondary 暗灰表面色不变（hover、popover 等）
2. **Replay 双栏**：进 Committee → Replay，左侧标的列表显示当前 HOLD+WATCH，点击切换；下方历史日期列表（先 mock 一个有归档记录的标的）；右侧显示报告
3. **Archive**：选 symbol → 点查询 → 左列出现日期+verdict badge → 点日期 → 右显示完整内容
4. **Tools 矩阵**：进 Committee → Tools，9 行 × 5 角色列的表格，每格 ✓R1+R2 / ✓R1 / ✓ / ✗ 一目了然

## Critical Files

- 新增：`src/app.css`（追加 `[data-invest-scope]` 段落）
- 修改：`src/routes/invest/+page.svelte`（根 div 加 `data-invest-scope` 属性，1 行）
- 重写：`src/lib/components/invest/CommitteeReplayTab.svelte`
- 重写：`src/lib/components/invest/CommitteeArchiveTab.svelte`
- 重写：`src/lib/components/invest/CommitteeToolsTab.svelte`
- 增量：`messages/en.json`、`messages/zh-CN.json`

不动：后端、其他 25 个 invest 组件、其他模块。

## Out of Scope

- CommitteeLiveTab（用户没抱怨）
- CommitteeRolesTab（用户没抱怨）
- 其他 system / strategy / trades / dashboard 组件的 UI
- 后端 `tools.rs` 工具集变更
- mockup 里 Risk 列 `get_company_news` 的真值（已对照后端确认正确为 R1+R2）
