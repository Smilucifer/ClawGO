<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { fmtRelative } from '$lib/i18n/format';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import EventTriggerDialog from './EventTriggerDialog.svelte';
  import type { InvestEvent } from '$lib/types';

  let { onNavigateToCommittee }: { onNavigateToCommittee?: () => void } = $props();

  const store = investStore;

  onMount(() => {
    store.fetchEvents();
    store.fetchScanStatus();
  });

  function handleTimeWindowChange(window: '24h' | '48h' | '7d') {
    store.setEventFilter({ timeWindow: window });
  }

  function handleSeverityChange(severity: 'all' | 'high' | 'medium' | 'low') {
    store.setEventFilter({ severity });
  }

  function handleSearchInput(e: Event) {
    const target = e.target as HTMLInputElement;
    store.setEventFilter({ search: target.value });
  }

  function handleScan() {
    store.triggerScan();
  }

  function severityColor(severity: string): string {
    switch (severity) {
      case 'high': return 'text-[#a87a7a] bg-[var(--color-error-bg)] border-[var(--color-error-bg)]';
      case 'medium': return 'text-[#b89a6a] bg-[var(--color-warning-bg)] border-[var(--color-warning-bg)]';
      default: return 'text-[var(--text-secondary)] bg-[var(--bg-hover)] border-[var(--border)]';
    }
  }

  function stanceColor(stance: string): string {
    switch (stance) {
      case 'bullish': return 'text-[var(--color-success)]';
      case 'bearish': return 'text-[var(--color-error)]';
      default: return 'text-[var(--text-tertiary)]';
    }
  }

  function severityLabel(severity: string): string {
    switch (severity) {
      case 'high': return t('invest.eventWatch.filterHigh');
      case 'medium': return t('invest.eventWatch.filterMedium');
      case 'low': return t('invest.eventWatch.filterLow');
      default: return severity;
    }
  }

  function stanceLabel(stance: string): string {
    switch (stance) {
      case 'bullish': return t('invest.eventWatch.stanceBullish');
      case 'bearish': return t('invest.eventWatch.stanceBearish');
      case 'neutral': return t('invest.eventWatch.stanceNeutral');
      default: return stance;
    }
  }

  let triggerEvent = $state<InvestEvent | null>(null);

  function handleTrigger(ev: InvestEvent) {
    triggerEvent = ev;
  }

  let expandedIds = $state<Set<string>>(new Set());

  function toggleExpand(id: string) {
    if (expandedIds.has(id)) {
      expandedIds.delete(id);
    } else {
      expandedIds.add(id);
    }
    expandedIds = new Set(expandedIds); // trigger reactivity
  }
</script>

<div class="flex flex-col h-full">
  <!-- Status bar -->
  <div class="flex items-center justify-between px-[var(--space-4)] py-[var(--space-2)] border-b border-[var(--border)]">
    <div class="flex items-center gap-3 text-[12px] text-[var(--text-secondary)]">
      {#if store.scanStatus}
        <span>{store.scanStatus.totalEvents} {t('invest.eventWatch.events')}</span>
        <span class="text-[#a87a7a]">{store.scanStatus.highCount} {t('invest.eventWatch.high')}</span>
        {#if store.scanStatus.untriggeredHigh > 0}
          <span class="text-[#b89a6a]">{store.scanStatus.untriggeredHigh} {t('invest.eventWatch.untriggered')}</span>
        {/if}
        {#if store.scanStatus.lastEventAt}
          <span>{t('invest.eventWatch.last')}: {fmtRelative(store.scanStatus.lastEventAt)}</span>
        {/if}
      {:else}
        <span>{t('invest.eventWatch.noScanData')}</span>
      {/if}
    </div>
    <div class="flex items-center gap-2">
      {#if store.error}
        <span class="text-[12px] text-[#a87a7a] max-w-xs truncate" title={store.error}>{store.error}</span>
      {/if}
      <button
        onclick={handleScan}
        disabled={store.isScanning}
        class="rounded-[var(--radius-md)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] bg-[var(--bg-input)] hover:bg-[var(--bg-hover)] text-[var(--text-secondary)] transition-colors disabled:opacity-50"
      >
        {store.isScanning ? t('invest.eventWatch.scanning') : t('invest.eventWatch.scanNow')}
      </button>
    </div>
  </div>

  <!-- Filters -->
  <div class="flex items-center gap-2 px-[var(--space-4)] py-[var(--space-2)] border-b border-[var(--border)]">
    <!-- Time window -->
    <div class="flex rounded-[var(--radius-md)] overflow-hidden border border-[var(--border)]">
      {#each ['24h', '48h', '7d'] as tw}
        <button
          onclick={() => handleTimeWindowChange(tw as '24h' | '48h' | '7d')}
          class="px-2 py-0.5 text-[12px] {store.eventFilter.timeWindow === tw ? 'bg-[var(--bg-hover)] text-[var(--text-primary)]' : 'text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]'}"
        >{tw}</button>
      {/each}
    </div>

    <!-- Severity -->
    <div class="flex gap-1">
      {#each ['all', 'high', 'medium', 'low'] as sev}
        <button
          onclick={() => handleSeverityChange(sev as 'all' | 'high' | 'medium' | 'low')}
          class="px-2 py-0.5 text-[12px] rounded-[var(--radius-md)] border {store.eventFilter.severity === sev ? 'border-[var(--text-tertiary)] text-[var(--text-primary)]' : 'border-[var(--border)] text-[var(--text-tertiary)] hover:text-[var(--text-secondary)]'}"
        >{sev === 'all' ? t('invest.eventWatch.filterAll') : sev === 'high' ? t('invest.eventWatch.filterHigh') : sev === 'medium' ? t('invest.eventWatch.filterMedium') : t('invest.eventWatch.filterLow')}</button>
      {/each}
    </div>

    <!-- Search -->
    <input
      type="text"
      placeholder={t('invest.eventWatch.searchPlaceholder')}
      value={store.eventFilter.search}
      oninput={handleSearchInput}
      class="ml-auto rounded-[var(--radius-md)] px-[var(--space-2)] py-[var(--space-1)] text-[12px] bg-[var(--bg-input)] border border-[var(--border)] text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)] w-40"
    />
  </div>

  <!-- Event list -->
  <div class="flex-1 overflow-y-auto">
    {#if store.filteredEvents.length === 0}
      <div class="flex items-center justify-center h-full text-[var(--text-tertiary)] text-[13px]">
        {t('invest.eventWatch.noEvents')}
      </div>
    {:else}
      {#each store.filteredEvents as event (event.id)}
        <div class="px-[var(--space-4)] py-[var(--space-2)] border-b border-[var(--border)]/50 hover:bg-[var(--bg-hover)] transition-colors">
          <div class="flex items-start gap-2">
            <!-- Severity badge -->
            <span class="px-1.5 py-0.5 text-[10px] rounded-[var(--radius-full)] border {severityColor(event.severity)}">
              {severityLabel(event.severity)}
            </span>

            <!-- Content -->
            <div class="flex-1 min-w-0 cursor-pointer" role="button" tabindex="0" onclick={() => toggleExpand(event.id)} onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleExpand(event.id); } }}>
              <div class="flex items-center gap-2">
                <span class="text-[13px] text-[var(--text-primary)] truncate" title={event.body && event.title !== event.body ? event.title : ''}>{event.body || event.title}</span>
                <span class="text-[10px] {stanceColor(event.stance)}">{stanceLabel(event.stance)}</span>
              </div>
              {#if event.body && event.title && event.title !== event.body}
                {#if expandedIds.has(event.id)}
                  <p class="text-[12px] text-[var(--text-secondary)] mt-1">{event.title}</p>
                {:else}
                  <p class="text-[12px] text-[var(--text-tertiary)] mt-0.5 line-clamp-1">{event.title}</p>
                {/if}
              {/if}
              <div class="flex items-center gap-2 mt-1">
                <span class="text-[10px] text-[var(--text-tertiary)]">{event.source}</span>
                <span class="text-[10px] text-[var(--text-tertiary)]">{fmtRelative(event.createdAt)}</span>
                {#if event.symbols}
                  <div class="flex gap-1">
                    {#each event.symbols.split(',').filter(Boolean) as sym}
                      <span class="px-1 py-0 text-[10px] bg-[var(--bg-input)] rounded-[var(--radius-md)] text-[var(--text-secondary)]">{sym}</span>
                    {/each}
                  </div>
                {/if}
              </div>
            </div>

            <!-- Trigger button -->
            {#if event.severity === 'high' && !event.triggered}
              <button
                onclick={(e) => { e.stopPropagation(); handleTrigger(event); }}
                class="rounded-[var(--radius-md)] px-2 py-1 text-[12px] bg-[var(--color-warning-bg)] hover:bg-[var(--color-warning-bg)]/80 text-[var(--color-warning)] transition-colors"
              >
                {t('invest.eventWatch.triggerCommittee')}
              </button>
            {:else if event.triggered}
              <span class="text-[10px] text-[var(--text-tertiary)]">{t('invest.eventWatch.triggered')}</span>
            {/if}
          </div>
        </div>
      {/each}
    {/if}
  </div>
</div>

{#if triggerEvent}
  <EventTriggerDialog
    event={triggerEvent}
    onClose={() => triggerEvent = null}
    onTriggered={() => {
      triggerEvent = null;
      onNavigateToCommittee?.();
    }}
  />
{/if}
