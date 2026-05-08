import type { PlatformPreset } from "$lib/types";

export const PLATFORM_PRESETS: PlatformPreset[] = [
  // ── LLM Providers ──
  {
    id: "anthropic",
    name: "Anthropic",
    base_url: "",
    auth_env_var: "ANTHROPIC_API_KEY",
    description: "Claude official API",
    key_placeholder: "your-anthropic-api-key",
    category: "provider",
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    base_url: "https://api.deepseek.com/anthropic",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "DeepSeek API",
    key_placeholder: "your-deepseek-key",
    category: "provider",
    models: ["deepseek-v4-pro", "deepseek-v4-flash"],
    extra_env: { API_TIMEOUT_MS: "600000" },
    docs_url: "https://api-docs.deepseek.com/guides/anthropic_api",
  },
  {
    id: "kimi",
    name: "Kimi (Moonshot)",
    base_url: "https://api.moonshot.cn/anthropic",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "Moonshot AI",
    key_placeholder: "your-kimi-key",
    category: "provider",
    models: ["kimi-k2.5"],
    docs_url: "https://platform.moonshot.ai/docs/guide/agent-support",
  },
  {
    id: "zhipu",
    name: "Zhipu (智谱)",
    base_url: "https://open.bigmodel.cn/api/anthropic",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "Zhipu AI — bigmodel.cn",
    key_placeholder: "your-zhipu-key",
    category: "provider",
    models: ["glm-5", "glm-4.7", "glm-4.5-air", "glm-4.5-flash"],
    docs_url: "https://docs.bigmodel.cn/cn/guide/develop/claude/introduction",
  },
  {
    id: "zhipu-intl",
    name: "Zhipu (智谱 Intl)",
    base_url: "https://api.z.ai/api/anthropic",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "Zhipu AI — z.ai",
    key_placeholder: "your-zhipu-key",
    category: "provider",
    models: ["glm-5", "glm-4.7", "glm-4.5-air", "glm-4.5-flash"],
    docs_url: "https://docs.z.ai/devpack/tool/claude",
  },
  {
    id: "bailian",
    name: "Bailian (百炼)",
    base_url: "https://coding.dashscope.aliyuncs.com/apps/anthropic",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "Alibaba DashScope",
    key_placeholder: "your-bailian-key",
    category: "provider",
    models: ["qwen3.5-plus", "qwen3-coder-next"],
    docs_url: "https://help.aliyun.com/zh/model-studio/anthropic-api-messages",
  },
  {
    id: "mimo-pro",
    name: "Xiaomi MiMo Pro (小米 Pro)",
    base_url: "https://token-plan-cn.xiaomimimo.com/anthropic",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "Xiaomi MiMo Pro",
    key_placeholder: "your-mimo-pro-key",
    category: "provider",
    models: ["mimo-v2.5-pro"],
  },
  {
    id: "packy-cx2cc",
    name: "Packy CX2CC",
    base_url: "https://www.packyapi.com",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "Packy CX2CC",
    key_placeholder: "your-packy-cx2cc-key",
    category: "provider",
  },

  // ── API Proxy ──
  {
    id: "vercel",
    name: "Vercel AI Gateway",
    base_url: "https://ai-gateway.vercel.sh",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "Vercel unified gateway",
    key_placeholder: "your-ai-gateway-api-key",
    category: "proxy",
    docs_url: "https://vercel.com/docs/ai-gateway/anthropic-compat",
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    base_url: "https://openrouter.ai/api",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "Multi-provider gateway",
    key_placeholder: "your-openrouter-key",
    category: "proxy",
    docs_url: "https://openrouter.ai/docs/guides/guides/claude-code-integration",
  },
  {
    id: "aihubmix",
    name: "AiHubMix",
    base_url: "https://aihubmix.com",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "AI aggregation platform",
    key_placeholder: "your-aihubmix-key",
    category: "proxy",
  },
  {
    id: "zenmux",
    name: "ZenMux",
    base_url: "https://zenmux.ai/api/anthropic",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "Multi-model API gateway",
    key_placeholder: "sk-ss-v1-xxx",
    category: "proxy",
    extra_env: { API_TIMEOUT_MS: "30000000" },
    docs_url: "https://zenmux.ai/docs/best-practices/claude-code.html",
  },

  // ── Local Proxy ──
  {
    id: "ccswitch",
    name: "CC Switch",
    base_url: "http://127.0.0.1:15721",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "CC Switch local proxy",
    key_placeholder: "(leave empty)",
    category: "local",
    docs_url: "https://github.com/farion1231/cc-switch",
    setup_hint: "settings_local_setupHint_ccswitch",
  },
  {
    id: "ccr",
    name: "Claude Code Router",
    base_url: "http://127.0.0.1:3456",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "Local proxy for third-party providers",
    key_placeholder: "(leave empty)",
    models: ["claude-sonnet-4-6"],
    category: "local",
    docs_url: "https://github.com/musistudio/claude-code-router",
    setup_hint: "settings_local_setupHint_ccr",
  },

  // ── Local Inference ──
  {
    id: "ollama",
    name: "Ollama",
    base_url: "http://localhost:11434",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "Local LLM (no key needed)",
    key_placeholder: "(leave empty for local)",
    category: "local",
    setup_hint: "settings_local_setupHint_ollama",
  },

  // ── Custom ──
  {
    id: "custom",
    name: "Custom",
    base_url: "",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "Custom API endpoint",
    key_placeholder: "your-api-key",
    category: "custom",
  },
];

export const PRESET_CATEGORIES = [
  { id: "provider", label: "LLM Providers" },
  { id: "proxy", label: "API Proxy" },
  { id: "local", label: "Local" },
  { id: "custom", label: "Custom" },
] as const;

/**
 * Build a merged platform list from static presets + dynamic custom credentials.
 * Excludes the single "custom" placeholder — custom entries use "custom-{timestamp}" ids.
 */
export function buildPlatformList(
  credentials: import("$lib/types").PlatformCredential[],
): PlatformPreset[] {
  const builtins = PLATFORM_PRESETS.filter((p) => p.id !== "custom");
  const customs: PlatformPreset[] = credentials
    .filter((c) => c.platform_id.startsWith("custom-"))
    .map((c) => ({
      id: c.platform_id,
      name: c.name ?? "Custom",
      base_url: c.base_url ?? "",
      auth_env_var: (c.auth_env_var ?? "ANTHROPIC_AUTH_TOKEN") as PlatformPreset["auth_env_var"],
      description: "Custom endpoint",
      key_placeholder: "your-api-key",
      category: "custom" as const,
      models: c.models,
      extra_env: c.extra_env,
    }));
  return [...builtins, ...customs];
}

/**
 * Expand a credential's models array into [opus, sonnet, haiku] tier tuple.
 * Mirrors backend resolve_model_tiers expansion:
 * 1 model → all same; 2 models → [0]=opus+sonnet, [1]=haiku; 3+ → positional.
 */
export function expandModelsToTiers(models?: string[]): [string, string, string] {
  if (!models || models.length === 0) return ["", "", ""];
  if (models.length === 1) return [models[0], models[0], models[0]];
  if (models.length === 2) return [models[0], models[0], models[1]];
  return [models[0], models[1], models[2]];
}

/**
 * Compress [opus, sonnet, haiku] tier inputs back to a models array for storage.
 * Returns undefined when all three are empty (→ backend falls back to preset).
 */
export function compressModelsFromTiers(
  opus: string,
  sonnet: string,
  haiku: string,
): string[] | undefined {
  const o = opus.trim(),
    s = sonnet.trim(),
    h = haiku.trim();
  if (!o && !s && !h) return undefined;
  return [o, s, h];
}

/** Check if a platform_id represents a user-created custom endpoint. */
export function isCustomPlatform(platformId: string): boolean {
  return platformId.startsWith("custom-");
}

/** Find a credential by platform_id. */
export function findCredential(
  credentials: import("$lib/types").PlatformCredential[],
  platformId: string,
): import("$lib/types").PlatformCredential | undefined {
  return credentials.find((c) => c.platform_id === platformId);
}
