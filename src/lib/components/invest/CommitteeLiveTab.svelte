<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { STEP_DEFS, getStepState, getRoundForStep } from './pipeline-config';
  import { getVerdictBadgeStyle } from '$lib/utils/invest-verdict';

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
  // getStepState / getRoundForStep / getVerdictBadgeStyle 已抽到共享模块

  async function runAll() {
    const syms = allAssets.map((a) => a.symbol);
    if (syms.length === 0) return;
    expandedSymbol = syms[0];
    await store.runCommittee(syms);
  }

  function toggleExpand(symbol: string) {
    expandedSymbol = expandedSymbol === symbol ? null : symbol;
  }

  function formatCash(v: number): string {
    if (v >= 10000) return '¥' + (v / 10000).toFixed(0) + 'K';
    return '¥' + v.toLocaleString();
  }
</script>

<div class="space-y-[var(--space-3)]">
  <!-- ── Top action bar ─────────────────────────────────────────────────── -->
  <div class="flex items-center justify-between rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-4)] py-[var(--space-3)]">
    <div class="flex items-center gap-[var(--space-3)]">
      <button
        class="flex items-center gap-[var(--space-2)] rounded-[var(--radius-md)] bg-[rgba(138,154,118,0.15)] px-[var(--space-4)] py-[var(--space-2)] text-[12px] font-medium text-[#8a9a76] transition-colors hover:bg-[rgba(138,154,118,0.25)] disabled:cursor-not-allowed disabled:opacity-40"
        disabled={store.running || allAssets.length === 0}
        onclick={runAll}
      >
        {#if store.running}
          <span class="inline-block h-2 w-2 animate-pulse rounded-full bg-[#8a9a76]"></span>
          {t('invest_committee_running_progress', {
            current: String(completedCount),
            total: String(allAssets.length),
          })}
        {:else}
          ▶ {t('invest_run_all_holdings')}
        {/if}
      </button>
      <label class="flex items-center gap-[6px] text-[12px] text-[var(--text-tertiary)]">
        <input type="checkbox" bind:checked={includeWatch} disabled={store.running} style="accent-color: var(--accent);" />
        {t('invest_include_watch')}
      </label>
    </div>
    <span class="text-[12px] text-[var(--text-tertiary)]">
      {t('invest_hold')} {holdCount}{watchCount > 0 ? ` + ${watchCount} ${t('invest_watch')}` : ''}
    </span>
  </div>

  <!-- ── Portfolio summary card ─────────────────────────────────────────── -->
  {#if holdCount > 0}
    <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] p-[var(--space-4)]">
      <div class="mb-[var(--space-3)] text-[14px] font-semibold text-[var(--text-primary)]">
        {t('invest_committee_portfolio_summary')}
      </div>
      <div style="display:grid; grid-template-columns:repeat(5,1fr); gap:var(--space-3); text-align:center;">
        <div>
          <div class="text-[11px] text-[var(--text-tertiary)]">{t('invest_position_count')}</div>
          <div class="text-[18px] font-bold font-[var(--font-mono)] text-[var(--text-primary)]">{holdCount}</div>
        </div>
        <div>
          <div class="text-[11px] text-[var(--text-tertiary)]">{t('invest_holdings_value')}</div>
          <div class="text-[18px] font-bold font-[var(--font-mono)] text-[var(--text-primary)]">{formatCash(portfolioStats.hv)}</div>
        </div>
        <div>
          <div class="text-[11px] text-[var(--text-tertiary)]">{t('invest_committee_emergency')}</div>
          <div class="text-[18px] font-bold font-[var(--font-mono)] text-[#8a9a76]">{formatCash(portfolioStats.cash)}</div>
        </div>
        <div>
          <div class="text-[11px] text-[var(--text-tertiary)]">{t('invest_committee_concentration')}</div>
          <div class="text-[18px] font-bold font-[var(--font-mono)] {portfolioStats.concentration.pct > 30 ? 'text-[#b89a6a]' : 'text-[var(--text-primary)]'}">
            {portfolioStats.concentration.pct > 0 ? `${portfolioStats.concentration.name} ${portfolioStats.concentration.pct.toFixed(0)}%` : '—'}
          </div>
        </div>
        <div>
          <div class="text-[11px] text-[var(--text-tertiary)]">{t('invest_total_return')}</div>
          <div class="text-[18px] font-bold font-[var(--font-mono)] {portfolioStats.ret >= 0 ? 'text-[#8a9a76]' : 'text-[#a87a7a]'}">
            {portfolioStats.ret >= 0 ? '+' : ''}{portfolioStats.ret.toFixed(1)}%
          </div>
        </div>
      </div>
    </div>
  {/if}

  <!-- ── Error banner ───────────────────────────────────────────────────── -->
  {#if store.runError}
    <div class="rounded-[var(--radius-md)] border border-[rgba(168,122,122,0.3)] bg-[rgba(168,122,122,0.1)] px-[var(--space-3)] py-[var(--space-2)] text-[12px] text-[#a87a7a]">
      {store.runError}
    </div>
  {/if}

  <!-- ── Empty state ────────────────────────────────────────────────────── -->
  {#if allAssets.length === 0 && !store.streaming}
    <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-4)] py-[var(--space-8)] text-center text-[12px] text-[var(--text-tertiary)]">
      {t('invest_committee_no_holdings')}
    </div>
  {/if}

  <!-- ── Per-symbol cards ───────────────────────────────────────────────── -->
  {#each allAssets as asset (asset.symbol)}
    {@const p = store.perSymbolProgress.get(asset.symbol)}
    {@const result = store.results.find((r) => r.symbol === asset.symbol)}
    {@const isExpanded = expandedSymbol === asset.symbol}

    <div class="overflow-hidden rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] transition-colors">
      <!-- Card header (clickable) -->
      <button
        class="flex w-full items-center gap-[var(--space-3)] px-[var(--space-4)] py-[var(--space-3)] text-left transition-colors hover:bg-[var(--bg-hover)]"
        onclick={() => toggleExpand(asset.symbol)}
      >
        <!-- Name + ticker -->
        <span class="min-w-[80px] text-[14px] font-semibold text-[var(--text-primary)]">{asset.name || asset.symbol}</span>
        <span class="shrink-0 text-[12px] font-[var(--font-mono)] text-[var(--text-tertiary)]">{asset.symbol}</span>
        <span class="shrink-0 rounded-[var(--radius-sm)] px-2 py-0.5 text-[10px] font-semibold {asset.kind === 'hold' ? 'bg-[rgba(138,154,118,0.15)] text-[#8a9a76]' : 'bg-[var(--accent-muted)] text-[var(--accent)]'}">
          {asset.kind === 'hold' ? 'HOLD' : 'WATCH'}
        </span>

        <!-- 8-step progress dots -->
        <div class="ml-auto mr-[var(--space-3)] flex shrink-0 items-center gap-[6px]">
          {#each STEP_DEFS as step}
            {@const state = getStepState(p, step.backendIdx, pipelineStarted)}
            <div
              class="flex h-[22px] w-[22px] items-center justify-center rounded-full text-[9px] font-bold transition-all {state === 'done' ? '' : state === 'active' ? 'animate-pulse' : ''}"
              style={state === 'done'
                ? `background:${step.color}25; color:${step.color};`
                : state === 'active'
                  ? `background:rgba(59,130,246,0.2); color:#3b82f6;`
                  : state === 'error'
                    ? 'background:rgba(168,122,122,0.2); color:#a87a7a;'
                    : 'background:var(--bg-input); color:var(--text-tertiary);'}
              title={t(step.labelKey)}
            >
              {#if state === 'done'}✓{:else if state === 'active'}◉{:else if state === 'error'}✗{:else}{step.key === 'regime' ? 'R' : step.key.charAt(0).toUpperCase()}{/if}
            </div>
          {/each}
        </div>

        <!-- Verdict badge -->
        {#if result}
          <span
            class="shrink-0 rounded-[var(--radius-full)] px-3 py-1 text-[11px] font-bold"
            style={getVerdictBadgeStyle(result.finalVerdict)}
          >
            {result.finalVerdict}
          </span>
        {/if}

        <!-- Expand arrow -->
        <span class="shrink-0 text-[12px] text-[var(--text-tertiary)] transition-transform {isExpanded ? 'rotate-90' : ''}">▶</span>
      </button>

      <!-- ── Expanded body ──────────────────────────────────────────────── -->
      {#if isExpanded}
        <div class="space-y-[var(--space-2)] border-t border-[var(--border)] px-[var(--space-4)] pb-[var(--space-4)] pt-[var(--space-3)]">
          {#each STEP_DEFS as step}
            {@const state = getStepState(p, step.backendIdx, pipelineStarted)}
            {@const round = getRoundForStep(p, step.backendIdx)}

            <div class="rounded-[var(--radius-md)] bg-[var(--bg-input)] p-[var(--space-3)]">
              <!-- Step header -->
              <div class="mb-[var(--space-2)] flex items-center justify-between">
                <span class="text-[12px] font-semibold" style="color: {step.color}">{t(step.labelKey)}</span>
                <span class="text-[10px] font-[var(--font-mono)] text-[var(--text-tertiary)]">
                  {#if round?.latencyMs && round.latencyMs > 0}
                    {(round.latencyMs / 1000).toFixed(1)}s
                  {/if}
                  {#if round?.tokensUsed && round.tokensUsed > 0}
                    <span class="ml-[var(--space-2)]">{round.tokensUsed} tok</span>
                  {/if}
                </span>
              </div>

              <!-- Step body -->
              {#if state === 'active'}
                <div class="flex items-center gap-[var(--space-2)] text-[12px] text-[var(--text-secondary)]">
                  <span class="inline-block h-3 w-3 animate-spin rounded-full border-2 border-[var(--border)] border-t-[#3b82f6]"></span>
                  {t('invest_debate_waiting_llm')}
                </div>
              {:else if round?.parsed?.rawText}
                <div class="max-h-[200px] overflow-y-auto whitespace-pre-wrap font-[var(--font-mono)] text-[12px] leading-[1.6] text-[var(--text-secondary)]">
                  {round.parsed.rawText}
                </div>
              {:else if step.key === 'regime' && state === 'done' && p?.regimeData}
                <div class="space-y-[var(--space-2)] text-[12px]">
                  <div class="flex flex-wrap gap-x-[var(--space-4)] gap-y-[var(--space-1)]">
                    <span class="text-[var(--text-tertiary)]">
                      {t('invest_regime_label')}: <span class="font-medium text-[#3b82f6]">{p.regimeData.regime}</span>
                    </span>
                    <span class="text-[var(--text-tertiary)]">
                      {t('invest_regime_reason')}: <span class="text-[var(--text-secondary)]">{p.regimeData.reason}</span>
                    </span>
                  </div>
                  <div class="text-[var(--text-tertiary)]">
                    {t('invest_regime_hint')}: <span class="text-[var(--text-secondary)]">{p.regimeData.strategyHint}</span>
                  </div>
                  <div class="flex flex-wrap gap-x-[var(--space-4)] gap-y-[var(--space-1)] text-[11px] text-[var(--text-tertiary)]">
                    <span>RSI-14: {p.regimeData.metrics.rsi14.toFixed(1)}</span>
                    <span>MA20: {p.regimeData.metrics.ma20.toFixed(2)}</span>
                    <span>MA60: {p.regimeData.metrics.ma60.toFixed(2)}</span>
                    <span>Vol: {(p.regimeData.metrics.volatilityAnn * 100).toFixed(1)}%</span>
                    <span>{t('invest_regime_inputs')}: {(p.regimeData.metrics.priceQuantile2y * 100).toFixed(0)}%</span>
                  </div>
                </div>
              {:else if step.key === 'regime' && state === 'done'}
                <div class="text-[12px] text-[var(--text-tertiary)]">{t('invest_committee_regime_computed')}</div>
              {:else if state === 'pending'}
                <div class="text-[12px] text-[var(--text-tertiary)]">{t('invest_overview_pending')}</div>
              {/if}
            </div>
          {/each}

          <!-- ── CIO Verdict Card ────────────────────────────────────────── -->
          {#if result}
            <div class="mt-[var(--space-3)] rounded-[var(--radius-lg)] border-2 border-[var(--border)] bg-[var(--bg-card)] p-[var(--space-4)]">
              <div class="mb-[var(--space-3)] flex items-center gap-[var(--space-3)]">
                <span class="text-[14px] font-bold text-[var(--text-primary)]">👔 {t('invest_replay_cio_verdict')}</span>
                <span
                  class="rounded-[var(--radius-full)] px-[14px] py-1 text-[14px] font-bold"
                  style={getVerdictBadgeStyle(result.finalVerdict)}
                >{result.finalVerdict}</span>
                <span class="text-[13px] font-[var(--font-mono)] text-[var(--text-secondary)]">
                  {t('invest_macro_signal')}: {(result.finalConfidence * 100).toFixed(0)}%
                </span>
              </div>

              {#if result.reasoning}
                <div class="mb-[var(--space-2)] text-[12px] leading-[1.7] text-[var(--text-secondary)]">{result.reasoning}</div>
              {/if}

              <!-- Gate badges -->
              <div class="mb-[var(--space-2)] flex gap-[var(--space-2)]">
                <span class="rounded-[var(--radius-sm)] px-2 py-0.5 text-[10px] font-semibold {result.sanityCheck.gate1Pass ? 'bg-[rgba(138,154,118,0.15)] text-[#8a9a76]' : 'bg-[rgba(168,122,122,0.15)] text-[#a87a7a]'}">
                  {result.sanityCheck.gate1Pass ? '✓' : '✗'} {t('invest_gate1_label')}
                </span>
                <span class="rounded-[var(--radius-sm)] px-2 py-0.5 text-[10px] font-semibold {result.sanityCheck.gate2Pass ? 'bg-[rgba(138,154,118,0.15)] text-[#8a9a76]' : 'bg-[rgba(168,122,122,0.15)] text-[#a87a7a]'}">
                  {result.sanityCheck.gate2Pass ? '✓' : '✗'} {t('invest_gate2_label')}
                </span>
                <span class="rounded-[var(--radius-sm)] px-2 py-0.5 text-[10px] font-semibold {result.sanityCheck.gate3Pass ? 'bg-[rgba(138,154,118,0.15)] text-[#8a9a76]' : 'bg-[rgba(168,122,122,0.15)] text-[#a87a7a]'}">
                  {result.sanityCheck.gate3Pass ? '✓' : '✗'} {t('invest_gate3_label')}
                </span>
                <span class="rounded-[var(--radius-sm)] px-2 py-0.5 text-[10px] font-semibold {result.sanityCheck.gate4Pass ? 'bg-[rgba(138,154,118,0.15)] text-[#8a9a76]' : 'bg-[rgba(168,122,122,0.15)] text-[#a87a7a]'}">
                  {result.sanityCheck.gate4Pass ? '✓' : '✗'} {t('invest_gate4_label')}
                </span>
              </div>

              <!-- Meta row -->
              <div class="flex gap-[var(--space-3)] text-[11px] text-[var(--text-tertiary)]">
                <span>{t('invest_committee_total_value')}: {(result.totalLatencyMs / 1000).toFixed(1)}s</span>
                <span>{result.totalTokens} tok</span>
                <span>{t('invest_committee_converged')}: {result.converged ? '✓' : '✗'}</span>
                {#if result.sentinelOverride}
                  <span class="text-[#a87a7a]">⚠ {t('invest_committee_sentinel')}</span>
                {/if}
              </div>

              {#if result.sanityCheck.notes.length > 0}
                <div class="mt-[var(--space-2)] space-y-0.5 text-[11px] text-[var(--text-tertiary)]">
                  {#each result.sanityCheck.notes as note}
                    <div>- {note}</div>
                  {/each}
                </div>
              {/if}

              {#if result.sentinelOverride}
                <div class="mt-[var(--space-2)] rounded-[var(--radius-md)] bg-[rgba(168,122,122,0.1)] px-[var(--space-2)] py-[var(--space-1)] text-[11px] text-[#a87a7a]">
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
