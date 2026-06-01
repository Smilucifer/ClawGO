<script lang="ts">
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import type { Holding } from '$lib/types';

  let { onSell, onConvert, onAddWatch, onDeleteWatch, tushareToken }: {
    onSell: (h: Holding) => void;
    onConvert: (h: Holding) => void;
    onAddWatch: () => void;
    onDeleteWatch: (h: Holding) => void;
    tushareToken: string;
  } = $props();

  function getPrice(sym: string): number | null {
    return investStore.priceMap[sym]?.close ?? null;
  }

  function getPnlPct(h: Holding): number | null {
    const price = getPrice(h.symbol);
    if (price == null || h.avgCost == null || h.avgCost === 0) return null;
    return ((price - h.avgCost) / h.avgCost) * 100;
  }

  function priceDecimals(assetType: string | null): number {
    return assetType === 'etf' ? 3 : 2;
  }

  function assetLabel(assetType: string | null): string {
    return assetType === 'etf' ? t('invest_asset_type_etf') : t('invest_asset_type_stock');
  }

  function assetBadgeClass(assetType: string | null): string {
    return assetType === 'etf'
      ? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400'
      : 'bg-muted text-muted-foreground';
  }

  function statusBadgeClass(kind: string): string {
    if (kind === 'hold') {
      return 'inline-block rounded px-1.5 py-0.5 text-xs font-medium bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400';
    }
    return 'inline-block rounded px-1.5 py-0.5 text-xs font-medium bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400';
  }
</script>

<div class="space-y-4">
  <div class="flex items-center justify-between">
    <h3 class="text-sm font-medium text-muted-foreground">
      {t('invest_hold')} {investStore.holdCount} / {t('invest_watch')} {investStore.watchCount}
    </h3>
    <button
      class="rounded px-2 py-0.5 text-xs hover:bg-muted"
      onclick={onAddWatch}
    >+ {t('invest_add_watch')}</button>
  </div>

  {#if investStore.mergedHoldings.length === 0}
    <p class="py-4 text-center text-sm text-muted-foreground">{t('invest_no_holdings')}</p>
  {:else}
    <div class="overflow-x-auto">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b text-left text-xs text-muted-foreground">
            <th class="pb-2 pr-4">{t('invest_trade_stock')}</th>
            <th class="pb-2 pr-4">{t('invest_status')}</th>
            <th class="pb-2 pr-4">{t('invest_asset_type')}</th>
            <th class="pb-2 pr-4">{t('invest_quantity')}</th>
            <th class="pb-2 pr-4">{t('invest_cost_price')}</th>
            <th class="pb-2 pr-4">{t('invest_current_price')}</th>
            <th class="pb-2 pr-4">{t('invest_pnl_pct')}</th>
            <th class="pb-2"></th>
          </tr>
        </thead>
        <tbody>
          {#each investStore.mergedHoldings as h}
            {@const pnlPct = h.kind === 'hold' ? getPnlPct(h) : null}
            <tr class="border-b border-border/50">
              <td class="py-2 pr-4">
                <span class="font-medium">{h.name ?? h.symbol}</span>
                <span class="ml-1 text-xs text-muted-foreground">{h.symbol}</span>
              </td>
              <td class="py-2 pr-4">
                <span class={statusBadgeClass(h.kind)}>
                  {h.kind === 'hold' ? t('invest_status_hold') : t('invest_status_watch')}
                </span>
              </td>
              <td class="py-2 pr-4">
                <span class="inline-block rounded px-1.5 py-0.5 text-xs {assetBadgeClass(h.assetType)}">
                  {assetLabel(h.assetType)}
                </span>
              </td>
              <td class="py-2 pr-4 tabular-nums">{h.shares ?? '-'}</td>
              <td class="py-2 pr-4 tabular-nums">{h.avgCost?.toFixed(priceDecimals(h.assetType)) ?? '-'}</td>
              <td class="py-2 pr-4 tabular-nums">{getPrice(h.symbol)?.toFixed(priceDecimals(h.assetType)) ?? '-'}</td>
              <td class="py-2 pr-4 tabular-nums">
                {#if pnlPct !== null}
                  <span class={pnlPct >= 0 ? 'text-green-600' : 'text-red-600'}>
                    {pnlPct.toFixed(2)}%
                  </span>
                {:else}
                  -
                {/if}
              </td>
              <td class="py-2">
                {#if h.kind === 'hold'}
                  <button
                    class="rounded px-2 py-0.5 text-xs hover:bg-muted"
                    onclick={() => onSell(h)}
                  >{t('invest_sell')}</button>
                {:else}
                  <button
                    class="rounded px-2 py-0.5 text-xs hover:bg-muted"
                    onclick={() => onConvert(h)}
                  >{t('invest_convert_to_hold')}</button>
                  <button
                    class="ml-1 rounded px-2 py-0.5 text-xs text-red-600 hover:bg-red-100 dark:hover:bg-red-900/30"
                    onclick={() => onDeleteWatch(h)}
                  >{t('invest_delete')}</button>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>
