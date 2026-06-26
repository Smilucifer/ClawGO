import * as api from "$lib/api";
import type { ClaudeSubscriptionUsage } from "$lib/types";

export class ClaudeUsageStore {
  data = $state<ClaudeSubscriptionUsage | null>(null);
  loading = $state(false);

  async refresh(): Promise<void> {
    this.loading = true;
    try {
      const next = await api.getClaudeSubscriptionUsage();
      // 成功才覆盖；后端用 error 字段表达软失败，仍覆盖以更新 error 状态
      this.data = next;
    } catch {
      // 硬失败（IPC 异常）：保留上一次数据，标记 stale 由 fetched_at 体现
    } finally {
      this.loading = false;
    }
  }
}

/** 单例：聊天页共享。 */
export const claudeUsageStore = new ClaudeUsageStore();
