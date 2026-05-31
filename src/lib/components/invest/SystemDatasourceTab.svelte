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

  interface LlmProviderInfo {
    providerId: string;
    hasKey: boolean;
    model: string;
  }

  let sources: DataSourceStatus[] = $state([]);
  let llmProviders: LlmProviderInfo[] = $state([]);
  let loading = $state(true);
  let error = $state('');

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  async function load() {
    loading = true;
    error = '';
    try {
      sources = await invoke<DataSourceStatus[]>('get_datasource_health');
      // Fetch LLM config to show provider details
      try {
        const config = await invoke<{
          providers: { providerId: string; apiKey: string; defaultModel: string }[];
        }>('get_llm_config');
        llmProviders = (config.providers ?? []).map((p) => ({
          providerId: p.providerId,
          hasKey: !!p.apiKey,
          model: p.defaultModel,
        }));
      } catch {
        llmProviders = [];
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function providerStatusText(name: string): string | null {
    if (name !== 'LLM Config' || llmProviders.length === 0) return null;
    const configured = llmProviders.filter((p) => p.hasKey);
    if (configured.length === 0) return t('invest_system_ds_llm_none');
    return configured.map((p) => p.providerId).join(', ');
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
              {#if providerStatusText(src.name)}
                <span class="text-muted-foreground">{providerStatusText(src.name)}</span>
              {:else}
                {src.sampleValue ?? '-'}
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
