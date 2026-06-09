<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import type { ArchivedDecision } from '$lib/stores/invest-committee-store.svelte';
  import { getVerdictBadgeStyle, buildVerdictMap } from '$lib/utils/invest-verdict';
  import MarkdownContent from '$lib/components/MarkdownContent.svelte';

  let symbol = $state('');
  let days = $state(14);
  let loading = $state(false);
  let archives = $state<ArchivedDecision[]>([]);
  let selectedDate = $state<string | null>(null);

  const selected = $derived(archives.find((a) => a.date === selectedDate) ?? null);

  // Pre-compute verdicts once per archives change — avoids re-running regex
  // on every selectedDate click for the entire list.
  const verdictMap = $derived.by(() => buildVerdictMap(archives));

  // Combined HOLD + WATCH list for dropdown — use centralized nameMap
  const symbolOptions = $derived(
    investStore.holdings
      .filter((h) => h.kind === 'hold' || h.kind === 'watch')
      .map((h) => ({ symbol: h.symbol, label: h.name ? `${h.symbol} ${h.name}` : h.symbol }))
  );

  // Symbol → Chinese name lookup from centralized store (enriched from holdings + price cache + trades)
  function symLabel(sym: string): string {
    return investStore.nameMap.get(sym) ?? sym;
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

<div class="archive-grid grid h-full gap-[var(--space-4)]">
  <!-- Left panel: query + list -->
  <aside class="flex flex-col gap-[var(--space-3)] overflow-y-auto rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-3)]">
    <!-- Symbol selector + query button -->
    <div class="flex items-center gap-[var(--space-2)]">
      <select
        class="flex-1 rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-[var(--space-2)] py-[6px] text-[12px] text-[var(--text-primary)]"
        bind:value={symbol}
      >
        <option value="">{t('invest_archive_symbol_label')}</option>
        {#each symbolOptions as opt}
          <option value={opt.symbol}>{opt.label}</option>
        {/each}
      </select>
      <button
        class="rounded-[var(--radius-md)] bg-[var(--accent)] px-[var(--space-3)] py-[6px] text-[12px] font-medium text-[var(--bg-base)] transition-opacity hover:opacity-90 disabled:opacity-40"
        disabled={loading || !symbol.trim()}
        onclick={load}
      >
        {loading ? '...' : t('invest_archive_query')}
      </button>
    </div>

    <!-- Days selector -->
    <div class="flex items-center gap-[var(--space-2)] text-[11px] text-[var(--text-tertiary)]">
      <span>{t('invest_replay_days')}:</span>
      <input
        type="number"
        min="1"
        max="90"
        class="w-16 rounded-[var(--radius-sm)] border border-border bg-[var(--bg-input)] px-2 py-0.5 text-[11px] text-[var(--text-primary)]"
        bind:value={days}
      />
    </div>

    <!-- Archive list -->
    <div class="flex-1 overflow-y-auto">
      {#if archives.length > 0}
        <div class="space-y-0.5">
          {#each archives as archive}
            {@const v = verdictMap.get(archive.date) ?? null}
            <button
              class="flex w-full items-center justify-between rounded-[var(--radius-md)] border px-[var(--space-2)] py-[var(--space-1)] text-left text-[12px] transition-colors"
              class:border-[var(--accent)]={selectedDate === archive.date}
              class:bg-[var(--accent-muted)]={selectedDate === archive.date}
              class:border-transparent={selectedDate !== archive.date}
              class:hover:bg-[var(--bg-hover)]={selectedDate !== archive.date}
              onclick={() => (selectedDate = archive.date)}
            >
              <span class="font-[var(--font-mono)] text-[var(--text-secondary)]">{archive.date}</span>
              {#if v}
                <span class="rounded-[var(--radius-sm)] px-1.5 py-0.5 text-[9px] font-bold" style={getVerdictBadgeStyle(v)}>
                  {v}
                </span>
              {/if}
            </button>
          {/each}
        </div>
      {:else if loading}
        <div class="py-4 text-center text-[11px] text-[var(--text-tertiary)]">
          {t('invest_replay_loading')}
        </div>
      {:else}
        <div class="py-4 text-center text-[11px] text-[var(--text-tertiary)]">
          {t('invest_archive_empty')}
        </div>
      {/if}
    </div>
  </aside>

  <!-- Right panel: detail -->
  <section class="flex min-h-0 flex-1 flex-col overflow-y-auto rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
    {#if selected}
      {@const v = verdictMap.get(selected.date) ?? null}
      <div class="mb-[var(--space-3)] flex items-center gap-[var(--space-3)]">
        <span class="text-[14px] font-semibold text-[var(--text-primary)]" title={selected.symbol}>
          {symLabel(selected.symbol)}
        </span>
        <span class="font-[var(--font-mono)] text-xs text-[var(--text-secondary)]">{selected.symbol}</span>
        <span class="text-xs text-[var(--text-tertiary)]">— {selected.date}</span>
        {#if v}
          <span class="ml-auto rounded-[var(--radius-full)] px-3 py-1 text-[11px] font-bold" style={getVerdictBadgeStyle(v)}>
            {v}
          </span>
        {/if}
      </div>
      <div class="min-h-0 flex-1 overflow-y-auto text-[13px] leading-[1.7]">
        <MarkdownContent text={selected.content} />
      </div>
    {:else}
      <div class="flex h-full items-center justify-center text-[13px] text-[var(--text-tertiary)]">
        {t('invest_archive_select')}
      </div>
    {/if}
  </section>
</div>

<style>
  .archive-grid {
    grid-template-columns: 250px 1fr;
    grid-template-rows: 1fr;
  }
  @media (max-width: 768px) {
    .archive-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
