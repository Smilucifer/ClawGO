<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import type { ArchivedDecision } from '$lib/stores/invest-committee-store.svelte';
  import type { MessageKey } from '$lib/i18n/types';
  import DebateBlock from './DebateBlock.svelte';
  import { STEP_DEFS, getStepState, getRoundForStep } from './pipeline-config';
  import { getVerdictBadgeStyle, buildVerdictMap } from '$lib/utils/invest-verdict';
  import MarkdownContent from '$lib/components/MarkdownContent.svelte';

  // ── Mode ───────────────────────────────────────────────────────────────────
  type ReplayMode = 'replay' | 'simulate';
  let mode = $state<ReplayMode>('replay');
  let symbol = $state('');
  let loading = $state(false);
  let archives = $state<ArchivedDecision[]>([]);
  let selectedDate = $state<string | null>(null);

  const selectedArchive = $derived(
    archives.find((a) => a.date === selectedDate) ?? archives[0] ?? null,
  );

  // Pre-compute verdicts once per archives change — avoids re-running regex
  // on every selectedDate click for the entire list.
  const verdictMap = $derived.by(() => buildVerdictMap(archives));

  // Simulate mode state
  let simulateRounds = $state(2);
  let simulateRunning = $state(false);

  const ROUND_OPTIONS = [
    { value: 1, descKey: 'invest_replay_simulate_round_1' },
    { value: 2, descKey: 'invest_replay_simulate_round_2' },
    { value: 4, descKey: 'invest_replay_simulate_round_4' },
    { value: 6, descKey: 'invest_replay_simulate_round_6' },
    { value: 8, descKey: 'invest_replay_simulate_round_8' },
  ] as const;

  // ── Holdings ───────────────────────────────────────────────────────────────
  // store.holdHoldings / watchHoldings are already deduplicated (hold takes priority)
  const allHoldings = $derived([
    ...investStore.holdHoldings.map((h) => ({ symbol: h.symbol, name: h.name ?? h.symbol, kind: 'HOLD' })),
    ...investStore.watchHoldings.map((h) => ({ symbol: h.symbol, name: h.name ?? h.symbol, kind: 'WATCH' })),
  ]);

  const hasHoldings = $derived(allHoldings.length > 0);

  // ── Replay: load archives ──────────────────────────────────────────────
  // Generation counter prevents stale-response races: rapid symbol clicks
  // discard responses for previous symbols.

  let loadGen = 0;

  async function loadArchives() {
    if (!symbol.trim()) return;
    const myGen = ++loadGen;
    loading = true;
    archives = [];
    selectedDate = null;
    try {
      const loaded = await investCommitteeStore.loadArchive(symbol.trim(), 30);
      if (myGen !== loadGen) return; // stale, discard
      archives = loaded;
      if (loaded.length > 0) {
        selectedDate = loaded[0].date;
      }
    } catch (e) {
      if (myGen !== loadGen) return;
      console.error('Failed to load archive:', e);
    } finally {
      if (myGen === loadGen) loading = false;
    }
  }

  // Auto-load when symbol changes in replay mode
  $effect(() => {
    if (mode === 'replay' && symbol.trim()) {
      loadArchives();
    } else {
      archives = [];
      selectedDate = null;
    }
  });

  // ── Simulate: run committee ────────────────────────────────────────────────

  async function startSimulation() {
    if (!symbol.trim()) return;
    simulateRunning = true;
    try {
      await investCommitteeStore.addToQueue([symbol.trim()]);
    } catch (e) {
      console.error('Simulation failed:', e);
    } finally {
      simulateRunning = false;
    }
  }

  const simulateProgress = $derived(
    symbol.trim() ? investCommitteeStore.perSymbolProgress.get(symbol.trim()) : undefined,
  );
  const simulateResult = $derived(
    symbol.trim() ? investCommitteeStore.results.find((r) => r.symbol === symbol.trim()) : undefined,
  );
  const simulateStarted = $derived(
    investCommitteeStore.streaming || investCommitteeStore.results.length > 0,
  );

  function switchMode(newMode: ReplayMode) {
    if (mode === newMode) return;
    mode = newMode;
    archives = [];
    selectedDate = null;
    loading = false;
    symbol = '';
  }
</script>

<div class="flex h-full flex-col gap-[var(--space-3)]">
  <!-- Mode toggle -->
  <div class="flex w-fit rounded-[var(--radius-lg)] border border-border bg-[var(--bg-base)] p-0.5">
    <button
      class="rounded-[var(--radius-md)] px-[var(--space-3)] py-[var(--space-1)] text-xs font-medium transition-colors"
      class:bg-[var(--bg-card)]={mode === 'replay'}
      class:text-[var(--text-primary)]={mode === 'replay'}
      class:text-[var(--text-secondary)]={mode !== 'replay'}
      class:hover:text-[var(--text-primary)]={mode !== 'replay'}
      onclick={() => switchMode('replay')}
    >
      📖 {t('invest_replay_tab_replay')}
    </button>
    <button
      class="rounded-[var(--radius-md)] px-[var(--space-3)] py-[var(--space-1)] text-xs font-medium transition-colors"
      class:bg-[var(--bg-card)]={mode === 'simulate'}
      class:text-[var(--text-primary)]={mode === 'simulate'}
      class:text-[var(--text-secondary)]={mode !== 'simulate'}
      class:hover:text-[var(--text-primary)]={mode !== 'simulate'}
      onclick={() => switchMode('simulate')}
    >
      ⚡ {t('invest_replay_tab_simulate')}
    </button>
  </div>

  <!-- ═══════════════════════════════════════════════════════════════════════ -->
  <!-- MODE A: Real Replay — two-column layout (250px / 1fr)                   -->
  <!-- ═══════════════════════════════════════════════════════════════════════ -->
  {#if mode === 'replay'}
    <div class="replay-grid grid min-h-0 flex-1 gap-[var(--space-4)]">
      <!-- Sidebar: symbol list + history dates -->
      <aside class="flex flex-col rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-3)] overflow-y-auto">
        <div class="mb-[var(--space-2)] px-[var(--space-2)] text-[12px] font-semibold text-[var(--text-primary)]">
          {t('invest_replay_symbol_label')}
        </div>

        {#if hasHoldings}
          <div class="space-y-0.5">
            {#each allHoldings as h}
              <button
                class="flex w-full items-center justify-between rounded-[var(--radius-md)] px-[var(--space-2)] py-[var(--space-1)] text-left text-[12px] transition-colors"
                class:bg-[var(--accent-muted)]={symbol === h.symbol}
                class:text-[var(--accent)]={symbol === h.symbol}
                class:text-[var(--text-secondary)]={symbol !== h.symbol}
                class:hover:bg-[var(--bg-hover)]={symbol !== h.symbol}
                onclick={() => (symbol = h.symbol)}
              >
                <span class="truncate">
                  <span class="font-medium">{h.name || h.symbol}</span>
                  {#if h.name}
                    <span class="ml-1 font-[var(--font-mono)] text-[10px] opacity-50">{h.symbol}</span>
                  {/if}
                </span>
                <span class="ml-2 shrink-0 rounded-[var(--radius-sm)] px-1.5 py-0.5 text-[9px] font-semibold {h.kind === 'HOLD' ? 'bg-[rgba(138,154,118,0.15)] text-[#8a9a76]' : 'bg-[var(--accent-muted)] text-[var(--accent)]'}">
                  {h.kind}
                </span>
              </button>
            {/each}
          </div>
        {:else}
          <div class="px-[var(--space-2)] py-[var(--space-3)] text-[11px] text-[var(--text-tertiary)]">
            {t('invest_replay_no_holdings')}
          </div>
        {/if}

        <!-- History dates -->
        {#if symbol.trim()}
          <div class="mt-[var(--space-3)] border-t border-border pt-[var(--space-2)]">
            <div class="mb-[var(--space-1)] px-[var(--space-2)] text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">
              {t('invest_replay_browse_dates')}
            </div>
            {#if loading}
              <div class="flex items-center gap-2 px-[var(--space-2)] py-[var(--space-2)] text-[11px] text-[var(--text-tertiary)]">
                <span class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-border border-t-[var(--accent)]"></span>
                {t('invest_replay_loading')}
              </div>
            {:else if archives.length === 0}
              <div class="px-[var(--space-2)] py-[var(--space-2)] text-[11px] text-[var(--text-tertiary)]">
                {t('invest_replay_no_history')}
              </div>
            {:else}
              <div class="space-y-0.5">
                {#each archives as archive}
                  {@const v = verdictMap.get(archive.date) ?? null}
                  <button
                    class="flex w-full items-center justify-between rounded-[var(--radius-md)] px-[var(--space-2)] py-[var(--space-1)] text-left text-[12px] font-[var(--font-mono)] transition-colors"
                    class:bg-[var(--accent-muted)]={selectedDate === archive.date}
                    class:text-[var(--accent)]={selectedDate === archive.date}
                    class:text-[var(--text-secondary)]={selectedDate !== archive.date}
                    class:hover:bg-[var(--bg-hover)]={selectedDate !== archive.date}
                    onclick={() => (selectedDate = archive.date)}
                  >
                    <span>{archive.date}</span>
                    {#if v}
                      <span class="rounded-[var(--radius-sm)] px-1.5 py-0.5 text-[9px] font-bold" style={getVerdictBadgeStyle(v)}>
                        {v}
                      </span>
                    {/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </aside>

      <!-- Content: archive detail -->
      <section class="flex min-h-0 flex-1 flex-col rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)] overflow-y-auto">
        {#if !symbol.trim()}
          <div class="flex h-full items-center justify-center py-12 text-center text-sm text-[var(--text-secondary)]">
            {t('invest_replay_empty')}
          </div>
        {:else if !selectedArchive}
          <div class="flex h-full items-center justify-center py-12 text-center text-sm text-[var(--text-tertiary)]">
            {t('invest_archive_select')}
          </div>
        {:else}
          {@const v = verdictMap.get(selectedArchive.date) ?? null}
          <div class="mb-[var(--space-3)] flex items-center gap-[var(--space-3)]">
            <span class="text-[14px] font-semibold text-[var(--text-primary)]">
              {allHoldings.find((h) => h.symbol === selectedArchive.symbol)?.name ?? selectedArchive.symbol}
            </span>
            <span class="font-[var(--font-mono)] text-xs text-[var(--text-secondary)]">{selectedArchive.symbol}</span>
            <span class="text-xs text-[var(--text-tertiary)]">— {selectedArchive.date}</span>
            {#if v}
              <span class="ml-auto rounded-[var(--radius-full)] px-3 py-1 text-[11px] font-bold" style={getVerdictBadgeStyle(v)}>
                {v}
              </span>
            {/if}
          </div>
          <div class="min-h-0 flex-1 overflow-y-auto text-[13px] leading-[1.7]">
            <MarkdownContent text={selectedArchive.content} />
          </div>
        {/if}
      </section>
    </div>

  <!-- ═══════════════════════════════════════════════════════════════════════ -->
  <!-- MODE B: Simulate Execution                                              -->
  <!-- ═══════════════════════════════════════════════════════════════════════ -->
  {:else}
    <!-- Symbol selector for simulate mode -->
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] px-[var(--space-4)] py-[var(--space-3)]">
      <div class="mb-[var(--space-2)] text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">
        {t('invest_replay_symbol_label')}
      </div>
      {#if hasHoldings}
        <select
          class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-sm text-[var(--text-primary)]"
          bind:value={symbol}
        >
          <option value="">{t('invest_replay_select_placeholder')}</option>
          {#each allHoldings as h}
            <option value={h.symbol}>{h.name} ({h.symbol}) [{h.kind}]</option>
          {/each}
        </select>
      {:else}
        <input
          type="text"
          class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-sm text-[var(--text-primary)]"
          placeholder="600519.SH"
          bind:value={symbol}
        />
      {/if}
    </div>

    {#if !hasHoldings && !symbol.trim()}
      <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] px-[var(--space-4)] py-8 text-center text-sm text-[var(--text-secondary)]">
        {t('invest_replay_no_holdings')}
      </div>
    {:else}
      <!-- Round selector + start button -->
      <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] px-[var(--space-4)] py-[var(--space-3)]">
        <div class="mb-2 text-[11px] font-medium uppercase tracking-wider text-[var(--text-secondary)]">
          {t('invest_replay_simulate_rounds')}
        </div>
        <div class="flex flex-wrap items-center gap-2">
          {#each ROUND_OPTIONS as opt}
            <button
              class="rounded-[var(--radius-md)] border px-[var(--space-3)] py-[var(--space-1)] text-xs font-medium transition-colors disabled:opacity-40 {simulateRounds === opt.value ? 'border-[var(--accent)] bg-[var(--accent-muted)] text-[var(--accent)]' : 'border-border text-[var(--text-secondary)] hover:border-[var(--text-tertiary)]'}"
              disabled={simulateRunning}
              onclick={() => (simulateRounds = opt.value)}
            >
              {t(opt.descKey as MessageKey)}
            </button>
          {/each}
        </div>

        <div class="mt-3 flex items-center gap-[var(--space-3)]">
          <button
            class="flex items-center gap-2 rounded-[var(--radius-md)] px-[var(--space-4)] py-[var(--space-2)] text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-40 {simulateRunning ? 'bg-[var(--bg-elevated)] text-[var(--text-primary)]' : 'bg-[var(--color-success)] text-white hover:brightness-110'}"
            disabled={simulateRunning || !symbol.trim()}
            onclick={startSimulation}
          >
            {#if simulateRunning}
              <span class="inline-block h-2 w-2 animate-pulse rounded-full bg-white"></span>
              {t('invest_replay_simulate_running')}
            {:else}
              {t('invest_replay_start_simulate')}
            {/if}
          </button>
        </div>
      </div>

      {#if !simulateStarted && !simulateRunning}
        <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] px-[var(--space-4)] py-8 text-center text-sm text-[var(--text-secondary)]">
          {t('invest_replay_simulate_empty')}
        </div>
      {/if}

      {#if investCommitteeStore.runError}
        <div class="rounded-[var(--radius-lg)] border border-[var(--color-error)]/50 bg-[var(--color-error-bg)] px-[var(--space-3)] py-[var(--space-2)] text-sm text-[var(--color-error)]">
          {investCommitteeStore.runError}
        </div>
      {/if}

      {#if simulateStarted && symbol.trim()}
        {@const p = simulateProgress}
        {@const result = simulateResult}
        {@const pipelineStarted = investCommitteeStore.streaming || (p?.completedSteps ?? 0) > 0}

        {#each STEP_DEFS as step}
          {@const state = getStepState(p, step.backendIdx, pipelineStarted)}
          {@const round = getRoundForStep(p, step.backendIdx)}

          {@const stepCardCls = state === 'done'
            ? 'border-[var(--color-success)]/40 bg-[var(--color-success)]/5'
            : state === 'active'
              ? 'border-blue-500/40 bg-blue-500/5'
              : state === 'error'
                ? 'border-[var(--color-error)]/40 bg-[var(--color-error-bg)]'
                : 'border-border bg-[var(--bg-base)]/30'}

          <div class="relative rounded-[var(--radius-lg)] border p-[var(--space-3)] transition-colors duration-150 {stepCardCls}">
            {#if result}
              <span class="absolute right-2 top-2 rounded bg-[var(--color-warning)]/20 px-1.5 py-0.5 text-[10px] font-bold text-[var(--color-warning)]">
                {t('invest_replay_simulate_watermark')}
              </span>
            {/if}

            {#if round}
              <DebateBlock {round} blockState={state} isStreaming={state === 'active'} />
            {:else}
              <div class="flex items-center gap-2">
                <span class="text-xs font-semibold" style="color: {step.color}">
                  {t(step.labelKey)}
                </span>
                {#if state === 'active'}
                  <span class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-border border-t-blue-400"></span>
                  <span class="text-xs text-[var(--text-secondary)]">{t('invest_debate_waiting_llm')}</span>
                {:else if step.key === 'regime' && state === 'done' && p?.regimeData}
                  <span class="text-[10px] text-[var(--color-success)]">✓</span>
                  <div class="mt-1 w-full space-y-1 text-xs">
                    <div class="flex flex-wrap gap-x-3 gap-y-1">
                      <span class="text-[var(--text-secondary)]">
                        {t('invest_regime_label')}: <span class="font-medium text-[var(--accent)]">{p.regimeData.regime}</span>
                      </span>
                      <span class="text-[var(--text-secondary)]">
                        {t('invest_regime_reason')}: <span class="text-[var(--text-primary)]">{p.regimeData.reason}</span>
                      </span>
                    </div>
                    <div class="text-[11px] text-[var(--text-tertiary)]">
                      RSI-14: {p.regimeData.metrics.rsi14.toFixed(1)} ·
                      MA20: {p.regimeData.metrics.ma20.toFixed(2)} ·
                      MA60: {p.regimeData.metrics.ma60.toFixed(2)} ·
                      Vol: {(p.regimeData.metrics.volatilityAnn * 100).toFixed(1)}%
                    </div>
                  </div>
                {:else if state === 'done' && step.key === 'regime'}
                  <span class="text-[10px] text-[var(--color-success)]">✓</span>
                  <span class="text-xs text-[var(--text-tertiary)]">{t('invest_committee_regime_computed')}</span>
                {:else if state === 'pending'}
                  <span class="text-xs text-[var(--text-tertiary)]">{t('invest_overview_pending')}</span>
                {:else if state === 'error'}
                  <span class="text-[10px] text-[var(--color-error)]">✗</span>
                {/if}
              </div>
            {/if}
          </div>
        {/each}

        {#if result}
          <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
            <div class="mb-2 text-[11px] font-medium uppercase tracking-wider text-[var(--color-warning)]">
              {t('invest_replay_cio_verdict')}
            </div>

            <div class="mb-3 flex flex-wrap items-center gap-[var(--space-3)]">
              <span class="rounded-[var(--radius-full)] px-3 py-1 text-[11px] font-bold" style={getVerdictBadgeStyle(result.finalVerdict)}>
                {result.finalVerdict}
              </span>
              <span class="text-sm text-[var(--text-secondary)]">
                {t('invest_macro_signal')}: {(result.finalConfidence * 100).toFixed(0)}%
              </span>
              {#if result.converged}
                <span class="rounded-[var(--radius-full)] bg-[var(--color-success)]/20 px-1.5 py-0.5 text-[10px] font-medium text-[var(--color-success)]">
                  {t('invest_replay_converged')}
                </span>
              {/if}
              {#if result.sentinelOverride}
                <span class="rounded-[var(--radius-full)] bg-[var(--color-error-bg)] px-1.5 py-0.5 text-[10px] font-medium text-[var(--color-error)]">
                  {t('invest_replay_sentinel_override')}
                </span>
              {/if}
            </div>

            <div class="mb-2 flex gap-3 text-[11px] text-[var(--text-tertiary)]">
              <span class:opacity-40={!result.sanityCheck.gate1Pass} title={t('invest_gate1_desc')}>
                {t('invest_gate1_label')} {result.sanityCheck.gate1Pass ? '✓' : '✗'}
              </span>
              <span class:opacity-40={!result.sanityCheck.gate2Pass} title={t('invest_gate2_desc')}>
                {t('invest_gate2_label')} {result.sanityCheck.gate2Pass ? '✓' : '✗'}
              </span>
              <span class="ml-auto font-[var(--font-mono)]">
                {result.totalTokens} tok / {(result.totalLatencyMs / 1000).toFixed(1)}s
              </span>
            </div>

            {#if result.reasoning}
              <div class="max-h-32 overflow-y-auto whitespace-pre-wrap text-xs leading-relaxed text-[var(--text-secondary)]">
                {result.reasoning}
              </div>
            {/if}

            {#if result.sanityCheck.notes.length > 0}
              <div class="mt-2 space-y-0.5 text-[11px] text-[var(--text-tertiary)]">
                {#each result.sanityCheck.notes as note}
                  <div>- {note}</div>
                {/each}
              </div>
            {/if}

            {#if result.sentinelOverride}
              <div class="mt-2 rounded bg-[var(--color-error-bg)] px-2 py-1 text-[11px] text-[var(--color-error)]">
                {result.sentinelOverride.reason}
              </div>
            {/if}
          </div>
        {/if}

        {#if result && !investCommitteeStore.streaming}
          <div class="rounded-[var(--radius-lg)] border border-[var(--color-warning)]/30 bg-[var(--color-warning)]/10 px-[var(--space-3)] py-[var(--space-2)] text-xs text-[var(--color-warning)]/80">
            ⚠️ {t('invest_replay_simulate_disclaimer')}
          </div>
        {/if}
      {/if}
    {/if}
  {/if}
</div>

<style>
  .replay-grid {
    grid-template-columns: 250px 1fr;
    grid-template-rows: 1fr;
  }
  @media (max-width: 768px) {
    .replay-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
