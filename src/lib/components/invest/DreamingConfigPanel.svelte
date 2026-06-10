<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';
  import { investStatusTextClass } from '$lib/utils/invest-status';

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  /** Either 'invest' or 'user_memory' — determines which dream mode to control. */
  let { path = 'invest' }: { path?: 'invest' | 'user_memory' } = $props();

  interface DreamConfig {
    investEnabled: boolean;
    investCron: string;
    userMemoryEnabled: boolean;
    userMemoryIntervalMin: number;
    lookbackDays: number;
    minScore: number;
    minCount: number;
  }

  interface StageResult {
    stage: string;
    durationMs: number;
    itemsProcessed: number;
    itemsOutput: number;
  }

  interface DreamResult {
    insightsWritten: number;
    insightsUpdated: number;
    insightsArchived: number;
    pipelineDurationMs: number;
    stages: StageResult[];
  }

  interface DreamSnapshot {
    id: number;
    dreamType: string;
    triggerType: string;
    beforeJson: string;
    afterJson?: string;
    status: string;
    summary?: string;
    rollbackReady: boolean;
    createdAt: string;
  }

  let config = $state<DreamConfig | null>(null);
  let traces = $state<DreamSnapshot[]>([]);
  let loading = $state(false);
  let saving = $state(false);
  let triggering = $state(false);
  let rollingBack = $state<number | null>(null);
  let lastResult = $state<DreamResult | null>(null);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);

  const isInvest = $derived(path === 'invest');

  async function loadConfig() {
    loading = true;
    try {
      config = await invoke<DreamConfig>('get_dream_config');
      traces = await invoke<DreamSnapshot[]>('list_dream_traces', { dreamType: path, limit: 10 });
    } catch (e) {
      error = `Failed to load config: ${e}`;
    } finally {
      loading = false;
    }
  }

  async function saveConfig() {
    if (!config) return;
    if (config.investCron) config.investCron = config.investCron.trim().replace(/[^0-9a-zA-Z\s\*\/\,\-]/g, '');
    saving = true;
    error = null;
    try {
      await invoke('save_dream_config', { config });
      success = 'Config saved';
      setTimeout(() => (success = null), 2000);
    } catch (e) {
      error = `Failed to save: ${e}`;
    } finally {
      saving = false;
    }
  }

  async function trigger() {
    triggering = true;
    error = null;
    lastResult = null;
    try {
      lastResult = await invoke<DreamResult>('trigger_dream', { mode: path });
      // Dispatch browser notification if permission granted
      if (Notification.permission === 'granted') {
        new Notification(
          t('invest_insights_pipeline_notification', { count: String(lastResult.insightsWritten) }),
        );
      }
      // Reload traces after a successful run
      traces = await invoke<DreamSnapshot[]>('list_dream_traces', { dreamType: path, limit: 10 });
    } catch (e) {
      error = `Dream failed: ${e}`;
    } finally {
      triggering = false;
    }
  }

  async function rollback(snapshotId: number) {
    rollingBack = snapshotId;
    error = null;
    try {
      await invoke('rollback_dream', { snapshotId });
      traces = await invoke<DreamSnapshot[]>('list_dream_traces', { dreamType: path, limit: 10 });
      success = 'Rolled back successfully';
      setTimeout(() => (success = null), 2000);
    } catch (e) {
      error = `Rollback failed: ${e}`;
    } finally {
      rollingBack = null;
    }
  }

  function formatDuration(ms: number): string {
    if (ms < 1000) return `${ms}ms`;
    return `${(ms / 1000).toFixed(1)}s`;
  }

  // investStatusTextClass → investStatusTextClass (shared via $lib/utils/invest-status)

  $effect(() => {
    loadConfig();
  });
</script>

<div class="space-y-4">
  <h3 class="text-lg font-semibold text-[var(--text-primary)]">
    {isInvest ? t('invest_dreaming_title_invest') : t('invest_dreaming_title_userMemory')}
  </h3>

  {#if loading}
    <p class="text-[var(--text-secondary)]">{t('invest_dreaming_loadingConfig')}</p>
  {:else if config}
    <!-- Config section -->
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-4 space-y-3">
      <h4 class="text-sm font-medium text-[var(--text-primary)]">{t('invest_dreaming_configuration')}</h4>

      {#if !isInvest}
        <div class="rounded-[var(--radius-sm)] border border-[var(--color-warning)] bg-[var(--color-warning)]/10 p-2 text-xs text-[var(--color-warning)]">
          {t('invest_dreaming_notImplemented')}
        </div>
      {/if}

      <div class="flex items-center gap-3">
        <button
          class="relative inline-flex h-5 w-9 items-center rounded-full transition-colors {(isInvest ? config.investEnabled : config.userMemoryEnabled) ? 'bg-[var(--accent)]' : 'bg-[var(--bg-input)]'} {!isInvest ? 'opacity-50 cursor-not-allowed' : ''}"
          aria-label="Toggle enabled"
          disabled={!isInvest}
          onclick={() => {
            if (isInvest) {
              config!.investEnabled = !config!.investEnabled;
            }
          }}
        >
          <span
            class="inline-block h-3.5 w-3.5 rounded-full bg-white transition-transform {(isInvest ? config.investEnabled : config.userMemoryEnabled)
              ? 'translate-x-4'
              : 'translate-x-1'}"
          ></span>
        </button>
        <span class="text-sm {!isInvest ? 'text-[var(--text-secondary)]' : ''}">{isInvest ? t('invest_dreaming_investEnabled') : t('invest_dreaming_userMemoryEnabled')}</span>
      </div>

      {#if isInvest}
        <div class="flex items-center gap-2">
          <label class="text-xs text-[var(--text-secondary)] w-20">{t('invest_dreaming_cron')}</label>
          <input
            class="flex-1 rounded-[var(--radius-sm)] border border-border bg-[var(--bg-input)] px-2 py-1 text-xs font-[var(--font-mono)] text-[var(--text-primary)]"
            bind:value={config!.investCron}
          />
        </div>
      {:else}
        <div class="flex items-center gap-2">
          <label class="text-xs text-[var(--text-secondary)] w-20">{t('invest_dreaming_intervalMin')}</label>
          <input
            type="number"
            class="w-24 rounded-[var(--radius-sm)] border border-border bg-[var(--bg-input)] px-2 py-1 text-xs text-[var(--text-primary)] opacity-50"
            disabled
            bind:value={config!.userMemoryIntervalMin}
          />
        </div>
      {/if}

      <div class="flex items-center gap-2">
        <label class="text-xs text-[var(--text-secondary)] w-20">{t('invest_dreaming_lookback')}</label>
        <input
          type="number"
          class="w-24 rounded-[var(--radius-sm)] border border-border bg-[var(--bg-input)] px-2 py-1 text-xs text-[var(--text-primary)]"
          bind:value={config!.lookbackDays}
        />
        <span class="text-xs text-[var(--text-tertiary)]">{t('invest_dreaming_days')}</span>
      </div>

      <div class="flex items-center gap-2">
        <label class="text-xs text-[var(--text-secondary)] w-20">{t('invest_dreaming_minScore')}</label>
        <input
          type="number"
          step="0.05"
          class="w-24 rounded-[var(--radius-sm)] border border-border bg-[var(--bg-input)] px-2 py-1 text-xs text-[var(--text-primary)]"
          bind:value={config!.minScore}
        />
      </div>

      <div class="flex items-center gap-2">
        <label class="text-xs text-[var(--text-secondary)] w-20">{t('invest_dreaming_minCount')}</label>
        <input
          type="number"
          class="w-24 rounded-[var(--radius-sm)] border border-border bg-[var(--bg-input)] px-2 py-1 text-xs text-[var(--text-primary)]"
          bind:value={config!.minCount}
        />
      </div>

      <div class="flex gap-2 pt-1">
        <button
          class="rounded-[var(--radius-md)] bg-[var(--accent)] px-[var(--space-3)] py-[var(--space-1)] text-xs font-medium text-[#1a1918] hover:opacity-90 disabled:opacity-50"
          disabled={saving}
          onclick={saveConfig}
        >
          {saving ? t('invest_dreaming_saving') : t('invest_dreaming_saveConfig')}
        </button>
        <button
          class="rounded-[var(--radius-md)] bg-[var(--color-warning)] px-[var(--space-3)] py-[var(--space-1)] text-xs font-medium text-[#1a1918] hover:opacity-90 disabled:opacity-50"
          disabled={triggering || !isInvest}
          onclick={trigger}
        >
          {triggering ? t('invest_dreaming_running') : t('invest_dreaming_runNow')}
        </button>
      </div>
    </div>

    <!-- Last result -->
    {#if lastResult}
      <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-4 space-y-2">
        <h4 class="text-sm font-medium text-[var(--text-primary)]">{t('invest_dreaming_lastResult')}</h4>
        <div class="grid grid-cols-3 gap-2 text-center text-xs">
          <div class="rounded-[var(--radius-sm)] bg-[var(--color-success)]/10 p-2">
            <div class="text-lg font-bold text-[var(--color-success)]">{lastResult.insightsWritten}</div>
            <div class="text-[var(--text-secondary)]">{t('invest_dreaming_written')}</div>
          </div>
          <div class="rounded-[var(--radius-sm)] bg-blue-500/10 p-2">
            <div class="text-lg font-bold text-blue-500">{lastResult.insightsUpdated}</div>
            <div class="text-[var(--text-secondary)]">{t('invest_dreaming_updated')}</div>
          </div>
          <div class="rounded-[var(--radius-sm)] bg-[var(--color-warning)]/10 p-2">
            <div class="text-lg font-bold text-[var(--color-warning)]">{lastResult.insightsArchived}</div>
            <div class="text-[var(--text-secondary)]">{t('invest_dreaming_archived')}</div>
          </div>
        </div>
        <div class="text-xs text-[var(--text-tertiary)]">
          {t('invest_dreaming_totalDuration')} {formatDuration(lastResult.pipelineDurationMs)}
        </div>
        {#if lastResult.stages.length > 0}
          <div class="space-y-1">
            {#each lastResult.stages as stage}
              <div class="flex items-center justify-between text-xs">
                <span class="font-medium text-[var(--text-primary)]">{stage.stage}</span>
                <span class="text-[var(--text-secondary)]">
                  {stage.itemsProcessed} in / {stage.itemsOutput} out ({formatDuration(stage.durationMs)})
                </span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <!-- Trace list -->
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-4 space-y-2">
      <h4 class="text-sm font-medium text-[var(--text-primary)]">{t('invest_dreaming_recentTraces')}</h4>
      {#if traces.length === 0}
        <p class="text-xs text-[var(--text-secondary)]">{t('invest_dreaming_noTraces')}</p>
      {:else}
        <div class="max-h-60 space-y-2 overflow-y-auto">
          {#each traces as trace}
            <div class="flex items-center justify-between rounded-[var(--radius-sm)] border border-border bg-[var(--bg-card)] px-3 py-2 text-xs">
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class={investStatusTextClass(trace.status)}>{trace.status}</span>
                  <span class="text-[var(--text-tertiary)]">#{trace.id}</span>
                  <span class="text-[var(--text-tertiary)]">{trace.triggerType}</span>
                </div>
                {#if trace.summary}
                  <div class="mt-0.5 truncate text-[var(--text-secondary)]">{trace.summary}</div>
                {/if}
                <div class="text-[var(--text-tertiary)]">
                  {new Date(trace.createdAt).toLocaleString()}
                </div>
              </div>
              {#if trace.rollbackReady && trace.status === 'completed'}
                <button
                  class="ml-2 rounded-[var(--radius-sm)] border border-[var(--color-error)]/30 px-2 py-1 text-xs text-[var(--color-error)] hover:bg-[var(--color-error)]/10 disabled:opacity-50"
                  disabled={rollingBack === trace.id}
                  onclick={() => rollback(trace.id)}
                >
                  {rollingBack === trace.id ? t('invest_dreaming_rollingBack') : t('invest_dreaming_rollback')}
                </button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Messages -->
    {#if error}
      <div class="flex items-center justify-between rounded-[var(--radius-md)] border border-[var(--color-error)]/50 bg-[var(--color-error)]/10 px-3 py-2 text-sm text-[var(--color-error)]">
        <span>{error}</span>
        <button class="ml-2 text-xs hover:underline text-[var(--color-error)]" onclick={() => (error = null)}>{t('invest_dreaming_dismiss')}</button>
      </div>
    {/if}
    {#if success}
      <div class="rounded-[var(--radius-md)] border border-[var(--color-success)]/50 bg-[var(--color-success)]/10 px-3 py-2 text-sm text-[var(--color-success)]">
        {success}
      </div>
    {/if}
  {/if}
</div>
