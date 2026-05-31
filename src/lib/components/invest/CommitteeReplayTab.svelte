<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import type { ArchivedDecision } from '$lib/stores/invest-committee-store.svelte';

  let symbol = $state('');
  let manualMode = $state(false);
  let days = $state(7);
  let loading = $state(false);
  let archives = $state<ArchivedDecision[]>([]);
  let selectedIndex = $state(0);
  let dryRunning = $state(false);

  const selected = $derived(archives[selectedIndex] ?? null);

  const allHoldings = $derived.by(() => {
    const seen = new Set<string>();
    const result: Array<{ symbol: string; name: string; kind: string }> = [];
    for (const h of investStore.holdHoldings) {
      if (!seen.has(h.symbol)) {
        seen.add(h.symbol);
        result.push({ symbol: h.symbol, name: h.name ?? h.symbol, kind: 'HOLD' });
      }
    }
    for (const h of investStore.watchHoldings) {
      if (!seen.has(h.symbol)) {
        seen.add(h.symbol);
        result.push({ symbol: h.symbol, name: h.name ?? h.symbol, kind: 'WATCH' });
      }
    }
    return result;
  });

  async function load() {
    if (!symbol.trim()) return;
    loading = true;
    try {
      archives = await investCommitteeStore.loadArchive(symbol.trim(), days);
      selectedIndex = 0;
    } catch (e) {
      console.error('Failed to load archive:', e);
    } finally {
      loading = false;
    }
  }

  async function dryRun() {
    if (!symbol.trim()) return;
    dryRunning = true;
    try {
      await investCommitteeStore.runCommittee([symbol.trim()]);
    } catch (e) {
      console.error('Dry run failed:', e);
    } finally {
      dryRunning = false;
    }
  }
</script>

<div class="space-y-4">
  <div class="flex items-end gap-2">
    <div class="flex-1">
      <label class="mb-1 block text-sm text-muted-foreground">{t('invest_replay_symbol')}</label>
      {#if allHoldings.length > 0 && !manualMode}
        <select
          class="w-full rounded border border-border bg-background px-3 py-1.5 text-sm"
          bind:value={symbol}
        >
          <option value="">{t('invest_replay_select_placeholder')}</option>
          {#each allHoldings as h}
            <option value={h.symbol}>{h.name} ({h.symbol}) [{h.kind}]</option>
          {/each}
        </select>
        <button class="mt-1 text-xs text-muted-foreground hover:underline" onclick={() => { manualMode = true; symbol = ''; }}>
          {t('invest_replay_manual_input')}
        </button>
      {:else}
        <input
          type="text"
          class="w-full rounded border border-border bg-background px-3 py-1.5 text-sm"
          placeholder="600519.SH"
          bind:value={symbol}
        />
        {#if allHoldings.length > 0}
          <button class="mt-1 text-xs text-muted-foreground hover:underline" onclick={() => { manualMode = false; symbol = ''; }}>
            {t('invest_replay_select_from_holdings')}
          </button>
        {/if}
      {/if}
    </div>
    <div>
      <label class="mb-1 block text-sm text-muted-foreground">{t('invest_replay_days')}</label>
      <input
        type="number"
        class="w-20 rounded border border-border bg-background px-3 py-1.5 text-sm"
        bind:value={days}
        min="1"
        max="30"
      />
    </div>
    <button
      class="rounded bg-primary px-4 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
      disabled={loading || !symbol.trim()}
      onclick={load}
    >
      {loading ? t('invest_replay_loading') : t('invest_replay_load')}
    </button>
    <button
      class="rounded bg-muted px-3 py-1.5 text-sm disabled:opacity-50"
      disabled={dryRunning || !symbol.trim()}
      onclick={dryRun}
    >
      {dryRunning ? '...' : t('invest_dry_run')}
    </button>
  </div>

  {#if archives.length === 0 && !loading}
    <div class="py-8 text-center text-sm text-muted-foreground">
      {t('invest_replay_empty')}
    </div>
  {:else if archives.length > 0}
    <!-- Date selector -->
    <div class="flex gap-1 overflow-x-auto border-b border-border pb-1">
      {#each archives as archive, i}
        <button
          class="rounded-t px-2 py-1 text-xs transition-colors"
          class:bg-primary={selectedIndex === i}
          class:text-primary-foreground={selectedIndex === i}
          class:text-muted-foreground={selectedIndex !== i}
          onclick={() => (selectedIndex = i)}
        >
          {archive.date}
        </button>
      {/each}
    </div>

    {#if selected}
      <div class="rounded-lg border border-border p-4">
        <div class="mb-2 flex items-center justify-between">
          <span class="text-sm font-semibold">{selected.symbol}</span>
          <span class="text-xs text-muted-foreground">{selected.date}</span>
        </div>
        <div class="max-h-[60vh] overflow-y-auto whitespace-pre-wrap font-mono text-sm">
          {selected.content}
        </div>
      </div>
    {/if}
  {/if}
</div>
