<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';
  import type { PnlSnapshot } from '$lib/types';

  let snapshots: PnlSnapshot[] = $state([]);
  let loading = $state(true);
  let error = $state('');

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  onMount(async () => {
    try {
      snapshots = await invoke<PnlSnapshot[]>('get_pnl_snapshots', { limit: 80 });
    } catch (e) {
      console.error('[SystemPnlHistoryTab] load error:', e);
      error = String(e);
    } finally {
      loading = false;
    }
  });

  function formatDate(d: string): string {
    return d.length > 10 ? d.slice(0, 10) : d;
  }

  function fmtPnl(v: number | null): string {
    if (v == null || !Number.isFinite(v)) return '-';
    const prefix = v >= 0 ? '+' : '';
    return `${prefix}¥${v.toLocaleString(undefined, { minimumFractionDigits: 2 })}`;
  }

  function fmtPct(v: number | null): string {
    if (v == null || !Number.isFinite(v)) return '-';
    const prefix = v >= 0 ? '+' : '';
    return `${prefix}${v.toFixed(2)}%`;
  }
</script>

<div class="space-y-3">
  <h3 class="text-sm font-medium">{t('invest_system_pnl_history_title')}</h3>

  {#if loading}
    <p class="text-sm text-muted-foreground">{t('invest_loading')}</p>
  {:else if error}
    <p class="text-sm text-red-400">{error}</p>
  {:else if snapshots.length === 0}
    <p class="text-sm text-muted-foreground">{t('invest_system_pnl_history_empty')}</p>
  {:else}
    <div class="overflow-x-auto">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b border-border text-left text-muted-foreground">
            <th class="pb-2 pr-3">{t('invest_system_pnl_date')}</th>
            <th class="pb-2 pr-3 text-right">{t('invest_system_pnl_total')}</th>
            <th class="pb-2 pr-3 text-right">{t('invest_system_pnl_cash')}</th>
            <th class="pb-2 pr-3 text-right">{t('invest_system_pnl_holdings')}</th>
            <th class="pb-2 pr-3 text-right">{t('invest_system_pnl_daily')}</th>
            <th class="pb-2 text-right">{t('invest_system_pnl_daily_pct')}</th>
          </tr>
        </thead>
        <tbody>
          {#each snapshots as snap}
            <tr class="border-b border-border/50">
              <td class="py-1.5 pr-3">{formatDate(snap.snapshotDate)}</td>
              <td class="py-1.5 pr-3 text-right">¥{snap.totalValue.toLocaleString(undefined, { minimumFractionDigits: 2 })}</td>
              <td class="py-1.5 pr-3 text-right">¥{snap.cash.toLocaleString(undefined, { minimumFractionDigits: 2 })}</td>
              <td class="py-1.5 pr-3 text-right">¥{snap.holdingsValue.toLocaleString(undefined, { minimumFractionDigits: 2 })}</td>
              <td class="py-1.5 pr-3 text-right" class:text-green-400={(snap.dailyPnl ?? 0) > 0} class:text-red-400={(snap.dailyPnl ?? 0) < 0}>
                {fmtPnl(snap.dailyPnl)}
              </td>
              <td class="py-1.5 text-right" class:text-green-400={(snap.dailyPnlPct ?? 0) > 0} class:text-red-400={(snap.dailyPnlPct ?? 0) < 0}>
                {fmtPct(snap.dailyPnlPct)}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>
