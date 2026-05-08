import { describe, expect, it } from "vitest";
import { getPhase7Provider, PHASE7_PROVIDERS, providerIdForRun } from "./provider-catalog";

describe("Phase 7 provider catalog", () => {
  it("exposes the visible providers in product order", () => {
    expect(PHASE7_PROVIDERS.map((provider) => provider.label)).toEqual([
      "Claude",
      "Codex",
      "DeepSeek",
      "GLM",
      "QWEN",
      "KIMI",
      "Xiaomi MiMo",
      "Xiaomi MiMo (按量)",
      "Packy CX2CC",
    ]);
  });

  it("keeps API providers on Claude execution with provider identity metadata", () => {
    expect(getPhase7Provider("deepseek")).toMatchObject({
      mode: "claude_compatible_api",
      executionAgent: "claude",
      platformId: "deepseek",
      defaultModel: "deepseek-v4-pro",
      requiredConfig: ["api_key"],
      defaultPermissionMode: "bypass",
    });
    expect(getPhase7Provider("glm")).toMatchObject({
      mode: "claude_compatible_api",
      executionAgent: "claude",
      platformId: "zhipu",
      requiredConfig: ["api_key", "base_url", "model"],
      defaultPermissionMode: "bypass",
    });
    expect(getPhase7Provider("qwen")).toMatchObject({
      mode: "claude_compatible_api",
      executionAgent: "claude",
      platformId: "bailian",
      defaultModel: "qwen3.5-plus",
      requiredConfig: ["api_key", "base_url", "model"],
      defaultPermissionMode: "bypass",
    });
    expect(getPhase7Provider("kimi")).toMatchObject({
      mode: "claude_compatible_api",
      executionAgent: "claude",
      platformId: "kimi",
      defaultModel: "kimi-k2.5",
      requiredConfig: ["api_key", "base_url", "model"],
      defaultPermissionMode: "bypass",
    });
    expect(getPhase7Provider("mimo-pro")).toMatchObject({
      mode: "claude_compatible_api",
      executionAgent: "claude",
      platformId: "mimo-pro",
      defaultModel: "mimo-v2.5-pro",
      requiredConfig: ["api_key"],
      defaultPermissionMode: "bypass",
    });
    expect(getPhase7Provider("xiaomi")).toMatchObject({
      mode: "claude_compatible_api",
      executionAgent: "claude",
      platformId: "xiaomi",
      defaultModel: "mimo-v2.5-pro",
      defaultBaseUrl: "https://api.xiaomimimo.com/anthropic",
      requiredConfig: ["api_key"],
      defaultPermissionMode: "bypass",
    });
    expect(getPhase7Provider("packy-cx2cc")).toMatchObject({
      mode: "claude_compatible_api",
      executionAgent: "claude",
      platformId: "packy-cx2cc",
      defaultBaseUrl: "https://www.packyapi.com/anthropic",
      requiredConfig: ["api_key"],
      defaultPermissionMode: "bypass",
    });
  });

  it("assigns required settings fields for fixed and parameterized API providers", () => {
    expect(getPhase7Provider("deepseek")).toMatchObject({
      requiredConfig: ["api_key"],
      defaultModel: "deepseek-v4-pro",
    });
    expect(getPhase7Provider("glm")).toMatchObject({
      requiredConfig: ["api_key", "base_url", "model"],
    });
    expect(getPhase7Provider("qwen")).toMatchObject({
      requiredConfig: ["api_key", "base_url", "model"],
    });
    expect(getPhase7Provider("kimi")).toMatchObject({
      requiredConfig: ["api_key", "base_url", "model"],
    });
  });

  it("maps run execution identity back to visible provider identity", () => {
    expect(providerIdForRun("claude", "deepseek")).toBe("deepseek");
    expect(providerIdForRun("claude", "zhipu")).toBe("glm");
    expect(providerIdForRun("claude", "bailian")).toBe("qwen");
    expect(providerIdForRun("claude", "kimi")).toBe("kimi");
    expect(providerIdForRun("claude", "mimo-pro")).toBe("mimo-pro");
    expect(providerIdForRun("claude", "xiaomi")).toBe("xiaomi");
    expect(providerIdForRun("claude", "packy-cx2cc")).toBe("packy-cx2cc");
    expect(providerIdForRun("codex")).toBe("codex");
    expect(providerIdForRun("claude")).toBe("claude");
  });
});
