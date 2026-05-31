<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';

  let error = $state('');

  const snapshots = $derived(investStore.pnlSnapshots);
  const loading = $derived(investStore.loading);

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

  async function handleDelete(id: number) {
    if (!confirm(t('invest_system_pnl_delete_confirm'))) return;
    try {
      await investStore.deletePnlSnapshot(id);
    } catch (e) {
      console.error('Failed to delete PnL snapshot:', e);
      error = String(e);
    }
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
            <th class="pb-2 pr-3 text-right">{t('invest_system_pnl_daily_pct')}</th>
            <th class="pb-2">{t('invest_actions')}</th>
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
              <td class="py-1.5 pr-3 text-right" class:text-green-400={(snap.dailyPnlPct ?? 0) > 0} class:text-red-400={(snap.dailyPnlPct ?? 0) < 0}>
                {fmtPct(snap.dailyPnlPct)}
              </td>
              <td class="py-1.5">
                <button
                  class="rounded px-2 py-1 text-xs text-destructive hover:bg-destructive/10"
                  onclick={() => handleDelete(snap.id)}
                >{t('invest_delete')}</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>
