import type { TaskRun } from "$lib/types";
import { providerIdForRun } from "./provider-catalog";

const FINISHED_STATUSES = new Set(["completed", "stopped", "failed"]);

export function findLastContinuableRun(runs: TaskRun[], providerId: string): TaskRun | null {
  const normalizedProvider = providerId.trim().toLowerCase();
  return (
    runs.find((run) => {
      if (providerIdForRun(run.agent, run.platform_id) !== normalizedProvider) return false;
      if (!FINISHED_STATUSES.has(run.status)) return false;

      if (normalizedProvider === "codex") {
        return run.conversation_ref?.kind === "codex_thread";
      }
      return run.agent === "claude" && !!run.session_id;
    }) ?? null
  );
}
