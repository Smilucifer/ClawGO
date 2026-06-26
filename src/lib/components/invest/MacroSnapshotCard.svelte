<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import type { MessageKey } from '$lib/i18n/types';
  import type { MacroSnapshot } from '$lib/stores/invest-committee-store.svelte';

  let { snapshot }: { snapshot: MacroSnapshot | null } = $props();

  interface IndicatorDef {
    key: keyof MacroSnapshot;
    i18n: MessageKey;
    fmt: (v: number) => string;
  }

  const INDICATORS: IndicatorDef[] = [
    { key: 'shCompositeClose', i18n: 'invest_macro_sh_composite_close', fmt: v => v.toFixed(2) },
    { key: 'shCompositeVol20', i18n: 'invest_macro_sh_composite_vol20', fmt: v => v.toFixed(2) + '%' },
    { key: 'northboundNet',    i18n: 'invest_macro_northbound_net',      fmt: v => v.toFixed(2) },
    { key: 'vix',              i18n: 'invest_macro_vix',                 fmt: v => v.toFixed(2) },
    { key: 'gold',             i18n: 'invest_macro_gold',                fmt: v => v.toFixed(2) },
    { key: 'advanceCount',     i18n: 'invest_macro_advance_count',       fmt: v => v.toFixed(0) },
    { key: 'declineCount',     i18n: 'invest_macro_decline_count',       fmt: v => v.toFixed(0) },
    { key: 'twoMarketVolume',  i18n: 'invest_macro_two_market_volume',   fmt: v => v.toFixed(2) },
    { key: 'limitUpCount',     i18n: 'invest_macro_limit_up_count',      fmt: v => v.toFixed(0) },
    { key: 'limitDownCount',   i18n: 'invest_macro_limit_down_count',    fmt: v => v.toFixed(0) },
  ];

  function fmtVal(def: IndicatorDef, snap: MacroSnapshot): string {
    const v = snap[def.key];
    return v != null ? def.fmt(v) : 'N/A';
  }
</script>

{#if snapshot}
  <div class="macro-snapshot">
    <div class="macro-title">{t('invest_macro_snapshot')}</div>
    <div class="macro-grid">
      {#each INDICATORS as ind}
        <div class="macro-cell">
          <span class="macro-label">{t(ind.i18n)}</span>
          <span class="macro-value">{fmtVal(ind, snapshot)}</span>
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .macro-snapshot {
    margin-top: var(--space-3);
    padding: var(--space-3);
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--bg-card);
  }

  .macro-title {
    margin-bottom: var(--space-2);
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-tertiary);
  }

  .macro-grid {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: var(--space-2);
    text-align: center;
  }

  .macro-cell {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-1) 0;
  }

  .macro-label {
    font-size: 10px;
    color: var(--text-tertiary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .macro-value {
    font-size: 13px;
    font-weight: 600;
    font-family: var(--font-mono);
    color: var(--text-primary);
  }
</style>
