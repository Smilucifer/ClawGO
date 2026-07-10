# 委员会「最终裁决」卡片 Markdown 渲染优化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让委员会 Live / Replay tab 的「最终裁决」正文从纯文本改为 Markdown 渲染，使标题/加粗/表格/列表/emoji 正确成型，提升可读性。

**Architecture:** 纯前端展示层改动。复用项目现有的 `MarkdownContent.svelte`（marked + DOMPurify）。顶部结构化徽章区保留不动，只把正文 `result.reasoning` 的纯文本渲染替换为 `MarkdownContent`，并调整 CSS 以适配密集暗色卡片。

**Tech Stack:** SvelteKit (Svelte 5 runes)、现有 `MarkdownContent.svelte`（基于 `marked` v15 + DOMPurify）、Tailwind prose。

## Global Constraints

- Windows-first，不引入 WSL/macOS/Linux 假设。
- 不改后端、不改类型定义（`CommitteeResult.reasoning: string` 不变）。
- 不新增 i18n 文案（`en.json` / `zh-CN.json` 不动）。
- Svelte 5 runes 语法。
- Conventional Commits。
- `MarkdownContent` 的 `class` prop 追加在内置 `prose prose-sm dark:prose-invert max-w-none ...` 之后，只能叠加/覆盖，无法摆脱 prose。
- `reasoning` 一次性到达（非逐字流），不传 `streaming`。

---

### Task 1: Live tab 裁决正文改用 MarkdownContent

**Files:**
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`（import 区 1-15 行附近；473 行渲染处；792 行 CSS）

**Interfaces:**
- Consumes: `MarkdownContent`（`text: string`、`class?: string` props，默认导出，路径 `$lib/components/MarkdownContent.svelte`）；`result.reasoning: string`（已在作用域，369 行 `const result = p?.result ?? null`）。
- Produces: 无下游依赖。

- [ ] **Step 1: 新增 MarkdownContent import**

在 `<script lang="ts">` 的 import 区（现有 import 之后，约第 13 行 `import { onMount } from 'svelte';` 下方）加入：

```svelte
  import MarkdownContent from '$lib/components/MarkdownContent.svelte';
```

- [ ] **Step 2: 替换 473 行的纯文本渲染**

将：

```svelte
                <div class="verdict-reasoning">{result.reasoning}</div>
```

改为：

```svelte
                {#if result.reasoning}
                  <MarkdownContent text={result.reasoning} class="verdict-reasoning" />
                {/if}
```

- [ ] **Step 3: 调整 .verdict-reasoning CSS 以适配 Markdown 块级元素**

将 792 行的：

```css
  .verdict-reasoning { font-size: 12.5px; color: var(--text-secondary); line-height: 1.8; }
```

替换为（用 `:global()` 穿透 MarkdownContent 容器内子元素，收紧 prose 默认间距，适配密集暗色卡片）：

```css
  .verdict-reasoning { font-size: 12.5px; color: var(--text-secondary); line-height: 1.8; }
  .verdict-reasoning :global(h1),
  .verdict-reasoning :global(h2),
  .verdict-reasoning :global(h3),
  .verdict-reasoning :global(h4) {
    font-size: 13px; font-weight: 600; color: var(--text-primary);
    margin: 10px 0 4px; line-height: 1.4;
  }
  .verdict-reasoning :global(p) { margin: 4px 0; }
  .verdict-reasoning :global(ul),
  .verdict-reasoning :global(ol) { margin: 4px 0; padding-left: 18px; }
  .verdict-reasoning :global(li) { margin: 2px 0; }
  .verdict-reasoning :global(strong) { color: var(--text-primary); font-weight: 600; }
  .verdict-reasoning :global(table) {
    border-collapse: collapse; margin: 6px 0; font-size: 11.5px; width: auto;
  }
  .verdict-reasoning :global(th),
  .verdict-reasoning :global(td) {
    border: 1px solid var(--border); padding: 3px 8px; text-align: left;
  }
  .verdict-reasoning :global(th) { background: var(--bg-base); font-weight: 600; }
  .verdict-reasoning :global(:first-child) { margin-top: 0; }
  .verdict-reasoning :global(:last-child) { margin-bottom: 0; }
```

- [ ] **Step 4: 构建校验**

Run: `npm run check`
Expected: 无新增 TS / Svelte 错误（涉及本文件）。

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte
git commit -m "feat(invest): 委员会 Live tab 最终裁决正文改用 Markdown 渲染"
```

---

### Task 2: Replay tab 裁决正文改用 MarkdownContent

**Files:**
- Modify: `src/lib/components/invest/CommitteeReplayTab.svelte`（438-442 行渲染处；CSS 区按需补充）

**Interfaces:**
- Consumes: `MarkdownContent`（已在第 10 行 import，直接复用）；`result.reasoning: string`（已在作用域）。
- Produces: 无下游依赖。

- [ ] **Step 1: 替换 438-442 行的纯文本渲染**

将：

```svelte
            {#if result.reasoning}
              <div class="max-h-32 overflow-y-auto whitespace-pre-wrap text-xs leading-relaxed text-[var(--text-secondary)]">
                {result.reasoning}
              </div>
            {/if}
```

改为（保留滚动容器与高度限制，内层用 MarkdownContent；通过 `class` 传入 invest 局部样式类名）：

```svelte
            {#if result.reasoning}
              <div class="max-h-32 overflow-y-auto">
                <MarkdownContent text={result.reasoning} class="replay-reasoning" />
              </div>
            {/if}
```

- [ ] **Step 2: 新增 .replay-reasoning CSS**

在该文件的 `<style>` 块内（若无 `<style>` 块则在文件末尾新增一个 `<style>` 块）加入：

```css
  .replay-reasoning { font-size: 12px; color: var(--text-secondary); line-height: 1.7; }
  .replay-reasoning :global(h1),
  .replay-reasoning :global(h2),
  .replay-reasoning :global(h3),
  .replay-reasoning :global(h4) {
    font-size: 12.5px; font-weight: 600; color: var(--text-primary);
    margin: 8px 0 3px; line-height: 1.4;
  }
  .replay-reasoning :global(p) { margin: 3px 0; }
  .replay-reasoning :global(ul),
  .replay-reasoning :global(ol) { margin: 3px 0; padding-left: 16px; }
  .replay-reasoning :global(li) { margin: 2px 0; }
  .replay-reasoning :global(strong) { color: var(--text-primary); font-weight: 600; }
  .replay-reasoning :global(table) {
    border-collapse: collapse; margin: 5px 0; font-size: 11px; width: auto;
  }
  .replay-reasoning :global(th),
  .replay-reasoning :global(td) {
    border: 1px solid var(--border); padding: 3px 7px; text-align: left;
  }
  .replay-reasoning :global(th) { background: var(--bg-base); font-weight: 600; }
  .replay-reasoning :global(:first-child) { margin-top: 0; }
  .replay-reasoning :global(:last-child) { margin-bottom: 0; }
```

注意：实现前先 Read 该文件确认是否已有 `<style>` 块及其位置，避免重复声明。

- [ ] **Step 3: 构建校验**

Run: `npm run check`
Expected: 无新增 TS / Svelte 错误（涉及本文件）。

- [ ] **Step 4: Commit**

```bash
git add src/lib/components/invest/CommitteeReplayTab.svelte
git commit -m "feat(invest): 委员会 Replay tab 最终裁决正文改用 Markdown 渲染"
```

---

### Task 3: 整体验证

**Files:**
- 无修改，仅运行校验命令。

- [ ] **Step 1: 全量构建**

Run: `npm run build`
Expected: 构建成功，无报错。

- [ ] **Step 2: i18n 校验（确认未误改文案）**

Run: `npm run i18n:check`
Expected: 通过。

- [ ] **Step 3: 目视验证（开发环境）**

Run: `npm run tauri dev`
跑一次委员会，确认 Live tab 与 Replay tab 的「最终裁决」卡片：
- 标题 / 加粗 / 表格 / 列表 / emoji 正确渲染。
- 排版清晰，无样式溢出、不撑破卡片布局。
- 顶部裁决徽章、置信度、Gate、meta 信息仍正常显示。

- [ ] **Step 4: 更新 changelog（可选，按项目惯例）**

按 `docs/changelog.md` 现有格式补充本次 UI 优化记录。
