<script lang="ts">
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import type { Holding } from '$lib/types';

  let { onSell, onConvert, onAddWatch, tushareToken }: {
    onSell: (h: Holding) => void;
    onConvert: (h: Holding) => void;
    onAddWatch: () => void;
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
</script>

<div class="space-y-4">
  <div>
    <h3 class="mb-2 text-sm font-medium text-muted-foreground">
      {t('invest_hold')} ({investStore.holdCount})
    </h3>
    {#if investStore.holdHoldings.length === 0}
      <p class="py-4 text-center text-sm text-muted-foreground">{t('invest_no_holdings')}</p>
    {:else}
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b text-left text-xs text-muted-foreground">
              <th class="pb-2 pr-4">{t('invest_trade_stock')}</th>
              <th class="pb-2 pr-4">{t('invest_quantity')}</th>
              <th class="pb-2 pr-4">{t('invest_cost_price')}</th>
              <th class="pb-2 pr-4">{t('invest_current_price')}</th>
              <th class="pb-2 pr-4">{t('invest_pnl_pct')}</th>
              <th class="pb-2"></th>
            </tr>
          </thead>
          <tbody>
            {#each investStore.holdHoldings as h}
              {@const pnlPct = getPnlPct(h)}
              <tr class="border-b border-border/50">
                <td class="py-2 pr-4">
                  <span class="font-medium">{h.name ?? h.symbol}</span>
                  <span class="ml-1 text-xs text-muted-foreground">{h.symbol}</span>
                </td>
                <td class="py-2 pr-4 tabular-nums">{h.shares ?? '-'}</td>
                <td class="py-2 pr-4 tabular-nums">{h.avgCost?.toFixed(2) ?? '-'}</td>
                <td class="py-2 pr-4 tabular-nums">{getPrice(h.symbol)?.toFixed(2) ?? '-'}</td>
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
                  <button
                    class="rounded px-2 py-0.5 text-xs hover:bg-muted"
                    onclick={() => onSell(h)}
                  >{t('invest_sell')}</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>

  <div>
    <div class="mb-2 flex items-center justify-between">
      <h3 class="text-sm font-medium text-muted-foreground">
        {t('invest_watch')} ({investStore.watchCount})
      </h3>
      <button
        class="rounded px-2 py-0.5 text-xs hover:bg-muted"
        onclick={onAddWatch}
      >+ {t('invest_add_watch')}</button>
    </div>
    {#if investStore.watchHoldings.length > 0}
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b text-left text-xs text-muted-foreground">
              <th class="pb-2 pr-4">{t('invest_trade_stock')}</th>
              <th class="pb-2 pr-4">{t('invest_current_price')}</th>
              <th class="pb-2"></th>
            </tr>
          </thead>
          <tbody>
            {#each investStore.watchHoldings as h}
              <tr class="border-b border-border/50">
                <td class="py-2 pr-4">
                  <span class="font-medium">{h.name ?? h.symbol}</span>
                  <span class="ml-1 text-xs text-muted-foreground">{h.symbol}</span>
                </td>
                <td class="py-2 pr-4 tabular-nums">{getPrice(h.symbol)?.toFixed(2) ?? '-'}</td>
                <td class="py-2">
                  <button
                    class="rounded px-2 py-0.5 text-xs hover:bg-muted"
                    onclick={() => onConvert(h)}
                  >{t('invest_convert_to_hold')}</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <p class="py-4 text-center text-sm text-muted-foreground">{t('invest_no_watchlist')}</p>
    {/if}
  </div>
</div>
