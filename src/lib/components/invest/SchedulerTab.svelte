<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';

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
      await loadJobs();
    } catch (e) {
      error = t('invest_scheduler_job_failed', { error: String(e) });
    } finally {
      triggering = null;
    }
  }

  async function saveCron(jobId: string) {
    error = null;
    try {
      await invoke('update_cron_schedule', { id: jobId, cronExpr: editCronValue });
      editingJob = null;
      await loadJobs();
    } catch (e) {
      error = t('invest_scheduler_save_failed', { error: String(e) });
    }
  }

  function humanCron(expr: string): string {
    const map: Record<string, string> = {
      '30 9,11 * * 1-5': 'Weekdays 9:30, 11:00',
      '0 13,15 * * 1-5': 'Weekdays 13:00, 15:00',
      '0 17 * * 1-5': 'Weekdays 17:00',
      '*/30 8-22 * * 1-5': 'Weekdays every 30min (8-22h)',
      '0 3 * * *': 'Daily 03:00',
    };
    return map[expr] || expr;
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

<div class="space-y-4">
  <h3 class="text-lg font-semibold">{t('invest_scheduler_title')}</h3>

  {#if loading}
    <p class="text-muted-foreground">{t('invest_scheduler_loading')}</p>
  {:else}
    {#if error}
      <div class="flex items-center justify-between rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
        <span>{error}</span>
        <button class="ml-2 text-xs hover:underline" onclick={() => (error = null)}>{t('invest_scheduler_dismiss')}</button>
      </div>
    {/if}
    <div class="rounded-lg border">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b bg-muted/50 text-left">
            <th class="p-3">{t('invest_scheduler_job_name')}</th>
            <th class="p-3">{t('invest_scheduler_cron_expr')}</th>
            <th class="p-3 text-center">{t('invest_scheduler_status')}</th>
            <th class="p-3">{t('invest_scheduler_last_run')}</th>
            <th class="p-3 text-right">{t('invest_scheduler_actions')}</th>
          </tr>
        </thead>
        <tbody>
          {#each jobs as job}
            <tr class="border-b last:border-0">
              <td class="p-3">
                <div class="font-medium">{job.name}</div>
                <div class="text-xs text-muted-foreground">{job.description}</div>
              </td>
              <td class="p-3">
                {#if editingJob === job.id}
                  <div class="flex items-center gap-2">
                    <input
                      class="w-40 rounded border bg-background px-2 py-1 text-xs"
                      bind:value={editCronValue}
                    />
                    <button
                      class="text-xs text-primary"
                      onclick={() => saveCron(job.id)}
                    >{t('invest_scheduler_save')}</button>
                    <button
                      class="text-xs text-muted-foreground"
                      onclick={() => (editingJob = null)}
                    >{t('invest_scheduler_cancel')}</button>
                  </div>
                {:else}
                  <span class="text-xs font-mono">{humanCron(job.cronExpr)}</span>
                  {#if job.intervalMin}
                    <span class="ml-1 text-xs text-muted-foreground">(every {job.intervalMin}min)</span>
                  {/if}
                  <button
                    class="ml-2 text-xs text-muted-foreground hover:text-foreground"
                    onclick={() => {
                      editingJob = job.id;
                      editCronValue = job.cronExpr;
                    }}
                  >{t('invest_scheduler_edit')}</button>
                {/if}
              </td>
              <td class="p-3 text-center">
                <button
                  class="relative inline-flex h-5 w-9 items-center rounded-full transition-colors {job.enabled ? 'bg-primary' : 'bg-muted'}"
                  aria-label={job.enabled ? t('invest_scheduler_disable') : t('invest_scheduler_enable')}
                  onclick={() => toggle(job)}
                >
                  <span
                    class="inline-block h-3.5 w-3.5 rounded-full bg-white transition-transform {job.enabled
                      ? 'translate-x-4'
                      : 'translate-x-1'}"
                  ></span>
                </button>
              </td>
              <td class="p-3">
                {#if job.lastRun}
                  <div class="text-xs">{new Date(job.lastRun).toLocaleString()}</div>
                  <div class="text-xs {statusColor(job.lastStatus)}">{job.lastStatus || '-'}</div>
                {:else}
                  <span class="text-xs text-muted-foreground">-</span>
                {/if}
              </td>
              <td class="p-3 text-right">
                <div class="flex items-center justify-end gap-2">
                  <button
                    class="rounded px-2 py-1 text-xs hover:bg-muted disabled:opacity-50"
                    disabled={triggering === job.id}
                    onclick={() => runNow(job.id)}
                  >
                    {triggering === job.id ? '...' : t('invest_scheduler_run_now')}
                  </button>
                  <button
                    class="rounded px-2 py-1 text-xs hover:bg-muted"
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
      <div class="rounded-lg border p-4">
        <h4 class="mb-2 text-sm font-medium">{t('invest_scheduler_logs_for', { job: expandedJob })}</h4>
        {#if logs.length === 0}
          <p class="text-xs text-muted-foreground">{t('invest_scheduler_no_logs')}</p>
        {:else}
          <div class="max-h-60 space-y-1 overflow-y-auto">
            {#each logs as log}
              <div class="flex items-center gap-3 text-xs">
                <span class="text-muted-foreground">{new Date(log.startedAt).toLocaleString()}</span>
                <span class={statusColor(log.status)}>{log.status}</span>
                <span class="text-muted-foreground">{log.durationMs ? `${log.durationMs}ms` : ''}</span>
                <span class="truncate">{log.message || ''}</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  {/if}
</div>
