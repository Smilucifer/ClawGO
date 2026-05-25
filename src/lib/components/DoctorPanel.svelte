<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { DoctorStore } from "$lib/stores/doctor-store.svelte";
  import { copyToClipboard } from "$lib/utils/tool-rendering";
  import type { ConfigDiagnostics, ConfigIssue, McpServerInfo } from "$lib/types";

  interface Props {
    open: boolean;
    onclose: () => void;
    cwd?: string;
    mcpServers?: McpServerInfo[];
  }

  let { open, onclose, cwd, mcpServers }: Props = $props();

  const store = new DoctorStore();

  function effectiveCwd(): string {
    return cwd || localStorage.getItem("clawgo:project-cwd") || "";
  }

  let copiedReport = $state(false);
  let expandedSections = $state<Set<string>>(new Set(["cli", "auth", "project", "configs", "services", "system"]));

  $effect(() => {
    if (open) {
      const c = effectiveCwd();
      if (c) store.run(c, mcpServers);
      function onKey(e: KeyboardEvent) {
        if (e.key === "Escape") onclose();
      }
      window.addEventListener("keydown", onKey);
      return () => window.removeEventListener("keydown", onKey);
    }
  });

  function toggleSection(key: string) {
    if (expandedSections.has(key)) {
      expandedSections.delete(key);
    } else {
      expandedSections.add(key);
    }
    expandedSections = new Set(expandedSections);
  }

  async function copyReport() {
    if (!store.report) return;
    await copyToClipboard(store.report);
    copiedReport = true;
    setTimeout(() => (copiedReport = false), 1500);
  }

  function configStatus(configs: ConfigDiagnostics): string {
    const all = [...configs.settings_issues, ...configs.keybinding_issues, ...configs.mcp_issues, ...configs.env_var_issues];
    if (all.some((i) => i.severity === "error")) return "fail";
    if (all.length > 0) return "warn";
    return "pass";
  }

  function allIssues(configs: ConfigDiagnostics): ConfigIssue[] {
    return [...configs.settings_issues, ...configs.keybinding_issues, ...configs.mcp_issues, ...configs.env_var_issues];
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-40 bg-black/20"
    onclick={onclose}
    onkeydown={(e) => e.key === "Escape" && onclose()}
  ></div>

  <div class="fixed right-0 top-0 bottom-0 z-50 flex w-96 flex-col border-l border-border bg-background shadow-xl">
    <!-- Header -->
    <div class="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
      <h2 class="text-sm font-semibold">{t("doctor_panelTitle")}</h2>
      <div class="flex items-center gap-1">
        {#if store.report}
          <button
            type="button"
            class="flex h-7 items-center rounded-md px-2 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
            onclick={copyReport}
          >
            {copiedReport ? t("common_copied") : t("common_copy")}
          </button>
        {/if}
        <button
          type="button"
          class="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
          onclick={onclose}
          aria-label="Close"
        >
          <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M18 6 6 18" /><path d="m6 6 12 12" />
          </svg>
        </button>
      </div>
    </div>

    <!-- Content -->
    <div class="flex-1 overflow-y-auto">
      {#if store.loading}
        <div class="flex flex-col items-center justify-center py-16 gap-3">
          <div class="h-5 w-5 animate-spin rounded-full border-2 border-primary/30 border-t-primary"></div>
          <span class="text-xs text-muted-foreground">{t("doctor_running")}</span>
        </div>
      {:else if store.error}
        <div class="mx-4 mt-4 rounded-md border border-destructive/30 bg-destructive/10 p-3 text-xs text-destructive">
          {t("doctor_failed")}: {store.error}
        </div>
      {:else if store.rawReport}
        {@const r = store.rawReport}
        <div class="divide-y divide-border">
          <!-- CLI Section -->
          {@render sectionHeader("cli", t("doctor_sectionInstallation"), r.cli.found ? "pass" : "fail")}
          {#if expandedSections.has("cli")}
            <div class="px-4 py-2 space-y-1.5 text-xs">
              {@render statusRow(r.cli.found, t("doctor_cliFound", { version: r.cli.version ?? "unknown" }))}
              {#if r.cli.path}
                {@render detailRow(t("doctor_cliPath", { path: r.cli.path }))}
              {/if}
              {#if r.cli.latest}
                {#if r.cli.version === r.cli.latest}
                  {@render detailRow(`Latest: v${r.cli.latest} (${t("doctor_cliUpToDate")})`)}
                {:else}
                  {@render statusRow(false, t("doctor_cliUpdateAvailable", { latest: r.cli.latest }))}
                {/if}
              {/if}
              {#if r.cli.auto_update_channel}
                {@render detailRow(t("doctor_cliAutoUpdate", { channel: r.cli.auto_update_channel }))}
              {/if}
              {@render statusRow(r.cli.ripgrep_available, t("doctor_cliRipgrepOk"), t("doctor_cliRipgrepMissing"))}
            </div>
          {/if}

          <!-- Auth Section -->
          {@render sectionHeader("auth", t("doctor_sectionAuth"), r.auth.has_oauth || r.auth.has_api_key ? "pass" : "warn")}
          {#if expandedSections.has("auth")}
            <div class="px-4 py-2 space-y-1.5 text-xs">
              {@render statusRow(r.auth.has_oauth, r.auth.oauth_account ? t("doctor_authOauth", { account: r.auth.oauth_account }) : t("doctor_authOauthNoAccount"), t("doctor_authNoOauth"))}
              {@render statusRow(r.auth.has_api_key, t("doctor_authApiKey", { source: r.auth.api_key_source ?? "unknown", hint: r.auth.api_key_hint ?? "***" }), t("doctor_authNoApiKey"))}
              {@render statusRow(r.auth.app_has_credentials, t("doctor_authAppCreds", { name: r.auth.app_platform_name ?? "custom" }), t("doctor_authAppNoCreds"))}
            </div>
          {/if}

          <!-- Project Section -->
          {@render sectionHeader("project", t("doctor_sectionProject"), r.project.has_claude_md ? "pass" : "warn")}
          {#if expandedSections.has("project")}
            <div class="px-4 py-2 space-y-1.5 text-xs">
              {#if r.project.skipped_project_scope}
                {@render statusRow(null, t("doctor_projectSkipped"))}
              {/if}
              {@render statusRow(r.project.has_claude_md, t("doctor_projectClaudeMd"), t("doctor_projectNoClaudeMd"))}
              {#each r.project.claude_md_files as f}
                {#if f.size_chars > 10000}
                  {@render statusRow(false, t("doctor_projectLargeFile", { path: f.path, size: String(f.size_chars) }))}
                {/if}
              {/each}
            </div>
          {/if}

          <!-- Config Section -->
          {@render sectionHeader("configs", t("doctor_sectionConfig"), configStatus(r.configs))}
          {#if expandedSections.has("configs")}
            <div class="px-4 py-2 space-y-1.5 text-xs">
              {#if r.configs.settings_issues.length === 0}
                {@render statusRow(true, t("doctor_configSettingsOk"))}
              {/if}
              {#if r.configs.keybinding_issues.length === 0}
                {@render statusRow(true, t("doctor_configKeybindingsOk"))}
              {/if}
              {#if r.configs.mcp_issues.length === 0}
                {@render statusRow(true, t("doctor_configMcpOk"))}
              {/if}
              {#if r.configs.env_var_issues.length === 0}
                {@render statusRow(true, t("doctor_configEnvOk"))}
              {/if}
              {#each allIssues(r.configs) as issue}
                {@render configIssueRow(issue)}
              {/each}
            </div>
          {/if}

          <!-- MCP Servers Section -->
          {#if mcpServers && mcpServers.length > 0}
            {@render sectionHeader("mcp", t("doctor_sectionMcpSession"), mcpServers.some((s) => s.status !== "connected" && s.status !== "running") ? "warn" : "pass")}
            {#if expandedSections.has("mcp")}
              <div class="px-4 py-2 space-y-1.5 text-xs">
                {#each mcpServers as s}
                  {@render mcpServerRow(s)}
                {/each}
              </div>
            {/if}
          {/if}

          <!-- Services Section -->
          {@render sectionHeader("services", t("doctor_sectionServices"), r.services.community_registry === true && r.services.mcp_registry === true ? "pass" : "warn")}
          {#if expandedSections.has("services")}
            <div class="px-4 py-2 space-y-1.5 text-xs">
              {@render statusRow(r.services.community_registry, t("doctor_serviceCommunityOk"), t("doctor_serviceCommunityFail"), t("doctor_serviceCommunityUnknown"))}
              {@render statusRow(r.services.mcp_registry, t("doctor_serviceMcpOk"), t("doctor_serviceMcpFail"), t("doctor_serviceMcpUnknown"))}
            </div>
          {/if}

          <!-- System Section -->
          {@render sectionHeader("system", t("doctor_sectionSystem"), r.system.sandbox_available !== false && r.system.lock_files.length === 0 ? "pass" : "warn")}
          {#if expandedSections.has("system")}
            <div class="px-4 py-2 space-y-1.5 text-xs">
              {#if r.system.sandbox_available !== null}
                {@render statusRow(r.system.sandbox_available, t("doctor_systemSandboxOk"), t("doctor_systemSandboxMissing"))}
              {/if}
              {@render statusRow(r.system.lock_files.length === 0, t("doctor_systemNoLocks"), t("doctor_systemLocks", { count: String(r.system.lock_files.length) }))}
              {#each r.system.lock_files as f}
                {@render detailRow(f)}
              {/each}
            </div>
          {/if}
        </div>
      {:else}
        <div class="flex flex-col items-center justify-center py-16 gap-2">
          <span class="text-sm text-muted-foreground">{t("doctor_empty")}</span>
          <button
            type="button"
            class="rounded-md bg-primary px-3 py-1.5 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors"
            onclick={() => store.run(effectiveCwd(), mcpServers)}
          >
            {t("doctor_run")}
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}

{#snippet sectionHeader(key: string, label: string, status: string)}
  <button
    type="button"
    class="flex w-full items-center gap-2 px-4 py-2.5 text-xs font-medium hover:bg-accent/50 transition-colors"
    onclick={() => toggleSection(key)}
  >
    <span class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full {status === 'pass' ? 'bg-green-500/20 text-green-600' : status === 'fail' ? 'bg-destructive/20 text-destructive' : 'bg-yellow-500/20 text-yellow-600'}">
      {#if status === "pass"}
        <svg class="h-2.5 w-2.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>
      {:else if status === "fail"}
        <svg class="h-2.5 w-2.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
      {:else}
        <svg class="h-2.5 w-2.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M12 9v4"/><circle cx="12" cy="17" r=".5"/></svg>
      {/if}
    </span>
    <span class="flex-1 text-left">{label}</span>
    <svg
      class="h-3 w-3 shrink-0 text-muted-foreground transition-transform {expandedSections.has(key) ? 'rotate-90' : ''}"
      viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
    >
      <path d="m9 18 6-6-6-6"/>
    </svg>
  </button>
{/snippet}

{#snippet statusRow(ok: boolean | null | undefined, passMsg: string, failMsg?: string, unknownMsg?: string)}
  <div class="flex items-start gap-2">
    {#if ok === true}
      <span class="mt-0.5 text-green-600">✓</span>
      <span class="text-muted-foreground">{passMsg}</span>
    {:else if ok === false}
      <span class="mt-0.5 text-destructive">✗</span>
      <span>{failMsg ?? passMsg}</span>
    {:else}
      <span class="mt-0.5 text-yellow-600">?</span>
      <span class="text-muted-foreground">{unknownMsg ?? passMsg}</span>
    {/if}
  </div>
{/snippet}

{#snippet detailRow(msg: string)}
  <div class="flex items-start gap-2 pl-5">
    <span class="text-muted-foreground/60">└</span>
    <span class="text-muted-foreground break-all">{msg}</span>
  </div>
{/snippet}

{#snippet configIssueRow(issue: ConfigIssue)}
  <div class="flex items-start gap-2">
    {#if issue.severity === "error"}
      <span class="mt-0.5 text-destructive">✗</span>
    {:else}
      <span class="mt-0.5 text-yellow-600">⚠</span>
    {/if}
    <span class="break-words"><span class="text-muted-foreground">{issue.file}:</span> {issue.message}</span>
  </div>
{/snippet}

{#snippet mcpServerRow(s: McpServerInfo)}
  <div class="flex items-start gap-2">
    {#if s.status === "connected" || s.status === "running"}
      <span class="mt-0.5 text-green-600">✓</span>
      <span class="text-muted-foreground">{s.name}{s.server_type ? ` (${s.server_type})` : ""}</span>
    {:else if s.error}
      <span class="mt-0.5 text-destructive">✗</span>
      <span>{s.name}{s.server_type ? ` (${s.server_type})` : ""} — {s.error}</span>
    {:else}
      <span class="mt-0.5 text-yellow-600">⚠</span>
      <span>{s.name}{s.server_type ? ` (${s.server_type})` : ""} — {s.status}</span>
    {/if}
  </div>
{/snippet}
