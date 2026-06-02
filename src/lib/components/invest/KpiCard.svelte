<script lang="ts">
  let { label, value, sub, trend }: {
    label: string;
    value: string;
    sub?: string;
    trend?: 'up' | 'down' | 'neutral';
  } = $props();
</script>

<div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
  <p class="mb-[var(--space-2)] text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{label}</p>
  <p class="text-[22px] font-bold leading-tight font-[var(--font-mono)] {trend === 'up' ? 'text-[#8a9a76]' : trend === 'down' ? 'text-[#a87a7a]' : 'text-[var(--text-primary)]'}">
    {value}
  </p>
  {#if sub}
    <p class="mt-[var(--space-1)] text-[11px] font-[var(--font-mono)] {trend === 'up' ? 'text-[#8a9a76]' : trend === 'down' ? 'text-[#a87a7a]' : 'text-[var(--text-tertiary)]'}">{sub}</p>
  {/if}
</div>
