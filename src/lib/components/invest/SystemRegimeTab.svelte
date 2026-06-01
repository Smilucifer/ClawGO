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

<div class="space-y-[var(--space-3)]">
  <h3 class="text-sm font-medium text-[var(--text-primary)]">{t('invest_system_regime_title')}</h3>
  <p class="text-xs text-[var(--text-secondary)]">{t('invest_system_regime_desc')}</p>

  <div class="flex gap-[var(--space-2)]">
    <input
      type="text"
      class="w-48 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-sm text-[var(--text-primary)]"
      placeholder={t('invest_system_regime_placeholder')}
      bind:value={symbol}
      onkeydown={(e) => e.key === 'Enter' && classify()}
    />
    <button
      class="rounded-[var(--radius-md)] bg-[var(--accent)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] font-medium text-[var(--bg-base)] disabled:opacity-50"
      onclick={classify}
      disabled={loading || !symbol.trim()}
    >
      {loading ? t('invest_loading') : t('invest_system_regime_classify')}
    </button>
  </div>

  {#if error}
    <p class="text-sm text-[var(--color-error)]">{error}</p>
  {/if}

  {#if result}
    <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] p-[var(--space-3)] space-y-[var(--space-2)]">
      <div class="flex items-center gap-[var(--space-2)]">
        <span class="text-sm font-medium text-[var(--text-primary)]">{result.tsCode}</span>
        <span class="rounded-[var(--radius-full)] bg-[var(--accent-muted)] px-3 py-1 text-[11px] font-bold text-[var(--accent)]">{result.regime}</span>
      </div>
      <p class="text-sm text-[var(--text-primary)]">{result.brief}</p>
      {#if result.metrics && Object.keys(result.metrics).length > 0}
        <table class="w-full text-xs">
          <thead>
            <tr>
              <th class="py-1 pr-3 text-left text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">Key</th>
              <th class="py-1 text-left text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">Value</th>
            </tr>
          </thead>
          <tbody>
            {#each Object.entries(result.metrics) as [key, val]}
              <tr class="border-t border-[var(--border)]">
                <td class="py-1 pr-3 text-[var(--text-secondary)]">{key}</td>
                <td class="py-1 font-[var(--font-mono)] text-[var(--text-primary)]">{typeof val === 'number' ? val.toFixed(2) : String(val)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
      <p class="text-xs text-[var(--text-tertiary)]">{t('invest_system_regime_computed')}: {result.computedAt}</p>
    </div>
  {/if}
</div>
