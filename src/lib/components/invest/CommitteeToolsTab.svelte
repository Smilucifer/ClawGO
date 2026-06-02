<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import type { MessageKey } from '$lib/i18n/types';
  import { ROLE_COLORS } from './pipeline-config';

  const store = investCommitteeStore;

  // ── 9-tool × 5-role access matrix
  // Source of truth: src-tauri/src/invest/committee/tools.rs:184-206
  // (Macro: R1 only; Quant/Risk: R1+R2 same toolset; L4: query_dreaming_insights only; CIO: no tools)
  type RoleKey = 'macro' | 'quant' | 'risk' | 'l4_officer' | 'cio';

  interface MatrixRow {
    name: string;
    descKey: string;
    access: Record<RoleKey, string>; // '' = no access; 'R1' / 'R1+R2' / '✓' = access label
  }

  const TOOLS_MATRIX: MatrixRow[] = [
    { name: 'get_history_data',              descKey: 'invest_tool_history_desc',
      access: { macro: 'R1', quant: 'R1+R2', risk: '',      l4_officer: '',  cio: '' } },
    { name: 'analyze_multi_timeframe',       descKey: 'invest_tool_mtf_desc',
      access: { macro: 'R1', quant: 'R1+R2', risk: '',      l4_officer: '',  cio: '' } },
    { name: 'get_macro_snapshot',            descKey: 'invest_tool_macro_desc',
      access: { macro: 'R1', quant: '',      risk: '',      l4_officer: '',  cio: '' } },
    { name: 'query_dreaming_insights',       descKey: 'invest_tool_dreaming_desc',
      access: { macro: 'R1', quant: '',      risk: 'R1+R2', l4_officer: '✓', cio: '' } },
    { name: 'get_recent_committee_verdicts', descKey: 'invest_tool_verdicts_desc',
      access: { macro: 'R1', quant: 'R1+R2', risk: 'R1+R2', l4_officer: '',  cio: '' } },
    { name: 'get_recent_events',             descKey: 'invest_tool_events_desc',
      access: { macro: 'R1', quant: '',      risk: '',      l4_officer: '',  cio: '' } },
    { name: 'get_moneyflow',                 descKey: 'invest_tool_moneyflow_desc',
      access: { macro: '',   quant: 'R1+R2', risk: '',      l4_officer: '',  cio: '' } },
    { name: 'get_company_info',              descKey: 'invest_tool_company_info_desc',
      access: { macro: '',   quant: 'R1+R2', risk: '',      l4_officer: '',  cio: '' } },
    { name: 'get_company_news',              descKey: 'invest_tool_company_news_desc',
      access: { macro: '',   quant: '',      risk: 'R1+R2', l4_officer: '',  cio: '' } },
  ];

  const ROLE_COLUMNS: { key: RoleKey; label: string; color: string }[] = [
    { key: 'macro',      label: 'Macro',     color: ROLE_COLORS.macro },
    { key: 'quant',      label: 'Quant',     color: ROLE_COLORS.quant },
    { key: 'risk',       label: 'Risk',      color: ROLE_COLORS.risk },
    { key: 'l4_officer', label: 'L4',        color: ROLE_COLORS.l4_officer },
    { key: 'cio',        label: 'CIO',       color: ROLE_COLORS.cio },
  ];

  // ── State ──
  let expandedIndex = $state<number | null>(null);
  let roleFilter = $state<string>('all');

  const filteredHistory = $derived(
    roleFilter === 'all'
      ? store.toolCallHistory
      : store.toolCallHistory.filter((r) => r.role === roleFilter)
  );

  const totalCalls = $derived(store.toolCallHistory.length);
  const successCount = $derived(store.toolCallHistory.filter((r) => r.success).length);
  const successRate = $derived(totalCalls > 0 ? successCount / totalCalls : 0);
  const avgLatency = $derived(
    totalCalls > 0
      ? Math.round(store.toolCallHistory.reduce((sum, r) => sum + r.latencyMs, 0) / totalCalls)
      : 0
  );

  const roleStats = $derived.by(() => {
    const map = new Map<string, { calls: number; errors: number; totalLatency: number }>();
    for (const r of store.toolCallHistory) {
      const existing = map.get(r.role) ?? { calls: 0, errors: 0, totalLatency: 0 };
      existing.calls++;
      if (!r.success) existing.errors++;
      existing.totalLatency += r.latencyMs;
      map.set(r.role, existing);
    }
    return map;
  });

  function formatArgs(raw: string): string {
    try {
      const obj = JSON.parse(raw);
      return Object.entries(obj)
        .map(([k, v]) => `${k}=${typeof v === 'string' ? v : JSON.stringify(v)}`)
        .join(', ');
    } catch {
      return raw;
    }
  }

  function roleLabel(role: string): string {
    return ROLE_COLUMNS.find((c) => c.key === role)?.label ?? role;
  }

  function roleBadgeStyle(role: string): string {
    const col = ROLE_COLUMNS.find((c) => c.key === role)?.color ?? '#6b7280';
    return `background:${col}26; color:${col};`;
  }
</script>

<div class="space-y-[var(--space-4)]">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h3 class="text-[16px] font-semibold text-[var(--text-primary)]">{t('invest_tools_title')}</h3>
      <p class="text-[13px] text-[var(--text-secondary)]">{t('invest_tools_desc')}</p>
    </div>
    {#if store.toolCallHistory.length > 0}
      <button
        class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-card)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] transition-colors hover:bg-[var(--bg-hover)]"
        onclick={() => { store.toolCallHistory = []; expandedIndex = null; }}
      >
        {t('invest_tools_clear')}
      </button>
    {/if}
  </div>

  <!-- KPI cards -->
  <div class="grid grid-cols-3 gap-[var(--space-3)]">
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-3)] text-center">
      <div class="font-[var(--font-mono)] text-[22px] font-bold text-[var(--text-primary)]">{totalCalls}</div>
      <div class="text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_tools_total_calls')}</div>
    </div>
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-3)] text-center">
      <div class="font-[var(--font-mono)] text-[22px] font-bold {successRate >= 0.9 ? 'text-[var(--color-success)]' : successRate >= 0.7 ? 'text-[var(--color-warning)]' : 'text-[var(--color-error)]'}">
        {totalCalls > 0 ? `${(successRate * 100).toFixed(0)}%` : '-'}
      </div>
      <div class="text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_tools_success_rate')}</div>
    </div>
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-3)] text-center">
      <div class="font-[var(--font-mono)] text-[22px] font-bold text-[var(--text-primary)]">{totalCalls > 0 ? `${avgLatency}ms` : '-'}</div>
      <div class="text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_tools_avg_latency')}</div>
    </div>
  </div>

  <!-- Role × Tool access matrix -->
  <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)]">
    <div class="border-b border-border bg-[var(--bg-input)] px-[var(--space-4)] py-[var(--space-2)] text-[13px] font-medium text-[var(--text-primary)]">
      {t('invest_tools_matrix_title')}
    </div>
    <div class="overflow-x-auto">
      <table class="w-full text-[12px]">
        <thead>
          <tr class="border-b border-border">
            <th class="px-[var(--space-3)] py-[var(--space-2)] text-left text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">
              {t('invest_tools_col_tool')}
            </th>
            {#each ROLE_COLUMNS as col}
              <th class="px-[var(--space-2)] py-[var(--space-2)] text-center text-[11px] font-medium uppercase tracking-wider" style="color:{col.color}">
                {col.label}
              </th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each TOOLS_MATRIX as row}
            <tr class="border-b border-border last:border-0 hover:bg-[var(--bg-hover)]">
              <td class="px-[var(--space-3)] py-[var(--space-2)]">
                <div class="font-[var(--font-mono)] text-[12px] text-[var(--text-primary)]">{row.name}</div>
                <div class="text-[11px] text-[var(--text-tertiary)]">{t(row.descKey as MessageKey)}</div>
              </td>
              {#each ROLE_COLUMNS as col}
                {@const v = row.access[col.key]}
                <td class="px-[var(--space-2)] py-[var(--space-2)] text-center font-[var(--font-mono)] text-[11px]">
                  {#if v}
                    <span class="text-[var(--color-success)]">✓ {v}</span>
                  {:else}
                    <span class="text-[var(--text-tertiary)]">✗</span>
                  {/if}
                </td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>

  <!-- Per-role stats when there's history -->
  {#if store.toolCallHistory.length > 0}
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)]">
      <div class="border-b border-border bg-[var(--bg-input)] px-[var(--space-4)] py-[var(--space-2)] text-[13px] font-medium text-[var(--text-primary)]">{t('invest_tools_by_role')}</div>
      <div class="grid grid-cols-5 gap-[var(--space-3)] p-[var(--space-4)]">
        {#each ROLE_COLUMNS as col}
          {@const stats = roleStats.get(col.key)}
          <div class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-card)] p-[var(--space-2)] text-center">
            <div class="mb-1 inline-block rounded-[var(--radius-md)] px-1.5 py-0.5 text-[11px] font-medium" style={roleBadgeStyle(col.key)}>
              {col.label}
            </div>
            <div class="font-[var(--font-mono)] text-[18px] font-bold text-[var(--text-primary)]">{stats?.calls ?? 0}</div>
            <div class="text-[11px] text-[var(--text-tertiary)]">
              {#if stats && stats.errors > 0}
                <span class="text-[var(--color-error)]">{stats.errors} err</span>
              {:else}
                {t('invest_tools_total_calls')}
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Tool call history -->
  <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)]">
    <div class="flex items-center justify-between border-b border-border bg-[var(--bg-input)] px-[var(--space-4)] py-[var(--space-2)]">
      <span class="text-[13px] font-medium text-[var(--text-primary)]">{t('invest_tools_history')}</span>
      {#if store.toolCallHistory.length > 0}
        <select
          class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-2 py-1 text-[12px] text-[var(--text-secondary)]"
          bind:value={roleFilter}
        >
          <option value="all">All Roles</option>
          {#each ROLE_COLUMNS as col}
            <option value={col.key}>{col.label}</option>
          {/each}
        </select>
      {/if}
    </div>

    {#if filteredHistory.length === 0}
      <div class="flex h-32 items-center justify-center">
        <p class="text-[13px] text-[var(--text-tertiary)]">{t('invest_tools_no_history')}</p>
      </div>
    {:else}
      <div class="max-h-96 overflow-y-auto">
        {#each filteredHistory as record, i}
          <div class="border-b border-border last:border-0">
            <button
              class="flex w-full items-center gap-2 px-[var(--space-4)] py-[var(--space-2)] text-left text-[13px] text-[var(--text-primary)] transition-colors hover:bg-[var(--bg-hover)]"
              onclick={() => (expandedIndex = expandedIndex === i ? null : i)}
            >
              <span class="inline-block rounded-[var(--radius-md)] px-1.5 py-0.5 text-[11px] font-medium" style={roleBadgeStyle(record.role)}>
                {roleLabel(record.role)}
              </span>
              <span class="font-[var(--font-mono)] text-[11px] text-[var(--text-secondary)]">{record.toolName}</span>
              <span class="ml-auto font-[var(--font-mono)] text-[11px] text-[var(--text-tertiary)]">{record.latencyMs}ms</span>
              <span class="inline-block h-2 w-2 rounded-full {record.success ? 'bg-[var(--color-success)]' : 'bg-[var(--color-error)]'}"></span>
              <svg class="h-3 w-3 text-[var(--text-tertiary)] transition-transform {expandedIndex === i ? 'rotate-90' : ''}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M9 18l6-6-6-6" />
              </svg>
            </button>

            {#if expandedIndex === i}
              <div class="space-y-2 border-t border-border bg-[var(--bg-hover)] px-[var(--space-4)] py-[var(--space-3)] text-[11px]">
                <div>
                  <span class="font-medium text-[var(--text-tertiary)]">{t('invest_tools_arguments')}: </span>
                  <span class="font-[var(--font-mono)] text-[var(--text-secondary)]">{formatArgs(record.arguments)}</span>
                </div>
                <div>
                  <span class="font-medium text-[var(--text-tertiary)]">{t('invest_tools_result')}: </span>
                  <span class={record.success ? 'text-[var(--color-success)]' : 'text-[var(--color-error)]'}>
                    {t(record.success ? 'invest_tools_success' : 'invest_tools_error')}
                  </span>
                </div>
                {#if record.result}
                  <div class="max-h-40 overflow-y-auto whitespace-pre-wrap rounded-[var(--radius-md)] bg-[var(--bg-input)] p-2 font-[var(--font-mono)] text-[11px] text-[var(--text-secondary)]">
                    {record.result}
                  </div>
                {/if}
                <div class="text-[var(--text-tertiary)]">
                  Round {record.round} | {record.latencyMs}ms
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>
