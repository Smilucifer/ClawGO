# 盘前观察报告前端改造 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 落地盘前观察报告设计文档的前端 5 项(入口迁移 / 导出 PNG·PDF 修复 / 生成状态提 store / 显示生成耗时 / 01 段加宽横排 + 画布放宽)。

**Architecture:** 后端加一个 `write_binary_export` 命令(base64 解码写盘,白名单 .png/.pdf);新建 `premarket-store.svelte.ts` 模块级单例持有生成状态(切 tab 不丢)；`PremarketReportTab.svelte` 改读 store、导出走 Tauri `save()` 对话框、工具栏显示秒表/耗时；`+page.svelte` 把 PremarketReportTab 从 system→reports 移到 committee→premarket。

**Tech Stack:** Svelte 5 runes(`$state`/`$derived`/`$effect`)、Tauri 2(`@tauri-apps/plugin-dialog` 的 `save()`）、Rust(base64 0.22、tokio::fs）、vitest、svelte-check。

## Global Constraints

- 文件编辑单次 ≤50 行；超出分多次 Write/Edit。
- i18n 文案键放 `messages/en.json` + `messages/zh-CN.json` 两份,键名 `invest_*`,两边键集必须一致。
- Tauri 2 默认把 Rust 命令的 snake_case 参数转 camelCase 暴露给 JS;本计划 `write_binary_export` 的 base64 参数命名为单词 `base64` 规避大小写歧义。
- 后端命令必须在 `src-tauri/src/lib.rs` 的 `tauri::generate_handler!` 列表注册,否则前端 invoke 报 "command not found"。
- 不改后端报告生成逻辑、不改 SABC 打分;仅前端展示 + 一个导出写盘命令。
- 后端验证用 `cargo build`(非仅 check —— check 漏单态化 Send 类错误);前端验证 `npm run check` + `npm run test` + `npm run build`。
- §11 机器约束:本机 Rust 测试二进制可能以 `0xc0000139 STATUS_ENTRYPOINT_NOT_FOUND` 启动失败(环境问题,非代码);若如此,以 `cargo build` 0 error + 测试编译通过为准。

## File Structure

- `src-tauri/src/commands/export.rs`(改):加 `write_binary_export` 命令 + `binary_export_ext_ok` 纯校验函数 + 单测。
- `src-tauri/src/lib.rs`(改):注册 `write_binary_export`。
- `src/lib/stores/premarket-store.svelte.ts`(建):模块级单例,持 `generating/startedAt/elapsedMs/lastError/completionSeq`,含 `generate()` 生命周期 + 秒表 tick。
- `src/lib/stores/premarket-store.test.ts`(建):秒表/状态纯逻辑单测(注入 `now`,不依赖真实计时器)。
- `src/lib/components/invest/PremarketReportTab.svelte`(改):读 store、导出走 `save()`+新命令、工具栏显示耗时;`<style>` 画布 720px→变量放宽 + theme-wall 4 列。
- `src/routes/invest/+page.svelte`(改):PremarketReportTab 入口 system/reports → committee/premarket。
- `messages/en.json` + `messages/zh-CN.json`(改):加 `invest_committee_sub_premarket`、`invest_premarket_elapsed`、`invest_premarket_elapsed_running`。

## Task 依赖

Task 1(后端命令)与 Task 2(store)相互独立,可并行。Task 3(组件接线)依赖 1+2。Task 4(入口迁移)独立于 3,但与 Task 3 不碰同一文件。Task 5(CSS)改 PremarketReportTab 的 `<style>`,须排在 Task 3 之后(同文件,避免冲突)。执行顺序:1 → 2 → 3 → 4 → 5。

---

### Task 1: 后端 `write_binary_export` 命令

**Files:**
- Modify: `src-tauri/src/commands/export.rs`(在文件末尾追加)
- Modify: `src-tauri/src/lib.rs`(handler 列表,现 `commands::export::write_html_export` 在第 321 行附近)
- Test: 同 `export.rs` 内 `#[cfg(test)]` 模块

**Interfaces:**
- Produces: `#[tauri::command] pub async fn write_binary_export(path: String, base64: String) -> Result<(), String>` —— 前端以 `invoke('write_binary_export', { path, base64 })` 调用(Task 3 消费)。
- Produces: `fn binary_export_ext_ok(path: &str) -> bool` —— 纯扩展名白名单校验(仅供单测 + 命令内部)。

- [ ] **Step 1: 写失败测试**(在 `export.rs` 末尾追加)

```rust
#[cfg(test)]
mod tests {
    use super::binary_export_ext_ok;

    #[test]
    fn accepts_png_and_pdf_case_insensitive() {
        assert!(binary_export_ext_ok("C:/tmp/a.png"));
        assert!(binary_export_ext_ok("/tmp/a.PDF"));
        assert!(binary_export_ext_ok("report.Png"));
    }

    #[test]
    fn rejects_other_extensions() {
        assert!(!binary_export_ext_ok("a.exe"));
        assert!(!binary_export_ext_ok("a.html"));
        assert!(!binary_export_ext_ok("noext"));
    }
}
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd src-tauri && cargo test binary_export_ext_ok 2>&1 | tail -15`
Expected: 编译失败 `cannot find function binary_export_ext_ok`。

- [ ] **Step 3: 实现校验函数 + 命令**(在 `export.rs` 的 `#[cfg(test)]` 之前追加)

```rust
/// 导出写盘白名单:仅 .png / .pdf(大小写不敏感)。
fn binary_export_ext_ok(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .is_some_and(|e| e == "png" || e == "pdf")
}

/// 把 base64 编码的二进制(PNG/PDF)解码后写入用户选定路径。
/// 仿 `write_html_export`,但白名单为 .png/.pdf,内容走 base64 解码。
#[tauri::command]
pub async fn write_binary_export(path: String, base64: String) -> Result<(), String> {
    use base64::Engine;
    log::debug!(
        "[export] write_binary_export: path={}, b64_len={}",
        path,
        base64.len()
    );
    if !binary_export_ext_ok(&path) {
        log::error!("[export] write_binary_export rejected path: {}", path);
        return Err("write_binary_export: only .png/.pdf paths allowed".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64.as_bytes())
        .map_err(|e| {
            log::error!("[export] write_binary_export base64 decode failed: {}", e);
            e.to_string()
        })?;
    tokio::fs::write(&path, bytes).await.map_err(|e| {
        log::error!("[export] write_binary_export write failed: {}", e);
        e.to_string()
    })
}
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd src-tauri && cargo test binary_export_ext_ok 2>&1 | tail -15`
Expected: 2 tests pass。（若 §11 启动失败,改跑 `cargo test --no-run binary_export_ext_ok` 确认编译通过。)

- [ ] **Step 5: 注册命令**

在 `src-tauri/src/lib.rs` 的 `tauri::generate_handler!` 列表中,`commands::export::write_html_export,` 行之后加一行:

```rust
            commands::export::write_binary_export,
```

- [ ] **Step 6: 全量构建**

Run: `cd src-tauri && cargo build 2>&1 | tail -3`
Expected: `Finished` 0 error。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/commands/export.rs src-tauri/src/lib.rs
git commit -m "feat(export): 加 write_binary_export 命令(base64→.png/.pdf 白名单写盘)"
```

---

### Task 2: `premarket-store` 生成状态单例

**Files:**
- Create: `src/lib/stores/premarket-store.svelte.ts`
- Test: `src/lib/stores/premarket-store.test.ts`

**Interfaces:**
- Consumes: `getTransport().invoke` —— 触发 `trigger_cron_job`/`generate_premarket_report_cmd`(与组件现逻辑一致)。
- Produces: `class PremarketStore`(可实例化供测试)+ `export const premarketStore = new PremarketStore()`(组件用单例)。
  - 字段:`generating: boolean`、`startedAt: number`、`elapsedMs: number`、`lastError: string | null`、`lastElapsedMs: number`、`completionSeq: number`。
  - 方法:`markStart(now: number): void`、`markFinish(err: string | null, now: number): void`、`elapsedSec(now: number): number`、`async generate(): Promise<void>`、`tick(now: number): void`。
  - `generate()` 在 store 内跑完整生命周期(切 tab 卸载组件也不中断);完成时 `completionSeq++`,组件用 `$effect` 观察其变化触发 `loadLatest()`。

- [ ] **Step 1: 写失败测试**

```typescript
import { describe, it, expect, vi } from 'vitest';
vi.mock('$lib/transport', () => ({ getTransport: () => ({ invoke: vi.fn() }) }));
import { PremarketStore } from './premarket-store.svelte';

describe('PremarketStore timer', () => {
  it('markStart sets generating + startedAt', () => {
    const s = new PremarketStore();
    s.markStart(1000);
    expect(s.generating).toBe(true);
    expect(s.startedAt).toBe(1000);
    expect(s.lastError).toBeNull();
  });

  it('markFinish records elapsed + clears generating + bumps seq', () => {
    const s = new PremarketStore();
    s.markStart(1000);
    const seq0 = s.completionSeq;
    s.markFinish(null, 4000);
    expect(s.generating).toBe(false);
    expect(s.lastElapsedMs).toBe(3000);
    expect(s.completionSeq).toBe(seq0 + 1);
  });

  it('elapsedSec computes whole seconds from startedAt', () => {
    const s = new PremarketStore();
    s.markStart(1000);
    expect(s.elapsedSec(5500)).toBe(4);
  });

  it('markFinish with error stores message, still bumps seq', () => {
    const s = new PremarketStore();
    s.markStart(0);
    s.markFinish('boom', 100);
    expect(s.lastError).toBe('boom');
    expect(s.generating).toBe(false);
  });
});
```

- [ ] **Step 2: 跑测试确认失败**

Run: `npm run test -- premarket-store 2>&1 | tail -20`
Expected: FAIL —— 找不到 `./premarket-store.svelte`。

- [ ] **Step 3: 实现 store**(建 `src/lib/stores/premarket-store.svelte.ts`)

```typescript
import { getTransport } from '$lib/transport';

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return getTransport().invoke<T>(cmd, args);
}

/**
 * 盘前报告生成状态单例。生命周期(generate/秒表)持在模块级,
 * 切 tab 卸载 PremarketReportTab 也不中断;重挂载时组件从此读回状态。
 */
export class PremarketStore {
  generating = $state<boolean>(false);
  startedAt = $state<number>(0);
  elapsedMs = $state<number>(0);
  lastElapsedMs = $state<number>(0);
  lastError = $state<string | null>(null);
  /** 每次生成完成自增;组件 $effect 观察它触发 loadLatest。 */
  completionSeq = $state<number>(0);

  markStart(now: number): void {
    this.generating = true;
    this.startedAt = now;
    this.elapsedMs = 0;
    this.lastError = null;
  }

  /** 秒表 tick:更新 elapsedMs(组件每秒调一次)。 */
  tick(now: number): void {
    if (this.generating) this.elapsedMs = now - this.startedAt;
  }

  markFinish(err: string | null, now: number): void {
    this.lastElapsedMs = now - this.startedAt;
    this.elapsedMs = this.lastElapsedMs;
    this.lastError = err;
    this.generating = false;
    this.completionSeq += 1;
  }

  elapsedSec(now: number): number {
    return Math.floor((now - this.startedAt) / 1000);
  }

  /** 完整生成生命周期:优先 cron dispatcher,失败回退 direct;结束 markFinish。 */
  async generate(): Promise<void> {
    if (this.generating) return;
    this.markStart(Date.now());
    try {
      try {
        await invoke<string>('trigger_cron_job', { id: 'premarket_report' });
      } catch (cronErr) {
        console.warn('[premarket] cron trigger failed, fallback:', cronErr);
        await invoke<string>('generate_premarket_report_cmd');
      }
      this.markFinish(null, Date.now());
    } catch (e) {
      console.error('[premarket] generate:', e);
      this.markFinish(String(e), Date.now());
    }
  }
}

export const premarketStore = new PremarketStore();
```

- [ ] **Step 4: 跑测试确认通过**

Run: `npm run test -- premarket-store 2>&1 | tail -20`
Expected: 4 tests pass。

- [ ] **Step 5: 提交**

```bash
git add src/lib/stores/premarket-store.svelte.ts src/lib/stores/premarket-store.test.ts
git commit -m "feat(premarket): 生成状态提到模块级单例 store(切 tab 不丢 + 秒表逻辑)"
```

---

### Task 3: 组件接线——读 store + 显示耗时 + 导出走 Tauri save()

**Files:**
- Modify: `src/lib/components/invest/PremarketReportTab.svelte`(`<script>` 段)

**Interfaces:**
- Consumes: Task 2 的 `premarketStore`(生成状态 + `generate()` + `completionSeq`);Task 1 的 `write_binary_export` 命令。
- Consumes(i18n,Task 4 会加到 messages,但本 Task 先引用键):`invest_premarket_elapsed`、`invest_premarket_elapsed_running`。

- [ ] **Step 1: 引入 store + 秒表 tick**

在现有 import 段(第 13-17 行)后加:

```typescript
  import { premarketStore } from '$lib/stores/premarket-store.svelte';
```

删除本地 `let generating = $state(false);`(第 117 行),改为从 store 读的派生 + 秒表状态:

```typescript
  const generating = $derived(premarketStore.generating);
  let nowTick = $state(Date.now());
  const elapsedSec = $derived(
    generating
      ? Math.floor((nowTick - premarketStore.startedAt) / 1000)
      : Math.round(premarketStore.lastElapsedMs / 1000),
  );
```

- [ ] **Step 2: 秒表 tick + 完成回调 effect**

把现有 `generate()` 函数(第 204-222 行整段)替换为薄封装 + 两个 `$effect`(放在 `onMount` 之前):

```typescript
  async function generate() {
    errorMsg = null;
    await premarketStore.generate();
  }

  // 秒表:生成中每秒推进 nowTick(驱动 elapsedSec 重算)
  $effect(() => {
    if (!premarketStore.generating) return;
    const h = setInterval(() => { nowTick = Date.now(); }, 1000);
    return () => clearInterval(h);
  });

  // 生成完成(completionSeq 变化)→ 刷新报告 + 回填错误
  let lastSeq = premarketStore.completionSeq;
  $effect(() => {
    const seq = premarketStore.completionSeq;
    if (seq !== lastSeq) {
      lastSeq = seq;
      if (premarketStore.lastError) errorMsg = premarketStore.lastError;
      else loadLatest();
    }
  });
```

- [ ] **Step 3: 跑 check 确认无类型错误**

Run: `npm run check 2>&1 | tail -20`
Expected: 0 errors（`generating` 现为 derived,原引用它的模板 `disabled={generating || loading}` 仍可用)。

- [ ] **Step 4: 导出改走 Tauri save() + write_binary_export**

把 `exportPng`(第 224-244 行)整段替换:

```typescript
  async function exportPng() {
    const el = document.getElementById('report-canvas');
    if (!el) return;
    exportingPng = true;
    try {
      const canvas = await html2canvas(el, {
        scale: 2,
        backgroundColor: getComputedStyle(el).backgroundColor || '#1a1918',
        useCORS: true,
      });
      const base64 = canvas.toDataURL('image/png').split(',')[1];
      const { save } = await import('@tauri-apps/plugin-dialog');
      const path = await save({
        defaultPath: `premarket_${report?.date ?? latestDate ?? 'report'}.png`,
        filters: [{ name: 'PNG', extensions: ['png'] }],
      });
      if (path) await invoke<void>('write_binary_export', { path, base64 });
    } catch (e) {
      console.error('[premarket] exportPng:', e);
      errorMsg = String(e);
    } finally {
      exportingPng = false;
    }
  }
```

- [ ] **Step 5: PDF 导出同改**

把 `exportPdf`(第 246-270 行)整段替换:

```typescript
  async function exportPdf() {
    const el = document.getElementById('report-canvas');
    if (!el) return;
    exportingPdf = true;
    try {
      const canvas = await html2canvas(el, {
        scale: 2,
        backgroundColor: getComputedStyle(el).backgroundColor || '#1a1918',
        useCORS: true,
      });
      const pdf = new jsPDF({
        unit: 'px',
        format: [canvas.width, canvas.height],
        orientation: canvas.width >= canvas.height ? 'landscape' : 'portrait',
      });
      pdf.addImage(canvas.toDataURL('image/png'), 'PNG', 0, 0, canvas.width, canvas.height);
      const base64 = pdf.output('datauristring').split(',')[1];
      const { save } = await import('@tauri-apps/plugin-dialog');
      const path = await save({
        defaultPath: `premarket_${report?.date ?? latestDate ?? 'report'}.pdf`,
        filters: [{ name: 'PDF', extensions: ['pdf'] }],
      });
      if (path) await invoke<void>('write_binary_export', { path, base64 });
    } catch (e) {
      console.error('[premarket] exportPdf:', e);
      errorMsg = String(e);
    } finally {
      exportingPdf = false;
    }
  }
```

- [ ] **Step 6: 工具栏显示耗时**

把工具栏生成按钮(第 325-327 行)替换为按钮 + 耗时标签:

```svelte
    <button class="btn primary" onclick={generate} disabled={generating || loading}>
      {generating
        ? `${t('invest_premarket_elapsed_running')} ${elapsedSec}s`
        : t('invest_premarket_generate_now')}
    </button>
    {#if !generating && premarketStore.lastElapsedMs > 0}
      <span class="elapsed-note">{t('invest_premarket_elapsed')} {elapsedSec}s</span>
    {/if}
```

- [ ] **Step 7: 加 `.elapsed-note` 样式**

在 `<style>` 段 `.btn:disabled { ... }`(第 741 行)之后加:

```css
  .elapsed-note {
    align-self: center;
    font-size: 11px;
    color: var(--text-tertiary);
    font-family: var(--font-mono);
    white-space: nowrap;
  }
```

- [ ] **Step 8: check + build 验证**

Run: `npm run check 2>&1 | tail -20 && npm run build 2>&1 | tail -8`
Expected: check 0 errors;build 成功。

- [ ] **Step 9: 提交**

```bash
git add src/lib/components/invest/PremarketReportTab.svelte
git commit -m "feat(premarket): 组件读 store 状态 + 显示生成耗时 + 导出走 Tauri save()/write_binary_export"
```

---

### Task 4: 入口迁移 system/reports → committee/premarket + i18n

**Files:**
- Modify: `src/routes/invest/+page.svelte`
- Modify: `messages/en.json`、`messages/zh-CN.json`

**Interfaces:**
- Consumes: 现有 `PremarketReportTab`(已 import,第 20 行)。

- [ ] **Step 1: 加 i18n 键**（3 个新键）

`messages/en.json`(在 `"invest_committee_sub_accuracy"` 行后加,并给 premarket 相关补两个耗时键——放在 premarket 键区,如 `"invest_premarket_export_png"` 附近):

```json
  "invest_committee_sub_premarket": "Premarket",
  "invest_premarket_elapsed": "Generated in",
  "invest_premarket_elapsed_running": "Generating…",
```

`messages/zh-CN.json`(同位置):

```json
  "invest_committee_sub_premarket": "盘前观察",
  "invest_premarket_elapsed": "本次用时",
  "invest_premarket_elapsed_running": "生成中",
```

注意:JSON 尾逗号非法——插入时确保前一行有逗号、且不在对象最后一项造成悬空逗号。

- [ ] **Step 2: CommitteeSubTab 类型加 premarket**

`+page.svelte` 第 30 行:

```typescript
  type CommitteeSubTab = 'live' | 'replay' | 'archive' | 'roles' | 'accuracy' | 'premarket';
```

- [ ] **Step 3: committeeSubTabs 数组加一项**（accuracy 之后,第 62 行 accuracy 项后)

```typescript
    { id: 'accuracy', label: t('invest_committee_sub_accuracy') },
    { id: 'premarket', label: t('invest_committee_sub_premarket') },
```

- [ ] **Step 4: committee 渲染块加分支**（第 235-236 行 accuracy 分支后)

```svelte
      {:else if committeeSubTab === 'accuracy'}
        <CommitteeAccuracyTab />
      {:else if committeeSubTab === 'premarket'}
        <PremarketReportTab />
      {/if}
```

- [ ] **Step 5: 从 system 移除 reports**

(a) `SystemSubTab` 类型(第 31 行)删 `'reports'`:

```typescript
  type SystemSubTab = 'cron' | 'events' | 'datasource' | 'pnl_history' | 'insights' | 'dreams' | 'profile' | 'cleanup';
```

(b) `systemSubTabs` 数组(第 48 行)删 `{ id: 'reports', label: t('invest_system_sub_reports') },` 整行。

(c) system 渲染块(第 301-302 行)删:

```svelte
      {:else if systemSubTab === 'reports'}
        <PremarketReportTab />
```

- [ ] **Step 6: check + 快速自检**

Run: `npm run check 2>&1 | tail -20`
Expected: 0 errors。确认无残留 `systemSubTab === 'reports'` 引用:`grep -n "'reports'" src/routes/invest/+page.svelte`(应空)。

- [ ] **Step 7: 提交**

```bash
git add src/routes/invest/+page.svelte messages/en.json messages/zh-CN.json
git commit -m "feat(premarket): 入口从 system/reports 迁到 committee/premarket + i18n 键"
```

---

### Task 5: 01 段加宽横排 + 画布放宽(CSS)

**Files:**
- Modify: `src/lib/components/invest/PremarketReportTab.svelte`(`<style>` 段)

**Interfaces:** 无(纯样式)。

- [ ] **Step 1: 定义画布宽度变量**

在 `.premarket-tab { ... }` 规则(第 709-715 行)内加一行 `--report-w`:

```css
  .premarket-tab {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    padding-bottom: var(--space-4);
    --report-w: 1080px;
  }
```

- [ ] **Step 2: 5 处写死 720px 改引用变量**

分别把这些规则里的 `width: 720px;` 改为 `width: var(--report-w);`:
- `.toolbar`(第 721 行)
- `.settings-panel`(第 744 行)
- `.err-strip`(第 769 行)
- `.empty`(第 779 行)
- `#report-canvas`(第 792 行)

每处用精确 Edit(`width: 720px;` → `width: var(--report-w);`),因 5 处字符串相同,须带各自上下文行保证唯一,或用 replace_all(该字符串仅这 5 处、语义一致,可 `replace_all: true`)。

- [ ] **Step 3: 01 段标签墙改 4 列**

`.theme-wall`(第 832 行)`repeat(2, 1fr)` → `repeat(4, 1fr)`:

```css
  .theme-wall { display: grid; grid-template-columns: repeat(4, 1fr); gap: var(--space-2); }
```

(风险预警卡的 `grid-column: 1 / -1` 内联样式在模板第 437 行已存在,通栏自动随 4 列生效,无需改。)

- [ ] **Step 4: check + build**

Run: `npm run check 2>&1 | tail -10 && npm run build 2>&1 | tail -8`
Expected: check 0 errors;build 成功。确认无残留写死宽:`grep -n "720px" src/lib/components/invest/PremarketReportTab.svelte`(应空)。

- [ ] **Step 5: 提交**

```bash
git add src/lib/components/invest/PremarketReportTab.svelte
git commit -m "style(premarket): 画布宽提为 --report-w:1080px 变量 + 01 段标签墙 4 列横排"
```

---

## 验证与手动冒烟

- **自动**:每 Task 内 `cargo build`(Task1)/`npm run test`(Task2)/`npm run check`+`npm run build`(Task3/4/5)。全部完成后跑一次全量 `npm run check && npm run build && cd src-tauri && cargo build`。
- **手动**(需运行 app):
  1. 进 投资 → 委员会 → 盘前观察(确认 tab 出现在"命中率"之后;system 页不再有"盘前观察")。
  2. 点"立即生成":按钮显示"生成中 Ns"秒表跳动;**生成中切到别的 tab 再切回**,秒表/生成态仍在(store 持有)。完成后显示"本次用时 Ns"。
  3. 点"导出 PNG":弹 Tauri 保存对话框,选路径→落盘 .png,打开可看图。
  4. 点"导出 PDF":同上落盘 .pdf。
  5. 目测画布比原来宽(1080px),01 段标签墙一行 4 卡,风险预警卡仍通栏。

## Self-Review 结论

- **Spec 覆盖**:§1 入口迁移→Task4;§2 导出修复→Task1(后端)+Task3(前端);§3 状态提 store→Task2+Task3;§4 显示耗时→Task2(逻辑)+Task3(UI);§5 加宽横排→Task5。5 项全覆盖。
- **类型一致性**:`premarketStore` 字段/方法签名在 Task2 定义、Task3 消费,名称一致(`generating`/`startedAt`/`lastElapsedMs`/`completionSeq`/`generate()`/`markStart`/`markFinish`/`elapsedSec`/`tick`)。`write_binary_export(path, base64)` 在 Task1 定义、Task3 调用,参数名一致。
- **i18n**:3 新键(`invest_committee_sub_premarket`/`invest_premarket_elapsed`/`invest_premarket_elapsed_running`)Task4 加到两份 messages、Task3 引用。
- **无占位符**:每步含实际代码/命令/预期输出。

## Execution Handoff

计划已存 `docs/superpowers/plans/2026-07-09-premarket-frontend-overhaul.md`。沿用后端同样的 **Subagent-Driven**(每 Task 独立 subagent + 两阶段评审)执行。
