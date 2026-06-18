<script lang="ts">
  import { onDestroy } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { formatYuan } from '$lib/utils/format';
  import { Chart, LineController, LineElement, PointElement, LinearScale, CategoryScale, Tooltip, Legend, Filler } from 'chart.js';

  Chart.register(LineController, LineElement, PointElement, LinearScale, CategoryScale, Tooltip, Legend, Filler);

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
            borderColor: '#3b82f6',
            backgroundColor: 'rgba(59, 130, 246, 0.1)',
            fill: true,
            tension: 0.3,
            pointRadius: 2,
            pointHoverRadius: 4,
            yAxisID: 'y',
          },
          {
            label: t('invest_pnl_daily_pnl'),
            data: dailyPnlData,
            borderColor: '#8a9a76',
            backgroundColor: 'rgba(138, 154, 118, 0.1)',
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
          x: {
            grid: { color: 'rgba(255,255,255,0.04)' },
            ticks: { color: '#6b6660', font: { size: 10 } },
          },
          y: {
            type: 'linear',
            position: 'left',
            beginAtZero: false,
            grid: { color: 'rgba(255,255,255,0.04)' },
            ticks: { color: '#6b6660', font: { size: 10 } },
            title: { display: true, text: t('invest_pnl_total_assets'), color: '#9a9590', font: { size: 11 } },
          },
          y1: {
            type: 'linear',
            position: 'right',
            beginAtZero: true,
            grid: { drawOnChartArea: false },
            ticks: { color: '#6b6660', font: { size: 10 } },
            title: { display: true, text: t('invest_pnl_daily_pnl'), color: '#9a9590', font: { size: 11 } },
          },
        },
        plugins: {
          legend: {
            display: true,
            labels: { color: '#9a9590', font: { size: 11 }, boxWidth: 12, padding: 16 },
          },
          tooltip: {
            backgroundColor: '#242220',
            titleColor: '#ebe8e4',
            bodyColor: '#9a9590',
            borderColor: 'rgba(255,255,255,0.06)',
            borderWidth: 1,
            callbacks: {
              label: (ctx) => {
                const val = ctx.parsed.y ?? 0;
                if (ctx.datasetIndex === 0) return `${t('invest_pnl_total_assets')}: ${formatYuan(val)}`;
                return `${t('invest_pnl_daily_pnl')}: ${formatYuan(val)}`;
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

<div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
  <div class="mb-[var(--space-2)] flex items-center justify-between">
    <h3 class="text-[14px] font-semibold text-[var(--text-primary)]">📈 {t('invest_pnl_chart')}</h3>
    <span class="text-[11px] text-[var(--text-tertiary)]">左轴: {t('invest_pnl_total_assets')} | 右轴: {t('invest_pnl_daily_pnl')}</span>
  </div>
  {#if investStore.pnlSnapshots.length === 0}
    <p class="py-[var(--space-4)] text-center text-[12px] text-[var(--text-tertiary)]">{t('invest_no_pnl')}</p>
  {:else}
    <div style="height: 220px;">
      <canvas bind:this={canvas}></canvas>
    </div>
  {/if}
</div>
