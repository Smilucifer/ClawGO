<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import PipelineFlow from '$lib/components/invest/PipelineFlow.svelte';
  import DebateBlock from '$lib/components/invest/DebateBlock.svelte';
  import ProviderConfigPanel from '$lib/components/invest/ProviderConfigPanel.svelte';

  // Must match PipelineFlow.NODES length (macro, quant_r1, risk_r1, quant_r2, risk_r2, cio)
  const PIPELINE_STEPS = 6;

  let symbolsInput = $state('');
  let selectedSymbol = $state<string | null>(null);
  let includeWatch = $state(false);

  const store = investCommitteeStore;

  // Auto-select first symbol when streaming starts
  $effect(() => {
    if (store.activeSymbols.length > 0 && !selectedSymbol) {
      selectedSymbol = store.activeSymbols[0];
    }
    if (store.activeSymbols.length === 0) {
      selectedSymbol = null;
    }
  });

  const currentProgress = $derived(
    selectedSymbol ? store.perSymbolProgress.get(selectedSymbol) : undefined
  );

  async function run() {
    const syms = symbolsInput
      .split(/[\s,]+/)
      .map((s) => s.trim())
      .filter(Boolean);
    if (syms.length === 0) return;
    selectedSymbol = syms[0];
    await store.runCommittee(syms);
  }

  async function runAllHoldings() {
    const syms = investStore.holdHoldings.map((h) => h.symbol);
    if (includeWatch) {
      syms.push(...investStore.watchHoldings.map((h) => h.symbol));
    }
    if (syms.length === 0) return;
    selectedSymbol = syms[0];
    await store.runCommittee(syms);
  }
</script>

<div class="space-y-4">
  <!-- Config panel (collapsed by default) -->
  <details class="rounded-lg border">
    <summary class="cursor-pointer px-3 py-2 text-sm text-muted-foreground">
      {t('invest_committee_config')}
    </summary>
    <div class="px-3 pb-3">
      <ProviderConfigPanel />
    </div>
  </details>

  <!-- Input row -->
  <div class="flex items-end gap-2">
    <div class="flex-1">
      <label class="mb-1 block text-sm text-muted-foreground">
        {t('invest_committee_symbols')}
      </label>
      <input
        type="text"
        class="w-full rounded border border-border bg-background px-3 py-1.5 text-sm"
        placeholder="600519.SH, 000001.SZ"
        bind:value={symbolsInput}
        disabled={store.running}
      />
    </div>
    <button
      class="rounded bg-primary px-4 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
      disabled={store.running || !symbolsInput.trim()}
      onclick={run}
    >
      {store.running ? t('invest_committee_running') : t('invest_committee_run')}
    </button>
  </div>

  <!-- Run all holdings -->
  <div class="flex items-center gap-3">
    <button
      class="rounded bg-muted px-3 py-1.5 text-sm disabled:opacity-50"
      disabled={store.running || (investStore.holdHoldings.length === 0 && (!includeWatch || investStore.watchHoldings.length === 0))}
      onclick={runAllHoldings}
    >
      {t('invest_run_all_holdings')}
    </button>
    <label class="flex items-center gap-1.5 text-xs text-muted-foreground">
      <input type="checkbox" bind:checked={includeWatch} disabled={store.running} />
      {t('invest_include_watch')}
    </label>
  </div>

  <!-- Multi-symbol overview table -->
  {#if store.activeSymbols.length > 1}
    <div class="rounded-lg border border-border overflow-hidden">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b bg-muted/30 text-xs text-muted-foreground">
            <th class="px-3 py-2 text-left">{t('invest_overview_symbol')}</th>
            <th class="px-3 py-2 text-left">{t('invest_overview_status')}</th>
            <th class="px-3 py-2 text-left">{t('invest_overview_progress')}</th>
            <th class="px-3 py-2 text-left">{t('invest_overview_verdict')}</th>
          </tr>
        </thead>
        <tbody>
          {#each store.activeSymbols as sym}
            {@const p = store.perSymbolProgress.get(sym)}
            <tr
              class="border-b border-border/50 cursor-pointer hover:bg-muted/20 {selectedSymbol === sym ? 'bg-muted/10' : ''}"
              onclick={() => (selectedSymbol = sym)}
            >
              <td class="px-3 py-2 font-mono text-xs">{sym}</td>
              <td class="px-3 py-2">
                {#if p?.done && !p.error}
                  <span class="text-green-500 text-xs">{t('invest_overview_done')}</span>
                {:else if p?.error}
                  <span class="text-red-500 text-xs">{t('invest_overview_error')}</span>
                {:else if p && p.activeStep >= 0}
                  <span class="text-yellow-500 text-xs">{t('invest_overview_running')}</span>
                {:else}
                  <span class="text-muted-foreground text-xs">{t('invest_overview_pending')}</span>
                {/if}
              </td>
              <td class="px-3 py-2 text-xs text-muted-foreground">
                {p?.completedSteps ?? 0} / {PIPELINE_STEPS}
              </td>
              <td class="px-3 py-2">
                {#if p?.result}
                  <span class="font-bold text-xs">{p.result.finalVerdict}</span>
                  <span class="ml-1 text-xs text-muted-foreground">
                    {(p.result.finalConfidence * 100).toFixed(0)}%
                  </span>
                {:else}
                  <span class="text-xs text-muted-foreground">-</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  <!-- Multi-symbol tab selector -->
  {#if store.activeSymbols.length > 1}
    <div class="flex gap-1 border-b border-border">
      {#each store.activeSymbols as sym}
        {@const p = store.perSymbolProgress.get(sym)}
        <button
          class="flex items-center gap-1.5 rounded-t-md px-3 py-1.5 text-sm transition-colors"
          class:bg-primary={selectedSymbol === sym}
          class:text-primary-foreground={selectedSymbol === sym}
          class:text-muted-foreground={selectedSymbol !== sym}
          onclick={() => (selectedSymbol = sym)}
        >
          <span>{sym}</span>
          {#if p?.done && !p.error}
            <span class="inline-block h-2 w-2 rounded-full bg-green-500"></span>
          {:else if p?.error}
            <span class="inline-block h-2 w-2 rounded-full bg-red-500"></span>
          {:else if p && p.activeStep >= 0}
            <span class="inline-block h-2 w-2 animate-pulse rounded-full bg-yellow-500"></span>
          {/if}
        </button>
      {/each}
    </div>
  {/if}

  <!-- Error banner -->
  {#if store.runError}
    <div class="rounded border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
      {store.runError}
    </div>
  {/if}

  <!-- PipelineFlow -->
  {#if store.streaming || store.results.length > 0}
    <PipelineFlow progress={currentProgress} />
  {/if}

  <!-- Streaming role output cards -->
  {#if currentProgress && currentProgress.completedRounds.length > 0}
    <div class="space-y-2">
      {#each currentProgress.completedRounds as round, i}
        <DebateBlock
          {round}
          blockState={i === currentProgress.completedRounds.length - 1 && currentProgress.activeStep >= 0 ? 'active' : 'done'}
        />
      {/each}
    </div>
  {/if}

  <!-- Final results (once streaming is done) -->
  {#if !store.streaming && store.results.length > 0}
    <div class="grid gap-3 sm:grid-cols-2">
      {#each store.results as result (result.symbol)}
        {@const isExpanded = selectedSymbol === result.symbol}
        <button
          class="rounded-lg border border-border p-4 text-left transition-colors hover:border-primary/50"
          class:ring-2={isExpanded}
          class:ring-primary={isExpanded}
          onclick={() => (selectedSymbol = result.symbol)}
        >
          <div class="mb-2 flex items-center justify-between">
            <span class="text-sm font-semibold">{result.symbol}</span>
            <div class="flex items-center gap-1.5">
              {#if result.converged}
                <span class="rounded bg-green-100 px-1.5 py-0.5 text-xs font-medium text-green-700 dark:bg-green-900/30 dark:text-green-400">
                  {t('invest_committee_converged')}
                </span>
              {/if}
              {#if result.sentinelOverride}
                <span class="rounded bg-red-100 px-1.5 py-0.5 text-xs font-medium text-red-700 dark:bg-red-900/30 dark:text-red-400">
                  {t('invest_committee_sentinel')}
                </span>
              {/if}
            </div>
          </div>

          <div class="mb-2 flex items-baseline gap-3">
            <span
              class="inline-block rounded px-2 py-0.5 text-sm font-bold {result.finalVerdict === 'BUY' || result.finalVerdict === 'ACCUMULATE'
                ? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400'
                : result.finalVerdict === 'TRIM' || result.finalVerdict === 'SELL'
                  ? 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400'
                  : result.finalVerdict === 'HOLD'
                    ? 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400'
                    : ''}"
            >
              {result.finalVerdict}
            </span>
            <span class="text-sm text-muted-foreground">
              {(result.finalConfidence * 100).toFixed(0)}%
            </span>
          </div>

          <div class="flex gap-2 text-xs text-muted-foreground">
            <span class:opacity-50={!result.sanityCheck.gate1Pass}>G1 {result.sanityCheck.gate1Pass ? '✓' : '✗'}</span>
            <span class:opacity-50={!result.sanityCheck.gate2Pass}>G2 {result.sanityCheck.gate2Pass ? '✓' : '✗'}</span>
            <span class:opacity-50={!result.sanityCheck.gate3Pass}>G3 {result.sanityCheck.gate3Pass ? '✓' : '✗'}</span>
            <span class="ml-auto">{result.totalTokens} tok / {(result.totalLatencyMs / 1000).toFixed(1)}s</span>
          </div>
        </button>
      {/each}
    </div>

    <!-- Expanded detail for selected symbol -->
    {#if selectedSymbol}
      {@const result = store.results.find((r) => r.symbol === selectedSymbol)}
      {#if result}
        <div class="mt-4 space-y-2">
          {#if result.reasoning}
            <div class="rounded-lg border border-border p-3">
              <div class="mb-1 text-xs font-medium text-muted-foreground">{t('invest_cio_reasoning')}</div>
              <div class="max-h-32 overflow-y-auto whitespace-pre-wrap text-sm">{result.reasoning}</div>
            </div>
          {/if}
          {#if result.sanityCheck.notes.length > 0}
            <div class="rounded bg-muted/50 px-3 py-2 text-xs text-muted-foreground">
              {#each result.sanityCheck.notes as note}
                <div>- {note}</div>
              {/each}
            </div>
          {/if}
          {#if result.sentinelOverride}
            <div class="rounded bg-red-50 px-3 py-2 text-xs text-red-600 dark:bg-red-900/20 dark:text-red-400">
              {result.sentinelOverride.reason}
            </div>
          {/if}
          <!-- All round details -->
          {#if result.rounds.length > 0}
            <div class="space-y-2">
              {#each result.rounds as round}
                <DebateBlock {round} />
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    {/if}
  {/if}
</div>
