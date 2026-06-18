<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import {
    investCommitteeStore,
    type PortfolioSnapshot,
    type SnapshotHolding,
    type SymbolProgress,
  } from '$lib/stores/invest-committee-store.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { STEP_DEFS, getStepState, getRoundForStep } from './pipeline-config';
  import { getVerdictBadgeStyle, normalizeConfidencePct } from '$lib/utils/invest-verdict';
  import { onMount } from 'svelte';

  const store = investCommitteeStore;
  const invest = investStore;

  let includeWatch = $state(true);
  let expandedSymbols = $state<Set<string>>(new Set());

  const CONCURRENCY_OPTIONS = [1, 2, 3, 5, 8, 10];

  function stepDef(key: string) {
    return STEP_DEFS.find((s) => s.key === key)!;
  }

  function started(p: SymbolProgress | undefined): boolean {
    return !!p && (p.status === 'running' || p.completedSteps > 0 || p.done);
  }

  function segIcon(state: string): string {
    if (state === 'done') return '✓';
    if (state === 'active') return '◉';
    if (state === 'failed') return '⚠';
    if (state === 'error') return '✗';
    if (state === 'aborted') return '⊘';
    return '';
  }

  // Hard fallbacks = truly no content; soft (missing_critical_fields) still has rawText.
  const HARD_FALLBACKS = new Set(['worker_unavailable', 'empty_text', 'cli_executor_none']);
  function isHardFallback(reason: string): boolean {
    return HARD_FALLBACKS.has(reason) || reason.startsWith('cli_error');
  }

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

  const portfolioStats = $derived.by(() => {
    const hv = invest.holdingsMarketValue;
    const cashVal = invest.cash;
    const total = invest.totalAssets;
    const ret = invest.totalReturnPct;

    let maxHolding = { name: '', pct: 0 };
    if (total > 0) {
      for (const h of invest.holdHoldings) {
        const price = invest.priceMap[h.symbol]?.close;
        const val = price && h.shares ? price * h.shares : h.notional || 0;
        const pct = (val / total) * 100;
        if (pct > maxHolding.pct) {
          maxHolding = { name: h.name || h.symbol, pct };
        }
      }
    }

    return { hv, cash: cashVal, total, ret, concentration: maxHolding };
  });

  function formatCash(v: number): string {
    if (v >= 10000) {
      return '¥' + (v / 1000).toFixed(3) + 'K';
    }
    return '¥' + v.toFixed(3);
  }

  // O(1) per-row lookups for the symbol card list (avoids O(assets×n) scans).
  const queueMap = $derived(new Map(store.queue.map((q) => [q.symbol, q])));
  const toolMap = $derived.by(() => {
    const m = new Map<string, typeof store.toolCallHistory>();
    for (const tc of store.toolCallHistory) {
      const bucket = m.get(tc.symbol);
      if (bucket) bucket.push(tc);
      else m.set(tc.symbol, [tc]);
    }
    return m;
  });

  function buildSnapshot(): PortfolioSnapshot {
    const holdings: SnapshotHolding[] = [...invest.holdHoldings, ...invest.watchHoldings].map(
      (h) => ({
        symbol: h.symbol,
        name: h.name,
        shares: h.shares,
        notional: h.notional,
        kind: h.kind,
      }),
    );
    return {
      holdings,
      cash: invest.cash,
      totalNotional: invest.holdingsMarketValue,
      timestamp: new Date().toISOString(),
    };
  }

  function runAll() {
    const syms = allAssets.map((a) => a.symbol);
    if (syms.length === 0) return;
    store.addToQueue(syms, buildSnapshot());
  }

  function runSymbol(sym: string) {
    expandedSymbols.add(sym);
    expandedSymbols = new Set(expandedSymbols);
    store.addToQueue([sym], buildSnapshot());
  }

  function toggleExpand(sym: string) {
    const next = new Set(expandedSymbols);
    if (next.has(sym)) next.delete(sym);
    else next.add(sym);
    expandedSymbols = next;
  }

  function onConcurrencyChange(e: Event) {
    store.setMaxConcurrent(Number((e.target as HTMLSelectElement).value));
  }

  onMount(() => {
    store.loadQueue();
  });
</script>

{#snippet pipelineBar(p: SymbolProgress | undefined)}
  <div class="pipeline-bar">
    {#each STEP_DEFS as step}
      {@const state = getStepState(p, step.backendIdx, started(p))}
      <div class="seg {state}" style="--seg-color:{step.color}" title={t(step.labelKey)}>
        {segIcon(state)}
      </div>
    {/each}
  </div>
{/snippet}

{#snippet stepCard(stepKey: string, p: SymbolProgress | undefined)}
  {@const def = stepDef(stepKey)}
  {@const state = getStepState(p, def.backendIdx, started(p))}
  {@const round = getRoundForStep(p, def.backendIdx)}
  <div class="step-card {state}" style="--sc:{def.color}">
    <div class="step-head">
      <div class="step-dot {state}"></div>
      <span class="step-title">
        {def.icon}
        {t(def.labelKey)}
        {#if def.round}<span class="step-round">{def.round}</span>{/if}
      </span>
      {#if round}
        <div class="step-meta">
          <span>{(round.latencyMs / 1000).toFixed(1)}s</span>
          <span>{round.tokensUsed} tok</span>
        </div>
      {/if}
    </div>
    <div class="step-body">
      {#if state === 'active'}
        <div class="waiting"><div class="spinner"></div><span>{t('invest_committee_analyzing')}</span></div>
      {:else if state === 'aborted'}
        <span class="muted">{t('invest_committee_aborted')}</span>
      {:else if round?.parsed?.fallbackReason && isHardFallback(round.parsed.fallbackReason)}
        <div class="fallback-message">
          <span class="fallback-icon">⚠</span><span>{round.parsed.fallbackReason}</span>
        </div>
      {:else if stepKey === 'regime' && p?.regimeData}
        {@const rd = p.regimeData}
        <div class="regime-box">
          <span class="regime-tag">{rd.regime}</span>
          <div class="regime-metrics">
            <span>RSI-14 {rd.metrics.rsi14.toFixed(1)}</span>
            <span>MA20 {rd.metrics.ma20.toFixed(2)}</span>
            <span>MA60 {rd.metrics.ma60.toFixed(2)}</span>
            <span>Vol {(rd.metrics.volatilityAnn * 100).toFixed(1)}%</span>
            <span>{(rd.metrics.priceQuantile2y * 100).toFixed(0)}%</span>
          </div>
          <div class="regime-hint">{rd.strategyHint}</div>
        </div>
      {:else if round?.parsed?.rawText}
        {@const pf = round.parsed}
        <div class="chip-row">
          {#if pf.signal}<span class="chip sig-{pf.signal.toLowerCase()}">{pf.signal}</span>{/if}
          {#if pf.strength != null}<span class="chip neutral">{t('invest_committee_chip_strength')} {pf.strength}</span>{/if}
          {#if pf.verdict}<span class="chip" style={getVerdictBadgeStyle(pf.verdict)}>{pf.verdict}</span>{/if}
          {#if pf.confidence != null}<span class="chip neutral">{normalizeConfidencePct(pf.confidence).toFixed(0)}%</span>{/if}
          {#if pf.marketPhase}<span class="chip neutral">{pf.marketPhase}</span>{/if}
          {#if pf.emotionTemperature}<span class="chip neutral">{pf.emotionTemperature}</span>{/if}
          {#if pf.buyPointAssessment}<span class="chip neutral">{pf.buyPointAssessment}</span>{/if}
          {#if pf.valuationAssessment}<span class="chip neutral">{pf.valuationAssessment}</span>{/if}
          {#if pf.concentrationPct != null}<span class="chip neutral">{t('invest_committee_chip_concentration')} {pf.concentrationPct}%</span>{/if}
          {#if pf.catalystTier}<span class="chip neutral">{pf.catalystTier}</span>{/if}
          {#if pf.fallbackReason}<span class="chip warn" title={pf.fallbackReason}>⚠ {t('invest_committee_chip_fields_partial')}</span>{/if}
        </div>
        <div class="raw-text">{pf.rawText}</div>
      {:else}
        <span class="muted">{t('invest_committee_waiting')}</span>
      {/if}
    </div>
  </div>
{/snippet}

<div class="space-y-3" data-invest-scope>
  <!-- Action Bar -->
  <div class="action-bar">
    <button class="btn primary" disabled={allAssets.length === 0} onclick={runAll}>
      ⏵ {t('invest_committee_add_all')}
    </button>
    {#if store.runningCount > 0}
      <button class="btn danger" onclick={() => store.abortAll()}>
        ⏹ {t('invest_committee_abort_all')}
      </button>
    {/if}
    <div class="action-sep"></div>
    <label class="checkbox-row">
      <input type="checkbox" bind:checked={includeWatch} />
      {t('invest_committee_include_watch')}
    </label>
    <div class="spacer"></div>
    <label class="conc-row">
      {t('invest_committee_concurrency')}
      <select value={store.maxConcurrent} onchange={onConcurrencyChange}>
        {#each CONCURRENCY_OPTIONS as n}
          <option value={n}>{n}</option>
        {/each}
      </select>
    </label>
    {#if store.runningCount > 0 || store.queuedCount > 0}
      <span class="progress-text">
        <span class="dot"></span>
        {t('invest_committee_in_progress', {
          current: String(store.doneCount),
          total: String(store.queue.length),
          running: String(store.runningCount),
        })}
      </span>
    {/if}
  </div>

  <!-- PRESERVE: portfolio summary -->
  {#if invest.holdCount > 0}
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
      <div class="mb-[var(--space-3)] text-[14px] font-semibold text-[var(--text-primary)]">
        {t('invest_committee_portfolio_summary')}
      </div>
      <div style="display:grid; grid-template-columns:repeat(5,1fr); gap:var(--space-3); text-align:center;">
        <div>
          <div class="text-[11px] text-[var(--text-tertiary)]">{t('invest_position_count')}</div>
          <div class="text-[18px] font-bold font-[var(--font-mono)] text-[var(--text-primary)]">{invest.holdCount}</div>
        </div>
        <div>
          <div class="text-[11px] text-[var(--text-tertiary)]">{t('invest_holdings_value')}</div>
          <div class="text-[18px] font-bold font-[var(--font-mono)] text-[var(--text-primary)]">{formatCash(portfolioStats.hv)}</div>
        </div>
        <div>
          <div class="text-[11px] text-[var(--text-tertiary)]">{t('invest_cash')}</div>
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

  <!-- Symbol cards -->
  {#each allAssets as asset (asset.symbol)}
    {@const p = store.perSymbolProgress.get(asset.symbol)}
    {@const queueItem = queueMap.get(asset.symbol)}
    {@const result = p?.result ?? null}
    {@const isExpanded = expandedSymbols.has(asset.symbol)}
    <div class="symbol-card" class:streaming={queueItem?.status === 'running'}>
      <div class="card-header" onclick={() => toggleExpand(asset.symbol)}>
        <div class="card-id">
          <span class="card-name">{asset.name ?? asset.symbol}</span>
          <span class="card-ticker">{asset.symbol}</span>
        </div>
        <span class="badge {asset.kind}">{asset.kind === 'hold' ? 'HOLD' : 'WATCH'}</span>
        {@render pipelineBar(p)}
        {#if result}
          <span class="verdict-badge-sm" style={getVerdictBadgeStyle(result.finalVerdict)}>
            {result.finalVerdict}
          </span>
        {/if}
        {#if queueItem?.status === 'running'}
          <button
            class="abort-btn"
            onclick={(e) => { e.stopPropagation(); store.abortSymbol(asset.symbol); }}
            title={t('invest_committee_abort')}
          >
            ⏹
          </button>
        {:else}
          <button
            class="run-btn"
            disabled={queueItem?.status === 'queued'}
            onclick={(e) => { e.stopPropagation(); runSymbol(asset.symbol); }}
            title={queueItem?.status === 'queued'
              ? t('invest_committee_queued')
              : queueItem
                ? t('invest_retry')
                : t('invest_committee_run')}
          >
            ▶
          </button>
        {/if}
        <span class="expand-arrow" class:open={isExpanded}>▶</span>
      </div>

      {#if isExpanded}
        {@const tools = toolMap.get(asset.symbol) ?? []}
        <div class="card-body">
          <div class="flow-grid">
            <div class="fw">{@render stepCard('macro', p)}</div>
            <div class="fw">{@render stepCard('regime', p)}</div>

            <div class="connector">
              <svg viewBox="0 0 400 32" preserveAspectRatio="xMidYMid meet">
                <line x1="200" y1="4" x2="130" y2="28" stroke="var(--border)" stroke-width="1.5" stroke-dasharray="4,3" />
                <line x1="200" y1="4" x2="270" y2="28" stroke="var(--border)" stroke-width="1.5" stroke-dasharray="4,3" />
              </svg>
            </div>

            <div>{@render stepCard('quant_r1', p)}</div>
            <div>{@render stepCard('risk_r1', p)}</div>

            <div class="connector">
              <svg viewBox="0 0 400 32" preserveAspectRatio="xMidYMid meet">
                <line x1="130" y1="0" x2="130" y2="28" stroke="var(--border)" stroke-width="1.5" stroke-dasharray="4,3" />
                <line x1="270" y1="0" x2="270" y2="28" stroke="var(--border)" stroke-width="1.5" stroke-dasharray="4,3" />
              </svg>
            </div>

            <div>{@render stepCard('quant_r2', p)}</div>
            <div>{@render stepCard('risk_r2', p)}</div>

            <div class="connector">
              <svg viewBox="0 0 400 32" preserveAspectRatio="xMidYMid meet">
                <line x1="130" y1="0" x2="200" y2="28" stroke="var(--border)" stroke-width="1.5" stroke-dasharray="4,3" />
                <line x1="270" y1="0" x2="200" y2="28" stroke="var(--border)" stroke-width="1.5" stroke-dasharray="4,3" />
              </svg>
            </div>

            <div class="fw">{@render stepCard('cio', p)}</div>

            {#if result}
              <div class="verdict-block">
                <div class="verdict-row">
                  <span class="verdict-action" style={getVerdictBadgeStyle(result.finalVerdict)}>
                    {result.finalVerdict}
                  </span>
                  <span class="verdict-confidence">
                    {t('invest_committee_confidence')}
                    {result.finalConfidence}%
                  </span>
                  <span class="gate-badge {result.sanityCheck.gate1Pass ? 'pass' : 'fail'}">
                    {result.sanityCheck.gate1Pass ? '✓' : '✗'} Gate 1
                  </span>
                  <span class="gate-badge {result.sanityCheck.gate2Pass ? 'pass' : 'fail'}">
                    {result.sanityCheck.gate2Pass ? '✓' : '✗'} Gate 2
                  </span>
                </div>
                <div class="verdict-reasoning">{result.reasoning}</div>
                <div class="verdict-meta">
                  <span class="meta-item">⏱ {(result.totalLatencyMs / 1000).toFixed(1)}s</span>
                  <span class="meta-item">🔤 {result.totalTokens} tok</span>
                  {#if result.converged}
                    <span class="meta-item">✅ {t('invest_committee_converged')}</span>
                  {/if}
                </div>
                {#if result.sanityCheck.notes.length > 0}
                  <ul class="verdict-notes">
                    {#each result.sanityCheck.notes as note}
                      <li>{note}</li>
                    {/each}
                  </ul>
                {/if}
                {#if result.sentinelOverride}
                  <div class="sentinel-override">
                    ⚠ {result.sentinelOverride.reason} → {result.sentinelOverride.forcedVerdict}
                  </div>
                {/if}
              </div>
            {/if}

            {#if tools.length > 0}
              <div class="tool-strip">
                🔧 {t('invest_committee_tools')}：
                {#each tools as tc}
                  <span class="tool-chip">{tc.toolName} <span class="tool-ms">{tc.latencyMs}ms</span></span>
                {/each}
              </div>
            {/if}
          </div>
        </div>
      {/if}
    </div>
  {/each}

  {#if allAssets.length === 0}
    <div class="empty-hint">{t('invest_committee_queue_empty')}</div>
  {/if}
</div>

<style>
  .action-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    flex-wrap: wrap;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-input);
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
  }
  .btn:hover:not(:disabled) { background: var(--bg-hover); border-color: var(--accent-muted); }
  .btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .btn.primary { background: var(--accent); color: #111; border-color: var(--accent); }
  .btn.danger { color: var(--color-error); border-color: var(--color-error); }
  .btn.danger:hover:not(:disabled) { background: rgba(168, 122, 122, 0.12); }
  .action-sep { width: 1px; height: 24px; background: var(--border); }
  .spacer { flex: 1; }
  .checkbox-row,
  .conc-row {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-secondary);
    cursor: pointer;
  }
  .checkbox-row input { accent-color: var(--accent); cursor: pointer; }
  .conc-row select {
    border: 1px solid var(--border);
    background: var(--bg-input);
    color: var(--text-primary);
    border-radius: var(--radius-sm);
    padding: 3px 6px;
    font-size: 12px;
  }
  .progress-text { font-size: 12px; color: var(--text-secondary); display: flex; align-items: center; gap: 6px; }
  .progress-text .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); animation: pulse 1.5s ease-in-out infinite; }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.3; } }

  .symbol-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
    transition: border-color 0.2s;
  }
  .symbol-card:hover { border-color: var(--accent-muted); }
  .symbol-card.streaming { border-color: var(--accent); }
  .card-header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    cursor: pointer;
    user-select: none;
    transition: background 0.15s;
  }
  .card-header:hover { background: var(--bg-hover); }
  .card-id { display: flex; flex-direction: column; min-width: 84px; }
  .card-name { font-size: 14px; font-weight: 600; }
  .card-ticker { font-size: 11px; color: var(--text-tertiary); font-family: var(--font-mono); }
  .badge {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.3px;
    flex-shrink: 0;
  }
  .badge.hold { background: rgba(138, 154, 118, 0.15); color: var(--color-success); }
  .badge.watch { background: rgba(196, 169, 110, 0.12); color: var(--accent-muted); }
  .verdict-badge-sm {
    padding: 3px 10px;
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    flex-shrink: 0;
  }
  .abort-btn {
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--bg-input);
    font-size: 11px;
    cursor: pointer;
    flex-shrink: 0;
    color: var(--color-error);
    border-color: var(--color-error);
  }
  .abort-btn:hover { background: rgba(168, 122, 122, 0.12); }
  .run-btn {
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--accent-muted);
    background: var(--bg-input);
    color: var(--accent);
    font-size: 11px;
    cursor: pointer;
    flex-shrink: 0;
  }
  .run-btn:hover:not(:disabled) { background: var(--accent-muted); }
  .run-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .raw-text { white-space: pre-wrap; word-break: break-word; }
  .expand-arrow {
    width: 20px; height: 20px;
    display: flex; align-items: center; justify-content: center;
    color: var(--text-tertiary);
    transition: transform 0.2s;
    flex-shrink: 0; font-size: 12px;
  }
  .expand-arrow.open { transform: rotate(90deg); }
  .card-body { border-top: 1px solid var(--border); padding: 20px; }

  /* Pipeline bar */
  .pipeline-bar {
    display: flex; flex: 1; height: 22px;
    border-radius: var(--radius-sm); overflow: hidden;
    background: var(--bg-input); gap: 2px; margin: 0 4px; min-width: 200px;
  }
  .seg {
    flex: 1; display: flex; align-items: center; justify-content: center;
    font-size: 9px; font-weight: 700; border-radius: 3px; transition: all 0.4s; color: transparent;
  }
  .seg.pending { background: var(--bg-input); }
  .seg.active { background: color-mix(in srgb, var(--seg-color) 25%, transparent); color: var(--seg-color); animation: seg-pulse 1.5s ease-in-out infinite; }
  .seg.done { background: color-mix(in srgb, var(--seg-color) 35%, transparent); color: var(--seg-color); }
  .seg.failed { background: color-mix(in srgb, var(--color-warning) 25%, transparent); color: var(--color-warning); }
  .seg.error { background: color-mix(in srgb, var(--color-error) 25%, transparent); color: var(--color-error); }
  .seg.aborted { background: color-mix(in srgb, var(--text-tertiary) 25%, transparent); color: var(--text-tertiary); }
  @keyframes seg-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }

  /* Debate flow grid */
  .flow-grid {
    display: grid; grid-template-columns: 1fr 1fr; gap: 14px;
    max-width: 1000px; margin: 0 auto; align-items: start;
  }
  .flow-grid .fw { grid-column: 1 / -1; justify-self: center; width: 65%; max-width: 560px; min-width: 320px; }
  .connector { grid-column: 1 / -1; display: flex; justify-content: center; height: 28px; }
  .connector svg { width: 100%; height: 100%; }
  @media (max-width: 700px) {
    .flow-grid { grid-template-columns: 1fr; }
    .flow-grid .fw { width: 100%; max-width: none; }
  }

  /* Step card */
  .step-card {
    width: 100%; background: var(--bg-base); border: 1px solid var(--border);
    border-radius: var(--radius-md); overflow: hidden; transition: border-color 0.3s, box-shadow 0.3s, opacity 0.4s;
  }
  .step-card.pending { opacity: 0.4; }
  .step-card.active { border-color: var(--sc); box-shadow: 0 0 20px color-mix(in srgb, var(--sc) 12%, transparent); }
  .step-card.done { border-color: color-mix(in srgb, var(--sc) 40%, transparent); }
  .step-card.failed { border-color: var(--color-warning); }
  .step-card.error { border-color: var(--color-error); }
  .step-card.aborted { border-color: var(--text-tertiary); opacity: 0.6; }
  .step-head {
    display: flex; align-items: center; gap: 8px; padding: 10px 14px;
    background: color-mix(in srgb, var(--sc) 6%, var(--bg-card));
    border-bottom: 1px solid var(--border);
  }
  .step-dot { width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; transition: all 0.3s; }
  .step-dot.pending { background: var(--bg-input); border: 1.5px solid var(--border); }
  .step-dot.active { background: var(--sc); animation: dot-pulse 1.2s ease-in-out infinite; }
  .step-dot.done { background: var(--sc); }
  .step-dot.failed { background: var(--color-warning); }
  .step-dot.error { background: var(--color-error); }
  .step-dot.aborted { background: var(--text-tertiary); }
  @keyframes dot-pulse { 0%, 100% { opacity: 1; transform: scale(1); } 50% { opacity: 0.4; transform: scale(0.7); } }
  .step-title { font-size: 13px; font-weight: 600; }
  .step-round { font-size: 10px; color: var(--text-tertiary); padding: 1px 5px; border-radius: 3px; background: var(--bg-input); }
  .step-meta { margin-left: auto; display: flex; gap: 10px; font-size: 10px; color: var(--text-tertiary); font-family: var(--font-mono); }
  .step-body {
    padding: 14px; font-size: 12.5px; color: var(--text-secondary); line-height: 1.85;
    max-height: 320px; overflow-y: auto; word-break: break-word;
    white-space: pre-wrap;
  }
  .muted { color: var(--text-tertiary); }
  .waiting { display: flex; align-items: center; gap: 8px; color: var(--text-tertiary); }
  .spinner {
    width: 14px; height: 14px; border: 2px solid var(--border); border-top-color: var(--sc);
    border-radius: 50%; animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .fallback-message {
    display: flex; align-items: center; gap: 0.5rem;
    padding: 0.75rem 1rem; background: rgba(255, 193, 7, 0.1);
    border: 1px solid rgba(255, 193, 7, 0.3); border-radius: 6px; color: var(--color-warning);
  }
  .fallback-icon { font-size: 1.1rem; flex-shrink: 0; }
  .regime-box { display: flex; flex-direction: column; gap: 6px; }
  .regime-tag { align-self: flex-start; padding: 2px 10px; border-radius: var(--radius-sm); background: color-mix(in srgb, var(--sc) 18%, transparent); color: var(--sc); font-weight: 600; font-size: 12px; }
  .regime-metrics { display: flex; flex-wrap: wrap; gap: 10px; font-family: var(--font-mono); font-size: 11px; color: var(--text-tertiary); }
  .regime-hint { font-size: 12px; }

  /* Key-field chips */
  .chip-row { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 10px; padding-bottom: 8px; border-bottom: 1px dashed var(--border); }
  .chip { font-size: 11px; font-weight: 700; padding: 2px 10px; border-radius: var(--radius-sm); text-transform: uppercase; }
  .chip.neutral { background: var(--bg-input); color: var(--text-secondary); font-weight: 600; text-transform: none; }
  .chip.warn { background: rgba(255,193,7,0.12); color: var(--color-warning); font-weight: 600; text-transform: none; }
  .chip.sig-risk_on, .chip.sig-buy, .chip.sig-bullish { background: rgba(138,154,118,0.15); color: var(--color-success); }
  .chip.sig-accumulate { background: rgba(59,130,246,0.15); color: var(--color-quant, #3b82f6); }
  .chip.sig-hold, .chip.sig-neutral { background: rgba(196,169,110,0.15); color: var(--accent); }
  .chip.sig-trim { background: rgba(255,193,7,0.15); color: var(--color-warning); }
  .chip.sig-risk_off, .chip.sig-sell, .chip.sig-bearish, .chip.sig-high_risk { background: rgba(168,122,122,0.2); color: var(--color-error); }

  /* Verdict block */
  .verdict-block {
    grid-column: 1 / -1; justify-self: center; width: 65%; max-width: 560px; min-width: 320px;
    background: var(--bg-base); border: 1px solid var(--accent-muted); border-radius: var(--radius-md);
    padding: 14px 16px; display: flex; flex-direction: column; gap: 10px;
  }
  .verdict-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .verdict-action { padding: 3px 12px; border-radius: var(--radius-sm); font-size: 12px; font-weight: 700; text-transform: uppercase; }
  .verdict-confidence { font-size: 12px; color: var(--text-secondary); }
  .gate-badge { font-size: 10px; padding: 2px 7px; border-radius: 3px; }
  .gate-badge.pass { background: rgba(138, 154, 118, 0.18); color: var(--color-success); }
  .gate-badge.fail { background: rgba(168, 122, 122, 0.18); color: var(--color-error); }
  .verdict-reasoning { font-size: 12.5px; color: var(--text-secondary); line-height: 1.8; }
  .verdict-meta { display: flex; gap: 14px; font-size: 11px; color: var(--text-tertiary); font-family: var(--font-mono); }
  .verdict-notes { margin: 0; padding-left: 18px; font-size: 11.5px; color: var(--text-tertiary); }
  .sentinel-override { font-size: 12px; color: var(--color-warning); }

  /* Tool strip */
  .tool-strip {
    grid-column: 1 / -1; display: flex; flex-wrap: wrap; align-items: center; gap: 8px;
    font-size: 11px; color: var(--text-tertiary);
  }
  .tool-chip { padding: 2px 8px; border-radius: var(--radius-sm); background: var(--bg-input); font-family: var(--font-mono); }
  .tool-ms { opacity: 0.5; }
  .empty-hint { padding: 32px; text-align: center; color: var(--text-tertiary); font-size: 13px; }
</style>