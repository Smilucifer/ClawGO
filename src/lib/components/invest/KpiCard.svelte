<script lang="ts">
  let { label, value, sub, trend }: {
    label: string;
    value: string;
    sub?: string;
    trend?: 'up' | 'down' | 'neutral';
  } = $props();
</script>

<div class="rounded-lg border bg-card p-4">
  <p class="text-xs text-muted-foreground">{label}</p>
  <p class="mt-1 text-2xl font-bold tabular-nums {trend === 'up' ? 'text-green-600' : trend === 'down' ? 'text-red-600' : ''}">
    {value}
  </p>
  {#if sub}
    <p class="mt-0.5 text-xs text-muted-foreground">{sub}</p>
  {/if}
</div>
