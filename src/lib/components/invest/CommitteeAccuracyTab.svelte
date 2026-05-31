<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';

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

  let summary = $state<ReviewSummary | null>(null);
  let detail = $state<ReviewEntry[]>([]);
  let loading = $state(false);
  let showDetail = $state(false);
  let error = $state<string | null>(null);

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

  function pct(v: number | null | undefined): string {
    if (v == null) return '-';
    return `${(v * 100).toFixed(1)}%`;
  }

  function hitColor(v: number | null | undefined): string {
    if (v == null) return 'text-muted-foreground';
    if (v >= 0.6) return 'text-green-500';
    if (v >= 0.4) return 'text-amber-500';
    return 'text-red-500';
  }

  $effect(() => { loadSummary(); });
</script>

<div class="space-y-4">
  <div class="flex items-center justify-between">
    <h3 class="text-lg font-semibold">{t('invest_accuracy_title')}</h3>
  </div>

  {#if error}
    <div class="rounded border border-red-300 bg-red-50 p-3 text-sm text-red-700 dark:bg-red-950 dark:text-red-300">
      {error}
    </div>
  {/if}

  {#if loading}
    <p class="text-muted-foreground">{t('common_loading')}</p>
  {:else if !summary || summary.totalVerdicts === 0}
    <div class="flex h-32 items-center justify-center">
      <p class="text-muted-foreground">{t('invest_accuracy_auto_tracking')}</p>
    </div>
  {:else}
    <!-- KPI Cards -->
    <div class="grid grid-cols-3 gap-4">
      <div class="rounded-lg border p-4 text-center">
        <div class="text-2xl font-bold">{summary.totalVerdicts}</div>
        <div class="text-xs text-muted-foreground">{t('invest_accuracy_total_verdicts')}</div>
      </div>
      <div class="rounded-lg border p-4 text-center">
        <div class="text-2xl font-bold {hitColor(summary.overallHitRate)}">{pct(summary.overallHitRate)}</div>
        <div class="text-xs text-muted-foreground">{t('invest_accuracy_overall_hit_rate')}</div>
      </div>
      <div class="rounded-lg border p-4 text-center border-primary/30">
        <div class="text-2xl font-bold {hitColor(summary.directionalHitRate)}">{pct(summary.directionalHitRate)}</div>
        <div class="text-xs text-muted-foreground">{t('invest_accuracy_directional_hit_rate')}</div>
      </div>
    </div>

    <!-- Honesty banner -->
    {#if summary.directionalHitRate < 0.5}
      <div class="rounded border border-yellow-300 bg-yellow-50 p-3 text-sm text-yellow-800 dark:bg-yellow-950 dark:text-yellow-200">
        {t('invest_accuracy_honesty_banner')}
      </div>
    {/if}

    <!-- By Window -->
    <div class="rounded-lg border">
      <div class="border-b bg-muted/50 px-4 py-2 text-sm font-medium">{t('invest_accuracy_by_window')}</div>
      <div class="p-4 space-y-3">
        {#each [...summary.byWindow].sort((a, b) => a.windowDays - b.windowDays) as w}
          <div class="flex items-center gap-3">
            <span class="w-12 text-sm font-medium">{w.windowDays}d</span>
            <div class="flex-1">
              <div class="h-3 rounded-full bg-muted overflow-hidden">
                <div class="h-full rounded-full bg-primary transition-all" style="width: {w.hitRate * 100}%"></div>
              </div>
            </div>
            <span class="w-16 text-right text-sm font-mono {hitColor(w.hitRate)}">{pct(w.hitRate)}</span>
            <span class="w-16 text-right text-xs text-muted-foreground">{w.sampleCount} {t('invest_accuracy_samples')}</span>
          </div>
        {/each}
      </div>
    </div>

    <!-- By Verdict Type -->
    <div class="rounded-lg border">
      <div class="border-b bg-muted/50 px-4 py-2 text-sm font-medium">{t('invest_accuracy_by_verdict')}</div>
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b text-left text-xs text-muted-foreground">
            <th class="px-4 py-2">{t('invest_accuracy_type')}</th>
            <th class="px-4 py-2 text-right">{t('invest_accuracy_count')}</th>
            <th class="px-4 py-2 text-right">1d</th>
            <th class="px-4 py-2 text-right">7d</th>
            <th class="px-4 py-2 text-right">30d</th>
          </tr>
        </thead>
        <tbody>
          {#each [...summary.byVerdict].sort((a, b) => b.sampleCount - a.sampleCount) as v}
            <tr class="border-b last:border-0">
              <td class="px-4 py-2 font-medium">{v.verdictType}</td>
              <td class="px-4 py-2 text-right">{v.sampleCount}</td>
              <td class="px-4 py-2 text-right {hitColor(v.hitRate1d)}">{pct(v.hitRate1d)}</td>
              <td class="px-4 py-2 text-right {hitColor(v.hitRate7d)}">{pct(v.hitRate7d)}</td>
              <td class="px-4 py-2 text-right {hitColor(v.hitRate30d)}">{pct(v.hitRate30d)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <!-- Detail toggle -->
    <button class="text-sm text-muted-foreground hover:text-foreground" onclick={loadDetail}>
      {showDetail ? t('invest_accuracy_hide') : t('invest_accuracy_show')} {t('invest_accuracy_detail')} ({detail.length} {t('invest_accuracy_entries')})
    </button>

    {#if showDetail && detail.length > 0}
      <div class="max-h-80 overflow-y-auto rounded-lg border">
        <table class="w-full text-xs">
          <thead>
            <tr class="border-b bg-muted/50 text-left">
              <th class="px-3 py-2">{t('invest_accuracy_symbol')}</th>
              <th class="px-3 py-2">{t('invest_accuracy_date')}</th>
              <th class="px-3 py-2">{t('invest_accuracy_verdict')}</th>
              <th class="px-3 py-2 text-right">{t('invest_accuracy_window')}</th>
              <th class="px-3 py-2 text-right">{t('invest_accuracy_returnPct')}</th>
              <th class="px-3 py-2 text-center">{t('invest_accuracy_hit')}</th>
            </tr>
          </thead>
          <tbody>
            {#each detail as d}
              <tr class="border-b last:border-0">
                <td class="px-3 py-2 font-mono">{d.symbol}</td>
                <td class="px-3 py-2">{d.verdictDate}</td>
                <td class="px-3 py-2">{d.verdictType}</td>
                <td class="px-3 py-2 text-right">{d.windowDays}d</td>
                <td class="px-3 py-2 text-right {(d.returnPct ?? 0) >= 0 ? 'text-green-500' : 'text-red-500'}">
                  {d.returnPct != null ? pct(d.returnPct) : '-'}
                </td>
                <td class="px-3 py-2 text-center">
                  <span class="inline-block w-5 rounded text-center {d.hit ? 'bg-green-100 text-green-700' : 'bg-red-100 text-red-700'}">
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
      <p class="text-xs text-muted-foreground">{t('invest_accuracy_lastReview')} {new Date(summary.lastReviewAt).toLocaleString()}</p>
    {/if}
  {/if}
</div>
