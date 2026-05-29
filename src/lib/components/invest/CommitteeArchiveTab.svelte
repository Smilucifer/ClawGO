<script lang="ts">
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import type { ArchivedDecision } from '$lib/stores/invest-committee-store.svelte';

  let symbol = $state('');
  let days = $state(14);
  let loading = $state(false);
  let archives = $state<ArchivedDecision[]>([]);
  let selectedDate = $state<string | null>(null);

  const selected = $derived(archives.find((a) => a.date === selectedDate) ?? null);

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
        <input
          type="text"
          class="w-full rounded border border-border bg-background px-3 py-1.5 text-sm"
          placeholder="股票代码"
          bind:value={symbol}
        />
      </div>
      <button
        class="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
        disabled={loading || !symbol.trim()}
        onclick={load}
      >
        {loading ? '...' : '查询'}
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
            <span class="text-xs text-muted-foreground">{archive.symbol}</span>
          </button>
        {/each}
      </div>
    {:else}
      <div class="py-4 text-center text-xs text-muted-foreground">
        输入代码查询归档
      </div>
    {/if}
  </div>

  <!-- Right panel: detail -->
  <div class="flex-1 overflow-auto">
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
    {:else}
      <div class="flex h-full items-center justify-center text-sm text-muted-foreground">
        选择左侧记录查看详情
      </div>
    {/if}
  </div>
</div>
