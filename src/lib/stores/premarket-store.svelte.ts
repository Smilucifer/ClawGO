import { getTransport } from '$lib/transport';

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return getTransport().invoke<T>(cmd, args);
}

/**
 * 盘前报告生成状态单例。生命周期(generate/秒表)持在模块级,
 * 切 tab 卸载 PremarketReportTab 也不中断;重挂载时组件从此读回状态。
 */
export class PremarketStore {
  generating = $state<boolean>(false);
  startedAt = $state<number>(0);
  elapsedMs = $state<number>(0);
  lastElapsedMs = $state<number>(0);
  lastError = $state<string | null>(null);
  /** 每次生成完成自增;组件 $effect 观察它触发 loadLatest。 */
  completionSeq = $state<number>(0);

  markStart(now: number): void {
    this.generating = true;
    this.startedAt = now;
    this.elapsedMs = 0;
    this.lastError = null;
  }

  /** 秒表 tick:更新 elapsedMs(组件每秒调一次)。 */
  tick(now: number): void {
    if (this.generating) this.elapsedMs = now - this.startedAt;
  }

  markFinish(err: string | null, now: number): void {
    this.lastElapsedMs = now - this.startedAt;
    this.elapsedMs = this.lastElapsedMs;
    this.lastError = err;
    this.generating = false;
    this.completionSeq += 1;
  }

  elapsedSec(now: number): number {
    return Math.floor((now - this.startedAt) / 1000);
  }

  /** 完整生成生命周期:优先 cron dispatcher,失败回退 direct;结束 markFinish。 */
  async generate(): Promise<void> {
    if (this.generating) return;
    this.markStart(Date.now());
    try {
      try {
        await invoke<string>('trigger_cron_job', { id: 'premarket_report' });
      } catch (cronErr) {
        console.warn('[premarket] cron trigger failed, fallback:', cronErr);
        await invoke<string>('generate_premarket_report_cmd');
      }
      this.markFinish(null, Date.now());
    } catch (e) {
      console.error('[premarket] generate:', e);
      this.markFinish(String(e), Date.now());
    }
  }
}

export const premarketStore = new PremarketStore();
