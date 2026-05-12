<script lang="ts">
  import { listManagedMcpServers, addManagedMcpServer, removeManagedMcpServer } from "$lib/api";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import { t } from "$lib/i18n/index.svelte";
  import type { ConfiguredMcpServer } from "$lib/types";

  let {
    visible = false,
    operationLoading = $bindable<string | null>(null),
    showToast,
    confirmAction = $bindable<{
      title: string;
      message: string;
      onConfirm: () => void;
    } | null>(null),
  }: {
    visible?: boolean;
    operationLoading: string | null;
    showToast: (message: string, type: "success" | "error") => void;
    confirmAction: {
      title: string;
      message: string;
      onConfirm: () => void;
    } | null;
  } = $props();

  // ── State ──
  let servers = $state<ConfiguredMcpServer[]>([]);
  let loading = $state(true);
  let selectedServer = $state<ConfiguredMcpServer | null>(null);
  let showAddForm = $state(false);

  // Add form state
  let newName = $state("");
  let newTransport = $state<"stdio" | "http" | "sse">("stdio");
  let newCommand = $state("");
  let newArgs = $state("");
  let newUrl = $state("");
  let newEnvJson = $state("");
  let newHeadersJson = $state("");
  let adding = $state(false);

  const envPlaceholder = '{"KEY":"value"}';
  const headersPlaceholder = '{"Authorization":"Bearer ..."}';

  // ── Init ──
  $effect(() => {
    if (visible) {
      loadServers();
    }
  });

  async function loadServers() {
    loading = true;
    try {
      servers = await listManagedMcpServers();
      dbg("mcp-managed", "loaded", { count: servers.length });
    } catch (e) {
      dbgWarn("mcp-managed", "load error", e);
      servers = [];
    } finally {
      loading = false;
    }
  }

  async function refreshServers() {
    try {
      servers = await listManagedMcpServers();
    } catch (e) {
      dbgWarn("mcp-managed", "refresh error", e);
    }
  }

  function resetAddForm() {
    newName = "";
    newTransport = "stdio";
    newCommand = "";
    newArgs = "";
    newUrl = "";
    newEnvJson = "";
    newHeadersJson = "";
  }

  async function handleAdd() {
    if (!newName.trim()) {
      showToast(t("mcp_managedNameRequired"), "error");
      return;
    }

    adding = true;
    try {
      const config: Record<string, unknown> = { type: newTransport };

      if (newTransport === "stdio") {
        if (!newCommand.trim()) {
          showToast(t("mcp_managedCommandRequired"), "error");
          return;
        }
        config.command = newCommand.trim();
        if (newArgs.trim()) {
          config.args = newArgs.trim().split(/\s+/);
        }
      } else {
        if (!newUrl.trim()) {
          showToast(t("mcp_managedUrlRequired"), "error");
          return;
        }
        config.url = newUrl.trim();
      }

      if (newEnvJson.trim()) {
        try {
          config.env = JSON.parse(newEnvJson);
        } catch {
          showToast(t("mcp_managedInvalidEnvJson"), "error");
          return;
        }
      }

      if (newHeadersJson.trim()) {
        try {
          config.headers = JSON.parse(newHeadersJson);
        } catch {
          showToast(t("mcp_managedInvalidHeadersJson"), "error");
          return;
        }
      }

      const isUpdate = servers.some((s) => s.name === newName.trim());
      const result = await addManagedMcpServer(newName.trim(), JSON.stringify(config));
      showToast(
        result.success
          ? t(isUpdate ? "mcp_updatedServer" : "mcp_addedServer", { name: newName.trim() })
          : result.message,
        result.success ? "success" : "error",
      );
      if (result.success) {
        resetAddForm();
        showAddForm = false;
        await refreshServers();
      }
    } catch (e) {
      showToast(t("mcp_errorGeneric", { error: String(e) }), "error");
    } finally {
      adding = false;
    }
  }

  function handleRemove(server: ConfiguredMcpServer) {
    confirmAction = {
      title: t("mcp_removeTitle"),
      message: t("mcp_managedRemoveConfirm", { name: server.name }),
      onConfirm: async () => {
        operationLoading = server.name;
        try {
          const result = await removeManagedMcpServer(server.name);
          showToast(
            result.success ? t("mcp_removedServer", { name: server.name }) : result.message,
            result.success ? "success" : "error",
          );
          if (result.success) {
            if (selectedServer?.name === server.name) {
              selectedServer = null;
            }
            await refreshServers();
          }
        } catch (e) {
          showToast(t("mcp_errorGeneric", { error: String(e) }), "error");
        } finally {
          operationLoading = null;
        }
      },
    };
  }

  function typeBadgeColor(serverType: string): string {
    switch (serverType) {
      case "stdio":
        return "bg-blue-500/10 text-blue-600 dark:text-blue-400";
      case "http":
        return "bg-teal-500/10 text-teal-600 dark:text-teal-400";
      case "sse":
        return "bg-purple-500/10 text-purple-600 dark:text-purple-400";
      default:
        return "bg-muted text-muted-foreground";
    }
  }
</script>

{#if loading}
  <div class="flex items-center justify-center py-8">
    <div
      class="h-4 w-4 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
    ></div>
    <span class="ml-2 text-xs text-muted-foreground">{t("mcp_loadingConfigured")}</span>
  </div>
{:else}
  <!-- Add button -->
  <div class="mb-3 flex items-center justify-between">
    <p class="text-xs text-muted-foreground">{t("mcp_managedDesc")}</p>
    <button
      class="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90 transition-colors"
      onclick={() => {
        showAddForm = !showAddForm;
        if (!showAddForm) resetAddForm();
      }}
    >
      {showAddForm ? t("common_cancel") : t("mcp_addButton")}
    </button>
  </div>

  <!-- Add form -->
  {#if showAddForm}
    <div class="mb-4 rounded-lg border border-border/50 bg-muted/20 p-4 space-y-3">
      <div class="grid grid-cols-2 gap-3">
        <div>
          <label class="text-[11px] font-medium text-muted-foreground mb-1 block" for="mcp-name"
            >{t("mcp_managedName")}</label
          >
          <input
            id="mcp-name"
            type="text"
            bind:value={newName}
            placeholder="my-server"
            class="w-full rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary"
          />
        </div>
        <div>
          <label class="text-[11px] font-medium text-muted-foreground mb-1 block" for="mcp-transport"
            >{t("mcp_managedTransport")}</label
          >
          <select
            id="mcp-transport"
            bind:value={newTransport}
            class="w-full rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
          >
            <option value="stdio">stdio</option>
            <option value="http">http</option>
            <option value="sse">sse</option>
          </select>
        </div>
      </div>

      {#if newTransport === "stdio"}
        <div>
          <label class="text-[11px] font-medium text-muted-foreground mb-1 block" for="mcp-command"
            >{t("mcp_command")}</label
          >
          <input
            id="mcp-command"
            type="text"
            bind:value={newCommand}
            placeholder="npx"
            class="w-full rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-foreground font-mono placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary"
          />
        </div>
        <div>
          <label class="text-[11px] font-medium text-muted-foreground mb-1 block" for="mcp-args"
            >{t("mcp_managedArgs")}</label
          >
          <input
            id="mcp-args"
            type="text"
            bind:value={newArgs}
            placeholder="-y @example/mcp-server"
            class="w-full rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-foreground font-mono placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary"
          />
        </div>
      {:else}
        <div>
          <label class="text-[11px] font-medium text-muted-foreground mb-1 block" for="mcp-url"
            >{t("mcp_url")}</label
          >
          <input
            id="mcp-url"
            type="text"
            bind:value={newUrl}
            placeholder={newTransport === "sse" ? "http://localhost:3000/sse" : "http://localhost:3000"}
            class="w-full rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-foreground font-mono placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary"
          />
        </div>
      {/if}

      <div class="grid grid-cols-2 gap-3">
        <div>
          <label class="text-[11px] font-medium text-muted-foreground mb-1 block" for="mcp-env"
            >{t("mcp_envVars")} (JSON)</label
          >
          <input
            id="mcp-env"
            type="text"
            bind:value={newEnvJson}
            placeholder={envPlaceholder}
            class="w-full rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-foreground font-mono placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary"
          />
        </div>
        {#if newTransport === "http"}
          <div>
            <label class="text-[11px] font-medium text-muted-foreground mb-1 block" for="mcp-headers"
              >{t("mcp_headers")} (JSON)</label
            >
            <input
              id="mcp-headers"
              type="text"
              bind:value={newHeadersJson}
              placeholder={headersPlaceholder}
              class="w-full rounded-md border border-border bg-background px-2.5 py-1.5 text-xs text-foreground font-mono placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary"
            />
          </div>
        {/if}
      </div>

      <div class="flex justify-end gap-2 pt-1">
        <button
          class="rounded-md border border-border px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
          onclick={() => {
            showAddForm = false;
            resetAddForm();
          }}
        >
          {t("common_cancel")}
        </button>
        <button
          class="rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
          onclick={handleAdd}
          disabled={adding}
        >
          {adding ? t("mcp_adding") : t("mcp_addButton")}
        </button>
      </div>
    </div>
  {/if}

  {#if servers.length === 0 && !showAddForm}
    <div class="flex flex-col items-center justify-center py-12 text-center">
      <div
        class="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-2xl border border-border bg-muted"
      >
        <svg
          class="h-6 w-6 text-muted-foreground"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
          ><rect width="20" height="8" x="2" y="2" rx="2" ry="2" /><rect
            width="20"
            height="8"
            x="2"
            y="14"
            rx="2"
            ry="2"
          /><line x1="6" x2="6.01" y1="6" y2="6" /><line x1="6" x2="6.01" y1="18" y2="18" /></svg
        >
      </div>
      <h2 class="text-sm font-medium text-foreground mb-1">{t("mcp_managedEmpty")}</h2>
      <p class="text-xs text-muted-foreground max-w-sm">
        {t("mcp_managedEmptyHint")}
      </p>
    </div>
  {:else if servers.length > 0}
    <div class="flex gap-3" style="height: calc(100vh - 360px); min-height: 260px;">
      <!-- Left: scrollable server list -->
      <div class="w-[280px] shrink-0 overflow-y-auto space-y-1.5 pr-1">
        {#each servers as server}
          <div
            class="w-full text-left rounded-lg border px-3 py-2 transition-colors cursor-pointer {selectedServer?.name ===
              server.name
              ? 'border-primary/50 bg-primary/5'
              : 'border-border/50 bg-muted/30 hover:bg-muted/50'}"
            onclick={() => (selectedServer = server)}
            onkeydown={(e) => {
              if (e.key === "Enter") selectedServer = server;
            }}
            role="button"
            tabindex="0"
          >
            <div class="flex items-center justify-between gap-2">
              <div class="flex-1 min-w-0">
                <span class="text-sm font-medium text-foreground truncate block">{server.name}</span>
                <div class="flex items-center gap-1.5 mt-0.5">
                  <span
                    class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {typeBadgeColor(
                      server.server_type,
                    )}"
                  >
                    {server.server_type}
                  </span>
                  <span
                    class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400"
                  >
                    managed
                  </span>
                </div>
              </div>
              <button
                class="shrink-0 rounded p-1 text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors disabled:opacity-50"
                onclick={(e) => {
                  e.stopPropagation();
                  handleRemove(server);
                }}
                title={t("mcp_removeServerTooltip")}
                disabled={operationLoading === server.name}
              >
                {#if operationLoading === server.name}
                  <div
                    class="h-3.5 w-3.5 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
                  ></div>
                {:else}
                  <svg
                    class="h-3.5 w-3.5"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    ><path d="M3 6h18" /><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" /><path
                      d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"
                    /></svg
                  >
                {/if}
              </button>
            </div>
          </div>
        {/each}
      </div>

      <!-- Right: detail panel -->
      <div class="flex-1 min-w-0 overflow-y-auto">
        {#if selectedServer}
          <div class="rounded-lg border border-border/50 bg-muted/20 p-4 space-y-3">
            <!-- Header -->
            <div class="flex items-start justify-between gap-2">
              <div class="flex-1 min-w-0">
                <h3 class="text-sm font-semibold text-foreground">{selectedServer.name}</h3>
                <div class="flex items-center gap-1.5 mt-1">
                  <span
                    class="rounded-full px-1.5 py-0.5 text-[10px] font-medium {typeBadgeColor(
                      selectedServer.server_type,
                    )}"
                  >
                    {selectedServer.server_type}
                  </span>
                  <span
                    class="rounded-full px-1.5 py-0.5 text-[10px] font-medium bg-amber-500/10 text-amber-600 dark:text-amber-400"
                  >
                    managed
                  </span>
                </div>
              </div>
              <button
                class="shrink-0 text-muted-foreground hover:text-foreground"
                onclick={() => (selectedServer = null)}
                title={t("common_close")}
              >
                <svg
                  class="h-3.5 w-3.5"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"><path d="M18 6 6 18" /><path d="m6 6 12 12" /></svg
                >
              </button>
            </div>

            <!-- Command + args (stdio) -->
            {#if selectedServer.command}
              <div class="border-t border-border pt-3">
                <div class="text-[11px] font-medium text-muted-foreground mb-1">
                  {t("mcp_command")}
                </div>
                <div class="rounded-md bg-muted/40 px-3 py-2 font-mono text-xs text-foreground">
                  {selectedServer.command}{#if selectedServer.args?.length > 0}{" " +
                      selectedServer.args.join(" ")}{/if}
                </div>
              </div>
            {/if}

            <!-- URL (http/sse) -->
            {#if selectedServer.url}
              <div class="border-t border-border pt-3">
                <div class="text-[11px] font-medium text-muted-foreground mb-1">{t("mcp_url")}</div>
                <div
                  class="rounded-md bg-muted/40 px-3 py-2 font-mono text-xs text-foreground truncate"
                >
                  {selectedServer.url}
                </div>
              </div>
            {/if}

            <!-- Env keys -->
            {#if selectedServer.env_keys?.length > 0}
              <div class="border-t border-border pt-3">
                <div class="text-[11px] font-medium text-muted-foreground mb-1">
                  {t("mcp_envVars")}
                </div>
                <div class="flex flex-wrap gap-1.5">
                  {#each selectedServer.env_keys as key}
                    <span
                      class="rounded-md bg-muted/40 px-2 py-1 font-mono text-[10px] text-foreground"
                      >{key}</span
                    >
                  {/each}
                </div>
              </div>
            {/if}

            <!-- Header keys -->
            {#if selectedServer.header_keys?.length > 0}
              <div class="border-t border-border pt-3">
                <div class="text-[11px] font-medium text-muted-foreground mb-1">
                  {t("mcp_headers")}
                </div>
                <div class="flex flex-wrap gap-1.5">
                  {#each selectedServer.header_keys as key}
                    <span
                      class="rounded-md bg-muted/40 px-2 py-1 font-mono text-[10px] text-foreground"
                      >{key}</span
                    >
                  {/each}
                </div>
              </div>
            {/if}

            <!-- Injection note -->
            <div class="border-t border-border pt-3">
              <p class="text-[11px] text-muted-foreground">
                {t("mcp_managedInjectionNote")}
              </p>
            </div>

            <!-- Remove button -->
            <div class="border-t border-border pt-3">
              <button
                class="rounded-md border border-destructive/30 px-3 py-1.5 text-xs text-destructive hover:bg-destructive/10 transition-colors disabled:opacity-50"
                onclick={() => handleRemove(selectedServer!)}
                disabled={operationLoading === selectedServer.name}
              >
                {operationLoading === selectedServer.name
                  ? t("mcp_removing")
                  : t("mcp_removeServer")}
              </button>
            </div>
          </div>
        {:else}
          <div
            class="rounded-lg border border-dashed border-border/50 p-6 flex items-center justify-center h-full"
          >
            <p class="text-xs text-muted-foreground">{t("mcp_selectServerDetails")}</p>
          </div>
        {/if}
      </div>
    </div>
  {/if}
{/if}
