<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { getTransport } from "$lib/transport";
  import * as api from "$lib/api";
  import Input from "$lib/components/Input.svelte";
  import { dbgWarn } from "$lib/utils/debug";
  import type { UserSettings, EmbeddingConfig } from "$lib/types";

  type MemTab = "userMemory" | "archived" | "extractionConfig";
  let activeTab: MemTab = $state("userMemory");

  const tabs: { id: MemTab; label: string }[] = $derived([
    { id: "userMemory", label: t("memoryMgmt_tab_userMemory") },
    { id: "archived", label: "Archived" },
    { id: "extractionConfig", label: t("memoryMgmt_tab_extractionConfig") },
  ]);

  let scopeFilter: string | null = $state(null);
  const scopeOptions = [null, "global", "project", "invest"];

  // ── Shared helpers ────────────────────────────────────────────────

  async function saveSettingsPatch(patch: Partial<UserSettings>) {
    try {
      settings = await api.updateUserSettings(patch);
    } catch (err) {
      dbgWarn("memory-mgmt", "save settings failed", err);
    }
  }

  // ── Memory Extraction Config state ──
  let memoryExtractionEnabled = $state(false);
  let memoryExtractionChatEndpoint = $state("");
  let memoryExtractionChatModel = $state("");
  let memoryExtractionChatApiKey = $state("");
  let memoryExtractionShowKey = $state(false);
  let memoryExtractionSaveDebounce: ReturnType<typeof setTimeout> | null = null;
  let memoryDreamEnabled = $state(true);
  let settings = $state<UserSettings | null>(null);

  function loadExtractionConfigFromSettings(s: UserSettings) {
    const ec = s.embedding_config;
    if (ec) {
      memoryExtractionEnabled = ec.enabled ?? false;
      memoryExtractionChatEndpoint = ec.chat_endpoint ?? ec.endpoint ?? "";
      memoryExtractionChatModel = ec.chat_model ?? ec.model ?? "";
      memoryExtractionChatApiKey = ec.chat_api_key ?? ec.api_key ?? "";
    } else {
      memoryExtractionEnabled = false;
      memoryExtractionChatEndpoint = "";
      memoryExtractionChatModel = "";
      memoryExtractionChatApiKey = "";
    }
  }

  async function saveExtractionConfigToSettings() {
    if (!settings) return;
    const ep = memoryExtractionChatEndpoint.trim();
    const mdl = memoryExtractionChatModel.trim();
    const key = memoryExtractionChatApiKey.trim();
    const ec: EmbeddingConfig | undefined = memoryExtractionEnabled
      ? {
          enabled: true,
          endpoint: ep || "http://localhost:8080/v1",
          model: mdl || "gpt-4o-mini",
          api_key: key || undefined,
          chat_endpoint: ep || undefined,
          chat_model: mdl || undefined,
          chat_api_key: key || undefined,
        }
      : undefined;
    await saveSettingsPatch({ embedding_config: ec } as Partial<UserSettings>);
  }

  function debouncedSaveExtraction() {
    if (memoryExtractionSaveDebounce) clearTimeout(memoryExtractionSaveDebounce);
    memoryExtractionSaveDebounce = setTimeout(() => saveExtractionConfigToSettings(), 500);
  }

  onMount(async () => {
    try {
      settings = await api.getUserSettings();
      memoryDreamEnabled = settings.memory_dream_enabled ?? true;
      loadExtractionConfigFromSettings(settings);
    } catch (e) {
      dbgWarn("memory-mgmt", "load settings failed", e);
    }
  });

  // ── Memory list loader (parameterized) ────────────────────────────

  let memories: any[] = $state([]);
  let archivedMemories: any[] = $state([]);
  let loading = $state(false);
  let archivedLoading = $state(false);
  let loadVersion = 0;
  let archivedLoadVersion = 0;

  async function loadMemoryList(
    statusFilter: "approved" | "archived",
  ) {
    const isApproved = statusFilter === "approved";
    const versionRef = isApproved ? ++loadVersion : ++archivedLoadVersion;
    const setLoad = (v: boolean) => { if (isApproved) loading = v; else archivedLoading = v; };
    const setResult = (r: any[]) => { if (isApproved) memories = r; else archivedMemories = r; };
    const isStale = () => isApproved ? versionRef !== loadVersion : versionRef !== archivedLoadVersion;

    setLoad(true);
    try {
      const result = await getTransport().invoke<any[]>("list_memories", {
        status_filter: statusFilter,
        scope_filter: scopeFilter,
      });
      if (!isStale()) setResult(result);
    } catch (e) {
      console.error(`Failed to load ${statusFilter} memories:`, e);
    } finally {
      if (!isStale()) setLoad(false);
    }
  }

  $effect(() => {
    void scopeFilter;
    loadMemoryList("approved");
  });

  $effect(() => {
    if (activeTab === "archived") {
      loadMemoryList("archived");
    }
  });

  // ── Memory actions (parameterized) ────────────────────────────────

  let actionLoadingId: string | null = $state(null);

  async function performMemoryAction(
    command: string,
    id: string,
    sourceList: "approved" | "archived",
  ) {
    actionLoadingId = id;
    try {
      await getTransport().invoke(command, { id });
      if (sourceList === "approved") {
        memories = memories.filter((m) => m.id !== id);
      } else {
        archivedMemories = archivedMemories.filter((m) => m.id !== id);
      }
    } catch (e) {
      console.error(`Failed to ${command}:`, e);
    } finally {
      actionLoadingId = null;
    }
  }

  // ── Manual add memory ─────────────────────────────────────────────

  const MEMORY_TYPES = ["fact", "preference", "skill", "feedback"] as const;
  const ADD_SCOPE_OPTIONS = ["global", "project", "invest"] as const;

  let showAddForm = $state(false);
  let addContent = $state("");
  let addType = $state<(typeof MEMORY_TYPES)[number]>("fact");
  let addScope = $state<(typeof ADD_SCOPE_OPTIONS)[number]>("global");
  let addSubmitting = $state(false);
  let addError = $state<string | null>(null);

  function resetAddForm() {
    addContent = "";
    addType = "fact";
    addScope = "global";
    addError = null;
  }

  async function submitAddMemory() {
    const trimmed = addContent.trim();
    if (!trimmed || addSubmitting) return;
    addSubmitting = true;
    addError = null;
    try {
      // save_memory signature: { content, memory_type, source_run_id?, confidence?, scope?, project_id? }
      await getTransport().invoke("save_memory", {
        content: trimmed,
        memory_type: addType,
        scope: addScope,
      });
      resetAddForm();
      showAddForm = false;
      await loadMemoryList("approved");
    } catch (e) {
      addError = e instanceof Error ? e.message : String(e);
    } finally {
      addSubmitting = false;
    }
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
      <div class="mb-4 flex items-center justify-between gap-2">
        <div class="flex gap-2">
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
        <button
          class="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90"
          onclick={() => {
            showAddForm = !showAddForm;
            if (!showAddForm) resetAddForm();
          }}
        >
          {showAddForm ? t("common_cancel") : t("memory_mgmt_add")}
        </button>
      </div>

      {#if showAddForm}
        <div class="mb-4 rounded-md border border-border bg-muted/30 p-3">
          <div class="mb-2">
            <label class="mb-1 block text-xs font-medium" for="add-mem-content">
              {t("memory_mgmt_add_content")}
            </label>
            <textarea
              id="add-mem-content"
              bind:value={addContent}
              rows="3"
              class="w-full rounded-md border border-input bg-background px-2 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-ring"
              placeholder=""
            ></textarea>
          </div>
          <div class="mb-2 flex gap-2">
            <div class="flex-1">
              <label class="mb-1 block text-xs font-medium" for="add-mem-type">
                {t("memory_mgmt_add_type")}
              </label>
              <select
                id="add-mem-type"
                bind:value={addType}
                class="w-full rounded-md border border-input bg-background px-2 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-ring"
              >
                {#each MEMORY_TYPES as ty}
                  <option value={ty}>{ty}</option>
                {/each}
              </select>
            </div>
            <div class="flex-1">
              <label class="mb-1 block text-xs font-medium" for="add-mem-scope">
                {t("memory_mgmt_add_scope")}
              </label>
              <select
                id="add-mem-scope"
                bind:value={addScope}
                class="w-full rounded-md border border-input bg-background px-2 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-ring"
              >
                {#each ADD_SCOPE_OPTIONS as sc}
                  <option value={sc}>{sc}</option>
                {/each}
              </select>
            </div>
          </div>
          {#if addError}
            <div class="mb-2 text-xs text-destructive">{addError}</div>
          {/if}
          <div class="flex justify-end gap-2">
            <button
              class="rounded-md bg-muted px-3 py-1 text-xs font-medium transition-colors hover:bg-muted/80 disabled:opacity-50"
              disabled={addSubmitting}
              onclick={() => {
                showAddForm = false;
                resetAddForm();
              }}
            >
              {t("common_cancel")}
            </button>
            <button
              class="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
              disabled={addSubmitting || !addContent.trim()}
              onclick={submitAddMemory}
            >
              {addSubmitting ? "..." : t("memory_mgmt_add_submit")}
            </button>
          </div>
        </div>
      {/if}

      {#if loading}
        <div class="text-muted-foreground text-sm">Loading...</div>
      {:else if memories.length === 0}
        <div class="text-muted-foreground text-sm">No memories found</div>
      {:else}
        <div class="flex flex-col gap-2">
          {#each memories as mem (mem.id)}
            <div class="rounded-md border border-border p-3">
              <div class="mb-1 flex items-center gap-2">
                <span class="rounded bg-muted px-1.5 py-0.5 text-xs">{mem.scope}</span>
                <span class="rounded bg-muted px-1.5 py-0.5 text-xs">{mem.memory_type}</span>
                {#if mem.confidence != null}
                  <span class="text-muted-foreground text-xs">confidence: {mem.confidence.toFixed(1)}</span>
                {/if}
              </div>
              <div class="text-sm">{mem.content}</div>
              <div class="mt-2 flex items-center gap-2">
                <span class="text-muted-foreground flex-1 text-xs">Updated: {mem.updated_at}</span>
                <button
                  class="rounded-md bg-muted px-2.5 py-1 text-xs font-medium transition-colors hover:bg-muted/80 disabled:opacity-50"
                  disabled={actionLoadingId === mem.id}
                  onclick={() => performMemoryAction("archive_memory", mem.id, "approved")}
                >
                  归档
                </button>
                <button
                  class="rounded-md bg-destructive/10 px-2.5 py-1 text-xs font-medium text-destructive transition-colors hover:bg-destructive/20 disabled:opacity-50"
                  disabled={actionLoadingId === mem.id}
                  onclick={() => performMemoryAction("remove_memory", mem.id, "approved")}
                >
                  删除
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}

    {:else if activeTab === "archived"}
      {#if archivedLoading}
        <div class="text-muted-foreground text-sm">Loading...</div>
      {:else if archivedMemories.length === 0}
        <div class="text-muted-foreground text-sm">No archived memories</div>
      {:else}
        <div class="flex flex-col gap-2">
          {#each archivedMemories as mem (mem.id)}
            <div class="rounded-md border border-border p-3">
              <div class="mb-1 flex items-center gap-2">
                <span class="rounded bg-muted px-1.5 py-0.5 text-xs">{mem.scope}</span>
                <span class="rounded bg-muted px-1.5 py-0.5 text-xs">{mem.memory_type}</span>
                {#if mem.confidence != null}
                  <span class="text-muted-foreground text-xs">confidence: {mem.confidence.toFixed(1)}</span>
                {/if}
                <span class="text-muted-foreground text-xs">{mem.updated_at}</span>
              </div>
              <div class="text-sm">{mem.content}</div>
              <div class="mt-2">
                <button
                  class="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
                  disabled={actionLoadingId === mem.id}
                  onclick={() => performMemoryAction("restore_memory", mem.id, "archived")}
                >
                  {actionLoadingId === mem.id ? "Restoring..." : "Restore"}
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
              saveExtractionConfigToSettings();
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
            onblur={debouncedSaveExtraction}
          />

          <!-- Chat API Key -->
          <div class="relative">
            <Input
              label="Chat API Key"
              type={memoryExtractionShowKey ? "text" : "password"}
              bind:value={memoryExtractionChatApiKey}
              placeholder="留空使用 Provider 默认 Key"
              onblur={debouncedSaveExtraction}
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
            onblur={debouncedSaveExtraction}
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
                await saveSettingsPatch({ memory_dream_enabled: memoryDreamEnabled } as Partial<UserSettings>);
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
