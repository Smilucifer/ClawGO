<script lang="ts">
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import type { ArchivedDecision } from '$lib/stores/invest-committee-store.svelte';

  let symbol = $state('');
  let days = $state(7);
  let loading = $state(false);
  let archives = $state<ArchivedDecision[]>([]);
  let selectedIndex = $state(0);

  const selected = $derived(archives[selectedIndex] ?? null);

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
</script>

<div class="space-y-4">
  <div class="flex items-end gap-2">
    <div class="flex-1">
      <label class="mb-1 block text-sm text-muted-foreground">股票代码</label>
      <input
        type="text"
        class="w-full rounded border border-border bg-background px-3 py-1.5 text-sm"
        placeholder="600519.SH"
        bind:value={symbol}
      />
    </div>
    <div>
      <label class="mb-1 block text-sm text-muted-foreground">天数</label>
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
      {loading ? '加载中...' : '加载'}
    </button>
  </div>

  {#if archives.length === 0 && !loading}
    <div class="py-8 text-center text-sm text-muted-foreground">
      输入股票代码查看历史决策
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
