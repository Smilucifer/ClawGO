<script lang="ts">
  import { onMount, getContext } from "svelte";
  import { page } from "$app/stores";
  import * as api from "$lib/api";
  import { loadCliInfo, KeybindingStore } from "$lib/stores";
  import type {
    UserSettings,
    CliConfigSettingDef,
    RemoteHost,
    RemoteTestResult,
    SshKeyInfo,
    CliCheckResult,
    ProviderValidationResult,
  } from "$lib/types";
  import Card from "$lib/components/Card.svelte";
  import Button from "$lib/components/Button.svelte";
  import Input from "$lib/components/Input.svelte";
  import KeybindingEditor from "$lib/components/KeybindingEditor.svelte";
  import { formatKeyDisplay } from "$lib/stores/keybindings.svelte";
  import { findCredential, PLATFORM_PRESETS, expandModelsToTiers } from "$lib/utils/platform-presets";
  import { PHASE7_PROVIDERS, getPhase7Provider, type Phase7ProviderEntry } from "$lib/utils/provider-catalog";
  import type {
    PlatformCredential,
    WindowsMsvcEnvMode,
    WindowsMsvcEnvStatus,
  } from "$lib/types";
  import {
    isDebugMode,
    setDebugMode,
    copyDebugLogs,
    getDebugLogCount,
    clearDebugLogs,
    getDebugFilter,
  } from "$lib/utils/debug";
  import { dbg, dbgWarn, redactSensitive } from "$lib/utils/debug";
  import { splitPath } from "$lib/utils/format";
  import { IS_WINDOWS } from "$lib/utils/platform";
  import { t, LOCALE_REGISTRY, currentLocale, switchLocale } from "$lib/i18n/index.svelte";
  import { getTransport } from "$lib/transport";

  // ── Tab state ──
  type SettingsTab = "general" | "connection" | "cli-config" | "shortcuts" | "remote" | "debug" | "characters" | "memory";
  const VALID_TABS: SettingsTab[] = [
    "general",
    "connection",
    "cli-config",
    "shortcuts",
    "remote",
    "debug",
    "characters",
    "memory",
  ];
  const urlTab = $page.url.searchParams.get("tab");
  const initialTab: SettingsTab = VALID_TABS.includes(urlTab as SettingsTab)
    ? (urlTab as SettingsTab)
    : "general";
  let activeTab = $state<SettingsTab>(initialTab);

  const tabLabels: Record<SettingsTab, () => string> = {
    general: () => t("settings_tab_general"),
    connection: () => t("settings_tab_connection"),
    "cli-config": () => t("settings_tab_cliConfig"),
    shortcuts: () => t("settings_tab_shortcuts"),
    remote: () => t("settings_tab_remote"),
    debug: () => t("settings_tab_debug"),
    characters: () => t("settings_tab_characters"),
    memory: () => "Memory Extraction",
  };

  const tabs: { id: SettingsTab; icon: string }[] = [
    {
      id: "general",
      icon: "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8z",
    },
    {
      id: "connection",
      icon: "M12 2a4 4 0 0 0-4 4c0 1.1.9 2 2 2h4a2 2 0 0 0 2-2 4 4 0 0 0-4-4z M8 8v2a4 4 0 0 0 8 0V8 M12 14v4 M8 18h8",
    },
    {
      id: "cli-config",
      icon: "M4 17l6-6-6-6 M12 19h8",
    },
    {
      id: "shortcuts",
      icon: "M10 8h.01 M12 12h.01 M14 8h.01 M16 12h.01 M18 8h.01 M6 8h.01 M7 16h10 M8 12h.01 M2 4h20v16H2z",
    },
    {
      id: "remote",
      icon: "M4 14a1 1 0 0 1-.78-1.63l9.9-10.2a.5.5 0 0 1 .86.46l-1.92 6.02A1 1 0 0 0 13 10h7a1 1 0 0 1 .78 1.63l-9.9 10.2a.5.5 0 0 1-.86-.46l1.92-6.02A1 1 0 0 0 11 14z",
    },
    { id: "debug", icon: "m18 16 4-4-4-4 M6 8l-4 4 4 4 M14.5 4l-5 16" },
    { id: "characters", icon: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2 M9 3a4 4 0 1 0 0 8 4 4 0 0 0 0-8z M22 21v-2a4 4 0 0 0-3-3.87 M16 3.13a4 4 0 0 1 0 7.75" },
    { id: "memory", icon: "M12 2a7 7 0 0 0-7 7c0 4.42 6.26 11.34 7 11.34s7-6.92 7-11.34A7 7 0 0 0 12 2z M12 6a3 3 0 1 0 0 6 3 3 0 0 0 0-6z" },
  ];

  let settings = $state<UserSettings | null>(null);
  let showApiKey = $state(false);
  let platformCredentials = $state<PlatformCredential[]>([]);

  // ── Custom Provider form state ──
  let showCustomForm = $state(false);
  let customFormName = $state("");
  let customFormUrl = $state("");
  let customFormKey = $state("");
  let customFormModel = $state("");
  let customFormEffort = $state("max");
  let customShowApiKey = $state(false);

  // ── Memory Extraction Config state ──
  let memoryExtractionEnabled = $state(true);
  let memoryExtractionChatEndpoint = $state("");
  let memoryExtractionChatModel = $state("");
  let memoryExtractionChatApiKey = $state("");
  let memoryExtractionShowKey = $state(false);
  let memoryExtractionSaveDebounce: ReturnType<typeof setTimeout> | null = null;

  function loadMemoryExtractionConfig() {
    try {
      const settings = JSON.parse(localStorage.getItem("clawgo-memory-extraction") ?? "{}");
      if (settings.chat_endpoint) memoryExtractionChatEndpoint = settings.chat_endpoint;
      if (settings.chat_model) memoryExtractionChatModel = settings.chat_model;
      if (settings.chat_api_key) memoryExtractionChatApiKey = settings.chat_api_key;
      if (settings.enabled === false) memoryExtractionEnabled = false;
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

  type ConnectionAgentTab = "claude" | "codex";
  const connectionAgentTabs: Array<{ id: ConnectionAgentTab; label: string; command: string }> = [
    { id: "claude", label: "Claude", command: "claude" },
    { id: "codex", label: "Codex", command: "codex" },
  ];
  let connectionCliChecks = $state<Record<ConnectionAgentTab, CliCheckResult | null>>({
    claude: null,
    codex: null,
  });
  let providerValidationResults = $state<Record<string, ProviderValidationResult | null>>({});
  let providerValidationApplying = $state(false);
  let providerValidationSummary = $state("");
  let connectionCliChecking = $state(false);
  let msvcEnvMode = $state<WindowsMsvcEnvMode>("auto");
  let msvcEnvStatus = $state<WindowsMsvcEnvStatus | null>(null);
  let msvcEnvLoading = $state(false);
  let msvcEnvSaving = $state(false);

  function providerCliCheck(provider: Phase7ProviderEntry): CliCheckResult | null {
    if (provider.mode !== "official_cli") return null;
    return connectionCliChecks[provider.executionAgent as ConnectionAgentTab];
  }

  function providerCredential(provider: Phase7ProviderEntry): PlatformCredential | undefined {
    if (!provider.platformId) return undefined;
    return findCredential(platformCredentials, provider.platformId);
  }

  const MIMO_SHARED_ENV_KEYS = [
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "CLAUDE_CODE_SUBAGENT_MODEL",
    "CLAUDE_CODE_EFFORT_LEVEL",
  ] as const;

  /** Get the mimo-api credential — shown as second row inside mimo-plan card. */
  function mimoApiCredential(): PlatformCredential | undefined {
    return findCredential(platformCredentials, "mimo-api");
  }

  function resolveMimoSharedExtraEnv(): Record<string, string> {
    const planExtra = providerCredential(getPhase7Provider("mimo-plan"))?.extra_env ?? {};
    const apiExtra = mimoApiCredential()?.extra_env ?? {};
    const merged: Record<string, string> = {};
    for (const key of MIMO_SHARED_ENV_KEYS) {
      const planValue = planExtra[key];
      const apiValue = apiExtra[key];
      if (planValue?.trim()) {
        merged[key] = planValue;
      } else if (apiValue?.trim()) {
        merged[key] = apiValue;
      }
    }
    return merged;
  }

  function updateMimoApiKey(value: string) {
    const existing = mimoApiCredential();
    const preset = PLATFORM_PRESETS.find((p) => p.id === "mimo-api");
    const next: PlatformCredential = {
      platform_id: "mimo-api",
      api_key: value || undefined,
      base_url: existing?.base_url ?? preset?.base_url,
      auth_env_var: existing?.auth_env_var ?? (preset?.auth_env_var as string) ?? "ANTHROPIC_AUTH_TOKEN",
      name: existing?.name ?? preset?.name,
      models: existing?.models ?? preset?.models,
      extra_env: existing?.extra_env,
    };
    const rest = platformCredentials.filter((c) => c.platform_id !== "mimo-api");
    platformCredentials = [...rest, next];
  }

  function updateMimoApiExtraEnv(envKey: string, value: string) {
    const existing = mimoApiCredential();
    const preset = PLATFORM_PRESETS.find((p) => p.id === "mimo-api");
    const extraEnv = { ...(existing?.extra_env ?? {}) };
    if (value.trim() === "") {
      delete extraEnv[envKey];
    } else {
      extraEnv[envKey] = value;
    }
    const next: PlatformCredential = {
      platform_id: "mimo-api",
      api_key: existing?.api_key,
      base_url: existing?.base_url ?? preset?.base_url,
      auth_env_var: existing?.auth_env_var ?? (preset?.auth_env_var as string) ?? "ANTHROPIC_AUTH_TOKEN",
      name: existing?.name ?? preset?.name,
      models: existing?.models ?? preset?.models,
      extra_env: Object.keys(extraEnv).length > 0 ? extraEnv : undefined,
    };
    const rest = platformCredentials.filter((c) => c.platform_id !== "mimo-api");
    platformCredentials = [...rest, next];
  }

  function updateMimoSharedExtraEnv(envKey: string, value: string) {
    updateApiProviderExtraEnv(getPhase7Provider("mimo-plan"), envKey, value);
    updateMimoApiExtraEnv(envKey, value);
  }

  function providerStatusLabel(provider: Phase7ProviderEntry): string {
    if (provider.mode === "official_cli") {
      const check = providerCliCheck(provider);
      if (!check) return "CLI · 检测中";
      return check.found
        ? `CLI · 已认证/已安装${check.version ? ` · ${check.version}` : ""}`
        : "CLI · 未检测到";
    }
    const cred = providerCredential(provider);
    const hasKey = !!cred?.api_key;
    if (provider.id === "glm") {
      const model = cred?.models?.[0] ?? provider.defaultModel ?? "";
      const baseUrl = cred?.base_url ?? provider.defaultBaseUrl ?? "";
      return `API · ${hasKey ? "已配置 Key" : "未配置 Key"} · ${model || "未配置模型"} · ${baseUrl || "未配置 URL"}`;
    }
    return `API · ${hasKey ? "已配置 Key" : "未配置 Key"}`;
  }

  function providerBadgeLabel(provider: Phase7ProviderEntry): string {
    if (provider.mode === "official_cli") return "订阅";
    // mimo-plan card: show OK if either mimo-plan OR mimo-api has a key
    if (provider.id === "mimo-plan") {
      const planKey = providerCredential(provider)?.api_key;
      const apiKey = mimoApiCredential()?.api_key;
      return planKey || apiKey ? "API" : "缺 Key";
    }
    return providerCredential(provider)?.api_key ? "API" : "缺 Key";
  }

  function providerPermissionLabel(provider: Phase7ProviderEntry): string {
    if (provider.defaultPermissionMode === "dangerously_bypass") {
      return "--dangerously-bypass-approvals-and-sandbox";
    }
    if (provider.defaultPermissionMode === "yolo") return "yolo";
    return "bypass";
  }

  function isBetaLocale(entry: { status: string }): boolean {
    return entry.status === "beta";
  }

  function providerFieldRules(provider: Phase7ProviderEntry) {
    if (provider.mode === "official_cli") {
      return {
        showApiKey: false,
        showBaseUrl: false,
        showModel: false,
        modelOptions: null as string[] | null,
        gridClass: "md:grid-cols-1",
      };
    }

    // Look up model options from platform presets (not hardcoded per provider)
    const preset = provider.platformId
      ? PLATFORM_PRESETS.find((p) => p.id === provider.platformId)
      : undefined;
    const modelOptions: string[] | null =
      preset?.models && preset.models.length > 0 ? preset.models : null;

    const showBaseUrl = provider.id !== "deepseek" && provider.id !== "mimo-plan";

    return {
      showApiKey: true,
      showBaseUrl,
      showModel: true,
      modelOptions,
      gridClass: showBaseUrl ? "md:grid-cols-3" : "md:grid-cols-2",
    };
  }

  function updateApiProviderField(
    provider: Phase7ProviderEntry,
    field: "api_key" | "base_url" | "model",
    value: string,
  ) {
    if (!provider.platformId) return;
    const existing = providerCredential(provider);
    const next: PlatformCredential = {
      platform_id: provider.platformId,
      api_key: existing?.api_key,
      base_url: existing?.base_url ?? provider.defaultBaseUrl,
      auth_env_var: existing?.auth_env_var ?? "ANTHROPIC_AUTH_TOKEN",
      name: existing?.name ?? provider.label,
      models: existing?.models ?? (provider.defaultModel ? [provider.defaultModel] : undefined),
      extra_env: existing?.extra_env,
    };
    if (field === "api_key") next.api_key = value || undefined;
    if (field === "base_url") next.base_url = value || provider.defaultBaseUrl || undefined;
    if (field === "model") next.models = value ? [value] : undefined;
    const rest = platformCredentials.filter((cred) => cred.platform_id !== provider.platformId);
    platformCredentials = [...rest, next];
  }

  function persistApiProviderConfig() {
    saveGeneralPatch({ platform_credentials: platformCredentials });
  }

  // ── Custom Provider CRUD ──
  function addCustomProvider() {
    const name = customFormName.trim();
    const url = customFormUrl.trim();
    const key = customFormKey.trim();
    const model = customFormModel.trim();
    if (!name || !url || !key || !model) return;

    // Basic URL validation
    try {
      const parsed = new URL(url);
      if (!["http:", "https:"].includes(parsed.protocol)) throw new Error();
    } catch {
      return;
    }

    // Collision guard: append random suffix if ID already exists
    let platformId = `custom-${Date.now()}`;
    if (platformCredentials.some((c) => c.platform_id === platformId)) {
      platformId = `custom-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    }

    const extraEnv: Record<string, string> = {};
    if (customFormEffort && customFormEffort !== "max") {
      extraEnv.CLAUDE_CODE_EFFORT_LEVEL = customFormEffort;
    }

    const newCred: PlatformCredential = {
      platform_id: platformId,
      api_key: key,
      base_url: url,
      auth_env_var: "ANTHROPIC_AUTH_TOKEN",
      name,
      models: [model],
      extra_env: Object.keys(extraEnv).length ? extraEnv : undefined,
    };

    platformCredentials = [...platformCredentials, newCred];
    persistApiProviderConfig();

    // Reset form
    customFormName = "";
    customFormUrl = "";
    customFormKey = "";
    customFormModel = "";
    customFormEffort = "max";
    customShowApiKey = false;
    showCustomForm = false;
  }

  function removeCustomProvider(platformId: string) {
    platformCredentials = platformCredentials.filter((c) => c.platform_id !== platformId);
    persistApiProviderConfig();
  }

  let customProviderCredentials = $derived(
    platformCredentials.filter((c) => c.platform_id.startsWith("custom-"))
  );

  // ── Collapsible panel state for CC Session providers ──
  let expandedProviderPanels = $state<Record<string, boolean>>({});

  function toggleProviderPanel(providerId: string) {
    expandedProviderPanels[providerId] = !expandedProviderPanels[providerId];
  }

  function updateApiProviderExtraEnv(
    provider: Phase7ProviderEntry,
    envKey: string,
    value: string,
  ) {
    if (!provider.platformId) return;
    const existing = providerCredential(provider);
    const extraEnv = { ...(existing?.extra_env ?? {}) };
    if (value.trim() === "") {
      delete extraEnv[envKey];
    } else {
      extraEnv[envKey] = value;
    }
    const rest = platformCredentials.filter((cred) => cred.platform_id !== provider.platformId);
    platformCredentials = [
      ...rest,
      {
        ...existing,
        platform_id: provider.platformId,
        extra_env: Object.keys(extraEnv).length > 0 ? extraEnv : undefined,
      },
    ];
  }

  async function loadMsvcEnvStatus(cwd?: string) {
    if (!IS_WINDOWS) return;
    msvcEnvLoading = true;
    try {
      msvcEnvStatus = await api.getWindowsMsvcEnvStatus(cwd);
    } catch (e) {
      dbgWarn("settings", "load MSVC env status failed", e);
      msvcEnvStatus = null;
    } finally {
      msvcEnvLoading = false;
    }
  }

  async function saveMsvcEnvMode(mode: WindowsMsvcEnvMode) {
    if (msvcEnvSaving || mode === msvcEnvMode) return;
    msvcEnvMode = mode;
    msvcEnvSaving = true;
    try {
      settings = await api.updateUserSettings({ windows_msvc_env_mode: mode } as Partial<UserSettings>);
      await loadMsvcEnvStatus(settings.working_directory);
    } catch (e) {
      dbgWarn("settings", "save MSVC env mode failed", e);
      msvcEnvMode = settings?.windows_msvc_env_mode ?? "auto";
    } finally {
      msvcEnvSaving = false;
    }
  }

  function msvcEnvStateClass(state?: WindowsMsvcEnvStatus["state"]): string {
    if (state === "injected") return "border-emerald-500/30 bg-emerald-500/5 text-emerald-400";
    if (state === "warning") return "border-amber-500/30 bg-amber-500/5 text-amber-400";
    return "border-border bg-muted/30 text-muted-foreground";
  }

  function msvcEnvModeLabel(mode: WindowsMsvcEnvMode): string {
    if (mode === "always") return t("settings_msvc_mode_always");
    if (mode === "off") return t("settings_msvc_mode_off");
    return t("settings_msvc_mode_auto");
  }

  function msvcEnvStateLabel(state: WindowsMsvcEnvStatus["state"]): string {
    if (state === "disabled") return t("settings_msvc_state_disabled");
    if (state === "non_windows") return t("settings_msvc_state_nonWindows");
    if (state === "not_needed") return t("settings_msvc_state_notNeeded");
    if (state === "injected") return t("settings_msvc_state_injected");
    if (state === "warning") return t("settings_msvc_state_warning");
    return t("settings_msvc_state_pending");
  }

  async function refreshConnectionCliChecks() {
    connectionCliChecking = true;
    const entries = await Promise.all(
      connectionAgentTabs.map(async (tab) => {
        try {
          return [tab.id, await api.checkAgentCli(tab.id)] as const;
        } catch {
          return [tab.id, { agent: tab.id, found: false }] as const;
        }
      }),
    );
    connectionCliChecks = Object.fromEntries(entries) as Record<
      ConnectionAgentTab,
      CliCheckResult | null
    >;
    connectionCliChecking = false;
  }

  // ── Web Server state (desktop-only) ──
  let webToken = $state<string | null>(null);
  let webStatus = $state<{
    enabled: boolean;
    running: boolean;
    port: number;
    bind: string;
    warning?: string;
  } | null>(null);
  let showWebToken = $state(false);
  let webTokenCopied = $state(false);
  let webLinkCopied = $state(false);
  let webRestarting = $state(false);
  let webRestartError = $state<string | null>(null);
  let webRestartWarning = $state<string | null>(null);
  let webPortInput = $state("9476");
  let webOriginInput = $state("");
  let webBindValue = $state("127.0.0.1");
  let webOrigins = $state<string[]>([]);
  let webOriginError = $state<string | null>(null);
  let webAdvancedOpen = $state(false);
  let webLanIp = $state<string | null>(null);
  let webTunnelUrl = $state("");
  let webTunnelError = $state<string | null>(null);
  let githubProxyEnabled = $state(false);
  let githubProxyPort = $state("7890");
  let webTunnelLinkCopied = $state(false);
  let lanIpRequestId = $state(0);

  let debugOn = $state(isDebugMode());
  let logCopied = $state(false);
  let debugFilter = $state(getDebugFilter() || "1");

  // ── UI Zoom state (desktop-only, dynamic import with fallback) ──

  let cachedWebview: any = null;
  async function getWebview() {
    if (!cachedWebview) {
      const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
      cachedWebview = getCurrentWebviewWindow();
    }
    return cachedWebview;
  }

  let zoomPreview = $state(1.0);

  $effect(() => {
    if (settings) {
      zoomPreview = Math.min(1.5, Math.max(0.75, settings.ui_zoom ?? 1.0));
    }
  });

  function clampZoom(v: number): number | null {
    if (!Number.isFinite(v)) return null;
    return Math.min(1.5, Math.max(0.75, v));
  }

  let pendingZoom: number | null = null;
  let zoomFlying = false;

  async function applyZoomQueued(factor: number) {
    if (zoomFlying) {
      pendingZoom = factor;
      return;
    }

    zoomFlying = true;
    try {
      const wv = await getWebview();
      await wv.setZoom(factor);
      dbg("settings", "applyZoomQueued", { factor });
    } catch (e) {
      dbgWarn("settings", "applyZoomQueued failed", e);
    }
    zoomFlying = false;

    if (pendingZoom !== null) {
      const next = pendingZoom;
      pendingZoom = null;
      void applyZoomQueued(next);
    }
  }

  function previewZoom(raw: number) {
    const factor = clampZoom(raw);
    if (factor === null) return;
    zoomPreview = factor;
  }

  let displaySaved = $state(false);

  async function commitZoom(raw: number) {
    const factor = clampZoom(raw);
    if (factor === null) return;

    // Persist
    try {
      settings = await api.updateUserSettings({ ui_zoom: factor });
      dbg("settings", "commitZoom saved", { factor });
      displaySaved = true;
      setTimeout(() => (displaySaved = false), 1500);
    } catch (e) {
      dbgWarn("settings", "commitZoom save failed", e);
      // Rollback to last persisted value
      const fallback = Math.min(1.5, Math.max(0.75, settings?.ui_zoom ?? 1.0));
      zoomPreview = fallback;
      pendingZoom = null;
      void applyZoomQueued(fallback);
      return;
    }

    // Apply final value via queue (overrides any stale preview)
    pendingZoom = null;
    void applyZoomQueued(factor);
  }
  let logCount = $state(getDebugLogCount());
  let rustCmdCopied = $state(false);
  let currentUsername = $state("");

  // ── Remote host state ──
  let remoteHosts = $state<RemoteHost[]>([]);
  let editingRemote = $state<RemoteHost | null>(null);
  let remoteFormName = $state("");
  let remoteFormHost = $state("");
  let remoteFormUser = $state("");
  let remoteFormPort = $state(22);
  let remoteFormKeyPath = $state("");
  let remoteFormRemoteCwd = $state("");
  let remoteFormClaudePath = $state("");
  let remoteFormForwardKey = $state(false);
  let remoteTesting = $state(false);
  let remoteTestResult = $state<RemoteTestResult | null>(null);
  let remoteSaving = $state(false);
  let remoteSaved = $state(false);

  function resetRemoteForm() {
    editingRemote = null;
    remoteFormName = "";
    remoteFormHost = "";
    remoteFormUser = "";
    remoteFormPort = 22;
    remoteFormKeyPath = "";
    remoteFormRemoteCwd = "";
    remoteFormClaudePath = "";
    remoteFormForwardKey = false;
    remoteTestResult = null;
    remoteFormTouched = false;
  }

  function editRemoteHost(host: RemoteHost) {
    editingRemote = host;
    remoteFormName = host.name;
    remoteFormHost = host.host;
    remoteFormUser = host.user;
    remoteFormPort = host.port;
    remoteFormKeyPath = host.key_path ?? "";
    remoteFormRemoteCwd = host.remote_cwd ?? "";
    remoteFormClaudePath = host.remote_claude_path ?? "";
    remoteFormForwardKey = host.forward_api_key;
    remoteTestResult = null;
  }

  async function saveRemoteHost(keepForm = false) {
    if (!remoteFormName.trim() || !remoteFormHost.trim() || !remoteFormUser.trim()) {
      remoteFormTouched = true;
      return;
    }
    remoteSaving = true;
    try {
      const newHost: RemoteHost = {
        name: remoteFormName.trim(),
        host: remoteFormHost.trim(),
        user: remoteFormUser.trim(),
        port: remoteFormPort || 22,
        key_path: remoteFormKeyPath.trim() || undefined,
        remote_cwd: remoteFormRemoteCwd.trim() || undefined,
        remote_claude_path: remoteFormClaudePath.trim() || undefined,
        forward_api_key: remoteFormForwardKey,
      };

      const updated = editingRemote
        ? remoteHosts.map((h) => (h.name === editingRemote!.name ? newHost : h))
        : [...remoteHosts, newHost];

      await api.updateUserSettings({ remote_hosts: updated } as Partial<UserSettings>);
      remoteHosts = updated;
      if (keepForm) {
        // Switch to edit mode so subsequent saves update instead of duplicate
        editingRemote = newHost;
      } else {
        resetRemoteForm();
      }
      remoteSaved = true;
      setTimeout(() => (remoteSaved = false), 2000);
      dbg("settings", "remote host saved", newHost.name);
    } catch (e) {
      dbgWarn("settings", "save remote host failed", e);
    } finally {
      remoteSaving = false;
    }
  }

  async function deleteRemoteHost(name: string) {
    const updated = remoteHosts.filter((h) => h.name !== name);
    try {
      await api.updateUserSettings({ remote_hosts: updated } as Partial<UserSettings>);
      remoteHosts = updated;
      if (editingRemote?.name === name) resetRemoteForm();
      dbg("settings", "remote host deleted", name);
    } catch (e) {
      dbgWarn("settings", "delete remote host failed", e);
    }
  }

  let remoteFormTouched = $state(false);

  async function testRemoteConnection() {
    if (!remoteFormHost.trim() || !remoteFormUser.trim()) {
      remoteFormTouched = true;
      return;
    }
    remoteTesting = true;
    remoteTestResult = null;
    try {
      remoteTestResult = await api.testRemoteHost(
        remoteFormHost.trim(),
        remoteFormUser.trim(),
        remoteFormPort || undefined,
        remoteFormKeyPath.trim() || undefined,
        remoteFormClaudePath.trim() || undefined,
      );
      dbg("settings", "remote test result", remoteTestResult);
      // Auto-save on successful SSH connection (keep form visible for user to review)
      if (remoteTestResult.ssh_ok && remoteFormName && remoteFormHost && remoteFormUser) {
        await saveRemoteHost(true);
      }
    } catch (e) {
      remoteTestResult = { ssh_ok: false, cli_found: false, error: String(e) };
      dbgWarn("settings", "remote test error", e);
    } finally {
      remoteTesting = false;
    }
  }

  // ── SSH Key wizard state ──
  type SshKeyStep =
    | "idle"
    | "checking"
    | "no_key"
    | "has_key"
    | "pub_missing"
    | "generating"
    | "done"
    | "error";
  let sshKeyStep = $state<SshKeyStep>("idle");
  let sshKeyInfo = $state<SshKeyInfo | null>(null);
  let sshKeyError = $state("");
  let sshCopied = $state(false);
  let sshVerifying = $state(false);
  let wizardKeyPath = $derived(sshKeyInfo?.key_path ?? "");

  function shellQuote(s: string): string {
    return "'" + s.replace(/'/g, "'\\''") + "'";
  }

  function pwshQuote(s: string): string {
    return "'" + s.replace(/'/g, "''") + "'";
  }

  function buildCopyCommand(keyInfo: SshKeyInfo, host: string, user: string, port: number): string {
    if (IS_WINDOWS) {
      const pubPath = pwshQuote(keyInfo.key_path_expanded + ".pub");
      const target = pwshQuote(`${user}@${host}`);
      const remoteScript = pwshQuote(
        "mkdir -p ~/.ssh && chmod 700 ~/.ssh && " +
          "touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys && " +
          'key=$(cat) && (grep -qxF "$key" ~/.ssh/authorized_keys 2>/dev/null || ' +
          'echo "$key" >> ~/.ssh/authorized_keys)',
      );
      return `Get-Content -LiteralPath ${pubPath} -Raw | ssh -p ${port} ${target} ${remoteScript}`;
    }
    const keyArg = shellQuote(keyInfo.key_path_expanded);
    const pubArg = shellQuote(keyInfo.key_path_expanded + ".pub");
    const target = `${shellQuote(user)}@${shellQuote(host)}`;

    if (keyInfo.ssh_copy_id_available) {
      return `ssh-copy-id -i ${keyArg} -p ${port} ${target}`;
    }
    const remoteScript =
      "mkdir -p ~/.ssh && chmod 700 ~/.ssh && " +
      "touch ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys && " +
      'key=$(cat) && (grep -qxF "$key" ~/.ssh/authorized_keys 2>/dev/null || ' +
      'echo "$key" >> ~/.ssh/authorized_keys)';
    return `cat ${pubArg} | ssh -p ${port} ${target} ${shellQuote(remoteScript)}`;
  }

  function buildRebuildPubKeyCommand(keyInfo: SshKeyInfo): string {
    if (IS_WINDOWS) {
      const keyPath = pwshQuote(keyInfo.key_path_expanded);
      const pubPath = pwshQuote(keyInfo.key_path_expanded + ".pub");
      return `ssh-keygen -y -f ${keyPath} | Out-File -Encoding ascii ${pubPath}`;
    }
    const keyArg = shellQuote(keyInfo.key_path_expanded);
    return `ssh-keygen -y -f ${keyArg} > ${shellQuote(keyInfo.key_path_expanded + ".pub")}`;
  }

  async function startSshKeyWizard() {
    sshKeyStep = "checking";
    sshKeyError = "";
    sshCopied = false;
    try {
      const info = await api.checkSshKey();
      sshKeyInfo = info;
      dbg("settings", "ssh key check", info);
      if (info.exists && info.pub_exists) {
        sshKeyStep = "has_key";
      } else if (info.exists && !info.pub_exists) {
        sshKeyStep = "pub_missing";
      } else {
        sshKeyStep = "no_key";
      }
    } catch (e) {
      sshKeyError = String(e);
      sshKeyStep = "error";
      dbgWarn("settings", "ssh key check failed", e);
    }
  }

  async function generateSshKey() {
    sshKeyStep = "generating";
    sshKeyError = "";
    try {
      const info = await api.generateSshKey();
      sshKeyInfo = info;
      sshKeyStep = "has_key";
      dbg("settings", "ssh key generated", info);
    } catch (e) {
      sshKeyError = String(e);
      sshKeyStep = "error";
      dbgWarn("settings", "ssh key generation failed", e);
    }
  }

  async function verifySshConnection() {
    if (!sshKeyInfo || !remoteFormHost || !remoteFormUser) return;
    sshVerifying = true;
    try {
      const result = await api.testRemoteHost(
        remoteFormHost.trim(),
        remoteFormUser.trim(),
        remoteFormPort || undefined,
        wizardKeyPath || undefined,
        remoteFormClaudePath.trim() || undefined,
      );
      dbg("settings", "ssh verify result", result);
      if (result.ssh_ok) {
        remoteFormKeyPath = wizardKeyPath;
        sshKeyStep = "done";
      } else {
        sshKeyError = result.error ?? "";
        sshKeyStep = "has_key"; // stay on has_key so user can retry
      }
      remoteTestResult = result;
    } catch (e) {
      sshKeyError = String(e);
      dbgWarn("settings", "ssh verify failed", e);
    } finally {
      sshVerifying = false;
    }
  }

  function closeSshWizard() {
    sshKeyStep = "idle";
    sshKeyError = "";
    sshCopied = false;
    sshVerifying = false;
  }

  // Keybinding store from layout context
  const keybindingStore = getContext<KeybindingStore>("keybindings");
  let cliSectionOpen = $state(false);
  let cliSource = $state<"defaults" | "file">("defaults");

  // Keybinding conflict warning for recording editor
  let recordingConflict = $state("");

  // Derived keybinding groups
  let appBindings = $derived(
    keybindingStore.resolved.filter((b) => b.source === "app" && b.editable),
  );
  let fixedBindings = $derived(
    keybindingStore.resolved.filter((b) => b.source === "app" && !b.editable),
  );
  let cliBindings = $derived(keybindingStore.resolved.filter((b) => b.source === "cli"));
  let hasOverrides = $derived(keybindingStore.overrides.length > 0);

  function isOverridden(command: string): boolean {
    return keybindingStore.overrides.some((o) => o.command === command);
  }

  function getConflictWarning(key: string, context: string, excludeCmd: string): string {
    const conflict = keybindingStore.findConflict(key, context, excludeCmd);
    return conflict ? t("settings_shortcuts_conflictsWith", { label: conflict.label }) : "";
  }

  // ── CLI Config state ──
  let cliConfig = $state<Record<string, unknown>>({});
  let projectCliConfig = $state<Record<string, unknown>>({});
  let cliConfigLoaded = $state(false);
  let cliConfigLoading = $state(false);
  let cliConfigError = $state("");

  // CLI Config setting definitions
  const CLI_CONFIG_SETTINGS: CliConfigSettingDef[] = [
    // Behavior
    {
      key: "thinkingEnabled",
      label: t("settings_cliConfig_thinkingModeLabel"),
      description: t("settings_cliConfig_thinkingModeDesc"),
      group: "behavior",
      type: "boolean",
      default: true,
    },
    {
      key: "fastMode",
      label: t("settings_cliConfig_fastModeLabel"),
      description: t("settings_cliConfig_fastModeDesc"),
      group: "behavior",
      type: "boolean",
      default: false,
    },
    {
      key: "autoCompactEnabled",
      label: t("settings_cliConfig_autoCompactLabel"),
      description: t("settings_cliConfig_autoCompactDesc"),
      group: "behavior",
      type: "boolean",
      default: true,
    },
    {
      key: "fileCheckpointingEnabled",
      label: t("settings_cliConfig_fileCheckpointsLabel"),
      description: t("settings_cliConfig_fileCheckpointsDesc"),
      group: "behavior",
      type: "boolean",
      default: true,
    },
    {
      key: "respectGitignore",
      label: t("settings_cliConfig_respectGitignoreLabel"),
      description: t("settings_cliConfig_respectGitignoreDesc"),
      group: "behavior",
      type: "boolean",
      default: true,
    },
    {
      key: "verbose",
      label: t("settings_cliConfig_verboseLabel"),
      description: t("settings_cliConfig_verboseDesc"),
      group: "behavior",
      type: "boolean",
      default: false,
    },
    {
      key: "defaultPermissionMode",
      label: t("settings_cliConfig_permissionModeLabel"),
      description: t("settings_cliConfig_permissionModeDesc"),
      group: "behavior",
      type: "enum",
      default: undefined,
      options: [
        { value: "default", label: t("settings_cliConfig_optDefault") },
        { value: "plan", label: t("settings_cliConfig_optPlan") },
        { value: "acceptEdits", label: t("settings_cliConfig_optAutoEdit") },
        { value: "bypassPermissions", label: t("settings_cliConfig_optFullAuto") },
      ],
    },
    {
      key: "teammateMode",
      label: t("settings_cliConfig_teammateModeLabel"),
      description: t("settings_cliConfig_teammateModeDesc"),
      group: "behavior",
      type: "enum",
      default: "auto",
      options: [
        { value: "auto", label: t("settings_cliConfig_optAuto") },
        { value: "always", label: t("settings_cliConfig_optAlways") },
        { value: "never", label: t("settings_cliConfig_optNever") },
      ],
    },
    // Appearance
    {
      key: "theme",
      label: t("settings_cliConfig_cliThemeLabel"),
      description: t("settings_cliConfig_cliThemeDesc"),
      group: "appearance",
      type: "enum",
      default: "dark",
      options: [
        { value: "dark", label: t("settings_cliConfig_optDark") },
        { value: "light", label: t("settings_cliConfig_optLight") },
        { value: "light-high-contrast", label: t("settings_cliConfig_optHighContrast") },
      ],
    },
    {
      key: "prefersReducedMotion",
      label: t("settings_cliConfig_reduceMotionLabel"),
      description: t("settings_cliConfig_reduceMotionDesc"),
      group: "appearance",
      type: "boolean",
      default: false,
    },
    {
      key: "language",
      label: t("settings_cliConfig_responseLangLabel"),
      description: t("settings_cliConfig_responseLangDesc"),
      group: "appearance",
      type: "string",
      default: undefined,
    },
    {
      key: "outputStyle",
      label: t("settings_cliConfig_outputStyleLabel"),
      description: t("settings_cliConfig_outputStyleDesc"),
      group: "appearance",
      type: "string",
      default: undefined,
    },
    // Advanced
    {
      key: "autoConnectIde",
      label: t("settings_cliConfig_autoConnectIdeLabel"),
      description: t("settings_cliConfig_autoConnectIdeDesc"),
      group: "advanced",
      type: "boolean",
      default: false,
    },
    {
      key: "promptSuggestionsEnabled",
      label: t("settings_cliConfig_promptSuggestionsLabel"),
      description: t("settings_cliConfig_promptSuggestionsDesc"),
      group: "advanced",
      type: "boolean",
      default: true,
    },
    {
      key: "spinnerTipsEnabled",
      label: t("settings_cliConfig_spinnerTipsLabel"),
      description: t("settings_cliConfig_spinnerTipsDesc"),
      group: "advanced",
      type: "boolean",
      default: true,
    },
    {
      key: "codeDiffFooterEnabled",
      label: t("settings_cliConfig_codeDiffFooterLabel"),
      description: t("settings_cliConfig_codeDiffFooterDesc"),
      group: "advanced",
      type: "boolean",
      default: true,
    },
    {
      key: "prStatusFooterEnabled",
      label: t("settings_cliConfig_prStatusFooterLabel"),
      description: t("settings_cliConfig_prStatusFooterDesc"),
      group: "advanced",
      type: "boolean",
      default: true,
    },
    {
      key: "autoUpdatesChannel",
      label: t("settings_cliConfig_updateChannelLabel"),
      description: t("settings_cliConfig_updateChannelDesc"),
      group: "advanced",
      type: "enum",
      default: undefined,
      options: [
        { value: "latest", label: t("settings_cliConfig_optLatest") },
        { value: "stable", label: t("settings_cliConfig_optStable") },
      ],
    },
    {
      key: "preferredNotifChannel",
      label: t("settings_cliConfig_notifChannelLabel"),
      description: t("settings_cliConfig_notifChannelDesc"),
      group: "advanced",
      type: "enum",
      default: "auto",
      options: [
        { value: "auto", label: t("settings_cliConfig_optAuto") },
        { value: "iterm2", label: t("settings_cliConfig_optIterm2") },
        { value: "terminal_bell", label: t("settings_cliConfig_optTerminalBell") },
      ],
    },
  ];

  const behaviorSettings = CLI_CONFIG_SETTINGS.filter((s) => s.group === "behavior");
  const appearanceSettings = CLI_CONFIG_SETTINGS.filter((s) => s.group === "appearance");
  const advancedSettings = CLI_CONFIG_SETTINGS.filter((s) => s.group === "advanced");

  function getCliConfigValue(key: string, def: CliConfigSettingDef): unknown {
    return key in cliConfig ? cliConfig[key] : def.default;
  }

  function isProjectOverride(key: string): boolean {
    return key in projectCliConfig;
  }

  async function saveCliConfigPatch(key: string, value: unknown) {
    dbg("settings", "saveCliConfigPatch", { key, value });
    try {
      // null value = delete key (restore CLI default)
      cliConfig = await api.updateCliConfig({ [key]: value ?? null });
    } catch (e) {
      dbgWarn("settings", "saveCliConfigPatch error", e);
    }
  }

  async function loadCliConfig() {
    if (cliConfigLoading) return;
    cliConfigLoading = true;
    cliConfigError = "";
    try {
      cliConfig = await api.getCliConfig();
      // Load project config for override indicators
      const cwd = localStorage.getItem("clawgo:project-cwd") || "";
      if (cwd) {
        projectCliConfig = await api.getProjectCliConfig(cwd);
      }
      cliConfigLoaded = true;
      dbg("settings", "cliConfig loaded", {
        keys: Object.keys(cliConfig).length,
        projectKeys: Object.keys(projectCliConfig).length,
      });
    } catch (e) {
      cliConfigError = String(e);
      dbgWarn("settings", "loadCliConfig error", e);
    } finally {
      cliConfigLoading = false;
    }
  }

  // Lazy load CLI config when tab activates
  $effect(() => {
    if (activeTab === "cli-config" && !cliConfigLoaded && !cliConfigLoading) {
      loadCliConfig();
    }
  });

  // Refresh log count periodically when debug is on
  $effect(() => {
    if (!debugOn) return;
    const timer = setInterval(() => {
      logCount = getDebugLogCount();
    }, 2000);
    return () => clearInterval(timer);
  });

  onMount(async () => {
    try {
      settings = await api.getUserSettings();
      remoteHosts = settings.remote_hosts ?? [];
      platformCredentials = settings.platform_credentials ?? [];
      msvcEnvMode = settings.windows_msvc_env_mode ?? "auto";
    } catch (e) {
      dbgWarn("settings", "error", e);
    }
    void refreshConnectionCliChecks();
    void loadMsvcEnvStatus(settings?.working_directory);
    loadMemoryExtractionConfig();
    // Load web server status + token (desktop only)
    if (getTransport().isDesktop()) {
      Promise.all([api.getWebServerStatus(), api.getWebServerToken()])
        .then(async ([status, token]) => {
          webStatus = status;
          webToken = token;
          // Initialize form fields from settings
          webPortInput = String(settings?.web_server_port ?? 9476);
          webBindValue = settings?.web_server_bind ?? "127.0.0.1";
          webOrigins = [...(settings?.web_server_allowed_origins ?? [])];
          webTunnelUrl = settings?.web_server_tunnel_url ?? "";
          githubProxyEnabled = settings?.github_proxy_enabled ?? false;
          githubProxyPort = String(settings?.github_proxy_port ?? 7890);
          dbg("settings", "webServer loaded", {
            enabled: status?.enabled,
            hasToken: !!token,
            tunnel: webTunnelUrl,
          });
          if (status?.running) await refreshLanIp(status.bind);
        })
        .catch((e) => {
          dbgWarn("settings", "webServer load failed", e);
        });
    }
    loadCliInfo();
    // Detect current username + CLI keybindings source
    import("@tauri-apps/api/path")
      .then(async (p) => {
        const home = await p.homeDir();
        const parts = splitPath(home.replace(/[/\\]+$/, ""));
        currentUsername = parts[parts.length - 1] || "";
        const absPath = await p.join(home, ".claude", "keybindings.json");
        return api.readTextFile(absPath);
      })
      .then(() => {
        cliSource = "file";
      })
      .catch(() => {
        cliSource = "defaults";
      });
  });

  async function saveGeneralPatch(patch: Record<string, unknown>) {
    dbg("settings", "saveGeneralPatch", redactSensitive(patch));
    try {
      settings = await api.updateUserSettings(patch as Partial<UserSettings>);
    } catch (e) {
      dbgWarn("settings", "saveGeneralPatch error", e);
    }
  }

  function providerValidation(platformId: string): ProviderValidationResult | null {
    return providerValidationResults[platformId] ?? null;
  }

  async function applyAndValidateProviderConfig() {
    providerValidationApplying = true;
    providerValidationSummary = "";
    try {
      settings = await api.updateUserSettings({ platform_credentials: platformCredentials });
      const response = await api.validatePlatformCredentials(platformCredentials);
      providerValidationResults = Object.fromEntries(
        response.results.map((result) => [result.platformId, result]),
      ) as Record<string, ProviderValidationResult | null>;
      const failed = response.results.filter((result) => !result.ok).length;
      providerValidationSummary = failed
        ? `已保存配置，${failed} 个 provider 仍需补齐设置`
        : "已保存配置，所有第三方 session provider 校验通过";
    } catch (e) {
      providerValidationSummary = "保存或校验失败，请稍后重试";
      dbgWarn("settings", "applyAndValidateProviderConfig error", e);
    } finally {
      providerValidationApplying = false;
    }
  }

  // ── Web Server helpers ──

  async function applyWebServerSettings() {
    webRestarting = true;
    webRestartError = null;
    webRestartWarning = null;
    webTunnelError = null;
    try {
      const portNum = parseInt(webPortInput, 10);
      if (isNaN(portNum) || portNum < 1024 || portNum > 65535) {
        throw new Error(t("settings_general_webPortInvalid"));
      }
      const result = await api.restartWebServer({
        enabled: true,
        port: portNum,
        bind: webBindValue,
        allowed_origins: webOrigins.length > 0 ? webOrigins : null,
        tunnel_url: webTunnelUrl.trim() || null,
      });
      webStatus = await api.getWebServerStatus();
      settings = await api.getUserSettings();
      if (!result.config_saved) {
        webRestartWarning = t("settings_general_webSaveWarning");
      }
      dbg("settings", "webServer apply", { started: result.started, saved: result.config_saved });
      if (webStatus?.running) await refreshLanIp(webStatus.bind);
    } catch (e: unknown) {
      webRestartError = (e as Error)?.message ?? String(e);
      webStatus = await api.getWebServerStatus();
      dbgWarn("settings", "webServer apply failed", e);
    } finally {
      webRestarting = false;
    }
  }

  function addWebOrigin() {
    const trimmed = webOriginInput.trim().replace(/\/+$/, "");
    if (!trimmed) return;
    try {
      const url = new URL(trimmed);
      if (url.protocol !== "http:" && url.protocol !== "https:") {
        webOriginError = t("settings_general_webOriginInvalid");
        return;
      }
      const origin = url.origin;
      if (!webOrigins.includes(origin)) {
        webOrigins = [...webOrigins, origin];
      }
    } catch {
      webOriginError = t("settings_general_webOriginInvalid");
      return;
    }
    webOriginInput = "";
    webOriginError = null;
  }

  async function refreshLanIp(bind: string): Promise<string | null> {
    const myId = ++lanIpRequestId;
    if (bind !== "0.0.0.0" && bind !== "::" && bind !== "[::]") {
      webLanIp = null;
      return null;
    }
    try {
      const preferV6 = bind === "::" || bind === "[::]";
      const ip = await api.getLocalIp(preferV6);
      if (myId !== lanIpRequestId) return webLanIp;
      webLanIp = ip;
      return ip;
    } catch (e) {
      dbgWarn("settings", "refreshLanIp failed", e);
      if (myId !== lanIpRequestId) return webLanIp;
      webLanIp = null;
      return null;
    }
  }

  function buildLocalAccessUrl(): string | null {
    if (!webStatus?.running || !webToken) return null;
    const bind = webStatus.bind;
    const isAll = bind === "0.0.0.0" || bind === "::" || bind === "[::]";
    const rawHost = isAll ? webLanIp : bind;
    if (!rawHost) return null;
    const host = rawHost.includes(":") ? `[${rawHost}]` : rawHost;
    return `http://${host}:${webStatus.port}/login#token=${webToken}`;
  }

  function buildTunnelAccessUrl(): string | null {
    if (!webStatus?.running || !webToken) return null;
    // Use saved (applied) tunnel URL, not the draft input value
    const tunnel = settings?.web_server_tunnel_url?.trim();
    if (!tunnel) return null;
    try {
      const u = new URL(tunnel);
      // Tunnel links use ?token= (server-side auth) to survive ngrok/cloudflared
      // interstitial pages. Local links keep #token= (fragment, never sent to server).
      return `${u.origin}/login?token=${webToken}`;
    } catch {
      return null;
    }
  }

  function buildAccessUrl(): string | null {
    return buildTunnelAccessUrl() ?? buildLocalAccessUrl();
  }

  async function copyAccessLink() {
    const url = buildAccessUrl();
    if (!url) return;
    await navigator.clipboard.writeText(url);
    webLinkCopied = true;
    dbg("settings", "webLink copied");
    setTimeout(() => (webLinkCopied = false), 1500);
  }

  async function openAccessLink() {
    const url = buildAccessUrl();
    if (!url) return;
    try {
      const { open } = await import("@tauri-apps/plugin-shell");
      await open(url);
      dbg("settings", "webLink opened in browser");
    } catch (e) {
      dbgWarn("settings", "failed to open browser", e);
    }
  }
</script>

{#key currentLocale()}
  <div class="max-w-4xl mx-auto p-6 animate-slide-up">
    <h1 class="text-2xl font-bold mb-5">{t("settings_title")}</h1>

    <!-- Tab bar -->
    <div class="flex gap-1 border-b border-border mb-6">
      {#each tabs as tab (tab.id)}
        <button
          class="flex items-center gap-1.5 px-4 py-2.5 text-sm font-medium transition-colors relative
          {activeTab === tab.id
            ? 'text-foreground'
            : 'text-muted-foreground hover:text-foreground/80'}"
          onclick={() => (activeTab = tab.id)}
        >
          <svg
            class="h-3.5 w-3.5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d={tab.icon} />
          </svg>
          {tabLabels[tab.id]()}
          {#if activeTab === tab.id}
            <span class="absolute bottom-0 left-2 right-2 h-0.5 bg-primary rounded-full"></span>
          {/if}
        </button>
      {/each}
    </div>

    <!-- ═══ General tab ═══ -->
    {#if activeTab === "general"}
      <div class="space-y-6">
        <!-- Language Card -->
        <Card class="p-6 space-y-4">
          <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
            {t("settings_general_language")}
          </h2>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm font-medium">{t("settings_general_displayLanguage")}</p>
              <p class="text-xs text-muted-foreground">
                {t("settings_general_displayLanguageDesc")}
              </p>
            </div>
            <div class="flex gap-1.5">
              {#each LOCALE_REGISTRY as entry}
                <button
                  class="rounded-md border px-3 py-1.5 text-xs transition-all duration-150
                  {currentLocale() === entry.code
                    ? 'bg-primary text-primary-foreground'
                    : isBetaLocale(entry)
                      ? 'border-muted-foreground/30 text-muted-foreground hover:bg-accent'
                      : 'hover:bg-accent'}"
                  onclick={() => switchLocale(entry.code)}
                >
                  {entry.nativeName}{#if isBetaLocale(entry)}<span
                      class="ml-1 text-[10px] opacity-60">(Beta)</span
                    >{/if}
                </button>
              {/each}
            </div>
          </div>
        </Card>

        <!-- Display Card -->
        <Card class="p-6 space-y-4">
          <div class="flex items-center justify-between">
            <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
              {t("settings_general_display")}
            </h2>
            {#if displaySaved}
              <span class="text-xs text-emerald-500 flex items-center gap-1 animate-fade-in">
                <svg
                  class="h-3 w-3"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
                >
                {t("settings_general_saved")}
              </span>
            {/if}
          </div>
          <div class="flex items-center justify-between gap-4">
            <div>
              <p class="text-sm font-medium">{t("settings_general_uiZoom")}</p>
              <p class="text-xs text-muted-foreground">{t("settings_general_uiZoomDesc")}</p>
            </div>
            <div class="flex items-center gap-3">
              <input
                type="range"
                min="0.75"
                max="1.5"
                step="0.05"
                value={zoomPreview}
                class="w-28 accent-primary"
                oninput={(e) => previewZoom(parseFloat((e.target as HTMLInputElement).value))}
                onchange={(e) => commitZoom(parseFloat((e.target as HTMLInputElement).value))}
              />
              <span class="text-xs text-muted-foreground w-10 text-right">
                {Math.round(zoomPreview * 100)}%
              </span>
            </div>
          </div>
        </Card>

        <!-- GitHub Update Proxy Card (desktop only) -->
        {#if getTransport().isDesktop()}
          <Card class="p-6 space-y-4">
            <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
              {t("settings_general_githubProxy")}
            </h2>

            <!-- Enabled toggle -->
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm font-medium">{t("settings_general_githubProxy")}</p>
                <p class="text-xs text-muted-foreground">{t("settings_general_githubProxyDesc")}</p>
              </div>
              <button
                role="switch"
                aria-checked={githubProxyEnabled}
                class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors {githubProxyEnabled
                  ? 'bg-primary'
                  : 'bg-muted'}"
                onclick={async () => {
                  const prev = githubProxyEnabled;
                  const newValue = !prev;
                  githubProxyEnabled = newValue;
                  try {
                    await saveGeneralPatch({ github_proxy_enabled: newValue });
                  } catch {
                    githubProxyEnabled = prev;
                  }
                }}
              >
                <span
                  class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform {githubProxyEnabled
                    ? 'translate-x-6'
                    : 'translate-x-1'}"
                ></span>
              </button>
            </div>

            <!-- Port input (show when enabled) -->
            {#if githubProxyEnabled}
              <div class="flex items-center gap-3">
                <label class="text-sm text-muted-foreground" for="github-proxy-port">
                  {t("settings_general_githubProxyPort")}
                </label>
                <input
                  id="github-proxy-port"
                  type="number"
                  min="1"
                  max="65535"
                  class="w-24 rounded-md border bg-background px-3 py-1.5 text-sm"
                  bind:value={githubProxyPort}
                  onblur={async () => {
                    const port = parseInt(githubProxyPort, 10);
                    if (!isNaN(port) && port >= 1 && port <= 65535) {
                      const prev = githubProxyPort;
                      try {
                        await saveGeneralPatch({ github_proxy_port: port });
                      } catch {
                        githubProxyPort = prev;
                      }
                    } else {
                      githubProxyPort = String(settings?.github_proxy_port ?? 7890);
                    }
                  }}
                />
                <span class="text-xs text-muted-foreground">
                  http://127.0.0.1:{githubProxyPort}
                </span>
              </div>
            {/if}
          </Card>
        {/if}

        <!-- Web Server Card (desktop only) -->
        {#if getTransport().isDesktop()}
          <Card class="p-6 space-y-4">
            <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
              {t("settings_general_webServer")}
            </h2>

            <!-- Enabled toggle -->
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm font-medium">{t("settings_general_webEnabled")}</p>
                <p class="text-xs text-muted-foreground">{t("settings_general_webEnabledDesc")}</p>
              </div>
              <button
                class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors {webStatus?.enabled
                  ? 'bg-primary'
                  : 'bg-muted'}"
                disabled={webRestarting}
                onclick={async () => {
                  const newEnabled = !webStatus?.enabled;
                  webRestarting = true;
                  webRestartError = null;
                  webRestartWarning = null;
                  try {
                    if (newEnabled) {
                      const portNum = parseInt(webPortInput, 10);
                      if (isNaN(portNum) || portNum < 1024 || portNum > 65535) {
                        throw new Error(t("settings_general_webPortInvalid"));
                      }
                      const result = await api.restartWebServer({
                        enabled: true,
                        port: portNum,
                        bind: webBindValue,
                        allowed_origins: webOrigins.length > 0 ? webOrigins : null,
                        tunnel_url: webTunnelUrl.trim() || null,
                      });
                      if (!result.config_saved) {
                        webRestartWarning = t("settings_general_webSaveWarning");
                      }
                    } else {
                      await api.restartWebServer({
                        enabled: false,
                        port: 0,
                        bind: "",
                        allowed_origins: null,
                        tunnel_url: null,
                      });
                    }
                    webStatus = await api.getWebServerStatus();
                    settings = await api.getUserSettings();
                    dbg("settings", "webServer toggled", { enabled: newEnabled });
                    if (webStatus?.running) await refreshLanIp(webStatus.bind);
                  } catch (e) {
                    webRestartError = (e as Error)?.message ?? String(e);
                    webStatus = await api.getWebServerStatus();
                    dbgWarn("settings", "webServer toggle failed", e);
                  } finally {
                    webRestarting = false;
                  }
                }}
              >
                <span
                  class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform {webStatus?.enabled
                    ? 'translate-x-6'
                    : 'translate-x-1'}"
                ></span>
              </button>
            </div>

            <!-- Config area (show when enabled OR running) -->
            {#if webStatus?.enabled || webStatus?.running}
              <!-- Startup warning banner -->
              {#if webStatus?.warning}
                <div class="rounded-md border border-amber-500/30 bg-amber-500/5 px-3 py-2">
                  <p class="text-xs text-amber-400 whitespace-pre-line">
                    {t("settings_general_webStartupWarning", { warning: webStatus.warning })}
                  </p>
                </div>
              {/if}

              <!-- Access link + token (only when running) -->
              {#if webStatus?.running && webToken}
                {@const isAllInterfaces =
                  webStatus.bind === "0.0.0.0" ||
                  webStatus.bind === "::" ||
                  webStatus.bind === "[::]"}
                {@const rawHost = isAllInterfaces ? webLanIp : webStatus.bind}
                {@const displayHost = rawHost
                  ? rawHost.includes(":")
                    ? `[${rawHost}]`
                    : rawHost
                  : null}
                {@const tunnelUrl = buildTunnelAccessUrl()}
                {@const localUrl = buildLocalAccessUrl()}
                <div class="space-y-2">
                  {#if tunnelUrl}
                    <!-- Tunnel link (primary) -->
                    <div class="flex items-center gap-2">
                      <span class="text-xs text-muted-foreground shrink-0"
                        >{t("settings_general_webTunnelLink")}</span
                      >
                      <code
                        class="flex-1 rounded-md border bg-muted/50 px-3 py-1.5 font-mono text-xs overflow-hidden text-ellipsis whitespace-nowrap"
                        >{tunnelUrl.replace(/[?#]token=.*$/, "?token=...")}</code
                      >
                      <button
                        class="rounded-md border px-3 py-1.5 text-xs text-muted-foreground hover:bg-accent transition-colors shrink-0"
                        onclick={async () => {
                          await navigator.clipboard.writeText(tunnelUrl);
                          webTunnelLinkCopied = true;
                          dbg("settings", "tunnelLink copied");
                          setTimeout(() => (webTunnelLinkCopied = false), 1500);
                        }}
                      >
                        {webTunnelLinkCopied
                          ? t("settings_general_webCopied")
                          : t("settings_general_webCopyLink")}
                      </button>
                      <button
                        class="rounded-md border px-3 py-1.5 text-xs text-muted-foreground hover:bg-accent transition-colors shrink-0"
                        onclick={async () => {
                          try {
                            const { open } = await import("@tauri-apps/plugin-shell");
                            await open(tunnelUrl);
                            dbg("settings", "tunnelLink opened in browser");
                          } catch (e) {
                            dbgWarn("settings", "failed to open browser", e);
                          }
                        }}
                      >
                        {t("settings_general_webOpenBrowser")}
                      </button>
                    </div>
                    <!-- Local link (secondary, muted) -->
                    {#if displayHost && localUrl}
                      <div class="flex items-center gap-2">
                        <span class="text-xs text-muted-foreground shrink-0"
                          >{t("settings_general_webLocalLink")}</span
                        >
                        <code
                          class="flex-1 rounded-md border bg-muted/30 px-3 py-1.5 font-mono text-xs text-muted-foreground overflow-hidden text-ellipsis whitespace-nowrap"
                          >{localUrl.replace(/#token=.*$/, "#token=...")}</code
                        >
                        <button
                          class="rounded-md border px-3 py-1.5 text-xs text-muted-foreground hover:bg-accent transition-colors shrink-0"
                          onclick={async () => {
                            if (localUrl) {
                              await navigator.clipboard.writeText(localUrl);
                              webLinkCopied = true;
                              dbg("settings", "localLink copied");
                              setTimeout(() => (webLinkCopied = false), 1500);
                            }
                          }}
                        >
                          {webLinkCopied
                            ? t("settings_general_webCopied")
                            : t("settings_general_webCopyLink")}
                        </button>
                      </div>
                    {/if}
                  {:else if displayHost}
                    <div class="flex items-center gap-2">
                      <code
                        class="flex-1 rounded-md border bg-muted/50 px-3 py-1.5 font-mono text-xs overflow-hidden text-ellipsis whitespace-nowrap"
                        >{`http://${displayHost}:${webStatus.port}/login#token=...`}</code
                      >
                      <button
                        class="rounded-md border px-3 py-1.5 text-xs text-muted-foreground hover:bg-accent transition-colors shrink-0"
                        onclick={copyAccessLink}
                      >
                        {webLinkCopied
                          ? t("settings_general_webCopied")
                          : t("settings_general_webCopyLink")}
                      </button>
                      <button
                        class="rounded-md border px-3 py-1.5 text-xs text-muted-foreground hover:bg-accent transition-colors shrink-0"
                        onclick={openAccessLink}
                      >
                        {t("settings_general_webOpenBrowser")}
                      </button>
                    </div>
                  {:else if isAllInterfaces}
                    <p class="text-xs text-amber-400">
                      {t("settings_general_webLanIpFailed")}
                    </p>
                  {/if}
                  <!-- Token reveal + regenerate -->
                  <div class="flex items-center gap-3 text-xs text-muted-foreground">
                    {#if showWebToken}
                      <code class="font-mono text-[11px] select-all">{webToken}</code>
                      <button
                        class="hover:text-foreground transition-colors shrink-0"
                        onclick={() => (showWebToken = false)}
                      >
                        {t("settings_general_hide")}
                      </button>
                      <button
                        class="hover:text-foreground transition-colors shrink-0"
                        onclick={async () => {
                          if (webToken) {
                            await navigator.clipboard.writeText(webToken);
                            webTokenCopied = true;
                            dbg("settings", "webToken copied");
                            setTimeout(() => (webTokenCopied = false), 1500);
                          }
                        }}
                      >
                        {webTokenCopied
                          ? t("settings_general_webCopied")
                          : t("settings_general_webCopy")}
                      </button>
                    {:else}
                      <button
                        class="hover:text-foreground transition-colors"
                        onclick={() => (showWebToken = true)}
                      >
                        {t("settings_general_webShowToken")}
                      </button>
                    {/if}
                    <span class="text-border">|</span>
                    <button
                      class="text-amber-400/70 hover:text-amber-400 transition-colors"
                      onclick={async () => {
                        try {
                          const newToken = await api.regenerateWebServerToken();
                          webToken = newToken;
                          showWebToken = false;
                          webTokenCopied = false;
                          webLinkCopied = false;
                          dbg("settings", "webToken regenerated");
                        } catch (e) {
                          dbgWarn("settings", "webToken regenerate failed", e);
                        }
                      }}
                    >
                      {t("settings_general_webRegenerate")}
                    </button>
                    <span class="text-muted-foreground">—</span>
                    <span class="text-muted-foreground"
                      >{t("settings_general_webRegenerateDesc")}</span
                    >
                  </div>
                </div>
              {/if}

              <!-- HTTP Tunnel -->
              <div>
                <p class="text-sm font-medium mb-1.5">{t("settings_general_webTunnel")}</p>
                <input
                  type="text"
                  class="w-full rounded-md border bg-background px-3 py-1.5 text-sm"
                  placeholder={t("settings_general_webTunnelPlaceholder")}
                  bind:value={webTunnelUrl}
                  onblur={() => {
                    const v = webTunnelUrl.trim();
                    if (v) {
                      try {
                        const u = new URL(v);
                        if (u.protocol !== "http:" && u.protocol !== "https:") {
                          webTunnelError = t("settings_general_webTunnelInvalid");
                        } else {
                          webTunnelError = null;
                        }
                      } catch {
                        webTunnelError = t("settings_general_webTunnelInvalid");
                      }
                    } else {
                      webTunnelError = null;
                    }
                  }}
                />
                {#if webTunnelError}
                  <p class="text-xs text-red-400 mt-1">{webTunnelError}</p>
                {:else}
                  <p class="text-xs text-muted-foreground mt-1">
                    {t("settings_general_webTunnelDesc")}
                  </p>
                {/if}
              </div>

              <!-- Access + Port — side by side -->
              <div class="grid grid-cols-[1fr_auto] gap-4 items-start">
                <div>
                  <p class="text-sm font-medium mb-1.5">{t("settings_general_webAccess")}</p>
                  <div class="flex gap-2">
                    <button
                      class="flex-1 rounded-md border px-3 py-2 text-[13px] transition-colors {webBindValue ===
                      '127.0.0.1'
                        ? 'border-primary bg-primary/10 text-primary'
                        : 'text-muted-foreground hover:bg-accent'}"
                      onclick={() => (webBindValue = "127.0.0.1")}
                    >
                      {t("settings_general_webAccessLocal")}
                    </button>
                    <button
                      class="flex-1 rounded-md border px-3 py-2 text-[13px] transition-colors {webBindValue ===
                      '0.0.0.0'
                        ? 'border-primary bg-primary/10 text-primary'
                        : 'text-muted-foreground hover:bg-accent'}"
                      onclick={() => (webBindValue = "0.0.0.0")}
                    >
                      {t("settings_general_webAccessLan")}
                    </button>
                  </div>
                  <p class="text-xs text-muted-foreground mt-1">
                    {t("settings_general_webAccessDesc")}
                  </p>
                </div>
                <div>
                  <p class="text-sm font-medium mb-1.5">{t("settings_general_webPort")}</p>
                  <input
                    type="number"
                    class="w-24 rounded-md border bg-background px-3 py-1.5 text-sm"
                    bind:value={webPortInput}
                    min="1024"
                    max="65535"
                    onblur={() => {
                      const n = parseInt(webPortInput, 10);
                      if (isNaN(n) || n < 1024 || n > 65535) {
                        webRestartError = t("settings_general_webPortInvalid");
                      } else {
                        if (webRestartError === t("settings_general_webPortInvalid")) {
                          webRestartError = null;
                        }
                      }
                    }}
                  />
                </div>
              </div>

              <!-- Advanced (collapsible) -->
              <div>
                <button
                  class="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
                  onclick={() => (webAdvancedOpen = !webAdvancedOpen)}
                >
                  <svg
                    class="h-3 w-3 transition-transform {webAdvancedOpen ? 'rotate-90' : ''}"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"><path d="m9 18 6-6-6-6" /></svg
                  >
                  {t("settings_general_webAdvanced")}
                </button>

                {#if webAdvancedOpen}
                  <div class="mt-3 space-y-2">
                    <p class="text-sm font-medium">{t("settings_general_webAllowedOrigins")}</p>
                    {#if webOrigins.length > 0}
                      <div class="flex flex-wrap gap-1.5">
                        {#each webOrigins as origin, i}
                          <span
                            class="inline-flex items-center gap-1 rounded-full border bg-muted/50 px-2.5 py-0.5 text-xs"
                          >
                            {origin}
                            <button
                              class="text-muted-foreground hover:text-foreground"
                              onclick={() => {
                                webOrigins = webOrigins.filter((_, idx) => idx !== i);
                              }}
                            >
                              <svg
                                class="h-3 w-3"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"><path d="M18 6L6 18M6 6l12 12" /></svg
                              >
                            </button>
                          </span>
                        {/each}
                      </div>
                    {/if}
                    <div class="flex gap-2">
                      <input
                        type="text"
                        class="flex-1 rounded-md border bg-background px-3 py-1.5 text-sm"
                        placeholder={t("settings_general_webAllowedOriginsPlaceholder")}
                        bind:value={webOriginInput}
                        onkeydown={(e) => {
                          if (e.key === "Enter") {
                            e.preventDefault();
                            addWebOrigin();
                          }
                        }}
                      />
                      <button
                        class="rounded-md border px-3 py-1.5 text-xs text-muted-foreground hover:bg-accent transition-colors shrink-0"
                        onclick={addWebOrigin}
                      >
                        {t("settings_general_webAddOrigin")}
                      </button>
                    </div>
                    {#if webOriginError}
                      <p class="text-xs text-red-400">{webOriginError}</p>
                    {/if}
                    <p class="text-xs text-muted-foreground">
                      {t("settings_general_webAllowedOriginsDesc")}
                    </p>
                  </div>
                {/if}
              </div>

              <!-- Apply + feedback -->
              <div class="space-y-2 pt-2 border-t border-border">
                {#if webRestartError}
                  <p class="text-xs text-red-400">
                    {t("settings_general_webRestartFailed", { error: webRestartError })}
                  </p>
                {/if}
                {#if webRestartWarning}
                  <p class="text-xs text-amber-400">{webRestartWarning}</p>
                {/if}
                <button
                  class="rounded-md border border-primary px-4 py-2 text-sm font-medium text-primary hover:bg-primary/10 transition-colors disabled:opacity-50"
                  disabled={webRestarting}
                  onclick={applyWebServerSettings}
                >
                  {#if webRestarting}
                    <span class="inline-flex items-center gap-2">
                      <span
                        class="h-3.5 w-3.5 animate-spin rounded-full border-2 border-primary border-t-transparent"
                      ></span>
                      {t("settings_general_webApplying")}
                    </span>
                  {:else}
                    {t("settings_general_webApply")}
                  {/if}
                </button>
              </div>
            {:else}
              <p class="text-sm text-muted-foreground">
                {t("settings_general_webDisabled")}
              </p>
            {/if}
          </Card>
        {/if}

        <!-- Tushare Token Card -->
        <Card class="p-6 space-y-4">
          <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
            {t("settings_tushare_token")}
          </h2>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm font-medium">{t("settings_tushare_token")}</p>
              <p class="text-xs text-muted-foreground">{t("invest_no_token")}</p>
            </div>
            <div class="flex items-center gap-2">
              <input
                type="password"
                class="w-64 rounded-md border bg-background px-3 py-1.5 text-sm"
                placeholder="Tushare Pro token"
                value={settings?.tushare_token ?? ""}
                onblur={async (e) => {
                  const val = (e.currentTarget as HTMLInputElement).value.trim();
                  const prev = settings?.tushare_token ?? "";
                  if (val !== prev) {
                    await saveGeneralPatch({ tushare_token: val || undefined });
                  }
                }}
              />
            </div>
          </div>
        </Card>
      </div>

      <!-- ═══ Connection tab ═══ -->
    {:else if activeTab === "connection"}
      <div class="space-y-4">
        <Card class="p-6 space-y-4">
          <div class="flex items-center justify-between gap-3">
            <div>
              <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
                AI 模型
              </h2>
              <p class="mt-1 text-xs text-muted-foreground">
                订阅 CLI 使用官方登录；DeepSeek 和 GLM 通过 Claude Code 兼容 API 配置启动。
              </p>
              {#if providerValidationSummary}
                <p class="mt-2 text-xs text-muted-foreground">{providerValidationSummary}</p>
              {/if}
            </div>
            <div class="flex items-center gap-2">
              <Button
                size="sm"
                variant="outline"
                disabled={providerValidationApplying}
                onclick={applyAndValidateProviderConfig}
              >
                {providerValidationApplying ? "校验中" : "应用并校验配置"}
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={connectionCliChecking}
                onclick={refreshConnectionCliChecks}
              >
                {connectionCliChecking ? "检测中" : "重新检测 CLI"}
              </Button>
            </div>
          </div>

          <div class="divide-y divide-border rounded-md border border-border">
            {#each PHASE7_PROVIDERS.filter((p) => p.id !== "mimo-api") as provider}
              {@const check = providerCliCheck(provider)}
              {@const credential = providerCredential(provider)}

              {#if provider.id === "mimo-plan"}
                <!-- ── Xiaomi dual-row card (Api + Tokenplan) ── -->
                {@const mimoApiCred = mimoApiCredential()}
                <div class="grid gap-3 p-4 md:grid-cols-[minmax(120px,1fr)_minmax(220px,3fr)_auto] md:items-center">
                  <!-- Left: provider name + badge -->
                  <div class="min-w-0 flex flex-col justify-center">
                    <div class="flex items-center gap-2">
                      <span class="text-sm font-medium">{provider.label}</span>
                      <span
                        class="rounded border px-1.5 py-0.5 text-[10px] font-medium {providerBadgeLabel(provider) === '缺 Key'
                          ? 'border-amber-500/40 text-amber-500'
                          : 'border-emerald-500/40 text-emerald-500'}"
                      >
                        {providerBadgeLabel(provider)}
                      </span>
                    </div>
                    <p class="mt-1 truncate text-xs text-muted-foreground">
                      {providerStatusLabel(provider)}
                    </p>
                    {#if providerValidation("mimo-plan")}
                      <div class="mt-2 space-y-1 text-[11px]">
                        <div class:text-emerald-500={providerValidation("mimo-plan")?.ok}
                             class:text-amber-500={!providerValidation("mimo-plan")?.ok}>
                          Token Plan · {providerValidation("mimo-plan")?.ok ? "配置校验通过" : providerValidation("mimo-plan")?.message}
                        </div>
                        {#if !providerValidation("mimo-plan")?.ok}
                          <ul class="list-inside list-disc text-muted-foreground">
                            {#each providerValidation("mimo-plan")?.issues ?? [] as issue}
                              <li>{issue.field}</li>
                            {/each}
                          </ul>
                        {/if}
                      </div>
                    {/if}
                    {#if providerValidation("mimo-api")}
                      <div class="mt-2 space-y-1 text-[11px]">
                        <div class:text-emerald-500={providerValidation("mimo-api")?.ok}
                             class:text-amber-500={!providerValidation("mimo-api")?.ok}>
                          API · {providerValidation("mimo-api")?.ok ? "配置校验通过" : providerValidation("mimo-api")?.message}
                        </div>
                        {#if !providerValidation("mimo-api")?.ok}
                          <ul class="list-inside list-disc text-muted-foreground">
                            {#each providerValidation("mimo-api")?.issues ?? [] as issue}
                              <li>{issue.field}</li>
                            {/each}
                          </ul>
                        {/if}
                      </div>
                    {/if}
                  </div>
                  <!-- Middle: two input rows with labels -->
                  <div class="space-y-2">
                    <div class="flex items-center gap-2">
                      <span class="text-xs text-muted-foreground w-20 shrink-0 text-right">Api</span>
                      <Input
                        type={showApiKey ? "text" : "password"}
                        placeholder="Xiaomi API Key (按量付费)"
                        value={mimoApiCred?.api_key ?? ""}
                        oninput={(event) =>
                          updateMimoApiKey((event.currentTarget as HTMLInputElement).value)}
                        onblur={persistApiProviderConfig}
                      />
                    </div>
                    <div class="flex items-center gap-2">
                      <span class="text-xs text-muted-foreground w-20 shrink-0 text-right">Tokenplan</span>
                      <Input
                        type={showApiKey ? "text" : "password"}
                        placeholder="Xiaomi Token Plan Key (订阅)"
                        value={credential?.api_key ?? ""}
                        oninput={(event) =>
                          updateApiProviderField(provider, "api_key", (event.currentTarget as HTMLInputElement).value)}
                        onblur={persistApiProviderConfig}
                      />
                    </div>
                  </div>
                  <!-- Right: expand toggle -->
                  <button
                    type="button"
                    class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border text-muted-foreground hover:bg-accent transition-colors"
                    title={expandedProviderPanels[provider.id] ? "收起配置" : "展开配置"}
                    onclick={() => toggleProviderPanel(provider.id)}
                  >
                    <svg
                      class="h-4 w-4 transition-transform {expandedProviderPanels[provider.id] ? 'rotate-180' : ''}"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="2"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                    >
                      <path d="m6 9 6 6 6-6" />
                    </svg>
                  </button>
                </div>
                <!-- Expanded panel: model config for mimo-plan + mimo-api -->
                {#if expandedProviderPanels[provider.id]}
                  {@const [tierOpus, tierSonnet, tierHaiku] = expandModelsToTiers(credential?.models ?? [])}
                  {@const sharedExtraEnv = resolveMimoSharedExtraEnv()}
                  <div class="mx-4 mb-4 grid grid-cols-2 gap-2 rounded-md border border-border/50 p-3">
                    <div class="col-span-2 text-[11px] font-medium text-muted-foreground">共用模型配置（同时应用到 Token Plan / API）</div>
                    <div class="space-y-1">
                      <label class="text-[11px] text-muted-foreground" for="mimo-plan-model">ANTHROPIC_MODEL</label>
                      <input
                        id="mimo-plan-model"
                        class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                        placeholder={tierOpus || provider.defaultModel || "自动"}
                        value={sharedExtraEnv.ANTHROPIC_MODEL ?? ""}
                        oninput={(event) =>
                          updateMimoSharedExtraEnv("ANTHROPIC_MODEL", (event.currentTarget as HTMLInputElement).value)}
                        onblur={persistApiProviderConfig}
                      />
                    </div>
                    <div class="space-y-1">
                      <label class="text-[11px] text-muted-foreground" for="mimo-plan-opus">OPUS_MODEL</label>
                      <input
                        id="mimo-plan-opus"
                        class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                        placeholder={tierOpus || provider.defaultModel || "自动"}
                        value={sharedExtraEnv.ANTHROPIC_DEFAULT_OPUS_MODEL ?? ""}
                        oninput={(event) =>
                          updateMimoSharedExtraEnv("ANTHROPIC_DEFAULT_OPUS_MODEL", (event.currentTarget as HTMLInputElement).value)}
                        onblur={persistApiProviderConfig}
                      />
                    </div>
                    <div class="space-y-1">
                      <label class="text-[11px] text-muted-foreground" for="mimo-plan-sonnet">SONNET_MODEL</label>
                      <input
                        id="mimo-plan-sonnet"
                        class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                        placeholder={tierSonnet || tierOpus || provider.defaultModel || "自动"}
                        value={sharedExtraEnv.ANTHROPIC_DEFAULT_SONNET_MODEL ?? ""}
                        oninput={(event) =>
                          updateMimoSharedExtraEnv("ANTHROPIC_DEFAULT_SONNET_MODEL", (event.currentTarget as HTMLInputElement).value)}
                        onblur={persistApiProviderConfig}
                      />
                    </div>
                    <div class="space-y-1">
                      <label class="text-[11px] text-muted-foreground" for="mimo-plan-haiku">HAIKU_MODEL</label>
                      <input
                        id="mimo-plan-haiku"
                        class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                        placeholder={tierHaiku || tierOpus || provider.defaultModel || "自动"}
                        value={sharedExtraEnv.ANTHROPIC_DEFAULT_HAIKU_MODEL ?? ""}
                        oninput={(event) =>
                          updateMimoSharedExtraEnv("ANTHROPIC_DEFAULT_HAIKU_MODEL", (event.currentTarget as HTMLInputElement).value)}
                        onblur={persistApiProviderConfig}
                      />
                    </div>
                    <div class="space-y-1">
                      <label class="text-[11px] text-muted-foreground" for="mimo-plan-subagent">SUBAGENT_MODEL</label>
                      <input
                        id="mimo-plan-subagent"
                        class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                        placeholder={tierOpus || provider.defaultModel || "自动"}
                        value={sharedExtraEnv.CLAUDE_CODE_SUBAGENT_MODEL ?? ""}
                        oninput={(event) =>
                          updateMimoSharedExtraEnv("CLAUDE_CODE_SUBAGENT_MODEL", (event.currentTarget as HTMLInputElement).value)}
                        onblur={persistApiProviderConfig}
                      />
                    </div>
                    <div class="space-y-1">
                      <label class="text-[11px] text-muted-foreground" for="mimo-plan-effort">EFFORT_LEVEL</label>
                      <select
                        id="mimo-plan-effort"
                        class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                        value={sharedExtraEnv.CLAUDE_CODE_EFFORT_LEVEL ?? ""}
                        onchange={(event) =>
                          updateMimoSharedExtraEnv("CLAUDE_CODE_EFFORT_LEVEL", (event.currentTarget as HTMLSelectElement).value)}
                        onblur={persistApiProviderConfig}
                      >
                        <option value="">默认 (max)</option>
                        <option value="low">low</option>
                        <option value="medium">medium</option>
                        <option value="high">high</option>
                        <option value="max">max</option>
                      </select>
                    </div>
                    <div class="col-span-2 mt-2 border-t border-border/50 pt-2 text-[11px] text-muted-foreground">Key 与 base_url 仍分别保存在 Token Plan / API 各自配置中，只有模型配置共用并双写。</div>
                  </div>
                {/if}
              {:else}
                <!-- ── Standard provider card ── -->
                <div
                  class="grid gap-3 p-4 md:grid-cols-[minmax(160px,1fr)_minmax(220px,2fr)_auto] md:items-center"
                >
                  <div class="min-w-0">
                    <div class="flex items-center gap-2">
                      <span class="text-sm font-medium">{provider.label}</span>
                      <span
                        class="rounded border px-1.5 py-0.5 text-[10px] font-medium {providerBadgeLabel(
                          provider,
                        ) === '缺 Key'
                          ? 'border-amber-500/40 text-amber-500'
                          : 'border-emerald-500/40 text-emerald-500'}"
                      >
                        {providerBadgeLabel(provider)}
                      </span>
                    </div>
                    <div class="mt-1 text-xs text-muted-foreground">
                      <div class="truncate">{providerStatusLabel(provider)}</div>
                      {#if provider.id === "deepseek"}
                        <div class="mt-1">需显式填写完整模型配置后才可启动。</div>
                      {/if}
                    </div>
                    {#if provider.platformId && providerValidation(provider.platformId)}
                      <div class="mt-2 space-y-1 text-[11px]">
                        <div class:text-emerald-500={providerValidation(provider.platformId)?.ok}
                             class:text-amber-500={!providerValidation(provider.platformId)?.ok}>
                          {providerValidation(provider.platformId)?.ok
                            ? "配置校验通过"
                            : providerValidation(provider.platformId)?.message}
                        </div>
                        {#if !providerValidation(provider.platformId)?.ok}
                          <ul class="list-inside list-disc text-muted-foreground">
                            {#each providerValidation(provider.platformId)?.issues ?? [] as issue}
                              <li>{issue.field}</li>
                            {/each}
                          </ul>
                        {/if}
                      </div>
                    {/if}
                  </div>

                  {#if provider.mode === "official_cli"}
                    <div class="text-xs text-muted-foreground">
                      <div>启动命令：{provider.executionAgent}</div>
                      <div>默认权限：{providerPermissionLabel(provider)}</div>
                      {#if check?.path}
                        <div class="truncate">路径：{check.path}</div>
                      {/if}
                    </div>
                  {:else}
                    {@const fieldRules = providerFieldRules(provider)}
                    <div class="space-y-2">
                      <!-- API Key row (always visible) + expand toggle -->
                      <div class="flex items-center gap-2">
                        {#if fieldRules.showApiKey}
                          <Input
                            type={showApiKey ? "text" : "password"}
                            placeholder={`${provider.label} API Key`}
                            value={credential?.api_key ?? ""}
                            oninput={(event) =>
                              updateApiProviderField(
                                provider,
                                "api_key",
                                (event.currentTarget as HTMLInputElement).value,
                              )}
                            onblur={persistApiProviderConfig}
                          />
                        {/if}
                        <button
                          type="button"
                          class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md border border-border text-muted-foreground hover:bg-accent transition-colors"
                          title={expandedProviderPanels[provider.id] ? "收起配置" : "展开配置"}
                          onclick={() => toggleProviderPanel(provider.id)}
                        >
                          <svg
                            class="h-4 w-4 transition-transform {expandedProviderPanels[provider.id]
                              ? 'rotate-180'
                              : ''}"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                          >
                            <path d="m6 9 6 6 6-6" />
                          </svg>
                        </button>
                      </div>
                    <!-- Expanded panel: 3 rows x 2 cols -->
                    {#if expandedProviderPanels[provider.id]}
                      {@const [tierOpus, tierSonnet, tierHaiku] = expandModelsToTiers(credential?.models ?? [])}
                      <div class="grid grid-cols-2 gap-2 rounded-md border border-border/50 p-3">
                        <div class="space-y-1">
                          <label class="text-[11px] text-muted-foreground" for="env-model-{provider.id}">
                            ANTHROPIC_MODEL
                          </label>
                          <input
                            id="env-model-{provider.id}"
                            class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                            placeholder={tierOpus || provider.defaultModel || "自动"}
                            value={credential?.extra_env?.ANTHROPIC_MODEL ?? ""}
                            oninput={(event) =>
                              updateApiProviderExtraEnv(provider, "ANTHROPIC_MODEL", (event.currentTarget as HTMLInputElement).value)}
                            onblur={persistApiProviderConfig}
                          />
                        </div>
                        <div class="space-y-1">
                          <label class="text-[11px] text-muted-foreground" for="env-opus-{provider.id}">
                            OPUS_MODEL
                          </label>
                          <input
                            id="env-opus-{provider.id}"
                            class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                            placeholder={tierOpus || provider.defaultModel || "自动"}
                            value={credential?.extra_env?.ANTHROPIC_DEFAULT_OPUS_MODEL ?? ""}
                            oninput={(event) =>
                              updateApiProviderExtraEnv(provider, "ANTHROPIC_DEFAULT_OPUS_MODEL", (event.currentTarget as HTMLInputElement).value)}
                            onblur={persistApiProviderConfig}
                          />
                        </div>
                        <div class="space-y-1">
                          <label class="text-[11px] text-muted-foreground" for="env-sonnet-{provider.id}">
                            SONNET_MODEL
                          </label>
                          <input
                            id="env-sonnet-{provider.id}"
                            class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                            placeholder={tierSonnet || tierOpus || provider.defaultModel || "自动"}
                            value={credential?.extra_env?.ANTHROPIC_DEFAULT_SONNET_MODEL ?? ""}
                            oninput={(event) =>
                              updateApiProviderExtraEnv(provider, "ANTHROPIC_DEFAULT_SONNET_MODEL", (event.currentTarget as HTMLInputElement).value)}
                            onblur={persistApiProviderConfig}
                          />
                        </div>
                        <div class="space-y-1">
                          <label class="text-[11px] text-muted-foreground" for="env-haiku-{provider.id}">
                            HAIKU_MODEL
                          </label>
                          <input
                            id="env-haiku-{provider.id}"
                            class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                            placeholder={tierHaiku || tierOpus || provider.defaultModel || "自动"}
                            value={credential?.extra_env?.ANTHROPIC_DEFAULT_HAIKU_MODEL ?? ""}
                            oninput={(event) =>
                              updateApiProviderExtraEnv(provider, "ANTHROPIC_DEFAULT_HAIKU_MODEL", (event.currentTarget as HTMLInputElement).value)}
                            onblur={persistApiProviderConfig}
                          />
                        </div>
                        <div class="space-y-1">
                          <label class="text-[11px] text-muted-foreground" for="env-subagent-{provider.id}">
                            SUBAGENT_MODEL
                          </label>
                          <input
                            id="env-subagent-{provider.id}"
                            class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                            placeholder={tierOpus || provider.defaultModel || "自动"}
                            value={credential?.extra_env?.CLAUDE_CODE_SUBAGENT_MODEL ?? ""}
                            oninput={(event) =>
                              updateApiProviderExtraEnv(provider, "CLAUDE_CODE_SUBAGENT_MODEL", (event.currentTarget as HTMLInputElement).value)}
                            onblur={persistApiProviderConfig}
                          />
                        </div>
                        <div class="space-y-1">
                          <label class="text-[11px] text-muted-foreground" for="env-effort-{provider.id}">
                            EFFORT_LEVEL
                          </label>
                          <select
                            id="env-effort-{provider.id}"
                            class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                            value={credential?.extra_env?.CLAUDE_CODE_EFFORT_LEVEL ?? ""}
                            onchange={(event) =>
                              updateApiProviderExtraEnv(provider, "CLAUDE_CODE_EFFORT_LEVEL", (event.currentTarget as HTMLSelectElement).value)}
                            onblur={persistApiProviderConfig}
                          >
                            <option value="">默认 (max)</option>
                            <option value="low">low</option>
                            <option value="medium">medium</option>
                            <option value="high">high</option>
                            <option value="max">max</option>
                          </select>
                        </div>
                      </div>
                    {/if}
                  </div>
                {/if}

                <div class="text-right text-[11px] text-muted-foreground">
                  {provider.mode === "official_cli"
                    ? check?.found
                      ? "可用"
                      : "需登录/安装"
                    : "CC session"}
                </div>
              </div>
              {/if}
            {/each}
          </div>
        </Card>

        <!-- ═══ Custom Providers ═══ -->
        <Card class="p-6 space-y-4">
          <div class="flex items-center justify-between">
            <div>
              <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
                Custom Providers
              </h2>
              <p class="mt-1 text-xs text-muted-foreground">
                自定义 Claude-compatible API 端点，使用 parameterized 模板启动。
              </p>
            </div>
            <Button
              size="sm"
              variant="outline"
              onclick={() => (showCustomForm = !showCustomForm)}
            >
              {showCustomForm ? "取消" : "新增 Custom Provider"}
            </Button>
          </div>

          <!-- Existing custom providers -->
          {#if customProviderCredentials.length > 0}
            <div class="divide-y divide-border rounded-md border border-border">
              {#each customProviderCredentials as cred (cred.platform_id)}
                <div class="grid gap-3 p-4 md:grid-cols-[minmax(160px,1fr)_minmax(220px,2fr)_auto] md:items-center">
                  <div class="min-w-0">
                    <div class="flex items-center gap-2">
                      <span class="text-sm font-medium">{cred.name ?? cred.platform_id}</span>
                      <span class="rounded border border-emerald-500/40 px-1.5 py-0.5 text-[10px] font-medium text-emerald-500">
                        Custom
                      </span>
                    </div>
                    <div class="mt-1 truncate text-xs text-muted-foreground">
                      {cred.models?.[0] ?? "未配置模型"} · {cred.base_url ?? "未配置 URL"}
                    </div>
                  </div>
                  <div class="text-xs text-muted-foreground">
                    Key: {cred.api_key ? "••••" + cred.api_key.slice(-4) : "未设置"}
                  </div>
                  <button
                    type="button"
                    class="rounded-md border border-red-500/30 px-3 py-1.5 text-xs text-red-400 hover:bg-red-500/10 transition-colors"
                    onclick={() => removeCustomProvider(cred.platform_id)}
                  >
                    删除
                  </button>
                </div>
              {/each}
            </div>
          {:else if !showCustomForm}
            <p class="text-sm text-muted-foreground">暂无自定义 Provider。</p>
          {/if}

          <!-- Add form -->
          {#if showCustomForm}
            <div class="rounded-md border border-border/50 p-4 space-y-3">
              <div class="grid grid-cols-2 gap-3">
                <div class="space-y-1">
                  <label class="text-[11px] text-muted-foreground" for="custom-name">Name *</label>
                  <input
                    id="custom-name"
                    class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                    placeholder="My API"
                    bind:value={customFormName}
                  />
                </div>
                <div class="space-y-1">
                  <label class="text-[11px] text-muted-foreground" for="custom-url">Base URL *</label>
                  <input
                    id="custom-url"
                    class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                    placeholder="https://api.example.com/anthropic"
                    bind:value={customFormUrl}
                  />
                </div>
                <div class="space-y-1">
                  <label class="text-[11px] text-muted-foreground" for="custom-key">API Key *</label>
                  <input
                    id="custom-key"
                    type={customShowApiKey ? "text" : "password"}
                    class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                    placeholder="sk-..."
                    bind:value={customFormKey}
                  />
                </div>
                <div class="space-y-1">
                  <label class="text-[11px] text-muted-foreground" for="custom-model">Model *</label>
                  <input
                    id="custom-model"
                    class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                    placeholder="claude-sonnet-4-20250514"
                    bind:value={customFormModel}
                  />
                </div>
                <div class="space-y-1">
                  <label class="text-[11px] text-muted-foreground" for="custom-effort">Effort Level</label>
                  <select
                    id="custom-effort"
                    class="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
                    bind:value={customFormEffort}
                  >
                    <option value="max">max (默认)</option>
                    <option value="low">low</option>
                    <option value="medium">medium</option>
                    <option value="high">high</option>
                  </select>
                </div>
              </div>
              <div class="flex justify-end">
                <Button
                  size="sm"
                  disabled={!customFormName.trim() || !customFormUrl.trim() || !customFormKey.trim() || !customFormModel.trim()}
                  onclick={addCustomProvider}
                >
                  保存
                </Button>
              </div>
            </div>
          {/if}
        </Card>
      </div>
    {:else if activeTab === "cli-config"}
      {#if cliConfigLoading && !cliConfigLoaded}
        <div class="flex items-center justify-center py-12">
          <div
            class="h-5 w-5 animate-spin rounded-full border-2 border-primary border-t-transparent"
          ></div>
          <span class="ml-3 text-sm text-muted-foreground">{t("settings_cliConfig_loading")}</span>
        </div>
      {:else if cliConfigError}
        <Card class="p-6">
          <p class="text-sm text-red-400">
            {t("settings_cliConfig_loadFailed", { error: cliConfigError })}
          </p>
          <button
            class="mt-3 rounded-md border px-3 py-1.5 text-xs hover:bg-accent transition-colors"
            onclick={() => {
              cliConfigLoaded = false;
              loadCliConfig();
            }}
          >
            {t("settings_cliConfig_retry")}
          </button>
        </Card>
      {:else}
        <div class="space-y-6">
          <!-- Behavior -->
          <Card class="p-6 space-y-4">
            <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
              {t("settings_cliConfig_behavior")}
            </h2>
            {#each behaviorSettings as def (def.key)}
              <div class="flex items-center justify-between gap-4 py-1">
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <p class="text-sm font-medium">{def.label}</p>
                    {#if isProjectOverride(def.key)}
                      <span
                        class="inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-medium bg-amber-500/15 text-amber-400 border border-amber-500/20"
                      >
                        {t("settings_cliConfig_projectOverride")}
                      </span>
                    {/if}
                  </div>
                  <p class="text-xs text-muted-foreground mt-0.5">{def.description}</p>
                </div>
                {#if def.type === "boolean"}
                  <button
                    aria-label={def.label}
                    class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-200 {getCliConfigValue(
                      def.key,
                      def,
                    ) === true
                      ? 'bg-primary'
                      : 'bg-neutral-700'}"
                    onclick={() => {
                      const current = getCliConfigValue(def.key, def);
                      const next = current === true ? false : true;
                      saveCliConfigPatch(def.key, next);
                      cliConfig = { ...cliConfig, [def.key]: next };
                    }}
                  >
                    <span
                      class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform duration-200 {getCliConfigValue(
                        def.key,
                        def,
                      ) === true
                        ? 'translate-x-6'
                        : 'translate-x-1'}"
                    ></span>
                  </button>
                {:else if def.type === "enum" && def.options}
                  <div class="flex gap-1.5 shrink-0">
                    {#each def.options as opt (opt.value)}
                      <button
                        class="rounded-md border px-3 py-1.5 text-xs transition-all duration-150
                        {getCliConfigValue(def.key, def) === opt.value
                          ? 'bg-primary text-primary-foreground'
                          : 'hover:bg-accent hover:border-ring/30'}"
                        onclick={() => {
                          saveCliConfigPatch(def.key, opt.value);
                          cliConfig = { ...cliConfig, [def.key]: opt.value };
                        }}
                      >
                        {opt.label}
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>
            {/each}
          </Card>

          <!-- Appearance -->
          <Card class="p-6 space-y-4">
            <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
              {t("settings_cliConfig_appearance")}
            </h2>
            {#each appearanceSettings as def (def.key)}
              <div class="flex items-center justify-between gap-4 py-1">
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <p class="text-sm font-medium">{def.label}</p>
                    {#if isProjectOverride(def.key)}
                      <span
                        class="inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-medium bg-amber-500/15 text-amber-400 border border-amber-500/20"
                      >
                        {t("settings_cliConfig_projectOverride")}
                      </span>
                    {/if}
                  </div>
                  <p class="text-xs text-muted-foreground mt-0.5">{def.description}</p>
                </div>
                {#if def.type === "boolean"}
                  <button
                    aria-label={def.label}
                    class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-200 {getCliConfigValue(
                      def.key,
                      def,
                    ) === true
                      ? 'bg-primary'
                      : 'bg-neutral-700'}"
                    onclick={() => {
                      const current = getCliConfigValue(def.key, def);
                      const next = current === true ? false : true;
                      saveCliConfigPatch(def.key, next);
                      cliConfig = { ...cliConfig, [def.key]: next };
                    }}
                  >
                    <span
                      class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform duration-200 {getCliConfigValue(
                        def.key,
                        def,
                      ) === true
                        ? 'translate-x-6'
                        : 'translate-x-1'}"
                    ></span>
                  </button>
                {:else if def.type === "enum" && def.options}
                  <div class="flex gap-1.5 shrink-0">
                    {#each def.options as opt (opt.value)}
                      <button
                        class="rounded-md border px-3 py-1.5 text-xs transition-all duration-150
                        {getCliConfigValue(def.key, def) === opt.value
                          ? 'bg-primary text-primary-foreground'
                          : 'hover:bg-accent hover:border-ring/30'}"
                        onclick={() => {
                          saveCliConfigPatch(def.key, opt.value);
                          cliConfig = { ...cliConfig, [def.key]: opt.value };
                        }}
                      >
                        {opt.label}
                      </button>
                    {/each}
                  </div>
                {:else if def.type === "string"}
                  <input
                    class="w-40 shrink-0 rounded-md border bg-transparent px-3 py-1.5 text-sm placeholder:text-muted-foreground focus:border-ring focus:outline-none"
                    value={getCliConfigValue(def.key, def) ?? ""}
                    placeholder={def.label}
                    onblur={(e) => {
                      const val = (e.target as HTMLInputElement).value.trim();
                      if (val) {
                        saveCliConfigPatch(def.key, val);
                        cliConfig = { ...cliConfig, [def.key]: val };
                      } else {
                        // Empty string → delete key (restore default)
                        saveCliConfigPatch(def.key, null);
                        const next = { ...cliConfig };
                        delete next[def.key];
                        cliConfig = next;
                      }
                    }}
                  />
                {/if}
              </div>
            {/each}
          </Card>

          <!-- Advanced -->
          <Card class="p-6 space-y-4">
            <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
              {t("settings_cliConfig_advanced")}
            </h2>
            {#each advancedSettings as def (def.key)}
              <div class="flex items-center justify-between gap-4 py-1">
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <p class="text-sm font-medium">{def.label}</p>
                    {#if isProjectOverride(def.key)}
                      <span
                        class="inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-medium bg-amber-500/15 text-amber-400 border border-amber-500/20"
                      >
                        {t("settings_cliConfig_projectOverride")}
                      </span>
                    {/if}
                  </div>
                  <p class="text-xs text-muted-foreground mt-0.5">{def.description}</p>
                </div>
                {#if def.type === "boolean"}
                  <button
                    aria-label={def.label}
                    class="relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors duration-200 {getCliConfigValue(
                      def.key,
                      def,
                    ) === true
                      ? 'bg-primary'
                      : 'bg-neutral-700'}"
                    onclick={() => {
                      const current = getCliConfigValue(def.key, def);
                      const next = current === true ? false : true;
                      saveCliConfigPatch(def.key, next);
                      cliConfig = { ...cliConfig, [def.key]: next };
                    }}
                  >
                    <span
                      class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform duration-200 {getCliConfigValue(
                        def.key,
                        def,
                      ) === true
                        ? 'translate-x-6'
                        : 'translate-x-1'}"
                    ></span>
                  </button>
                {:else if def.type === "enum" && def.options}
                  <div class="flex gap-1.5 shrink-0">
                    {#each def.options as opt (opt.value)}
                      <button
                        class="rounded-md border px-3 py-1.5 text-xs transition-all duration-150
                        {getCliConfigValue(def.key, def) === opt.value
                          ? 'bg-primary text-primary-foreground'
                          : 'hover:bg-accent hover:border-ring/30'}"
                        onclick={() => {
                          saveCliConfigPatch(def.key, opt.value);
                          cliConfig = { ...cliConfig, [def.key]: opt.value };
                        }}
                      >
                        {opt.label}
                      </button>
                    {/each}
                  </div>
                {/if}
              </div>
            {/each}
          </Card>

          <!-- MSVC Developer Environment (Windows only) -->
          {#if IS_WINDOWS}
            <Card class="p-6 space-y-4">
              <div class="flex items-center justify-between">
                <div>
                  <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
                    {t("settings_msvc_title")}
                  </h2>
                  <p class="mt-1 text-xs text-muted-foreground">{t("settings_msvc_desc")}</p>
                </div>
                {#if msvcEnvLoading}
                  <span class="text-xs text-muted-foreground">{t("settings_msvc_checking")}</span>
                {/if}
              </div>

              <div class="grid grid-cols-3 gap-2">
                {#each (["auto", "always", "off"] as WindowsMsvcEnvMode[]) as mode}
                  <button
                    class="rounded-md border px-3 py-2 text-sm transition-colors disabled:opacity-60 {msvcEnvMode ===
                    mode
                      ? 'border-primary bg-primary/10 text-primary'
                      : 'text-muted-foreground hover:bg-accent'}"
                    disabled={msvcEnvSaving}
                    onclick={() => saveMsvcEnvMode(mode)}
                  >
                    {msvcEnvModeLabel(mode)}
                  </button>
                {/each}
              </div>

              {#if msvcEnvStatus}
                <div class="rounded-md border px-3 py-2 {msvcEnvStateClass(msvcEnvStatus.state)}">
                  <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
                    <span class="font-medium">{msvcEnvStateLabel(msvcEnvStatus.state)}</span>
                    {#if msvcEnvStatus.source_path}
                      <span class="font-mono text-[11px] opacity-80">
                        {msvcEnvStatus.source_path}
                      </span>
                    {/if}
                    {#if msvcEnvStatus.arch}
                      <span class="text-[11px] opacity-80">{msvcEnvStatus.arch}</span>
                    {/if}
                  </div>
                  {#if msvcEnvStatus.message}
                    <p class="mt-1 text-xs opacity-90">{msvcEnvStatus.message}</p>
                  {/if}
                  {#if msvcEnvStatus.next_action}
                    <p class="mt-1 text-xs opacity-80">{msvcEnvStatus.next_action}</p>
                  {/if}
                </div>
              {/if}
            </Card>
          {/if}

          <!-- Footer note -->
          <p class="text-[10px] text-muted-foreground px-1">
            {t("settings_cliConfig_footer")}
          </p>
        </div>
      {/if}

      <!-- ═══ Shortcuts tab ═══ -->
    {:else if activeTab === "shortcuts"}
      <div class="space-y-6">
        <!-- App shortcuts (editable) -->
        <Card class="p-6 space-y-5">
          <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
            {t("settings_shortcuts_appShortcuts")}
          </h2>
          <div class="divide-y divide-border/50">
            {#each appBindings as binding (binding.command)}
              <KeybindingEditor
                {binding}
                isOverridden={isOverridden(binding.command)}
                conflictWarning={recordingConflict}
                onSave={(key) => {
                  const conflict = getConflictWarning(key, binding.context, binding.command);
                  if (conflict) {
                    recordingConflict = conflict;
                  }
                  keybindingStore.setOverride(binding.command, key);
                  recordingConflict = "";
                }}
                onReset={isOverridden(binding.command)
                  ? () => keybindingStore.resetBinding(binding.command)
                  : undefined}
              />
            {/each}
          </div>
        </Card>

        <!-- Fixed shortcuts -->
        <Card class="p-6 space-y-5">
          <h2 class="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
            {t("settings_shortcuts_inputFixed")}
          </h2>
          <div class="divide-y divide-border/50">
            {#each fixedBindings as binding (binding.command)}
              <div class="flex items-center gap-3 py-1.5">
                <span class="text-sm text-foreground/60 min-w-[140px]">{binding.label}</span>
                <span
                  class="inline-flex items-center rounded-md border bg-muted/30 px-2.5 py-1 text-xs font-mono text-muted-foreground min-w-[60px] justify-center"
                >
                  {formatKeyDisplay(binding.key)}
                </span>
              </div>
            {/each}
          </div>
        </Card>

        <!-- CLI shortcuts (collapsible) -->
        <Card class="p-6 space-y-4">
          <button
            class="flex items-center gap-2 text-sm font-semibold text-muted-foreground uppercase tracking-wider hover:text-foreground transition-colors w-full"
            onclick={() => (cliSectionOpen = !cliSectionOpen)}
          >
            <svg
              class="h-3 w-3 transition-transform {cliSectionOpen ? 'rotate-90' : ''}"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"><path d="m9 18 6-6-6-6" /></svg
            >
            {t("settings_shortcuts_cliShortcuts")}
            <span class="text-[10px] font-normal normal-case tracking-normal text-muted-foreground"
              >{t("settings_shortcuts_readOnly")}</span
            >
          </button>
          {#if cliSectionOpen}
            <div class="divide-y divide-border/50">
              {#each cliBindings as binding (binding.command)}
                <div class="flex items-center gap-3 py-1.5">
                  <span class="text-sm text-foreground/60 min-w-[140px]">{binding.label}</span>
                  <span
                    class="inline-flex items-center rounded-md border bg-muted/30 px-2.5 py-1 text-xs font-mono text-muted-foreground min-w-[60px] justify-center"
                  >
                    {formatKeyDisplay(binding.key)}
                  </span>
                </div>
              {/each}
            </div>
            <p class="text-[10px] text-muted-foreground">
              {t("settings_shortcuts_source", {
                source:
                  cliSource === "file"
                    ? IS_WINDOWS
                      ? "%USERPROFILE%\\.claude\\keybindings.json"
                      : "~/.claude/keybindings.json"
                    : t("settings_shortcuts_cliDefaults"),
              })}
            </p>
          {/if}
        </Card>

        <!-- Reset all -->
        {#if hasOverrides}
          <div class="flex justify-end">
            <button
              class="rounded-md border px-3 py-1.5 text-xs text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
              onclick={() => keybindingStore.resetAll()}
            >
              {t("settings_shortcuts_resetAll")}
            </button>
          </div>
        {/if}
      </div>

      <!-- ═══ Remote tab ═══ -->
    {:else if activeTab === "remote"}
      <Card class="p-6 space-y-5">
        <div class="flex items-start justify-between">
          <div>
            <p class="text-sm font-medium">{t("settings_remote_title")}</p>
            <p class="text-xs text-muted-foreground mt-0.5">
              {t("settings_remote_desc")}
            </p>
          </div>
          {#if remoteSaved}
            <span class="text-xs text-emerald-500 flex items-center gap-1 animate-fade-in">
              <svg
                class="h-3 w-3"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
              >
              {t("settings_general_saved")}
            </span>
          {/if}
        </div>

        <!-- Existing hosts list -->
        {#if remoteHosts.length > 0}
          <div class="space-y-2">
            {#each remoteHosts as host (host.name)}
              <div
                class="flex items-center justify-between p-3 bg-muted/50 rounded-lg border border-border"
              >
                <div>
                  <p class="text-sm font-medium">{host.name}</p>
                  <p class="text-xs text-muted-foreground">
                    {host.user}@{host.host}{host.port !== 22 ? `:${host.port}` : ""}
                  </p>
                  {#if host.remote_cwd}
                    <p class="text-xs text-muted-foreground">cwd: {host.remote_cwd}</p>
                  {/if}
                </div>
                <div class="flex gap-2">
                  <button
                    class="text-xs px-2 py-1 rounded hover:bg-accent text-muted-foreground"
                    onclick={() => editRemoteHost(host)}>{t("settings_remote_edit")}</button
                  >
                  <button
                    class="text-xs px-2 py-1 rounded hover:bg-destructive/10 text-destructive"
                    onclick={() => deleteRemoteHost(host.name)}
                    >{t("settings_remote_delete")}</button
                  >
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <p class="text-xs text-muted-foreground italic">{t("settings_remote_noHosts")}</p>
        {/if}

        <!-- Add / Edit form -->
        <div class="border border-border rounded-lg p-4 space-y-3">
          <p class="text-sm font-medium">
            {editingRemote
              ? t("settings_remote_editHost", { name: editingRemote.name })
              : t("settings_remote_addHost")}
          </p>

          <div class="grid grid-cols-2 gap-3">
            <label class="block">
              <span class="text-xs text-muted-foreground block mb-1"
                >{t("settings_remote_name")} *</span
              >
              <input
                type="text"
                bind:value={remoteFormName}
                placeholder="mac-mini"
                class="w-full text-sm px-2 py-1.5 rounded border bg-background {remoteFormTouched &&
                !remoteFormName.trim()
                  ? 'border-red-500'
                  : 'border-input'}"
              />
            </label>
            <label class="block">
              <span class="text-xs text-muted-foreground block mb-1"
                >{t("settings_remote_host")} *</span
              >
              <input
                type="text"
                bind:value={remoteFormHost}
                placeholder="macmini.local"
                class="w-full text-sm px-2 py-1.5 rounded border bg-background {remoteFormTouched &&
                !remoteFormHost.trim()
                  ? 'border-red-500'
                  : 'border-input'}"
              />
            </label>
            <label class="block">
              <span class="text-xs text-muted-foreground block mb-1"
                >{t("settings_remote_user")} *</span
              >
              <input
                type="text"
                bind:value={remoteFormUser}
                placeholder={currentUsername || "username"}
                class="w-full text-sm px-2 py-1.5 rounded border bg-background {remoteFormTouched &&
                !remoteFormUser.trim()
                  ? 'border-red-500'
                  : 'border-input'}"
              />
            </label>
            <label class="block">
              <span class="text-xs text-muted-foreground block mb-1"
                >{t("settings_remote_port")}</span
              >
              <input
                type="number"
                bind:value={remoteFormPort}
                placeholder="22"
                class="w-full text-sm px-2 py-1.5 rounded border border-input bg-background"
              />
            </label>
            <div class="col-span-2">
              <span class="text-xs text-muted-foreground block mb-1"
                >{t("settings_remote_keyPath")}</span
              >
              <div class="flex gap-2">
                <input
                  type="text"
                  aria-label={t("settings_remote_keyPath")}
                  bind:value={remoteFormKeyPath}
                  placeholder="~/.ssh/id_ed25519"
                  class="flex-1 text-sm px-2 py-1.5 rounded border border-input bg-background"
                />
                {#if sshKeyStep === "idle"}
                  <button
                    class="shrink-0 text-xs px-2 py-1.5 rounded border border-input hover:bg-accent transition-colors text-muted-foreground"
                    onclick={startSshKeyWizard}
                  >
                    {t("settings_remote_setupSshKey")}
                  </button>
                {/if}
              </div>

              <!-- SSH Key Wizard inline panel -->
              {#if sshKeyStep !== "idle"}
                <div class="mt-2 rounded-lg border border-border p-3 space-y-2 text-xs bg-muted/30">
                  {#if sshKeyStep === "checking"}
                    <div class="flex items-center gap-2 text-muted-foreground">
                      <div
                        class="h-3.5 w-3.5 animate-spin rounded-full border-2 border-primary border-t-transparent"
                      ></div>
                      {t("settings_remote_sshKeyChecking")}
                    </div>
                  {:else if sshKeyStep === "no_key"}
                    <p class="text-muted-foreground">{t("settings_remote_sshKeyNotFound")}</p>
                    <button
                      class="rounded border px-3 py-1.5 text-xs hover:bg-accent transition-colors"
                      onclick={generateSshKey}
                    >
                      {t("settings_remote_sshKeyGenerate")}
                    </button>
                  {:else if sshKeyStep === "generating"}
                    <div class="flex items-center gap-2 text-muted-foreground">
                      <div
                        class="h-3.5 w-3.5 animate-spin rounded-full border-2 border-primary border-t-transparent"
                      ></div>
                      {t("settings_remote_sshKeyGenerating")}
                    </div>
                  {:else if sshKeyStep === "pub_missing" && sshKeyInfo}
                    <p class="text-amber-400">
                      {t(
                        IS_WINDOWS
                          ? "settings_remote_sshKeyPubMissing_win"
                          : "settings_remote_sshKeyPubMissing",
                      )}
                    </p>
                    <div class="flex items-center gap-2">
                      <code
                        class="flex-1 rounded bg-muted px-2 py-1.5 font-mono text-[11px] break-all select-all"
                      >
                        {buildRebuildPubKeyCommand(sshKeyInfo)}
                      </code>
                      <button
                        class="shrink-0 rounded border px-2 py-1 text-[10px] hover:bg-accent transition-colors"
                        onclick={async () => {
                          await navigator.clipboard.writeText(
                            buildRebuildPubKeyCommand(sshKeyInfo!),
                          );
                          sshCopied = true;
                          setTimeout(() => (sshCopied = false), 2000);
                        }}
                      >
                        {sshCopied ? t("settings_remote_sshKeyCopied") : t("common_copy")}
                      </button>
                    </div>
                    <p class="text-muted-foreground text-[10px]">
                      After running the command, click "Setup SSH Key" again.
                    </p>
                    <button
                      class="text-[10px] text-muted-foreground hover:underline"
                      onclick={closeSshWizard}
                    >
                      {t("settings_remote_sshKeyClose")}
                    </button>
                  {:else if sshKeyStep === "has_key" && sshKeyInfo}
                    <p class="text-emerald-500">
                      {t("settings_remote_sshKeyFound", { keyType: sshKeyInfo.key_type })}
                      <span class="text-muted-foreground ml-1 font-mono">{sshKeyInfo.key_path}</span
                      >
                    </p>

                    {#if remoteFormHost && remoteFormUser}
                      <p class="text-muted-foreground">
                        {t(
                          IS_WINDOWS
                            ? "settings_remote_sshKeyCopyCmd_win"
                            : "settings_remote_sshKeyCopyCmd",
                        )}
                      </p>
                      <div class="flex items-center gap-2">
                        <code
                          class="flex-1 rounded bg-muted px-2 py-1.5 font-mono text-[11px] break-all select-all"
                        >
                          {buildCopyCommand(
                            sshKeyInfo,
                            remoteFormHost.trim(),
                            remoteFormUser.trim(),
                            remoteFormPort || 22,
                          )}
                        </code>
                        <button
                          class="shrink-0 rounded border px-2 py-1 text-[10px] hover:bg-accent transition-colors"
                          onclick={async () => {
                            await navigator.clipboard.writeText(
                              buildCopyCommand(
                                sshKeyInfo!,
                                remoteFormHost.trim(),
                                remoteFormUser.trim(),
                                remoteFormPort || 22,
                              ),
                            );
                            sshCopied = true;
                            setTimeout(() => (sshCopied = false), 2000);
                          }}
                        >
                          {sshCopied ? t("settings_remote_sshKeyCopied") : t("common_copy")}
                        </button>
                      </div>

                      <div class="flex items-center gap-2 mt-1">
                        <button
                          class="rounded border px-3 py-1.5 text-xs hover:bg-accent transition-colors"
                          disabled={sshVerifying}
                          onclick={verifySshConnection}
                        >
                          {sshVerifying
                            ? t("settings_remote_sshKeyVerifying")
                            : t("settings_remote_sshKeyVerify")}
                        </button>
                        <button
                          class="text-[10px] text-muted-foreground hover:underline"
                          onclick={closeSshWizard}
                        >
                          {t("settings_remote_sshKeyClose")}
                        </button>
                      </div>

                      {#if sshKeyError && sshKeyStep === "has_key"}
                        <p class="text-red-400 text-[11px]">
                          {t(
                            IS_WINDOWS
                              ? "settings_remote_sshKeyFailed_win"
                              : "settings_remote_sshKeyFailed",
                          )}
                        </p>
                      {/if}
                    {:else}
                      <p class="text-muted-foreground text-[10px]">
                        Fill in Host and User above, then come back to copy the install command.
                      </p>
                      <button
                        class="text-[10px] text-muted-foreground hover:underline"
                        onclick={closeSshWizard}
                      >
                        {t("settings_remote_sshKeyClose")}
                      </button>
                    {/if}
                  {:else if sshKeyStep === "done"}
                    <p class="text-emerald-500">{t("settings_remote_sshKeySuccess")}</p>
                    <button
                      class="text-[10px] text-muted-foreground hover:underline"
                      onclick={closeSshWizard}
                    >
                      {t("settings_remote_sshKeyClose")}
                    </button>
                  {:else if sshKeyStep === "error"}
                    <p class="text-red-400">
                      {t("settings_remote_sshKeyGenError", { error: sshKeyError })}
                    </p>
                    <button
                      class="text-[10px] text-muted-foreground hover:underline"
                      onclick={closeSshWizard}
                    >
                      {t("settings_remote_sshKeyClose")}
                    </button>
                  {/if}
                </div>
              {/if}
            </div>
            <label class="block">
              <span class="text-xs text-muted-foreground block mb-1"
                >{t("settings_remote_remoteCwd")}</span
              >
              <input
                type="text"
                bind:value={remoteFormRemoteCwd}
                placeholder={currentUsername ? "~/projects" : "~/projects"}
                class="w-full text-sm px-2 py-1.5 rounded border border-input bg-background"
              />
            </label>
            <label class="block">
              <span class="text-xs text-muted-foreground block mb-1"
                >{t("settings_remote_claudePath")}</span
              >
              <input
                type="text"
                bind:value={remoteFormClaudePath}
                placeholder="claude (default)"
                class="w-full text-sm px-2 py-1.5 rounded border border-input bg-background"
              />
            </label>
            <div class="flex items-end">
              <label class="flex items-center gap-2 text-sm cursor-pointer">
                <input type="checkbox" bind:checked={remoteFormForwardKey} class="rounded" />
                {t("settings_remote_forwardKey")}
              </label>
            </div>
          </div>

          {#if remoteFormForwardKey}
            <div
              class="flex items-start gap-2 p-2 rounded bg-yellow-500/10 border border-yellow-500/20 text-xs text-yellow-600 dark:text-yellow-400"
            >
              <span class="shrink-0 mt-0.5">&#9888;</span>
              <span>{t("settings_remote_forwardKeyWarning")}</span>
            </div>
          {/if}

          <!-- Test + Save buttons -->
          <div class="flex gap-2 items-center">
            <Button
              variant="secondary"
              size="sm"
              disabled={remoteTesting}
              onclick={testRemoteConnection}
            >
              {remoteTesting ? t("settings_remote_testing") : t("settings_remote_testConnection")}
            </Button>
            <Button size="sm" disabled={remoteSaving} onclick={() => saveRemoteHost()}>
              {remoteSaving
                ? t("settings_remote_saving")
                : editingRemote
                  ? t("settings_remote_update")
                  : t("settings_remote_add")}
            </Button>
            {#if editingRemote}
              <button
                class="text-xs text-muted-foreground hover:underline"
                onclick={resetRemoteForm}>{t("settings_remote_cancel")}</button
              >
            {/if}
          </div>

          <!-- Test result -->
          {#if remoteTestResult}
            <div
              class="text-xs space-y-1 p-2 rounded border {remoteTestResult.ssh_ok
                ? 'border-green-500/30 bg-green-500/5'
                : 'border-red-500/30 bg-red-500/5'}"
            >
              <p>
                {t("settings_remote_sshLabel")}
                {remoteTestResult.ssh_ok
                  ? t("settings_remote_connected")
                  : t("settings_remote_failed")}
              </p>
              {#if remoteTestResult.ssh_ok}
                <p>
                  {t("settings_remote_cliLabel")}
                  {remoteTestResult.cli_found
                    ? t("settings_remote_found")
                    : t("settings_remote_notFound")}
                </p>
                {#if remoteTestResult.cli_version}
                  <p>{t("settings_remote_version", { version: remoteTestResult.cli_version })}</p>
                {/if}
                {#if remoteTestResult.cli_path}
                  <p>{t("settings_remote_path", { path: remoteTestResult.cli_path })}</p>
                {/if}
                {#if remoteTestResult.ssh_ok && !remoteTestResult.cli_found}
                  <div
                    class="mt-1.5 p-2 rounded bg-amber-500/10 border border-amber-500/20 space-y-1"
                  >
                    <p class="text-amber-400">{t("settings_remote_cliNotFoundHint")}</p>
                    <code class="block rounded bg-muted px-2 py-1 font-mono text-[11px] select-all"
                      >which claude</code
                    >
                    <p class="text-muted-foreground">{t("settings_remote_cliNotFoundHint2")}</p>
                  </div>
                {/if}
              {/if}
              {#if remoteTestResult.error}
                <p class="text-red-500">{remoteTestResult.error}</p>
              {/if}
            </div>
          {/if}
        </div>
      </Card>

      <!-- ═══ Debug tab ═══ -->
    {:else if activeTab === "debug"}
      <Card class="p-6 space-y-5">
        <div class="flex items-center justify-between">
          <div>
            <p class="text-sm font-medium">{t("settings_debug_title")}</p>
            <p class="text-xs text-muted-foreground mt-0.5">
              {t("settings_debug_desc")}
              {t("settings_debug_rustHint")}
              <code class="text-xs">RUST_LOG=debug cargo tauri dev</code>
            </p>
          </div>
          <button
            aria-label="Debug mode"
            class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors duration-200 {debugOn
              ? 'bg-primary'
              : 'bg-neutral-700'}"
            onclick={() => {
              debugOn = !debugOn;
              setDebugMode(debugOn);
            }}
          >
            <span
              class="inline-block h-4 w-4 transform rounded-full bg-white transition-transform duration-200 {debugOn
                ? 'translate-x-6'
                : 'translate-x-1'}"
            ></span>
          </button>
        </div>

        {#if debugOn}
          <!-- Tag filter -->
          <div>
            <label class="text-sm font-medium mb-1 block" for="debug-filter"
              >{t("settings_debug_tagFilter")}</label
            >
            <input
              id="debug-filter"
              class="w-full rounded-md border bg-transparent px-3 py-1.5 text-sm font-mono placeholder:text-muted-foreground focus:border-ring focus:outline-none"
              value={debugFilter}
              placeholder="1 = all, api,bus = only those, -replay = exclude"
              oninput={(e) => {
                const val = (e.target as HTMLInputElement).value.trim();
                debugFilter = val;
                setDebugMode(val || "1");
              }}
            />
            <p class="mt-1 text-[10px] text-muted-foreground">
              <code class="text-xs">1</code> = {t("settings_debug_filterHelp_all")} &nbsp;|&nbsp;
              <code class="text-xs">api,bus</code> = {t("settings_debug_filterHelp_only")} &nbsp;|&nbsp;
              <code class="text-xs">-replay</code> = {t("settings_debug_filterHelp_exclude")}
            </p>
          </div>

          <!-- Log actions -->
          <div class="flex items-center gap-3">
            <button
              class="rounded-md border px-3 py-1.5 text-xs transition-colors hover:bg-accent"
              onclick={async () => {
                logCopied = await copyDebugLogs();
                if (logCopied) setTimeout(() => (logCopied = false), 2000);
              }}
            >
              {logCopied
                ? t("settings_debug_copied")
                : t("settings_debug_copyLogs", { count: String(logCount) })}
            </button>
            <button
              class="rounded-md border px-3 py-1.5 text-xs transition-colors hover:bg-accent text-muted-foreground"
              onclick={() => {
                clearDebugLogs();
                logCount = 0;
              }}
            >
              {t("settings_debug_clear")}
            </button>
            <span class="text-[10px] text-muted-foreground ml-auto"
              >{t("settings_debug_entriesBuffered", { count: String(logCount) })}</span
            >
          </div>

          <!-- Rust log hint -->
          <div class="rounded-md bg-muted/50 p-3">
            <p class="text-xs text-muted-foreground mb-1.5">
              {t("settings_debug_rustBackendLogs")}
            </p>
            <div class="flex items-center gap-2">
              <code class="flex-1 text-xs font-mono break-all">RUST_LOG=debug cargo tauri dev</code>
              <button
                class="shrink-0 rounded border px-2 py-1 text-[10px] transition-colors hover:bg-accent"
                onclick={async () => {
                  await navigator.clipboard.writeText("RUST_LOG=debug cargo tauri dev");
                  rustCmdCopied = true;
                  setTimeout(() => (rustCmdCopied = false), 2000);
                }}
              >
                {rustCmdCopied ? t("settings_debug_copied") : t("settings_debug_copy")}
              </button>
            </div>
          </div>

          <p class="text-[10px] text-muted-foreground">
            {t("settings_debug_maxEntries")}
          </p>
        {/if}
      </Card>
    {:else if activeTab === "characters"}
      <Card class="p-6 space-y-4">
        <p class="text-sm text-muted-foreground">
          {t("settings_characters")}
        </p>
        <a
          href="/settings/characters"
          class="inline-flex items-center gap-2 rounded-md border border-input px-4 py-2 text-sm font-medium hover:bg-accent transition-colors"
        >
          <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2 M9 3a4 4 0 1 0 0 8 4 4 0 0 0 0-8z M22 21v-2a4 4 0 0 0-3-3.87 M16 3.13a4 4 0 0 1 0 7.75" />
          </svg>
          {t("settings_characters_manage")}
        </a>
      </Card>
    {:else if activeTab === "memory"}
      <Card class="p-6 space-y-4">
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
      </Card>
    {/if}
  </div>
{/key}
