<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { fortuneStore } from '$lib/stores/fortune-store.svelte';
  import { levelColor, fmtReturn, fmtScore, levelLabel } from './fortune-helpers';
  const s = $derived(fortuneStore.summary);
  onMount(() => { if (!s) fortuneStore.loadAll(); });
  const kpis = $derived(s ? [
    { label: t('fortune_kpi_total'), val: `${s.totalDays}` },
    { label: t('fortune_kpi_win'), val: `${s.winDays}` },
    { label: t('fortune_kpi_winrate'), val: `${(s.winRate * 100).toFixed(0)}%` },
    { label: t('fortune_kpi_cumulative'), val: fmtReturn(s.cumulativeReturn) },
    { label: t('fortune_kpi_avg'), val: fmtReturn(s.avgDailyReturn) },
  ] : []);
</script>

{#if !s || s.totalDays === 0}
  <div class="text-[var(--text-tertiary)]">{t('fortune_insufficient')}</div>
{:else}
  <!-- KPI 卡片 -->
  <div class="mb-[var(--space-4)] grid grid-cols-2 gap-[var(--space-3)] sm:grid-cols-5">
    {#each kpis as k}
      <div class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-card)] p-[var(--space-3)]">
        <div class="text-[11px] text-[var(--text-tertiary)]">{k.label}</div>
        <div class="font-mono text-[20px] font-bold text-[var(--text-primary)]">{k.val}</div>
      </div>
    {/each}
  </div>

  <!-- Top3 排行 / 风险 -->
  <div class="mb-[var(--space-4)] grid grid-cols-1 gap-[var(--space-4)] md:grid-cols-2">
    <!-- 排行 -->
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
      <h3 class="mb-[var(--space-3)] text-[13px] font-semibold text-[var(--text-primary)]">{t('fortune_top_rank')}</h3>
      <div class="grid grid-cols-2 gap-[var(--space-3)]">
        <div>
          <div class="mb-[var(--space-2)] text-[11px] text-[var(--text-tertiary)]">天干</div>
          {#each s.topStems as item, i}
            <div class="flex items-center justify-between py-[var(--space-1)]">
              <span class="font-mono text-[12px] text-[var(--text-primary)]">{i + 1}. {item.name}</span>
              <span class="font-mono text-[12px] font-semibold" style="color:{levelColor(item.level)}">{fmtScore(item.score)}</span>
            </div>
          {/each}
        </div>
        <div>
          <div class="mb-[var(--space-2)] text-[11px] text-[var(--text-tertiary)]">地支</div>
          {#each s.topBranches as item, i}
            <div class="flex items-center justify-between py-[var(--space-1)]">
              <span class="font-mono text-[12px] text-[var(--text-primary)]">{i + 1}. {item.name}</span>
              <span class="font-mono text-[12px] font-semibold" style="color:{levelColor(item.level)}">{fmtScore(item.score)}</span>
            </div>
          {/each}
        </div>
      </div>
    </div>

    <!-- 风险 -->
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
      <h3 class="mb-[var(--space-3)] text-[13px] font-semibold text-[var(--text-primary)]">{t('fortune_top_risk')}</h3>
      <div class="grid grid-cols-2 gap-[var(--space-3)]">
        <div>
          <div class="mb-[var(--space-2)] text-[11px] text-[var(--text-tertiary)]">天干</div>
          {#each s.riskStems as item, i}
            <div class="flex items-center justify-between py-[var(--space-1)]">
              <span class="font-mono text-[12px] text-[var(--text-primary)]">{i + 1}. {item.name}</span>
              <span class="font-mono text-[12px] font-semibold" style="color:{levelColor(item.level)}">{fmtScore(item.score)}</span>
            </div>
          {/each}
        </div>
        <div>
          <div class="mb-[var(--space-2)] text-[11px] text-[var(--text-tertiary)]">地支</div>
          {#each s.riskBranches as item, i}
            <div class="flex items-center justify-between py-[var(--space-1)]">
              <span class="font-mono text-[12px] text-[var(--text-primary)]">{i + 1}. {item.name}</span>
              <span class="font-mono text-[12px] font-semibold" style="color:{levelColor(item.level)}">{fmtScore(item.score)}</span>
            </div>
          {/each}
        </div>
      </div>
    </div>
  </div>

  <!-- 月度统计 -->
  <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
    <h3 class="mb-[var(--space-3)] text-[13px] font-semibold text-[var(--text-primary)]">{t('fortune_monthly')}</h3>
    <div class="flex items-end gap-[var(--space-2)]">
      {#each s.monthly as m}
        <div class="flex flex-col items-center">
          <div class="text-[10px] font-mono" style="color:{m.avgReturn >= 0 ? 'var(--up)' : 'var(--down)'}">{fmtReturn(m.avgReturn)}</div>
          <div style="height:{Math.min(Math.abs(m.avgReturn) * 20, 80)}px;width:20px;
            background:{m.avgReturn >= 0 ? 'var(--up)' : 'var(--down)'};border-radius:2px"></div>
          <div class="mt-1 text-[10px] text-[var(--text-tertiary)]">{m.month.slice(5)}</div>
        </div>
      {/each}
    </div>
  </div>
{/if}
