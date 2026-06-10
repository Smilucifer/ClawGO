<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { formatDuration } from '$lib/utils/format';
  import { investStatusDotClass, investStatusTextClass } from '$lib/utils/invest-status';

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  // ── Types ──
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

  interface CronFields {
    minute: string;
    hour: string;
    dom: string;
    month: string;
    dow: string;
  }

  // ── Presets (6-field: second minute hour dom month dow) ──
  const PRESETS: { label: string; cron: string }[] = [
    { label: 'Weekdays 17:00',           cron: '0 0 17 * * 1-5' },
    { label: 'Daily 03:00',              cron: '0 0 3 * * *' },
    { label: 'Weekdays 9:30, 11:00',     cron: '0 30 9,11 * * 1-5' },
    { label: 'Weekdays 13:00, 15:00',    cron: '0 0 13,15 * * 1-5' },
    { label: 'Weekdays 22:00',           cron: '0 0 22 * * 1-5' },
    { label: 'Weekdays every 15min (8-22h)', cron: '0 */15 8-22 * * 1-5' },
    { label: 'Weekdays every 30min (8-22h)', cron: '0 */30 8-22 * * 1-5' },
  ];

  const MINUTE_OPTIONS = ['0', '15', '30', '45', '*/15', '*/30'];
  const DOW_OPTIONS: { value: string; label: string }[] = [
    { value: '*',   label: 'Every day' },
    { value: '1-5', label: 'Mon-Fri' },
    { value: '0,6', label: 'Sat-Sun' },
    { value: '1',   label: 'Mon' },
    { value: '2',   label: 'Tue' },
    { value: '3',   label: 'Wed' },
    { value: '4',   label: 'Thu' },
    { value: '5',   label: 'Fri' },
    { value: '6',   label: 'Sat' },
    { value: '0',   label: 'Sun' },
  ];

  // ── State ──
  let jobs: CronJob[] = $state([]);
  let logs: SchedulerLog[] = $state([]);
  let loading = $state(false);
  let expandedJob = $state<string | null>(null);
  let triggering = $state<string | null>(null);
  let error = $state<string | null>(null);
  let disposed = false;
  let tick = $state(0); // bumped every 60s to refresh countdown display

  // Cron builder state (per-expanded-job)
  let builderFields = $state<CronFields>({ minute: '0', hour: '*', dom: '*', month: '*', dow: '*' });
  let builderMinuteCustom = $state('');
  let builderHourCustom = $state('');
  let useCustomBuilder = $state(false);

  // ── Cron parsing & generation ──

  /** Strip the leading seconds field from a 6-field cron string, returning the 5-field array. */
  function stripSeconds(expr: string): string[] {
    const parts = expr.trim().split(/\s+/);
    return parts.length === 6 ? parts.slice(1) : parts;
  }

  function parseCronToFields(expr: string): CronFields {
    const p = stripSeconds(expr);
    return {
      minute: p[0] ?? '*',
      hour:   p[1] ?? '*',
      dom:    p[2] ?? '*',
      month:  p[3] ?? '*',
      dow:    p[4] ?? '*',
    };
  }

  function fieldsToCron(f: CronFields): string {
    return `0 ${f.minute} ${f.hour} ${f.dom} ${f.month} ${f.dow}`;
  }

  function isPresetCron(expr: string): boolean {
    const norm = fieldsToCron(parseCronToFields(expr));
    return PRESETS.some(p => p.cron === norm);
  }

  function humanCron(expr: string): string {
    const p = stripSeconds(expr);
    if (p.length < 5) return expr;
    const [min, hr, dom, mon, dow] = p;

    // Day-of-week names
    const dowNames: Record<string, string> = { '0': 'Sun', '1': 'Mon', '2': 'Tue', '3': 'Wed', '4': 'Thu', '5': 'Fri', '6': 'Sat', '7': 'Sun' };

    // Build day-of-week part
    let dowStr = '';
    if (dow === '1-5') {
      dowStr = 'Weekdays';
    } else if (dow === '0,6' || dow === '6,0') {
      dowStr = 'Weekends';
    } else if (dow.includes(',')) {
      dowStr = dow.split(',').map(d => dowNames[d.trim()] ?? d).join(', ');
    } else {
      dowStr = dowNames[dow] ?? dow;
    }

    // Build time part
    let timeStr = '';
    if (min.startsWith('*/')) {
      const interval = min.slice(2);
      if (hr === '*') {
        timeStr = `every ${interval}min`;
      } else {
        timeStr = `every ${interval}min (${hr}h)`;
      }
    } else if (hr.includes(',')) {
      // Multiple hours: "9:30, 11:00"
      const hours = hr.split(',');
      const times = hours.map(h => `${h.trim()}:${min.padStart(2, '0')}`);
      timeStr = times.join(', ');
    } else if (hr.includes('-') && !hr.includes('/')) {
      // Range: "8-22" with specific minute
      timeStr = `${hr}h @ :${min.padStart(2, '0')}`;
    } else if (hr === '*') {
      timeStr = `every hour @ :${min.padStart(2, '0')}`;
    } else {
      timeStr = `${hr}:${min.padStart(2, '0')}`;
    }

    // Build day-of-month part
    let domStr = '';
    if (dom !== '*' && mon !== '*') {
      domStr = `${mon}/${dom}`;
    } else if (dom !== '*') {
      domStr = `day ${dom}`;
    }

    // Compose
    const parts2 = [dowStr, timeStr, domStr].filter(Boolean);
    return parts2.join(' · ') || expr;
  }

  // ── Countdown formatting (uses backend-computed nextRun) ──

  function formatCountdown(nextRunIso: string, _tick: number): string {
    void _tick; // subscribe to tick for reactivity
    const diff = new Date(nextRunIso).getTime() - Date.now();
    if (diff <= 0) return 'now';
    const totalMin = Math.floor(diff / 60000);
    if (totalMin < 60) return `${totalMin}m`;
    const h = Math.floor(totalMin / 60);
    const m = totalMin % 60;
    return m > 0 ? `${h}h ${m}m` : `${h}h`;
  }

  // ── Builder helpers ──

  function syncBuilderFromExpr(expr: string) {
    const f = parseCronToFields(expr);
    builderFields = { ...f };
    useCustomBuilder = !isPresetCron(expr);
    // Sync custom inputs
    builderMinuteCustom = MINUTE_OPTIONS.includes(f.minute) ? '' : f.minute;
    builderHourCustom = f.hour === '*' ? '' : f.hour;
  }

  function builderCron(): string {
    return fieldsToCron(builderFields);
  }

  // ── Data loading ──

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

  async function toggle(job: CronJob, e: Event) {
    e.stopPropagation();
    error = null;
    try {
      await invoke('toggle_cron_job', { id: job.id, enabled: !job.enabled });
      await loadJobs();
    } catch (e2) {
      error = t('invest_scheduler_toggle_failed', { error: String(e2) });
    }
  }

  async function runNow(jobId: string, e: Event) {
    e.stopPropagation();
    error = null;
    triggering = jobId;
    try {
      await invoke('trigger_cron_job', { id: jobId });
      const storeRefresh = jobId === 'pnl_snapshot' ? investStore.refreshPnlSnapshots()
        : jobId === 'verdict_review' || jobId === 'dream_invest' ? investStore.loadAll()
        : Promise.resolve();
      await Promise.all([loadJobs(), storeRefresh]);
    } catch (e2) {
      error = t('invest_scheduler_job_failed', { error: String(e2) });
    } finally {
      triggering = null;
    }
  }

  async function saveCron(jobId: string, e: Event) {
    e.stopPropagation();
    error = null;
    const cronExpr = useCustomBuilder ? builderCron() : '';
    if (!cronExpr) return;
    try {
      await invoke('update_cron_schedule', { id: jobId, cronExpr });
      expandedJob = null;
      await loadJobs();
    } catch (e2) {
      error = t('invest_scheduler_save_failed', { error: String(e2) });
    }
  }

  async function selectPreset(cron: string, e: Event) {
    e.stopPropagation();
    syncBuilderFromExpr(cron);
    useCustomBuilder = false;
    if (!expandedJob) return;
    error = null;
    try {
      await invoke('update_cron_schedule', { id: expandedJob, cronExpr: cron });
      expandedJob = null;
      await loadJobs();
    } catch (e2) {
      error = t('invest_scheduler_save_failed', { error: String(e2) });
    }
  }

  function enterCustomBuilder(e: Event) {
    e.stopPropagation();
    useCustomBuilder = true;
    // Initialize fields from current job's cron
    if (expandedJob) {
      const job = jobs.find(j => j.id === expandedJob);
      if (job) syncBuilderFromExpr(job.cronExpr);
    }
  }

  function expandCard(jobId: string) {
    if (expandedJob === jobId) {
      expandedJob = null;
      return;
    }
    expandedJob = jobId;
    logs = [];
    loadLogs(jobId);
    // Init builder from current cron
    const job = jobs.find(j => j.id === jobId);
    if (job) syncBuilderFromExpr(job.cronExpr);
  }

  $effect(() => {
    disposed = false;
    loadJobs();
    // Refresh countdowns every 60s without re-fetching jobs
    const timer = setInterval(() => { tick++; }, 60_000);
    return () => { disposed = true; clearInterval(timer); };
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

    <div class="flex flex-col gap-[var(--space-2)]">
      {#each jobs as job (job.id)}
        {@const isExpanded = expandedJob === job.id}
        {@const countdown = job.nextRun ? formatCountdown(job.nextRun, tick) : null}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="job-card rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] overflow-hidden transition-[border-color] {isExpanded ? 'border-[rgba(200,169,110,0.4)]' : 'hover:border-[rgba(200,169,110,0.25)]'}"
        >
          <!-- Card Header -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="card-header grid items-center gap-[14px] px-[18px] py-[14px] cursor-pointer select-none hover:bg-[var(--bg-hover)] transition-colors"
            onclick={() => expandCard(job.id)}
          >
            <!-- Status dot -->
            <div class="status-dot {investStatusDotClass(job.lastStatus)}"></div>

            <!-- Name + description -->
            <div class="min-w-0">
              <div class="text-[13px] font-medium text-[var(--text-primary)] truncate">{job.name}</div>
              <div class="text-[11px] text-[var(--text-secondary)] mt-[2px]">
                {job.description}
                {#if job.requiresTradingDay}
                  <span class="badge-trading-day">{t('invest_scheduler_trading_day')}</span>
                {/if}
              </div>
            </div>

            <!-- Cron display -->
            <div class="cron-col text-right">
              <div class="text-[12px] text-[var(--text-primary)]">{humanCron(job.cronExpr)}</div>
              <div class="text-[10px] text-[var(--text-tertiary)] font-[family-name:var(--font-mono)] mt-[1px]">{job.cronExpr}</div>
            </div>

            <!-- Next run countdown -->
            <div class="next-run-col text-right min-w-[100px]">
              {#if !job.enabled}
                <span class="text-[11px] text-[var(--text-tertiary)]">{t('invest_scheduler_paused')}</span>
              {:else if countdown}
                <span class="text-[11px] text-[var(--text-tertiary)]">{t('invest_scheduler_next_run')}:</span>
                <span class="text-[12px] font-[family-name:var(--font-mono)] text-[var(--accent)]">{countdown}</span>
              {:else}
                <span class="text-[11px] text-[var(--text-tertiary)]">-</span>
              {/if}
            </div>

            <!-- Toggle -->
            <button
              class="toggle-switch {job.enabled ? 'on' : ''}"
              aria-label={job.enabled ? t('invest_scheduler_disable') : t('invest_scheduler_enable')}
              onclick={(e) => toggle(job, e)}
            >
              <span class="toggle-knob {job.enabled ? 'translate' : ''}"></span>
            </button>
          </div>

          <!-- Expanded Detail -->
          {#if isExpanded}
            <div class="detail-panel">
              <div class="detail-grid">
                <!-- Left: Schedule Editor -->
                <div>
                  <div class="section-label">{t('invest_scheduler_presets')}</div>
                  <div class="preset-list">
                    {#each PRESETS as preset}
                      <button
                        class="preset-item {fieldsToCron(builderFields) === preset.cron && !useCustomBuilder ? 'active' : ''}"
                        onclick={(e) => selectPreset(preset.cron, e)}
                      >
                        <span>{preset.label}</span>
                        <span class="preset-cron">{preset.cron}</span>
                      </button>
                    {/each}
                    <button
                      class="preset-item {useCustomBuilder ? 'active' : ''}"
                      onclick={(e) => enterCustomBuilder(e)}
                    >
                      <span>{t('invest_scheduler_custom')}</span>
                    </button>
                  </div>

                  {#if useCustomBuilder}
                    <div class="cron-builder" onclick={(e) => e.stopPropagation()}>
                      <!-- Minute -->
                      <div class="builder-row">
                        <label class="builder-label">{t('invest_scheduler_cron_minute')}</label>
                        <div class="builder-chips">
                          {#each MINUTE_OPTIONS as opt}
                            <button
                              class="chip {builderFields.minute === opt ? 'active' : ''}"
                              onclick={() => { builderFields.minute = opt; builderMinuteCustom = ''; }}
                            >{opt}</button>
                          {/each}
                          <input
                            class="chip-input"
                            placeholder="custom"
                            value={builderMinuteCustom}
                            oninput={(e) => {
                              builderMinuteCustom = e.currentTarget.value.trim();
                              if (builderMinuteCustom) builderFields.minute = builderMinuteCustom;
                            }}
                          />
                        </div>
                      </div>

                      <!-- Hour -->
                      <div class="builder-row">
                        <label class="builder-label">{t('invest_scheduler_cron_hour')}</label>
                        <div class="builder-chips">
                          <button
                            class="chip {builderFields.hour === '*' ? 'active' : ''}"
                            onclick={() => { builderFields.hour = '*'; builderHourCustom = ''; }}
                          >*</button>
                          <button
                            class="chip {builderFields.hour === '8-22' ? 'active' : ''}"
                            onclick={() => { builderFields.hour = '8-22'; builderHourCustom = ''; }}
                          >8-22</button>
                          <button
                            class="chip {builderFields.hour === '9-15' ? 'active' : ''}"
                            onclick={() => { builderFields.hour = '9-15'; builderHourCustom = ''; }}
                          >9-15</button>
                          <input
                            class="chip-input"
                            placeholder="e.g. 9,11"
                            value={builderHourCustom}
                            oninput={(e) => {
                              builderHourCustom = e.currentTarget.value.trim();
                              if (builderHourCustom) builderFields.hour = builderHourCustom;
                            }}
                          />
                        </div>
                      </div>

                      <!-- Day of Month -->
                      <div class="builder-row">
                        <label class="builder-label">{t('invest_scheduler_cron_day')}</label>
                        <div class="builder-chips">
                          <button
                            class="chip {builderFields.dom === '*' ? 'active' : ''}"
                            onclick={() => { builderFields.dom = '*'; }}
                          >*</button>
                          <select
                            class="chip-select"
                            value={builderFields.dom === '*' ? '*' : builderFields.dom}
                            onchange={(e) => { builderFields.dom = e.currentTarget.value; }}
                          >
                            <option value="*">*</option>
                            {#each Array.from({length: 31}, (_, i) => i + 1) as d}
                              <option value={String(d)}>{d}</option>
                            {/each}
                          </select>
                        </div>
                      </div>

                      <!-- Month -->
                      <div class="builder-row">
                        <label class="builder-label">{t('invest_scheduler_cron_month')}</label>
                        <div class="builder-chips">
                          <button
                            class="chip {builderFields.month === '*' ? 'active' : ''}"
                            onclick={() => { builderFields.month = '*'; }}
                          >*</button>
                          <select
                            class="chip-select"
                            value={builderFields.month === '*' ? '*' : builderFields.month}
                            onchange={(e) => { builderFields.month = e.currentTarget.value; }}
                          >
                            <option value="*">*</option>
                            {#each Array.from({length: 12}, (_, i) => i + 1) as m}
                              <option value={String(m)}>{m}</option>
                            {/each}
                          </select>
                        </div>
                      </div>

                      <!-- Day of Week -->
                      <div class="builder-row">
                        <label class="builder-label">{t('invest_scheduler_cron_weekday')}</label>
                        <div class="builder-chips">
                          {#each DOW_OPTIONS as opt}
                            <button
                              class="chip {builderFields.dow === opt.value ? 'active' : ''}"
                              onclick={() => { builderFields.dow = opt.value; }}
                            >{opt.label}</button>
                          {/each}
                        </div>
                      </div>

                      <!-- Preview -->
                      <div class="cron-preview">
                        <div class="cron-preview-expr">{builderCron()}</div>
                        <div class="cron-preview-human">{humanCron(builderCron())}</div>
                      </div>

                      <button
                        class="btn-save"
                        onclick={(e) => saveCron(job.id, e)}
                      >{t('invest_scheduler_save')}</button>
                    </div>
                  {/if}
                </div>

                <!-- Right: Run Timeline -->
                <div>
                  <div class="section-label">{t('invest_scheduler_recent_runs')}</div>
                  {#if logs.length === 0}
                    <p class="text-[12px] text-[var(--text-tertiary)]">{t('invest_scheduler_no_runs')}</p>
                  {:else}
                    <div class="timeline">
                      {#each logs.slice(0, 8) as log}
                        <div class="timeline-item">
                          <div class="timeline-dot {investStatusDotClass(log.status)}"></div>
                          <div class="timeline-time">{new Date(log.startedAt).toLocaleString()}</div>
                          <div class="timeline-meta">
                            <span class="timeline-status {investStatusTextClass(log.status)}">{log.status}</span>
                            <span class="timeline-duration">{log.durationMs ? formatDuration(log.durationMs) : ''}</span>
                          </div>
                          {#if log.message}
                            <div class="timeline-msg">{log.message}</div>
                          {/if}
                        </div>
                      {/each}
                    </div>
                  {/if}
                </div>
              </div>

              <!-- Actions -->
              <div class="card-actions">
                <button
                  class="btn-action primary"
                  disabled={triggering === job.id}
                  onclick={(e) => runNow(job.id, e)}
                >
                  {triggering === job.id ? '...' : t('invest_scheduler_run_now')}
                </button>
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>

    <!-- Footer -->
    <div class="footer-info">
      <span>{t('invest_scheduler_footer')}</span>
    </div>
  {/if}
</div>

<style>
  /* ── Card Header Grid ── */
  .card-header {
    grid-template-columns: 8px 1fr auto auto auto;
  }

  /* ── Status Dot ── */
  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .status-dot.ok {
    background: var(--color-ok);
    box-shadow: 0 0 6px rgba(92,184,92,0.4);
  }
  .status-dot.error {
    background: var(--color-error);
    box-shadow: 0 0 6px rgba(168,122,122,0.4);
  }
  .status-dot.skipped { background: var(--text-tertiary); }
  .status-dot.pending { background: var(--text-tertiary); opacity: 0.5; }

  /* ── Trading Day Badge ── */
  .badge-trading-day {
    display: inline-block;
    padding: 0 5px;
    border-radius: 3px;
    font-size: 10px;
    font-weight: 500;
    background: var(--accent-dim);
    color: var(--accent);
    margin-left: 4px;
    vertical-align: middle;
  }

  /* ── Toggle Switch ── */
  .toggle-switch {
    position: relative;
    width: 36px;
    height: 20px;
    border-radius: 10px;
    background: var(--bg-hover);
    border: none;
    cursor: pointer;
    transition: background 0.2s;
    flex-shrink: 0;
  }
  .toggle-switch.on { background: var(--accent); }
  .toggle-knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #fff;
    transition: transform 0.2s;
  }
  .toggle-knob.translate { transform: translateX(16px); }

  /* ── Detail Panel ── */
  .detail-panel {
    border-top: 1px solid var(--border);
    padding: 16px 18px;
    background: rgba(0,0,0,0.15);
  }
  .detail-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 20px;
  }

  /* ── Section Label ── */
  .section-label {
    font-size: 11px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-tertiary);
    margin-bottom: 8px;
  }

  /* ── Preset List ── */
  .preset-list {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .preset-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    font-size: 12px;
    color: var(--text-secondary);
    cursor: pointer;
    border: 1px solid transparent;
    background: transparent;
    transition: all 0.15s;
    text-align: left;
    width: 100%;
  }
  .preset-item:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .preset-item.active {
    background: var(--accent-dim);
    border-color: rgba(200,169,110,0.3);
    color: var(--accent);
  }
  .preset-cron {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-tertiary);
    margin-left: 8px;
    white-space: nowrap;
  }
  .preset-item.active .preset-cron { color: rgba(200,169,110,0.6); }

  /* ── Cron Builder ── */
  .cron-builder {
    margin-top: 12px;
    padding: 12px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: var(--bg-input);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .builder-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .builder-label {
    width: 48px;
    flex-shrink: 0;
    font-size: 11px;
    font-weight: 500;
    color: var(--text-tertiary);
    padding-top: 5px;
  }
  .builder-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  .chip {
    padding: 3px 8px;
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-secondary);
    background: transparent;
    border: 1px solid var(--border);
    cursor: pointer;
    transition: all 0.15s;
  }
  .chip:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .chip.active {
    background: var(--accent-dim);
    border-color: rgba(200,169,110,0.3);
    color: var(--accent);
  }
  .chip-input {
    width: 70px;
    padding: 3px 6px;
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-primary);
    background: var(--bg-card);
    border: 1px solid var(--border);
    outline: none;
  }
  .chip-input:focus { border-color: var(--accent); }
  .chip-input::placeholder { color: var(--text-tertiary); font-size: 10px; }
  .chip-select {
    padding: 3px 6px;
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-primary);
    background: var(--bg-card);
    border: 1px solid var(--border);
    outline: none;
    cursor: pointer;
  }

  /* ── Cron Preview ── */
  .cron-preview {
    margin-top: 4px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: var(--bg-card);
    border: 1px solid var(--border);
  }
  .cron-preview-expr {
    font-size: 13px;
    font-family: var(--font-mono);
    color: var(--accent);
    font-weight: 500;
  }
  .cron-preview-human {
    font-size: 11px;
    color: var(--text-secondary);
    margin-top: 2px;
  }

  /* ── Save Button ── */
  .btn-save {
    align-self: flex-end;
    padding: 5px 16px;
    border-radius: var(--radius-md);
    font-size: 12px;
    font-weight: 500;
    border: 1px solid rgba(200,169,110,0.3);
    background: var(--accent-dim);
    color: var(--accent);
    cursor: pointer;
    transition: all 0.15s;
  }
  .btn-save:hover { background: rgba(200,169,110,0.25); }

  /* ── Timeline ── */
  .timeline {
    position: relative;
    padding-left: 16px;
  }
  .timeline::before {
    content: '';
    position: absolute;
    left: 4px;
    top: 4px;
    bottom: 4px;
    width: 1px;
    background: var(--border);
  }
  .timeline-item {
    position: relative;
    padding: 4px 0 12px 0;
  }
  .timeline-item:last-child { padding-bottom: 0; }
  .timeline-dot {
    position: absolute;
    left: -14px;
    top: 7px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    border: 2px solid var(--bg-card);
  }
  .timeline-dot.ok { background: var(--color-ok); }
  .timeline-dot.error { background: var(--color-error); }
  .timeline-dot.skipped { background: var(--text-tertiary); }
  .timeline-dot.pending { background: var(--text-tertiary); opacity: 0.5; }
  .timeline-time {
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-tertiary);
  }
  .timeline-meta {
    display: flex;
    gap: 10px;
    margin-top: 2px;
  }
  .timeline-status {
    font-size: 11px;
    font-weight: 500;
  }
  .timeline-duration {
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--text-tertiary);
  }
  .timeline-msg {
    font-size: 11px;
    color: var(--text-secondary);
    margin-top: 2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 300px;
  }

  /* ── Actions ── */
  .card-actions {
    display: flex;
    gap: 8px;
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }
  .btn-action {
    padding: 5px 14px;
    border-radius: var(--radius-md);
    font-size: 12px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: all 0.15s;
  }
  .btn-action:hover { background: var(--bg-hover); color: var(--text-primary); }
  .btn-action:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-action.primary {
    background: var(--accent-dim);
    border-color: rgba(200,169,110,0.3);
    color: var(--accent);
  }
  .btn-action.primary:hover { background: rgba(200,169,110,0.25); }

  /* ── Footer ── */
  .footer-info {
    padding: 10px 14px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    font-size: 11px;
    color: var(--text-tertiary);
  }

  /* ── Responsive ── */
  @media (max-width: 700px) {
    .detail-grid { grid-template-columns: 1fr; }
    .card-header { grid-template-columns: 8px 1fr auto; }
    .cron-col, .next-run-col { display: none; }
  }
</style>
