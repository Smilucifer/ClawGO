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
      "Xiaomi (Plan)",
      "Xiaomi (API)",
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
    expect(getPhase7Provider("mimo-plan")).toMatchObject({
      mode: "claude_compatible_api",
      executionAgent: "claude",
      platformId: "mimo-plan",
      defaultModel: "mimo-v2.5-pro",
      requiredConfig: ["api_key"],
      defaultPermissionMode: "bypass",
    });
    expect(getPhase7Provider("mimo-api")).toMatchObject({
      mode: "claude_compatible_api",
      executionAgent: "claude",
      platformId: "mimo-api",
      defaultModel: "mimo-v2.5-pro",
      defaultBaseUrl: "https://api.xiaomimimo.com/anthropic",
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
    expect(providerIdForRun("claude", "mimo-plan")).toBe("mimo-plan");
    expect(providerIdForRun("claude", "mimo-api")).toBe("mimo-api");
    expect(providerIdForRun("codex")).toBe("codex");
    expect(providerIdForRun("claude")).toBe("claude");
  });

  it("falls back to claude for custom-* platform IDs", () => {
    expect(providerIdForRun("claude", "custom-1234567890")).toBe("claude");
    expect(providerIdForRun("claude", "custom-abc-def")).toBe("claude");
  });
});

