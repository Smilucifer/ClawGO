import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("$lib/api", () => ({
  getClaudeSubscriptionUsage: vi.fn(),
}));
import { getClaudeSubscriptionUsage } from "$lib/api";
import { ClaudeUsageStore } from "./claude-usage-store.svelte";

describe("ClaudeUsageStore", () => {
  beforeEach(() => vi.resetAllMocks());

  it("refresh stores fetched data", async () => {
    (getClaudeSubscriptionUsage as any).mockResolvedValue({
      five_hour: { utilization: 0.42, resets_at: null },
      seven_day: { utilization: 0.18, resets_at: null },
      seven_day_opus: null,
      subscription_type: "max",
      rate_limit_tier: "tier_x",
      fetched_at: "2026-06-26T00:00:00Z",
      error: null,
    });
    const store = new ClaudeUsageStore();
    await store.refresh();
    expect(store.data?.five_hour?.utilization).toBe(0.42);
    expect(store.loading).toBe(false);
  });

  it("keeps previous data on fetch error (stale)", async () => {
    const store = new ClaudeUsageStore();
    (getClaudeSubscriptionUsage as any).mockResolvedValue({
      five_hour: { utilization: 0.5, resets_at: null },
      seven_day: null, seven_day_opus: null,
      subscription_type: null, rate_limit_tier: null,
      fetched_at: "t1", error: null,
    });
    await store.refresh();
    (getClaudeSubscriptionUsage as any).mockRejectedValue(new Error("boom"));
    await store.refresh();
    expect(store.data?.five_hour?.utilization).toBe(0.5); // 保留上次
    expect(store.stale).toBe(true); // 硬失败标记 stale
  });
});
