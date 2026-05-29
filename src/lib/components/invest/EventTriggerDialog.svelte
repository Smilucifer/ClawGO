<script lang="ts">
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import type { InvestEvent } from '$lib/types';

  let { event, onClose, onTriggered }: {
    event: InvestEvent;
    onClose: () => void;
    onTriggered: () => void;
  } = $props();

  let debateRounds = $state(4);
  let confirming = $state(false);
  let errorMsg = $state<string | null>(null);
  const DEBATE_OPTIONS = [1, 2, 3, 4, 6, 8];

  const symbolList = $derived(
    event.symbols ? event.symbols.split(',').map(s => s.trim()).filter(Boolean) : []
  );

  async function handleConfirm() {
    if (symbolList.length === 0 || confirming) return;

    confirming = true;
    errorMsg = null;
    try {
      // Mark event as triggered
      await investStore.triggerCommittee(event.id, null);

      // Notify parent (switches tab)
      onTriggered();

      // Start committee run
      await investCommitteeStore.runCommittee(symbolList, debateRounds);
    } catch (e) {
      errorMsg = t('invest.eventWatch.triggerError');
    } finally {
      confirming = false;
    }
  }

  function handleBackdropKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center"
  role="dialog"
  tabindex="-1"
  aria-modal="true"
  aria-label={t('invest.eventWatch.triggerDialogTitle')}
  onclick={onClose}
  onkeydown={handleBackdropKeydown}
>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="bg-zinc-900 border border-zinc-700 rounded-lg p-6 max-w-md w-full mx-4 shadow-lg"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <h3 class="text-lg font-semibold mb-4 text-zinc-100">
      {t('invest.eventWatch.triggerDialogTitle')}
    </h3>

    <div class="space-y-3 text-sm">
      <div class="text-zinc-400">{t('invest.eventWatch.triggerDialogDetected')}:</div>
      <div class="p-2 bg-zinc-800 rounded text-zinc-200">
        {event.body || event.title}
      </div>

      <div class="flex gap-2 text-xs">
        <span class="text-red-400">{t('invest.eventWatch.severityLabel')}: {event.severity.toUpperCase()}</span>
        <span class="text-zinc-600">|</span>
        <span class="text-zinc-300">{t('invest.eventWatch.stanceLabel')}: {event.stance}</span>
      </div>

      {#if symbolList.length > 0}
        <div class="text-xs">
          <span class="text-zinc-400">{t('invest.eventWatch.triggerDialogHoldings')}:</span>
          <span class="text-zinc-200">{symbolList.join(', ')}</span>
        </div>
      {/if}

      <div class="flex items-center gap-2">
        <span class="text-zinc-400 text-xs">{t('invest.eventWatch.triggerDialogRounds')}:</span>
        <select bind:value={debateRounds} class="px-2 py-1 rounded border border-zinc-700 bg-zinc-800 text-sm text-zinc-200">
          {#each DEBATE_OPTIONS as opt (opt)}
            <option value={opt}>{opt}</option>
          {/each}
        </select>
      </div>

      <div class="text-zinc-400 text-xs">
        {t('invest.eventWatch.triggerDialogConfirm')}
      </div>
    </div>

    {#if errorMsg}
      <div class="mt-4 px-3 py-2 rounded bg-red-500/10 border border-red-500/30 text-sm text-red-400">
        {errorMsg}
      </div>
    {/if}

    <div class="flex justify-end gap-3 mt-6">
      <button
        class="px-4 py-2 text-sm rounded-md border border-zinc-700 text-zinc-300 hover:bg-zinc-800"
        onclick={onClose}
      >{t('invest.eventWatch.triggerDialogCancel')}</button>
      <button
        class="px-4 py-2 text-sm rounded-md bg-amber-600 text-white hover:bg-amber-700 disabled:opacity-50"
        onclick={handleConfirm}
        disabled={symbolList.length === 0 || confirming}
      >{confirming ? t('invest_committee_running') : t('invest.eventWatch.triggerDialogConfirmBtn')}</button>
    </div>
  </div>
</div>
