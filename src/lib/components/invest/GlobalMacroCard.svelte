<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import type { GlobalMacroState } from '$lib/stores/invest-committee-store.svelte';

  let { macro, onRefresh }: { macro: GlobalMacroState; onRefresh: () => void } = $props();

  const s = $derived(macro.snapshot);
  const v = $derived(macro.verdict);
  const meClass = $derived(v?.moneyEffect ?? '');            // hot|active|calm|cold|''
  const adv = $derived(s?.advanceCount ?? 0);
  const dec = $derived(s?.declineCount ?? 0);
  const sentiment = $derived(adv + dec > 0 ? (adv / (adv + dec)) * 100 : null);

  // 赚钱效应 i18n：固定 key 映射，避免动态模板字面量类型错误
  const ME_LABELS = {
    hot: 'invest_money_effect_hot',
    active: 'invest_money_effect_active',
    calm: 'invest_money_effect_calm',
    cold: 'invest_money_effect_cold',
  } as const;
  type MeKey = keyof typeof ME_LABELS;

  const meLabel = $derived(
    macro.status !== 'ready' || !v?.moneyEffect
      ? t('invest_money_effect_nodata')
      : t(ME_LABELS[v.moneyEffect as MeKey] ?? 'invest_money_effect_nodata'),
  );
  const num = (x: number | null | undefined, d = 0) =>
    x == null ? '—' : x.toLocaleString(undefined, { maximumFractionDigits: d });
</script>

<div class="macro-snapshot" data-invest-scope>
  <div class="macro-header">
    <span class="macro-title">
      {t('invest_macro_snapshot')}
      {#if v?.signal}· <span class="chip sig-{v.signal}">{v.signal}</span>{/if}
      {#if v?.strength != null}· {t('invest_macro_strength')} {v.strength.toFixed(1)}{/if}
    </span>
    <button class="macro-refresh" onclick={onRefresh} disabled={macro.refreshing}>
      {macro.refreshing ? t('invest_macro_verdict_analyzing') : t('invest_macro_refresh')}
    </button>
  </div>

  {#if macro.status === 'empty'}
    <p class="macro-empty">{t('invest_macro_no_data')} · {t('invest_macro_refresh')}</p>
  {:else}
    <!-- 宏观指标 5 格 -->
    <div class="macro-sub">{t('invest_macro_indicators')}</div>
    <div class="macro-grid">
      <div class="macro-cell"><span>{t('invest_macro_sh_composite_close')}</span><b>{num(s?.shCompositeClose, 2)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_northbound_net')}</span><b>{num(s?.northboundNet, 1)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_vix')}</span><b>{num(s?.vix, 2)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_gold')}</span><b>{num(s?.gold, 1)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_two_market_volume')}</span><b>{num(s?.twoMarketVolume, 0)}</b></div>
    </div>
    <!-- 市场广度 5 格 -->
    <div class="macro-sub">{t('invest_macro_breadth')}</div>
    <div class="macro-grid">
      <div class="macro-cell"><span>{t('invest_macro_apd')}</span>
        <span class="apd"><span class="u">{num(adv)}</span><span class="sep">-</span><span class="p">{num(s?.flatCount)}</span><span class="sep">-</span><span class="d">{num(dec)}</span></span></div>
      <div class="macro-cell"><span>{t('invest_macro_limit_up_count')}</span><b class="u">{num(s?.limitUpCount)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_limit_down_count')}</span><b class="d">{num(s?.limitDownCount)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_up_over_3pct')}</span><b class="u">{num(s?.upOver3pctCount)}</b></div>
      <div class="macro-cell"><span>{t('invest_macro_sentiment_pct')}</span><b>{sentiment == null ? '—' : sentiment.toFixed(0) + '%'}</b></div>
    </div>
    <!-- 赚钱效应带 -->
    <div class="money-strip">
      <span class="me-title">{t('invest_money_effect')}</span>
      <span class="me-badge {meClass}">{meLabel}</span>
      <span class="me-reason">
        {#if macro.status === 'stale'}{t('invest_macro_verdict_stale')}
        {:else if macro.status === 'analyzing'}{t('invest_macro_verdict_analyzing')}
        {:else}{v?.moneyEffectReason ?? ''}{/if}
      </span>
    </div>
  {/if}
</div>

<style>
  .macro-snapshot { border: 1px solid var(--border); background: var(--bg-card);
    border-radius: var(--radius-lg); padding: var(--space-4); display: flex; flex-direction: column; gap: var(--space-2); }
  .macro-header { display: flex; justify-content: space-between; align-items: center; }
  .macro-title { font-size: 13px; color: var(--text-secondary); }
  .macro-refresh { font-size: 11px; padding: 2px 10px; border-radius: var(--radius-sm);
    border: 1px solid var(--border); background: var(--bg-input); color: var(--text-secondary); cursor: pointer; }
  .macro-refresh:disabled { opacity: 0.5; cursor: default; }
  .macro-sub { font-size: 10px; text-transform: uppercase; color: var(--text-tertiary); margin-top: var(--space-1); }
  .macro-grid { display: grid; grid-template-columns: repeat(5, 1fr); gap: var(--space-2); }
  .macro-cell { display: flex; flex-direction: column; gap: 2px; padding: var(--space-2);
    background: var(--bg-hover); border-radius: var(--radius-sm); }
  .macro-cell span { font-size: 10px; color: var(--text-tertiary); }
  .macro-cell b { font-family: var(--font-mono); font-size: 14px; color: var(--text-primary); }
  .macro-cell b.u, .apd .u { color: var(--up); }
  .macro-cell b.d, .apd .d { color: var(--down); }
  .apd { display: inline-flex; gap: 2px; font-family: var(--font-mono); font-size: 13px; font-weight: 600; }
  .apd .p { color: var(--flat); } .apd .sep { color: var(--text-tertiary); }
  .money-strip { display: flex; align-items: center; gap: 10px; padding-top: var(--space-2);
    border-top: 1px solid var(--border); margin-top: var(--space-1); }
  .me-title { font-size: 10px; text-transform: uppercase; color: var(--text-tertiary); }
  .me-badge { font-size: 12px; font-weight: 700; padding: 2px 10px; border-radius: var(--radius-sm); }
  .me-badge.hot { color: var(--up); background: rgba(197,111,98,0.18); }
  .me-badge.active { color: var(--accent); background: var(--accent-subtle); }
  .me-badge.calm { color: var(--flat); background: rgba(158,154,150,0.14); }
  .me-badge.cold { color: var(--down); background: rgba(127,157,109,0.18); }
  .me-reason { font-size: 12px; color: var(--text-secondary); }
  .macro-empty { font-size: 12px; color: var(--text-tertiary); }
  .chip { font-size: 11px; padding: 1px 8px; border-radius: var(--radius-sm); }
  .chip.sig-risk_on { background: rgba(197,111,98,0.15); color: var(--up); }
  .chip.sig-risk_off { background: rgba(127,157,109,0.2); color: var(--down); }
  .chip.sig-neutral { background: rgba(196,169,110,0.15); color: var(--accent); }
</style>
