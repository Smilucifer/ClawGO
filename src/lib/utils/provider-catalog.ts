export type ProviderMode = "official_cli" | "claude_compatible_api";
export type ExecutionAgent = "claude" | "codex";
export type Phase7ProviderId =
  | "claude"
  | "codex"
  | "deepseek"
  | "glm"
  | "qwen"
  | "kimi"
  | "mimo-pro";

export interface Phase7ProviderEntry {
  id: Phase7ProviderId;
  label: string;
  mode: ProviderMode;
  executionAgent: ExecutionAgent;
  platformId?: string;
  defaultModel?: string;
  defaultBaseUrl?: string;
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
    requiredConfig: [],
    defaultPermissionMode: "bypass",
  },
  {
    id: "codex",
    label: "Codex",
    mode: "official_cli",
    executionAgent: "codex",
    defaultModel: "gpt-5.5",
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
    requiredConfig: ["api_key", "base_url", "model"],
    defaultPermissionMode: "bypass",
  },
  {
    id: "mimo-pro",
    label: "MiMo Pro",
    mode: "claude_compatible_api",
    executionAgent: "claude",
    platformId: "mimo-pro",
    defaultModel: "mimo-v2.5-pro",
    defaultBaseUrl: "https://token-plan-cn.xiaomimimo.com/anthropic",
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
  if (platformId === "mimo-pro") return "mimo-pro";
  if (agent === "codex") return "codex";
  return "claude";
}
