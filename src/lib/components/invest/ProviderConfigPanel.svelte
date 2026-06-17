<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { getUserSettings } from '$lib/api';
  import {
    investCommitteeStore,
    type InvestLlmConfig,
  } from '$lib/stores/invest-committee-store.svelte';

  const DEBATE_ROUND_OPTIONS = [1, 2, 3, 4, 6, 8];
  const CONCURRENCY_OPTIONS = [1, 2, 3, 5, 8, 10];

  let expanded = $state(false);
  let saving = $state(false);

  /** Platform credentials from App settings */
  let platformCredentials = $state<Array<{ platform_id: string; name?: string }>>([]);
  let credentialsLoaded = $state(false);

  const config = $derived(investCommitteeStore.llmConfig);

  async function loadCredentials() {
    if (credentialsLoaded) return;
    try {
      const settings = await getUserSettings();
      platformCredentials = (settings.platform_credentials ?? []).map((c) => ({
        platform_id: c.platform_id,
        name: c.name,
      }));
    } catch (e) {
      console.error('Failed to load platform credentials:', e);
    } finally {
      credentialsLoaded = true;
    }
  }

  function handleProviderChange(e: Event) {
    const target = e.target as HTMLSelectElement;
    if (!config) return;
    config.selectedProvider = target.value;
    scheduleSave();
  }

  function handleRoundsChange(e: Event) {
    const target = e.target as HTMLSelectElement;
    if (!config) return;
    config.debateRounds = Number(target.value);
    scheduleSave();
  }

  function handleConcurrencyChange(e: Event) {
    const target = e.target as HTMLSelectElement;
    if (!config) return;
    config.maxConcurrentSymbols = Number(target.value);
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
    // Load platform credentials on first expand
    if (expanded && !credentialsLoaded) {
      loadCredentials();
    }
    return () => {
      mounted = false;
      if (saveTimer) clearTimeout(saveTimer);
    };
  });
</script>

<div class="mb-4 rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)]">
  <button
    class="flex w-full items-center justify-between px-[var(--space-4)] py-[var(--space-2)] text-[13px] font-medium text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
    onclick={() => {
      expanded = !expanded;
      if (expanded) loadCredentials();
    }}
  >
    <span class="flex items-center gap-2">
      {t('invest_committee_llm_config')}
      {#if saving}
        <span class="text-[11px] text-[var(--text-tertiary)]">saving...</span>
      {/if}
    </span>
    <span class="text-[var(--text-tertiary)]">{expanded ? '▲' : '▼'}</span>
  </button>

  {#if expanded}
    <div class="border-t border-border px-[var(--space-4)] py-[var(--space-3)]">
      {#if investCommitteeStore.configLoading}
        <div class="text-[13px] text-[var(--text-secondary)]">Loading...</div>
      {:else if config}
        <!-- Provider selector (from App settings → Connection) -->
        <div class="mb-3">
          <label class="mb-1 block text-[11px] text-[var(--text-tertiary)]">
            {t('invest_committee_provider')}
          </label>
          <select
            class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-[var(--space-2)] py-[var(--space-1)] text-[13px] text-[var(--text-primary)]"
            value={config.selectedProvider}
            onchange={handleProviderChange}
          >
            <option value="default">{t('invest_committee_provider_default')}</option>
            {#each platformCredentials as cred (cred.platform_id)}
              <option value={cred.platform_id}>{cred.name ?? cred.platform_id}</option>
            {/each}
          </select>
          <p class="mt-1 text-[11px] text-[var(--text-tertiary)]">
            {t('invest_committee_provider_hint')}
          </p>
        </div>

        <!-- Global settings -->
        <div class="mt-[var(--space-3)] flex flex-wrap items-center gap-[var(--space-4)]">
          <div>
            <label class="mr-1 text-[11px] text-[var(--text-tertiary)]">
              {t('invest_committee_debate_rounds')}
            </label>
            <select
              class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-[var(--space-2)] py-[var(--space-1)] text-[13px] text-[var(--text-primary)]"
              value={config.debateRounds}
              onchange={handleRoundsChange}
            >
              {#each DEBATE_ROUND_OPTIONS as opt}
                <option value={opt}>{opt}</option>
              {/each}
            </select>
          </div>
          <div>
            <label class="mr-1 text-[11px] text-[var(--text-tertiary)]">
              {t('invest_committee_concurrency')}
            </label>
            <select
              class="rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-[var(--space-2)] py-[var(--space-1)] text-[13px] text-[var(--text-primary)]"
              value={config.maxConcurrentSymbols ?? 5}
              onchange={handleConcurrencyChange}
            >
              {#each CONCURRENCY_OPTIONS as opt}
                <option value={opt}>{opt}</option>
              {/each}
            </select>
          </div>
        </div>
      {:else}
        <div class="text-[13px] text-[var(--text-secondary)]">
          {t('invest_committee_no_config')}
        </div>
      {/if}
    </div>
  {/if}
</div>
