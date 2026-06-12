<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import type { RoundOutputSummary } from '$lib/stores/invest-committee-store.svelte';

  let { round, blockState = 'done', isStreaming = false }: {
    round: RoundOutputSummary;
    blockState?: 'pending' | 'active' | 'done' | 'error' | 'failed';
    isStreaming?: boolean;
  } = $props();

  let collapsed = $state(false);

  const ROLE_COLORS: Record<string, string> = {
    macro: '#8b5cf6',
    quant: '#3b82f6',
    risk: '#f97316',
    l4_officer: '#ef4444',
    cio: '#eab308',
  };

  const roleColor = $derived(ROLE_COLORS[round.role] ?? '#6b7280');
</script>

<div
  class="debate-block rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-3)]"
  style="border-left: 3px solid {roleColor};"
>
  <!-- Header -->
  <div class="flex items-center gap-[var(--space-2)]">
    <span
      class="inline-block rounded-[var(--radius-full)] px-3 py-1 text-[11px] font-bold text-white"
      style="background: {roleColor};"
    >
      {round.label}
    </span>

    {#if isStreaming}
      <span class="flex items-center gap-1 text-[12px] text-[var(--text-secondary)]">
        <span class="spinner"></span>
        {t('invest_debate_thinking')}
      </span>
    {/if}

    <span class="ml-auto flex items-center gap-[var(--space-2)] text-[12px] text-[var(--text-tertiary)]">
      {#if round.latencyMs > 0}
        <span class="font-[var(--font-mono)]">{(round.latencyMs / 1000).toFixed(1)}s</span>
      {/if}
      {#if round.tokensUsed > 0}
        <span class="font-[var(--font-mono)]">{round.tokensUsed} tok</span>
      {/if}
    </span>

    <button
      class="ml-1 text-[12px] text-[var(--text-tertiary)] hover:text-[var(--text-primary)]"
      onclick={() => (collapsed = !collapsed)}
    >
      {collapsed ? '▸' : '▾'}
    </button>
  </div>

  <!-- Body -->
  {#if !collapsed}
    {#if blockState === 'active'}
      <div class="mt-[var(--space-2)] flex items-center gap-[var(--space-2)] text-[14px] text-[var(--text-secondary)]">
        <span class="spinner"></span>
        {t('invest_debate_waiting_llm')}
      </div>
    {:else if round.parsed.rawText}
      <div class="mt-[var(--space-2)] max-h-48 overflow-y-auto whitespace-pre-wrap text-[14px] text-[var(--text-primary)]">
        {round.parsed.rawText}
      </div>
    {/if}

    <!-- Structured fields -->
    {#if round.parsed.signal}
      <div class="mt-[var(--space-1)] text-[12px] text-[var(--text-secondary)]">
        {t('invest_signal_label')}: {round.parsed.signal}
        {#if round.parsed.strength != null}
          ({t('invest_strength_label')}: {round.parsed.strength.toFixed(1)})
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .spinner {
    display: inline-block;
    width: 0.75rem;
    height: 0.75rem;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
