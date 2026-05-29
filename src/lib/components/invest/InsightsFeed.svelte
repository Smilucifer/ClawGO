<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';
  import type { DomainInsight } from '$lib/types';

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  let insights = $state<DomainInsight[]>([]);
  let loading = $state(false);
  let searchQuery = $state('');
  let statusFilter = $state<'active' | 'archived'>('active');
  let error = $state<string | null>(null);

  async function loadInsights() {
    loading = true;
    error = null;
    try {
      insights = await invoke<DomainInsight[]>('list_insights', {
        status: statusFilter,
        symbol: null,
        limit: 50,
      });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function archiveInsight(id: string) {
    try {
      await invoke('archive_insight', { id });
      insights = insights.filter((i) => i.id !== id);
    } catch (e) {
      error = String(e);
    }
  }

  async function unarchiveInsight(id: string) {
    try {
      await invoke('unarchive_insight', { id });
      await loadInsights();
    } catch (e) {
      error = String(e);
    }
  }

  const filteredInsights = $derived(
    searchQuery
      ? insights.filter(
          (i) =>
            i.content.toLowerCase().includes(searchQuery.toLowerCase()) ||
            (i.symbol && i.symbol.toLowerCase().includes(searchQuery.toLowerCase())) ||
            i.insightType.toLowerCase().includes(searchQuery.toLowerCase()),
        )
      : insights,
  );

  function confidenceColor(c: number | null): string {
    if (c == null) return 'text-muted-foreground';
    if (c >= 0.8) return 'text-green-500';
    if (c >= 0.5) return 'text-amber-500';
    return 'text-red-400';
  }

  function typeBadge(type: string): string {
    const map: Record<string, string> = {
      pattern: 'bg-blue-500/10 text-blue-400',
      regime: 'bg-purple-500/10 text-purple-400',
      sector: 'bg-emerald-500/10 text-emerald-400',
      risk: 'bg-red-500/10 text-red-400',
      opportunity: 'bg-amber-500/10 text-amber-400',
    };
    return map[type] || 'bg-muted text-muted-foreground';
  }

  $effect(() => {
    loadInsights();
  });
</script>

<div class="space-y-4">
  <div class="flex items-center justify-between">
    <h3 class="text-lg font-semibold">{t('invest_insights_title')}</h3>
    <button
      class="rounded bg-muted px-3 py-1.5 text-xs hover:bg-muted/80"
      onclick={loadInsights}
    >
      {t('common_refresh')}
    </button>
  </div>

  <!-- Filters -->
  <div class="flex items-center gap-3">
    <div class="flex gap-1 rounded-md border p-0.5">
      <button
        class="rounded px-2.5 py-1 text-xs transition-colors {statusFilter === 'active'
          ? 'bg-primary text-primary-foreground'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => { statusFilter = 'active'; loadInsights(); }}
      >
        {t('invest_insights_active')}
      </button>
      <button
        class="rounded px-2.5 py-1 text-xs transition-colors {statusFilter === 'archived'
          ? 'bg-primary text-primary-foreground'
          : 'text-muted-foreground hover:text-foreground'}"
        onclick={() => { statusFilter = 'archived'; loadInsights(); }}
      >
        {t('invest_insights_archived')}
      </button>
    </div>
    <input
      type="text"
      class="rounded-md border bg-background px-3 py-1.5 text-sm"
      placeholder={t('invest_insights_search')}
      bind:value={searchQuery}
    />
  </div>

  {#if error}
    <div class="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      {error}
    </div>
  {/if}

  {#if loading}
    <p class="text-sm text-muted-foreground">{t('invest_scheduler_loading')}</p>
  {:else if filteredInsights.length === 0}
    <p class="text-sm text-muted-foreground">{t('invest_insights_empty')}</p>
  {:else}
    <div class="space-y-2">
      {#each filteredInsights as insight}
        <div class="rounded-lg border p-4 space-y-2">
          <div class="flex items-start justify-between gap-3">
            <div class="flex items-center gap-2">
              <span class="rounded px-2 py-0.5 text-xs font-medium {typeBadge(insight.insightType)}">
                {insight.insightType}
              </span>
              {#if insight.symbol}
                <span class="text-xs font-mono text-muted-foreground">{insight.symbol}</span>
              {/if}
            </div>
            <div class="flex items-center gap-2">
              {#if insight.confidence != null}
                <span class="text-xs {confidenceColor(insight.confidence)}">
                  {(insight.confidence * 100).toFixed(0)}%
                </span>
              {/if}
              {#if statusFilter === 'active'}
                <button
                  class="text-xs text-muted-foreground hover:text-foreground"
                  onclick={() => archiveInsight(insight.id)}
                >
                  {t('invest_accuracy_hide')}
                </button>
              {:else}
                <button
                  class="text-xs text-muted-foreground hover:text-foreground"
                  onclick={() => unarchiveInsight(insight.id)}
                >
                  {t('invest_accuracy_show')}
                </button>
              {/if}
            </div>
          </div>
          <p class="text-sm leading-relaxed">{insight.content}</p>
          <div class="text-xs text-muted-foreground">
            {new Date(insight.createdAt).toLocaleString()}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
