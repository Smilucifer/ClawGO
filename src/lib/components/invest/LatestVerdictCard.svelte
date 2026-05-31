<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';

  const latestVerdict = $derived(investStore.verdicts[0] ?? null);

  const verdictColor = $derived(
    latestVerdict?.verdict === 'BUY' || latestVerdict?.verdict === 'ACCUMULATE'
      ? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400'
      : latestVerdict?.verdict === 'TRIM' || latestVerdict?.verdict === 'SELL'
        ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
        : 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400'
  );
</script>

<div class="rounded-lg border border-border p-4">
  <h3 class="mb-3 text-sm font-semibold flex items-center gap-2">
    <span class="h-4 w-0.5 rounded-full bg-primary"></span>
    {t('invest_latest_verdict')}
  </h3>

  {#if latestVerdict}
    <div class="space-y-2">
      <div class="flex items-center gap-2">
        <span class="text-sm font-semibold">{latestVerdict.symbol}</span>
        <span class="inline-block rounded px-2 py-0.5 text-xs font-bold {verdictColor}">
          {latestVerdict.verdict}
        </span>
        <span class="text-xs text-muted-foreground">
          {latestVerdict.confidence ? (latestVerdict.confidence * 100).toFixed(0) + '%' : '-'}
        </span>
      </div>
      {#if latestVerdict.reasoning}
        <p class="text-xs text-muted-foreground line-clamp-3">{latestVerdict.reasoning}</p>
      {/if}
      <div class="flex gap-3 text-[10px] text-muted-foreground/60">
        <span>{latestVerdict.model ?? '-'}</span>
        <span>{latestVerdict.provider ?? '-'}</span>
        <span>{latestVerdict.tokensUsed} tok</span>
        <span>{latestVerdict.latencyMs ? (latestVerdict.latencyMs / 1000).toFixed(1) + 's' : '-'}</span>
      </div>
    </div>
  {:else}
    <p class="text-xs text-muted-foreground">{t('invest_no_verdicts')}</p>
  {/if}
</div>
