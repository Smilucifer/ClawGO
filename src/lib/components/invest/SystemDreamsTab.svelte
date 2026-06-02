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

<div class="space-y-[var(--space-3)]">
  <h3 class="text-[13px] font-medium text-[var(--text-primary)]">{t('invest_system_dreams_title')}</h3>

  {#if loading}
    <p class="text-[13px] text-[var(--text-secondary)]">{t('invest_loading')}</p>
  {:else if error}
    <p class="text-[13px] text-[var(--color-error)]">{error}</p>
  {:else if traces.length === 0}
    <p class="text-[13px] text-[var(--text-secondary)]">{t('invest_system_dreams_empty')}</p>
  {:else}
    <div class="space-y-[var(--space-2)]">
      {#each traces as trace}
        <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-3)]">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-[var(--space-2)]">
              <span class="rounded-[var(--radius-md)] bg-[var(--bg-input)] px-2 py-0.5 text-[11px] text-[var(--text-secondary)]">{trace.dreamType}</span>
              <span class="text-[11px] text-[var(--text-tertiary)]">{trace.triggerType}</span>
              <span class="rounded-[var(--radius-full)] px-1.5 py-0.5 text-[11px] font-bold {trace.status === 'completed' ? 'bg-[rgba(138,154,118,0.15)] text-[var(--color-success)]' : trace.status === 'failed' ? 'bg-[rgba(168,122,122,0.15)] text-[var(--color-error)]' : 'bg-[var(--bg-input)] text-[var(--text-secondary)]'}">{trace.status}</span>
              {#if trace.rollbackReady}
                <span class="rounded-[var(--radius-full)] bg-[rgba(184,154,106,0.15)] px-1.5 py-0.5 text-[11px] font-bold text-[var(--color-warning)]">rollback</span>
              {/if}
            </div>
            <div class="flex items-center gap-[var(--space-2)]">
              <span class="text-[11px] text-[var(--text-tertiary)]">{trace.createdAt.slice(0, 16)}</span>
              <button
                class="rounded-[var(--radius-md)] px-1.5 py-0.5 text-[11px] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors"
                onclick={() => toggleExpand(trace.id)}
              >
                {expandedId === trace.id ? '▲' : '▼'}
              </button>
            </div>
          </div>

          {#if trace.summary && expandedId !== trace.id}
            <p class="mt-1 text-[12px] text-[var(--text-tertiary)]">{truncate(trace.summary)}</p>
          {/if}

          {#if expandedId === trace.id}
            <div class="mt-2 space-y-[var(--space-2)] border-t border-border/50 pt-[var(--space-2)]">
              {#if trace.summary}
                <div>
                  <span class="text-[12px] font-medium text-[var(--text-secondary)]">Summary:</span>
                  <p class="whitespace-pre-wrap text-[12px] text-[var(--text-primary)]">{trace.summary}</p>
                </div>
              {/if}
              {#if trace.beforeJson}
                <div>
                  <span class="text-[12px] font-medium text-[var(--text-secondary)]">Before:</span>
                  <pre class="mt-1 max-h-40 overflow-auto rounded-[var(--radius-md)] bg-[var(--bg-input)] p-[var(--space-2)] text-[11px] font-[var(--font-mono)] text-[var(--text-primary)]">{truncate(trace.beforeJson, 500)}</pre>
                </div>
              {/if}
              {#if trace.afterJson}
                <div>
                  <span class="text-[12px] font-medium text-[var(--text-secondary)]">After:</span>
                  <pre class="mt-1 max-h-40 overflow-auto rounded-[var(--radius-md)] bg-[var(--bg-input)] p-[var(--space-2)] text-[11px] font-[var(--font-mono)] text-[var(--text-primary)]">{truncate(trace.afterJson, 500)}</pre>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
