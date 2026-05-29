<script lang="ts">
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { t } from '$lib/i18n/index.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import type { InvestEvent } from '$lib/types';

  let { event, onClose, onTriggered }: {
    event: InvestEvent;
    onClose: () => void;
    onTriggered: () => void;
  } = $props();

  let open = $state(true);
  let debateRounds = $state(4);
  let confirming = $state(false);
  let errorMsg = $state<string | null>(null);
  const DEBATE_OPTIONS = [1, 2, 3, 4, 6, 8];

  const symbolList = $derived(
    event.symbols ? event.symbols.split(',').map(s => s.trim()).filter(Boolean) : []
  );

  // Sync Modal close with parent
  $effect(() => {
    if (!open) onClose();
  });

  async function handleConfirm() {
    if (symbolList.length === 0 || confirming) return;

    confirming = true;
    errorMsg = null;
    try {
      // Start committee run first — only mark triggered on success
      await investCommitteeStore.runCommittee(symbolList, debateRounds);

      // Check if runCommittee failed (catches internally, never re-throws)
      if (investCommitteeStore.runError) {
        errorMsg = investCommitteeStore.runError;
        return;
      }

      // Mark event as triggered after committee starts successfully
      const marked = await investStore.triggerCommittee(event.id, null);
      if (!marked) {
        errorMsg = investStore.error || t('invest.eventWatch.triggerError');
        return;
      }

      // Notify parent (switches tab)
      onTriggered();
    } catch {
      errorMsg = t('invest.eventWatch.triggerError');
    } finally {
      confirming = false;
    }
  }
</script>

<Modal bind:open title={t('invest.eventWatch.triggerDialogTitle')}>
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
      onclick={() => { open = false; }}
    >{t('invest.eventWatch.triggerDialogCancel')}</button>
    <button
      class="px-4 py-2 text-sm rounded-md bg-amber-600 text-white hover:bg-amber-700 disabled:opacity-50"
      onclick={handleConfirm}
      disabled={symbolList.length === 0 || confirming}
    >{confirming ? t('invest_committee_running') : t('invest.eventWatch.triggerDialogConfirmBtn')}</button>
  </div>
</Modal>
