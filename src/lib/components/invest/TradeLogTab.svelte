<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import TradeDialog from './TradeDialog.svelte';
  import type { Trade } from '$lib/types';

  let { tushareToken = '' }: { tushareToken?: string } = $props();

  let symbolFilter = $state('');
  let directionFilter = $state<string>('all');
  let editingTrade = $state<Trade | null>(null);

  const filtered = $derived(
    investStore.trades.filter((tr) => {
      if (symbolFilter && !tr.symbol.includes(symbolFilter.toUpperCase())) return false;
      if (directionFilter !== 'all' && tr.action !== directionFilter) return false;
      return true;
    })
  );

  async function handleDelete(trade: Trade) {
    if (!confirm(t('invest_trade_delete_confirm'))) return;
    try {
      await investStore.deleteTrade(trade.id);
    } catch (e) {
      console.error('Failed to delete trade:', e);
    }
  }

  function exportCsv() {
    const header = 'Date,Stock,Direction,Quantity,Price,Amount,Notes\n';
    const rows = filtered.map((tr) =>
      [
        new Date(tr.createdAt).toLocaleDateString(),
        tr.symbol,
        tr.action,
        tr.shares ?? '',
        tr.price?.toFixed(2) ?? '',
        tr.amount?.toFixed(2) ?? '',
        (tr.notes ?? '').replace(/,/g, ';'),
      ].join(',')
    );
    const blob = new Blob([header + rows.join('\n')], { type: 'text/csv' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `trades-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<div>
  <div class="mb-4 flex flex-wrap items-center gap-3">
    <h3 class="text-sm font-medium text-muted-foreground">
      {t('invest_trade_log')} ({filtered.length})
    </h3>
    <input
      class="rounded border bg-background px-2 py-1 text-sm"
      placeholder={t('invest_trade_stock')}
      bind:value={symbolFilter}
    />
    <select class="rounded border bg-background px-2 py-1 text-sm" bind:value={directionFilter}>
      <option value="all">All</option>
      <option value="buy">Buy</option>
      <option value="sell">Sell</option>
    </select>
    <button class="ml-auto rounded bg-muted px-3 py-1 text-sm" onclick={exportCsv}>
      {t('invest_export_csv')}
    </button>
  </div>

  {#if filtered.length === 0}
    <p class="py-4 text-center text-sm text-muted-foreground">{t('invest_no_trades')}</p>
  {:else}
    <div class="overflow-x-auto">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b text-left text-xs text-muted-foreground">
            <th class="pb-2 pr-4">{t('invest_trade_date')}</th>
            <th class="pb-2 pr-4">{t('invest_trade_stock')}</th>
            <th class="pb-2 pr-4">{t('invest_trade_direction')}</th>
            <th class="pb-2 pr-4">{t('invest_quantity')}</th>
            <th class="pb-2 pr-4">{t('invest_price')}</th>
            <th class="pb-2 pr-4">{t('invest_trade_amount')}</th>
            <th class="pb-2">{t('invest_trade_notes')}</th>
            <th class="pb-2">{t('invest_actions')}</th>
          </tr>
        </thead>
        <tbody>
          {#each filtered as tr}
            <tr class="border-b border-border/50">
              <td class="py-2 pr-4 text-xs">{new Date(tr.createdAt).toLocaleDateString()}</td>
              <td class="py-2 pr-4 font-medium">{tr.symbol}</td>
              <td class="py-2 pr-4">
                <span class={tr.action === 'buy' ? 'text-red-600' : 'text-green-600'}>
                  {tr.action.toUpperCase()}
                </span>
              </td>
              <td class="py-2 pr-4 tabular-nums">{tr.shares ?? '-'}</td>
              <td class="py-2 pr-4 tabular-nums">{tr.price?.toFixed(2) ?? '-'}</td>
              <td class="py-2 pr-4 tabular-nums">{tr.amount?.toLocaleString(undefined, { minimumFractionDigits: 2 }) ?? '-'}</td>
              <td class="py-2 text-xs text-muted-foreground">{tr.notes ?? ''}</td>
              <td class="py-2">
                <div class="flex gap-1">
                  <button
                    class="rounded px-2 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground"
                    onclick={() => { editingTrade = tr; }}
                  >{t('invest_edit')}</button>
                  <button
                    class="rounded px-2 py-1 text-xs text-destructive hover:bg-destructive/10"
                    onclick={() => handleDelete(tr)}
                  >{t('invest_delete')}</button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

{#if editingTrade}
  <TradeDialog
    mode="edit"
    editTrade={editingTrade}
    {tushareToken}
    onClose={() => { editingTrade = null; }}
  />
{/if}
