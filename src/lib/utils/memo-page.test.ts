import { describe, expect, it, vi } from "vitest";
import { shouldApplyMemoProjectCwdChange } from "./memo-page";

describe("shouldApplyMemoProjectCwdChange", () => {
  it("blocks project cwd changes when a dirty project memo is not discarded", () => {
    const confirmDiscard = vi.fn(() => false);

    expect(shouldApplyMemoProjectCwdChange("project", true, confirmDiscard)).toBe(false);
    expect(confirmDiscard).toHaveBeenCalledTimes(1);
  });

  it("does not prompt for global memo scope when project cwd changes", () => {
    const confirmDiscard = vi.fn(() => false);

    expect(shouldApplyMemoProjectCwdChange("global", true, confirmDiscard)).toBe(true);
    expect(confirmDiscard).not.toHaveBeenCalled();
  });
});
