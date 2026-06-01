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
  let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;

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

  async function doSearch() {
    loading = true;
    error = null;
    try {
      insights = await invoke<DomainInsight[]>('search_domain_insights', {
        query: searchQuery,
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

  $effect(() => {
    const q = searchQuery;
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
    if (q) {
      searchDebounceTimer = setTimeout(() => doSearch(), 300);
    } else {
      loadInsights();
    }
    return () => { if (searchDebounceTimer) clearTimeout(searchDebounceTimer); };
  });

  function confidenceColor(c: number | null): string {
    if (c == null) return 'text-[var(--text-tertiary)]';
    if (c >= 0.8) return 'text-[var(--color-success)]';
    if (c >= 0.5) return 'text-[var(--color-warning)]';
    return 'text-[var(--color-error)]';
  }

  function typeBadge(type: string): string {
    const map: Record<string, string> = {
      pattern: 'bg-[rgba(59,130,246,0.10)] text-[#3b82f6]',
      regime: 'bg-[rgba(139,92,246,0.10)] text-[#8b5cf6]',
      sector: 'bg-[rgba(138,154,118,0.10)] text-[var(--color-success)]',
      risk: 'bg-[rgba(168,122,122,0.10)] text-[var(--color-error)]',
      opportunity: 'bg-[rgba(184,154,106,0.10)] text-[var(--color-warning)]',
    };
    return map[type] || 'bg-[var(--bg-input)] text-[var(--text-tertiary)]';
  }

</script>

<div class="space-y-[var(--space-4)]">
  <div class="flex items-center justify-between">
    <h3 class="text-lg font-semibold text-[var(--text-primary)]">{t('invest_insights_title')}</h3>
    <button
      class="rounded-[var(--radius-md)] bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors"
      onclick={loadInsights}
    >
      {t('common_refresh')}
    </button>
  </div>

  <!-- Filters -->
  <div class="flex items-center gap-[var(--space-3)]">
    <div class="flex gap-1 rounded-[var(--radius-md)] border border-[var(--border)] p-0.5">
      <button
        class="rounded-[var(--radius-md)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] transition-colors {statusFilter === 'active'
          ? 'bg-[var(--accent)] text-[#1a1918]'
          : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'}"
        onclick={() => { statusFilter = 'active'; loadInsights(); }}
      >
        {t('invest_insights_active')}
      </button>
      <button
        class="rounded-[var(--radius-md)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] transition-colors {statusFilter === 'archived'
          ? 'bg-[var(--accent)] text-[#1a1918]'
          : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'}"
        onclick={() => { statusFilter = 'archived'; loadInsights(); }}
      >
        {t('invest_insights_archived')}
      </button>
    </div>
    <input
      type="text"
      class="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-sm text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)]"
      placeholder={t('invest_insights_search')}
      bind:value={searchQuery}
    />
  </div>

  {#if error}
    <div class="rounded-[var(--radius-md)] border border-[var(--color-error)]/50 bg-[var(--color-error)]/10 px-[var(--space-3)] py-[var(--space-2)] text-sm text-[var(--color-error)]">
      {error}
    </div>
  {/if}

  {#if loading}
    <p class="text-sm text-[var(--text-secondary)]">{t('invest_scheduler_loading')}</p>
  {:else if insights.length === 0}
    <p class="text-sm text-[var(--text-secondary)]">{t('invest_insights_empty')}</p>
  {:else}
    <div class="space-y-[var(--space-2)]">
      {#each insights as insight}
        <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] p-[var(--space-4)] space-y-[var(--space-2)]">
          <div class="flex items-start justify-between gap-[var(--space-3)]">
            <div class="flex items-center gap-[var(--space-2)]">
              <span class="rounded-full px-3 py-1 text-[11px] font-bold {typeBadge(insight.insightType)}">
                {insight.insightType}
              </span>
              {#if insight.symbol}
                <span class="text-[11px] font-[var(--font-mono)] text-[var(--text-tertiary)]">{insight.symbol}</span>
              {/if}
            </div>
            <div class="flex items-center gap-[var(--space-2)]">
              {#if insight.confidence != null}
                <span class="text-[12px] font-[var(--font-mono)] {confidenceColor(insight.confidence)}">
                  {(insight.confidence * 100).toFixed(0)}%
                </span>
              {/if}
              {#if statusFilter === 'active'}
                <button
                  class="text-[12px] text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
                  onclick={() => archiveInsight(insight.id)}
                >
                  {t('invest_accuracy_hide')}
                </button>
              {:else}
                <button
                  class="text-[12px] text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
                  onclick={() => unarchiveInsight(insight.id)}
                >
                  {t('invest_accuracy_show')}
                </button>
              {/if}
            </div>
          </div>
          <p class="text-sm leading-relaxed text-[var(--text-primary)]">{insight.content}</p>
          <div class="text-[11px] text-[var(--text-tertiary)]">
            {new Date(insight.createdAt).toLocaleString()}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
