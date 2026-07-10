<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import type { MessageKey } from "$lib/i18n/types";
  import { investStore } from "$lib/stores/invest-store.svelte";
  import {
    stanceColor,
    stanceLabel,
    formatRelativeTime,
  } from "./news-helpers";

  type DigestFilter = "all" | "anns" | "stock" | "xueqiu";
  type DigestKind = "anns" | "stock" | "xueqiu";

  interface DigestRow {
    id: string;
    kind: DigestKind;
    title: string;
    summary: string | null;
    stance: string;
    symbol: string | null;
    ts: string;
  }

  let filter = $state<DigestFilter>("all");

  const annsRows = $derived<DigestRow[]>(
    investStore.events
      .filter((e) => e.source === "tushare_anns_d")
      .map((e) => ({
        id: `anns:${e.id}`,
        kind: "anns" as const,
        title: e.title,
        summary: e.body,
        stance: e.stance,
        symbol: e.symbols,
        ts: e.createdAt,
      })),
  );

  const stockRows = $derived<DigestRow[]>(
    investStore.events
      .filter((e) => e.source.startsWith("akshare:"))
      .map((e) => ({
        id: `stock:${e.id}`,
        kind: "stock" as const,
        title: e.title,
        summary: e.body,
        stance: e.stance,
        symbol: e.symbols,
        ts: e.createdAt,
      })),
  );

  const xueqiuRows = $derived<DigestRow[]>(
    investStore.sentimentItems.map((s) => ({
      id: `xueqiu:${s.id}`,
      kind: "xueqiu" as const,
      title: s.title,
      summary: s.summary,
      stance: s.stance,
      symbol: s.symbol ?? s.affectedSymbols,
      ts: s.publishedAt ?? s.createdAt,
    })),
  );

  const merged = $derived<DigestRow[]>(
    [...annsRows, ...stockRows, ...xueqiuRows].sort((a, b) =>
      a.ts < b.ts ? 1 : a.ts > b.ts ? -1 : 0,
    ),
  );

  const filtered = $derived(
    merged.filter((r) => {
      if (filter === "anns") return r.kind === "anns";
      if (filter === "stock") return r.kind === "stock";
      if (filter === "xueqiu") return r.kind === "xueqiu";
      return true;
    }),
  );

  const chips: { id: DigestFilter; labelKey: MessageKey }[] = [
    { id: "all", labelKey: "invest.news.filterAll" },
    { id: "anns", labelKey: "invest.news.filterAnns" },
    { id: "stock", labelKey: "invest.news.filterStock" },
    { id: "xueqiu", labelKey: "invest.news.filterXueqiu" },
  ];

  function sourceTagLabel(kind: DigestKind): string {
    switch (kind) {
      case "anns":
        return t("invest.news.srcAnns");
      case "stock":
        return t("invest.news.srcStock");
      case "xueqiu":
        return t("invest.news.srcXueqiu");
    }
  }

  function sourceTagClass(kind: DigestKind): string {
    return `src-tag src-${kind}`;
  }
</script>

<section class="digest-col">
  <header class="col-header">
    <h3 class="col-title">{t("invest.news.digestTitle")}</h3>
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
      <div class="empty">{t("invest.news.digestEmpty")}</div>
    {:else}
      <ul class="rows">
        {#each filtered as row (row.id)}
          <li class="row">
            <div class="row-head">
              <span class={sourceTagClass(row.kind)}>
                {sourceTagLabel(row.kind)}
              </span>
              <span
                class="stance"
                style="color: {stanceColor(row.stance)};"
              >
                {stanceLabel(row.stance, t)}
              </span>
              {#if row.symbol}
                <span class="sym">{row.symbol}</span>
              {/if}
              <span class="ts">{formatRelativeTime(row.ts)}</span>
            </div>
            <div class="title">{row.title}</div>
            {#if row.summary}
              <div class="summary">{row.summary}</div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

<style>
  .digest-col {
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
    gap: 4px;
  }
  .row-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: 0.72rem;
  }
  .src-tag {
    padding: 1px var(--space-2);
    border-radius: var(--radius-full);
    font-weight: 600;
    font-size: 0.7rem;
  }
  .src-anns {
    background: var(--accent);
    color: #fff;
  }
  .src-stock {
    background: var(--bg-input);
    color: var(--text-secondary);
  }
  .src-xueqiu {
    background: #7c94a8;
    color: #fff;
  }
  .stance {
    font-weight: 600;
  }
  .sym {
    color: var(--text-secondary);
    font-family: var(--font-mono, monospace);
  }
  .ts {
    margin-left: auto;
    color: var(--text-tertiary);
  }
  .title {
    font-size: 0.85rem;
    line-height: 1.4;
    color: var(--text-primary);
    font-weight: 500;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .summary {
    font-size: 0.78rem;
    line-height: 1.4;
    color: var(--text-tertiary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .empty {
    color: var(--text-tertiary);
    font-size: 0.85rem;
    text-align: center;
    padding: var(--space-6) 0;
  }
</style>
