<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { fortuneStore } from '$lib/stores/fortune-store.svelte';
  import { levelLabel, levelColor, fmtScore, fmtReturn } from './fortune-helpers';
  const ov = $derived(fortuneStore.overview);
  onMount(() => { if (!ov) fortuneStore.loadAll(); });
</script>

{#if !ov || ov.stems.every((s) => s.sample === 0)}
  <div class="text-[var(--text-tertiary)]">{t('fortune_insufficient')}</div>
{:else}
  <!-- 预告卡：4 路（最强/最弱 天干/地支） -->
  <div class="mb-[var(--space-4)] grid grid-cols-2 gap-[var(--space-3)] sm:grid-cols-4">
    {#each ov.forecasts as f}
      <div class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-card)] p-[var(--space-3)]"
        style="border-left:3px solid {f.isStrong ? 'var(--up)' : 'var(--down)'}">
        <div class="text-[11px] text-[var(--text-tertiary)]">{f.label}</div>
        <div class="font-mono text-[18px]" style="color:var(--accent)">{f.ganzhi}</div>
        <div class="text-[12px] text-[var(--text-secondary)]">{f.date} {f.weekday}</div>
        <div class="text-[12px]" style="color:{levelColor(f.level)}">{fmtScore(f.score)} {levelLabel(f.level)}</div>
      </div>
    {/each}
  </div>

  <!-- 天干表：10 行 -->
  <div class="mb-[var(--space-4)]">
    <h3 class="mb-[var(--space-2)] text-[13px] font-semibold text-[var(--text-primary)]">天干排行</h3>
    <div class="overflow-x-auto">
      <table class="w-full text-[12px]">
        <thead>
          <tr class="border-b border-border text-[var(--text-tertiary)]">
            <th class="px-[var(--space-2)] py-[var(--space-1)] text-left">干支</th>
            <th class="px-[var(--space-2)] py-[var(--space-1)] text-right">均收益</th>
            <th class="px-[var(--space-2)] py-[var(--space-1)] text-right">胜率</th>
            <th class="px-[var(--space-2)] py-[var(--space-1)] text-right">次数</th>
            <th class="px-[var(--space-2)] py-[var(--space-1)] text-right">分数</th>
            <th class="px-[var(--space-2)] py-[var(--space-1)] text-center">吉凶</th>
          </tr>
        </thead>
        <tbody>
          {#each ov.stems as s}
            <tr class="border-b border-border/50">
              <td class="px-[var(--space-2)] py-[var(--space-1)] font-mono font-semibold text-[var(--text-primary)]">{s.name}</td>
              <td class="px-[var(--space-2)] py-[var(--space-1)] text-right font-mono" style="color:{s.avgReturn >= 0 ? 'var(--up)' : 'var(--down)'}">{fmtReturn(s.avgReturn)}</td>
              <td class="px-[var(--space-2)] py-[var(--space-1)] text-right font-mono">{(s.winRate * 100).toFixed(0)}%</td>
              <td class="px-[var(--space-2)] py-[var(--space-1)] text-right font-mono">{s.sample}</td>
              <td class="px-[var(--space-2)] py-[var(--space-1)] text-right font-mono font-semibold" style="color:{levelColor(s.level)}">{fmtScore(s.score)}</td>
              <td class="px-[var(--space-2)] py-[var(--space-1)] text-center">
                <span class="rounded-full px-2 py-0.5 text-[11px] font-semibold" style="color:{levelColor(s.level)};background:{levelColor(s.level)}22">{levelLabel(s.level)}</span>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>

  <!-- 地支表：12 行 -->
  <div>
    <h3 class="mb-[var(--space-2)] text-[13px] font-semibold text-[var(--text-primary)]">地支排行</h3>
    <div class="overflow-x-auto">
      <table class="w-full text-[12px]">
        <thead>
          <tr class="border-b border-border text-[var(--text-tertiary)]">
            <th class="px-[var(--space-2)] py-[var(--space-1)] text-left">干支</th>
            <th class="px-[var(--space-2)] py-[var(--space-1)] text-right">均收益</th>
            <th class="px-[var(--space-2)] py-[var(--space-1)] text-right">胜率</th>
            <th class="px-[var(--space-2)] py-[var(--space-1)] text-right">次数</th>
            <th class="px-[var(--space-2)] py-[var(--space-1)] text-right">分数</th>
            <th class="px-[var(--space-2)] py-[var(--space-1)] text-center">吉凶</th>
          </tr>
        </thead>
        <tbody>
          {#each ov.branches as b}
            <tr class="border-b border-border/50">
              <td class="px-[var(--space-2)] py-[var(--space-1)] font-mono font-semibold text-[var(--text-primary)]">{b.name}</td>
              <td class="px-[var(--space-2)] py-[var(--space-1)] text-right font-mono" style="color:{b.avgReturn >= 0 ? 'var(--up)' : 'var(--down)'}">{fmtReturn(b.avgReturn)}</td>
              <td class="px-[var(--space-2)] py-[var(--space-1)] text-right font-mono">{(b.winRate * 100).toFixed(0)}%</td>
              <td class="px-[var(--space-2)] py-[var(--space-1)] text-right font-mono">{b.sample}</td>
              <td class="px-[var(--space-2)] py-[var(--space-1)] text-right font-mono font-semibold" style="color:{levelColor(b.level)}">{fmtScore(b.score)}</td>
              <td class="px-[var(--space-2)] py-[var(--space-1)] text-center">
                <span class="rounded-full px-2 py-0.5 text-[11px] font-semibold" style="color:{levelColor(b.level)};background:{levelColor(b.level)}22">{levelLabel(b.level)}</span>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </div>
{/if}
