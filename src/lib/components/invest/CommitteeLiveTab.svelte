<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import type { SymbolProgress, RoundOutputSummary } from '$lib/stores/invest-committee-store.svelte';
  import { STEP_DEFS, roleToBackendIdx } from './pipeline-config';

  let expandedSymbol = $state<string | null>(null);
  let includeWatch = $state(true);

  const store = investCommitteeStore;
  const invest = investStore;

  // ── Derived ──────────────────────────────────────────────────────────────

  const allAssets = $derived.by(() => {
    const assets: { symbol: string; name: string | null; kind: 'hold' | 'watch' }[] = [
      ...invest.holdHoldings.map((h) => ({ symbol: h.symbol, name: h.name, kind: h.kind })),
    ];
    if (includeWatch) {
      assets.push(
        ...invest.watchHoldings.map((h) => ({ symbol: h.symbol, name: h.name, kind: h.kind })),
      );
    }
    return assets;
  });

  const holdCount = $derived(invest.holdCount);
  const watchCount = $derived(invest.watchCount);

  const portfolioStats = $derived.by(() => {
    const hv = invest.holdingsMarketValue;
    const cashVal = invest.cash;
    const total = invest.totalAssets;
    const ret = invest.totalReturnPct;

    let maxHolding = { name: '', pct: 0 };
    if (hv > 0) {
      for (const h of invest.holdHoldings) {
        const price = invest.priceMap[h.symbol]?.close;
        const val = price && h.shares ? price * h.shares : h.notional || 0;
        const pct = (val / hv) * 100;
        if (pct > maxHolding.pct) {
          maxHolding = { name: h.name || h.symbol, pct };
        }
      }
    }

    return { hv, cash: cashVal, total, ret, concentration: maxHolding };
  });

  const pipelineStarted = $derived(store.streaming || store.results.length > 0);

  const completedCount = $derived.by(() => {
    let count = 0;
    for (const [, p] of store.perSymbolProgress) {
      if (p.done) count++;
    }
    return count;
  });

  // ── Helpers ──────────────────────────────────────────────────────────────

  function getStepState(
    symProgress: SymbolProgress | undefined,
    backendIdx: number,
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

  function getVerdictColorClass(verdict: string): string {
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

  async function runAll() {
    const syms = allAssets.map((a) => a.symbol);
    if (syms.length === 0) return;
    expandedSymbol = syms[0];
    await store.runCommittee(syms);
  }

  function toggleExpand(symbol: string) {
    expandedSymbol = expandedSymbol === symbol ? null : symbol;
  }
</script>

<div class="committee-live space-y-3">
  <!-- ── Top action bar ─────────────────────────────────────────────────── -->
  <div class="flex items-center gap-3 rounded-lg border border-[#334155] bg-[#0F172A] px-4 py-3">
    <button
      class="flex items-center gap-2 rounded-md px-4 py-2 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-40"
      class:bg-green-600={!store.running}
      class:hover:bg-green-500={!store.running}
      class:text-white={!store.running}
      class:bg-slate-700={store.running}
      class:text-slate-300={store.running}
      disabled={store.running || allAssets.length === 0}
      onclick={runAll}
    >
      {#if store.running}
        <span class="inline-block h-2 w-2 animate-pulse rounded-full bg-green-400"></span>
        {t('invest_committee_running_progress', {
          current: String(completedCount),
          total: String(allAssets.length),
        })}
      {:else}
        {t('invest_run_all_holdings')}
        ({holdCount}{watchCount > 0 ? ` + ${watchCount}W` : ''})
      {/if}
    </button>

    <label class="flex items-center gap-1.5 text-xs text-slate-400">
      <input type="checkbox" bind:checked={includeWatch} disabled={store.running} class="accent-green-500" />
      {t('invest_include_watch')}
    </label>

    {#if store.streaming}
      <span class="ml-auto text-xs text-slate-500">
        {completedCount}/{allAssets.length}
      </span>
    {/if}
  </div>

  <!-- ── Portfolio summary card (shared) ────────────────────────────────── -->
  {#if holdCount > 0}
    <div class="rounded-lg border border-[#334155] bg-[#0F172A] px-4 py-3">
      <div class="mb-2 text-xs font-medium uppercase tracking-wider text-[#06b6d4]">
        {t('invest_committee_portfolio_summary')}
      </div>
      <div class="flex flex-wrap gap-x-6 gap-y-1 text-sm">
        <span class="text-slate-300">
          {holdCount} {t('invest_committee_steps')}
          {#if watchCount > 0}
            <span class="text-slate-500">+ {watchCount} {t('invest_include_watch')}</span>
          {/if}
        </span>
        <span class="text-slate-300">
          {t('invest_committee_total_value')}: <span class="font-mono text-white">¥{portfolioStats.hv.toLocaleString()}</span>
        </span>
        <span class="text-slate-300">
          {t('invest_committee_emergency')}: <span class="font-mono text-white">¥{portfolioStats.cash.toLocaleString()}</span>
        </span>
        {#if portfolioStats.concentration.pct > 0}
          <span
            class:text-amber-400={portfolioStats.concentration.pct > 30}
            class:text-slate-300={portfolioStats.concentration.pct <= 30}
          >
            {t('invest_committee_concentration')}: {portfolioStats.concentration.name}
            {portfolioStats.concentration.pct.toFixed(0)}%
            {#if portfolioStats.concentration.pct > 30}
              ⚠️
            {/if}
          </span>
        {/if}
        {#if portfolioStats.ret !== 0}
          <span
            class:text-green-400={portfolioStats.ret > 0}
            class:text-red-400={portfolioStats.ret < 0}
            class:text-slate-300={portfolioStats.ret === 0}
          >
            {t('invest_total_return')}: {portfolioStats.ret > 0 ? '+' : ''}{portfolioStats.ret.toFixed(1)}%
          </span>
        {/if}
      </div>
    </div>
  {/if}

  <!-- ── Error banner ───────────────────────────────────────────────────── -->
  {#if store.runError}
    <div class="rounded-lg border border-red-800/50 bg-red-900/20 px-3 py-2 text-sm text-red-400">
      {store.runError}
    </div>
  {/if}

  <!-- ── Empty state ────────────────────────────────────────────────────── -->
  {#if allAssets.length === 0 && !store.streaming}
    <div class="rounded-lg border border-[#334155] bg-[#0F172A] px-4 py-8 text-center text-sm text-slate-500">
      {t('invest_committee_no_holdings')}
    </div>
  {/if}

  <!-- ── Per-symbol cards ───────────────────────────────────────────────── -->
  {#each allAssets as asset (asset.symbol)}
    {@const p = store.perSymbolProgress.get(asset.symbol)}
    {@const result = store.results.find((r) => r.symbol === asset.symbol)}
    {@const isExpanded = expandedSymbol === asset.symbol}

    {@const cardBorder = !p ? 'border-[#334155]' : p.error ? 'border-red-700/50' : p.done ? 'border-green-700/50' : 'border-[#334155]'}
    <div class="symbol-card overflow-hidden rounded-lg border bg-[#0F172A] transition-colors {cardBorder}">
      <!-- Card header (clickable) -->
      <button
        class="flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-white/[0.02]"
        onclick={() => toggleExpand(asset.symbol)}
      >
        <!-- Name + badges -->
        <div class="flex flex-1 items-center gap-2 overflow-hidden">
          <span class="truncate text-sm font-medium text-white">
            {asset.name || asset.symbol}
          </span>
          <span class="shrink-0 rounded bg-slate-700 px-1.5 py-0.5 font-mono text-[10px] text-slate-400">
            {asset.symbol}
          </span>
          <span class="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium {asset.kind === 'hold' ? 'bg-green-900/40 text-green-400' : 'bg-amber-900/40 text-amber-400'}">
            {asset.kind === 'hold' ? 'HOLD' : 'WATCH'}
          </span>
        </div>

        <!-- 8-step progress dots -->
        <div class="flex shrink-0 items-center gap-0">
          {#each STEP_DEFS as step, i}
            {@const state = getStepState(p, step.backendIdx)}
            {@const prevDone = i > 0 ? getStepState(p, STEP_DEFS[i - 1].backendIdx) : 'pending'}
            {@const connBg = i > 0 && (state === 'done' || state === 'active')
              ? `linear-gradient(to right, ${STEP_DEFS[i - 1].color}, ${step.color})`
              : state === 'error' && i > 0 ? '#ef4444' : ''}
            {@const dotCls = state === 'done'
              ? `border-[${step.color}] bg-[${step.color}]/15 text-[${step.color}]`
              : state === 'active'
                ? 'border-blue-500 bg-blue-500/15 text-blue-400'
                : state === 'error'
                  ? 'border-red-500 bg-red-500/15 text-red-400'
                  : 'border-slate-600 text-slate-500'}
            <div class="flex items-center">
              {#if i > 0}
                <div
                  class="h-[2px] w-3 transition-colors duration-200 {connBg ? '' : 'bg-slate-600'}"
                  style="background: {connBg}"
                ></div>
              {/if}
              <div
                class="flex h-5 w-5 items-center justify-center rounded-full border text-[9px] font-bold transition-all duration-200 {dotCls} {state === 'active' ? 'animate-pulse' : ''}"
                style={state === 'done' ? `border-color: ${step.color}; color: ${step.color}; background: ${step.color}15;` : ''}
                title={t(step.labelKey)}
              >
                {#if state === 'done'}✓{:else if state === 'active'}◉{:else if state === 'error'}✗{:else}{step.key === 'regime' ? 'R' : step.key.charAt(0).toUpperCase()}{/if}
              </div>
            </div>
          {/each}
        </div>

        <!-- Verdict badge (when done) -->
        {#if result}
          <span
            class="shrink-0 rounded border px-2 py-0.5 text-xs font-bold {getVerdictColorClass(result.finalVerdict)}"
          >
            {result.finalVerdict}
          </span>
        {/if}

        <!-- Expand indicator -->
        <span class="shrink-0 text-xs text-slate-500">{isExpanded ? '▾' : '▸'}</span>
      </button>

      <!-- ── Expanded: step detail cards ────────────────────────────────── -->
      {#if isExpanded}
        <div class="space-y-2 border-t border-[#334155] px-4 py-3">
          {#each STEP_DEFS as step, i}
            {@const state = getStepState(p, step.backendIdx)}
            {@const round = getRoundForStep(p, step.backendIdx)}

            {@const stepCardCls = state === 'done'
              ? 'border-green-700/50 bg-green-900/5'
              : state === 'active'
                ? 'border-blue-600/50 bg-blue-900/5'
                : state === 'error'
                  ? 'border-red-700/50 bg-red-900/5'
                  : 'border-slate-700 bg-slate-900/30'}
            <div class="rounded-md border p-3 transition-colors duration-150 {stepCardCls}">
              <!-- Step header -->
              <div class="mb-1 flex items-center justify-between">
                <div class="flex items-center gap-2">
                  <span
                    class="text-xs font-semibold"
                    style="color: {step.color}"
                  >
                    {t(step.labelKey)}
                  </span>
                  {#if state === 'done'}
                    <span class="text-[10px] text-green-500">✅</span>
                  {:else if state === 'active'}
                    <span class="text-[10px] text-blue-400 animate-pulse">🔵</span>
                  {:else if state === 'error'}
                    <span class="text-[10px] text-red-400">❌</span>
                  {:else}
                    <span class="text-[10px] text-slate-500">⏳</span>
                  {/if}
                </div>
                <span class="text-[10px] text-slate-500">
                  {#if round?.latencyMs && round.latencyMs > 0}
                    {(round.latencyMs / 1000).toFixed(1)}s
                  {/if}
                  {#if round?.tokensUsed && round.tokensUsed > 0}
                    <span class="ml-1">{round.tokensUsed} tok</span>
                  {/if}
                </span>
              </div>

              <!-- Step body -->
              {#if state === 'active'}
                <div class="flex items-center gap-2 text-xs text-slate-400">
                  <span class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-slate-600 border-t-blue-400"></span>
                  {t('invest_debate_waiting_llm')}
                </div>
              {:else if round?.parsed?.rawText}
                <div class="max-h-40 overflow-y-auto whitespace-pre-wrap text-xs leading-relaxed text-slate-300">
                  {round.parsed.rawText}
                </div>
                {#if round.parsed.signal}
                  <div class="mt-1 text-[11px] text-slate-400">
                    {t('invest_macro_signal')}: <span class="font-medium text-slate-300">{round.parsed.signal}</span>
                    {#if round.parsed.strength != null}
                      ({round.parsed.strength.toFixed(1)})
                    {/if}
                  </div>
                {/if}
              {:else if step.key === 'regime' && state === 'done' && p?.regimeData}
                <!-- REGIME step with computed metrics -->
                <div class="space-y-2 text-xs">
                  <div class="flex flex-wrap gap-x-4 gap-y-1">
                    <span class="text-slate-400">
                      {t('invest_regime_label')}: <span class="font-medium text-cyan-400">{p.regimeData.regime}</span>
                    </span>
                    <span class="text-slate-400">
                      {t('invest_regime_reason')}: <span class="text-slate-300">{p.regimeData.reason}</span>
                    </span>
                  </div>
                  <div class="text-slate-400">
                    {t('invest_regime_hint')}: <span class="text-slate-300">{p.regimeData.strategyHint}</span>
                  </div>
                  <div class="flex flex-wrap gap-x-4 gap-y-1 text-[11px] text-slate-500">
                    <span>RSI-14: {p.regimeData.metrics.rsi14.toFixed(1)}</span>
                    <span>MA20: {p.regimeData.metrics.ma20.toFixed(2)}</span>
                    <span>MA60: {p.regimeData.metrics.ma60.toFixed(2)}</span>
                    <span>Vol: {(p.regimeData.metrics.volatilityAnn * 100).toFixed(1)}%</span>
                    <span>{t('invest_regime_inputs')}: {(p.regimeData.metrics.priceQuantile2y * 100).toFixed(0)}%</span>
                  </div>
                </div>
              {:else if step.key === 'regime' && state === 'done'}
                <div class="text-xs text-slate-500">
                  {t('invest_committee_regime_computed')}
                </div>
              {:else if state === 'pending'}
                <div class="text-xs text-slate-600">
                  {t('invest_overview_pending')}
                </div>
              {/if}
            </div>
          {/each}

          <!-- ── CIO final result ───────────────────────────────────────── -->
          {#if result}
            <div class="rounded-md border border-[#334155] bg-[#0F172A] p-3">
              <div class="mb-2 flex items-center gap-3">
                <span
                  class="rounded border px-3 py-1 text-sm font-bold {getVerdictColorClass(result.finalVerdict)}"
                >
                  {result.finalVerdict}
                </span>
                <span class="text-sm text-slate-400">
                  {t('invest_macro_signal')}: {(result.finalConfidence * 100).toFixed(0)}%
                </span>
                {#if result.converged}
                  <span class="rounded bg-green-900/30 px-1.5 py-0.5 text-[10px] font-medium text-green-400">
                    {t('invest_committee_converged')}
                  </span>
                {/if}
                {#if result.sentinelOverride}
                  <span class="rounded bg-red-900/30 px-1.5 py-0.5 text-[10px] font-medium text-red-400">
                    {t('invest_committee_sentinel')}
                  </span>
                {/if}
              </div>

              <!-- Sanity check gates -->
              <div class="mb-2 flex gap-3 text-[11px] text-slate-500">
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
                <span class="ml-auto">
                  {result.totalTokens} tok / {(result.totalLatencyMs / 1000).toFixed(1)}s
                </span>
              </div>

              {#if result.reasoning}
                <div class="max-h-32 overflow-y-auto whitespace-pre-wrap text-xs leading-relaxed text-slate-400">
                  {result.reasoning}
                </div>
              {/if}

              {#if result.sanityCheck.notes.length > 0}
                <div class="mt-2 space-y-0.5 text-[11px] text-slate-500">
                  {#each result.sanityCheck.notes as note}
                    <div>- {note}</div>
                  {/each}
                </div>
              {/if}

              {#if result.sentinelOverride}
                <div class="mt-2 rounded bg-red-900/20 px-2 py-1 text-[11px] text-red-400">
                  {result.sentinelOverride.reason}
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/each}
</div>
