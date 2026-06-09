import { describe, it, expect } from "vitest";
import { encodeCwdSlug } from "./format";

describe("encodeCwdSlug", () => {
  it("replaces forward slashes with hyphens", () => {
    expect(encodeCwdSlug("/Users/alice/project")).toBe("-Users-alice-project");
  });

  it("replaces backslashes with hyphens", () => {
    expect(encodeCwdSlug("C:\\Users\\alice\\project")).toBe(
      "C--Users-alice-project",
    );
  });

  it("replaces colons with hyphens (Windows drive letter)", () => {
    expect(encodeCwdSlug("D:\\ClaudeWorkspace\\Code")).toBe(
      "D--ClaudeWorkspace-Code",
    );
  });

  it("handles mixed separators", () => {
    expect(encodeCwdSlug("C:/Users\\alice/project")).toBe(
      "C--Users-alice-project",
    );
  });

  it("handles root path", () => {
    expect(encodeCwdSlug("/")).toBe("-");
  });

  it("handles relative path unchanged", () => {
    expect(encodeCwdSlug("relative")).toBe("relative");
  });
});
