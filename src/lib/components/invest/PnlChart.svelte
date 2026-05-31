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
    const totalAssetsData = snapshots.map((s) => s.totalValue);
    const dailyPnlData = snapshots.map((s) => s.dailyPnl ?? 0);

    if (chart) chart.destroy();

    chart = new Chart(canvas, {
      type: 'line',
      data: {
        labels,
        datasets: [
          {
            label: t('invest_pnl_total_assets'),
            data: totalAssetsData,
            borderColor: 'rgb(59, 130, 246)',
            backgroundColor: 'rgba(59, 130, 246, 0.1)',
            fill: true,
            tension: 0.3,
            pointRadius: 2,
            yAxisID: 'y',
          },
          {
            label: t('invest_pnl_daily_pnl'),
            data: dailyPnlData,
            borderColor: 'rgb(34, 197, 94)',
            backgroundColor: 'rgba(34, 197, 94, 0.1)',
            fill: false,
            tension: 0.3,
            pointRadius: 2,
            borderDash: [4, 2],
            yAxisID: 'y1',
          },
        ],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        interaction: {
          mode: 'index',
          intersect: false,
        },
        scales: {
          y: {
            type: 'linear',
            position: 'left',
            beginAtZero: false,
            title: { display: true, text: t('invest_pnl_total_assets') },
          },
          y1: {
            type: 'linear',
            position: 'right',
            beginAtZero: true,
            grid: { drawOnChartArea: false },
            title: { display: true, text: t('invest_pnl_daily_pnl') },
          },
        },
        plugins: {
          legend: { display: true },
          tooltip: {
            callbacks: {
              label: (ctx) => {
                const val = ctx.parsed.y ?? 0;
                if (ctx.datasetIndex === 0) return `${t('invest_pnl_total_assets')}: ¥${val.toLocaleString()}`;
                return `${t('invest_pnl_daily_pnl')}: ¥${val.toLocaleString()}`;
              },
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
