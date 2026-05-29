<script lang="ts">
  import { onDestroy } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { Chart, LineController, LineElement, PointElement, LinearScale, CategoryScale, Tooltip, Legend } from 'chart.js';

  Chart.register(LineController, LineElement, PointElement, LinearScale, CategoryScale, Tooltip, Legend);

  let canvas: HTMLCanvasElement;
  let chart: Chart | null = null;

  function buildChart() {
    if (!canvas || investStore.pnlSnapshots.length === 0) return;

    const snapshots = [...investStore.pnlSnapshots].reverse();
    const labels = snapshots.map((s) => s.snapshotDate);
    const data = snapshots.map((s) => s.totalValue);

    if (chart) chart.destroy();

    chart = new Chart(canvas, {
      type: 'line',
      data: {
        labels,
        datasets: [
          {
            label: 'Total Assets',
            data,
            borderColor: 'rgb(59, 130, 246)',
            backgroundColor: 'rgba(59, 130, 246, 0.1)',
            fill: true,
            tension: 0.3,
            pointRadius: 2,
          },
        ],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          y: { beginAtZero: false },
        },
        plugins: {
          legend: { display: false },
          tooltip: {
            callbacks: {
              label: (ctx) => `¥${(ctx.parsed.y ?? 0).toLocaleString()}`,
            },
          },
        },
      },
    });
  }

  $effect(() => {
    if (investStore.pnlSnapshots.length > 0 && canvas) {
      buildChart();
    }
  });

  onDestroy(() => {
    if (chart) chart.destroy();
  });
</script>

<div>
  <h3 class="mb-2 text-sm font-medium text-muted-foreground">{t('invest_pnl_chart')}</h3>
  {#if investStore.pnlSnapshots.length === 0}
    <p class="py-4 text-center text-sm text-muted-foreground">{t('invest_no_pnl')}</p>
  {:else}
    <div class="rounded-lg border p-4" style="height: 300px;">
      <canvas bind:this={canvas}></canvas>
    </div>
  {/if}
</div>
