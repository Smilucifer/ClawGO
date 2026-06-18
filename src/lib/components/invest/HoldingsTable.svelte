<script lang="ts">
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { getVerdictBadgeStyle } from '$lib/utils/invest-verdict';
  import { getInvestDate } from '$lib/i18n/format';
  import type { Holding } from '$lib/types';

  let { onBuy, onSell, onAddWatch, onEdit, onConvertToWatch, onDeleteWatch, tushareToken }: {
    onBuy: (h: Holding) => void;
    onSell: (h: Holding) => void;
    onAddWatch: () => void;
    onEdit: (h: Holding) => void;
    onConvertToWatch: (h: Holding) => void;
    onDeleteWatch: (h: Holding) => void;
    tushareToken: string;
  } = $props();

  // Shared button style classes
  const clsAction = 'ml-[4px] rounded-[var(--radius-md)] border px-[8px] py-[2px] text-[11px] font-medium transition-colors';
  const clsBuy = `${clsAction} border-[rgba(138,154,118,0.3)] bg-[rgba(138,154,118,0.15)] text-[#8a9a76] hover:bg-[rgba(138,154,118,0.25)]`;
  const clsSell = `${clsAction} border-[rgba(168,122,122,0.3)] bg-transparent text-[#a87a7a] hover:bg-[rgba(168,122,122,0.1)]`;
  const clsAccent = `${clsAction} border-[var(--accent-muted)] bg-transparent text-[var(--accent)] hover:bg-[var(--accent-muted)]`;

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

  function marketValue(h: Holding): number | null {
    const price = getPrice(h.symbol);
    if (price != null && h.shares) return price * h.shares;
    return h.notional ?? null;
  }

  function pnlAmount(h: Holding): number | null {
    const price = getPrice(h.symbol);
    if (price == null || h.avgCost == null || !h.shares) return null;
    return (price - h.avgCost) * h.shares;
  }

  function dailyPnlAmount(h: Holding): number | null {
    const q = investStore.priceMap[h.symbol];
    if (!q || !h.shares) return null;
    return q.change * h.shares;
  }

  function dailyPnlPct(h: Holding): number | null {
    return investStore.priceMap[h.symbol]?.pctChg ?? null;
  }

  function positionPct(h: Holding): number | null {
    const mv = marketValue(h);
    if (mv == null || investStore.totalAssets <= 0) return null;
    return (mv / investStore.totalAssets) * 100;
  }

  function availableShares(h: Holding): number | null {
    if (h.shares == null) return null;
    return Math.max(0, h.shares - (h.frozenShares ?? 0));
  }

  function todayTraded(sym: string): { buy: number; sell: number } {
    return investStore.todayTradedShares.get(sym) ?? { buy: 0, sell: 0 };
  }

  function isVerdictFresh(createdAt: string | undefined): boolean {
    if (!createdAt) return false;
    const today = getInvestDate(); // returns 'YYYY-MM-DD'
    const todayMs = new Date(today + 'T00:00:00').getTime();
    const createdMs = new Date(createdAt.slice(0, 10) + 'T00:00:00').getTime();
    return (todayMs - createdMs) / (1000 * 60 * 60 * 24) <= 4;
  }
</script>

<div class="overflow-x-auto rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)]">
  <!-- Header -->
  <div class="flex items-center justify-between border-b border-border px-[var(--space-4)] py-[var(--space-3)]">
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
        class="ml-[var(--space-2)] rounded-[var(--radius-md)] border border-border bg-transparent px-[var(--space-3)] py-1 text-[11px] text-[var(--text-tertiary)] transition-colors hover:bg-[var(--bg-hover)] hover:text-[var(--text-secondary)]"
        onclick={onAddWatch}
      >+ {t('invest_add_watch')}</button>
    </div>
  </div>

  {#if filteredHoldings.length === 0}
    <p class="py-[var(--space-4)] text-center text-[12px] text-[var(--text-tertiary)]">{t('invest_no_holdings')}</p>
  {:else}
    <table class="w-full min-w-max">
      <thead>
        <tr class="border-b border-border">
          {#each [
            t('invest_trade_stock'), t('invest_status'), t('invest_asset_type'),
            t('invest_quantity'), t('invest_available_shares'), t('invest_frozen_shares'),
            t('invest_cost_price'), t('invest_current_price'), t('invest_market_value'),
            t('invest_pnl_amount'), t('invest_pnl_pct'), t('invest_daily_pnl'),
            t('invest_position_pct'), t('invest_today_buy'), t('invest_today_sell'),
            t('invest_rating'), t('invest_actions')
          ] as col}
            <th class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-2)] text-left text-[11px] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">{col}</th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each filteredHoldings as h}
          {@const isHold = h.kind === 'hold'}
          {@const price = getPrice(h.symbol)}
          {@const mv = marketValue(h)}
          {@const pnl = isHold ? pnlAmount(h) : null}
          {@const pnlPct = isHold ? getPnlPct(h) : null}
          {@const dPnl = isHold ? dailyPnlAmount(h) : null}
          {@const dPct = isHold ? dailyPnlPct(h) : null}
          {@const posPct = isHold ? positionPct(h) : null}
          {@const avail = isHold ? availableShares(h) : null}
          {@const traded = todayTraded(h.symbol)}
          {@const verdict = investStore.latestVerdictMap.get(h.symbol)}
          {@const dec = priceDecimals(h.assetType)}
          <tr class="border-b border-border transition-colors last:border-b-0 hover:bg-[var(--bg-hover)]">
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)]">
              <span class="text-[13px] font-semibold text-[var(--text-primary)]" title={h.symbol}>{h.name || h.symbol}</span>
              {#if h.name}<span class="ml-[var(--space-2)] text-[11px] font-[var(--font-mono)] text-[var(--text-tertiary)]">{h.symbol}</span>{/if}
            </td>
            <td class="px-[var(--space-3)] py-[var(--space-3)]">
              {#if isHold}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[rgba(138,154,118,0.15)] px-2 py-0.5 text-[10px] font-semibold text-[#8a9a76]">HOLD</span>
              {:else}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[var(--accent-muted)] px-2 py-0.5 text-[10px] font-semibold text-[var(--accent)]">WATCH</span>
              {/if}
            </td>
            <td class="px-[var(--space-3)] py-[var(--space-3)]">
              {#if h.assetType === 'etf'}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[rgba(139,92,246,0.15)] px-2 py-0.5 text-[10px] font-semibold text-[#8b5cf6]">etf</span>
              {:else}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[rgba(59,130,246,0.15)] px-2 py-0.5 text-[10px] font-semibold text-[#3b82f6]">stock</span>
              {/if}
            </td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{h.shares ?? '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{avail ?? '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] {h.frozenShares ? 'text-[#b89a6a]' : 'text-[var(--text-tertiary)]'}">{h.frozenShares ?? '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{h.avgCost?.toFixed(dec) ?? '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{price?.toFixed(dec) ?? '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{mv != null ? '¥' + mv.toFixed(3) : '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px]">
              {#if pnl !== null}<span class={pnl >= 0 ? 'text-[#8a9a76]' : 'text-[#a87a7a]'}>{pnl >= 0 ? '+' : ''}{pnl.toFixed(3)}</span>{:else}<span class="text-[var(--text-tertiary)]">—</span>{/if}
            </td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px]">
              {#if pnlPct !== null}<span class={pnlPct >= 0 ? 'text-[#8a9a76]' : 'text-[#a87a7a]'}>{pnlPct >= 0 ? '+' : ''}{pnlPct.toFixed(2)}%</span>{:else}<span class="text-[var(--text-tertiary)]">—</span>{/if}
            </td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px]">
              {#if dPnl !== null}<span class={dPnl >= 0 ? 'text-[#8a9a76]' : 'text-[#a87a7a]'}>{dPnl >= 0 ? '+' : ''}{dPnl.toFixed(3)}{#if dPct !== null}<span class="ml-1 text-[10px] opacity-70">{dPct >= 0 ? '+' : ''}{dPct.toFixed(2)}%</span>{/if}</span>{:else}<span class="text-[var(--text-tertiary)]">—</span>{/if}
            </td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{posPct != null ? posPct.toFixed(1) + '%' : '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{traded.buy || '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{traded.sell || '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)]">
              {#if verdict && isVerdictFresh(verdict.createdAt)}
                <span class="inline-block rounded-[var(--radius-sm)] px-2 py-0.5 text-[10px] font-semibold" style={getVerdictBadgeStyle(verdict.verdict)} title={'置信度 ' + Math.min(100, (verdict.confidence ?? 0) <= 1 ? (verdict.confidence ?? 0) * 100 : (verdict.confidence ?? 0)).toFixed(0) + '% · ' + verdict.createdAt.slice(0, 10)}>{verdict.verdict}</span>
              {:else}
                <span class="text-[var(--text-tertiary)]">—</span>
              {/if}
            </td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)]">
              <button class="{clsAction} border-border text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]" onclick={() => onEdit(h)}>{t('invest_edit')}</button>
              {#if isHold}
                <button class={clsBuy} onclick={() => onBuy(h)}>{t('invest_buy')}</button>
                <button class={clsSell} onclick={() => onSell(h)}>{t('invest_sell')}</button>
                <button class={clsAccent} onclick={() => onConvertToWatch(h)}>{t('invest_convert_to_watch')}</button>
              {:else}
                <button class={clsBuy} onclick={() => onBuy(h)}>{t('invest_convert_to_hold')}</button>
                <button class={clsSell} onclick={() => onDeleteWatch(h)}>{t('invest_delete_watch')}</button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
