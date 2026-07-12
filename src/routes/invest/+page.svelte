<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { getTransport } from '$lib/transport';
  import KpiCard from '$lib/components/invest/KpiCard.svelte';
  import { formatYuan } from '$lib/utils/format';
  import HoldingsTable from '$lib/components/invest/HoldingsTable.svelte';
  import TradeDialog from '$lib/components/invest/TradeDialog.svelte';
  import TradeLogTab from '$lib/components/invest/TradeLogTab.svelte';
  import StrategyTab from '$lib/components/invest/StrategyTab.svelte';
  import PnlChart from '$lib/components/invest/PnlChart.svelte';
  import UserProfileSection from '$lib/components/invest/UserProfileSection.svelte';
  import CommitteeLiveTab from '$lib/components/invest/CommitteeLiveTab.svelte';
  import CommitteeReplayTab from '$lib/components/invest/CommitteeReplayTab.svelte';
  import CommitteeArchiveTab from '$lib/components/invest/CommitteeArchiveTab.svelte';
  import CommitteeRolesTab from '$lib/components/invest/CommitteeRolesTab.svelte';
  import CommitteeAccuracyTab from '$lib/components/invest/CommitteeAccuracyTab.svelte';
  import EventWatchTab from '$lib/components/invest/EventWatchTab.svelte';
  import PremarketReportTab from '$lib/components/invest/PremarketReportTab.svelte';
  import SchedulerTab from '$lib/components/invest/SchedulerTab.svelte';
  import InsightsFeed from '$lib/components/invest/InsightsFeed.svelte';
  import SystemDatasourceTab from '$lib/components/invest/SystemDatasourceTab.svelte';
  import SystemPnlHistoryTab from '$lib/components/invest/SystemPnlHistoryTab.svelte';
  import SystemDreamsTab from '$lib/components/invest/SystemDreamsTab.svelte';
  import SystemCleanupTab from '$lib/components/invest/SystemCleanupTab.svelte';
  import FortuneAnalysisTab from '$lib/components/invest/FortuneAnalysisTab.svelte';
  import FortuneStemBranchTab from '$lib/components/invest/FortuneStemBranchTab.svelte';
  import FortuneDataTab from '$lib/components/invest/FortuneDataTab.svelte';
  import type { Holding } from '$lib/types';

  type InvestTab = 'dashboard' | 'committee' | 'strategy' | 'trades' | 'system' | 'fortune';
  type CommitteeSubTab = 'live' | 'replay' | 'archive' | 'news' | 'roles' | 'accuracy' | 'premarket';
  type SystemSubTab = 'cron' | 'datasource' | 'pnl_history' | 'insights' | 'dreams' | 'profile' | 'cleanup';
  type FortuneSubTab = 'analysis' | 'stembranch' | 'data';

  let activeTab: InvestTab = $state('dashboard');
  let committeeSubTab: CommitteeSubTab = $state('live');
  let systemSubTab: SystemSubTab = $state('cron');
  let fortuneSubTab: FortuneSubTab = $state('analysis');

  const tabs: { id: InvestTab; label: string }[] = $derived([
    { id: 'dashboard', label: t('invest_tab_dashboard') },
    { id: 'committee', label: t('invest_tab_committee') },
    { id: 'strategy', label: t('invest_strategy') },
    { id: 'trades', label: t('invest_trade_log') },
    { id: 'system', label: t('invest_tab_system') },
    { id: 'fortune', label: t('invest_tab_fortune') },
  ]);

  const systemSubTabs: { id: SystemSubTab; label: string }[] = $derived([
    { id: 'cron', label: t('invest_system_sub_cron') },
    { id: 'datasource', label: t('invest_system_sub_datasource') },
    { id: 'pnl_history', label: t('invest_system_sub_pnl_history') },
    { id: 'insights', label: t('invest_system_sub_insights') },
    { id: 'dreams', label: t('invest_system_sub_dreams') },
    { id: 'profile', label: t('invest_system_sub_profile') },
    { id: 'cleanup', label: t('invest_system_sub_cleanup') },
  ]);

  const committeeSubTabs: { id: CommitteeSubTab; label: string }[] = $derived([
    { id: 'live', label: t('invest_committee_sub_live') },
    { id: 'replay', label: t('invest_committee_sub_replay') },
    { id: 'archive', label: t('invest_committee_sub_archive') },
    { id: 'news', label: t('invest_committee_sub_news') },
    { id: 'roles', label: t('invest_committee_sub_roles') },
    { id: 'accuracy', label: t('invest_committee_sub_accuracy') },
    { id: 'premarket', label: t('invest_committee_sub_premarket') },
  ]);

  const fortuneSubTabs: { id: FortuneSubTab; label: string }[] = $derived([
    { id: 'analysis', label: t('fortune_sub_analysis') },
    { id: 'stembranch', label: t('fortune_sub_stembranch') },
    { id: 'data', label: t('fortune_sub_data') },
  ]);

  let tushareToken = $state<string>('');
  let dialogMode = $state<'buy' | 'sell' | 'cash' | 'add_watch' | 'edit_holding' | null>(null);
  let dialogPrefill = $state<{ symbol?: string; name?: string; holding?: Holding } | undefined>();
  let refreshInterval = $state<ReturnType<typeof setInterval> | null>(null);
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
      } catch {
        // 读取设置失败时静默降级：tushareToken 保持空串，后续按未配置处理
      }

      if (destroyed) return;

      await investStore.loadAll();

      try {
        const result = await invoke<string>('migrate_legacy_portfolio');
        if (result !== 'no_legacy') {
          console.log('[invest] legacy migration:', result);
        }
      } catch {
        // 历史数据迁移失败不阻塞页面加载：无 legacy 数据或迁移异常均静默跳过
      }

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
  function openBuyFromHolding(h: Holding) { dialogMode = 'buy'; dialogPrefill = { symbol: h.symbol, name: h.name ?? undefined, holding: h }; }
  function openSell(h: Holding) { dialogMode = 'sell'; dialogPrefill = { symbol: h.symbol, name: h.name ?? undefined, holding: h }; }
  function openCash() { dialogMode = 'cash'; dialogPrefill = undefined; }
  function openAddWatch() { dialogMode = 'add_watch'; dialogPrefill = undefined; }
  function openEditHolding(h: Holding) { dialogMode = 'edit_holding'; dialogPrefill = { symbol: h.symbol, name: h.name ?? undefined, holding: h }; }
  function closeDialog() { dialogMode = null; dialogPrefill = undefined; }
  /** Convert a HOLD to Watch: sell all shares first (auto-converts to watch), or delete if no shares. */
  function convertToWatch(h: Holding) {
    if (h.shares && h.shares > 0) {
      // Open sell dialog — selling all shares auto-converts to watch
      openSell(h);
    } else {
      // No shares — delete directly; user can re-add to watch
      investStore.deleteWatch(h.symbol).catch(e => console.error('[invest] convertToWatch:', e));
    }
  }
  function deleteWatchFromTable(h: Holding) {
    investStore.deleteWatch(h.symbol).catch(e => console.error('[invest] deleteWatch:', e));
  }
</script>

<div class="flex h-full flex-col bg-[var(--bg-base)]" data-invest-scope>
  <!-- Header -->
  <div class="border-b border-border px-[var(--space-4)] pt-[var(--space-4)]">
    <h1 class="mb-[var(--space-1)] text-[22px] font-bold text-[var(--text-primary)]">{t('nav_invest')}</h1>
    <p class="mb-[var(--space-3)] text-[12px] text-[var(--text-tertiary)]">openInvest</p>
    <p class="mb-[var(--space-2)] text-[11px] text-[var(--text-tertiary)]">📅 {t('invest_date_rule')}</p>

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
  <div class="min-h-0 flex-1 overflow-auto p-[var(--space-4)]">
    {#if activeTab === 'dashboard'}
      {#if !tushareToken}
        <div class="mb-[var(--space-4)] rounded-[var(--radius-lg)] border border-dashed border-border bg-[var(--bg-card)] p-[var(--space-4)] text-center text-[13px] text-[var(--text-tertiary)]">
          {t('invest_no_token')}
        </div>
      {/if}

      <div class="mb-[var(--space-6)] grid grid-cols-2 gap-[var(--space-3)] sm:grid-cols-3 lg:grid-cols-6">
        <KpiCard label={t('invest_total_assets')} value={formatYuan(investStore.totalAssets)} />
        <KpiCard label={t('invest_holdings_value')} value={formatYuan(investStore.holdingsMarketValue)} />
        <KpiCard label={t('invest_cash')} value={formatYuan(investStore.cash)} sub="✎" />
        <KpiCard
          label={t('invest_total_return')}
          value={formatYuan(investStore.holdingsMarketValue - investStore.totalCostBasis, { signed: true })}
          sub={(investStore.totalReturnPct >= 0 ? '+' : '') + investStore.totalReturnPct.toFixed(3) + '%'}
          trend={investStore.totalReturnPct >= 0 ? 'up' : 'down'}
        />
        <KpiCard
          label={t('invest_daily_return')}
          value={formatYuan(investStore.dailyPnl, { signed: true })}
          sub={(investStore.dailyPnlPct >= 0 ? '+' : '') + investStore.dailyPnlPct.toFixed(3) + '%'}
          trend={investStore.dailyPnl >= 0 ? 'up' : 'down'}
        />
        <KpiCard label={t('invest_position_count')} value={t('invest_hold') + ' ' + investStore.holdCount + ' + ' + t('invest_watch') + ' ' + investStore.watchCount} />
      </div>

      <div class="mb-[var(--space-4)] flex gap-[var(--space-2)]">
        <button class="rounded-[var(--radius-md)] bg-[var(--accent)] px-[var(--space-4)] py-[var(--space-1)] text-[12px] font-medium text-[var(--bg-base)] transition-colors hover:opacity-90" onclick={openBuy}>{t('invest_buy')}</button>
        <button class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-card)] px-[var(--space-4)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] transition-colors hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]" onclick={openCash}>{t('invest_cash_management')}</button>
        <button class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-card)] px-[var(--space-4)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] transition-colors hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]" onclick={() => investStore.refreshPrices(tushareToken)}>{t('invest_refresh_prices')}</button>
      </div>

      <HoldingsTable onBuy={openBuyFromHolding} onSell={openSell} onAddWatch={openAddWatch} onEdit={openEditHolding} onConvertToWatch={convertToWatch} onDeleteWatch={deleteWatchFromTable} />

      <div class="mt-[var(--space-6)]">
        <PnlChart />
      </div>

    {:else if activeTab === 'trades'}
      <TradeLogTab {tushareToken} />
    {:else if activeTab === 'strategy'}
      <StrategyTab />
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
      {:else if committeeSubTab === 'news'}
        <EventWatchTab onNavigateToCommittee={() => { committeeSubTab = 'live'; }} />
      {:else if committeeSubTab === 'roles'}
        <CommitteeRolesTab />
      {:else if committeeSubTab === 'accuracy'}
        <CommitteeAccuracyTab />
      {:else if committeeSubTab === 'premarket'}
        <PremarketReportTab />
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
      <div class="mb-[var(--space-4)] rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
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
              class="w-36 rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-[var(--space-2)] py-[var(--space-1)] text-[12px] text-[var(--text-primary)] placeholder:text-[var(--text-tertiary)]"
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
      {:else if systemSubTab === 'cleanup'}
        <SystemCleanupTab />
      {/if}
    {:else if activeTab === 'fortune'}
      <div class="mb-[var(--space-4)] flex flex-wrap items-center gap-[var(--space-2)]">
        {#each fortuneSubTabs as subTab}
          <button
            class="rounded-full px-[var(--space-3)] py-[var(--space-1)] text-[12px] font-medium transition-colors"
            class:bg-[var(--accent-muted)]={fortuneSubTab === subTab.id}
            class:text-[var(--accent)]={fortuneSubTab === subTab.id}
            class:text-[var(--text-tertiary)]={fortuneSubTab !== subTab.id}
            onclick={() => (fortuneSubTab = subTab.id)}
          >{subTab.label}</button>
        {/each}
      </div>
      {#if fortuneSubTab === 'analysis'}
        <FortuneAnalysisTab />
      {:else if fortuneSubTab === 'stembranch'}
        <FortuneStemBranchTab />
      {:else if fortuneSubTab === 'data'}
        <FortuneDataTab />
      {/if}
    {/if}
  </div>
</div>

{#if dialogMode}
  <TradeDialog mode={dialogMode} prefill={dialogPrefill} {tushareToken} onClose={closeDialog} />
{/if}

