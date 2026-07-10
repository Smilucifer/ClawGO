// Shared visual helpers for the 新闻/舆论 columns.
// These duplicate the small mapping fns that used to live inside EventWatchTab.svelte;
// extracting them keeps NewsFlashColumn and NewsDigestColumn DRY without pulling
// in a wider utility barrel.

import type { MessageKey } from "$lib/i18n/types";

export type Stance = "bullish" | "bearish" | "neutral" | string;
export type Severity = "high" | "medium" | "low" | string;

/** Background token for a severity chip. 红涨绿跌 is applied by stanceColor(), not here. */
export function severityBadgeBg(severity: Severity): string {
  switch (severity) {
    case "high":
      return "var(--color-error-bg)";
    case "medium":
      return "var(--color-warning-bg)";
    default:
      return "var(--bg-hover)";
  }
}

/** Foreground color for a stance chip. 偏多=RED(--up), 偏空=GREEN(--down). */
export function stanceColor(stance: Stance): string {
  switch (stance) {
    case "bullish":
      return "var(--up)";
    case "bearish":
      return "var(--down)";
    default:
      return "var(--text-tertiary)";
  }
}

export function severityLabel(
  severity: Severity,
  t: (key: MessageKey) => string,
): string {
  switch (severity) {
    case "high":
      return t("invest.eventWatch.filterHigh");
    case "medium":
      return t("invest.eventWatch.filterMedium");
    case "low":
      return t("invest.eventWatch.filterLow");
    default:
      return severity;
  }
}

export function stanceLabel(
  stance: Stance,
  t: (key: MessageKey) => string,
): string {
  switch (stance) {
    case "bullish":
      return t("invest.eventWatch.stanceBullish");
    case "bearish":
      return t("invest.eventWatch.stanceBearish");
    case "neutral":
      return t("invest.eventWatch.stanceNeutral");
    default:
      return stance;
  }
}

/** Compact relative time — s / m / h / d. Falls back to the raw string on parse failure. */
export function formatRelativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return iso;
  const diffMs = Date.now() - then;
  if (diffMs < 0) return "0s";
  const sec = Math.floor(diffMs / 1000);
  if (sec < 60) return `${sec}s`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h`;
  const day = Math.floor(hr / 24);
  return `${day}d`;
}

/** Split comma-separated sector string into a clean array. */
export function splitCsv(input: string | null | undefined): string[] {
  if (!input) return [];
  return input
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}
