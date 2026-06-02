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
  import UserProfileSection from '$lib/components/invest/UserProfileSection.svelte';
  import MacroSnapshotCard from '$lib/components/invest/MacroSnapshotCard.svelte';
  import LatestVerdictCard from '$lib/components/invest/LatestVerdictCard.svelte';
  import CommitteeLiveTab from '$lib/components/invest/CommitteeLiveTab.svelte';
  import CommitteeReplayTab from '$lib/components/invest/CommitteeReplayTab.svelte';
  import CommitteeArchiveTab from '$lib/components/invest/CommitteeArchiveTab.svelte';
  import CommitteeRolesTab from '$lib/components/invest/CommitteeRolesTab.svelte';
  import CommitteeAccuracyTab from '$lib/components/invest/CommitteeAccuracyTab.svelte';
  import CommitteeToolsTab from '$lib/components/invest/CommitteeToolsTab.svelte';
  import EventWatchTab from '$lib/components/invest/EventWatchTab.svelte';
  import SchedulerTab from '$lib/components/invest/SchedulerTab.svelte';
  import InsightsFeed from '$lib/components/invest/InsightsFeed.svelte';
  import SystemRegimeTab from '$lib/components/invest/SystemRegimeTab.svelte';
  import SystemDatasourceTab from '$lib/components/invest/SystemDatasourceTab.svelte';
  import SystemPnlHistoryTab from '$lib/components/invest/SystemPnlHistoryTab.svelte';
  import SystemDreamsTab from '$lib/components/invest/SystemDreamsTab.svelte';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import type { Holding } from '$lib/types';

  type InvestTab = 'dashboard' | 'committee' | 'strategy' | 'trades' | 'system';
  type CommitteeSubTab = 'live' | 'replay' | 'archive' | 'roles' | 'accuracy' | 'tools';
  type SystemSubTab = 'cron' | 'regime' | 'events' | 'datasource' | 'pnl_history' | 'insights' | 'dreams' | 'profile';

  let activeTab: InvestTab = $state('dashboard');
  let committeeSubTab: CommitteeSubTab = $state('live');
  let systemSubTab: SystemSubTab = $state('cron');

  const tabs: { id: InvestTab; label: string }[] = $derived([
    { id: 'dashboard', label: t('invest_tab_dashboard') },
    { id: 'committee', label: t('invest_tab_committee') },
    { id: 'strategy', label: t('invest_strategy') },
    { id: 'trades', label: t('invest_trade_log') },
    { id: 'system', label: t('invest_tab_system') },
  ]);

  const systemSubTabs: { id: SystemSubTab; label: string }[] = $derived([
    { id: 'cron', label: t('invest_system_sub_cron') },
    { id: 'regime', label: t('invest_system_sub_regime') },
    { id: 'events', label: t('invest_system_sub_events') },
    { id: 'datasource', label: t('invest_system_sub_datasource') },
    { id: 'pnl_history', label: t('invest_system_sub_pnl_history') },
    { id: 'insights', label: t('invest_system_sub_insights') },
    { id: 'dreams', label: t('invest_system_sub_dreams') },
    { id: 'profile', label: t('invest_system_sub_profile') },
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
  let dialogMode = $state<'buy' | 'sell' | 'cash' | 'convert' | 'add_watch' | 'add_trade' | 'edit_holding' | null>(null);
  let dialogPrefill = $state<{ symbol?: string; name?: string; holding?: Holding } | undefined>();
  let refreshInterval = $state<ReturnType<typeof setInterval> | null>(null);
  let confirmDeleteWatch = $state<{ open: boolean; symbol: string; name: string }>({ open: false, symbol: '', name: '' });
  let initLoading = $state(false);
  let initResult = $state<string | null>(null);
  let initBalance = $state('');

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
  function openAddWatch() { dialogMode = 'add_watch'; dialogPrefill = undefined; }
  function openEditHolding(h: Holding) { dialogMode = 'edit_holding'; dialogPrefill = { symbol: h.symbol, name: h.name ?? undefined, holding: h }; }
  function openDeleteWatch(h: Holding) {
    confirmDeleteWatch = { open: true, symbol: h.symbol, name: h.name ?? h.symbol };
  }
  function closeDialog() { dialogMode = null; dialogPrefill = undefined; }
</script>

<div class="flex h-full flex-col bg-[var(--bg-base)]" data-invest-scope>
  <!-- Header -->
  <div class="border-b border-[var(--border)] px-[var(--space-4)] pt-[var(--space-4)]">
    <h1 class="mb-[var(--space-1)] text-[22px] font-bold text-[var(--text-primary)]">{t('nav_invest')}</h1>
    <p class="mb-[var(--space-3)] text-[12px] text-[var(--text-tertiary)]">openInvest</p>

    <!-- Primary tab navigation -->
    <div class="flex gap-0">
      {#each tabs as tab}
        <button
          class="relative px-[var(--space-3)] pb-[var(--space-2)] text-[12px] tracking-wide uppercase transition-colors"
          class:text-[var(--accent)]={activeTab === tab.id}
          class:text-[var(--text-tertiary)]={activeTab !== tab.id}
          class:hover:text-[var(--text-secondary)]={activeTab !== tab.id}
          onclick={() => (activeTab = tab.id)}
        >
          {tab.label}
          {#if activeTab === tab.id}
            <span class="absolute bottom-0 left-0 h-[2px] w-full rounded-full bg-[var(--accent)]"></span>
          {/if}
        </button>
      {/each}
    </div>
  </div>

  <!-- Content area -->
  <div class="flex-1 overflow-auto p-[var(--space-4)]">
    {#if activeTab === 'dashboard'}
      {#if !tushareToken}
        <div class="mb-[var(--space-4)] rounded-[var(--radius-lg)] border border-dashed border-[var(--border)] bg-[var(--bg-card)] p-[var(--space-4)] text-center text-[13px] text-[var(--text-tertiary)]">
          {t('invest_no_token')}
        </div>
      {/if}

      <div class="mb-[var(--space-6)] grid grid-cols-2 gap-[var(--space-3)] sm:grid-cols-5">
        <KpiCard label={t('invest_total_assets')} value={'¥' + investStore.totalAssets.toLocaleString(undefined, { minimumFractionDigits: 2 })} />
        <KpiCard label={t('invest_holdings_value')} value={'¥' + investStore.holdingsMarketValue.toLocaleString(undefined, { minimumFractionDigits: 2 })} />
        <KpiCard label={t('invest_cash')} value={'¥' + investStore.cash.toLocaleString(undefined, { minimumFractionDigits: 2 })} sub="✎" />
        <KpiCard label={t('invest_total_return')} value={investStore.totalReturnPct.toFixed(2) + '%'} trend={investStore.totalReturnPct >= 0 ? 'up' : 'down'} />
        <KpiCard label={t('invest_position_count')} value={t('invest_hold') + ' ' + investStore.holdCount + ' + ' + t('invest_watch') + ' ' + investStore.watchCount} />
      </div>

      <!-- Macro snapshot + Latest verdict -->
      <div class="mb-[var(--space-4)] grid gap-[var(--space-3)] sm:grid-cols-2">
        <MacroSnapshotCard />
        <LatestVerdictCard />
      </div>

      <div class="mb-[var(--space-4)] flex gap-[var(--space-2)]">
        <button class="rounded-[var(--radius-md)] bg-[var(--accent)] px-[var(--space-4)] py-[var(--space-1)] text-[12px] font-medium text-[var(--bg-base)] transition-colors hover:opacity-90" onclick={openBuy}>{t('invest_buy')}</button>
        <button class="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-4)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] transition-colors hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]" onclick={openCash}>{t('invest_edit_cash')}</button>
        <button class="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-4)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] transition-colors hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]" onclick={() => investStore.refreshPrices(tushareToken)}>{t('invest_refresh_prices')}</button>
      </div>

      <HoldingsTable onSell={openSell} onConvert={openConvert} onAddWatch={openAddWatch} onDeleteWatch={openDeleteWatch} onEdit={openEditHolding} {tushareToken} />

      <div class="mt-[var(--space-6)]">
        <PnlChart />
      </div>

    {:else if activeTab === 'trades'}
      <TradeLogTab {tushareToken} />
    {:else if activeTab === 'strategy'}
      <StrategyTab {tushareToken} />
    {:else if activeTab === 'committee'}
      <!-- Committee sub-tab navigation (pill style) -->
      <div class="mb-[var(--space-4)] flex flex-wrap gap-[var(--space-2)]">
        {#each committeeSubTabs as subTab}
          <button
            class="rounded-full px-[var(--space-3)] py-[var(--space-1)] text-[12px] font-medium transition-colors"
            class:bg-[var(--accent-muted)]={committeeSubTab === subTab.id}
            class:text-[var(--accent)]={committeeSubTab === subTab.id}
            class:bg-[var(--bg-hover)]={committeeSubTab !== subTab.id}
            class:text-[var(--text-tertiary)]={committeeSubTab !== subTab.id}
            class:hover:bg-[var(--accent-muted)]={committeeSubTab !== subTab.id}
            class:hover:text-[var(--accent)]={committeeSubTab !== subTab.id}
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
    {:else if activeTab === 'system'}
      <!-- System sub-tab navigation (pill style) -->
      <div class="mb-[var(--space-4)] flex flex-wrap items-center gap-[var(--space-2)]">
        {#each systemSubTabs as subTab}
          <button
            class="rounded-full px-[var(--space-3)] py-[var(--space-1)] text-[12px] font-medium transition-colors"
            class:bg-[var(--accent-muted)]={systemSubTab === subTab.id}
            class:text-[var(--accent)]={systemSubTab === subTab.id}
            class:bg-[var(--bg-hover)]={systemSubTab !== subTab.id}
            class:text-[var(--text-tertiary)]={systemSubTab !== subTab.id}
            class:hover:bg-[var(--accent-muted)]={systemSubTab !== subTab.id}
            class:hover:text-[var(--accent)]={systemSubTab !== subTab.id}
            onclick={() => (systemSubTab = subTab.id)}
          >
            {subTab.label}
          </button>
        {/each}
      </div>

      <!-- Data initialization section -->
      <div class="mb-[var(--space-4)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] p-[var(--space-4)]">
        <div class="flex items-center justify-between">
          <div>
            <div class="text-[13px] font-semibold text-[var(--text-primary)]">{t('invest_data_init')}</div>
            <div class="text-[11px] text-[var(--text-tertiary)]">{t('invest_data_init_desc')}</div>
          </div>
          <div class="flex items-center gap-2">
            <input
              type="number"
              placeholder={t('invest_data_init_balance')}
              bind:value={initBalance}
              class="w-36 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-[var(--space-2)] py-[var(--space-1)] text-[12px] text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)]"
            />
            <button
              class="rounded-[var(--radius-md)] bg-[var(--accent)] px-[var(--space-4)] py-[var(--space-1)] text-[12px] font-medium text-[var(--bg-base)] transition-colors hover:opacity-90 disabled:opacity-40"
              disabled={!tushareToken || initLoading}
              onclick={async () => {
                initLoading = true;
                initResult = null;
                try {
                  const bal = initBalance ? parseFloat(initBalance) : undefined;
                  initResult = await investStore.initInvestData(tushareToken, bal);
                  if (!initResult.startsWith('Err')) initBalance = '';
                } catch (e) {
                  initResult = String(e);
                } finally {
                  initLoading = false;
                }
              }}
            >
              {initLoading ? '...' : t('invest_data_init_btn')}
            </button>
          </div>
        </div>
        {#if initResult !== null}
          <p class="mt-2 text-[11px] text-[var(--text-secondary)]">{initResult}</p>
        {/if}
      </div>

      {#if systemSubTab === 'cron'}
        <SchedulerTab />
      {:else if systemSubTab === 'regime'}
        <SystemRegimeTab />
      {:else if systemSubTab === 'events'}
        <EventWatchTab onNavigateToCommittee={() => { activeTab = 'committee'; committeeSubTab = 'live'; }} />
      {:else if systemSubTab === 'datasource'}
        <SystemDatasourceTab />
      {:else if systemSubTab === 'pnl_history'}
        <SystemPnlHistoryTab />
      {:else if systemSubTab === 'insights'}
        <InsightsFeed />
      {:else if systemSubTab === 'dreams'}
        <SystemDreamsTab />
      {:else if systemSubTab === 'profile'}
        <UserProfileSection />
      {/if}
    {/if}
  </div>
</div>

{#if dialogMode}
  <TradeDialog mode={dialogMode} prefill={dialogPrefill} {tushareToken} onClose={closeDialog} />
{/if}

<ConfirmDialog
  bind:open={confirmDeleteWatch.open}
  title={t('invest_delete')}
  message={`${t('invest_delete')} ${confirmDeleteWatch.name}?`}
  variant="danger"
  onConfirm={() => { investStore.deleteWatch(confirmDeleteWatch.symbol); }}
/>
