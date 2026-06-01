<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';

  const latestVerdict = $derived(investStore.verdicts[0] ?? null);

  const signalLabel = $derived(
    latestVerdict?.macroSignal === 'risk_on'
      ? 'Risk-On'
      : latestVerdict?.macroSignal === 'risk_off'
        ? 'Risk-Off'
        : 'Neutral'
  );

  const signalBadgeClass = $derived(
    latestVerdict?.macroSignal === 'risk_on'
      ? 'bg-[rgba(138,154,118,0.15)] text-[#8a9a76]'
      : latestVerdict?.macroSignal === 'risk_off'
        ? 'bg-[rgba(168,122,122,0.15)] text-[#a87a7a]'
        : 'bg-[var(--accent-muted)] text-[var(--accent)]'
  );

  const barColor = $derived(
    (latestVerdict?.macroStrength ?? 0) >= 7 ? '#8a9a76'
      : (latestVerdict?.macroStrength ?? 0) >= 4 ? '#b89a6a'
        : '#a87a7a'
  );
</script>

<div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] p-[var(--space-4)]">
  <div class="mb-[var(--space-3)] flex items-center justify-between">
    <h3 class="text-[14px] font-semibold text-[var(--text-primary)]">📊 {t('invest_macro_snapshot')}</h3>
    {#if latestVerdict?.macroSignal}
      <span class="rounded-[var(--radius-full)] px-3 py-1 text-[11px] font-semibold {signalBadgeClass}">
        {signalLabel}
      </span>
    {/if}
  </div>

  {#if latestVerdict?.macroSignal}
    <div class="space-y-[var(--space-2)]">
      {#if latestVerdict.macroStrength != null}
        <div class="flex items-center gap-[var(--space-2)]">
          <span class="flex-1 text-[12px] text-[var(--text-tertiary)]">{t('invest_macro_strength')}</span>
          <span class="min-w-[50px] text-right text-[12px] font-[var(--font-mono)] text-[var(--text-secondary)]">{latestVerdict.macroStrength}/10</span>
          <div class="h-[4px] w-[80px] overflow-hidden rounded-[2px] bg-[var(--bg-input)]">
            <div class="h-full rounded-[2px] transition-all" style="width: {(latestVerdict.macroStrength / 10) * 100}%; background: {barColor};"></div>
          </div>
        </div>
      {/if}
      <div class="flex items-center gap-[var(--space-2)]">
        <span class="flex-1 text-[12px] text-[var(--text-tertiary)]">{t('invest_macro_from')}</span>
        <span class="text-[11px] font-[var(--font-mono)] text-[var(--text-secondary)]">{latestVerdict.symbol}</span>
      </div>
      <div class="flex items-center gap-[var(--space-2)]">
        <span class="flex-1 text-[12px] text-[var(--text-tertiary)]">{t('invest_macro_signal')}</span>
        <span class="text-[11px] font-[var(--font-mono)] text-[var(--text-secondary)]">{new Date(latestVerdict.createdAt).toLocaleString()}</span>
      </div>
    </div>
  {:else}
    <p class="text-[12px] text-[var(--text-tertiary)]">{t('invest_macro_no_data')}</p>
  {/if}
</div>
