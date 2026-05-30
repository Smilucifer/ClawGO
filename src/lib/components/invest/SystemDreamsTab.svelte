<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';

  interface DreamSnapshot {
    id: number;
    dreamType: string;
    triggerType: string;
    beforeJson: string;
    afterJson: string;
    status: string;
    summary: string | null;
    rollbackReady: boolean;
    createdAt: string;
  }

  let traces: DreamSnapshot[] = $state([]);
  let loading = $state(true);
  let error = $state('');
  let expandedId = $state<number | null>(null);

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  onMount(async () => {
    try {
      traces = await invoke<DreamSnapshot[]>('list_dream_traces', { limit: 30 });
    } catch (e) {
      console.error('[SystemDreamsTab] load error:', e);
      error = String(e);
    } finally {
      loading = false;
    }
  });

  function toggleExpand(id: number) {
    expandedId = expandedId === id ? null : id;
  }

  function truncate(s: string | null, max = 120): string {
    if (!s) return '-';
    return s.length > max ? s.slice(0, max) + '...' : s;
  }
</script>

<div class="space-y-3">
  <h3 class="text-sm font-medium">{t('invest_system_dreams_title')}</h3>

  {#if loading}
    <p class="text-sm text-muted-foreground">{t('invest_loading')}</p>
  {:else if error}
    <p class="text-sm text-red-400">{error}</p>
  {:else if traces.length === 0}
    <p class="text-sm text-muted-foreground">{t('invest_system_dreams_empty')}</p>
  {:else}
    <div class="space-y-2">
      {#each traces as trace}
        <div class="rounded border border-border p-3">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-2">
              <span class="rounded bg-muted px-2 py-0.5 text-xs">{trace.dreamType}</span>
              <span class="text-xs text-muted-foreground">{trace.triggerType}</span>
              <span class="rounded px-1.5 py-0.5 text-xs {trace.status === 'completed' ? 'bg-green-500/10 text-green-400' : trace.status === 'failed' ? 'bg-red-500/10 text-red-400' : 'bg-secondary'}">{trace.status}</span>
              {#if trace.rollbackReady}
                <span class="rounded bg-yellow-500/10 px-1.5 py-0.5 text-xs text-yellow-400">rollback</span>
              {/if}
            </div>
            <div class="flex items-center gap-2">
              <span class="text-xs text-muted-foreground">{trace.createdAt.slice(0, 16)}</span>
              <button
                class="rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-muted"
                onclick={() => toggleExpand(trace.id)}
              >
                {expandedId === trace.id ? '▲' : '▼'}
              </button>
            </div>
          </div>

          {#if trace.summary && expandedId !== trace.id}
            <p class="mt-1 text-xs text-muted-foreground">{truncate(trace.summary)}</p>
          {/if}

          {#if expandedId === trace.id}
            <div class="mt-2 space-y-2 border-t border-border/50 pt-2">
              {#if trace.summary}
                <div>
                  <span class="text-xs font-medium">Summary:</span>
                  <p class="whitespace-pre-wrap text-xs">{trace.summary}</p>
                </div>
              {/if}
              {#if trace.beforeJson}
                <div>
                  <span class="text-xs font-medium">Before:</span>
                  <pre class="mt-1 max-h-40 overflow-auto rounded bg-muted p-2 text-xs">{truncate(trace.beforeJson, 500)}</pre>
                </div>
              {/if}
              {#if trace.afterJson}
                <div>
                  <span class="text-xs font-medium">After:</span>
                  <pre class="mt-1 max-h-40 overflow-auto rounded bg-muted p-2 text-xs">{truncate(trace.afterJson, 500)}</pre>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
