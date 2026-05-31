<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import type { RoundOutputSummary } from '$lib/stores/invest-committee-store.svelte';

  let { round, blockState = 'done', isStreaming = false }: {
    round: RoundOutputSummary;
    blockState?: 'pending' | 'active' | 'done' | 'error';
    isStreaming?: boolean;
  } = $props();

  let collapsed = $state(false);

  const ROLE_COLORS: Record<string, string> = {
    macro: '#8b5cf6',
    quant: '#3b82f6',
    risk: '#f97316',
    cio: '#eab308',
  };

  const roleColor = $derived(ROLE_COLORS[round.role] ?? '#6b7280');
</script>

<div
  class="debate-block rounded-lg border p-3"
  style="border-left: 3px solid {roleColor};"
>
  <!-- Header -->
  <div class="flex items-center gap-2">
    <span
      class="inline-block rounded px-1.5 py-0.5 text-xs font-medium text-white"
      style="background: {roleColor};"
    >
      {round.label}
    </span>

    {#if isStreaming}
      <span class="flex items-center gap-1 text-xs text-muted-foreground">
        <span class="spinner"></span>
        {t('invest_debate_thinking')}
      </span>
    {/if}

    <span class="ml-auto flex items-center gap-2 text-xs text-muted-foreground">
      {#if round.latencyMs > 0}
        <span>{(round.latencyMs / 1000).toFixed(1)}s</span>
      {/if}
      {#if round.tokensUsed > 0}
        <span>{round.tokensUsed} tok</span>
      {/if}
    </span>

    <button
      class="ml-1 text-xs text-muted-foreground hover:text-foreground"
      onclick={() => (collapsed = !collapsed)}
    >
      {collapsed ? '▸' : '▾'}
    </button>
  </div>

  <!-- Body -->
  {#if !collapsed}
    {#if blockState === 'active'}
      <div class="mt-2 flex items-center gap-2 text-sm text-muted-foreground">
        <span class="spinner"></span>
        {t('invest_debate_waiting_llm')}
      </div>
    {:else if round.parsed.rawText}
      <div class="mt-2 max-h-48 overflow-y-auto whitespace-pre-wrap text-sm text-foreground/90">
        {round.parsed.rawText}
      </div>
    {/if}

    <!-- Structured fields -->
    {#if round.parsed.signal}
      <div class="mt-1 text-xs text-muted-foreground">
        Signal: {round.parsed.signal}
        {#if round.parsed.strength != null}
          (strength: {round.parsed.strength.toFixed(1)})
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  .debate-block {
    background: var(--card);
    transition: all 0.2s ease;
  }

  .spinner {
    display: inline-block;
    width: 0.75rem;
    height: 0.75rem;
    border: 2px solid var(--border);
    border-top-color: var(--primary);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
