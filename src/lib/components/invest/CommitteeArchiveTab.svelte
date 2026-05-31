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
  const nameMap = $derived(() => {
    const map = new Map<string, string>();
    for (const h of investStore.holdings) {
      if (h.name) map.set(h.symbol, h.name);
    }
    return map;
  });

  function symLabel(sym: string): string {
    return nameMap().get(sym) ?? sym;
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
  <div class="w-64 shrink-0 space-y-3">
    <div class="flex items-end gap-2">
      <div class="flex-1">
        <label class="mb-1 block text-xs text-muted-foreground">{t('invest_archive_symbol_label')}</label>
        <select
          class="w-full rounded border border-border bg-background px-3 py-1.5 text-sm"
          bind:value={symbol}
        >
          <option value="">{t('invest_archive_empty')}</option>
          {#each symbolOptions as opt}
            <option value={opt.symbol}>{opt.label}</option>
          {/each}
        </select>
      </div>
      <button
        class="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
        disabled={loading || !symbol.trim()}
        onclick={load}
      >
        {loading ? '...' : t('invest_archive_query')}
      </button>
    </div>

    {#if archives.length > 0}
      <div class="space-y-1">
        {#each archives as archive}
          <button
            class="flex w-full items-center justify-between rounded border px-3 py-2 text-left text-sm transition-colors"
            class:border-primary={selectedDate === archive.date}
            class:bg-muted={selectedDate === archive.date}
            onclick={() => (selectedDate = archive.date)}
          >
            <span class="font-medium">{archive.date}</span>
            <span class="text-xs text-muted-foreground" title={archive.symbol}>{symLabel(archive.symbol)}</span>
          </button>
        {/each}
      </div>
    {:else}
      <div class="py-4 text-center text-xs text-muted-foreground">
        {t('invest_archive_empty')}
      </div>
    {/if}
  </div>

  <!-- Right panel: detail -->
  <div class="flex-1 overflow-auto">
    {#if selected}
      <div class="rounded-lg border border-border p-4">
        <div class="mb-2 flex items-center justify-between">
          <span class="text-sm font-semibold" title={selected.symbol}>{symLabel(selected.symbol)}</span>
          <span class="text-xs text-muted-foreground">{selected.date}</span>
        </div>
        <div class="max-h-[60vh] overflow-y-auto whitespace-pre-wrap font-mono text-sm">
          {selected.content}
        </div>
      </div>
    {:else}
      <div class="flex h-full items-center justify-center text-sm text-muted-foreground">
        {t('invest_archive_select')}
      </div>
    {/if}
  </div>
</div>
