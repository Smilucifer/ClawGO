# 盘前舆论卡片 + 盈亏保存 + 路由状态保持

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 三项改动：(1) 重构盘前报告 section 01 标签体系和 UI 布局；(2) 修复盈亏批量录入保存无反应 bug；(3) invest tab 和 chat input 路由切换状态保持。

**Architecture:**
- section01: Rust prompt+struct → JSON → Svelte 组件渲染，`AiSector` 增加正/负面计数，grid 自适应 1-2 行
- 盈亏 bug: `$effect` 初始化 guard + 错误反馈 + ConfirmDialog async 支持
- 路由状态: invest tab 改 CSS display 保持挂载；chat input 自动 stash/restore

**Tech Stack:** Rust (serde), Svelte 5 runes, CSS Grid, SvelteKit routing

## Spec

### 标签体系重定义

| 旧 tag | 新 tag | 方向 | 判定规则 |
|---|---|---|---|
| 新闻强 | 利好密集 | 🟢 看多 | 该板块下 [bullish] 新闻占多数 |
| 催化强 | 催化驱动 | 🟢 看多 | 有明确利好事件（政策利好/订单/中标），且方向偏多 |
| 情绪强 | 情绪转弱 | 🔴 看空 | 该板块下 [bearish] 新闻占多数，市场情绪偏悲观 |
| 分歧大 | 分歧大 | ⚪ 中性 | bullish 与 bearish 数量接近，多空交锋 |
| 风险预警 | 风险预警 | 🔴 看空 | 监管处罚/退市/政策转向/地缘扰动等硬风险 |

### 正面/负面计数

- `AiSector` 增加 `positive_count: u32`、`negative_count: u32`
- 判定来源：`build_news_block_for_ai()` 已在每条新闻前拼 `[bullish]/[bearish]/[neutral]` stance 标签，LLM 据此计数
- UI 展示：sector 卡片内 `3↑ 2↓` 样式，↑ 用 `--up` 色，↓ 用 `--down` 色

### UI 布局

```
≤ 4 个 sector → 一行均分（4列网格，现状不变）
5 个 sector   → 3列网格 → 3+2 自动换行
6 个 sector   → 3列网格 → 3+3
7 个 sector   → 4列网格 → 4+3
8 个 sector   → 4列网格 → 4+4
```

`tone`（基调总述）保持在卡片墙下方，不动。

### 向后兼容

缓存的旧报告 tag 仍为旧名（新闻强/催化强/情绪强），`evalClass()` 映射表同时覆盖新旧 tag 名。

## Global Constraints

- 仅改 `report.rs` 和 `PremarketReportTab.svelte` 两个文件，不动其他模块
- `AiSector` 仅在 `report.rs` 使用，无外部依赖
- i18n：tag 名为 LLM 生成的中文，不需要 i18n key；正面/负面计数符号 `↑↓` 为纯文本
- 不改 SABC 评分逻辑，AI 点评仅影响展示层

---

### Task 1: Rust — 重写 AiSector 结构体 + prompt + 渲染

**Files:**
- Modify: `src-tauri/src/invest/premarket/report.rs:503-530` (AiSector struct + ai_commentary prompt)
- Modify: `src-tauri/src/invest/premarket/report.rs:571-581` (render_ai_commentary_md)

**Interfaces:**
- Produces: `AiSector { name, tag, count, positive_count, negative_count, note }` — serialized as camelCase JSON, consumed by frontend Task 2

- [ ] **Step 1: 修改 AiSector 结构体，增加 positive_count 和 negative_count**

在 `src-tauri/src/invest/premarket/report.rs` 第 505-510 行，将：

```rust
pub struct AiSector {
    pub name: String,
    pub tag: String,
    pub count: u32,
    pub note: String,
}
```

改为：

```rust
pub struct AiSector {
    pub name: String,
    pub tag: String,
    pub count: u32,
    pub positive_count: u32,
    pub negative_count: u32,
    pub note: String,
}
```

- [ ] **Step 2: 重写 ai_commentary 的 prompt**

在 `src-tauri/src/invest/premarket/report.rs` 第 526-529 行，将 prompt 从：

```rust
let prompt = format!(
    "你是A股盘前分析师。把以下新闻聚合成3-5个板块，每个给：name、tag(只能选:新闻强/催化强/情绪强/分歧大/风险预警)、count、note(一句话)。\
     风险预警专收监管\政策转向\处罚退市\地缘扰动。输出JSON: {{\"sectors\":[...],\"tone\":\"基调总述\"}}。只输出JSON。\n\n{}",
    news_block
);
```

改为：

```rust
let prompt = format!(
    "你是A股盘前分析师。把以下新闻聚合成3-5个板块，每个板块输出以下字段：\n\
     - name：板块名\n\
     - tag（只能选以下5个之一）：\n\
       · 利好密集 — 该板块正面舆情[bullish]占多数\n\
       · 催化驱动 — 有明确利好事件（政策利好/重大订单/中标/回购增持），方向偏多\n\
       · 情绪转弱 — 该板块负面舆情[bearish]占多数，市场情绪偏悲观\n\
       · 分歧大 — bullish与bearish数量接近，多空交锋激烈\n\
       · 风险预警 — 监管处罚/退市/政策转向/地缘扰动等硬风险（高严重度bearish直接归此类）\n\
     - count：该板块新闻总条数\n\
     - positive_count：该板块中stance为bullish的新闻条数\n\
     - negative_count：该板块中stance为bearish的新闻条数\n\
     - note：一句话总结该板块核心信息\n\
     风险预警板块的grid-column占满整行（JSON中不用管，前端处理）。\n\
     输出JSON: {{\"sectors\":[{{\"name\":\"...\",\"tag\":\"...\",\"count\":N,\"positive_count\":N,\"negative_count\":N,\"note\":\"...\"}}],\"tone\":\"基调总述\"}}。只输出JSON。\n\n{}",
    news_block
);
```

- [ ] **Step 3: 更新 render_ai_commentary_md 展示正面/负面计数**

在 `src-tauri/src/invest/premarket/report.rs` 第 574-578 行，将：

```rust
for s in &ai.sectors {
    md.push_str(&format!(
        "- **{}**（{}，{} 条）：{}\n",
        s.name, s.tag, s.count, s.note
    ));
}
```

改为：

```rust
for s in &ai.sectors {
    md.push_str(&format!(
        "- **{}**（{}，{} 条，{}↑ {}↓）：{}\n",
        s.name, s.tag, s.count, s.positive_count, s.negative_count, s.note
    ));
}
```

- [ ] **Step 4: cargo check 验证编译通过**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过，无 error

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/premarket/report.rs
git commit -m "feat(invest): add positive/negative counts to AiSector, rewrite premarket commentary prompt"
```

---

### Task 2: 前端 — 更新类型 + 标签样式 + 网格布局 + 卡片 UI

**Files:**
- Modify: `src/lib/components/invest/PremarketReportTab.svelte:44-49` (AiSector interface)
- Modify: `src/lib/components/invest/PremarketReportTab.svelte:187-194` (wallClass logic)
- Modify: `src/lib/components/invest/PremarketReportTab.svelte:352-356` (evalClass function)
- Modify: `src/lib/components/invest/PremarketReportTab.svelte:516-531` (sector card template)
- Modify: `src/lib/components/invest/PremarketReportTab.svelte:951-974` (CSS grid + eval-tag styles)

**Interfaces:**
- Consumes: `AiSector` JSON from Task 1 (camelCase: `positiveCount`, `negativeCount`)
- Produces: section 01 渲染结果

- [ ] **Step 1: 更新 AiSector 接口**

在 `src/lib/components/invest/PremarketReportTab.svelte` 第 44-49 行，将：

```typescript
interface AiSector {
  name: string;
  tag: string; // 新闻强 / 催化强 / 情绪强 / 分歧大 / 风险预警
  count: number;
  note: string;
}
```

改为：

```typescript
interface AiSector {
  name: string;
  tag: string; // 利好密集 / 催化驱动 / 情绪转弱 / 分歧大 / 风险预警
  count: number;
  positiveCount: number;
  negativeCount: number;
  note: string;
}
```

- [ ] **Step 2: 更新 wallClass 自适应逻辑**

在 `src/lib/components/invest/PremarketReportTab.svelte` 第 187-194 行，将：

```typescript
const wallClass = $derived.by(() => {
  if (!commentary) return 'wall-3col';
  const n = commentary.sectors.length;
  if (hasRisk) return 'wall-3col';
  if (n <= 4) return `wall-n${n}`;
  if (n === 5) return 'wall-1plus4';
  return 'wall-3col';
});
```

改为：

```typescript
const wallClass = $derived.by(() => {
  if (!commentary) return 'wall-3col';
  const n = commentary.sectors.length;
  if (n <= 4) return `wall-n${n}`;
  // 5-6 → 3列(3+2 / 3+3), 7-8 → 4列(4+3 / 4+4)
  return 'wall-3col';
});
```

同时删除 `hasRisk` 变量（第 184-186 行），因为它不再影响 wallClass：

```rust
// 删除这三行：
const hasRisk = $derived(
  !!commentary && commentary.sectors.some((s) => s.tag.includes('风险')),
);
```

- [ ] **Step 3: 重写 evalClass，兼容新旧 tag 名**

在 `src/lib/components/invest/PremarketReportTab.svelte` 第 352-356 行，将：

```typescript
function evalClass(tag: string): string {
  // Demo tags: 新闻强 / 催化强 / 情绪强 / 分歧大 / 风险预警
  if (tag.includes('新闻') || tag.includes('催化')) return 'cata';
  if (tag.includes('情绪')) return 'mood';
  if (tag.includes('分歧')) return 'split';
  if (tag.includes('风险')) return 'risk';
  return 'news';
}
```

改为：

```typescript
function evalClass(tag: string): string {
  // 新 tag: 利好密集 / 催化驱动 / 情绪转弱 / 分歧大 / 风险预警
  // 旧 tag 兼容: 新闻强 / 催化强 / 情绪强
  if (tag.includes('利好') || tag.includes('新闻')) return 'bull';
  if (tag.includes('催化')) return 'cata';
  if (tag.includes('情绪转弱')) return 'bear';
  if (tag.includes('情绪强')) return 'split';   // 旧 tag 方向不明，映射中性色
  if (tag.includes('分歧')) return 'split';
  if (tag.includes('风险')) return 'risk';
  return 'bull';
}
```

- [ ] **Step 4: 重写 sector 卡片模板，展示正面/负面计数**

在 `src/lib/components/invest/PremarketReportTab.svelte` 第 516-531 行，将 sector 卡片渲染从：

```svelte
<div class="theme-wall {wallClass}">
  {#each commentary.sectors as sec, i}
    <div
      class="theme-tag-card"
      class:ttc-first={i === 0 && wallClass === 'wall-1plus4'}
      style={sec.tag.includes('风险') ? 'grid-column: 1 / -1;' : ''}
    >
      <div class="ttc-head">
        <span class="ttc-name">{sec.name}</span>
        <span class="eval-tag {evalClass(sec.tag)}">{sec.tag}</span>
        <span class="ttc-count">{sec.count} {t('invest_premarket_news_count_unit')}</span>
      </div>
      <div class="ttc-desc">{sec.note}</div>
    </div>
  {/each}
</div>
```

改为：

```svelte
<div class="theme-wall {wallClass}">
  {#each commentary.sectors as sec}
    <div
      class="theme-tag-card"
      style={sec.tag.includes('风险') ? 'grid-column: 1 / -1;' : ''}
    >
      <div class="ttc-head">
        <span class="eval-tag {evalClass(sec.tag)}">{sec.tag}</span>
        <span class="ttc-name">{sec.name}</span>
        <span class="ttc-sentiment">
          {#if sec.positiveCount > 0}<span class="sent-up">{sec.positiveCount}↑</span>{/if}
          {#if sec.negativeCount > 0}<span class="sent-down">{sec.negativeCount}↓</span>{/if}
        </span>
        <span class="ttc-count">{sec.count}{t('invest_premarket_news_count_unit')}</span>
      </div>
      <div class="ttc-desc">{sec.note}</div>
    </div>
  {/each}
</div>
```

- [ ] **Step 5: 更新 CSS — grid 布局 + 新 tag 样式 + 正负面计数样式**

在 `src/lib/components/invest/PremarketReportTab.svelte` 的样式区域，将第 951-974 行的 grid 和 eval-tag 样式替换为：

```css
/* --- theme wall grid --- */
.theme-wall { display: grid; gap: var(--space-2); grid-template-columns: repeat(4, 1fr); }
.theme-wall.wall-n1 { grid-template-columns: 1fr; }
.theme-wall.wall-n2 { grid-template-columns: repeat(2, 1fr); }
.theme-wall.wall-n3 { grid-template-columns: repeat(3, 1fr); }
.theme-wall.wall-n4 { grid-template-columns: repeat(4, 1fr); }
.theme-wall.wall-3col { grid-template-columns: repeat(3, 1fr); }

/* --- sector tag card --- */
.theme-tag-card {
  background: var(--surface-1);
  border: 1px solid var(--border-subtle);
  border-radius: var(--radius-sm);
  padding: var(--space-2) var(--space-3);
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.ttc-head { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
.ttc-name { font-size: 12px; font-weight: 600; color: var(--text-primary); }
.ttc-desc { font-size: 11px; color: var(--text-secondary); line-height: 1.4; }
/* ttc-sentiment 用 margin-left:auto 推到右侧，ttc-count 紧跟其后不加 auto */
.ttc-count { font-size: 9px; color: var(--text-tertiary); font-family: var(--font-mono); }

/* --- sentiment counts --- */
.ttc-sentiment { display: flex; gap: 4px; margin-left: auto; }
.sent-up { font-size: 10px; font-weight: 700; color: var(--up); font-family: var(--font-mono); }
.sent-down { font-size: 10px; font-weight: 700; color: var(--down); font-family: var(--font-mono); }

/* --- eval tag badges --- */
.eval-tag { font-size: 10px; font-weight: 700; padding: 2px 8px; border-radius: 4px; white-space: nowrap; }
.eval-tag.bull { color: var(--up); background: rgba(192, 82, 74, 0.18); }
.eval-tag.cata { color: var(--accent); background: var(--accent-muted); }
.eval-tag.bear { color: var(--down); background: rgba(78, 154, 95, 0.16); }
.eval-tag.split { color: var(--grade-b); background: rgba(124, 148, 168, 0.16); }
.eval-tag.risk { color: var(--text-primary); background: rgba(168, 122, 122, 0.30); border: 1px solid var(--down); }
```

同时删除已废弃的 `.theme-tag-card.ttc-first` 样式（第 958 行）和 `.theme-wall.wall-1plus4`（第 956 行）。

- [ ] **Step 6: npm run check 验证前端编译**

Run: `npm run check`
Expected: 无 error

- [ ] **Step 7: Commit**

```bash
git add src/lib/components/invest/PremarketReportTab.svelte
git commit -m "feat(invest): redesign premarket section01 tags, add sentiment counts, adaptive 2-row grid"
```

---

### Task 3: i18n + 全量验证

**Files:**
- Check: `messages/en.json`, `messages/zh-CN.json` — 确认无需新增 key（tag 为 LLM 输出，计数为纯数字）
- Verify: `npm run build` + `cargo check`

- [ ] **Step 1: 确认 section01 i18n 无遗漏**

tag 名（利好密集/催化驱动/情绪转弱/分歧大/风险预警）为 LLM 直接输出的中文文本，不走 i18n。计数符号 `↑↓` 为纯 Unicode。`t('invest_premarket_news_count_unit')` 已有。section01 部分无需新增 key。

（Task 4 盈亏修复需要新增 2 个 i18n key，在 Task 4 Step 6 处理。）

- [ ] **Step 2: 全量构建验证**

Run: `npm run build`
Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 两者均通过

- [ ] **Step 3: Commit（如有意外修复）**

```bash
git add -A
git commit -m "chore: verify premarket section01 refactor builds clean"
```

---

## bug1: 盈亏批量录入保存无反应

### Task 4: 修复 FortuneRecordDialog 批量保存

**Root Cause:** `$effect` 第 44-53 行无条件用 `recorded` 重置 `batchVals`。当 `fortuneStore.analysis` 被任何异步操作（如 `loadAll()`）更新时，`recorded` 变化触发 `$effect`，用户已输入的批量数据被清空。`buildBatchEntries()` 返回空数组 → `if (!entries.length) return;` 静默退出，用户看到"保存没反应"。

**次要问题：** `guardedSave` catch 只 `console.error`；`ConfirmDialog.handleConfirm` 不 await 异步回调。

**Files:**
- Modify: `src/lib/components/invest/FortuneRecordDialog.svelte:44-53` ($effect 重置逻辑)
- Modify: `src/lib/components/invest/FortuneRecordDialog.svelte:126-143` (submitBatch 反馈)
- Modify: `src/lib/components/invest/FortuneRecordDialog.svelte:99-117` (handleOverwriteConfirm 反馈)
- Modify: `src/lib/components/ConfirmDialog.svelte:25-28` (handleConfirm await)

**Interfaces:**
- 无新接口，修复内部行为

- [ ] **Step 1: 加 initialized flag，防止 $effect 覆盖用户编辑**

在 `FortuneRecordDialog.svelte` 第 42-53 行，将：

```typescript
let batchVals = $state<Record<string, string>>({});
// 进入某月时用已录旧值预填空白项
$effect(() => {
  // year, month, dates, recorded are tracked; batchVals is only written, not read
  const _y = year; const _m = month; const _d = dates; const _r = recorded;
  const newVals: Record<string, string> = {};
  for (const ds of _d) {
    const old = _r.get(ds);
    newVals[ds] = old != null ? String(old) : '';
  }
  batchVals = newVals;
});
```

改为：

```typescript
let batchVals = $state<Record<string, string>>({});
let batchInitialized = false;
let lastBatchMonth = '';

// 仅在月份切换时预填，不覆盖用户编辑
$effect(() => {
  const _y = year; const _m = month; const _d = dates;
  const monthKey = `${_y}-${_m}`;
  // 每月首次进入时预填已录入值，之后不覆盖
  if (monthKey !== lastBatchMonth) {
    lastBatchMonth = monthKey;
    batchInitialized = false;
  }
  if (!batchInitialized) {
    const _r = recorded;
    const newVals: Record<string, string> = {};
    for (const ds of _d) {
      const old = _r.get(ds);
      newVals[ds] = old != null ? String(old) : '';
    }
    batchVals = newVals;
    batchInitialized = true;
  }
});
```

- [ ] **Step 2: submitBatch 空 entries 时给提示**

在 `FortuneRecordDialog.svelte` 第 126-129 行，将：

```typescript
async function submitBatch() {
  if (saving) return;
  const entries = buildBatchEntries();
  if (!entries.length) return;
```

改为：

```typescript
let batchError = $state('');

async function submitBatch() {
  if (saving) return;
  batchError = '';
  const entries = buildBatchEntries();
  if (!entries.length) {
    batchError = t('fortune_batch_empty_hint');
    return;
  }
```

（`fortune_batch_empty_hint` 需要在 i18n 文件中添加，值为"请先填写收益率数据" / "Please enter return data first"）

- [ ] **Step 3: guardedSave 失败时显示错误**

在 `FortuneRecordDialog.svelte` 的 submitBatch 调用 `guardedSave` 处（第 139-142 行），将：

```typescript
await guardedSave(saving, (v) => saving = v, async () => {
  await fortuneStore.batchUpsert(entries);
  onclose();
});
```

改为：

```typescript
await guardedSave(saving, (v) => saving = v, async () => {
  await fortuneStore.batchUpsert(entries);
  onclose();
}, (e) => {
  console.error('[fortune] batch save failed:', e);
  batchError = t('fortune_batch_save_failed');
});
```

对 `handleOverwriteConfirm` 中的 `guardedSave`（第 113-116 行）做同样处理。

- [ ] **Step 4: ConfirmDialog handleConfirm 支持 async**

在 `ConfirmDialog.svelte` 第 25-28 行，将：

```typescript
function handleConfirm() {
  onConfirm?.();
  open = false;
}
```

改为：

```typescript
async function handleConfirm() {
  await onConfirm?.();
  open = false;
}
```

同时将 `onConfirm` 类型从 `(() => void)` 改为 `(() => void | Promise<void>)`（第 21 行）。

注：项目中仅 2 处使用 ConfirmDialog——`FortuneRecordDialog` 和 `FortuneDataTab`，两者的 `onConfirm` 均为 async（`FortuneDataTab.confirmDelete` 也是 async），改动安全。行为变更：保存期间对话框保持打开（而非立即关闭），用户能看到操作进行中。

- [ ] **Step 5: 在模板中显示 batchError**

在 `FortuneRecordDialog.svelte` 第 185 行保存按钮之后、第 186 行 `{/if}` 之前，添加：

```svelte
        onclick={submitBatch}>批量保存</button>
      {#if batchError}
        <div class="mt-[var(--space-2)] text-[11px] text-[var(--down)]">{batchError}</div>
      {/if}
    {/if}
```

- [ ] **Step 6: 添加 i18n key**

在 `messages/zh-CN.json` 和 `messages/en.json` 中添加：
- `fortune_batch_empty_hint`: "请先填写收益率数据" / "Please enter return data first"
- `fortune_batch_save_failed`: "保存失败，请重试" / "Save failed, please retry"

- [ ] **Step 7: npm run check + commit**

Run: `npm run check`
Expected: 无 error

```bash
git add src/lib/components/invest/FortuneRecordDialog.svelte src/lib/components/ConfirmDialog.svelte messages/
git commit -m "fix(invest): fortune batch save silent failure — effect reset + missing feedback"
```

---

## feature1: 路由切换状态保持

### Task 5: Invest tab 切换保持组件存活

**Root Cause:** `invest/+page.svelte` 使用 `{#if activeTab === 'dashboard'}/{:else if activeTab === 'trades'}` 条件渲染，切换 tab 时组件被 unmount，局部 `$state` 全部丢失。

**方案：** 改为所有 tab 内容同时渲染，用 CSS `display:none` 隐藏非活跃 tab。

**Files:**
- Modify: `src/routes/invest/+page.svelte:178-340` (tab 内容区域)

**Interfaces:**
- 无接口变更，纯 UI 行为修改

- [ ] **Step 1: 将 {#if}/{:else if} 改为同时渲染 + CSS 隐藏**

在 `src/routes/invest/+page.svelte` 第 178-340 行，将 `{#if activeTab === 'dashboard'}/{:else if ...}/{/if}` 条件渲染结构改为每段内容用 `<div style:display={activeTab === 'xxx' ? 'contents' : 'none'}>` 包裹。

**操作规则：**
1. 删除第 178 行的 `{#if activeTab === 'dashboard'}`
2. 在原 dashboard 内容块前后分别加 `<div style:display={activeTab === 'dashboard' ? 'contents' : 'none'}>` 和 `</div>`
3. 将每个 `{:else if activeTab === 'xxx'}` 替换为上一个 tab 的 `</div>` + 新 tab 的 `<div style:display={activeTab === 'xxx' ? 'contents' : 'none'}>`
4. 删除最后的 `{/if}`
5. 每个 `{#if}` / `{/if}` 嵌套（如 committee 子 tab 的 `{#if}`）保持不变，只改最外层的 tab 切换结构

`display: contents` 让包裹 div 不影响子元素的 grid/flex 布局，组件实例保持挂载，局部 `$state` 不丢失。

- [ ] **Step 2: npm run check + commit**

Run: `npm run check`
Expected: 无 error

```bash
git add src/routes/invest/+page.svelte
git commit -m "feat(invest): preserve tab state by keeping all tabs mounted with CSS display"
```

---

### Task 6: Chat 输入框自动 stash/restore

**Root Cause:** 切换 runId 或离开 `/chat` 路由时，`PromptInput` 的 `inputText` 局部 `$state` 丢失。项目已有手动 stash 机制（keybinding `chat:stashPrompt`），但需要自动触发。

**方案：** 在 `$effect` 检测到 runId 变化时，自动 stash 当前输入；返回时自动 restore。

**Files:**
- Modify: `src/routes/chat/+page.svelte:1487-1525` (runId $effect)
- Modify: `src/routes/chat/+page.svelte:508` (stashedInput)

**Interfaces:**
- 复用已有的 `PromptInputSnapshot` 类型和 `restoreSnapshot` 方法

- [ ] **Step 1: runId 切换时自动 stash**

在 `src/routes/chat/+page.svelte` 第 1487-1525 行的 `$effect` 中，在 `loadRunProgressive` 调用前添加自动 stash：

```typescript
$effect(() => {
  if (!middlewareReady) return;
  const id = runId;
  const hasResume = hasResumeParam;
  untrack(() => {
    middleware.subscribeCurrent(id, store);

    if (store.resumeInFlight || resuming) {
      dbg("effect", "skip loadRun — resume in progress");
      return;
    }
    if (hasResume) return;

    if (!id) {
      // 切换到空 run 时，stash 当前输入
      if (promptRef && store.run) {
        const snap = promptRef.getInputSnapshot();
        if (snap?.text?.trim()) {
          stashedInput = snap;
        }
      }
      store.loadRun("", xtermRef);
      cancelProgressive();
      return;
    }

    if (store.run?.id === id && store.sessionAlive) {
      // 同一个 run，不 stash
      const scrollTo = $page.url.searchParams.get("scrollTo");
      if (scrollTo) {
        const clean = new URL($page.url);
        clean.searchParams.delete("scrollTo");
        replaceState(clean, {});
        tick().then(() => scrollToMessage(scrollTo));
      }
      return;
    }

    // 切换到不同 run 时，stash 当前输入
    if (promptRef && store.run && store.run.id !== id) {
      const snap = promptRef.getInputSnapshot();
      if (snap?.text?.trim()) {
        stashedInput = snap;
      }
    }

    loadRunProgressive(id, xtermRef);
  });
});
```

- [ ] **Step 2: 确认 PromptInput 已有 getInputSnapshot 方法**

`src/lib/components/PromptInput.svelte:1757` 已 export `getInputSnapshot(): PromptInputSnapshot`，返回 `{ text, attachments, pastedBlocks, pathRefs }`。无需新增代码，Step 1 中调用 `promptRef.getInputSnapshot()` 即可。

- [ ] **Step 3: 加载 run 后自动 restore stash**

在 runId effect 的 `loadRunProgressive(id, xtermRef)` 调用后添加自动恢复：

```typescript
loadRunProgressive(id, xtermRef).then(() => {
  // 自动恢复之前 stash 的输入
  if (stashedInput) {
    promptRef?.restoreSnapshot(stashedInput);
    stashedInput = null;
  }
});
```

不限制 `store.phase`——只要之前有 stash 的输入就恢复。用户切换到新 run 时如果之前打了草稿，输入框自动填回。

- [ ] **Step 4: npm run check + commit**

Run: `npm run check`
Expected: 无 error

```bash
git add src/routes/chat/+page.svelte src/lib/components/PromptInput.svelte
git commit -m "feat(chat): auto-stash/restore prompt input on runId switch"
```

---

### Task 7: 全量构建验证

- [ ] **Step 1: 全量构建**

Run: `npm run build`
Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 两者均通过

- [ ] **Step 2: Commit（如有意外修复）**

```bash
git add -A
git commit -m "chore: verify all changes build clean"
```
