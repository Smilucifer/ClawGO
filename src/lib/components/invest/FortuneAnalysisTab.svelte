<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { fortuneStore } from '$lib/stores/fortune-store.svelte';
  import type { DayScore } from '$lib/stores/fortune-store.svelte';
  import { levelLabel, levelColor, fmtScore, fmtReturn, returnColor } from './fortune-helpers';
  import FortuneCalendar from './FortuneCalendar.svelte';
  import FortuneRecordDialog from './FortuneRecordDialog.svelte';

  let showDialog = $state(false);
  let reading = $state<string | null>(null);
  let readingError = $state<string | null>(null);

  const analysis = $derived(fortuneStore.analysis);
  const today = $derived(analysis?.today ?? null);
  const tomorrow = $derived(analysis?.tomorrow ?? null);

  onMount(async () => {
    await fortuneStore.loadAll();
    if (today) reading = await fortuneStore.getReading(today.date).catch(() => null);
  });

  // 卡副标签：已录=盘后，未录=预测
  function cardLabel(d: DayScore): string {
    return d.actualReturn != null ? t('fortune_post') : t('fortune_predict');
  }
  function cardScore(d: DayScore): number {
    return d.postScore ?? d.predictScore;
  }

  async function genReading() {
    if (!today) return;
    readingError = null;
    try { reading = await fortuneStore.generateReading(today.date); }
    catch (e) { readingError = String(e); }
  }
</script>

{#if !analysis || analysis.calendar.length === 0}
  <div class="rounded-[var(--radius-lg)] border border-dashed border-border bg-[var(--bg-card)] p-[var(--space-6)] text-center text-[13px] text-[var(--text-tertiary)]">
    {t('fortune_empty_hint')}
  </div>
{:else}
  <div class="grid grid-cols-1 gap-[var(--space-4)] lg:grid-cols-[1fr_1.4fr]">
    <div class="flex flex-col gap-[var(--space-4)]">
      <!-- 今日卡 -->
      {#if today}
        {@const sc = cardScore(today)}
        {@const lvl = today.postLevel ?? today.predictLevel}
        <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
          <div class="mb-[var(--space-3)] flex justify-between text-[12px] text-[var(--text-secondary)]">
            <span>今日 {today.date}</span><span>{cardLabel(today)}</span>
          </div>
          <div class="flex items-center gap-[var(--space-4)]">
            <div class="font-mono text-[44px] font-bold" style="color:var(--accent)">{today.stem}{today.branch}</div>
            <div class="font-mono text-[52px] font-extrabold" style="color:{levelColor(lvl)}">{fmtScore(sc)}</div>
            <span class="rounded-[var(--radius-sm)] px-[var(--space-3)] py-[var(--space-1)] text-[15px] font-semibold"
              style="color:{levelColor(lvl)}">{levelLabel(lvl)}</span>
          </div>
          {#if today.actualReturn != null}
            <div class="mt-[var(--space-2)] font-mono text-[14px]" style="color:{returnColor(today.actualReturn)}">
              实测 {fmtReturn(today.actualReturn)}
            </div>
          {/if}
        </div>
      {/if}

      <!-- 明日卡 -->
      {#if tomorrow}
        {@const sc = cardScore(tomorrow)}
        {@const lvl = tomorrow.predictLevel}
        <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
          <div class="mb-[var(--space-3)] flex justify-between text-[12px] text-[var(--text-secondary)]">
            <span>明日 {tomorrow.date}</span><span>{t('fortune_predict')}</span>
          </div>
          <div class="flex items-center gap-[var(--space-4)]">
            <div class="font-mono text-[44px] font-bold" style="color:var(--accent)">{tomorrow.stem}{tomorrow.branch}</div>
            <div class="font-mono text-[52px] font-extrabold" style="color:{levelColor(lvl)}">{fmtScore(sc)}</div>
            <span class="rounded-[var(--radius-sm)] px-[var(--space-3)] py-[var(--space-1)] text-[15px] font-semibold"
              style="color:{levelColor(lvl)}">{levelLabel(lvl)}</span>
          </div>
        </div>
      {/if}
    </div>

    <!-- 右侧 AI 解读卡 -->
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-[var(--space-4)]">
      <div class="mb-[var(--space-3)] flex items-center justify-between">
        <span class="text-[13px] text-[var(--text-secondary)]">AI 解读</span>
        <button class="rounded-[var(--radius-sm)] bg-[var(--accent)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] font-semibold text-[var(--bg-base)] disabled:opacity-50"
          disabled={fortuneStore.readingBusy || !today} onclick={genReading}>
          {fortuneStore.readingBusy ? t('fortune_generating') : t('fortune_generate_reading')}
        </button>
      </div>
      {#if fortuneStore.readingBusy}
        <div class="h-16 animate-pulse rounded bg-[var(--bg-input)]"></div>
      {:else if readingError}
        <div class="text-[13px]" style="color:var(--color-error)">{t('fortune_reading_failed')}</div>
      {:else if reading}
        <p class="text-[13px] leading-relaxed text-[var(--text-primary)]">{reading}</p>
      {/if}
    </div>
  </div>

  <div class="mt-[var(--space-4)]"><FortuneCalendar /></div>
{/if}

<div class="mt-[var(--space-4)]">
  <button class="rounded-[var(--radius-sm)] bg-[var(--accent)] px-[var(--space-4)] py-[var(--space-2)] text-[13px] font-semibold text-[var(--bg-base)]"
    onclick={() => (showDialog = true)}>{t('fortune_record_btn')}</button>
</div>
{#if showDialog}
  <FortuneRecordDialog onclose={() => (showDialog = false)} />
{/if}
