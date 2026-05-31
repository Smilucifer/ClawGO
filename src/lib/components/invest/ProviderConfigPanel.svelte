<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import {
    investCommitteeStore,
    type InvestLlmConfig,
    type InvestLlmProviderConfig,
  } from '$lib/stores/invest-committee-store.svelte';

  const DEBATE_ROUND_OPTIONS = [1, 2, 3, 4, 6, 8];

  let expanded = $state(false);
  let saving = $state(false);

  const config = $derived(investCommitteeStore.llmConfig);

  const selectedProvider = $derived(
    config?.providers.find((p) => p.providerId === (config?.selectedProvider ?? 'deepseek'))
      ?? config?.providers[0]
      ?? null
  );

  function handleProviderChange(e: Event) {
    const target = e.target as HTMLSelectElement;
    if (!config) return;
    config.selectedProvider = target.value;
    scheduleSave();
  }

  function handleApiKeyInput(e: Event) {
    const target = e.target as HTMLInputElement;
    if (!selectedProvider) return;
    selectedProvider.apiKey = target.value;
    scheduleSave();
  }

  function handleBaseUrlInput(e: Event) {
    const target = e.target as HTMLInputElement;
    if (!selectedProvider) return;
    selectedProvider.baseUrl = target.value;
    scheduleSave();
  }

  function handleModelInput(e: Event) {
    const target = e.target as HTMLInputElement;
    if (!selectedProvider) return;
    selectedProvider.defaultModel = target.value;
    scheduleSave();
  }

  function handleRoundsChange(e: Event) {
    const target = e.target as HTMLSelectElement;
    if (!config) return;
    config.debateRounds = Number(target.value);
    scheduleSave();
  }

  function handleBufferInput(e: Event) {
    const target = e.target as HTMLInputElement;
    if (!config) return;
    config.emergencyBufferCny = Number(target.value) || 0;
    scheduleSave();
  }

  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  function scheduleSave() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      if (!config) return;
      saving = true;
      try {
        await investCommitteeStore.saveConfig({ ...config });
      } catch (e) {
        console.error('Auto-save failed:', e);
      } finally {
        saving = false;
      }
    }, 500);
  }

  let configLoadAttempted = $state(false);

  $effect(() => {
    let mounted = true;
    if (!investCommitteeStore.llmConfig && !investCommitteeStore.configLoading && !configLoadAttempted) {
      configLoadAttempted = true;
      investCommitteeStore.loadConfig().then(() => {
        if (mounted && investCommitteeStore.llmConfig) configLoadAttempted = false;
      });
    }
    return () => {
      mounted = false;
      if (saveTimer) clearTimeout(saveTimer);
    };
  });
</script>

<div class="mb-4 rounded-lg border border-border">
  <button
    class="flex w-full items-center justify-between px-4 py-2.5 text-sm font-medium hover:bg-muted/50"
    onclick={() => (expanded = !expanded)}
  >
    <span class="flex items-center gap-2">
      {t('invest_committee_llm_config')}
      {#if saving}
        <span class="text-xs text-muted-foreground">saving...</span>
      {/if}
    </span>
    <span class="text-muted-foreground">{expanded ? '▲' : '▼'}</span>
  </button>

  {#if expanded}
    <div class="border-t border-border px-4 py-3">
      {#if investCommitteeStore.configLoading}
        <div class="text-sm text-muted-foreground">Loading...</div>
      {:else if config}
        <!-- Provider selector -->
        <div class="mb-3">
          <label class="mb-1 block text-xs text-muted-foreground">
            {t('invest_committee_provider')}
          </label>
          <select
            class="w-full rounded border border-border bg-background px-2 py-1.5 text-sm"
            value={config.selectedProvider}
            onchange={handleProviderChange}
          >
            {#each config.providers as provider (provider.providerId)}
              <option value={provider.providerId}>{provider.providerId}</option>
            {/each}
          </select>
        </div>

        <!-- Selected provider settings -->
        {#if selectedProvider}
          <div class="rounded border border-border p-3">
            <div class="mb-2 text-xs font-medium uppercase text-muted-foreground">
              {selectedProvider.providerId}
            </div>
            <label class="mb-1 block text-xs text-muted-foreground">
              {t('invest_committee_api_key')}
            </label>
            <input
              type="password"
              class="mb-2 w-full rounded border border-border bg-background px-2 py-1 text-sm"
              value={selectedProvider.apiKey}
              oninput={handleApiKeyInput}
            />

            <label class="mb-1 block text-xs text-muted-foreground">
              {t('invest_committee_base_url')}
            </label>
            <input
              type="text"
              class="mb-2 w-full rounded border border-border bg-background px-2 py-1 text-sm"
              value={selectedProvider.baseUrl}
              oninput={handleBaseUrlInput}
            />

            <label class="mb-1 block text-xs text-muted-foreground">
              {t('invest_committee_model')}
            </label>
            <input
              type="text"
              class="w-full rounded border border-border bg-background px-2 py-1 text-sm"
              value={selectedProvider.defaultModel}
              oninput={handleModelInput}
            />
          </div>
        {/if}

        <!-- Global settings -->
        <div class="mt-3 flex flex-wrap items-center gap-4">
          <div>
            <label class="mr-1 text-xs text-muted-foreground">
              {t('invest_committee_debate_rounds')}
            </label>
            <select
              class="rounded border border-border bg-background px-2 py-1 text-sm"
              value={config.debateRounds}
              onchange={handleRoundsChange}
            >
              {#each DEBATE_ROUND_OPTIONS as opt}
                <option value={opt}>{opt}</option>
              {/each}
            </select>
          </div>

          <div>
            <label class="mr-1 text-xs text-muted-foreground">
              {t('invest_committee_emergency_buffer')}
            </label>
            <input
              type="number"
              class="w-28 rounded border border-border bg-background px-2 py-1 text-sm"
              value={config.emergencyBufferCny}
              min={0}
              step={10000}
              oninput={handleBufferInput}
            />
          </div>
        </div>
      {:else}
        <div class="text-sm text-muted-foreground">
          {t('invest_committee_no_config')}
        </div>
      {/if}
    </div>
  {/if}
</div>
