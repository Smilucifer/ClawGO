<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { fortuneStore } from '$lib/stores/fortune-store.svelte';

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

  // 已录入映射：date → return_pct（从 analysis.calendar 拿）
  const recorded = $derived.by(() => {
    const m = new Map<string, number>();
    for (const d of fortuneStore.analysis?.calendar ?? []) {
      if (d.actualReturn != null) m.set(d.date, d.actualReturn);
    }
    return m;
  });

  // 批量输入缓存：date → string
  let batchVals = $state<Record<string, string>>({});
  // 进入某月时用已录旧值预填空白项
  $effect(() => {
    for (const ds of dates) {
      if (batchVals[ds] === undefined) {
        const old = recorded.get(ds);
        batchVals[ds] = old != null ? String(old) : '';
      }
    }
  });

  function prevMonth() {
    if (month === 1) { year -= 1; month = 12; } else { month -= 1; }
  }
  function nextMonth() {
    if (isCurrentMonth) return;   // 不可翻未来
    if (month === 12) { year += 1; month = 1; } else { month += 1; }
  }

  async function submitSingle() {
    const v = parseFloat(singleVal);
    if (Number.isNaN(v)) return;   // 前端拦截非法
    await fortuneStore.upsert(singleDate, v, singleNote);
    onclose();
  }
  async function submitBatch() {
    const entries = dates
      .filter((ds) => batchVals[ds]?.trim() !== '')
      .map((ds) => ({ date: ds, returnPct: parseFloat(batchVals[ds]), note: '' }))
      .filter((e) => !Number.isNaN(e.returnPct));   // 跳过非法行
    if (entries.length) await fortuneStore.batchUpsert(entries);
    onclose();
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
