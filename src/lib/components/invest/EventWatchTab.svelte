<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { fmtRelative } from "$lib/i18n/format";
  import { investStore } from "$lib/stores/invest-store.svelte";
  import type { InvestEvent } from "$lib/types";
  import NewsFlashColumn from "./NewsFlashColumn.svelte";
  import NewsDigestColumn from "./NewsDigestColumn.svelte";
  import EventTriggerDialog from "./EventTriggerDialog.svelte";

  let { onNavigateToCommittee }: { onNavigateToCommittee?: () => void } =
    $props();

  let triggerTarget = $state<InvestEvent | null>(null);

  onMount(() => {
    investStore.fetchEvents();
    investStore.fetchScanStatus();
    investStore.fetchSentimentItems();
  });

  // `triggerScan()` is the existing store method; it internally re-fetches events
  // + scan status. We additionally refresh sentiment items so the right column updates.
  async function runScan(): Promise<void> {
    await investStore.triggerScan();
    await investStore.fetchSentimentItems();
  }

  function openTrigger(event: InvestEvent): void {
    triggerTarget = event;
  }

  function closeTrigger(): void {
    triggerTarget = null;
  }

  function onTriggerSuccess(): void {
    triggerTarget = null;
    onNavigateToCommittee?.();
  }
</script>

<div class="news-tab">
  <header class="status-bar">
    <div class="status-summary">
      {#if investStore.scanStatus}
        <span class="stat">
          <span class="stat-label">{t("invest.eventWatch.events")}</span>
          <span class="stat-value">{investStore.scanStatus.totalEvents}</span>
        </span>
        <span class="stat">
          <span class="stat-label">{t("invest.eventWatch.high")}</span>
          <span class="stat-value">{investStore.scanStatus.highCount}</span>
        </span>
        <span class="stat">
          <span class="stat-label">{t("invest.eventWatch.untriggered")}</span>
          <span class="stat-value"
            >{investStore.scanStatus.untriggeredHigh}</span
          >
        </span>
        {#if investStore.scanStatus.lastEventAt}
          <span class="stat stat-muted">
            {t("invest.eventWatch.last")}
            {fmtRelative(investStore.scanStatus.lastEventAt)}
          </span>
        {/if}
      {:else}
        <span class="stat stat-muted"
          >{t("invest.eventWatch.noScanData")}</span
        >
      {/if}
    </div>
    <div class="status-actions">
      <button
        type="button"
        class="scan-btn"
        disabled={investStore.isScanning}
        onclick={runScan}
      >
        {investStore.isScanning
          ? t("invest.eventWatch.scanning")
          : t("invest.eventWatch.scanNow")}
      </button>
    </div>
  </header>

  <div class="two-col">
    <NewsFlashColumn onTrigger={openTrigger} />
    <NewsDigestColumn />
  </div>

  {#if triggerTarget}
    <EventTriggerDialog
      event={triggerTarget}
      onClose={closeTrigger}
      onTriggered={onTriggerSuccess}
    />
  {/if}
</div>

<style>
  .news-tab {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    height: 100%;
    min-height: 0;
    padding: var(--space-3) var(--space-4);
  }
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    background: var(--bg-input);
    border-radius: var(--radius-md);
    flex-wrap: wrap;
  }
  .status-summary {
    display: flex;
    gap: var(--space-4);
    flex-wrap: wrap;
    align-items: center;
  }
  .stat {
    display: inline-flex;
    gap: 4px;
    font-size: 0.8rem;
    align-items: baseline;
  }
  .stat-label {
    color: var(--text-tertiary);
  }
  .stat-value {
    color: var(--text-primary);
    font-weight: 600;
  }
  .stat-muted {
    color: var(--text-tertiary);
  }
  .scan-btn {
    font-size: 0.8rem;
    padding: 4px var(--space-3);
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: var(--radius-full);
    cursor: pointer;
  }
  .scan-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .scan-btn:not(:disabled):hover {
    filter: brightness(1.1);
  }
  .two-col {
    flex: 1 1 auto;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
  }
  /* Below 900px viewport width, the columns stack and each keeps its own scroll region. */
  @media (max-width: 900px) {
    .two-col {
      grid-template-columns: 1fr;
      grid-auto-rows: minmax(0, 1fr);
    }
  }
</style>
