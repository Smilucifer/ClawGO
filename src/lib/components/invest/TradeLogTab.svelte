<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import TradeDialog from './TradeDialog.svelte';
  import type { Trade } from '$lib/types';

  let { tushareToken = '' }: { tushareToken?: string } = $props();

  let symbolFilter = $state('');
  let directionFilter = $state<string>('all');
  let showSystemActions = $state(false);
  let editingTrade = $state<Trade | null>(null);

  const SYSTEM_ACTIONS = new Set(['cash_adjust', 'cost_edit', 'add_watch', 'delete_watch']);

  /** symbol → assetType lookup from holdings */
  const assetTypeMap = $derived(
    Object.fromEntries(investStore.holdings.map((h) => [h.symbol, h.assetType ?? 'stock']))
  );

  function priceDecimals(symbol: string): number {
    return assetTypeMap[symbol] === 'etf' ? 3 : 2;
  }

  /** symbol → Chinese name lookup from holdings */
  const nameMap = $derived(new Map(investStore.holdings.filter(h => h.name).map(h => [h.symbol, h.name!])));

  const filtered = $derived(
    investStore.trades.filter((tr) => {
      if (!showSystemActions && SYSTEM_ACTIONS.has(tr.action)) return false;
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
        tr.price?.toFixed(priceDecimals(tr.symbol)) ?? '',
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
  <div class="mb-4 flex flex-wrap items-center gap-[var(--space-3)]">
    <h3 class="text-[13px] font-medium text-[var(--text-primary)]">
      {t('invest_trade_log')} ({filtered.length})
    </h3>
    <input
      class="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-[var(--space-2)] py-[var(--space-1)] text-[13px] text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)]"
      placeholder={t('invest_trade_stock')}
      bind:value={symbolFilter}
    />
    <select class="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-[var(--space-2)] py-[var(--space-1)] text-[13px] text-[var(--text-primary)]" bind:value={directionFilter}>
      <option value="all">{t('invest_trade_filter_all')}</option>
      <option value="buy">{t('invest_trade_filter_buy')}</option>
      <option value="sell">{t('invest_trade_filter_sell')}</option>
    </select>
    <label class="flex items-center gap-[var(--space-1)] text-[12px] text-[var(--text-secondary)] cursor-pointer select-none">
      <input type="checkbox" bind:checked={showSystemActions} class="rounded border border-[var(--border)] accent-[var(--accent)]" />
      {t('invest_trade_show_system')}
    </label>
    <button class="ml-auto rounded-[var(--radius-md)] bg-[var(--bg-hover)] border border-[var(--border)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] hover:bg-[var(--accent-muted)] hover:text-[var(--text-primary)] transition-colors" onclick={exportCsv}>
      {t('invest_export_csv')}
    </button>
  </div>

  {#if filtered.length === 0}
    <p class="py-[var(--space-8)] text-center text-[13px] text-[var(--text-tertiary)]">{t('invest_no_trades')}</p>
  {:else}
    <div class="overflow-x-auto rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)]">
      <table class="w-full text-[13px]">
        <thead>
          <tr class="border-b border-[var(--border)] text-left text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">
            <th class="px-[var(--space-3)] py-[var(--space-2)]">{t('invest_trade_date')}</th>
            <th class="px-[var(--space-3)] py-[var(--space-2)]">{t('invest_trade_stock')}</th>
            <th class="px-[var(--space-3)] py-[var(--space-2)]">{t('invest_trade_direction')}</th>
            <th class="px-[var(--space-3)] py-[var(--space-2)]">{t('invest_quantity')}</th>
            <th class="px-[var(--space-3)] py-[var(--space-2)]">{t('invest_price')}</th>
            <th class="px-[var(--space-3)] py-[var(--space-2)]">{t('invest_trade_amount')}</th>
            <th class="px-[var(--space-3)] py-[var(--space-2)]">{t('invest_trade_notes')}</th>
            <th class="px-[var(--space-3)] py-[var(--space-2)]">{t('invest_actions')}</th>
          </tr>
        </thead>
        <tbody>
          {#each filtered as tr}
            <tr class="border-b border-[var(--border)] last:border-b-0 hover:bg-[var(--bg-hover)] transition-colors">
              <td class="px-[var(--space-3)] py-[var(--space-2)] text-[12px] text-[var(--text-secondary)]">{new Date(tr.createdAt).toLocaleDateString()}</td>
              <td class="px-[var(--space-3)] py-[var(--space-2)] font-medium text-[var(--text-primary)]" title={tr.symbol}>{nameMap.get(tr.symbol) ?? tr.symbol}</td>
              <td class="px-[var(--space-3)] py-[var(--space-2)]">
                <span class={tr.action === 'buy' ? 'text-[var(--color-error)]' : 'text-[var(--color-success)]'}>
                  {tr.action.toUpperCase()}
                </span>
              </td>
              <td class="px-[var(--space-3)] py-[var(--space-2)] font-[var(--font-mono)] text-[var(--text-primary)]">{tr.shares ?? '-'}</td>
              <td class="px-[var(--space-3)] py-[var(--space-2)] font-[var(--font-mono)] text-[var(--text-primary)]">{tr.price?.toFixed(priceDecimals(tr.symbol)) ?? '-'}</td>
              <td class="px-[var(--space-3)] py-[var(--space-2)] font-[var(--font-mono)] text-[var(--text-primary)]">{tr.amount?.toLocaleString(undefined, { minimumFractionDigits: 2 }) ?? '-'}</td>
              <td class="px-[var(--space-3)] py-[var(--space-2)] text-[12px] text-[var(--text-tertiary)]">{tr.notes ?? ''}</td>
              <td class="px-[var(--space-3)] py-[var(--space-2)]">
                <div class="flex gap-[var(--space-1)]">
                  <button
                    class="rounded-[var(--radius-md)] px-[var(--space-2)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] transition-colors"
                    onclick={() => { editingTrade = tr; }}
                  >{t('invest_edit')}</button>
                  <button
                    class="rounded-[var(--radius-md)] px-[var(--space-2)] py-[var(--space-1)] text-[12px] text-[var(--color-error)] hover:bg-[var(--color-error)]/10 transition-colors"
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
