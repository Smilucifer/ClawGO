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
  <h3 class="text-sm font-medium">{t('invest_system_datasource_title')}</h3>

  {#if loading && sources.length === 0}
    <p class="text-sm text-muted-foreground">{t('invest_loading')}</p>
  {:else if error}
    <p class="text-sm text-red-400">{error}</p>
  {:else}
    <table class="w-full text-sm">
      <thead>
        <tr class="border-b border-border text-left text-muted-foreground">
          <th class="pb-2 pr-3">{t('invest_system_ds_name')}</th>
          <th class="pb-2 pr-3">{t('invest_system_ds_status')}</th>
          <th class="pb-2 pr-3">{t('invest_system_ds_last_success')}</th>
          <th class="pb-2">{t('invest_system_ds_sample')}</th>
        </tr>
      </thead>
      <tbody>
        {#each sources as src}
          <tr class="border-b border-border/50">
            <td class="py-1.5 pr-3">{src.name}</td>
            <td class="py-1.5 pr-3">
              <span
                class="inline-block h-2 w-2 rounded-full"
                class:bg-green-400={src.ok}
                class:bg-red-400={!src.ok}
              ></span>
              <span class="ml-1 text-xs">{src.ok ? 'OK' : 'DOWN'}</span>
            </td>
            <td class="py-1.5 pr-3 text-xs text-muted-foreground">
              {src.lastSuccess ?? '-'}
            </td>
            <td class="py-1.5 text-xs">
              {src.sampleValue ?? '-'}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
