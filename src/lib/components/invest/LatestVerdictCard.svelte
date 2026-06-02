<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';

  const latestVerdict = $derived(investStore.verdicts[0] ?? null);

  const verdictBadgeClass = $derived.by(() => {
    const v = latestVerdict?.verdict;
    if (v === 'BUY') return 'bg-[rgba(138,154,118,0.2)] text-[#8a9a76]';
    if (v === 'ACCUMULATE') return 'bg-[rgba(59,130,246,0.2)] text-[#3b82f6]';
    if (v === 'HOLD') return 'bg-[var(--accent-muted)] text-[var(--accent)]';
    if (v === 'TRIM') return 'bg-[rgba(245,158,11,0.2)] text-[#f59e0b]';
    if (v === 'SELL') return 'bg-[rgba(168,122,122,0.2)] text-[#a87a7a]';
    return 'bg-[var(--bg-input)] text-[var(--text-tertiary)]';
  });

  function verdictLabel(verdict: string): string {
    const map: Record<string, string> = {
      'BUY': t('invest_verdict_buy'),
      'ACCUMULATE': t('invest_verdict_accumulate'),
      'HOLD': t('invest_verdict_hold'),
      'TRIM': t('invest_verdict_trim'),
      'SELL': t('invest_verdict_sell'),
    };
    return map[verdict] || verdict;
  }
</script>

<div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
  <div class="mb-[var(--space-3)] flex items-center justify-between">
    <h3 class="text-[14px] font-semibold text-[var(--text-primary)]">🏛️ {t('invest_latest_verdict')}</h3>
    {#if latestVerdict}
      <span class="rounded-[var(--radius-full)] px-3 py-1 text-[11px] font-semibold {verdictBadgeClass}">
        {verdictLabel(latestVerdict.verdict)}
      </span>
    {/if}
  </div>

  {#if latestVerdict}
    <div class="space-y-[var(--space-2)]">
      <div class="text-[13px] font-semibold text-[var(--text-primary)]">
        {latestVerdict.name || latestVerdict.symbol}
        <span class="ml-1 font-[var(--font-mono)] text-[12px] text-[var(--text-tertiary)]">{latestVerdict.symbol}</span>
      </div>
      {#if latestVerdict.reasoning}
        <p class="text-[12px] leading-[1.6] text-[var(--text-secondary)]" style="display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden;">
          {latestVerdict.reasoning}
        </p>
      {/if}
      <div class="flex gap-[var(--space-3)] text-[11px] text-[var(--text-tertiary)]">
        <span>{t('invest_macro_signal')}: {latestVerdict.confidence ? (latestVerdict.confidence * 100).toFixed(0) + '%' : '-'}</span>
        <span>{latestVerdict.model ?? '-'}</span>
        <span>{latestVerdict.latencyMs ? (latestVerdict.latencyMs / 1000).toFixed(1) + 's' : '-'}</span>
      </div>
    </div>
  {:else}
    <p class="text-[12px] text-[var(--text-tertiary)]">{t('invest_no_verdicts')}</p>
  {/if}
</div>
