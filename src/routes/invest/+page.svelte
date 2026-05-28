<script lang="ts">
  import { t } from "$lib/i18n";

  type InvestTab = "dashboard" | "committee" | "strategy" | "trades" | "events" | "scheduler";
  let activeTab: InvestTab = $state("dashboard");

  const tabs: { id: InvestTab; label: string }[] = $derived([
    { id: "dashboard", label: t("invest_tab_dashboard") },
    { id: "committee", label: t("invest_tab_committee") },
    { id: "strategy", label: t("invest_tab_strategy") },
    { id: "trades", label: t("invest_tab_trades") },
    { id: "events", label: t("invest_tab_events") },
    { id: "scheduler", label: t("invest_tab_scheduler") },
  ]);
</script>

<div class="flex h-full flex-col">
  <div class="border-b border-border px-4 pt-3">
    <h1 class="mb-3 text-lg font-semibold">{t("nav_invest")}</h1>
    <div class="flex gap-1">
      {#each tabs as tab}
        <button
          class="rounded-t-md px-3 py-1.5 text-sm transition-colors"
          class:bg-primary={activeTab === tab.id}
          class:text-primary-foreground={activeTab === tab.id}
          class:text-muted-foreground={activeTab !== tab.id}
          class:hover:bg-muted={activeTab !== tab.id}
          onclick={() => (activeTab = tab.id)}
        >
          {tab.label}
        </button>
      {/each}
    </div>
  </div>

  <div class="flex-1 overflow-auto p-4">
    {#if activeTab === "dashboard"}
      <div class="text-muted-foreground">Dashboard — coming in Phase 2</div>
    {:else if activeTab === "committee"}
      <div class="text-muted-foreground">Committee — coming in Phase 3</div>
    {:else if activeTab === "strategy"}
      <div class="text-muted-foreground">Strategy — coming in Phase 2</div>
    {:else if activeTab === "trades"}
      <div class="text-muted-foreground">Trade Log — coming in Phase 2</div>
    {:else if activeTab === "events"}
      <div class="text-muted-foreground">Event Monitor — coming in Phase 3c</div>
    {:else if activeTab === "scheduler"}
      <div class="text-muted-foreground">Scheduled Tasks — coming in Phase 4</div>
    {/if}
  </div>
</div>
