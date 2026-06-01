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

<div class="space-y-[var(--space-3)]">
  <h3 class="text-sm font-medium text-[var(--text-primary)]">{t('invest_system_pnl_history_title')}</h3>

  {#if loading}
    <p class="text-sm text-[var(--text-secondary)]">{t('invest_loading')}</p>
  {:else if error}
    <p class="text-sm text-[var(--color-error)]">{error}</p>
  {:else if snapshots.length === 0}
    <p class="text-sm text-[var(--text-secondary)]">{t('invest_system_pnl_history_empty')}</p>
  {:else}
    <div class="overflow-x-auto rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)]">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b border-[var(--border)] text-left text-[var(--text-tertiary)]">
            <th class="text-[11px] font-medium uppercase tracking-wider pb-[var(--space-2)] pr-[var(--space-3)]">{t('invest_system_pnl_date')}</th>
            <th class="text-[11px] font-medium uppercase tracking-wider pb-[var(--space-2)] pr-[var(--space-3)] text-right">{t('invest_system_pnl_total')}</th>
            <th class="text-[11px] font-medium uppercase tracking-wider pb-[var(--space-2)] pr-[var(--space-3)] text-right">{t('invest_system_pnl_cash')}</th>
            <th class="text-[11px] font-medium uppercase tracking-wider pb-[var(--space-2)] pr-[var(--space-3)] text-right">{t('invest_system_pnl_holdings')}</th>
            <th class="text-[11px] font-medium uppercase tracking-wider pb-[var(--space-2)] pr-[var(--space-3)] text-right">{t('invest_system_pnl_daily')}</th>
            <th class="text-[11px] font-medium uppercase tracking-wider pb-[var(--space-2)] pr-[var(--space-3)] text-right">{t('invest_system_pnl_daily_pct')}</th>
            <th class="text-[11px] font-medium uppercase tracking-wider pb-[var(--space-2)]">{t('invest_actions')}</th>
          </tr>
        </thead>
        <tbody>
          {#each snapshots as snap}
            <tr class="border-b border-[var(--border)]">
              <td class="py-[var(--space-2)] pr-[var(--space-3)] text-[var(--text-primary)] font-[var(--font-mono)]">{formatDate(snap.snapshotDate)}</td>
              <td class="py-[var(--space-2)] pr-[var(--space-3)] text-right text-[var(--text-primary)] font-[var(--font-mono)]">¥{snap.totalValue.toLocaleString(undefined, { minimumFractionDigits: 2 })}</td>
              <td class="py-[var(--space-2)] pr-[var(--space-3)] text-right text-[var(--text-primary)] font-[var(--font-mono)]">¥{snap.cash.toLocaleString(undefined, { minimumFractionDigits: 2 })}</td>
              <td class="py-[var(--space-2)] pr-[var(--space-3)] text-right text-[var(--text-primary)] font-[var(--font-mono)]">¥{snap.holdingsValue.toLocaleString(undefined, { minimumFractionDigits: 2 })}</td>
              <td class="py-[var(--space-2)] pr-[var(--space-3)] text-right font-[var(--font-mono)]" class:text-[var(--color-success)]={(snap.dailyPnl ?? 0) > 0} class:text-[var(--color-error)]={(snap.dailyPnl ?? 0) < 0}>
                {fmtPnl(snap.dailyPnl)}
              </td>
              <td class="py-[var(--space-2)] pr-[var(--space-3)] text-right font-[var(--font-mono)]" class:text-[var(--color-success)]={(snap.dailyPnlPct ?? 0) > 0} class:text-[var(--color-error)]={(snap.dailyPnlPct ?? 0) < 0}>
                {fmtPct(snap.dailyPnlPct)}
              </td>
              <td class="py-[var(--space-2)]">
                <button
                  class="rounded-[var(--radius-md)] px-[var(--space-2)] py-[var(--space-1)] text-[11px] font-medium text-[var(--color-error)] hover:bg-[rgba(168,122,122,0.1)]"
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
