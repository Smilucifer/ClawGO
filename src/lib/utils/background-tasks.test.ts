import { describe, expect, it } from "vitest";
import type { TaskNotificationItem } from "$lib/stores/session-store.svelte";
import {
  getBackgroundTaskDisplayStatus,
  isActiveBackgroundTask,
  sortBackgroundTasks,
} from "./background-tasks";

function task(
  task_id: string,
  status: string,
  startedAt: number,
  message = task_id,
): TaskNotificationItem {
  return {
    task_id,
    status,
    startedAt,
    message,
    data: {},
  };
}

describe("background task helpers", () => {
  it("treats unfinished task statuses as active", () => {
    expect(isActiveBackgroundTask(task("started", "started", 1))).toBe(true);
    expect(isActiveBackgroundTask(task("running", "running", 1))).toBe(true);
    expect(isActiveBackgroundTask(task("pending", "pending", 1))).toBe(true);
  });

  it("treats completed and failed statuses as inactive", () => {
    expect(isActiveBackgroundTask(task("completed", "completed", 1))).toBe(false);
    expect(isActiveBackgroundTask(task("failed", "failed", 1))).toBe(false);
    expect(isActiveBackgroundTask(task("error", "error", 1))).toBe(false);
    expect(isActiveBackgroundTask(task("cancelled", "cancelled", 1))).toBe(false);
    expect(isActiveBackgroundTask(task("canceled", "canceled", 1))).toBe(false);
  });

  it("classifies cancelled tasks as inactive neutral display states", () => {
    expect(getBackgroundTaskDisplayStatus(task("running", "running", 1))).toBe("running");
    expect(getBackgroundTaskDisplayStatus(task("completed", "completed", 1))).toBe("done");
    expect(getBackgroundTaskDisplayStatus(task("failed", "failed", 1))).toBe("error");
    expect(getBackgroundTaskDisplayStatus(task("cancelled", "cancelled", 1))).toBe("other");
    expect(getBackgroundTaskDisplayStatus(task("canceled", "canceled", 1))).toBe("other");
  });

  it("sorts active tasks first, then newest first", () => {
    const sorted = sortBackgroundTasks([
      task("old-done", "completed", 100),
      task("new-active", "running", 400),
      task("old-active", "started", 200),
      task("new-done", "completed", 300),
    ]);

    expect(sorted.map((item) => item.task_id)).toEqual([
      "new-active",
      "old-active",
      "new-done",
      "old-done",
    ]);
  });
});
