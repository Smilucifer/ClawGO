<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { getInvestDate } from '$lib/i18n/format';
  import type { Holding, Trade } from '$lib/types';
  import type { MessageKey } from '$lib/i18n/types';

  let { mode, prefill, editTrade, tushareToken, onClose }: {
    mode: 'buy' | 'sell' | 'cash' | 'convert' | 'add_watch' | 'edit' | 'add_trade' | 'edit_holding';
    prefill?: { symbol?: string; name?: string; holding?: Holding };
    editTrade?: Trade;
    tushareToken: string;
    onClose: () => void;
  } = $props();

  let symbol = $state(editTrade?.symbol ?? prefill?.symbol ?? '');
  let name = $state(prefill?.name ?? editTrade?.name ?? '');
  let quantity = $state(editTrade?.shares ?? 0);
  let price = $state(editTrade?.price ?? 0);
  let notes = $state(editTrade?.notes ?? '');
  let tradeDate = $state(editTrade?.tradeDate ?? getInvestDate());
  let cashReason = $state('');
  // Cash sub-mode: transfer_in (银行→证券), transfer_out (证券→银行), fine_tune (微调修正)
  let cashSubMode = $state<'transfer_in' | 'transfer_out' | 'fine_tune'>('transfer_in');
  let cashAmount = $state(0);
  const CASH_MODES: Array<{ key: 'transfer_in' | 'transfer_out' | 'fine_tune'; i18n: MessageKey; activeBg: string; activeText: string }> = [
    { key: 'transfer_in', i18n: 'invest_transfer_in', activeBg: 'bg-[var(--color-success)]', activeText: 'text-white' },
    { key: 'transfer_out', i18n: 'invest_transfer_out', activeBg: 'bg-[var(--color-error)]', activeText: 'text-white' },
    { key: 'fine_tune', i18n: 'invest_fine_tune', activeBg: 'bg-[var(--accent)]', activeText: 'text-[#1a1918]' },
  ];
  let loading = $state(false);
  let error = $state<string | null>(null);
  let searchResults = $state<Array<{ tsCode: string; name: string }>>([]);
  let searchQuery = $state('');
  let assetType = $state<'stock' | 'etf'>('stock');

  // For edit_holding mode
  let holdingEntryDate = $state('');
  let holdingNotes = $state('');
  let holdingShares = $state(0);
  let holdingAvgCost = $state(0);

  // Initialize edit_holding fields from prefill (capture once)
  $effect(() => {
    if (mode === 'edit_holding' && prefill?.holding) {
      holdingEntryDate = prefill.holding.entryDate ?? '';
      holdingNotes = prefill.holding.notes ?? '';
      holdingShares = prefill.holding.shares ?? 0;
      holdingAvgCost = prefill.holding.avgCost ?? 0;
    }
  });

  // For add_trade mode
  let tradeAction = $state<'buy' | 'sell'>('buy');

  async function doSearch() {
    if (!searchQuery || !tushareToken) return;
    try {
      if (assetType === 'etf') {
        const etfs = await investStore.searchEtfs(searchQuery, tushareToken);
        searchResults = etfs.map((f) => ({ tsCode: f.tsCode, name: f.name }));
      } else {
        searchResults = await investStore.searchStocks(searchQuery, tushareToken);
      }
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
      if (mode === 'cash' && cashSubMode === 'transfer_out' && cashAmount > investStore.cash) {
        error = t('invest_transfer_out_exceeds_cash');
        return;
      }
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
          name: name || editTrade.name || null,
          tradeDate: tradeDate ?? null,
        });
      } else if (mode === 'buy') {
        await investStore.buyStock(symbol, name, quantity, price, tushareToken, assetType, tradeDate);
      } else if (mode === 'sell') {
        await investStore.sellStock(symbol, quantity, price);
      } else if (mode === 'cash') {
        // Record cash transfer/adjustment as a trade for audit trail.
        // Map UI-only 'fine_tune' to the storage-level 'cash_adjust' action.
        const cashAction = cashSubMode === 'fine_tune' ? 'cash_adjust' : cashSubMode;
        await investStore.recordTrade({
          symbol: 'CASH',
          kind: 'cash',
          action: cashAction,
          shares: null,
          price: null,
          amount: cashAmount,
          notes: cashReason || null,
          name: null,
          tradeDate: tradeDate ?? null,
          assetType: null,
        });
      } else if (mode === 'convert') {
        await investStore.convertWatchToHold(symbol, name, quantity, price);
      } else if (mode === 'add_watch') {
        await investStore.addToWatch(symbol, name, price, assetType);
      } else if (mode === 'add_trade') {
        // Manual trade entry — records a buy or sell trade with custom date.
        // Backend auto-recalculates cash via recalculate_cash_inner.
        const amount = quantity * price;
        await investStore.recordTrade({
          symbol,
          kind: 'hold',
          action: tradeAction,
          shares: quantity,
          price,
          amount,
          notes: notes || null,
          name: name || null,
          tradeDate: tradeDate ?? null,
          assetType,
        });
      } else if (mode === 'edit_holding' && prefill?.holding) {
        // Update holding entry_date, cost, shares, notes
        const h = prefill.holding;
        await investStore.updateHoldingMeta({
          symbol: h.symbol,
          currency: h.currency,
          kind: h.kind,
          name: h.name,
          notional: h.notional,
          avgCost: holdingAvgCost ?? h.avgCost,
          shares: holdingShares ?? h.shares,
          entryDate: holdingEntryDate || null,
          linkedVerdictId: h.linkedVerdictId,
          notes: holdingNotes || null,
          assetType: h.assetType,
        });
      }
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  const title = $derived(
    mode === 'edit' ? t('invest_trade_edit')
    : mode === 'buy' ? t('invest_confirm_buy')
    : mode === 'sell' ? t('invest_confirm_sell')
    : mode === 'convert' ? t('invest_convert_to_hold')
    : mode === 'add_watch' ? t('invest_add_watch')
    : mode === 'add_trade' ? t('invest_trade_add')
    : mode === 'edit_holding' ? t('invest_holding_edit')
    : t('invest_cash_management')
  );

  const SUBMIT_LABELS: Record<string, MessageKey> = {
    edit: 'invest_trade_save',
    buy: 'invest_confirm_buy',
    sell: 'invest_confirm_sell',
    convert: 'invest_convert_to_hold',
    add_watch: 'invest_add_watch',
    add_trade: 'invest_trade_add',
    edit_holding: 'invest_trade_save',
  };
  const submitLabel = $derived(
    loading ? '...' : (SUBMIT_LABELS[mode] ? t(SUBMIT_LABELS[mode])
    : cashSubMode === 'transfer_in' ? t('invest_transfer_in')
    : cashSubMode === 'transfer_out' ? t('invest_transfer_out')
    : t('invest_fine_tune'))
  );

  const needsSearch = $derived(mode === 'buy' || mode === 'add_watch' || mode === 'add_trade');
  const needsQty = $derived(mode !== 'cash' && mode !== 'add_watch');
  const needsPrice = $derived(mode !== 'cash');
  const canSubmit = $derived(
    mode === 'cash' ? (cashAmount !== 0 && (cashSubMode !== 'transfer_out' || cashAmount <= investStore.cash))
    : mode === 'add_watch' ? (symbol && price > 0)
    : mode === 'edit_holding' ? true
    : (symbol && quantity > 0 && price > 0)
  );
</script>

<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50" data-invest-scope>
  <div class="w-full max-w-md rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-6 shadow-lg">
    <h2 class="mb-4 text-lg font-semibold text-[var(--text-primary)]">{title}</h2>

    {#if error}
      <p class="mb-3 rounded-[var(--radius-md)] bg-[rgba(168,122,122,0.1)] px-3 py-2 text-sm text-[var(--color-error)]">{error}</p>
    {/if}

    {#if mode === 'edit_holding' && prefill?.holding}
      <!-- Edit holding mode -->
      <div class="mb-3">
        <p class="text-sm font-medium text-[var(--text-primary)]">{prefill.holding.name ?? prefill.holding.symbol} ({prefill.holding.symbol})</p>
      </div>
      <div class="mb-3 grid grid-cols-2 gap-3">
        <div>
          <label for="eh-shares" class="mb-1 block text-sm text-[var(--text-secondary)]">{t('invest_quantity')}</label>
          <input id="eh-shares" type="number" class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-3 py-1.5 text-sm text-[var(--text-primary)] font-[var(--font-mono)]" step="100" min="0" bind:value={holdingShares} />
        </div>
        <div>
          <label for="eh-cost" class="mb-1 block text-sm text-[var(--text-secondary)]">{t('invest_cost_price')}</label>
          <input id="eh-cost" type="number" class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-3 py-1.5 text-sm text-[var(--text-primary)] font-[var(--font-mono)]" step="0.01" min="0" bind:value={holdingAvgCost} />
        </div>
      </div>
      <div class="mb-3">
        <label for="eh-date" class="mb-1 block text-sm text-[var(--text-secondary)]">{t('invest_trade_buy_date')}</label>
        <input id="eh-date" type="date" class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-3 py-1.5 text-sm text-[var(--text-primary)]" bind:value={holdingEntryDate} />
      </div>
      <div class="mb-3">
        <label for="eh-notes" class="mb-1 block text-sm text-[var(--text-secondary)]">{t('invest_trade_notes')}</label>
        <textarea id="eh-notes" class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-3 py-1.5 text-sm text-[var(--text-primary)]" rows="2" bind:value={holdingNotes}></textarea>
      </div>

    {:else if needsSearch}
      <!-- Search + select stock/ETF -->
      <div class="mb-3">
        <label for="td-search" class="mb-1 block text-sm text-[var(--text-secondary)]">{t('invest_trade_stock')}</label>
        <div class="mb-2 flex gap-2">
          <button
            class="rounded-[var(--radius-md)] px-3 py-1 text-xs transition-colors"
            class:bg-[var(--accent)]={assetType === 'stock'}
            class:text-[#1a1918]={assetType === 'stock'}
            class:bg-[var(--bg-input)]={assetType !== 'stock'}
            class:text-[var(--text-secondary)]={assetType !== 'stock'}
            onclick={() => assetType = 'stock'}
          >{t('invest_asset_type_stock')}</button>
          <button
            class="rounded-[var(--radius-md)] px-3 py-1 text-xs transition-colors"
            class:bg-[var(--accent)]={assetType === 'etf'}
            class:text-[#1a1918]={assetType === 'etf'}
            class:bg-[var(--bg-input)]={assetType !== 'etf'}
            class:text-[var(--text-secondary)]={assetType !== 'etf'}
            onclick={() => assetType = 'etf'}
          >{t('invest_asset_type_etf')}</button>
        </div>
        <div class="flex gap-2">
          <input
            id="td-search"
            class="flex-1 rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-3 py-1.5 text-sm text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)]"
            placeholder={t('invest_stock_search')}
            bind:value={searchQuery}
            onkeydown={(e) => { if (e.key === 'Enter') doSearch(); }}
          />
          <button class="rounded-[var(--radius-md)] bg-[var(--bg-input)] px-3 py-1.5 text-sm text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]" onclick={doSearch}>{t('invest_search')}</button>
        </div>
        {#if searchResults.length > 0}
          <div class="mt-1 max-h-32 overflow-auto rounded-[var(--radius-md)] border border-border">
            {#each searchResults as s}
              <button
                class="w-full px-3 py-1.5 text-left text-sm text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
                onclick={() => { symbol = s.tsCode; name = s.name; searchResults = []; }}
              >
                {s.name} ({s.tsCode})
              </button>
            {/each}
          </div>
        {/if}
        {#if symbol}
          <p class="mt-1 text-xs text-[var(--text-tertiary)]">{t('invest_selected')}: {name} ({symbol})</p>
        {/if}
      </div>
    {/if}

    {#if mode === 'add_trade'}
      <!-- Direction selector for manual trade entry -->
      <div class="mb-3 flex gap-2">
        <button
          class="rounded-[var(--radius-md)] px-3 py-1 text-xs font-medium transition-colors"
          class:bg-[var(--color-error)]={tradeAction === 'buy'}
          class:text-white={tradeAction === 'buy'}
          class:bg-[var(--bg-input)]={tradeAction !== 'buy'}
          class:text-[var(--text-secondary)]={tradeAction !== 'buy'}
          onclick={() => tradeAction = 'buy'}
        >{t('invest_trade_filter_buy')}</button>
        <button
          class="rounded-[var(--radius-md)] px-3 py-1 text-xs font-medium transition-colors"
          class:bg-[var(--color-success)]={tradeAction === 'sell'}
          class:text-white={tradeAction === 'sell'}
          class:bg-[var(--bg-input)]={tradeAction !== 'sell'}
          class:text-[var(--text-secondary)]={tradeAction !== 'sell'}
          onclick={() => tradeAction = 'sell'}
        >{t('invest_trade_filter_sell')}</button>
      </div>
    {/if}

    {#if mode !== 'cash' && mode !== 'edit_holding'}
      <div class="mb-3 grid grid-cols-2 gap-3">
        {#if needsQty}
          <div>
            <label for="td-qty" class="mb-1 block text-sm text-[var(--text-secondary)]">{t('invest_quantity')}</label>
            <input id="td-qty" type="number" class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-3 py-1.5 text-sm text-[var(--text-primary)] font-[var(--font-mono)]" step="100" min="0" bind:value={quantity} />
          </div>
        {/if}
        <div class={mode === 'add_watch' ? 'col-span-2' : ''}>
          <label for="td-price" class="mb-1 block text-sm text-[var(--text-secondary)]">{mode === 'add_watch' ? t('invest_watch_price') : t('invest_price')}</label>
          <div class="flex gap-1">
            <input id="td-price" type="number" class="flex-1 rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-3 py-1.5 text-sm text-[var(--text-primary)] font-[var(--font-mono)]" step={assetType === 'etf' ? '0.001' : '0.01'} bind:value={price} />
            <button class="rounded-[var(--radius-md)] bg-[var(--bg-input)] px-2 py-1.5 text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]" onclick={fillMarketPrice}>{t('invest_market_price')}</button>
          </div>
        </div>
      </div>
      {#if mode !== 'add_watch'}
        <p class="mb-3 text-sm text-[var(--text-secondary)]">
          {t('invest_amount')}: <span class="font-[var(--font-mono)]">¥{(quantity * price).toLocaleString(undefined, { minimumFractionDigits: 2 })}</span>
        </p>
      {/if}
      <!-- Trade date input -->
      <div class="mb-3">
        <label for="td-date" class="mb-1 block text-sm text-[var(--text-secondary)]">{t('invest_trade_buy_date')}</label>
        <input id="td-date" type="date" class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-3 py-1.5 text-sm text-[var(--text-primary)]" bind:value={tradeDate} />
      </div>
      <!-- Notes (for edit, add_trade modes) -->
      {#if mode === 'edit' || mode === 'add_trade'}
        <div class="mb-3">
          <label for="td-notes" class="mb-1 block text-sm text-[var(--text-secondary)]">{t('invest_trade_notes')}</label>
          <textarea id="td-notes" class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-3 py-1.5 text-sm text-[var(--text-primary)]" rows="2" bind:value={notes}></textarea>
        </div>
      {/if}
    {:else if mode === 'cash'}
      <!-- Current balance display -->
      <p class="mb-3 text-sm text-[var(--text-secondary)]">{t('invest_cash')}: <span class="font-[var(--font-mono)] text-[var(--text-primary)]">¥{investStore.cash.toLocaleString(undefined, { minimumFractionDigits: 2 })}</span></p>
      <!-- Sub-mode selector -->
      <div class="mb-3 flex gap-2">
        {#each CASH_MODES as m}
          {@const active = cashSubMode === m.key}
          <button
            class="flex-1 rounded-[var(--radius-md)] px-2 py-1.5 text-xs font-medium transition-colors {active ? `${m.activeBg} ${m.activeText}` : 'bg-[var(--bg-input)] text-[var(--text-secondary)]'}"
            onclick={() => { cashSubMode = m.key; cashAmount = 0; }}
          >{t(m.i18n)}</button>
        {/each}
      </div>
      <!-- Sub-mode description -->
      <p class="mb-2 text-xs text-[var(--text-tertiary)]">
        {cashSubMode === 'transfer_in' ? t('invest_transfer_in_desc')
        : cashSubMode === 'transfer_out' ? t('invest_transfer_out_desc')
        : t('invest_fine_tune_desc')}
      </p>
      <!-- Amount input -->
      <div class="mb-3">
        <label for="td-cash-amt" class="mb-1 block text-sm text-[var(--text-secondary)]">
          {cashSubMode === 'fine_tune' ? t('invest_fine_tune_amount') : t('invest_transfer_amount')}
        </label>
        <input
          id="td-cash-amt"
          type="number"
          class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-3 py-1.5 text-sm text-[var(--text-primary)] font-[var(--font-mono)]"
          step="0.01"
          min={cashSubMode !== 'fine_tune' ? 0 : undefined}
          bind:value={cashAmount}
        />
      </div>
      <!-- Notes -->
      <div class="mb-3">
        <label for="td-cash-notes" class="mb-1 block text-sm text-[var(--text-secondary)]">{t('invest_trade_notes')}</label>
        <textarea id="td-cash-notes" class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-3 py-1.5 text-sm text-[var(--text-primary)]" rows="2" bind:value={cashReason}></textarea>
      </div>
    {/if}

    <div class="flex justify-end gap-2">
      <button class="rounded-[var(--radius-md)] px-4 py-1.5 text-sm text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]" onclick={onClose}>{t('invest_cancel')}</button>
      <button
        class="rounded-[var(--radius-md)] bg-[var(--accent)] px-4 py-1.5 text-sm text-[#1a1918] font-medium disabled:opacity-50"
        disabled={loading || !canSubmit}
        onclick={handleSubmit}
      >
        {submitLabel}
      </button>
    </div>
  </div>
</div>
