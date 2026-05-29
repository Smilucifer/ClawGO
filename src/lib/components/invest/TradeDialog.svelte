<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import type { Holding } from '$lib/types';

  let { mode, prefill, tushareToken, onClose }: {
    mode: 'buy' | 'sell' | 'cash' | 'convert';
    prefill?: { symbol?: string; name?: string; holding?: Holding };
    tushareToken: string;
    onClose: () => void;
  } = $props();

  let symbol = $state(prefill?.symbol ?? '');
  let name = $state(prefill?.name ?? '');
  let quantity = $state(0);
  let price = $state(0);
  let cashBalance = $state(investStore.cash);
  let cashReason = $state('');
  let loading = $state(false);
  let error = $state<string | null>(null);
  let searchResults = $state<Array<{ tsCode: string; name: string }>>([]);
  let searchQuery = $state('');

  async function doSearch() {
    if (!searchQuery || !tushareToken) return;
    try {
      searchResults = await investStore.searchStocks(searchQuery, tushareToken);
    } catch (e) {
      error = String(e);
    }
  }

  async function fillMarketPrice() {
    if (!symbol || !tushareToken) return;
    try {
      price = await investStore.getLatestPrice(symbol, tushareToken);
    } catch (e) {
      error = String(e);
    }
  }

  async function handleSubmit() {
    loading = true;
    error = null;
    try {
      if (mode === 'buy') {
        await investStore.buyStock(symbol, name, quantity, price, tushareToken);
      } else if (mode === 'sell') {
        await investStore.sellStock(symbol, quantity, price);
      } else if (mode === 'cash') {
        await investStore.updateCash(cashBalance, cashReason);
      } else if (mode === 'convert') {
        await investStore.convertWatchToHold(symbol, name, quantity, price);
      }
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
  <div class="w-full max-w-md rounded-lg border bg-background p-6 shadow-lg">
    <h2 class="mb-4 text-lg font-semibold">
      {mode === 'buy' ? t('invest_confirm_buy') : mode === 'sell' ? t('invest_confirm_sell') : mode === 'convert' ? t('invest_convert_to_hold') : t('invest_edit_cash')}
    </h2>

    {#if error}
      <p class="mb-3 rounded bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p>
    {/if}

    {#if mode === 'buy' || mode === 'convert'}
      <div class="mb-3">
        {#if mode === 'buy'}
          <label for="td-search" class="mb-1 block text-sm">{t('invest_trade_stock')}</label>
          <div class="flex gap-2">
            <input
              id="td-search"
              class="flex-1 rounded border bg-background px-3 py-1.5 text-sm"
              placeholder={t('invest_stock_search')}
              bind:value={searchQuery}
            />
            <button class="rounded bg-muted px-3 py-1.5 text-sm" onclick={doSearch}>搜索</button>
          </div>
          {#if searchResults.length > 0}
            <div class="mt-1 max-h-32 overflow-auto rounded border">
              {#each searchResults as s}
                <button
                  class="w-full px-3 py-1.5 text-left text-sm hover:bg-muted"
                  onclick={() => { symbol = s.tsCode; name = s.name; searchResults = []; }}
                >
                  {s.name} ({s.tsCode})
                </button>
              {/each}
            </div>
          {/if}
        {/if}
        {#if symbol}
          <p class="mt-1 text-xs text-muted-foreground">已选: {name} ({symbol})</p>
        {/if}
      </div>
    {/if}

    {#if mode !== 'cash'}
      <div class="mb-3 grid grid-cols-2 gap-3">
        <div>
          <label for="td-qty" class="mb-1 block text-sm">{t('invest_quantity')}</label>
          <input id="td-qty" type="number" class="w-full rounded border bg-background px-3 py-1.5 text-sm" step="100" min="0" bind:value={quantity} />
        </div>
        <div>
          <label for="td-price" class="mb-1 block text-sm">{t('invest_price')}</label>
          <div class="flex gap-1">
            <input id="td-price" type="number" class="flex-1 rounded border bg-background px-3 py-1.5 text-sm" step="0.01" bind:value={price} />
            <button class="rounded bg-muted px-2 py-1.5 text-xs" onclick={fillMarketPrice}>{t('invest_market_price')}</button>
          </div>
        </div>
      </div>
      <p class="mb-3 text-sm text-muted-foreground">
        金额: ¥{(quantity * price).toLocaleString(undefined, { minimumFractionDigits: 2 })}
      </p>
    {:else}
      <div class="mb-3">
        <label for="td-cash" class="mb-1 block text-sm">{t('invest_cash_new_balance')}</label>
        <input id="td-cash" type="number" class="w-full rounded border bg-background px-3 py-1.5 text-sm" step="0.01" bind:value={cashBalance} />
      </div>
      <div class="mb-3">
        <label for="td-reason" class="mb-1 block text-sm">{t('invest_cash_reason')}</label>
        <textarea id="td-reason" class="w-full rounded border bg-background px-3 py-1.5 text-sm" rows="2" bind:value={cashReason}></textarea>
      </div>
    {/if}

    <div class="flex justify-end gap-2">
      <button class="rounded px-4 py-1.5 text-sm hover:bg-muted" onclick={onClose}>Cancel</button>
      <button
        class="rounded bg-primary px-4 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
        disabled={loading || (mode !== 'cash' && (!symbol || quantity <= 0 || price <= 0))}
        onclick={handleSubmit}
      >
        {loading ? '...' : mode === 'buy' ? t('invest_confirm_buy') : mode === 'sell' ? t('invest_confirm_sell') : t('invest_strategy_save')}
      </button>
    </div>
  </div>
</div>
