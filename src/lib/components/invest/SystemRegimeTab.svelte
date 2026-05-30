<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';

  let symbol = $state('');
  let loading = $state(false);
  let error = $state('');
  let result = $state<{
    tsCode: string;
    regime: string;
    brief: string;
    metrics: Record<string, unknown>;
    computedAt: string;
  } | null>(null);

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  async function classify() {
    if (!symbol.trim()) return;
    loading = true;
    error = '';
    result = null;
    try {
      const settings = await invoke<{ tushare_token?: string }>('get_user_settings');
      const token = settings.tushare_token ?? '';
      if (!token) {
        error = t('invest_no_token');
        return;
      }
      result = await invoke('get_regime_classification', {
        tsCode: symbol.trim().toUpperCase(),
        tushareToken: token,
      });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
</script>

<div class="space-y-3">
  <h3 class="text-sm font-medium">{t('invest_system_regime_title')}</h3>
  <p class="text-xs text-muted-foreground">{t('invest_system_regime_desc')}</p>

  <div class="flex gap-2">
    <input
      type="text"
      class="w-48 rounded border border-border bg-background px-3 py-1.5 text-sm"
      placeholder={t('invest_system_regime_placeholder')}
      bind:value={symbol}
      onkeydown={(e) => e.key === 'Enter' && classify()}
    />
    <button
      class="rounded bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
      onclick={classify}
      disabled={loading || !symbol.trim()}
    >
      {loading ? t('invest_loading') : t('invest_system_regime_classify')}
    </button>
  </div>

  {#if error}
    <p class="text-sm text-red-400">{error}</p>
  {/if}

  {#if result}
    <div class="rounded border border-border p-3 space-y-2">
      <div class="flex items-center gap-2">
        <span class="text-sm font-medium">{result.tsCode}</span>
        <span class="rounded bg-secondary px-2 py-0.5 text-xs font-medium">{result.regime}</span>
      </div>
      <p class="text-sm">{result.brief}</p>
      {#if result.metrics && Object.keys(result.metrics).length > 0}
        <table class="w-full text-xs">
          <tbody>
            {#each Object.entries(result.metrics) as [key, val]}
              <tr class="border-t border-border/30">
                <td class="py-1 pr-3 text-muted-foreground">{key}</td>
                <td class="py-1">{typeof val === 'number' ? val.toFixed(2) : String(val)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
      <p class="text-xs text-muted-foreground">{t('invest_system_regime_computed')}: {result.computedAt}</p>
    </div>
  {/if}
</div>
