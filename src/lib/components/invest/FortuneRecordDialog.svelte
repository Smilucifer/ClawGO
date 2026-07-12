<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { fortuneStore } from '$lib/stores/fortune-store.svelte';
  import { fmtReturn } from './fortune-helpers';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import { guardedSave } from '$lib/utils/with-saving';

  let { onclose }: { onclose: () => void } = $props();
  let mode = $state<'single' | 'batch'>('single');

  // ── 单日 ──
  let singleDate = $state(new Date().toISOString().slice(0, 10));   // 默认今天
  let singleVal = $state('');
  let singleNote = $state('');

  // ── 批量：按月 ──
  const today = new Date().toISOString().slice(0, 10);              // "YYYY-MM-DD"
  const [ty, tm] = today.split('-').map(Number);
  let year = $state(ty);
  let month = $state(tm);                      // 1-12
  const isCurrentMonth = $derived(year === ty && month === tm);

  // 该月工作日列表（当前月截到今天）
  function workdays(y: number, m: number): string[] {
    const out: string[] = [];
    const last = new Date(y, m, 0).getDate();   // 该月天数
    for (let d = 1; d <= last; d++) {
      const dow = new Date(y, m - 1, d).getDay();  // 0=日,6=六
      if (dow === 0 || dow === 6) continue;
      const ds = `${y}-${String(m).padStart(2,'0')}-${String(d).padStart(2,'0')}`;
      if (y === ty && m === tm && ds > today) break;  // 当前月不列未来
      out.push(ds);
    }
    return out;
  }
  const dates = $derived(workdays(year, month));

  // 已录入映射：date → return_pct（从 store 拿）
  const recorded = $derived(fortuneStore.recordedMap);

  // 批量输入缓存：date → string（每月重置，避免无限增长）
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

  function prevMonth() {
    if (month === 1) { year -= 1; month = 12; } else { month -= 1; }
  }
  function nextMonth() {
    if (isCurrentMonth) return;   // 不可翻未来
    if (month === 12) { year += 1; month = 1; } else { month += 1; }
  }

  let saving = $state(false);
  // 覆盖确认
  let overwriteConfirm = $state<{ date: string; val: number; note: string; batchCount?: number } | null>(null);
  let overwriteOpen = $state(false);
  const overwriteMessage = $derived(overwriteConfirm
    ? (() => {
        const oc = overwriteConfirm;
        const existing = fortuneStore.recordedMap.get(oc.date);
        const val = existing != null ? fmtReturn(existing) : '—';
        return oc.batchCount && oc.batchCount > 1
          ? t('fortune_overwrite_batch_confirm', { count: String(oc.batchCount), date: oc.date, val })
          : t('fortune_overwrite_confirm', { date: oc.date, val });
      })()
    : '');

  async function checkOverwriteAndSubmit(date: string, val: number, note: string) {
    if (saving) return;
    const existing = recorded.get(date);
    if (existing != null) {
      overwriteConfirm = { date, val, note };
      overwriteOpen = true;
      return;
    }
    await guardedSave(saving, (v) => saving = v, async () => {
      await fortuneStore.upsert(date, val, note);
      onclose();
    });
  }

  async function handleOverwriteConfirm() {
    const oc = overwriteConfirm;
    if (!oc) return;
    overwriteOpen = false;
    if (mode === 'single') {
      await guardedSave(saving, (v) => saving = v, async () => {
        await fortuneStore.upsert(oc.date, oc.val, oc.note);
        onclose();
      });
    } else {
      await submitBatch();
    }
  }

  async function submitSingle() {
    const v = parseFloat(singleVal);
    if (Number.isNaN(v)) return;
    checkOverwriteAndSubmit(singleDate, v, singleNote);
  }

  async function submitBatch() {
    if (saving) return;
    const entries = dates
      .filter((ds) => batchVals[ds]?.trim() !== '')
      .map((ds) => ({ date: ds, returnPct: parseFloat(batchVals[ds]), note: '' }))
      .filter((e) => !Number.isNaN(e.returnPct));
    if (!entries.length) return;
    // 批量模式：检查是否有已录入日期需要覆盖
    const conflicts = fortuneStore.findConflicts(entries.map((e) => e.date));
    if (conflicts.length > 0 && !overwriteOpen) {
      const first = entries.find((e) => e.date === conflicts[0].date)!;
      overwriteConfirm = { date: first.date, val: first.returnPct, note: first.note, batchCount: conflicts.length };
      overwriteOpen = true;
      return;
    }
    overwriteOpen = false;
    await guardedSave(saving, (v) => saving = v, async () => {
      await fortuneStore.batchUpsert(entries);
      onclose();
    });
  }
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onclick={onclose}>
  <div class="max-h-[80vh] w-[520px] overflow-auto rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-5)]"
    onclick={(e) => e.stopPropagation()}>
    <div class="mb-[var(--space-4)] flex gap-[var(--space-2)]">
      <button class:text-[var(--accent)]={mode === 'single'} onclick={() => (mode = 'single')}>单日</button>
      <button class:text-[var(--accent)]={mode === 'batch'} onclick={() => (mode = 'batch')}>批量</button>
    </div>

    {#if mode === 'single'}
      <div class="flex flex-col gap-[var(--space-3)]">
        <input type="date" bind:value={singleDate}
          class="rounded-[var(--radius-sm)] border border-border bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-[13px]" />
        <input type="number" step="0.01" bind:value={singleVal} placeholder="收益率 %"
          class="rounded-[var(--radius-sm)] border border-border bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-[13px]" />
        <input bind:value={singleNote} placeholder="备注（可选）"
          class="rounded-[var(--radius-sm)] border border-border bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-[13px]" />
        <button class="rounded-[var(--radius-sm)] bg-[var(--accent)] px-[var(--space-4)] py-[var(--space-2)] text-[13px] font-semibold text-[var(--bg-base)]"
          onclick={submitSingle}>保存</button>
      </div>
    {:else}
      <div class="mb-[var(--space-3)] flex items-center justify-between">
        <button class="text-[12px] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
          onclick={prevMonth}>‹ 上一月</button>
        <span class="font-mono text-[13px] font-semibold">{year}年{month}月</span>
        <button class="text-[12px] text-[var(--text-secondary)] hover:text-[var(--text-primary)] disabled:opacity-40"
          onclick={nextMonth} disabled={isCurrentMonth}>下一月 ›</button>
      </div>
      <div class="flex flex-col gap-[var(--space-2)]">
        {#each dates as ds}
          <div class="flex items-center gap-[var(--space-3)]">
            <span class="w-24 font-mono text-[12px] text-[var(--text-secondary)]">
              {ds}{#if recorded.has(ds)}<span style="color:var(--up)"> ●</span>{/if}
            </span>
            <input type="number" step="0.01" bind:value={batchVals[ds]} placeholder="—"
              class="flex-1 rounded-[var(--radius-sm)] border border-border bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-[13px]" />
          </div>
        {/each}
      </div>
      <button class="mt-[var(--space-3)] rounded-[var(--radius-sm)] bg-[var(--accent)] px-[var(--space-4)] py-[var(--space-2)] text-[13px] font-semibold text-[var(--bg-base)]"
        onclick={submitBatch}>批量保存</button>
    {/if}
  </div>
</div>

<ConfirmDialog
  bind:open={overwriteOpen}
  message={overwriteMessage}
  confirmLabel={t('fortune_btn_overwrite')}
  cancelLabel={t('fortune_btn_cancel')}
  variant="default"
  onConfirm={handleOverwriteConfirm}
  onCancel={() => (overwriteConfirm = null)}
/>
