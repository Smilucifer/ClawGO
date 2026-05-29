<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';

  const store = investStore;

  // Load events on mount
  $effect(() => {
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

  function formatTime(dateStr: string): string {
    if (!dateStr) return '';
    const d = new Date(dateStr);
    const now = new Date();
    const diffMs = now.getTime() - d.getTime();
    const diffMin = Math.floor(diffMs / 60000);
    if (diffMin < 60) return `${diffMin}m ago`;
    const diffH = Math.floor(diffMin / 60);
    if (diffH < 24) return `${diffH}h ago`;
    return d.toLocaleDateString();
  }

  function severityColor(severity: string): string {
    switch (severity) {
      case 'high': return 'text-red-400 bg-red-500/10 border-red-500/30';
      case 'medium': return 'text-yellow-400 bg-yellow-500/10 border-yellow-500/30';
      default: return 'text-zinc-400 bg-zinc-500/10 border-zinc-500/30';
    }
  }

  function stanceColor(stance: string): string {
    switch (stance) {
      case 'bullish': return 'text-green-400';
      case 'bearish': return 'text-red-400';
      default: return 'text-zinc-400';
    }
  }
</script>

<div class="flex flex-col h-full">
  <!-- Status bar -->
  <div class="flex items-center justify-between px-4 py-2 border-b border-zinc-800">
    <div class="flex items-center gap-3 text-xs text-zinc-400">
      {#if store.scanStatus}
        <span>{store.scanStatus.totalEvents} events</span>
        <span class="text-red-400">{store.scanStatus.highCount} high</span>
        {#if store.scanStatus.untriggeredHigh > 0}
          <span class="text-amber-400">{store.scanStatus.untriggeredHigh} untriggered</span>
        {/if}
        {#if store.scanStatus.lastEventAt}
          <span>last: {formatTime(store.scanStatus.lastEventAt)}</span>
        {/if}
      {:else}
        <span>No scan data</span>
      {/if}
    </div>
    <button
      onclick={handleScan}
      disabled={store.isScanning}
      class="px-3 py-1 text-xs rounded bg-zinc-800 hover:bg-zinc-700 text-zinc-300 disabled:opacity-50"
    >
      {store.isScanning ? 'Scanning...' : 'Scan Now'}
    </button>
  </div>

  <!-- Filters -->
  <div class="flex items-center gap-2 px-4 py-2 border-b border-zinc-800">
    <!-- Time window -->
    <div class="flex rounded overflow-hidden border border-zinc-700">
      {#each ['24h', '48h', '7d'] as tw}
        <button
          onclick={() => handleTimeWindowChange(tw as '24h' | '48h' | '7d')}
          class="px-2 py-0.5 text-xs {store.eventFilter.timeWindow === tw ? 'bg-zinc-700 text-white' : 'text-zinc-400 hover:text-zinc-300'}"
        >{tw}</button>
      {/each}
    </div>

    <!-- Severity -->
    <div class="flex gap-1">
      {#each ['all', 'high', 'medium', 'low'] as sev}
        <button
          onclick={() => handleSeverityChange(sev as 'all' | 'high' | 'medium' | 'low')}
          class="px-2 py-0.5 text-xs rounded border {store.eventFilter.severity === sev ? 'border-zinc-500 text-white' : 'border-zinc-700 text-zinc-500 hover:text-zinc-400'}"
        >{sev}</button>
      {/each}
    </div>

    <!-- Search -->
    <input
      type="text"
      placeholder="Search..."
      value={store.eventFilter.search}
      oninput={handleSearchInput}
      class="ml-auto px-2 py-0.5 text-xs bg-zinc-900 border border-zinc-700 rounded text-zinc-300 placeholder-zinc-600 w-40"
    />
  </div>

  <!-- Event list -->
  <div class="flex-1 overflow-y-auto">
    {#if store.filteredEvents.length === 0}
      <div class="flex items-center justify-center h-full text-zinc-500 text-sm">
        No events found
      </div>
    {:else}
      {#each store.filteredEvents as event (event.id)}
        <div class="px-4 py-2 border-b border-zinc-800/50 hover:bg-zinc-800/30 transition-colors">
          <div class="flex items-start gap-2">
            <!-- Severity badge -->
            <span class="px-1.5 py-0.5 text-[10px] rounded border {severityColor(event.severity)}">
              {event.severity.toUpperCase()}
            </span>

            <!-- Content -->
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2">
                <span class="text-sm text-zinc-200 truncate">{event.title}</span>
                <span class="text-[10px] {stanceColor(event.stance)}">{event.stance}</span>
              </div>
              {#if event.body && event.body !== event.title}
                <p class="text-xs text-zinc-500 mt-0.5 line-clamp-2">{event.body}</p>
              {/if}
              <div class="flex items-center gap-2 mt-1">
                <span class="text-[10px] text-zinc-600">{event.source}</span>
                <span class="text-[10px] text-zinc-600">{formatTime(event.createdAt)}</span>
                {#if event.symbols}
                  <div class="flex gap-1">
                    {#each event.symbols.split(',').filter(Boolean) as sym}
                      <span class="px-1 py-0 text-[10px] bg-zinc-800 rounded text-zinc-400">{sym}</span>
                    {/each}
                  </div>
                {/if}
              </div>
            </div>

            <!-- Trigger button -->
            {#if event.severity === 'high' && !event.triggered}
              <button
                onclick={() => {/* Task 10: trigger dialog */}}
                class="px-2 py-1 text-xs bg-amber-600/20 hover:bg-amber-600/30 text-amber-400 rounded"
              >
                Trigger
              </button>
            {:else if event.triggered}
              <span class="text-[10px] text-zinc-600">Triggered</span>
            {/if}
          </div>
        </div>
      {/each}
    {/if}
  </div>
</div>
