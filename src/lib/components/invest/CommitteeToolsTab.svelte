<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investCommitteeStore, type ToolCallRecord } from '$lib/stores/invest-committee-store.svelte';

  const store = investCommitteeStore;

  // ── Static role→tool mapping (mirrors src-tauri/src/invest/committee/tools.rs) ──

  interface ToolDef {
    name: string;
    desc: string;
  }

  const ALL_TOOLS: ToolDef[] = [
    { name: 'get_history_data', desc: '获取指定股票的历史行情数据（日线）' },
    { name: 'analyze_multi_timeframe', desc: '对股票进行多时间框架技术分析（5日/20日/60日）' },
    { name: 'get_macro_snapshot', desc: '获取A股宏观指标快照' },
    { name: 'query_dreaming_insights', desc: '查询投资洞察和历史裁决' },
    { name: 'get_recent_committee_verdicts', desc: '获取近期委员会裁决记录' },
  ];

  interface RoleToolAccess {
    role: string;
    label: string;
    tools: string[];
    r2Note?: string;
  }

  const ROLE_ACCESS: RoleToolAccess[] = [
    { role: 'macro', label: '宏观分析师 (Macro)', tools: ['get_history_data', 'analyze_multi_timeframe', 'get_macro_snapshot', 'query_dreaming_insights', 'get_recent_committee_verdicts'], r2Note: 'R2 不可用工具' },
    { role: 'quant', label: '量化分析师 (Quant)', tools: ['get_history_data', 'analyze_multi_timeframe', 'get_recent_committee_verdicts'], r2Note: 'R1+R2 均可用' },
    { role: 'risk', label: '风控官 (Risk)', tools: ['query_dreaming_insights', 'get_recent_committee_verdicts'], r2Note: 'R1+R2 均可用' },
    { role: 'cio', label: '首席投资官 (CIO)', tools: [], r2Note: '禁止调用工具' },
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

  function formatResult(raw: string | undefined): string {
    if (!raw) return '-';
    // Truncate long results for display
    return raw.length > 200 ? raw.slice(0, 200) + '...' : raw;
  }

  function truncateResult(raw: string | undefined): string {
    if (!raw) return '';
    return raw;
  }

  function roleLabel(role: string): string {
    switch (role) {
      case 'macro': return 'Macro';
      case 'quant': return 'Quant';
      case 'risk': return 'Risk';
      case 'cio': return 'CIO';
      default: return role;
    }
  }

  function roleColor(role: string): string {
    switch (role) {
      case 'macro': return 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400';
      case 'quant': return 'bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400';
      case 'risk': return 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400';
      case 'cio': return 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400';
      default: return 'bg-gray-100 text-gray-700 dark:bg-gray-900/30 dark:text-gray-400';
    }
  }
</script>

<div class="space-y-4">
  <!-- Header -->
  <div class="flex items-center justify-between">
    <div>
      <h3 class="text-lg font-semibold">{t('invest_tools_title')}</h3>
      <p class="text-sm text-muted-foreground">{t('invest_tools_desc')}</p>
    </div>
    {#if store.toolCallHistory.length > 0}
      <button
        class="rounded border border-border px-3 py-1.5 text-xs text-muted-foreground hover:bg-muted/50"
        onclick={() => { store.toolCallHistory = []; expandedIndex = null; }}
      >
        {t('invest_tools_clear')}
      </button>
    {/if}
  </div>

  <!-- KPI cards -->
  <div class="grid grid-cols-3 gap-3">
    <div class="rounded-lg border p-3 text-center">
      <div class="text-2xl font-bold">{totalCalls}</div>
      <div class="text-xs text-muted-foreground">{t('invest_tools_total_calls')}</div>
    </div>
    <div class="rounded-lg border p-3 text-center">
      <div class="text-2xl font-bold {successRate >= 0.9 ? 'text-green-500' : successRate >= 0.7 ? 'text-amber-500' : 'text-red-500'}">
        {totalCalls > 0 ? `${(successRate * 100).toFixed(0)}%` : '-'}
      </div>
      <div class="text-xs text-muted-foreground">{t('invest_tools_success_rate')}</div>
    </div>
    <div class="rounded-lg border p-3 text-center">
      <div class="text-2xl font-bold">{totalCalls > 0 ? `${avgLatency}ms` : '-'}</div>
      <div class="text-xs text-muted-foreground">{t('invest_tools_avg_latency')}</div>
    </div>
  </div>

  <!-- Role → Tool Access mapping -->
  <div class="rounded-lg border">
    <div class="border-b bg-muted/50 px-4 py-2 text-sm font-medium">{t('invest_tools_role_mapping')}</div>
    <div class="p-4 space-y-3">
      {#each ROLE_ACCESS as access}
        <div class="flex items-start gap-3">
          <span class="inline-block min-w-[120px] shrink-0 rounded px-2 py-0.5 text-xs font-medium {roleColor(access.role)}">
            {access.label}
          </span>
          <div class="flex-1">
            {#if access.tools.length > 0}
              <div class="flex flex-wrap gap-1">
                {#each access.tools as toolName}
                  <span class="inline-block rounded bg-muted px-2 py-0.5 text-xs font-mono">
                    {toolName}
                  </span>
                {/each}
              </div>
            {:else}
              <span class="text-xs text-muted-foreground italic">{t('invest_tools_none')}</span>
            {/if}
            {#if access.r2Note}
              <div class="mt-1 text-xs text-muted-foreground">{access.r2Note}</div>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  </div>

  <!-- Per-role stats when there's history -->
  {#if store.toolCallHistory.length > 0}
    <div class="rounded-lg border">
      <div class="border-b bg-muted/50 px-4 py-2 text-sm font-medium">{t('invest_tools_by_role')}</div>
      <div class="p-4 grid grid-cols-4 gap-3">
        {#each ROLE_ACCESS as access}
          {@const stats = roleStats.get(access.role)}
          <div class="rounded border p-2 text-center">
            <div class="text-xs font-medium {roleColor(access.role)} inline-block rounded px-1.5 py-0.5 mb-1">
              {roleLabel(access.role)}
            </div>
            <div class="text-lg font-bold">{stats?.calls ?? 0}</div>
            <div class="text-xs text-muted-foreground">
              {#if stats && stats.errors > 0}
                <span class="text-red-500">{stats.errors} err</span>
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
  <div class="rounded-lg border">
    <div class="border-b bg-muted/50 px-4 py-2 flex items-center justify-between">
      <span class="text-sm font-medium">{t('invest_tools_history')}</span>
      {#if store.toolCallHistory.length > 0}
        <select
          class="rounded border border-border bg-background px-2 py-1 text-xs"
          bind:value={roleFilter}
        >
          <option value="all">All Roles</option>
          {#each ROLE_ACCESS as access}
            <option value={access.role}>{roleLabel(access.role)}</option>
          {/each}
        </select>
      {/if}
    </div>

    {#if filteredHistory.length === 0}
      <div class="flex h-32 items-center justify-center">
        <p class="text-sm text-muted-foreground">{t('invest_tools_no_history')}</p>
      </div>
    {:else}
      <div class="max-h-96 overflow-y-auto">
        {#each filteredHistory as record, i}
          <div class="border-b border-border/50 last:border-0">
            <!-- Summary row (clickable) -->
            <button
              class="flex w-full items-center gap-2 px-4 py-2 text-left text-sm hover:bg-muted/20 transition-colors"
              onclick={() => (expandedIndex = expandedIndex === i ? null : i)}
            >
              <span class="inline-block rounded px-1.5 py-0.5 text-xs font-medium {roleColor(record.role)}">
                {roleLabel(record.role)}
              </span>
              <span class="font-mono text-xs">{record.toolName}</span>
              <span class="ml-auto text-xs text-muted-foreground">{record.latencyMs}ms</span>
              <span class="inline-block h-2 w-2 rounded-full {record.success ? 'bg-green-500' : 'bg-red-500'}"></span>
              <svg class="h-3 w-3 text-muted-foreground transition-transform {expandedIndex === i ? 'rotate-90' : ''}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M9 18l6-6-6-6" />
              </svg>
            </button>

            <!-- Expanded details -->
            {#if expandedIndex === i}
              <div class="border-t border-border/30 bg-muted/10 px-4 py-3 space-y-2 text-xs">
                <div>
                  <span class="font-medium text-muted-foreground">{t('invest_tools_arguments')}: </span>
                  <span class="font-mono">{formatArgs(record.arguments)}</span>
                </div>
                <div>
                  <span class="font-medium text-muted-foreground">{t('invest_tools_result')}: </span>
                  <span class="{record.success ? 'text-green-600 dark:text-green-400' : 'text-red-600 dark:text-red-400'}">
                    {t(record.success ? 'invest_tools_success' : 'invest_tools_error')}
                  </span>
                </div>
                {#if record.result}
                  <div class="rounded bg-muted/50 p-2 font-mono text-xs whitespace-pre-wrap max-h-40 overflow-y-auto">
                    {truncateResult(record.result)}
                  </div>
                {/if}
                <div class="text-muted-foreground">
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
