<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import type { Holding, Trade } from '$lib/types';

  let { mode, prefill, editTrade, tushareToken, onClose }: {
    mode: 'buy' | 'sell' | 'cash' | 'convert' | 'add_watch' | 'edit';
    prefill?: { symbol?: string; name?: string; holding?: Holding };
    editTrade?: Trade;
    tushareToken: string;
    onClose: () => void;
  } = $props();

  let symbol = $state(editTrade?.symbol ?? prefill?.symbol ?? '');
  let name = $state(prefill?.name ?? '');
  let quantity = $state(editTrade?.shares ?? 0);
  let price = $state(editTrade?.price ?? 0);
  let notes = $state(editTrade?.notes ?? '');
  let cashBalance = $state(investStore.cash);
  let cashReason = $state('');
  let loading = $state(false);
  let error = $state<string | null>(null);
  let searchResults = $state<Array<{ tsCode: string; name: string }>>([]);
  let searchQuery = $state('');
  let assetType = $state<'stock' | 'etf'>('stock');

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
      if (mode === 'edit' && editTrade) {
        const amount = quantity * price;
        await investStore.updateTrade({
          id: editTrade.id,
          symbol: editTrade.symbol,
          currency: editTrade.currency,
          kind: editTrade.kind,
          action: editTrade.action,
          shares: quantity || null,
          price: price || null,
          amount: amount || null,
          notes: notes || null,
        });
      } else if (mode === 'buy') {
        await investStore.buyStock(symbol, name, quantity, price, tushareToken, assetType);
      } else if (mode === 'sell') {
        await investStore.sellStock(symbol, quantity, price);
      } else if (mode === 'cash') {
        await investStore.updateCash(cashBalance, cashReason);
      } else if (mode === 'convert') {
        await investStore.convertWatchToHold(symbol, name, quantity, price);
      } else if (mode === 'add_watch') {
        await investStore.addToWatch(symbol, name, price, assetType);
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
      {mode === 'edit' ? t('invest_trade_edit') : mode === 'buy' ? t('invest_confirm_buy') : mode === 'sell' ? t('invest_confirm_sell') : mode === 'convert' ? t('invest_convert_to_hold') : mode === 'add_watch' ? t('invest_add_watch') : t('invest_edit_cash')}
    </h2>

    {#if error}
      <p class="mb-3 rounded bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p>
    {/if}

    {#if mode === 'buy' || mode === 'convert' || mode === 'add_watch'}
      <div class="mb-3">
        {#if mode === 'buy' || mode === 'add_watch'}
          <label for="td-search" class="mb-1 block text-sm">{t('invest_trade_stock')}</label>
          <div class="mb-2 flex gap-2">
            <button
              class="rounded px-3 py-1 text-xs transition-colors"
              class:bg-primary={assetType === 'stock'}
              class:text-primary-foreground={assetType === 'stock'}
              class:bg-muted={assetType !== 'stock'}
              onclick={() => assetType = 'stock'}
            >{t('invest_asset_type_stock')}</button>
            <button
              class="rounded px-3 py-1 text-xs transition-colors"
              class:bg-primary={assetType === 'etf'}
              class:text-primary-foreground={assetType === 'etf'}
              class:bg-muted={assetType !== 'etf'}
              onclick={() => assetType = 'etf'}
            >{t('invest_asset_type_etf')}</button>
          </div>
          <div class="flex gap-2">
            <input
              id="td-search"
              class="flex-1 rounded border bg-background px-3 py-1.5 text-sm"
              placeholder={t('invest_stock_search')}
              bind:value={searchQuery}
            />
            <button class="rounded bg-muted px-3 py-1.5 text-sm" onclick={doSearch}>{t('invest_search')}</button>
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
          <p class="mt-1 text-xs text-muted-foreground">{t('invest_selected')}: {name} ({symbol})</p>
        {/if}
      </div>
    {/if}

    {#if mode !== 'cash'}
      <div class="mb-3 grid grid-cols-2 gap-3">
        {#if mode !== 'add_watch'}
          <div>
            <label for="td-qty" class="mb-1 block text-sm">{t('invest_quantity')}</label>
            <input id="td-qty" type="number" class="w-full rounded border bg-background px-3 py-1.5 text-sm" step="100" min="0" bind:value={quantity} />
          </div>
        {/if}
        <div class={mode === 'add_watch' ? 'col-span-2' : ''}>
          <label for="td-price" class="mb-1 block text-sm">{mode === 'add_watch' ? t('invest_watch_price') : t('invest_price')}</label>
          <div class="flex gap-1">
            <input id="td-price" type="number" class="flex-1 rounded border bg-background px-3 py-1.5 text-sm" step="0.01" bind:value={price} />
            <button class="rounded bg-muted px-2 py-1.5 text-xs" onclick={fillMarketPrice}>{t('invest_market_price')}</button>
          </div>
        </div>
      </div>
      {#if mode !== 'add_watch'}
        <p class="mb-3 text-sm text-muted-foreground">
          {t('invest_amount')}: ¥{(quantity * price).toLocaleString(undefined, { minimumFractionDigits: 2 })}
        </p>
      {/if}
      {#if mode === 'edit'}
        <div class="mb-3">
          <label for="td-notes" class="mb-1 block text-sm">{t('invest_trade_notes')}</label>
          <textarea id="td-notes" class="w-full rounded border bg-background px-3 py-1.5 text-sm" rows="2" bind:value={notes}></textarea>
        </div>
      {/if}
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
        disabled={loading || (mode !== 'cash' && mode !== 'add_watch' && mode !== 'edit' && (!symbol || quantity <= 0 || price <= 0)) || (mode === 'add_watch' && (!symbol || price <= 0))}
        onclick={handleSubmit}
      >
        {loading ? '...' : mode === 'edit' ? t('invest_trade_save') : mode === 'buy' ? t('invest_confirm_buy') : mode === 'sell' ? t('invest_confirm_sell') : mode === 'add_watch' ? t('invest_add_watch') : t('invest_strategy_save')}
      </button>
    </div>
  </div>
</div>
