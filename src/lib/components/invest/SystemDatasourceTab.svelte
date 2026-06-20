<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';

  interface DataSourceStatus {
    name: string;
    ok: boolean;
    lastSuccess: string | null;
    sampleValue: string | null;
  }

  let sources: DataSourceStatus[] = $state([]);
  let loading = $state(true);
  let error = $state('');

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  async function load() {
    loading = true;
    error = '';
    try {
      sources = await invoke<DataSourceStatus[]>('get_datasource_health');
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    load();
    const interval = setInterval(load, 60_000);
    return () => clearInterval(interval);
  });
</script>

<div class="space-y-3">
  <h3 class="text-[13px] font-medium text-[var(--text-primary)]">{t('invest_system_datasource_title')}</h3>

  {#if loading && sources.length === 0}
    <p class="text-[13px] text-[var(--text-secondary)]">{t('invest_loading')}</p>
  {:else if error}
    <p class="text-[13px] text-[var(--color-error)]">{error}</p>
  {:else}
    <div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] overflow-hidden">
      <table class="w-full text-[13px]">
        <thead>
          <tr class="border-b border-border text-left">
            <th class="pb-2 pr-3 text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_system_ds_name')}</th>
            <th class="pb-2 pr-3 text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_system_ds_status')}</th>
            <th class="pb-2 pr-3 text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_system_ds_last_success')}</th>
            <th class="pb-2 text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">{t('invest_system_ds_sample')}</th>
          </tr>
        </thead>
        <tbody>
          {#each sources as src}
            <tr class="border-b border-border last:border-b-0 hover:bg-[var(--bg-hover)]">
              <td class="py-1.5 pr-3 text-[var(--text-primary)]">{src.name}</td>
              <td class="py-1.5 pr-3">
                <span
                  class="inline-block h-2 w-2 rounded-full"
                  class:bg-[var(--color-success)]={src.ok}
                  class:bg-[var(--color-error)]={!src.ok}
                ></span>
                <span class="ml-1 text-[12px] text-[var(--text-secondary)]">{src.ok ? 'OK' : 'DOWN'}</span>
              </td>
              <td class="py-1.5 pr-3 text-[12px] text-[var(--text-secondary)] font-[var(--font-mono)]">
                {src.lastSuccess ?? '-'}
              </td>
              <td class="py-1.5 text-[12px] text-[var(--text-secondary)]">
                {src.sampleValue ?? '-'}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>
