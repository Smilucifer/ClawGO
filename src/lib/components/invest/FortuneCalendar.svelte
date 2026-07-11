<script lang="ts">
  import { fortuneStore } from '$lib/stores/fortune-store.svelte';
  import type { DayScore } from '$lib/stores/fortune-store.svelte';
  import { levelColor, fmtScore } from './fortune-helpers';

  // 后端已产出完整月历（三态齐）；这里按 "YYYY-MM" 分组，默认展示最后一个月，可翻页。
  const all = $derived(fortuneStore.analysis?.calendar ?? []);
  const months = $derived([...new Set(all.map((d) => d.date.slice(0, 7)))]);  // 升序
  let monthIdx = $state(0);   // 相对 months 末尾的偏移，0=最新月
  const curMonth = $derived(months[months.length - 1 - monthIdx] ?? '');
  const cells = $derived(all.filter((d) => d.date.startsWith(curMonth)));

  // 单格样式：三态 = 休市/预测/盘后
  function cellStyle(d: DayScore): { border: string; bg: string; dashed: boolean } {
    if (!d.isTradingDay) return { border: 'var(--border)', bg: 'var(--bg-input)', dashed: false };
    const lvl = d.postLevel ?? d.predictLevel;
    const color = levelColor(lvl);
    const isPost = d.actualReturn != null;
    return { border: color, bg: isPost ? color + '22' : 'transparent', dashed: !isPost };
  }

  // 获取某月第一天是周几（0=周日，1=周一...6=周六）
  function firstDayOfMonth(monthStr: string): number {
    const d = new Date(monthStr + '-01');
    return d.getDay();
  }

  // 格式化日期显示（去掉年月，只显示日）
  function dayOfMonth(dateStr: string): string {
    return parseInt(dateStr.split('-')[2], 10).toString();
  }
</script>

{#if months.length === 0}
  <div class="text-[var(--text-tertiary)] text-[13px]">暂无日历数据</div>
{:else}
  <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
    <!-- 月份翻页 -->
    <div class="mb-[var(--space-3)] flex items-center justify-between">
      <button
        class="rounded-[var(--radius-sm)] px-[var(--space-2)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] disabled:opacity-40"
        disabled={monthIdx >= months.length - 1}
        onclick={() => monthIdx++}
      >‹</button>
      <span class="font-mono text-[13px] font-semibold text-[var(--text-primary)]">{curMonth}</span>
      <button
        class="rounded-[var(--radius-sm)] px-[var(--space-2)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] disabled:opacity-40"
        disabled={monthIdx <= 0}
        onclick={() => monthIdx--}
      >›</button>
    </div>

    <!-- 星期头 -->
    <div class="mb-[var(--space-2)] grid grid-cols-7 gap-1 text-center text-[11px] text-[var(--text-tertiary)]">
      {#each ['日', '一', '二', '三', '四', '五', '六'] as wd}
        <div>{wd}</div>
      {/each}
    </div>

    <!-- 日历格 -->
    <div class="grid grid-cols-7 gap-1">
      <!-- 首格前的空位 -->
      {#each Array(firstDayOfMonth(curMonth)) as _}
        <div></div>
      {/each}

      {#each cells as d}
        {@const style = cellStyle(d)}
        <div
          class="flex min-h-[48px] flex-col items-center justify-center rounded-[var(--radius-sm)] p-[var(--space-1)] text-[11px]"
          style="border:1px {style.dashed ? 'dashed' : 'solid'} {style.border};background:{style.bg}"
        >
          <span class="font-mono font-semibold text-[var(--text-primary)]">{dayOfMonth(d.date)}</span>
          <span class="text-[10px] text-[var(--text-tertiary)]">{d.stem}{d.branch}</span>
          {#if d.isTradingDay}
            <span class="font-mono text-[10px] font-semibold" style="color:{levelColor(d.postLevel ?? d.predictLevel)}">
              {fmtScore(d.postScore ?? d.predictScore)}
            </span>
          {/if}
        </div>
      {/each}
    </div>

    <!-- 图例 -->
    <div class="mt-[var(--space-3)] flex flex-wrap gap-[var(--space-3)] text-[10px] text-[var(--text-tertiary)]">
      <div class="flex items-center gap-1">
        <div class="h-3 w-3 rounded-sm" style="border:1px solid var(--up);background:var(--up)22"></div>
        <span>盘后（实线）</span>
      </div>
      <div class="flex items-center gap-1">
        <div class="h-3 w-3 rounded-sm" style="border:1px dashed var(--up);background:transparent"></div>
        <span>预测（虚线）</span>
      </div>
      <div class="flex items-center gap-1">
        <div class="h-3 w-3 rounded-sm" style="border:1px solid var(--border);background:var(--bg-input)"></div>
        <span>休市</span>
      </div>
    </div>
  </div>
{/if}
