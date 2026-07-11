import { describe, it, expect, vi, beforeEach } from "vitest";

const invokeMock = vi.fn();
vi.mock("$lib/transport", () => ({ getTransport: () => ({ invoke: invokeMock }) }));

describe("fortuneStore", () => {
  beforeEach(() => { invokeMock.mockReset(); });

  it("upsert 后重新 loadAll", async () => {
    const { fortuneStore } = await import("./fortune-store.svelte");
    invokeMock.mockResolvedValue({ today: null, tomorrow: null, calendar: [] });
    invokeMock.mockResolvedValueOnce(undefined);  // upsert 调用
    await fortuneStore.upsert("2026-07-01", 1.5);
    // upsert(1) + loadAll 的 3 个查询 = 4 次
    expect(invokeMock).toHaveBeenCalledWith("fortune_upsert_return",
      { date: "2026-07-01", returnPct: 1.5, note: "" });
  });
});
