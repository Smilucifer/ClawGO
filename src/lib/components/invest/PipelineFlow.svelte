<script lang="ts">
  import type { SymbolProgress } from '$lib/stores/invest-committee-store.svelte';

  let { progress, compact = false }: {
    progress: SymbolProgress | undefined;
    compact?: boolean;
  } = $props();

  const NODES = [
    { role: 'macro', label: '宏观', color: '#8b5cf6', icon: 'M' },
    { role: 'quant_r1', label: '量化R1', color: '#3b82f6', icon: 'Q1' },
    { role: 'risk_r1', label: '风控R1', color: '#f97316', icon: 'R1' },
    { role: 'wealth', label: '财富', color: '#22c55e', icon: 'W' },
    { role: 'quant_r2', label: '量化R2', color: '#3b82f6', icon: 'Q2' },
    { role: 'risk_r2', label: '风控R2', color: '#f97316', icon: 'R2' },
    { role: 'cio', label: 'CIO', color: '#eab308', icon: 'C' },
  ];

  function nodeState(index: number): 'pending' | 'active' | 'done' | 'error' {
    if (!progress) return 'pending';
    if (progress.error && progress.activeStep === -1) return 'error';
    if (index === progress.activeStep) return 'active';
    if (index < progress.activeStep || progress.done) return 'done';
    // Check completedRounds to determine which steps are done
    const completedSteps = progress.completedRounds.length;
    if (index < completedSteps) return 'done';
    return 'pending';
  }
</script>

<div class="pipeline-flow" class:compact>
  {#each NODES as node, i}
    {@const state = nodeState(i)}
    <div class="pipeline-node-wrapper">
      <!-- Connection line (before node, except first) -->
      {#if i > 0}
        <div
          class="connector"
          class:active={state === 'done' || state === 'active'}
          style="--from-color: {NODES[i - 1].color}; --to-color: {node.color};"
        ></div>
      {/if}

      <!-- Node -->
      <div
        class="node"
        class:pending={state === 'pending'}
        class:active={state === 'active'}
        class:done={state === 'done'}
        class:error={state === 'error'}
        style="--node-color: {node.color};"
        title={node.label}
      >
        <span class="node-icon">
          {#if state === 'done'}✓{:else if state === 'error'}✗{:else}{node.icon}{/if}
        </span>
        {#if !compact}
          <span class="node-label">{node.label}</span>
        {/if}
      </div>
    </div>
  {/each}
</div>

<style>
  .pipeline-flow {
    display: flex;
    align-items: center;
    gap: 0;
    padding: 0.75rem 0;
    overflow-x: auto;
  }

  .pipeline-flow.compact {
    padding: 0.25rem 0;
    transform: scale(0.8);
    transform-origin: left center;
  }

  .pipeline-node-wrapper {
    display: flex;
    align-items: center;
  }

  .connector {
    width: 2rem;
    height: 3px;
    border-radius: 2px;
    background: var(--border);
    transition: background 0.4s ease;
  }

  .connector.active {
    background: linear-gradient(to right, var(--from-color), var(--to-color));
  }

  .node {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    width: 3rem;
    height: 3rem;
    border-radius: 50%;
    border: 2px solid var(--border);
    background: var(--background);
    transition: all 0.3s ease;
    cursor: default;
    position: relative;
  }

  .compact .node {
    width: 2rem;
    height: 2rem;
  }

  .node-icon {
    font-size: 0.75rem;
    font-weight: 600;
    line-height: 1;
  }

  .node-label {
    font-size: 0.6rem;
    margin-top: 0.25rem;
    white-space: nowrap;
    color: var(--muted-foreground);
    position: absolute;
    bottom: -1.25rem;
  }

  .node.pending {
    opacity: 0.5;
  }

  .node.active {
    border-color: var(--node-color);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--node-color) 30%, transparent);
    animation: pulse 1.5s ease-in-out infinite;
  }

  .node.done {
    border-color: var(--node-color);
    background: color-mix(in srgb, var(--node-color) 15%, var(--background));
    color: var(--node-color);
  }

  .node.error {
    border-color: hsl(0, 84%, 60%);
    background: hsl(0, 84%, 95%);
    color: hsl(0, 84%, 50%);
  }

  :global(.dark) .node.error {
    background: hsl(0, 84%, 15%);
    color: hsl(0, 84%, 70%);
  }

  @keyframes pulse {
    0%, 100% { box-shadow: 0 0 0 3px color-mix(in srgb, var(--node-color) 30%, transparent); }
    50% { box-shadow: 0 0 0 6px color-mix(in srgb, var(--node-color) 15%, transparent); }
  }
</style>
