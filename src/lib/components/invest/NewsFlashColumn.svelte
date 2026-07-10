<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { investStore } from "$lib/stores/invest-store.svelte";
  import type { InvestEvent } from "$lib/types";
  import type { MessageKey } from "$lib/i18n/types";
  import {
    severityBadgeBg,
    stanceColor,
    severityLabel,
    stanceLabel,
    formatRelativeTime,
    splitCsv,
  } from "./news-helpers";

  type FlashFilter = "all" | "high" | "bull" | "bear";

  let { onTrigger }: { onTrigger: (event: InvestEvent) => void } = $props();

  let filter = $state<FlashFilter>("all");

  // Left column is jinshi_flash only — always source-scoped, then user filter on top.
  const flashEvents = $derived(
    investStore.events.filter((e) => e.source === "jinshi_flash"),
  );

  const filtered = $derived(
    flashEvents.filter((e) => {
      if (filter === "high") return e.severity === "high";
      if (filter === "bull") return e.stance === "bullish";
      if (filter === "bear") return e.stance === "bearish";
      return true;
    }),
  );

  const chips: { id: FlashFilter; labelKey: MessageKey }[] = [
    { id: "all", labelKey: "invest.news.filterAll" },
    { id: "high", labelKey: "invest.news.filterHighOnly" },
    { id: "bull", labelKey: "invest.news.filterBull" },
    { id: "bear", labelKey: "invest.news.filterBear" },
  ];
</script>

<section class="flash-col">
  <header class="col-header">
    <h3 class="col-title">{t("invest.news.flashTitle")}</h3>
    <div class="chips" role="tablist">
      {#each chips as chip}
        <button
          type="button"
          role="tab"
          aria-selected={filter === chip.id}
          class="chip"
          class:chip-active={filter === chip.id}
          onclick={() => (filter = chip.id)}
        >
          {t(chip.labelKey)}
        </button>
      {/each}
    </div>
  </header>

  <div class="scroll">
    {#if filtered.length === 0}
      <div class="empty">{t("invest.news.flashEmpty")}</div>
    {:else}
      <ul class="rows">
        {#each filtered as event (event.id)}
          {@const sectors = splitCsv(event.sectors)}
          <li class="row">
            <div class="row-head">
              <span
                class="sev-badge"
                style="background: {severityBadgeBg(event.severity)};"
              >
                {severityLabel(event.severity, t)}
              </span>
              <span
                class="stance"
                style="color: {stanceColor(event.stance)};"
              >
                {stanceLabel(event.stance, t)}
              </span>
              <span class="ts">{formatRelativeTime(event.createdAt)}</span>
            </div>
            <div class="body">
              {event.body ?? event.title}
            </div>
            {#if sectors.length > 0}
              <div class="sectors">
                {#each sectors as sec}
                  <span class="sector-chip">{sec}</span>
                {/each}
              </div>
            {/if}
            {#if event.severity === "high" && !event.triggered}
              <div class="actions">
                <button
                  type="button"
                  class="trigger-btn"
                  onclick={() => onTrigger(event)}
                >
                  {t("invest.eventWatch.triggerCommittee")}
                </button>
              </div>
            {:else if event.triggered}
              <div class="triggered-note">
                {t("invest.eventWatch.triggered")}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

<style>
  .flash-col {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-input);
    border-radius: var(--radius-md);
  }
  .col-header {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--bg-hover);
  }
  .col-title {
    font-size: 0.95rem;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .chip {
    border: 1px solid transparent;
    background: var(--bg-hover);
    color: var(--text-secondary);
    padding: 2px var(--space-3);
    border-radius: var(--radius-full);
    font-size: 0.75rem;
    cursor: pointer;
    line-height: 1.4;
  }
  .chip:hover {
    color: var(--text-primary);
  }
  .chip-active {
    background: var(--accent);
    color: #fff;
  }
  .scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-3) var(--space-4);
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .row {
    padding: var(--space-3);
    background: var(--bg-hover);
    border-radius: var(--radius-md);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .row-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: 0.72rem;
  }
  .sev-badge {
    padding: 1px var(--space-2);
    border-radius: var(--radius-full);
    color: var(--text-primary);
    font-weight: 600;
  }
  .stance {
    font-weight: 600;
  }
  .ts {
    margin-left: auto;
    color: var(--text-tertiary);
  }
  .body {
    font-size: 0.85rem;
    line-height: 1.5;
    color: var(--text-primary);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .sectors {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .sector-chip {
    font-size: 0.7rem;
    padding: 1px var(--space-2);
    border-radius: var(--radius-full);
    background: var(--bg-input);
    color: var(--text-secondary);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
  }
  .trigger-btn {
    font-size: 0.75rem;
    padding: 2px var(--space-3);
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: var(--radius-full);
    cursor: pointer;
  }
  .trigger-btn:hover {
    filter: brightness(1.1);
  }
  .triggered-note {
    font-size: 0.72rem;
    color: var(--text-tertiary);
    text-align: right;
  }
  .empty {
    color: var(--text-tertiary);
    font-size: 0.85rem;
    text-align: center;
    padding: var(--space-6) 0;
  }
</style>
