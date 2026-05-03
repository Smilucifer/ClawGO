import type { TaskNotificationItem } from "$lib/stores/session-store.svelte";

const DONE_TASK_STATUSES = new Set(["completed"]);
const FAILED_TASK_STATUSES = new Set(["failed", "error"]);
const CANCELLED_TASK_STATUSES = new Set(["cancelled", "canceled"]);
const INACTIVE_TASK_STATUSES = new Set([
  ...DONE_TASK_STATUSES,
  ...FAILED_TASK_STATUSES,
  ...CANCELLED_TASK_STATUSES,
]);

export type BackgroundTaskDisplayStatus = "running" | "done" | "error" | "other";

export function isActiveBackgroundTask(item: TaskNotificationItem): boolean {
  return !INACTIVE_TASK_STATUSES.has(item.status);
}

export function getBackgroundTaskDisplayStatus(
  item: TaskNotificationItem,
): BackgroundTaskDisplayStatus {
  if (isActiveBackgroundTask(item)) return "running";
  if (DONE_TASK_STATUSES.has(item.status)) return "done";
  if (FAILED_TASK_STATUSES.has(item.status)) return "error";
  return "other";
}

export function sortBackgroundTasks(items: TaskNotificationItem[]): TaskNotificationItem[] {
  return [...items].sort((a, b) => {
    const aActive = isActiveBackgroundTask(a) ? 0 : 1;
    const bActive = isActiveBackgroundTask(b) ? 0 : 1;
    if (aActive !== bActive) return aActive - bActive;
    return b.startedAt - a.startedAt;
  });
}
