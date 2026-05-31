<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';

  const latestVerdict = $derived(investStore.verdicts[0] ?? null);

  const signalColor = $derived(
    latestVerdict?.macroSignal === 'risk_on'
      ? 'text-green-500'
      : latestVerdict?.macroSignal === 'risk_off'
        ? 'text-red-500'
        : 'text-yellow-500'
  );
</script>

<div class="rounded-lg border border-border p-4">
  <h3 class="mb-3 text-sm font-semibold flex items-center gap-2">
    <span class="h-4 w-0.5 rounded-full bg-primary"></span>
    {t('invest_macro_snapshot')}
  </h3>

  {#if latestVerdict?.macroSignal}
    <div class="space-y-2">
      <div class="flex items-center gap-2">
        <span class="text-xs text-muted-foreground">{t('invest_macro_signal')}:</span>
        <span class="text-sm font-bold {signalColor}">{latestVerdict.macroSignal}</span>
      </div>
      {#if latestVerdict.macroStrength != null}
        <div class="flex items-center gap-2">
          <span class="text-xs text-muted-foreground">{t('invest_macro_strength')}:</span>
          <div class="flex-1 h-2 rounded-full bg-muted overflow-hidden">
            <div
              class="h-full rounded-full transition-all"
              class:bg-green-500={latestVerdict.macroStrength >= 7}
              class:bg-yellow-500={latestVerdict.macroStrength >= 4 && latestVerdict.macroStrength < 7}
              class:bg-red-500={latestVerdict.macroStrength < 4}
              style="width: {(latestVerdict.macroStrength / 10) * 100}%"
            ></div>
          </div>
          <span class="text-xs font-mono">{latestVerdict.macroStrength}/10</span>
        </div>
      {/if}
      <div class="text-xs text-muted-foreground">
        {t('invest_macro_from')}: {latestVerdict.symbol} · {new Date(latestVerdict.createdAt).toLocaleDateString()}
      </div>
    </div>
  {:else}
    <p class="text-xs text-muted-foreground">{t('invest_macro_no_data')}</p>
  {/if}
</div>
