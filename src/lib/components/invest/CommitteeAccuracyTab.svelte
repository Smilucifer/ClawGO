<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';
  import { investStore } from '$lib/stores/invest-store.svelte';

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  interface WindowStats {
    windowDays: number;
    sampleCount: number;
    hitRate: number;
  }
  interface VerdictStats {
    verdictType: string;
    sampleCount: number;
    avgConfidence: number;
    hitRate1d: number | null;
    hitRate7d: number | null;
    hitRate30d: number | null;
  }
  interface ReviewSummary {
    totalVerdicts: number;
    overallHitRate: number;
    directionalHitRate: number;
    byWindow: WindowStats[];
    byVerdict: VerdictStats[];
    lastReviewAt?: string;
  }
  interface ReviewEntry {
    id: number;
    verdictId: string;
    symbol: string;
    verdictType: string;
    verdictDate: string;
    windowDays: number;
    priceAtVerdict?: number;
    priceAfter?: number;
    returnPct?: number;
    hit: boolean;
    flatThreshold?: number;
  }

  /** Use centralized nameMap from store (enriched from holdings + price cache + trades) */
  const nameMap = $derived(investStore.nameMap);

  let summary = $state<ReviewSummary | null>(null);
  let detail = $state<ReviewEntry[]>([]);
  let loading = $state(false);
  let showDetail = $state(false);
  let error = $state<string | null>(null);
  let reviewing = $state(false);
  let reviewResult = $state<string | null>(null);

  async function loadSummary() {
    loading = true;
    try {
      summary = await invoke<ReviewSummary>('get_verdict_review_summary');
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function loadDetail() {
    showDetail = !showDetail;
    if (showDetail && detail.length === 0) {
      detail = await invoke<ReviewEntry[]>('get_verdict_review_detail', {});
    }
  }

  async function runReview() {
    reviewing = true;
    reviewResult = null;
    error = null;
    try {
      const settings = await invoke<{ tushare_token?: string }>('get_user_settings');
      const tushareToken = settings.tushare_token ?? '';
      if (!tushareToken) {
        reviewResult = 'error';
        error = 'Tushare token not configured';
        return;
      }
      await invoke('run_verdict_review_cmd', { tushareToken });
      reviewResult = 'success';
      await loadSummary();
    } catch (e) {
      reviewResult = 'error';
      error = String(e);
    } finally {
      reviewing = false;
    }
  }

  function pct(v: number | null | undefined): string {
    if (v == null) return '-';
    return `${(v * 100).toFixed(1)}%`;
  }

  function hitColor(v: number | null | undefined): string {
    if (v == null) return 'text-[var(--text-tertiary)]';
    if (v >= 0.6) return 'text-[#8a9a76]';
    if (v >= 0.4) return 'text-[#b89a6a]';
    return 'text-[#a87a7a]';
  }

  $effect(() => { loadSummary(); });
</script>

<div class="flex flex-col gap-[var(--space-4)]">
  <div class="flex items-center justify-between">
    <h3 class="text-[16px] font-semibold text-[var(--text-primary)]">{t('invest_accuracy_title')}</h3>
    <div class="flex items-center gap-[var(--space-2)]">
      {#if reviewResult === 'success'}
        <span class="text-[12px] text-[#8a9a76]">{t('invest_done')}</span>
      {/if}
      <button
        class="rounded-[var(--radius-md)] bg-[var(--accent)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] text-[var(--bg-base)] disabled:opacity-50"
        disabled={reviewing}
        onclick={runReview}
      >
        {reviewing ? '...' : t('invest_accuracy_run_review')}
      </button>
    </div>
  </div>

  {#if error}
    <div class="rounded-[var(--radius-md)] border border-[#a87a7a]/30 bg-[#a87a7a]/10 p-[var(--space-3)] text-[13px] text-[#a87a7a]">
      {error}
    </div>
  {/if}

  {#if loading}
    <p class="text-[13px] text-[var(--text-secondary)]">{t('common_loading')}</p>
  {:else if !summary || summary.totalVerdicts === 0}
    <div class="flex h-32 items-center justify-center">
      <p class="text-[13px] text-[var(--text-secondary)]">{t('invest_accuracy_auto_tracking')}</p>
    </div>
  {:else}
    <!-- KPI Cards -->
    <div class="grid grid-cols-3 gap-[var(--space-4)]">
      <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] p-[var(--space-4)] text-center">
        <div class="text-[24px] font-bold text-[var(--text-primary)] font-[var(--font-mono)]">{summary.totalVerdicts}</div>
        <div class="text-[11px] text-[var(--text-tertiary)]">{t('invest_accuracy_total_verdicts')}</div>
      </div>
      <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] p-[var(--space-4)] text-center">
        <div class="text-[24px] font-bold font-[var(--font-mono)] {hitColor(summary.overallHitRate)}">{pct(summary.overallHitRate)}</div>
        <div class="text-[11px] text-[var(--text-tertiary)]">{t('invest_accuracy_overall_hit_rate')}</div>
      </div>
      <div class="rounded-[var(--radius-lg)] border border-[var(--accent)]/30 bg-[var(--bg-card)] p-[var(--space-4)] text-center">
        <div class="text-[24px] font-bold font-[var(--font-mono)] {hitColor(summary.directionalHitRate)}">{pct(summary.directionalHitRate)}</div>
        <div class="text-[11px] text-[var(--text-tertiary)]">{t('invest_accuracy_directional_hit_rate')}</div>
      </div>
    </div>

    <!-- Honesty banner -->
    {#if summary.directionalHitRate < 0.5}
      <div class="rounded-[var(--radius-md)] border border-[#b89a6a]/30 bg-[#b89a6a]/10 p-[var(--space-3)] text-[13px] text-[#b89a6a]">
        {t('invest_accuracy_honesty_banner')}
      </div>
    {/if}

    <!-- By Window -->
    <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)]">
      <div class="border-b border-[var(--border)] bg-[var(--bg-hover)] px-[var(--space-4)] py-[var(--space-2)] text-[13px] font-medium text-[var(--text-secondary)]">{t('invest_accuracy_by_window')}</div>
      <div class="p-[var(--space-4)] flex flex-col gap-[var(--space-3)]">
        {#each [...summary.byWindow].sort((a, b) => a.windowDays - b.windowDays) as w}
          <div class="flex items-center gap-[var(--space-3)]">
            <span class="w-12 text-[13px] font-medium text-[var(--text-secondary)]">{w.windowDays}d</span>
            <div class="flex-1">
              <div class="h-3 rounded-full bg-[var(--bg-input)] overflow-hidden">
                <div class="h-full rounded-full bg-[var(--accent)] transition-all" style="width: {w.hitRate * 100}%"></div>
              </div>
            </div>
            <span class="w-16 text-right text-[13px] font-[var(--font-mono)] {hitColor(w.hitRate)}">{pct(w.hitRate)}</span>
            <span class="w-16 text-right text-[11px] text-[var(--text-tertiary)]">{w.sampleCount} {t('invest_accuracy_samples')}</span>
          </div>
        {/each}
      </div>
    </div>

    <!-- By Verdict Type -->
    <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)]">
      <div class="border-b border-[var(--border)] bg-[var(--bg-hover)] px-[var(--space-4)] py-[var(--space-2)] text-[13px] font-medium text-[var(--text-secondary)]">{t('invest_accuracy_by_verdict')}</div>
      <table class="w-full text-[13px]">
        <thead>
          <tr class="border-b border-[var(--border)] text-left text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">
            <th class="px-[var(--space-4)] py-[var(--space-2)]">{t('invest_accuracy_type')}</th>
            <th class="px-[var(--space-4)] py-[var(--space-2)] text-right">{t('invest_accuracy_count')}</th>
            <th class="px-[var(--space-4)] py-[var(--space-2)] text-right">1d</th>
            <th class="px-[var(--space-4)] py-[var(--space-2)] text-right">7d</th>
            <th class="px-[var(--space-4)] py-[var(--space-2)] text-right">30d</th>
          </tr>
        </thead>
        <tbody>
          {#each [...summary.byVerdict].sort((a, b) => b.sampleCount - a.sampleCount) as v}
            <tr class="border-b border-[var(--border)] last:border-0">
              <td class="px-[var(--space-4)] py-[var(--space-2)] font-medium text-[var(--text-primary)]">{v.verdictType}</td>
              <td class="px-[var(--space-4)] py-[var(--space-2)] text-right font-[var(--font-mono)]">{v.sampleCount}</td>
              <td class="px-[var(--space-4)] py-[var(--space-2)] text-right font-[var(--font-mono)] {hitColor(v.hitRate1d)}">{pct(v.hitRate1d)}</td>
              <td class="px-[var(--space-4)] py-[var(--space-2)] text-right font-[var(--font-mono)] {hitColor(v.hitRate7d)}">{pct(v.hitRate7d)}</td>
              <td class="px-[var(--space-4)] py-[var(--space-2)] text-right font-[var(--font-mono)] {hitColor(v.hitRate30d)}">{pct(v.hitRate30d)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <!-- Detail toggle -->
    <button class="text-[13px] text-[var(--text-secondary)] hover:text-[var(--text-primary)]" onclick={loadDetail}>
      {showDetail ? t('invest_accuracy_hide') : t('invest_accuracy_show')} {t('invest_accuracy_detail')} ({detail.length} {t('invest_accuracy_entries')})
    </button>

    {#if showDetail && detail.length > 0}
      <div class="max-h-80 overflow-y-auto rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)]">
        <table class="w-full text-[11px]">
          <thead>
            <tr class="border-b border-[var(--border)] bg-[var(--bg-hover)] text-left text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">
              <th class="px-[var(--space-3)] py-[var(--space-2)]">{t('invest_accuracy_symbol')}</th>
              <th class="px-[var(--space-3)] py-[var(--space-2)]">{t('invest_accuracy_date')}</th>
              <th class="px-[var(--space-3)] py-[var(--space-2)]">{t('invest_accuracy_verdict')}</th>
              <th class="px-[var(--space-3)] py-[var(--space-2)] text-right">{t('invest_accuracy_window')}</th>
              <th class="px-[var(--space-3)] py-[var(--space-2)] text-right">{t('invest_accuracy_returnPct')}</th>
              <th class="px-[var(--space-3)] py-[var(--space-2)] text-center">{t('invest_accuracy_hit')}</th>
            </tr>
          </thead>
          <tbody>
            {#each detail as d}
              <tr class="border-b border-[var(--border)] last:border-0">
                <td class="px-[var(--space-3)] py-[var(--space-2)] text-[var(--text-primary)]" title={d.symbol}>{nameMap.get(d.symbol) ?? d.symbol}</td>
                <td class="px-[var(--space-3)] py-[var(--space-2)]">{d.verdictDate}</td>
                <td class="px-[var(--space-3)] py-[var(--space-2)]">{d.verdictType}</td>
                <td class="px-[var(--space-3)] py-[var(--space-2)] text-right">{d.windowDays}d</td>
                <td class="px-[var(--space-3)] py-[var(--space-2)] text-right font-[var(--font-mono)] {(d.returnPct ?? 0) >= 0 ? 'text-[#8a9a76]' : 'text-[#a87a7a]'}">
                  {d.returnPct != null ? pct(d.returnPct) : '-'}
                </td>
                <td class="px-[var(--space-3)] py-[var(--space-2)] text-center">
                  <span class="inline-block w-5 rounded-[var(--radius-sm)] text-center {d.hit ? 'bg-[#8a9a76]/15 text-[#8a9a76]' : 'bg-[#a87a7a]/15 text-[#a87a7a]'}">
                    {d.hit ? '✓' : '✗'}
                  </span>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}

    {#if summary.lastReviewAt}
      <p class="text-[11px] text-[var(--text-tertiary)]">{t('invest_accuracy_lastReview')} {new Date(summary.lastReviewAt).toLocaleString()}</p>
    {/if}
  {/if}
</div>
