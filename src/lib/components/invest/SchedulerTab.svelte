<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';
  import { investStore } from '$lib/stores/invest-store.svelte';

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  interface CronJob {
    id: string;
    name: string;
    cronExpr: string;
    intervalMin?: number;
    enabled: boolean;
    requiresTradingDay: boolean;
    lastRun?: string;
    nextRun?: string;
    lastStatus?: string;
    description: string;
  }

  interface SchedulerLog {
    id: number;
    taskName: string;
    status: string;
    message?: string;
    startedAt: string;
    finishedAt?: string;
    durationMs?: number;
  }

  let jobs: CronJob[] = $state([]);
  let logs: SchedulerLog[] = $state([]);
  let loading = $state(false);
  let expandedJob = $state<string | null>(null);
  let editingJob = $state<string | null>(null);
  let editCronValue = $state('');
  let triggering = $state<string | null>(null);
  let error = $state<string | null>(null);
  let disposed = false;

  async function loadJobs() {
    loading = true;
    try {
      const result = await invoke<CronJob[]>('list_cron_jobs');
      if (!disposed) jobs = result;
    } catch (e) {
      if (!disposed) error = String(e);
    } finally {
      if (!disposed) loading = false;
    }
  }

  async function loadLogs(jobId: string) {
    expandedJob = expandedJob === jobId ? null : jobId;
    if (!expandedJob) {
      logs = [];
      return;
    }
    try {
      const result = await invoke<SchedulerLog[]>('get_cron_job_logs', { taskName: jobId, limit: 20 });
      if (!disposed) logs = result;
    } catch (e) {
      if (!disposed) {
        error = String(e);
        logs = [];
      }
    }
  }

  async function toggle(job: CronJob) {
    error = null;
    try {
      await invoke('toggle_cron_job', { id: job.id, enabled: !job.enabled });
      await loadJobs();
    } catch (e) {
      error = t('invest_scheduler_toggle_failed', { error: String(e) });
    }
  }

  async function runNow(jobId: string) {
    error = null;
    triggering = jobId;
    try {
      await invoke('trigger_cron_job', { id: jobId });
      // Parallel: refresh scheduler list + targeted invest store data
      const storeRefresh = jobId === 'pnl_snapshot' ? investStore.refreshPnlSnapshots()
        : jobId === 'verdict_review' || jobId === 'dream_invest' ? investStore.loadAll()
        : Promise.resolve();
      await Promise.all([loadJobs(), storeRefresh]);
    } catch (e) {
      error = t('invest_scheduler_job_failed', { error: String(e) });
    } finally {
      triggering = null;
    }
  }

  async function saveCron(jobId: string) {
    error = null;
    editCronValue = editCronValue.trim().replace(/[^0-9a-zA-Z\s\*\/\,\-]/g, '');
    if (!editCronValue) {
      error = t('invest_scheduler_save_failed', { error: 'Cron expression cannot be empty' });
      return;
    }
    try {
      await invoke('update_cron_schedule', { id: jobId, cronExpr: editCronValue });
      editingJob = null;
      await loadJobs();
    } catch (e) {
      error = t('invest_scheduler_save_failed', { error: String(e) });
    }
  }

  function humanCron(expr: string): string {
    // Normalize: if 6-field (seconds prefix), drop the first field for display
    const parts = expr.trim().split(/\s+/);
    const normalized = parts.length === 6 ? parts.slice(1).join(' ') : expr;
    const map: Record<string, string> = {
      '30 9,11 * * 1-5': 'Weekdays 9:30, 11:00',
      '0 13,15 * * 1-5': 'Weekdays 13:00, 15:00',
      '0 17 * * 1-5': 'Weekdays 17:00',
      '*/30 8-22 * * 1-5': 'Weekdays every 30min (8-22h)',
      '0 3 * * *': 'Daily 03:00',
      '*/15 8-22 * * 1-5': 'Weekdays every 15min (8-22h)',
      '0 22 * * 1-5': 'Weekdays 22:00',
    };
    return map[normalized] || map[expr] || expr;
  }

  function statusColor(status?: string): string {
    if (status === 'ok') return 'text-green-500';
    if (status === 'error') return 'text-red-500';
    if (status === 'skipped') return 'text-muted-foreground';
    return 'text-muted-foreground';
  }

  $effect(() => {
    disposed = false;
    loadJobs();
    return () => { disposed = true; };
  });
</script>

<div class="flex flex-col gap-[var(--space-4)]">
  <h3 class="text-[15px] font-semibold text-[var(--text-primary)]">{t('invest_scheduler_title')}</h3>

  {#if loading}
    <p class="text-[var(--text-secondary)] text-[13px]">{t('invest_scheduler_loading')}</p>
  {:else}
    {#if error}
      <div class="flex items-center justify-between rounded-[var(--radius-md)] border border-[rgba(168,122,122,0.5)] bg-[rgba(168,122,122,0.1)] px-[var(--space-3)] py-[var(--space-2)] text-[13px] text-[var(--color-error)]">
        <span>{error}</span>
        <button class="ml-2 text-[11px] hover:underline text-[var(--text-secondary)]" onclick={() => (error = null)}>{t('invest_scheduler_dismiss')}</button>
      </div>
    {/if}
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] overflow-hidden">
      <table class="w-full text-[13px]">
        <thead>
          <tr class="border-b border-border bg-[var(--bg-hover)] text-left">
            <th class="p-[var(--space-3)] text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_scheduler_job_name')}</th>
            <th class="p-[var(--space-3)] text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_scheduler_cron_expr')}</th>
            <th class="p-[var(--space-3)] text-center text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_scheduler_status')}</th>
            <th class="p-[var(--space-3)] text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_scheduler_last_run')}</th>
            <th class="p-[var(--space-3)] text-right text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_scheduler_actions')}</th>
          </tr>
        </thead>
        <tbody>
          {#each jobs as job}
            <tr class="border-b border-border last:border-0 hover:bg-[var(--bg-hover)] transition-colors">
              <td class="p-[var(--space-3)]">
                <div class="font-medium text-[var(--text-primary)]">{job.name}</div>
                <div class="text-[11px] text-[var(--text-secondary)]">{job.description}</div>
              </td>
              <td class="p-[var(--space-3)]">
                {#if editingJob === job.id}
                  <div class="flex items-center gap-[var(--space-2)]">
                    <input
                      class="w-40 rounded-[var(--radius-sm)] border border-border bg-[var(--bg-input)] px-[var(--space-2)] py-[var(--space-1)] text-[12px] text-[var(--text-primary)]"
                      bind:value={editCronValue}
                    />
                    <button
                      class="text-[12px] text-[var(--accent)]"
                      onclick={() => saveCron(job.id)}
                    >{t('invest_scheduler_save')}</button>
                    <button
                      class="text-[12px] text-[var(--text-secondary)]"
                      onclick={() => (editingJob = null)}
                    >{t('invest_scheduler_cancel')}</button>
                  </div>
                {:else}
                  <span class="text-[12px] font-[var(--font-mono)] text-[var(--text-primary)]">{humanCron(job.cronExpr)}</span>
                  {#if job.intervalMin}
                    <span class="ml-1 text-[12px] text-[var(--text-tertiary)]">(every {job.intervalMin}min)</span>
                  {/if}
                  <button
                    class="ml-2 text-[12px] text-[var(--text-tertiary)] hover:text-[var(--text-primary)]"
                    onclick={() => {
                      editingJob = job.id;
                      editCronValue = job.cronExpr;
                    }}
                  >{t('invest_scheduler_edit')}</button>
                {/if}
              </td>
              <td class="p-[var(--space-3)] text-center">
                <button
                  class="relative inline-flex h-5 w-9 items-center rounded-[var(--radius-full)] transition-colors {job.enabled ? 'bg-[var(--accent)]' : 'bg-[var(--bg-hover)]'}"
                  aria-label={job.enabled ? t('invest_scheduler_disable') : t('invest_scheduler_enable')}
                  onclick={() => toggle(job)}
                >
                  <span
                    class="inline-block h-3.5 w-3.5 rounded-[var(--radius-full)] bg-white transition-transform {job.enabled
                      ? 'translate-x-4'
                      : 'translate-x-1'}"
                  ></span>
                </button>
              </td>
              <td class="p-[var(--space-3)]">
                {#if job.lastRun}
                  <div class="text-[12px] text-[var(--text-primary)]">{new Date(job.lastRun).toLocaleString()}</div>
                  <div class="text-[12px] {statusColor(job.lastStatus)}">{job.lastStatus || '-'}</div>
                {:else}
                  <span class="text-[12px] text-[var(--text-tertiary)]">-</span>
                {/if}
              </td>
              <td class="p-[var(--space-3)] text-right">
                <div class="flex items-center justify-end gap-[var(--space-2)]">
                  <button
                    class="rounded-[var(--radius-md)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] disabled:opacity-50 transition-colors"
                    disabled={triggering === job.id}
                    onclick={() => runNow(job.id)}
                  >
                    {triggering === job.id ? '...' : t('invest_scheduler_run_now')}
                  </button>
                  <button
                    class="rounded-[var(--radius-md)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors"
                    onclick={() => loadLogs(job.id)}
                  >
                    {t('invest_scheduler_view_logs')}
                  </button>
                </div>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    {#if expandedJob}
      <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
        <h4 class="mb-[var(--space-2)] text-[13px] font-medium text-[var(--text-primary)]">{t('invest_scheduler_logs_for', { job: expandedJob })}</h4>
        {#if logs.length === 0}
          <p class="text-[12px] text-[var(--text-tertiary)]">{t('invest_scheduler_no_logs')}</p>
        {:else}
          <div class="max-h-60 flex flex-col gap-[var(--space-1)] overflow-y-auto">
            {#each logs as log}
              <div class="flex items-center gap-[var(--space-3)] text-[12px]">
                <span class="text-[var(--text-tertiary)] font-[var(--font-mono)]">{new Date(log.startedAt).toLocaleString()}</span>
                <span class={statusColor(log.status)}>{log.status}</span>
                <span class="text-[var(--text-tertiary)] font-[var(--font-mono)]">{log.durationMs ? `${log.durationMs}ms` : ''}</span>
                <span class="truncate text-[var(--text-secondary)]">{log.message || ''}</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</div>
