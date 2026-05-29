<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { getTransport } from "$lib/transport";

  type MemTab = "userMemory" | "extractionConfig" | "archived";
  let activeTab: MemTab = $state("userMemory");

  const tabs: { id: MemTab; label: string }[] = $derived([
    { id: "userMemory", label: t("memoryMgmt_tab_userMemory") },
    { id: "extractionConfig", label: t("memoryMgmt_tab_extractionConfig") },
    { id: "archived", label: "Archived" },
  ]);

  let scopeFilter: string | null = $state(null);
  const scopeOptions = [null, "global", "project", "invest"];

  let memories: any[] = $state([]);
  let loading = $state(false);
  let loadVersion = $state(0);

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
  let archivedLoadVersion = $state(0);

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

  let configDirty = $state(false);
  let extractEnabled = $state(true);
  let chatEndpoint = $state("");
  let chatApiKey = $state("");
  let chatModel = $state("");

  function handleApplyConfig() {
    // TODO: save config via Tauri command (Phase 2)
    configDirty = false;
  }
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

    {:else if activeTab === "extractionConfig"}
      <div class="max-w-lg">
        <div class="mb-4">
          <label class="flex items-center gap-2">
            <input type="checkbox" bind:checked={extractEnabled} />
            <span class="text-sm">Enable auto extraction</span>
          </label>
        </div>

        <div class="mb-3">
          <label class="text-muted-foreground mb-1 block text-xs">Chat API Endpoint</label>
          <input
            class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm"
            bind:value={chatEndpoint}
            oninput={() => (configDirty = true)}
          />
        </div>

        <div class="mb-3">
          <label class="text-muted-foreground mb-1 block text-xs">Chat API Key</label>
          <input
            type="password"
            class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm"
            bind:value={chatApiKey}
            oninput={() => (configDirty = true)}
          />
        </div>

        <div class="mb-4">
          <label class="text-muted-foreground mb-1 block text-xs">Chat Model</label>
          <input
            class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-sm"
            bind:value={chatModel}
            oninput={() => (configDirty = true)}
          />
        </div>

        <div class="flex items-center justify-between border-t border-border pt-3">
          <span class="text-muted-foreground text-xs">Click apply after config changes</span>
          <button
            class="rounded-md px-3 py-1.5 text-sm font-medium transition-colors"
            class:bg-green-600={configDirty}
            class:text-white={configDirty}
            class:bg-muted={!configDirty}
            class:text-muted-foreground={!configDirty}
            disabled={!configDirty}
            onclick={handleApplyConfig}
          >
            Apply & Reload
          </button>
        </div>
      </div>

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
