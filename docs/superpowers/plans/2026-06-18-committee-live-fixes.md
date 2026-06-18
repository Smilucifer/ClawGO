# 委员会直播修复 (Part A) 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复委员会直播页面的 6 项问题:切页面丢数据、操作交互冗余、卡片排版缺失、解析误报、输出过长、关键信息未展示。

**Architecture:** 前端 store 拥有持久化进度的 schema,后端 `queue.rs` 作哑存储透传;解析层做 prompt↔parser↔前端三方字段对账;展示层弱化误报、对齐 demo 卡片视觉。

**Tech Stack:** SvelteKit (Svelte 5 runes), Rust (Tauri), serde_json。

**Spec:** `docs/superpowers/specs/2026-06-18-committee-live-and-dashboard-design.md` (Part A)

## Global Constraints

- 字段枚举值(signal / verdict 等)保持英文原样显示,与 `docs/demo-committee-live.html` 对齐。
- 前端用 Svelte 5 runes(`$state` / `$derived` / `$effect` / `$props`)。
- 新增 UI 文案必须同步 `messages/en.json` 与 `messages/zh-CN.json`。
- Rust 验证用 `cargo check --manifest-path src-tauri/Cargo.toml`(本机单测因 VCRUNTIME 问题无法运行二进制,但 parser 纯逻辑测试可在 CI/其它环境跑;本机以 `cargo check` 验证编译,单测代码仍须写)。
- 前端验证:`npm run check` + `npm run build`。
- Conventional Commit 风格(`feat:` / `fix:` / `refactor:`)。
- 每个 task 完成后:simplify 审查 → 修复 → commit → 验证。

---

## File Structure

| 文件 | 责任 | 改动 |
|------|------|------|
| `src-tauri/src/invest/committee/queue.rs` | 队列持久化(哑存储) | `QueueItem` 加 `progress` 透传字段 |
| `src/lib/stores/invest-committee-store.svelte.ts` | 队列调度 + 事件处理 + 持久化 | progress 序列化/恢复 |
| `src/lib/components/invest/CommitteeLiveTab.svelte` | 直播页 UI | 操作栏精简、卡片按钮状态机、排版、chip、弱化误报 |
| `src/lib/components/invest/pipeline-config.ts` | step 定义 + getStepState | (按需) |
| `src-tauri/src/invest/committee/parser.rs` | LLM 输出解析 | 字段对账、新增提取 + 测试 |
| `src-tauri/src/invest/committee/roles.rs` | prompt 模板 | 精简输出约束 |
| `messages/en.json` / `messages/zh-CN.json` | i18n | 新增/调整文案 |

实现顺序:Task 1(持久化后端)→ Task 2(持久化前端)→ Task 3(操作栏+按钮状态机)→ Task 4(parser 对账)→ Task 5(prompt 精简)→ Task 6(排版修复)→ Task 7(关键 chip + 视觉)。Task 4 为 Task 7 提供字段,须先行。

---

### Task 1: queue.rs 增加 progress 透传字段

**Files:**
- Modify: `src-tauri/src/invest/committee/queue.rs:22-29`(QueueItem 结构)
- Test: 同文件 `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `QueueItem.progress: Option<serde_json::Value>` — 前端写入的进度快照,后端不解析。

- [ ] **Step 1: 修改 QueueItem 结构,增加 progress 字段**

将 `src-tauri/src/invest/committee/queue.rs:22-29` 的 `QueueItem` 改为:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub symbol: String,
    pub status: QueueItemStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Opaque per-symbol progress snapshot owned by the frontend store.
    /// Backend persists it verbatim and never parses its shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<serde_json::Value>,
}
```

- [ ] **Step 2: 更新现有 roundtrip 测试,补 progress 字段断言**

修改 `queue_state_roundtrips_through_json` 测试中第一个 `QueueItem`(`src-tauri/src/invest/committee/queue.rs:137-141`),加上 progress,并补断言。把该 QueueItem 改为:

```rust
QueueItem {
    symbol: "600519".into(),
    status: QueueItemStatus::Done,
    error: None,
    progress: Some(serde_json::json!({ "completedSteps": 7, "done": true })),
},
```

第二个 QueueItem(`000001`)加 `progress: None,`。在该测试末尾(`src-tauri/src/invest/committee/queue.rs:170` 后)补:

```rust
        assert_eq!(
            back.items[0].progress.as_ref().unwrap()["completedSteps"],
            serde_json::json!(7)
        );
        assert!(back.items[1].progress.is_none());
```

- [ ] **Step 3: 验证编译 + 测试编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过,无 warning。

- [ ] **Step 4: simplify 审查 + commit**

```bash
git add src-tauri/src/invest/committee/queue.rs
git commit -m "feat(invest): add opaque progress field to committee queue item"
```

---

### Task 2: 前端 store 持久化与恢复完整进度

**Files:**
- Modify: `src/lib/stores/invest-committee-store.svelte.ts`(类型 155-160、`loadQueue` 270-290、`_flushQueue` 297-309、事件处理 424-553)

**Interfaces:**
- Consumes: `QueueItem.progress`(Task 1)。
- Produces: `PersistedProgress` 接口;`loadQueue()` 恢复带内容进度;`_flushQueue()` 写入 progress。

- [ ] **Step 1: 定义 PersistedProgress 类型并扩展 QueueItem / CommitteeQueueState**

在 `src/lib/stores/invest-committee-store.svelte.ts` 的 `QueueItem` 接口(134-138)后,新增:

```ts
/** Serializable subset of SymbolProgress persisted to disk for cross-restart recovery. */
export interface PersistedProgress {
  completedSteps: number;
  completedRounds: RoundOutputSummary[];
  done: boolean;
  error: string | null;
  result: CommitteeResult | null;
  regimeData: RegimeStepData | null;
  failedSteps: number[]; // Set serialized as array
}
```

并在 `QueueItem` 接口加字段:

```ts
export interface QueueItem {
  symbol: string;
  status: QueueItemStatus;
  error?: string;
  progress?: PersistedProgress | null;
}
```

- [ ] **Step 2: 新增进度 ↔ 持久化互转辅助函数**

在 `InvestCommitteeStore` 类的 `_freshProgress`(312-324)方法后,新增两个私有方法:

```ts
  /** Convert in-memory SymbolProgress → serializable PersistedProgress. */
  private _toPersisted(p: SymbolProgress): PersistedProgress {
    return {
      completedSteps: p.completedSteps,
      completedRounds: p.completedRounds,
      done: p.done,
      error: p.error,
      result: p.result,
      regimeData: p.regimeData,
      failedSteps: p.failedSteps ? Array.from(p.failedSteps) : [],
    };
  }

  /** Rebuild SymbolProgress from persisted snapshot, restoring transient fields. */
  private _fromPersisted(pp: PersistedProgress, status: QueueItemStatus): SymbolProgress {
    return {
      activeStep: -1,
      completedSteps: pp.completedSteps,
      completedRounds: pp.completedRounds ?? [],
      done: pp.done,
      error: pp.error,
      result: pp.result,
      regimeData: pp.regimeData,
      failedSteps: new Set(pp.failedSteps ?? []),
      status,
    };
  }
```

- [ ] **Step 3: `_flushQueue` 写入 progress**

将 `_flushQueue`(297-309)的 `items` 映射改为带 progress:

```ts
  private async _flushQueue() {
    const state: CommitteeQueueState = {
      items: this.queue.map((q) => {
        const p = this.perSymbolProgress.get(q.symbol);
        return {
          symbol: q.symbol,
          status: q.status,
          error: q.error,
          progress: p ? this._toPersisted(p) : null,
        };
      }),
      snapshot: this.portfolioSnapshot,
      maxConcurrent: this.maxConcurrent,
      updatedAt: new Date().toISOString(),
    };
    try {
      await invoke('save_committee_queue', { state });
    } catch (e) {
      console.error('save_committee_queue failed:', e);
    }
  }
```

- [ ] **Step 4: `loadQueue` 恢复带内容进度**

将 `loadQueue`(270-290)的进度重建段改为优先用 persisted:

```ts
  async loadQueue() {
    try {
      const state = await invoke<CommitteeQueueState>('load_committee_queue');
      this.maxConcurrent = state.maxConcurrent && state.maxConcurrent > 0 ? state.maxConcurrent : 5;
      this.portfolioSnapshot = state.snapshot ?? null;
      // Restore queue for display; running items (interrupted by restart) → queued.
      this.queue = (state.items ?? []).map((it) => ({
        symbol: it.symbol,
        status: it.status === 'running' ? ('queued' as QueueItemStatus) : it.status,
        error: it.error,
      }));
      const progress = new Map<string, SymbolProgress>();
      const restoredResults: CommitteeResult[] = [];
      for (const item of state.items ?? []) {
        const status: QueueItemStatus = item.status === 'running' ? 'queued' : item.status;
        if (item.progress) {
          const sp = this._fromPersisted(item.progress, status);
          progress.set(item.symbol, sp);
          if (sp.result) restoredResults.push(sp.result);
        } else {
          progress.set(item.symbol, this._freshProgress(status));
        }
      }
      this.perSymbolProgress = progress;
      this.results = restoredResults;
      this._recomputeRunning();
    } catch (e) {
      console.error('load_committee_queue failed:', e);
    }
  }
```

- [ ] **Step 5: 验证类型 + 构建**

Run: `npm run check`
Expected: 无类型错误。

Run: `npm run build`
Expected: 构建成功。

- [ ] **Step 6: 手动验证恢复行为**

启动 `npm run tauri dev`,跑一个标的至完成 → 切到其它 tab → 切回委员会直播。
Expected: 卡片内容(各角色输出 + verdict)仍在,不只剩重试按钮。

- [ ] **Step 7: simplify 审查 + commit**

```bash
git add src/lib/stores/invest-committee-store.svelte.ts
git commit -m "feat(invest): persist and restore full committee progress across restart"
```

---

### Task 3: 操作栏精简 + 卡片独立运行/中止按钮

**Files:**
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`(脚本 18-149、操作栏 216-260、卡片头 304-349)
- Modify: `messages/en.json` / `messages/zh-CN.json`(若新增 key)

**Interfaces:**
- Consumes: `store.addToQueue`、`store.abortSymbol`、`store.abortAll`、`store.setMaxConcurrent`(均已存在)。

- [ ] **Step 1: 移除多选相关状态与函数**

在 `src/lib/components/invest/CommitteeLiveTab.svelte` 脚本块:
- 删除 `selectedSymbols` 状态声明(19 行)。
- 删除 `runSelected`(111-117)、`toggleSel`(125-130)、`toggleAll`(132-134)三个函数。
- 保留 `runAll`(119-123)、`toggleExpand`、`onConcurrencyChange`、`buildSnapshot`。
- 新增单标的运行函数(放在 `runAll` 后):

```ts
  function runSymbol(sym: string) {
    expandedSymbols.add(sym);
    expandedSymbols = new Set(expandedSymbols);
    store.addToQueue([sym], buildSnapshot());
  }
```

- [ ] **Step 2: 操作栏删除「运行选中」「全选」**

将操作栏(216-240)改为(删除运行选中按钮和全选复选框,保留全部运行/中止全部/include watch):

```svelte
  <div class="action-bar">
    <button class="btn primary" disabled={allAssets.length === 0} onclick={runAll}>
      ⏵ {t('invest_committee_add_all')}
    </button>
    {#if store.runningCount > 0}
      <button class="btn danger" onclick={() => store.abortAll()}>
        ⏹ {t('invest_committee_abort_all')}
      </button>
    {/if}
    <div class="action-sep"></div>
    <label class="checkbox-row">
      <input type="checkbox" bind:checked={includeWatch} />
      {t('invest_committee_include_watch')}
    </label>
    <div class="spacer"></div>
    <label class="conc-row">
      {t('invest_committee_concurrency')}
      <select value={store.maxConcurrent} onchange={onConcurrencyChange}>
        {#each CONCURRENCY_OPTIONS as n}
          <option value={n}>{n}</option>
        {/each}
      </select>
    </label>
    {#if store.runningCount > 0 || store.queuedCount > 0}
      <span class="progress-text">
        <span class="dot"></span>
        {t('invest_committee_in_progress', {
          current: String(store.doneCount),
          total: String(store.queue.length),
          running: String(store.runningCount),
        })}
      </span>
    {/if}
  </div>
```

- [ ] **Step 3: 卡片头删除复选框,改为运行/中止切换按钮**

将卡片头(304-349)改为(移除 checkbox,按 queue 状态显示单按钮):

```svelte
      <div class="card-header" onclick={() => toggleExpand(asset.symbol)}>
        <div class="card-id">
          <span class="card-name">{asset.name ?? asset.symbol}</span>
          <span class="card-ticker">{asset.symbol}</span>
        </div>
        <span class="badge {asset.kind}">{asset.kind === 'hold' ? 'HOLD' : 'WATCH'}</span>
        {@render pipelineBar(p)}
        {#if result}
          <span class="verdict-badge-sm" style={getVerdictBadgeStyle(result.finalVerdict)}>
            {result.finalVerdict}
          </span>
        {/if}
        {#if queueItem?.status === 'running'}
          <button
            class="abort-btn"
            onclick={(e) => { e.stopPropagation(); store.abortSymbol(asset.symbol); }}
            title={t('invest_committee_abort')}
          >
            ⏹
          </button>
        {:else}
          <button
            class="run-btn"
            onclick={(e) => { e.stopPropagation(); runSymbol(asset.symbol); }}
            title={queueItem && queueItem.status !== 'queued' ? t('invest_retry') : t('invest_committee_run')}
          >
            ▶
          </button>
        {/if}
        <span class="expand-arrow" class:open={isExpanded}>▶</span>
      </div>
```

- [ ] **Step 4: 删除 card-checkbox 样式,新增 run-btn 样式**

在 `<style>` 块删除 `.card-checkbox`(520 行)样式;在 `.retry-btn` 样式(557-558)旁新增:

```css
  .run-btn {
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--accent-muted);
    background: var(--bg-input);
    color: var(--accent);
    font-size: 11px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .run-btn:hover { background: var(--accent-muted); }
```

(保留现有 `.abort-btn`、`.retry-btn` 样式;`.retry-btn` 仍被展开区其它处引用则保留,否则一并清理。)

- [ ] **Step 5: 新增 i18n key**

在 `messages/en.json` 和 `messages/zh-CN.json` 加入 `invest_committee_run`:
- en: `"invest_committee_run": "Run"`
- zh-CN: `"invest_committee_run": "运行"`

- [ ] **Step 6: 验证 i18n + 类型 + 构建**

Run: `npm run i18n:check`
Expected: 通过,无缺失 key。

Run: `npm run check && npm run build`
Expected: 通过。

- [ ] **Step 7: simplify 审查 + commit**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte messages/en.json messages/zh-CN.json
git commit -m "feat(invest): replace multi-select with per-card run/abort button in committee live"
```

---

### Task 4: parser 字段对账 + 补全提取

**Files:**
- Modify: `src-tauri/src/invest/committee/parser.rs`(ParsedFields 结构、parse_cio、新增测试)

**Interfaces:**
- Produces: `ParsedFields.execution_mode: Option<String>`、`ParsedFields.first_tranche_cny: Option<f64>`(CIO prompt 输出"执行模式/首笔金额",此前未提取)。
- Produces: `ParsedFields.signal_reason`、`market_phase_reason`(Macro prompt 输出但未提取)。

- [ ] **Step 1: 写失败测试 — CIO 执行模式/首笔金额提取**

在 `src-tauri/src/invest/committee/parser.rs` 测试模块末尾(`test_quant_r1_buy_point_not_overridden_by_r2` 后,1210 行附近)新增:

```rust
    #[test]
    fn test_parse_cio_execution_mode_and_first_tranche() {
        let text = "VERDICT: ACCUMULATE\nCONFIDENCE: 0.7\n执行模式: pyramid\n首笔金额: 30000";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.execution_mode.as_deref(), Some("pyramid"));
        assert_eq!(parsed.first_tranche_cny, Some(30000.0));
    }

    #[test]
    fn test_parse_macro_signal_reason() {
        let text = "SIGNAL: risk_on\n信号理由: 北向资金持续流入\n市场阶段: 主升\n市场阶段理由: 站上MA60";
        let parsed = parse_role_output(CommitteeRole::Macro, text, false);
        assert_eq!(parsed.signal_reason.as_deref(), Some("北向资金持续流入"));
        assert_eq!(parsed.market_phase_reason.as_deref(), Some("站上MA60"));
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml committee::parser::tests::test_parse_cio_execution_mode 2>&1 | head -20`
Expected: 编译失败(字段 `execution_mode` 不存在)。若本机因 VCRUNTIME 无法运行,以 `cargo check` 确认字段缺失导致的编译错误。

- [ ] **Step 3: ParsedFields 新增字段**

在 `src-tauri/src/invest/committee/parser.rs` 的 `ParsedFields` 结构,CIO-specific 段(`pub execution_plan` 后,82 行附近)新增:

```rust
    /// CIO: 执行模式 lump-sum | pyramid | grid | none
    pub execution_mode: Option<String>,
    /// CIO: 首笔金额
    pub first_tranche_cny: Option<f64>,
```

在 Macro-specific 段(`pub emotion_temperature` 后,48 行附近)新增:

```rust
    /// Macro: 信号理由(一句话)
    pub signal_reason: Option<String>,
    /// Macro: 市场阶段理由(一句话)
    pub market_phase_reason: Option<String>,
```

- [ ] **Step 4: parse_cio / parse_macro 补提取**

在 `parse_cio`(469-521)的 `parsed.execution_plan` 提取行(495)后新增:

```rust
    parsed.execution_mode = extract_field_any(text, &["EXECUTION_MODE", "执行模式"]);
    parsed.first_tranche_cny = extract_f64_any(text, &["FIRST_TRANCHE_CNY", "首笔金额"]);
```

在 `parse_macro`(394-417)的 `parsed.emotion_temperature` 提取(415-416)后新增:

```rust
    parsed.signal_reason = extract_field_any(text, &["SIGNAL_REASON", "信号理由"]);
    parsed.market_phase_reason = extract_field_any(text, &["MARKET_PHASE_REASON", "市场阶段理由"]);
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml committee::parser::tests 2>&1 | tail -20`
Expected: 全部通过(本机无法运行则 `cargo check` 通过 + 逻辑自检)。

- [ ] **Step 6: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。

- [ ] **Step 7: simplify 审查 + commit**

```bash
git add src-tauri/src/invest/committee/parser.rs
git commit -m "feat(invest): reconcile parser fields with prompts (cio exec mode, macro reasons)"
```

---

### Task 5: prompt 精简输出约束

**Files:**
- Modify: `src-tauri/src/invest/committee/roles.rs`(6 个 prompt 常量 244-549+)

**Interfaces:** 无新接口,仅改 prompt 文本常量。

- [ ] **Step 1: 在 MACRO_PROMPT 输出要求段加精简约束**

在 `src-tauri/src/invest/committee/roles.rs` 的 `MACRO_PROMPT`,"输出要求"列表(277-283)末尾(`- 市场阶段是全局信号...` 后)追加一行:

```
- 每个字段值必须一句话结束,不分点、不换行续写;理由类字段每条≤一句话
```

- [ ] **Step 2: 对 QUANT_PROMPT / QUANT_R2_PROMPT / RISK_PROMPT / RISK_R2_PROMPT 加同款约束**

在以下每个常量的"输出要求"段末尾追加同一行(措辞一致):
- `QUANT_PROMPT` 输出要求段(351-358)
- `QUANT_R2_PROMPT` 输出要求段(405-411)
- `RISK_PROMPT` 输出要求段(457-461)
- `RISK_R2_PROMPT` 输出要求段(497-503)

追加文本(对结构化列表字段保留原条数约束,不冲突):

```
- 每个字段值必须一句话结束,不分点、不换行续写;reasoning/一句话等理由字段每条≤一句话。结构化列表(如关键数据)保持既定条数
```

- [ ] **Step 3: CIO_PROMPT 加精简约束**

`CIO_PROMPT` 的输出格式段已含字段列表(520-537),在 `**Hard Rules**` 段(546)前插入一行约束:

```
**输出精简要求**:每个字段值一句话结束,个人备注/推理类≤一句话,不分点展开。

```

- [ ] **Step 4: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过(仅字符串常量改动)。

- [ ] **Step 5: 验证现有 prompt 测试仍通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml committee::roles::tests 2>&1 | tail -10`
Expected: `test_load_prompt_for_round_*` 系列通过(占位符替换不受影响;本机无法运行则 `cargo check`)。

- [ ] **Step 6: simplify 审查 + commit**

```bash
git add src-tauri/src/invest/committee/roles.rs
git commit -m "feat(invest): enforce concise one-sentence output in committee prompts"
```

---

### Task 6: 卡片排版修复

**Files:**
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`(stepCard snippet 渲染 204-206、`.step-body` 样式 627-630)

**Interfaces:** 无新接口。

- [ ] **Step 1: step-body 样式补 white-space**

将 `src/lib/components/invest/CommitteeLiveTab.svelte` 的 `.step-body` 样式(627-630)改为保留换行:

```css
  .step-body {
    padding: 14px; font-size: 12.5px; color: var(--text-secondary); line-height: 1.85;
    max-height: 320px; overflow-y: auto; word-break: break-word;
    white-space: pre-wrap;
  }
```

注:`renderMarkdown` 输出已是 HTML,`pre-wrap` 主要兜底裸文本换行。若 `renderMarkdown` 把 `\n` 吃掉,改在 Step 2 处理。

- [ ] **Step 2: 确认 renderMarkdown 换行行为,必要时预处理**

读 `src/lib/utils/markdown.ts`,确认 `renderMarkdown` 是否保留 LLM 多行 `KEY: 值` 的换行。
- 若它走完整 markdown 解析(单换行被折叠)→ 在传入前把 rawText 的单换行转双换行(段落),即把 stepCard(204-206)的渲染改为:

```svelte
      {:else if round?.parsed?.rawText}
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        {@html renderMarkdown(round.parsed.rawText.replace(/\n(?!\n)/g, '\n\n'))}
```

- 若 `renderMarkdown` 已保留换行,则只保留 Step 1 的 `pre-wrap`,本步无需改。实现时按实际行为二选一,并在 commit message 注明。

- [ ] **Step 3: 验证构建 + 目测**

Run: `npm run check && npm run build`
Expected: 通过。

`npm run tauri dev` 跑一个标的,展开卡片。
Expected: 各角色文本分行清晰,不再挤成一坨。

- [ ] **Step 4: simplify 审查 + commit**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte
git commit -m "fix(invest): preserve line breaks in committee step card body"
```

---

### Task 7: 关键 chip + demo 视觉对齐

**Files:**
- Modify: `src/lib/stores/invest-committee-store.svelte.ts`(`RoundOutputSummary.parsed` 接口 29-39)
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`(stepCard snippet、弱化误报、chip 渲染 + 样式)
- Modify: `messages/en.json` / `messages/zh-CN.json`(chip 标签若需要)

**Interfaces:**
- Consumes: 后端已序列化的 `ParsedFields` 全字段(含 Task 4 新增)。

- [ ] **Step 1: 扩展前端 parsed TS 接口**

将 `src/lib/stores/invest-committee-store.svelte.ts` 的 `RoundOutputSummary.parsed`(29-39)扩展为声明 demo chip 所需字段:

```ts
  parsed: {
    rawText: string;
    signal?: string;
    strength?: number;
    verdict?: string;
    confidence?: number;
    oneLiner?: string;
    reasoning?: string;
    truncated?: boolean;
    fallbackReason?: string;
    // Macro
    marketPhase?: string;
    emotionTemperature?: string;
    // Quant
    buyPointAssessment?: string;
    valuationAssessment?: string;
    moneyFlow?: string;
    // Risk
    concentrationPct?: number;
    dryPowderCny?: number;
    pnlPct?: number;
    stockRiskSummary?: string;
    // CIO
    catalystTier?: string;
    catalystSummary?: string;
  };
```

- [ ] **Step 2: 弱化解析误报 — 区分真空 fallback 与字段缺失**

将 `src/lib/components/invest/CommitteeLiveTab.svelte` 的 `stepCard` snippet 中 fallback 分支(187-190)改为只对真空 fallback 显示警告条:

```svelte
      {:else if round?.parsed?.fallbackReason && isHardFallback(round.parsed.fallbackReason)}
        <div class="fallback-message">
          <span class="fallback-icon">⚠</span><span>{round.parsed.fallbackReason}</span>
        </div>
```

在脚本块新增判定函数(放在 `segIcon` 后):

```ts
  // Hard fallbacks = truly no content; soft (missing_critical_fields) still has rawText.
  const HARD_FALLBACKS = new Set(['worker_unavailable', 'empty_text', 'cli_executor_none']);
  function isHardFallback(reason: string): boolean {
    return HARD_FALLBACKS.has(reason) || reason.startsWith('cli_error');
  }
```

- [ ] **Step 3: stepCard 顶部渲染关键 chip**

在 `stepCard` 的 `.step-body` 内,active/aborted/hardFallback/regime 分支之后、rawText 渲染之前,插入按角色 chip 行。在 `{:else if round?.parsed?.rawText}` 分支(204）改为先渲染 chip 再渲染正文:

```svelte
      {:else if round?.parsed?.rawText}
        {@const pf = round.parsed}
        <div class="chip-row">
          {#if pf.signal}<span class="chip sig-{pf.signal.toLowerCase()}">{pf.signal}</span>{/if}
          {#if pf.strength != null}<span class="chip neutral">强度 {pf.strength}</span>{/if}
          {#if pf.verdict}<span class="chip sig-{pf.verdict.toLowerCase()}">{pf.verdict}</span>{/if}
          {#if pf.confidence != null}<span class="chip neutral">{(pf.confidence * 100).toFixed(0)}%</span>{/if}
          {#if pf.marketPhase}<span class="chip neutral">{pf.marketPhase}</span>{/if}
          {#if pf.emotionTemperature}<span class="chip neutral">{pf.emotionTemperature}</span>{/if}
          {#if pf.buyPointAssessment}<span class="chip neutral">{pf.buyPointAssessment}</span>{/if}
          {#if pf.valuationAssessment}<span class="chip neutral">{pf.valuationAssessment}</span>{/if}
          {#if pf.concentrationPct != null}<span class="chip neutral">集中度 {pf.concentrationPct}%</span>{/if}
          {#if pf.catalystTier}<span class="chip neutral">{pf.catalystTier}</span>{/if}
          {#if pf.fallbackReason}<span class="chip warn" title={pf.fallbackReason}>⚠ 部分字段未识别</span>{/if}
        </div>
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        {@html renderMarkdown(pf.rawText)}
```

(若 Task 6 Step 2 选了换行预处理,这里 `renderMarkdown(pf.rawText)` 同步加 `.replace(...)`。)

- [ ] **Step 4: 新增 chip 样式(对齐 demo 配色)**

在 `<style>` 块(regime 样式附近)新增,配色取自 `docs/demo-committee-live.html` 的 `.signal-chip` 系列:

```css
  .chip-row { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 10px; padding-bottom: 8px; border-bottom: 1px dashed var(--border); }
  .chip { font-size: 11px; font-weight: 700; padding: 2px 10px; border-radius: var(--radius-sm); text-transform: uppercase; }
  .chip.neutral { background: var(--bg-input); color: var(--text-secondary); font-weight: 600; text-transform: none; }
  .chip.warn { background: rgba(255,193,7,0.12); color: var(--color-warning); font-weight: 600; text-transform: none; }
  .chip.sig-risk_on, .chip.sig-buy, .chip.sig-bullish { background: rgba(138,154,118,0.15); color: var(--color-success); }
  .chip.sig-accumulate { background: rgba(59,130,246,0.15); color: var(--color-quant, #3b82f6); }
  .chip.sig-hold, .chip.sig-neutral { background: rgba(196,169,110,0.15); color: var(--accent); }
  .chip.sig-trim { background: rgba(255,193,7,0.15); color: var(--color-warning); }
  .chip.sig-risk_off, .chip.sig-sell, .chip.sig-bearish, .chip.sig-high_risk { background: rgba(168,122,122,0.2); color: var(--color-error); }
```

- [ ] **Step 5: 验证类型 + 构建 + 目测**

Run: `npm run check && npm run build`
Expected: 通过。

`npm run tauri dev` 跑一个标的,展开各角色卡片。
Expected: 卡片顶部显示 signal/verdict/集中度等 chip;`missing_critical_fields` 不再用大黄条盖内容,只附小 chip。

- [ ] **Step 6: simplify 审查 + commit**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte src/lib/stores/invest-committee-store.svelte.ts messages/en.json messages/zh-CN.json
git commit -m "feat(invest): add key-field chips and soften parse-error display in committee live"
```

---

## Self-Review

**Spec 覆盖检查(Part A)**:
- A1 跨重启恢复 → Task 1 + Task 2 ✓
- A2 操作栏 + 卡片按钮 → Task 3 ✓
- A3 解析弱化 + 增强 → Task 4(增强)+ Task 7 Step 2(弱化)✓
- A4 prompt 精简 → Task 5 ✓
- A5 排版修复 → Task 6 ✓
- A6 关键 chip + 视觉 → Task 7 ✓

**依赖顺序**:Task 4 先于 Task 7(chip 用到新字段)✓;Task 1 先于 Task 2(progress 字段)✓。

**类型一致性**:`PersistedProgress`(Task 2)字段与 `_toPersisted`/`_fromPersisted` 一致;`progress` 字段名前后端一致(Task 1 Rust `progress` + camelCase,Task 2 TS `progress`)✓。
