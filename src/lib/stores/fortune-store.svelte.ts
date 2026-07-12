import { getTransport } from "$lib/transport";

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return getTransport().invoke<T>(cmd, args);
}

export type FortuneLevel =
  | "great_fortune" | "fortune" | "neutral" | "misfortune" | "great_misfortune";

export interface DayScore {
  date: string; stem: string; branch: string;
  predictScore: number; predictLevel: FortuneLevel;
  actualReturn: number | null; postScore: number | null; postLevel: FortuneLevel | null;
  isTradingDay: boolean;
}
export interface LayerRow {
  name: string; avgReturn: number; winRate: number; sample: number;
  score: number; level: FortuneLevel;
}
export interface ForecastItem {
  label: string; date: string; weekday: string; ganzhi: string;
  score: number; level: FortuneLevel; isStrong: boolean;
}
export interface Analysis { today: DayScore | null; tomorrow: DayScore | null; calendar: DayScore[]; }
export interface Overview { stems: LayerRow[]; branches: LayerRow[]; forecasts: ForecastItem[]; }
export interface MonthStat { month: string; avgReturn: number; }
export interface DataSummary {
  totalDays: number; winDays: number; winRate: number;
  cumulativeReturn: number; avgDailyReturn: number;
  topStems: LayerRow[]; topBranches: LayerRow[];
  riskStems: LayerRow[]; riskBranches: LayerRow[]; monthly: MonthStat[];
}
export interface BatchEntry { date: string; returnPct: number; note: string; }

class FortuneStore {
  analysis = $state<Analysis | null>(null);
  overview = $state<Overview | null>(null);
  summary = $state<DataSummary | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);
  readingBusy = $state(false);

  /** Map of dates that already have recorded returns: date → returnPct. */
  get recordedMap(): Map<string, number> {
    const m = new Map<string, number>();
    for (const d of this.analysis?.calendar ?? []) {
      if (d.actualReturn != null) m.set(d.date, d.actualReturn);
    }
    return m;
  }

  /**
   * Find which of the given dates already have recorded data.
   * Returns `{ date, existingReturn }[]` — empty if no conflicts.
   */
  findConflicts(dates: string[]): { date: string; existingReturn: number }[] {
    const map = this.recordedMap;
    return dates
      .map((d) => ({ date: d, existingReturn: map.get(d)! }))
      .filter((c) => c.existingReturn != null);
  }

  async loadAll(): Promise<void> {
    this.loading = true; this.error = null;
    try {
      const [analysis, overview, summary] = await Promise.all([
        invoke<Analysis>("fortune_get_analysis"),
        invoke<Overview>("fortune_get_overview"),
        invoke<DataSummary>("fortune_get_data_summary"),
      ]);
      this.analysis = analysis; this.overview = overview; this.summary = summary;
    } catch (e) { this.error = String(e); }
    finally { this.loading = false; }
  }

  async upsert(date: string, returnPct: number, note = ""): Promise<void> {
    await invoke("fortune_upsert_return", { date, returnPct, note });
    await this.loadAll();   // invalidate：预测↔盘后自动切换
  }

  async batchUpsert(entries: BatchEntry[]): Promise<void> {
    await invoke("fortune_batch_upsert", { entries });
    await this.loadAll();
  }

  async deleteReturn(date: string): Promise<void> {
    await invoke("fortune_delete_return", { date });
    await this.loadAll();
  }

  async generateReading(date: string): Promise<string> {
    this.readingBusy = true;
    try { return await invoke<string>("fortune_generate_reading", { date }); }
    finally { this.readingBusy = false; }
  }

  async getReading(date: string): Promise<string | null> {
    return await invoke<string | null>("fortune_get_reading", { date });
  }
}

export const fortuneStore = new FortuneStore();
