<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';

  interface CleanupReport {
    dailyReportsRows: number;
    eventSourcesRows: number;
    domainInsightsRows: number;
    domainInsightsEmptyRows: number;
    roomsDirExists: boolean;
  }

  interface CleanupTargets {
    dailyReports: boolean;
    eventSources: boolean;
    domainInsightsEmpty: boolean;
    roomsDir: boolean;
  }

  interface CleanupResult {
    deleted: string[];
  }

  let report = $state<CleanupReport | null>(null);
  let loading = $state(true);
  let applying = $state(false);
  let error = $state('');
  let lastDeleted = $state<string[] | null>(null);

  const targets = $state<CleanupTargets>({
    dailyReports: false,
    eventSources: false,
    domainInsightsEmpty: false,
    roomsDir: false,
  });

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  const anySelected = $derived(
    targets.dailyReports || targets.eventSources || targets.domainInsightsEmpty || targets.roomsDir,
  );

  async function scan() {
    loading = true;
    error = '';
    try {
      report = await invoke<CleanupReport>('invest_cleanup_scan');
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function apply() {
    if (!anySelected) return;
    if (!confirm(t('invest_cleanup_confirm'))) return;

    applying = true;
    error = '';
    lastDeleted = null;
    try {
      const result = await invoke<CleanupResult>('invest_cleanup_apply', {
        targets: { ...$state.snapshot(targets) },
      });
      lastDeleted = result.deleted;
      // Reset selections + refresh counts
      targets.dailyReports = false;
      targets.eventSources = false;
      targets.domainInsightsEmpty = false;
      targets.roomsDir = false;
      await scan();
    } catch (e) {
      error = String(e);
    } finally {
      applying = false;
    }
  }

  onMount(() => {
    scan();
  });
</script>

<div class="space-y-3">
  <div class="flex items-center justify-between">
    <h3 class="text-[13px] font-medium text-[var(--text-primary)]">{t('invest_cleanup_title')}</h3>
    <button
      type="button"
      class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-card)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] transition-colors hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] disabled:opacity-40"
      disabled={loading || applying}
      onclick={scan}
    >
      {t('invest_cleanup_scan')}
    </button>
  </div>

  {#if loading && !report}
    <p class="text-[13px] text-[var(--text-secondary)]">{t('invest_loading')}</p>
  {:else if error}
    <p class="text-[13px] text-[var(--color-error)]">{error}</p>
  {:else if report}
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] divide-y divide-[var(--border)]">
      <label class="flex items-center justify-between px-[var(--space-3)] py-[var(--space-2)]">
        <span class="flex items-center gap-2 text-[13px] text-[var(--text-primary)]">
          <input type="checkbox" bind:checked={targets.dailyReports} disabled={report.dailyReportsRows === 0} />
          {t('invest_cleanup_target_daily_reports')}
        </span>
        <span class="text-[12px] font-[var(--font-mono)] text-[var(--text-secondary)]">
          {report.dailyReportsRows}
        </span>
      </label>

      <label class="flex items-center justify-between px-[var(--space-3)] py-[var(--space-2)]">
        <span class="flex items-center gap-2 text-[13px] text-[var(--text-primary)]">
          <input type="checkbox" bind:checked={targets.eventSources} disabled={report.eventSourcesRows === 0} />
          {t('invest_cleanup_target_event_sources')}
        </span>
        <span class="text-[12px] font-[var(--font-mono)] text-[var(--text-secondary)]">
          {report.eventSourcesRows}
        </span>
      </label>

      <label class="flex items-center justify-between px-[var(--space-3)] py-[var(--space-2)]">
        <span class="flex items-center gap-2 text-[13px] text-[var(--text-primary)]">
          <input type="checkbox" bind:checked={targets.domainInsightsEmpty} disabled={report.domainInsightsEmptyRows === 0} />
          {t('invest_cleanup_target_domain_insights_empty')}
        </span>
        <span class="text-[12px] font-[var(--font-mono)] text-[var(--text-secondary)]">
          {report.domainInsightsEmptyRows} / {report.domainInsightsRows}
        </span>
      </label>

      <label class="flex items-center justify-between px-[var(--space-3)] py-[var(--space-2)]">
        <span class="flex items-center gap-2 text-[13px] text-[var(--text-primary)]">
          <input type="checkbox" bind:checked={targets.roomsDir} disabled={!report.roomsDirExists} />
          {t('invest_cleanup_target_rooms_dir')}
        </span>
        <span class="text-[12px] font-[var(--font-mono)] text-[var(--text-secondary)]">
          {report.roomsDirExists ? t('invest_cleanup_present') : t('invest_cleanup_absent')}
        </span>
      </label>
    </div>

    <div class="flex items-center justify-between">
      <p class="text-[11px] text-[var(--text-tertiary)]">{t('invest_cleanup_warning')}</p>
      <button
        type="button"
        class="rounded-[var(--radius-md)] bg-[var(--color-error)] px-[var(--space-4)] py-[var(--space-1)] text-[12px] font-medium text-[var(--bg-base)] transition-colors hover:opacity-90 disabled:opacity-40"
        disabled={!anySelected || applying}
        onclick={apply}
      >
        {applying ? '...' : t('invest_cleanup_apply')}
      </button>
    </div>

    {#if lastDeleted && lastDeleted.length > 0}
      <div class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-card)] px-[var(--space-3)] py-[var(--space-2)]">
        <div class="text-[12px] font-medium text-[var(--text-primary)]">{t('invest_cleanup_done')}</div>
        <ul class="mt-1 space-y-0.5 text-[12px] text-[var(--text-secondary)] font-[var(--font-mono)]">
          {#each lastDeleted as line}
            <li>{line}</li>
          {/each}
        </ul>
      </div>
    {/if}
  {/if}
</div>
