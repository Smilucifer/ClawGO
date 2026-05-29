<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { getTransport } from '$lib/transport';
  import KpiCard from '$lib/components/invest/KpiCard.svelte';
  import HoldingsTable from '$lib/components/invest/HoldingsTable.svelte';
  import TradeDialog from '$lib/components/invest/TradeDialog.svelte';
  import TradeLogTab from '$lib/components/invest/TradeLogTab.svelte';
  import StrategyTab from '$lib/components/invest/StrategyTab.svelte';
  import PnlChart from '$lib/components/invest/PnlChart.svelte';
  import ProviderConfigPanel from '$lib/components/invest/ProviderConfigPanel.svelte';
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import type { Holding } from '$lib/types';

  type InvestTab = 'dashboard' | 'committee' | 'strategy' | 'trades' | 'events' | 'scheduler';
  let activeTab: InvestTab = $state('dashboard');

  const tabs: { id: InvestTab; label: string }[] = $derived([
    { id: 'dashboard', label: t('invest_tab_dashboard') },
    { id: 'committee', label: t('invest_tab_committee') },
    { id: 'strategy', label: t('invest_strategy') },
    { id: 'trades', label: t('invest_trade_log') },
    { id: 'events', label: t('invest_tab_events') },
    { id: 'scheduler', label: t('invest_tab_scheduler') },
  ]);

  let tushareToken = $state<string>('');
  let dialogMode = $state<'buy' | 'sell' | 'cash' | 'convert' | null>(null);
  let dialogPrefill = $state<{ symbol?: string; name?: string; holding?: Holding } | undefined>();
  let refreshInterval = $state<ReturnType<typeof setInterval> | null>(null);

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  onMount(() => {
    (async () => {
      try {
        const settings = await invoke<{ tushare_token?: string }>('get_user_settings');
        tushareToken = settings.tushare_token ?? '';
      } catch {}

      await investStore.loadAll();

      try {
        const result = await invoke<string>('migrate_legacy_portfolio');
        if (result !== 'no_legacy') {
          console.log('[invest] legacy migration:', result);
        }
      } catch {}

      if (tushareToken) {
        investStore.refreshPrices(tushareToken);
        refreshInterval = setInterval(() => {
          investStore.refreshPrices(tushareToken);
        }, 60_000);
      }
    })();

    return () => {
      if (refreshInterval) clearInterval(refreshInterval);
    };
  });

  function openBuy() { dialogMode = 'buy'; dialogPrefill = undefined; }
  function openSell(h: Holding) { dialogMode = 'sell'; dialogPrefill = { symbol: h.symbol, name: h.name ?? undefined, holding: h }; }
  function openCash() { dialogMode = 'cash'; dialogPrefill = undefined; }
  function openConvert(h: Holding) { dialogMode = 'convert'; dialogPrefill = { symbol: h.symbol, name: h.name ?? undefined }; }
  function closeDialog() { dialogMode = null; dialogPrefill = undefined; }

  // ── Committee ──────────────────────────────────────────────────────────
  let committeeSymbols = $state('');
  let expandedRound = $state<number | null>(null);

  async function runCommitteeAction() {
    const syms = committeeSymbols
      .split(/[\s,]+/)
      .map((s) => s.trim())
      .filter(Boolean);
    if (syms.length === 0) return;
    await investCommitteeStore.runCommittee(syms);
  }
</script>

<div class="flex h-full flex-col">
  <div class="border-b border-border px-4 pt-3">
    <h1 class="mb-3 text-lg font-semibold">{t('nav_invest')}</h1>
    <div class="flex gap-1">
      {#each tabs as tab}
        <button
          class="rounded-t-md px-3 py-1.5 text-sm transition-colors"
          class:bg-primary={activeTab === tab.id}
          class:text-primary-foreground={activeTab === tab.id}
          class:text-muted-foreground={activeTab !== tab.id}
          class:hover:bg-muted={activeTab !== tab.id}
          onclick={() => (activeTab = tab.id)}
        >
          {tab.label}
        </button>
      {/each}
    </div>
  </div>

  <div class="flex-1 overflow-auto p-4">
    {#if activeTab === 'dashboard'}
      {#if !tushareToken}
        <div class="mb-4 rounded-lg border border-dashed p-4 text-center text-sm text-muted-foreground">
          {t('invest_no_token')}
        </div>
      {/if}

      <div class="mb-6 grid grid-cols-2 gap-3 sm:grid-cols-5">
        <KpiCard label={t('invest_total_assets')} value={'¥' + investStore.totalAssets.toLocaleString(undefined, { minimumFractionDigits: 2 })} />
        <KpiCard label={t('invest_holdings_value')} value={'¥' + investStore.holdingsMarketValue.toLocaleString(undefined, { minimumFractionDigits: 2 })} />
        <KpiCard label={t('invest_cash')} value={'¥' + investStore.cash.toLocaleString(undefined, { minimumFractionDigits: 2 })} sub="✎" />
        <KpiCard label={t('invest_total_return')} value={investStore.totalReturnPct.toFixed(2) + '%'} trend={investStore.totalReturnPct >= 0 ? 'up' : 'down'} />
        <KpiCard label={t('invest_position_count')} value={t('invest_hold') + ' ' + investStore.holdCount + ' + ' + t('invest_watch') + ' ' + investStore.watchCount} />
      </div>

      <div class="mb-4 flex gap-2">
        <button class="rounded bg-primary px-4 py-1.5 text-sm text-primary-foreground" onclick={openBuy}>{t('invest_buy')}</button>
        <button class="rounded bg-muted px-4 py-1.5 text-sm" onclick={openCash}>{t('invest_edit_cash')}</button>
        <button class="rounded bg-muted px-4 py-1.5 text-sm" onclick={() => investStore.refreshPrices(tushareToken)}>{t('invest_refresh_prices')}</button>
      </div>

      <HoldingsTable onSell={openSell} onConvert={openConvert} {tushareToken} />

      <div class="mt-6">
        <PnlChart />
      </div>

    {:else if activeTab === 'trades'}
      <TradeLogTab />
    {:else if activeTab === 'strategy'}
      <StrategyTab {tushareToken} />
    {:else if activeTab === 'committee'}
      <ProviderConfigPanel />

      <div class="mb-4 flex items-end gap-2">
        <div class="flex-1">
          <label class="mb-1 block text-sm text-muted-foreground">
            {t('invest_committee_symbols')}
          </label>
          <input
            type="text"
            class="w-full rounded border border-border bg-background px-3 py-1.5 text-sm"
            placeholder="600519.SH, 000001.SZ"
            bind:value={committeeSymbols}
            disabled={investCommitteeStore.running}
          />
        </div>
        <button
          class="rounded bg-primary px-4 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
          disabled={investCommitteeStore.running || !committeeSymbols.trim()}
          onclick={runCommitteeAction}
        >
          {investCommitteeStore.running
            ? t('invest_committee_running')
            : t('invest_committee_run')}
        </button>
      </div>

      {#if investCommitteeStore.runError}
        <div class="mb-4 rounded border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {investCommitteeStore.runError}
        </div>
      {/if}

      {#if investCommitteeStore.results.length > 0}
        <div class="grid gap-3 sm:grid-cols-2">
          {#each investCommitteeStore.results as result (result.symbol)}
            <div class="rounded-lg border border-border p-4">
              <!-- Header -->
              <div class="mb-2 flex items-center justify-between">
                <span class="text-sm font-semibold">{result.symbol}</span>
                <div class="flex items-center gap-1.5">
                  {#if result.converged}
                    <span class="rounded bg-green-100 px-1.5 py-0.5 text-xs font-medium text-green-700 dark:bg-green-900/30 dark:text-green-400">
                      {t('invest_committee_converged')}
                    </span>
                  {/if}
                  {#if result.sentinelOverride}
                    <span class="rounded bg-red-100 px-1.5 py-0.5 text-xs font-medium text-red-700 dark:bg-red-900/30 dark:text-red-400">
                      {t('invest_committee_sentinel')}
                    </span>
                  {/if}
                </div>
              </div>

              <!-- Verdict + Confidence -->
              <div class="mb-2 flex items-baseline gap-3">
                <span
                  class="inline-block rounded px-2 py-0.5 text-sm font-bold {result.finalVerdict === 'BUY' || result.finalVerdict === 'ACCUMULATE'
                    ? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400'
                    : result.finalVerdict === 'TRIM' || result.finalVerdict === 'SELL'
                      ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
                      : result.finalVerdict === 'HOLD'
                        ? 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400'
                        : ''}"
                >
                  {result.finalVerdict}
                </span>
                <span class="text-sm text-muted-foreground">
                  {(result.finalConfidence * 100).toFixed(0)}%
                </span>
              </div>

              <!-- Macro signal -->
              <div class="mb-2 text-xs text-muted-foreground">
                Macro: {result.macroSignal}
                {#if result.macroStrength != null}
                  (strength: {result.macroStrength.toFixed(1)})
                {/if}
              </div>

              <!-- Sanity checks -->
              <div class="mb-2 flex gap-2 text-xs">
                <span class:opacity-50={!result.sanityCheck.gate1Pass}>
                  G1 {result.sanityCheck.gate1Pass ? '✓' : '✗'}
                </span>
                <span class:opacity-50={!result.sanityCheck.gate2Pass}>
                  G2 {result.sanityCheck.gate2Pass ? '✓' : '✗'}
                </span>
                <span class:opacity-50={!result.sanityCheck.gate3Pass}>
                  G3 {result.sanityCheck.gate3Pass ? '✓' : '✗'}
                </span>
                <span class="ml-auto text-muted-foreground">
                  {result.totalTokens} tok / {(result.totalLatencyMs / 1000).toFixed(1)}s
                </span>
              </div>

              <!-- Reasoning -->
              {#if result.reasoning}
                <div class="mb-2 max-h-32 overflow-y-auto whitespace-pre-wrap text-xs text-muted-foreground">
                  {result.reasoning}
                </div>
              {/if}

              <!-- Sanity notes -->
              {#if result.sanityCheck.notes.length > 0}
                <div class="mb-2 rounded bg-muted/50 px-2 py-1 text-xs text-muted-foreground">
                  {#each result.sanityCheck.notes as note}
                    <div>- {note}</div>
                  {/each}
                </div>
              {/if}

              <!-- Sentinel override -->
              {#if result.sentinelOverride}
                <div class="mb-2 rounded bg-red-50 px-2 py-1 text-xs text-red-600 dark:bg-red-900/20 dark:text-red-400">
                  {result.sentinelOverride.reason}
                </div>
              {/if}

              <!-- Round outputs (collapsible) -->
              {#if result.rounds.length > 0}
                <button
                  class="text-xs text-muted-foreground hover:underline"
                  onclick={() => (expandedRound = expandedRound === result.rounds.length ? null : result.rounds.length)}
                >
                  {expandedRound === result.rounds.length
                    ? t('invest_committee_hide_rounds')
                    : t('invest_committee_show_rounds', { count: String(result.rounds.length) })}
                </button>

                {#if expandedRound === result.rounds.length}
                  <div class="mt-2 max-h-60 overflow-y-auto">
                    {#each result.rounds as round}
                      <div class="mb-2 rounded border border-border p-2 text-xs">
                        <div class="mb-1 font-medium">{round.label}</div>
                        <div class="max-h-24 overflow-y-auto whitespace-pre-wrap text-muted-foreground">
                          {round.parsed.rawText}
                        </div>
                        <div class="mt-1 text-muted-foreground">
                          {round.tokensUsed} tok / {(round.latencyMs / 1000).toFixed(1)}s
                        </div>
                      </div>
                    {/each}
                  </div>
                {/if}
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    {:else if activeTab === 'events'}
      <div class="text-muted-foreground">Event Monitor — coming in Phase 3c</div>
    {:else if activeTab === 'scheduler'}
      <div class="text-muted-foreground">Scheduled Tasks — coming in Phase 4</div>
    {/if}
  </div>
</div>

{#if dialogMode}
  <TradeDialog mode={dialogMode} prefill={dialogPrefill} {tushareToken} onClose={closeDialog} />
{/if}
