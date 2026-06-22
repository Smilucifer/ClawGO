# 委员会「最终裁决」卡片渲染优化

**日期：** 2026-06-23
**范围：** 前端展示层（openInvest 委员会 Live / Replay tab）

## 背景与问题

委员会运行结束后，每个标的会展示一张「最终裁决」卡片。该卡片正文 `result.reasoning` 是后端在 `symbol_complete` 事件里一次性返回的一整段**自由格式 Markdown**（含 `##` 标题、`**加粗**`、表格、列表、emoji 混排）。

当前 `CommitteeLiveTab.svelte:473` 把它当成纯文本直接塞进 DOM：

```svelte
<div class="verdict-reasoning">{result.reasoning}</div>
```

且 `.verdict-reasoning` 的 CSS（792 行）连 `white-space: pre-wrap` 都没有。结果：所有换行折叠、Markdown 源码字符裸露，整段文字挤成一坨，可读性极差（见用户截图）。

对比之下，其他角色卡片（CIO / Macro / Quant / Risk）可读性好，是因为后端已经把它们的输出**预解析成结构化字段**（`RoundOutputSummary.parsed`），前端用 `.field-list` 网格渲染成「字段—值」表格。`reasoning` 是自由文本，无法拆成固定字段。

## 目标

让「最终裁决」卡片像其他卡片一样清晰可读，方向为：**顶部保留结构化字段/徽章，正文用 Markdown 渲染**。

## 方案

裁决卡片维持两层结构：

1. **顶部结构化区（已有，保留不动）** —— `verdict-action`（裁决徽章）、`verdict-confidence`（置信度）、`Gate 1/2` 徽章、`verdict-meta`（耗时/token/收敛）、`sanityCheck.notes`、`sentinelOverride`。这些已是结构化展示。

2. **正文区（核心改动）** —— 把 `{result.reasoning}` 纯文本改用项目现有的 `MarkdownContent` 组件渲染，让标题/加粗/表格/列表/emoji 正确成型。这与 Archive、Replay tab 已有的 `MarkdownContent` 用法一致。

### 复用现有基建

- `src/lib/components/MarkdownContent.svelte` —— 内置 `marked`（gfm）+ DOMPurify 消毒 + 代码块复制 + 相对图片解析。`class` prop 追加在内置 `prose prose-sm dark:prose-invert max-w-none ...` 之后（第 103 行），可叠加覆盖样式，但无法完全摆脱 prose。
- `reasoning` 一次性到达（非逐字流），因此**不传 `streaming`**，无需特殊处理。

## 具体改动点

### 1. `src/lib/components/invest/CommitteeLiveTab.svelte`

- **import**：顶部新增 `import MarkdownContent from "$lib/components/MarkdownContent.svelte";`（该文件当前未引入；Replay/Archive 已有先例）。
- **473 行**：
  ```svelte
  <div class="verdict-reasoning">{result.reasoning}</div>
  ```
  改为（含空值守卫，当前 Live 这处缺守卫，空 `reasoning` 会渲染空 div）：
  ```svelte
  {#if result.reasoning}
    <MarkdownContent text={result.reasoning} class="verdict-reasoning" />
  {/if}
  ```
- **CSS `.verdict-reasoning`（792 行）**：调整为容纳块级元素（标题/表格的 margin、字号 ~12.5px、表格边框、与暗色卡片主题协调），收紧 prose 默认间距，避免把密集卡片撑过大。

### 2. `src/lib/components/invest/CommitteeReplayTab.svelte`

- 该文件已 import `MarkdownContent`（第 10 行），直接复用。
- **438-442 行**：CIO 裁决的 `result.reasoning`（当前是 `whitespace-pre-wrap` 纯文本）改为 `MarkdownContent` 渲染，保持空值守卫 `{#if result.reasoning}`（已存在）。

## 不改动的范围

- 后端、类型定义（`CommitteeResult.reasoning: string` 不变）。
- i18n（无新增文案）。
- 其他角色卡片（CIO/Macro/Quant/Risk）—— 它们已是结构化字段渲染，不在本次范围。
- Archive tab —— 已正确使用 `MarkdownContent`。

## 风险

- **XSS**：`MarkdownContent` 内置 DOMPurify 消毒，无风险。
- **色板差异**：`MarkdownContent` 的 prose 使用 Tailwind 主题 token（`text-foreground` 等），裁决卡用 invest CSS 变量（`--text-secondary` 等），可能有轻微色差。但 Archive/Replay 已接受这种渲染，保持一致即可。
- **唯一打磨项**：正文区 CSS 微调，确保 prose 默认间距不破坏密集暗色卡片布局。属打磨，不影响设计成立。

## 验证

- `npm run build`
- `npm run check`
- `npm run tauri dev` 跑一次委员会，目视确认 Live / Replay 两处裁决卡片：标题/加粗/表格/列表/emoji 正确渲染，排版清晰，无样式溢出。
