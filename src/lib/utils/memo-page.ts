export type MemoPageScopeTab = "global" | "project";

export function shouldApplyMemoProjectCwdChange(
  selectedScope: MemoPageScopeTab,
  isDirty: boolean,
  confirmDiscard: () => boolean,
): boolean {
  if (selectedScope !== "project") return true;
  if (!isDirty) return true;
  return confirmDiscard();
}
