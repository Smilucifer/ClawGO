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
  import CommitteeLiveTab from '$lib/components/invest/CommitteeLiveTab.svelte';
  import CommitteeReplayTab from '$lib/components/invest/CommitteeReplayTab.svelte';
  import CommitteeArchiveTab from '$lib/components/invest/CommitteeArchiveTab.svelte';
  import CommitteeRolesTab from '$lib/components/invest/CommitteeRolesTab.svelte';
  import CommitteeAccuracyTab from '$lib/components/invest/CommitteeAccuracyTab.svelte';
  import CommitteeToolsTab from '$lib/components/invest/CommitteeToolsTab.svelte';
  import EventWatchTab from '$lib/components/invest/EventWatchTab.svelte';
  import SchedulerTab from '$lib/components/invest/SchedulerTab.svelte';
  import type { Holding } from '$lib/types';

  type InvestTab = 'dashboard' | 'committee' | 'strategy' | 'trades' | 'events' | 'scheduler';
  type CommitteeSubTab = 'live' | 'replay' | 'archive' | 'roles' | 'accuracy' | 'tools';

  let activeTab: InvestTab = $state('dashboard');
  let committeeSubTab: CommitteeSubTab = $state('live');

  const tabs: { id: InvestTab; label: string }[] = $derived([
    { id: 'dashboard', label: t('invest_tab_dashboard') },
    { id: 'committee', label: t('invest_tab_committee') },
    { id: 'strategy', label: t('invest_strategy') },
    { id: 'trades', label: t('invest_trade_log') },
    { id: 'events', label: t('invest_tab_events') },
    { id: 'scheduler', label: t('invest_tab_scheduler') },
  ]);

  const committeeSubTabs: { id: CommitteeSubTab; label: string }[] = $derived([
    { id: 'live', label: t('invest_committee_sub_live') },
    { id: 'replay', label: t('invest_committee_sub_replay') },
    { id: 'archive', label: t('invest_committee_sub_archive') },
    { id: 'roles', label: t('invest_committee_sub_roles') },
    { id: 'accuracy', label: t('invest_committee_sub_accuracy') },
    { id: 'tools', label: t('invest_committee_sub_tools') },
  ]);

  let tushareToken = $state<string>('');
  let dialogMode = $state<'buy' | 'sell' | 'cash' | 'convert' | null>(null);
  let dialogPrefill = $state<{ symbol?: string; name?: string; holding?: Holding } | undefined>();
  let refreshInterval = $state<ReturnType<typeof setInterval> | null>(null);

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  onMount(() => {
    let destroyed = false;

    (async () => {
      try {
        const settings = await invoke<{ tushare_token?: string }>('get_user_settings');
        tushareToken = settings.tushare_token ?? '';
      } catch {}

      if (destroyed) return;

      await investStore.loadAll();

      try {
        const result = await invoke<string>('migrate_legacy_portfolio');
        if (result !== 'no_legacy') {
          console.log('[invest] legacy migration:', result);
        }
      } catch {}

      if (destroyed) return;

      if (tushareToken) {
        investStore.refreshPrices(tushareToken);
        refreshInterval = setInterval(() => {
          investStore.refreshPrices(tushareToken);
        }, 60_000);
      }
    })();

    return () => {
      destroyed = true;
      if (refreshInterval) clearInterval(refreshInterval);
    };
  });

  function openBuy() { dialogMode = 'buy'; dialogPrefill = undefined; }
  function openSell(h: Holding) { dialogMode = 'sell'; dialogPrefill = { symbol: h.symbol, name: h.name ?? undefined, holding: h }; }
  function openCash() { dialogMode = 'cash'; dialogPrefill = undefined; }
  function openConvert(h: Holding) { dialogMode = 'convert'; dialogPrefill = { symbol: h.symbol, name: h.name ?? undefined }; }
  function closeDialog() { dialogMode = null; dialogPrefill = undefined; }
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
      <!-- Sub-tab navigation -->
      <div class="mb-4 flex gap-1 border-b border-border">
        {#each committeeSubTabs as subTab}
          <button
            class="rounded-t-md px-3 py-1.5 text-sm transition-colors"
            class:bg-primary={committeeSubTab === subTab.id}
            class:text-primary-foreground={committeeSubTab === subTab.id}
            class:text-muted-foreground={committeeSubTab !== subTab.id}
            class:hover:bg-muted={committeeSubTab !== subTab.id}
            onclick={() => (committeeSubTab = subTab.id)}
          >
            {subTab.label}
          </button>
        {/each}
      </div>

      {#if committeeSubTab === 'live'}
        <CommitteeLiveTab />
      {:else if committeeSubTab === 'replay'}
        <CommitteeReplayTab />
      {:else if committeeSubTab === 'archive'}
        <CommitteeArchiveTab />
      {:else if committeeSubTab === 'roles'}
        <CommitteeRolesTab />
      {:else if committeeSubTab === 'accuracy'}
        <CommitteeAccuracyTab />
      {:else if committeeSubTab === 'tools'}
        <CommitteeToolsTab />
      {/if}
    {:else if activeTab === 'events'}
      <EventWatchTab onNavigateToCommittee={() => { activeTab = 'committee'; committeeSubTab = 'live'; }} />
    {:else if activeTab === 'scheduler'}
      <SchedulerTab />
    {/if}
  </div>
</div>

{#if dialogMode}
  <TradeDialog mode={dialogMode} prefill={dialogPrefill} {tushareToken} onClose={closeDialog} />
{/if}
