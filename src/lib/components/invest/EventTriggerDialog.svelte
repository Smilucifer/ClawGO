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

  function severityLabel(severity: string): string {
    switch (severity) {
      case 'high': return t('invest.eventWatch.filterHigh');
      case 'medium': return t('invest.eventWatch.filterMedium');
      case 'low': return t('invest.eventWatch.filterLow');
      default: return severity;
    }
  }

  function stanceLabel(stance: string): string {
    switch (stance) {
      case 'bullish': return t('invest.eventWatch.stanceBullish');
      case 'bearish': return t('invest.eventWatch.stanceBearish');
      case 'neutral': return t('invest.eventWatch.stanceNeutral');
      default: return stance;
    }
  }

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
  <div class="space-y-[var(--space-3)] text-[13px]">
    <div class="text-[var(--text-secondary)]">{t('invest.eventWatch.triggerDialogDetected')}:</div>
    <div class="rounded-[var(--radius-md)] bg-[var(--bg-input)] p-[var(--space-2)] text-[var(--text-primary)]">
      {event.body || event.title}
    </div>

    <div class="flex gap-2 text-[11px]">
      <span class="text-[var(--color-error)]">{t('invest.eventWatch.severityLabel')}: {severityLabel(event.severity)}</span>
      <span class="text-[var(--text-tertiary)]">|</span>
      <span class="text-[var(--text-secondary)]">{t('invest.eventWatch.stanceLabel')}: {stanceLabel(event.stance)}</span>
    </div>

    {#if symbolList.length > 0}
      <div class="text-[11px]">
        <span class="text-[var(--text-secondary)]">{t('invest.eventWatch.triggerDialogHoldings')}:</span>
        <span class="text-[var(--text-primary)] font-[var(--font-mono)]">{symbolList.join(', ')}</span>
      </div>
    {/if}

    <div class="flex items-center gap-2">
      <span class="text-[var(--text-secondary)] text-[11px]">{t('invest.eventWatch.triggerDialogRounds')}:</span>
      <select bind:value={debateRounds} class="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-2 py-1 text-[13px] text-[var(--text-primary)]">
        {#each DEBATE_OPTIONS as opt (opt)}
          <option value={opt}>{opt}</option>
        {/each}
      </select>
    </div>

    <div class="text-[var(--text-secondary)] text-[11px]">
      {t('invest.eventWatch.triggerDialogConfirm')}
    </div>
  </div>

  {#if errorMsg}
    <div class="mt-[var(--space-4)] rounded-[var(--radius-md)] border border-[rgba(168,122,122,0.3)] bg-[rgba(168,122,122,0.1)] px-[var(--space-3)] py-[var(--space-2)] text-[13px] text-[var(--color-error)]">
      {errorMsg}
    </div>
  {/if}

  <div class="mt-[var(--space-6)] flex justify-end gap-[var(--space-3)]">
    <button
      class="rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-card)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] transition-colors hover:bg-[var(--bg-hover)]"
      onclick={() => { open = false; }}
    >{t('invest.eventWatch.triggerDialogCancel')}</button>
    <button
      class="rounded-[var(--radius-md)] bg-[#c9a96e] px-[var(--space-3)] py-[var(--space-1)] text-[12px] text-white transition-colors hover:bg-[#b8985e] disabled:opacity-50"
      onclick={handleConfirm}
      disabled={symbolList.length === 0 || confirming}
    >{confirming ? t('invest_committee_running') : t('invest.eventWatch.triggerDialogConfirmBtn')}</button>
  </div>
</Modal>
