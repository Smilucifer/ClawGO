<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';

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

  function statusColor(status: string): string {
    if (status === 'completed') return 'text-green-500';
    if (status === 'rolled_back') return 'text-amber-500';
    if (status === 'failed') return 'text-red-500';
    return 'text-muted-foreground';
  }

  $effect(() => {
    loadConfig();
  });
</script>

<div class="space-y-4">
  <h3 class="text-lg font-semibold">
    {isInvest ? 'Investment Dreaming' : 'User Memory Dreaming'}
  </h3>

  {#if loading}
    <p class="text-muted-foreground">Loading config...</p>
  {:else if config}
    <!-- Config section -->
    <div class="rounded-lg border p-4 space-y-3">
      <h4 class="text-sm font-medium">Configuration</h4>

      {#if !isInvest}
        <div class="rounded border border-amber-300 bg-amber-50 p-2 text-xs text-amber-700 dark:bg-amber-950 dark:text-amber-300">
          User memory dreaming is not yet implemented. This panel is a preview.
        </div>
      {/if}

      <div class="flex items-center gap-3">
        <button
          class="relative inline-flex h-5 w-9 items-center rounded-full transition-colors {(isInvest ? config.investEnabled : config.userMemoryEnabled) ? 'bg-primary' : 'bg-muted'} {!isInvest ? 'opacity-50 cursor-not-allowed' : ''}"
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
        <span class="text-sm {!isInvest ? 'text-muted-foreground' : ''}">{isInvest ? 'Invest pipeline enabled' : 'User memory enabled (coming soon)'}</span>
      </div>

      {#if isInvest}
        <div class="flex items-center gap-2">
          <label class="text-xs text-muted-foreground w-20">Cron</label>
          <input
            class="flex-1 rounded border bg-background px-2 py-1 text-xs font-mono"
            bind:value={config!.investCron}
          />
        </div>
      {:else}
        <div class="flex items-center gap-2">
          <label class="text-xs text-muted-foreground w-20">Interval (min)</label>
          <input
            type="number"
            class="w-24 rounded border bg-background px-2 py-1 text-xs opacity-50"
            disabled
            bind:value={config!.userMemoryIntervalMin}
          />
        </div>
      {/if}

      <div class="flex items-center gap-2">
        <label class="text-xs text-muted-foreground w-20">Lookback</label>
        <input
          type="number"
          class="w-24 rounded border bg-background px-2 py-1 text-xs"
          bind:value={config!.lookbackDays}
        />
        <span class="text-xs text-muted-foreground">days</span>
      </div>

      <div class="flex items-center gap-2">
        <label class="text-xs text-muted-foreground w-20">Min score</label>
        <input
          type="number"
          step="0.05"
          class="w-24 rounded border bg-background px-2 py-1 text-xs"
          bind:value={config!.minScore}
        />
      </div>

      <div class="flex items-center gap-2">
        <label class="text-xs text-muted-foreground w-20">Min count</label>
        <input
          type="number"
          class="w-24 rounded border bg-background px-2 py-1 text-xs"
          bind:value={config!.minCount}
        />
      </div>

      <div class="flex gap-2 pt-1">
        <button
          class="rounded bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
          disabled={saving}
          onclick={saveConfig}
        >
          {saving ? 'Saving...' : 'Save Config'}
        </button>
        <button
          class="rounded bg-amber-600 px-3 py-1.5 text-xs text-white hover:bg-amber-700 disabled:opacity-50"
          disabled={triggering || !isInvest}
          onclick={trigger}
        >
          {triggering ? 'Running...' : 'Run Now'}
        </button>
      </div>
    </div>

    <!-- Last result -->
    {#if lastResult}
      <div class="rounded-lg border p-4 space-y-2">
        <h4 class="text-sm font-medium">Last Result</h4>
        <div class="grid grid-cols-3 gap-2 text-center text-xs">
          <div class="rounded bg-green-500/10 p-2">
            <div class="text-lg font-bold text-green-500">{lastResult.insightsWritten}</div>
            <div class="text-muted-foreground">Written</div>
          </div>
          <div class="rounded bg-blue-500/10 p-2">
            <div class="text-lg font-bold text-blue-500">{lastResult.insightsUpdated}</div>
            <div class="text-muted-foreground">Updated</div>
          </div>
          <div class="rounded bg-amber-500/10 p-2">
            <div class="text-lg font-bold text-amber-500">{lastResult.insightsArchived}</div>
            <div class="text-muted-foreground">Archived</div>
          </div>
        </div>
        <div class="text-xs text-muted-foreground">
          Total duration: {formatDuration(lastResult.pipelineDurationMs)}
        </div>
        {#if lastResult.stages.length > 0}
          <div class="space-y-1">
            {#each lastResult.stages as stage}
              <div class="flex items-center justify-between text-xs">
                <span class="font-medium">{stage.stage}</span>
                <span class="text-muted-foreground">
                  {stage.itemsProcessed} in / {stage.itemsOutput} out ({formatDuration(stage.durationMs)})
                </span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    <!-- Trace list -->
    <div class="rounded-lg border p-4 space-y-2">
      <h4 class="text-sm font-medium">Recent Traces</h4>
      {#if traces.length === 0}
        <p class="text-xs text-muted-foreground">No traces yet. Run the dream pipeline to see history.</p>
      {:else}
        <div class="max-h-60 space-y-2 overflow-y-auto">
          {#each traces as trace}
            <div class="flex items-center justify-between rounded border px-3 py-2 text-xs">
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class={statusColor(trace.status)}>{trace.status}</span>
                  <span class="text-muted-foreground">#{trace.id}</span>
                  <span class="text-muted-foreground">{trace.triggerType}</span>
                </div>
                {#if trace.summary}
                  <div class="mt-0.5 truncate text-muted-foreground">{trace.summary}</div>
                {/if}
                <div class="text-muted-foreground">
                  {new Date(trace.createdAt).toLocaleString()}
                </div>
              </div>
              {#if trace.rollbackReady && trace.status === 'completed'}
                <button
                  class="ml-2 rounded border px-2 py-1 text-xs hover:bg-destructive/10 text-destructive disabled:opacity-50"
                  disabled={rollingBack === trace.id}
                  onclick={() => rollback(trace.id)}
                >
                  {rollingBack === trace.id ? 'Rolling back...' : 'Rollback'}
                </button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Messages -->
    {#if error}
      <div class="flex items-center justify-between rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
        <span>{error}</span>
        <button class="ml-2 text-xs hover:underline" onclick={() => (error = null)}>Dismiss</button>
      </div>
    {/if}
    {#if success}
      <div class="rounded-md border border-green-500/50 bg-green-500/10 px-3 py-2 text-sm text-green-500">
        {success}
      </div>
    {/if}
  {/if}
</div>
