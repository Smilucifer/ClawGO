<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import type { ArchivedDecision } from '$lib/stores/invest-committee-store.svelte';

  let symbol = $state('');
  let days = $state(14);
  let loading = $state(false);
  let archives = $state<ArchivedDecision[]>([]);
  let selectedDate = $state<string | null>(null);

  const selected = $derived(archives.find((a) => a.date === selectedDate) ?? null);

  // Combined HOLD + WATCH list for dropdown
  const symbolOptions = $derived(
    investStore.holdings
      .filter((h) => h.kind === 'hold' || h.kind === 'watch')
      .map((h) => ({ symbol: h.symbol, label: h.name ? `${h.symbol} ${h.name}` : h.symbol }))
  );

  // Symbol → Chinese name lookup from holdings
  const nameMap = $derived.by(() => {
    const map = new Map<string, string>();
    for (const h of investStore.holdings) {
      if (h.name) map.set(h.symbol, h.name);
    }
    return map;
  });

  function symLabel(sym: string): string {
    return nameMap.get(sym) ?? sym;
  }

  async function load() {
    if (!symbol.trim()) return;
    loading = true;
    try {
      archives = await investCommitteeStore.loadArchive(symbol.trim(), days);
      selectedDate = archives[0]?.date ?? null;
    } catch (e) {
      console.error('Failed to load archive:', e);
    } finally {
      loading = false;
    }
  }
</script>

<div class="flex h-full gap-4">
  <!-- Left panel: list -->
  <div class="w-64 shrink-0 flex flex-col gap-3">
    <div class="flex items-end gap-2">
      <div class="flex-1">
        <label class="mb-1 block text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_archive_symbol_label')}</label>
        <select
          class="w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-3 py-1.5 text-[13px] text-[var(--text-primary)]"
          bind:value={symbol}
        >
          <option value="">{t('invest_archive_empty')}</option>
          {#each symbolOptions as opt}
            <option value={opt.symbol}>{opt.label}</option>
          {/each}
        </select>
      </div>
      <button
        class="rounded-[var(--radius-md)] bg-[var(--accent)] px-3 py-1.5 text-[13px] font-medium text-[#1a1918] disabled:opacity-40 transition-opacity"
        disabled={loading || !symbol.trim()}
        onclick={load}
      >
        {loading ? '...' : t('invest_archive_query')}
      </button>
    </div>

    {#if archives.length > 0}
      <div class="flex flex-col gap-1">
        {#each archives as archive}
          <button
            class="flex w-full items-center justify-between rounded-[var(--radius-md)] border px-3 py-2 text-left text-[13px] transition-colors"
            class:border-[var(--accent)]={selectedDate === archive.date}
            class:bg-[var(--accent-muted)]={selectedDate === archive.date}
            class:border-[var(--border)]={selectedDate !== archive.date}
            class:text-[var(--text-primary)]={selectedDate === archive.date}
            class:text-[var(--text-secondary)]={selectedDate !== archive.date}
            onclick={() => (selectedDate = archive.date)}
          >
            <span class="font-medium">{archive.date}</span>
            <span class="text-[11px] text-[var(--text-tertiary)]" title={archive.symbol}>{symLabel(archive.symbol)}</span>
          </button>
        {/each}
      </div>
    {:else}
      <div class="py-4 text-center text-[11px] text-[var(--text-tertiary)]">
        {t('invest_archive_empty')}
      </div>
    {/if}
  </div>

  <!-- Right panel: detail -->
  <div class="flex-1 overflow-auto">
    {#if selected}
      <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] p-4">
        <div class="mb-2 flex items-center justify-between">
          <span class="text-[13px] font-semibold text-[var(--text-primary)]" title={selected.symbol}>{symLabel(selected.symbol)}</span>
          <span class="text-[11px] text-[var(--text-tertiary)]">{selected.date}</span>
        </div>
        <div class="max-h-[60vh] overflow-y-auto whitespace-pre-wrap font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)] leading-relaxed">
          {selected.content}
        </div>
      </div>
    {:else}
      <div class="flex h-full items-center justify-center text-[13px] text-[var(--text-tertiary)]">
        {t('invest_archive_select')}
      </div>
    {/if}
  </div>
</div>
