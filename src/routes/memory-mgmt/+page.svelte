<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { getTransport } from "$lib/transport";
  import * as api from "$lib/api";
  import Input from "$lib/components/Input.svelte";
  import { dbgWarn } from "$lib/utils/debug";
  import type { UserSettings } from "$lib/types";

  type MemTab = "userMemory" | "archived" | "extractionConfig";
  let activeTab: MemTab = $state("userMemory");

  const tabs: { id: MemTab; label: string }[] = $derived([
    { id: "userMemory", label: t("memoryMgmt_tab_userMemory") },
    { id: "archived", label: "Archived" },
    { id: "extractionConfig", label: t("memoryMgmt_tab_extractionConfig") },
  ]);

  let scopeFilter: string | null = $state(null);
  const scopeOptions = [null, "global", "project", "invest"];

  // ── Memory Extraction Config state ──
  let memoryExtractionEnabled = $state(true);
  let memoryExtractionChatEndpoint = $state("");
  let memoryExtractionChatModel = $state("");
  let memoryExtractionChatApiKey = $state("");
  let memoryExtractionShowKey = $state(false);
  let memoryExtractionSaveDebounce: ReturnType<typeof setTimeout> | null = null;
  let memoryDreamEnabled = $state(true);
  let settings = $state<UserSettings | null>(null);

  function loadMemoryExtractionConfig() {
    try {
      const s = JSON.parse(localStorage.getItem("clawgo-memory-extraction") ?? "{}");
      if (s.chat_endpoint) memoryExtractionChatEndpoint = s.chat_endpoint;
      if (s.chat_model) memoryExtractionChatModel = s.chat_model;
      if (s.chat_api_key) memoryExtractionChatApiKey = s.chat_api_key;
      if (s.enabled === false) memoryExtractionEnabled = false;
    } catch {
      // defaults are fine
    }
  }

  function saveMemoryExtractionConfig() {
    localStorage.setItem("clawgo-memory-extraction", JSON.stringify({
      enabled: memoryExtractionEnabled,
      chat_endpoint: memoryExtractionChatEndpoint.trim() || undefined,
      chat_model: memoryExtractionChatModel.trim() || undefined,
      chat_api_key: memoryExtractionChatApiKey.trim() || undefined,
    }));
  }

  function debouncedSaveMemoryExtraction() {
    if (memoryExtractionSaveDebounce) clearTimeout(memoryExtractionSaveDebounce);
    memoryExtractionSaveDebounce = setTimeout(() => saveMemoryExtractionConfig(), 500);
  }

  onMount(async () => {
    try {
      settings = await api.getUserSettings();
      memoryDreamEnabled = settings.memory_dream_enabled ?? true;
    } catch (e) {
      dbgWarn("memory-mgmt", "load settings failed", e);
    }
    loadMemoryExtractionConfig();
  });

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

    {:else if activeTab === "extractionConfig"}
      <div class="space-y-4 max-w-lg">
        <h3 class="text-sm font-semibold">Memory Extraction</h3>
        <p class="text-xs text-muted-foreground">
          配置从群聊对话中自动提取用户记忆的 LLM。使用 SQLite FTS5 全文检索，无需 Embedding API。
        </p>

        <!-- Enable/Disable -->
        <label class="flex items-center gap-3 cursor-pointer">
          <input
            type="checkbox"
            checked={memoryExtractionEnabled}
            onchange={(e) => {
              memoryExtractionEnabled = e.currentTarget.checked;
              saveMemoryExtractionConfig();
            }}
            class="h-4 w-4 rounded border-input"
          />
          <span class="text-sm">启用自动记忆提取</span>
        </label>

        {#if memoryExtractionEnabled}
          <!-- Chat Endpoint -->
          <Input
            label="Chat API Endpoint"
            type="text"
            bind:value={memoryExtractionChatEndpoint}
            placeholder="留空使用 Provider 默认端点"
            onblur={debouncedSaveMemoryExtraction}
          />

          <!-- Chat API Key -->
          <div class="relative">
            <Input
              label="Chat API Key"
              type={memoryExtractionShowKey ? "text" : "password"}
              bind:value={memoryExtractionChatApiKey}
              placeholder="留空使用 Provider 默认 Key"
              onblur={debouncedSaveMemoryExtraction}
            />
            <button
              class="absolute right-2 top-8 text-xs text-muted-foreground hover:text-foreground"
              onclick={() => (memoryExtractionShowKey = !memoryExtractionShowKey)}
            >
              {memoryExtractionShowKey ? "隐藏" : "显示"}
            </button>
          </div>

          <!-- Chat Model -->
          <Input
            label="Chat Model"
            type="text"
            bind:value={memoryExtractionChatModel}
            placeholder="留空使用 Provider 默认模型"
            onblur={debouncedSaveMemoryExtraction}
          />
        {:else}
          <p class="text-xs text-muted-foreground">
            自动记忆提取已禁用。群聊对话中的信息将不会被自动提取为用户记忆。
          </p>
        {/if}

        <hr class="border-border" />

        <!-- Memory Dream Cycle -->
        <div class="space-y-2">
          <h4 class="text-sm font-semibold">记忆衰减与归档</h4>
          <p class="text-xs text-muted-foreground">
            启用后，后台定期执行记忆合并（去重）和置信度衰减，低于阈值的记忆自动归档。
          </p>
          <label class="flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={memoryDreamEnabled}
              onchange={async (e) => {
                memoryDreamEnabled = e.currentTarget.checked;
                try {
                  settings = await api.updateUserSettings({ memory_dream_enabled: memoryDreamEnabled } as Partial<UserSettings>);
                } catch (err) {
                  dbgWarn("memory-mgmt", "save memory_dream_enabled failed", err);
                }
              }}
              class="h-4 w-4 rounded border-input"
            />
            <span class="text-sm">启用记忆衰减与归档</span>
          </label>
        </div>
      </div>
    {/if}
  </div>
</div>
