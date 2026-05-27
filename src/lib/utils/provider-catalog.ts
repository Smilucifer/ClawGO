export type ProviderMode = "official_cli" | "claude_compatible_api";
export type ExecutionAgent = "claude" | "codex";
export type Phase7ProviderId =
  | "claude"
  | "codex"
  | "deepseek"
  | "glm"
  | "qwen"
  | "kimi"
  | "mimo-plan"
  | "mimo-api";

export interface Phase7ProviderEntry {
  id: Phase7ProviderId;
  label: string;
  mode: ProviderMode;
  executionAgent: ExecutionAgent;
  platformId?: string;
  defaultModel?: string;
  defaultBaseUrl?: string;
  contextWindow?: number;
  requiredConfig: Array<"api_key" | "base_url" | "model">;
  defaultPermissionMode: "bypass" | "dangerously_bypass" | "yolo";
}

export const PHASE7_PROVIDERS: Phase7ProviderEntry[] = [
  {
    id: "claude",
    label: "Claude",
    mode: "official_cli",
    executionAgent: "claude",
    defaultModel: "claude-opus-4-7[1m]",
    contextWindow: 1_000_000,
    requiredConfig: [],
    defaultPermissionMode: "bypass",
  },
  {
    id: "codex",
    label: "Codex",
    mode: "official_cli",
    executionAgent: "codex",
    defaultModel: "gpt-5.5",
    contextWindow: 1_000_000,
    requiredConfig: [],
    defaultPermissionMode: "dangerously_bypass",
  },
  {
    id: "deepseek",
    label: "DeepSeek",
    mode: "claude_compatible_api",
    executionAgent: "claude",
    platformId: "deepseek",
    defaultModel: "deepseek-v4-pro",
    defaultBaseUrl: "https://api.deepseek.com/anthropic",
    contextWindow: 1_000_000,
    requiredConfig: ["api_key"],
    defaultPermissionMode: "bypass",
  },
  {
    id: "glm",
    label: "GLM",
    mode: "claude_compatible_api",
    executionAgent: "claude",
    platformId: "zhipu",
    defaultModel: "glm-5",
    defaultBaseUrl: "https://open.bigmodel.cn/api/anthropic",
    contextWindow: 200_000,
    requiredConfig: ["api_key", "base_url", "model"],
    defaultPermissionMode: "bypass",
  },
  {
    id: "qwen",
    label: "QWEN",
    mode: "claude_compatible_api",
    executionAgent: "claude",
    platformId: "bailian",
    defaultModel: "qwen3.5-plus",
    defaultBaseUrl: "https://coding.dashscope.aliyuncs.com/apps/anthropic",
    contextWindow: 1_000_000,
    requiredConfig: ["api_key", "base_url", "model"],
    defaultPermissionMode: "bypass",
  },
  {
    id: "kimi",
    label: "KIMI",
    mode: "claude_compatible_api",
    executionAgent: "claude",
    platformId: "kimi",
    defaultModel: "kimi-k2.5",
    defaultBaseUrl: "https://api.moonshot.cn/anthropic",
    contextWindow: 256_000,
    requiredConfig: ["api_key", "base_url", "model"],
    defaultPermissionMode: "bypass",
  },
  {
    id: "mimo-plan",
    label: "Xiaomi (Plan)",
    mode: "claude_compatible_api",
    executionAgent: "claude",
    platformId: "mimo-plan",
    defaultModel: "mimo-v2.5-pro",
    defaultBaseUrl: "https://token-plan-cn.xiaomimimo.com/anthropic",
    contextWindow: 1_000_000,
    requiredConfig: ["api_key"],
    defaultPermissionMode: "bypass",
  },
  {
    id: "mimo-api",
    label: "Xiaomi (API)",
    mode: "claude_compatible_api",
    executionAgent: "claude",
    platformId: "mimo-api",
    defaultModel: "mimo-v2.5-pro",
    defaultBaseUrl: "https://api.xiaomimimo.com/anthropic",
    contextWindow: 1_000_000,
    requiredConfig: ["api_key"],
    defaultPermissionMode: "bypass",
  },
];

export function getPhase7Provider(id: string): Phase7ProviderEntry {
  return PHASE7_PROVIDERS.find((provider) => provider.id === id) ?? PHASE7_PROVIDERS[0];
}

export function providerIdForRun(agent: string, platformId?: string | null): Phase7ProviderId {
  if (platformId === "deepseek") return "deepseek";
  if (platformId === "zhipu" || platformId === "zhipu-intl") return "glm";
  if (platformId === "bailian") return "qwen";
  if (platformId === "kimi") return "kimi";
  if (platformId === "mimo-plan") return "mimo-plan";
  if (platformId === "mimo-api") return "mimo-api";
  if (agent === "codex") return "codex";
  return "claude";
}

/** Static model → context window mapping (tokens). */
export const MODEL_CONTEXT_WINDOWS: Record<string, number> = {
  "claude-opus-4-7": 1_000_000,
  "claude-sonnet-4-6": 1_000_000,
  "claude-haiku-4-5": 200_000,
  "deepseek-v4-pro": 1_000_000,
  "deepseek-v4-flash": 1_000_000,
  "qwen3.5-plus": 1_000_000,
  "qwen3.6-plus": 1_000_000,
  "qwen3.7-max": 1_000_000,
  "qwen-long": 10_000_000,
  "mimo-v2.5-pro": 1_000_000,
  "kimi-k2.5": 256_000,
  "glm-5": 200_000,
  "glm-4.7": 200_000,
};

/** Look up context window by model name. Returns 200_000 as conservative fallback. */
export function getContextWindowForModel(modelName: string): number {
  // Strip tier suffix like "[1m]" from model names
  const base = modelName.replace(/\[.*\]$/, "");
  return MODEL_CONTEXT_WINDOWS[base] ?? 200_000;
}

/** Look up context window by platformId via the provider catalog. */
export function getContextWindowForPlatform(platformId: string): number {
  const provider = PHASE7_PROVIDERS.find((p) => p.platformId === platformId);
  return provider?.contextWindow ?? 200_000;
}
