<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import type { ArchivedDecision, RoundOutputSummary, SymbolProgress } from '$lib/stores/invest-committee-store.svelte';
  import type { MessageKey } from '$lib/i18n/types';
  import DebateBlock from './DebateBlock.svelte';
  import { STEP_DEFS, roleToBackendIdx } from './pipeline-config';

  // ── Mode ───────────────────────────────────────────────────────────────────
  type ReplayMode = 'replay' | 'simulate';
  let mode = $state<ReplayMode>('replay');
  let manualMode = $state(false);
  let symbol = $state('');
  let loading = $state(false);
  let archives = $state<ArchivedDecision[]>([]);
  let selectedDate = $state<string | null>(null);

  const selectedArchive = $derived(
    archives.find((a) => a.date === selectedDate) ?? archives[0] ?? null,
  );

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
  const allHoldings = $derived.by(() => {
    const seen = new Set<string>();
    const result: Array<{ symbol: string; name: string; kind: string }> = [];
    for (const h of investStore.holdHoldings) {
      if (!seen.has(h.symbol)) {
        seen.add(h.symbol);
        result.push({ symbol: h.symbol, name: h.name ?? h.symbol, kind: 'HOLD' });
      }
    }
    for (const h of investStore.watchHoldings) {
      if (!seen.has(h.symbol)) {
        seen.add(h.symbol);
        result.push({ symbol: h.symbol, name: h.name ?? h.symbol, kind: 'WATCH' });
      }
    }
    return result;
  });

  const hasHoldings = $derived(allHoldings.length > 0);

  // ── Helpers ────────────────────────────────────────────────────────────────

  function getStepState(
    symProgress: SymbolProgress | undefined,
    backendIdx: number,
    pipelineStarted: boolean,
  ): 'pending' | 'active' | 'done' | 'error' {
    if (!symProgress) return 'pending';
    if (backendIdx === -1) return pipelineStarted ? 'done' : 'pending';
    if (symProgress.activeStep === backendIdx) return 'active';
    for (const round of symProgress.completedRounds) {
      if (roleToBackendIdx(round.role, round.round) === backendIdx) return 'done';
    }
    if (symProgress.done && !symProgress.error) return 'done';
    if (symProgress.error && backendIdx >= symProgress.completedSteps) return 'error';
    return 'pending';
  }

  function getRoundForStep(
    symProgress: SymbolProgress | undefined,
    backendIdx: number,
  ): RoundOutputSummary | undefined {
    if (!symProgress) return undefined;
    return symProgress.completedRounds.find(
      (r) => roleToBackendIdx(r.role, r.round) === backendIdx,
    );
  }

  function getVerdictColor(verdict: string): string {
    if (verdict === 'BUY' || verdict === 'ACCUMULATE')
      return 'bg-green-900/30 text-green-400 border-green-700';
    if (verdict === 'SELL' || verdict === 'TRIM')
      return 'bg-red-900/30 text-red-400 border-red-700';
    if (verdict === 'HOLD')
      return 'bg-yellow-900/30 text-yellow-400 border-yellow-700';
    if (verdict === 'WATCH')
      return 'bg-amber-900/30 text-amber-400 border-amber-700';
    return 'bg-slate-800 text-slate-300 border-slate-600';
  }

  // ── Replay: load archives ──────────────────────────────────────────────

  async function loadArchives() {
    if (!symbol.trim()) return;
    loading = true;
    archives = [];
    selectedDate = null;
    try {
      const loaded = await investCommitteeStore.loadArchive(symbol.trim(), 30);
      archives = loaded;
      // Default to the most recent date
      if (loaded.length > 0) {
        selectedDate = loaded[0].date;
      }
    } catch (e) {
      console.error('Failed to load archive:', e);
    } finally {
      loading = false;
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
      await investCommitteeStore.runCommittee([symbol.trim()], simulateRounds);
    } catch (e) {
      console.error('Simulation failed:', e);
    } finally {
      simulateRunning = false;
    }
  }

  // ── Derived for simulate mode ─────────────────────────────────────────────

  const simulateProgress = $derived(
    symbol.trim() ? investCommitteeStore.perSymbolProgress.get(symbol.trim()) : undefined,
  );
  const simulateResult = $derived(
    symbol.trim() ? investCommitteeStore.results.find((r) => r.symbol === symbol.trim()) : undefined,
  );
  const simulateStarted = $derived(
    investCommitteeStore.streaming || investCommitteeStore.results.length > 0,
  );

  // ── Mode switching ─────────────────────────────────────────────────────────

  function switchMode(newMode: ReplayMode) {
    if (mode === newMode) return;
    mode = newMode;
    // Reset state when switching modes
    archives = [];
    selectedDate = null;
    loading = false;
    symbol = '';
  }
</script>

<div class="replay-tab space-y-[var(--space-4)]">
  <!-- ── Top bar: mode tabs + symbol selector ────────────────────────────── -->
  <div class="flex items-center gap-[var(--space-3)]">
    <!-- Mode tabs -->
    <div class="flex rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-base)] p-0.5">
      <button
        class="rounded-[var(--radius-md)] px-[var(--space-3)] py-[var(--space-1)] text-xs font-medium transition-colors"
        class:bg-[var(--bg-card)]={mode === 'replay'}
        class:text-[var(--text-primary)]={mode === 'replay'}
        class:text-[var(--text-secondary)]={mode !== 'replay'}
        class:hover:text-[var(--text-primary)]={mode !== 'replay'}
        onclick={() => switchMode('replay')}
      >
        {t('invest_replay_tab_replay')}
      </button>
      <button
        class="rounded-[var(--radius-md)] px-[var(--space-3)] py-[var(--space-1)] text-xs font-medium transition-colors"
        class:bg-[var(--bg-card)]={mode === 'simulate'}
        class:text-[var(--text-primary)]={mode === 'simulate'}
        class:text-[var(--text-secondary)]={mode !== 'simulate'}
        class:hover:text-[var(--text-primary)]={mode !== 'simulate'}
        onclick={() => switchMode('simulate')}
      >
        {t('invest_replay_tab_simulate')}
      </button>
    </div>

    <!-- Symbol selector -->
    <div class="flex-1">
      {#if hasHoldings && !manualMode}
        <select
          class="w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-sm text-[var(--text-primary)]"
          bind:value={symbol}
        >
          <option value="">{t('invest_replay_select_placeholder')}</option>
          {#each allHoldings as h}
            <option value={h.symbol}>{h.name} ({h.symbol}) [{h.kind}]</option>
          {/each}
        </select>
        <button
          class="mt-1 text-xs text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
          onclick={() => { manualMode = true; symbol = ''; }}
        >
          {t('invest_replay_manual_input')}
        </button>
      {:else}
        <input
          type="text"
          class="w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-sm text-[var(--text-primary)]"
          placeholder="600519.SH"
          bind:value={symbol}
        />
        {#if hasHoldings}
          <button
            class="mt-1 text-xs text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
            onclick={() => { manualMode = false; symbol = ''; }}
          >
            {t('invest_replay_select_from_holdings')}
          </button>
        {/if}
      {/if}
    </div>
  </div>

  <!-- ═══════════════════════════════════════════════════════════════════════ -->
  <!-- MODE A: Real Replay                                                     -->
  <!-- ═══════════════════════════════════════════════════════════════════════ -->
  {#if mode === 'replay'}
    <!-- Loading / empty states -->
    {#if !symbol.trim()}
      <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-4)] py-8 text-center text-sm text-[var(--text-secondary)]">
        {t('invest_replay_empty')}
      </div>
    {:else if loading}
      <div class="flex items-center justify-center gap-2 rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-4)] py-8 text-sm text-[var(--text-secondary)]">
        <span class="inline-block h-4 w-4 animate-spin rounded-full border-2 border-[var(--border)] border-t-[var(--accent)]"></span>
        {t('invest_replay_loading')}
      </div>
    {:else if archives.length === 0}
      <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-4)] py-8 text-center text-sm text-[var(--text-secondary)]">
        {t('invest_replay_no_history')}
      </div>
    {:else}
      <!-- Date selector (when multiple dates available) -->
      {#if archives.length > 1}
        <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-4)] py-[var(--space-2)]">
          <div class="mb-1 text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">
            {t('invest_replay_browse_dates')}
          </div>
          <div class="flex flex-wrap gap-1.5">
            {#each archives as archive}
              <button
                class="rounded-[var(--radius-md)] border px-2.5 py-1 text-xs font-medium transition-colors {selectedDate === archive.date
                  ? 'border-[var(--accent)] bg-[var(--accent-muted)] text-[var(--accent)]'
                  : 'border-[var(--border)] text-[var(--text-secondary)] hover:border-[var(--text-tertiary)] hover:text-[var(--text-primary)]'}"
                onclick={() => (selectedDate = archive.date)}
              >
                {archive.date}
              </button>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Verdict info card -->
      {#if selectedArchive}
        <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-4)] py-[var(--space-3)]">
          <div class="mb-2 text-[11px] font-medium uppercase tracking-wider text-[var(--accent)]">
            {t('invest_replay_latest_verdict')}
          </div>
          <div class="flex items-center gap-[var(--space-3)] text-sm">
            <span class="font-medium text-[var(--text-primary)]">
              {allHoldings.find((h) => h.symbol === selectedArchive.symbol)?.name ?? selectedArchive.symbol}
            </span>
            <span class="font-[var(--font-mono)] text-xs text-[var(--text-secondary)]">{selectedArchive.symbol}</span>
            <span class="text-xs text-[var(--text-tertiary)]">{selectedArchive.date}</span>
          </div>
        </div>

        <!-- Discussion steps card -->
        <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-4)] py-[var(--space-3)]">
          <div class="mb-3 text-[11px] font-medium uppercase tracking-wider text-purple-500">
            {t('invest_replay_report_steps')}
          </div>
          <div class="max-h-[60vh] overflow-y-auto whitespace-pre-wrap font-[var(--font-mono)] text-sm leading-relaxed text-[var(--text-primary)]">
            {selectedArchive.content}
          </div>
        </div>
      {/if}
    {/if}

  <!-- ═══════════════════════════════════════════════════════════════════════ -->
  <!-- MODE B: Simulate Execution                                              -->
  <!-- ═══════════════════════════════════════════════════════════════════════ -->
  {:else}
    {#if !hasHoldings}
      <!-- No holdings -->
      <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-4)] py-8 text-center text-sm text-[var(--text-secondary)]">
        {t('invest_replay_no_holdings')}
      </div>
    {:else}
      <!-- Round selector + start button -->
      <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-4)] py-[var(--space-3)]">
        <div class="mb-2 text-[11px] font-medium uppercase tracking-wider text-[var(--text-secondary)]">
          {t('invest_replay_simulate_rounds')}
        </div>
        <div class="flex flex-wrap items-center gap-2">
          {#each ROUND_OPTIONS as opt}
            <button
              class="rounded-[var(--radius-md)] border px-[var(--space-3)] py-[var(--space-1)] text-xs font-medium transition-colors disabled:opacity-40 {simulateRounds === opt.value ? 'border-blue-500 bg-blue-500/10 text-blue-400' : 'border-[var(--border)] text-[var(--text-secondary)] hover:border-[var(--text-tertiary)]'}"
              disabled={simulateRunning}
              onclick={() => (simulateRounds = opt.value)}
            >
              {t(opt.descKey as MessageKey)}
            </button>
          {/each}
        </div>

        <div class="mt-3 flex items-center gap-[var(--space-3)]">
          <button
            class="flex items-center gap-2 rounded-[var(--radius-md)] px-[var(--space-4)] py-[var(--space-2)] text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-40"
            class:bg-[var(--color-success)]={!simulateRunning}
            class:hover:brightness-110={!simulateRunning}
            class:text-white={!simulateRunning}
            class:bg-[var(--bg-elevated)]={simulateRunning}
            class:text-[var(--text-primary)]={simulateRunning}
            disabled={simulateRunning || !symbol.trim()}
            onclick={startSimulation}
          >
            {#if simulateRunning}
              <span class="inline-block h-2 w-2 animate-pulse rounded-full bg-[var(--color-success)]"></span>
              {t('invest_replay_simulate_running')}
            {:else}
              {t('invest_replay_start_simulate')}
            {/if}
          </button>
        </div>
      </div>

      <!-- Empty state (no simulation started yet) -->
      {#if !simulateStarted && !simulateRunning}
        <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-4)] py-8 text-center text-sm text-[var(--text-secondary)]">
          {t('invest_replay_simulate_empty')}
        </div>
      {/if}

      <!-- Error banner -->
      {#if investCommitteeStore.runError}
        <div class="rounded-[var(--radius-lg)] border border-red-700/50 bg-[var(--color-error)]/10 px-[var(--space-3)] py-[var(--space-2)] text-sm text-[var(--color-error)]">
          {investCommitteeStore.runError}
        </div>
      {/if}

      <!-- Streaming step cards -->
      {#if simulateStarted && symbol.trim()}
        {@const p = simulateProgress}
        {@const result = simulateResult}
        {@const pipelineStarted = investCommitteeStore.streaming || (p?.completedSteps ?? 0) > 0}

        <!-- Step cards -->
        {#each STEP_DEFS as step, i}
          {@const state = getStepState(p, step.backendIdx, pipelineStarted)}
          {@const round = getRoundForStep(p, step.backendIdx)}

          {@const stepCardCls = state === 'done'
            ? 'border-[var(--color-success)]/40 bg-[var(--color-success)]/5'
            : state === 'active'
              ? 'border-blue-500/40 bg-blue-500/5'
              : state === 'error'
                ? 'border-[var(--color-error)]/40 bg-[var(--color-error)]/5'
                : 'border-[var(--border)] bg-[var(--bg-base)]/30'}

          <div class="relative rounded-[var(--radius-lg)] border p-[var(--space-3)] transition-colors duration-150 {stepCardCls}">
            <!-- Simulation watermark -->
            {#if result}
              <span class="absolute right-2 top-2 rounded bg-[var(--color-warning)]/20 px-1.5 py-0.5 text-[10px] font-bold text-[var(--color-warning)]">
                {t('invest_replay_simulate_watermark')}
              </span>
            {/if}

            {#if round}
              <DebateBlock {round} blockState={state} isStreaming={state === 'active'} />
            {:else}
              <!-- Step header (no round data yet) -->
              <div class="flex items-center gap-2">
                <span
                  class="text-xs font-semibold"
                  style="color: {step.color}"
                >
                  {t(step.labelKey)}
                </span>
                {#if state === 'active'}
                  <span class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-[var(--border)] border-t-blue-400"></span>
                  <span class="text-xs text-[var(--text-secondary)]">{t('invest_debate_waiting_llm')}</span>
                {:else if step.key === 'regime' && state === 'done' && p?.regimeData}
                  <!-- REGIME step with computed metrics -->
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

        <!-- CIO final verdict card (when done) -->
        {#if result}
          <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] p-[var(--space-4)]">
            <div class="mb-2 text-[11px] font-medium uppercase tracking-wider text-[var(--color-warning)]">
              {t('invest_replay_cio_verdict')}
            </div>

            <div class="mb-3 flex flex-wrap items-center gap-[var(--space-3)]">
              <span class="rounded-[var(--radius-full)] border px-3 py-1 text-[11px] font-bold {getVerdictColor(result.finalVerdict)}">
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
                <span class="rounded-[var(--radius-full)] bg-[var(--color-error)]/20 px-1.5 py-0.5 text-[10px] font-medium text-[var(--color-error)]">
                  {t('invest_replay_sentinel_override')}
                </span>
              {/if}
            </div>

            <!-- Sanity check gates -->
            <div class="mb-2 flex gap-3 text-[11px] text-[var(--text-tertiary)]">
              <span class:opacity-40={!result.sanityCheck.gate1Pass} title={t('invest_gate1_desc')}>
                {t('invest_gate1_label')} {result.sanityCheck.gate1Pass ? '✓' : '✗'}
              </span>
              <span class:opacity-40={!result.sanityCheck.gate2Pass} title={t('invest_gate2_desc')}>
                {t('invest_gate2_label')} {result.sanityCheck.gate2Pass ? '✓' : '✗'}
              </span>
              <span class:opacity-40={!result.sanityCheck.gate3Pass} title={t('invest_gate3_desc')}>
                {t('invest_gate3_label')} {result.sanityCheck.gate3Pass ? '✓' : '✗'}
              </span>
              <span class:opacity-40={!result.sanityCheck.gate4Pass} title={t('invest_gate4_desc')}>
                {t('invest_gate4_label')} {result.sanityCheck.gate4Pass ? '✓' : '✗'}
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
              <div class="mt-2 rounded bg-[var(--color-error)]/15 px-2 py-1 text-[11px] text-[var(--color-error)]">
                {result.sentinelOverride.reason}
              </div>
            {/if}
          </div>
        {/if}

        <!-- Disclaimer footer (only when done) -->
        {#if result && !investCommitteeStore.streaming}
          <div class="rounded-[var(--radius-lg)] border border-[var(--color-warning)]/30 bg-[var(--color-warning)]/10 px-[var(--space-3)] py-[var(--space-2)] text-xs text-[var(--color-warning)]/80">
            ⚠️ {t('invest_replay_simulate_disclaimer')}
          </div>
        {/if}
      {/if}
    {/if}
  {/if}
</div>
