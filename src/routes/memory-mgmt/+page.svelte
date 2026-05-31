<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { getTransport } from "$lib/transport";
  import { goto } from "$app/navigation";

  type MemTab = "userMemory" | "archived";
  let activeTab: MemTab = $state("userMemory");

  const tabs: { id: MemTab; label: string }[] = $derived([
    { id: "userMemory", label: t("memoryMgmt_tab_userMemory") },
    { id: "archived", label: "Archived" },
  ]);

  let scopeFilter: string | null = $state(null);
  const scopeOptions = [null, "global", "project", "invest"];

  let memories: any[] = $state([]);
  let loading = $state(false);
  let loadVersion = 0;

  async function loadMemories() {
    const version = ++loadVersion;
    loading = true;
    try {
      const result = await getTransport().invoke<any[]>("list_memories", {
        scope_filter: scopeFilter,
      });
      if (version === loadVersion) {
        memories = result;
      }
    } catch (e) {
      console.error("Failed to load memories:", e);
    } finally {
      if (version === loadVersion) {
        loading = false;
      }
    }
  }

  $effect(() => {
    void scopeFilter;
    loadMemories();
  });

  // ── Archived insights ──────────────────────────────────────────────

  let archivedInsights: any[] = $state([]);
  let archivedLoading = $state(false);
  let archivedLoadVersion = 0;

  async function loadArchivedInsights() {
    const version = ++archivedLoadVersion;
    archivedLoading = true;
    try {
      const result = await getTransport().invoke<any[]>("list_insights", {
        status: "archived",
        limit: 50,
      });
      if (version === archivedLoadVersion) {
        archivedInsights = result;
      }
    } catch (e) {
      console.error("Failed to load archived insights:", e);
    } finally {
      if (version === archivedLoadVersion) {
        archivedLoading = false;
      }
    }
  }

  let restoringId: string | null = $state(null);

  async function restoreInsight(id: string) {
    restoringId = id;
    try {
      await getTransport().invoke("unarchive_insight", { id });
      await loadArchivedInsights();
    } catch (e) {
      console.error("Failed to restore insight:", e);
    } finally {
      restoringId = null;
    }
  }

  $effect(() => {
    if (activeTab === "archived") {
      loadArchivedInsights();
    }
  });

</script>

<div class="flex h-full flex-col">
  <div class="border-b border-border px-4 pt-3">
    <h1 class="mb-3 text-lg font-semibold">{t("nav_memoryMgmt")}</h1>
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

  <div class="border-b border-border bg-muted/50 px-4 py-2">
    <p class="text-xs text-muted-foreground">
      提取配置（API Endpoint / Key / Model）已移至
      <button
        class="underline hover:text-foreground"
        onclick={() => goto("/settings?tab=memory")}
      >设置 > Memory Extraction</button>
      统一管理。记忆衰减与归档开关也位于该页面。
    </p>
  </div>

  <div class="flex-1 overflow-auto p-4">
    {#if activeTab === "userMemory"}
      <div class="mb-4 flex gap-2">
        {#each scopeOptions as scope}
          <button
            class="rounded-md px-2.5 py-1 text-xs transition-colors"
            class:bg-primary={scopeFilter === scope}
            class:text-primary-foreground={scopeFilter === scope}
            class:bg-muted={scopeFilter !== scope}
            class:text-muted-foreground={scopeFilter !== scope}
            onclick={() => (scopeFilter = scope)}
          >
            {scope ?? "all"}
          </button>
        {/each}
      </div>

      {#if loading}
        <div class="text-muted-foreground text-sm">Loading...</div>
      {:else if memories.length === 0}
        <div class="text-muted-foreground text-sm">No memories found</div>
      {:else}
        <div class="flex flex-col gap-2">
          {#each memories as mem}
            <div class="rounded-md border border-border p-3">
              <div class="mb-1 flex items-center gap-2">
                <span class="rounded bg-muted px-1.5 py-0.5 text-xs">{mem.scope}</span>
                <span class="rounded bg-muted px-1.5 py-0.5 text-xs">{mem.type}</span>
                {#if mem.confidence != null}
                  <span class="text-muted-foreground text-xs">confidence: {mem.confidence.toFixed(1)}</span>
                {/if}
              </div>
              <div class="text-sm">{mem.content}</div>
              <div class="text-muted-foreground mt-1 text-xs">Updated: {mem.updated_at}</div>
            </div>
          {/each}
        </div>
      {/if}

    {:else if activeTab === "archived"}
      {#if archivedLoading}
        <div class="text-muted-foreground text-sm">Loading...</div>
      {:else if archivedInsights.length === 0}
        <div class="text-muted-foreground text-sm">No archived insights</div>
      {:else}
        <div class="flex flex-col gap-2">
          {#each archivedInsights as insight}
            <div class="rounded-md border border-border p-3">
              <div class="mb-1 flex items-center gap-2">
                <span class="rounded bg-muted px-1.5 py-0.5 text-xs">{insight.insightType}</span>
                {#if insight.symbol}
                  <span class="rounded bg-muted px-1.5 py-0.5 text-xs">{insight.symbol}</span>
                {/if}
                <span class="text-muted-foreground text-xs">{insight.updatedAt}</span>
              </div>
              <div class="text-sm">{insight.content}</div>
              <div class="mt-2">
                <button
                  class="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={restoringId === insight.id}
                  onclick={() => restoreInsight(insight.id)}
                >
                  {restoringId === insight.id ? "Restoring..." : "Restore"}
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  </div>
</div>
