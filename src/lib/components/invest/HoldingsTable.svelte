<script lang="ts">
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import type { Holding } from '$lib/types';

  let { onSell, onConvert, onAddWatch, onDeleteWatch, onEdit, tushareToken }: {
    onSell: (h: Holding) => void;
    onConvert: (h: Holding) => void;
    onAddWatch: () => void;
    onDeleteWatch: (h: Holding) => void;
    onEdit: (h: Holding) => void;
    tushareToken: string;
  } = $props();

  let filter = $state<'all' | 'hold' | 'watch'>('all');

  const filteredHoldings = $derived.by(() => {
    if (filter === 'hold') return investStore.mergedHoldings.filter(h => h.kind === 'hold');
    if (filter === 'watch') return investStore.mergedHoldings.filter(h => h.kind === 'watch');
    return investStore.mergedHoldings;
  });

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
</script>

<div class="overflow-hidden rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)]">
  <!-- Header -->
  <div class="flex items-center justify-between border-b border-[var(--border)] px-[var(--space-4)] py-[var(--space-3)]">
    <h3 class="text-[14px] font-semibold text-[var(--text-primary)]">{t('invest_holdings_value')}</h3>
    <div class="flex items-center gap-[var(--space-2)]">
      <div class="flex gap-[var(--space-1)]">
        <button
          class="rounded-[var(--radius-sm)] px-[10px] py-1 text-[11px] transition-colors {filter === 'all' ? 'bg-[var(--accent-muted)] text-[var(--accent)]' : 'text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-secondary)]'}"
          onclick={() => filter = 'all'}
        >{t('invest_filter_all')}</button>
        <button
          class="rounded-[var(--radius-sm)] px-[10px] py-1 text-[11px] transition-colors {filter === 'hold' ? 'bg-[var(--accent-muted)] text-[var(--accent)]' : 'text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-secondary)]'}"
          onclick={() => filter = 'hold'}
        >{t('invest_status_hold')} ({investStore.holdCount})</button>
        <button
          class="rounded-[var(--radius-sm)] px-[10px] py-1 text-[11px] transition-colors {filter === 'watch' ? 'bg-[var(--accent-muted)] text-[var(--accent)]' : 'text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-secondary)]'}"
          onclick={() => filter = 'watch'}
        >{t('invest_status_watch')} ({investStore.watchCount})</button>
      </div>
      <button
        class="ml-[var(--space-2)] rounded-[var(--radius-md)] border border-[var(--border)] bg-transparent px-[var(--space-3)] py-1 text-[11px] text-[var(--text-tertiary)] transition-colors hover:bg-[var(--bg-hover)] hover:text-[var(--text-secondary)]"
        onclick={onAddWatch}
      >+ {t('invest_add_watch')}</button>
    </div>
  </div>

  {#if filteredHoldings.length === 0}
    <p class="py-[var(--space-4)] text-center text-[12px] text-[var(--text-tertiary)]">{t('invest_no_holdings')}</p>
  {:else}
    <table class="w-full">
      <thead>
        <tr class="border-b border-[var(--border)]">
          <th class="px-[var(--space-4)] py-[var(--space-2)] text-left text-[11px] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_trade_stock')}</th>
          <th class="px-[var(--space-4)] py-[var(--space-2)] text-left text-[11px] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_status')}</th>
          <th class="px-[var(--space-4)] py-[var(--space-2)] text-left text-[11px] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_asset_type')}</th>
          <th class="px-[var(--space-4)] py-[var(--space-2)] text-left text-[11px] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_quantity')}</th>
          <th class="px-[var(--space-4)] py-[var(--space-2)] text-left text-[11px] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_cost_price')}</th>
          <th class="px-[var(--space-4)] py-[var(--space-2)] text-left text-[11px] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_current_price')}</th>
          <th class="px-[var(--space-4)] py-[var(--space-2)] text-left text-[11px] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_pnl_pct')}</th>
          <th class="px-[var(--space-4)] py-[var(--space-2)] text-left text-[11px] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_actions')}</th>
        </tr>
      </thead>
      <tbody>
        {#each filteredHoldings as h}
          {@const pnlPct = h.kind === 'hold' ? getPnlPct(h) : null}
          <tr class="border-b border-[var(--border)] transition-colors last:border-b-0 hover:bg-[var(--bg-hover)]">
            <td class="px-[var(--space-4)] py-[var(--space-3)]">
              <span class="text-[13px] font-semibold text-[var(--text-primary)]" title={h.symbol}>{h.name || h.symbol}</span>
              {#if h.name}
                <span class="ml-[var(--space-2)] text-[11px] font-[var(--font-mono)] text-[var(--text-tertiary)]">{h.symbol}</span>
              {/if}
            </td>
            <td class="px-[var(--space-4)] py-[var(--space-3)]">
              {#if h.kind === 'hold'}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[rgba(138,154,118,0.15)] px-2 py-0.5 text-[10px] font-semibold text-[#8a9a76]">HOLD</span>
              {:else}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[var(--accent-muted)] px-2 py-0.5 text-[10px] font-semibold text-[var(--accent)]">WATCH</span>
              {/if}
            </td>
            <td class="px-[var(--space-4)] py-[var(--space-3)]">
              {#if h.assetType === 'etf'}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[rgba(139,92,246,0.15)] px-2 py-0.5 text-[10px] font-semibold text-[#8b5cf6]">etf</span>
              {:else}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[rgba(59,130,246,0.15)] px-2 py-0.5 text-[10px] font-semibold text-[#3b82f6]">stock</span>
              {/if}
            </td>
            <td class="px-[var(--space-4)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{h.shares ?? '—'}</td>
            <td class="px-[var(--space-4)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{h.avgCost?.toFixed(priceDecimals(h.assetType)) ?? '—'}</td>
            <td class="px-[var(--space-4)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{getPrice(h.symbol)?.toFixed(priceDecimals(h.assetType)) ?? '—'}</td>
            <td class="px-[var(--space-4)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px]">
              {#if pnlPct !== null}
                <span class={pnlPct >= 0 ? 'text-[#8a9a76]' : 'text-[#a87a7a]'}>
                  {pnlPct >= 0 ? '+' : ''}{pnlPct.toFixed(2)}%
                </span>
              {:else}
                <span class="text-[var(--text-tertiary)]">—</span>
              {/if}
            </td>
            <td class="px-[var(--space-4)] py-[var(--space-3)]">
              <button
                class="rounded-[var(--radius-md)] border border-[var(--border)] bg-transparent px-[8px] py-[2px] text-[11px] font-medium text-[var(--text-secondary)] transition-colors hover:bg-[var(--bg-hover)]"
                onclick={() => onEdit(h)}
              >{t('invest_edit')}</button>
              {#if h.kind === 'hold'}
                <button
                  class="ml-[4px] rounded-[var(--radius-md)] border border-[rgba(168,122,122,0.3)] bg-transparent px-[8px] py-[2px] text-[11px] font-medium text-[#a87a7a] transition-colors hover:bg-[rgba(168,122,122,0.1)]"
                  onclick={() => onSell(h)}
                >{t('invest_sell')}</button>
              {:else}
                <button
                  class="ml-[4px] rounded-[var(--radius-md)] border border-[rgba(138,154,118,0.3)] bg-[rgba(138,154,118,0.15)] px-[8px] py-[2px] text-[11px] font-medium text-[#8a9a76] transition-colors hover:bg-[rgba(138,154,118,0.25)]"
                  onclick={() => onConvert(h)}
                >{t('invest_convert_to_hold')}</button>
                <button
                  class="ml-[4px] rounded-[var(--radius-md)] border border-[rgba(168,122,122,0.3)] bg-transparent px-[8px] py-[2px] text-[11px] font-medium text-[#a87a7a] transition-colors hover:bg-[rgba(168,122,122,0.1)]"
                  onclick={() => onDeleteWatch(h)}
                >{t('invest_delete')}</button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
