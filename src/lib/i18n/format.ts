/**
 * Locale-aware formatting functions based on Intl API.
 *
 * All functions use currentLocale() for locale-sensitive output.
 * Every function includes Invalid Date / NaN / Infinity guards.
 */
import { currentLocale } from "./index.svelte";

// ── Helpers ─────────────────────────────────────────────────────

function toDate(d: Date | string): Date {
  return typeof d === "string" ? new Date(d) : d;
}

function isValidDate(d: Date): boolean {
  return !isNaN(d.getTime());
}

// ── Number formatting ───────────────────────────────────────────

/** Format a number with locale-aware thousand separators. NaN/Infinity → "0". */
export function fmtNumber(n: number): string {
  if (isNaN(n) || !isFinite(n)) return "0";
  return new Intl.NumberFormat(currentLocale()).format(n);
}

// ── Date/time formatting ────────────────────────────────────────

/** Time only: "12:30". Invalid Date → "—". */
export function fmtTime(d: Date | string): string {
  const date = toDate(d);
  if (!isValidDate(date)) return "—";
  return new Intl.DateTimeFormat(currentLocale(), {
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

/** Short date: "Feb 20" / "2月20日". Invalid Date → "—". */
export function fmtDate(d: Date | string): string {
  const date = toDate(d);
  if (!isValidDate(date)) return "—";
  return new Intl.DateTimeFormat(currentLocale(), {
    month: "short",
    day: "numeric",
  }).format(date);
}

/** Date + time: "2/20 12:30". Invalid Date → "—". */
export function fmtDateTime(d: Date | string): string {
  const date = toDate(d);
  if (!isValidDate(date)) return "—";
  return new Intl.DateTimeFormat(currentLocale(), {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

/** Full date-time (for tooltips): "2026/2/20 12:30:45". Invalid Date → "—". */
export function fmtFull(d: Date | string): string {
  const date = toDate(d);
  if (!isValidDate(date)) return "—";
  return new Intl.DateTimeFormat(currentLocale(), {
    year: "numeric",
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(date);
}

// ── Relative time ───────────────────────────────────────────────

/** Relative time: "3 minutes ago" / "3 分钟前". Invalid Date → "—". */
export function fmtRelative(d: Date | string): string {
  const date = toDate(d);
  if (!isValidDate(date)) return "—";

  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffSec = Math.floor(diffMs / 1000);
  const diffMin = Math.floor(diffSec / 60);
  const diffHr = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHr / 24);

  const rtf = new Intl.RelativeTimeFormat(currentLocale(), { numeric: "auto" });

  if (diffSec < 10) return rtf.format(0, "second"); // "now" / "现在"
  if (diffSec < 60) return rtf.format(-diffSec, "second");
  if (diffMin < 60) return rtf.format(-diffMin, "minute");
  if (diffHr < 24) return rtf.format(-diffHr, "hour");
  if (diffDay < 7) return rtf.format(-diffDay, "day");

  // Beyond 7 days: show formatted date
  return fmtDate(date);
}

// ── Invest date helpers ─────────────────────────────────────────

/** Invest 统计截止小时：每天 05:00 之前归属前一天。与 Rust `INVEST_DATE_CUTOFF_HOUR` 对齐。 */
const INVEST_DATE_CUTOFF_HOUR = 5;

/**
 * 返回 invest 统计日期（YYYY-MM-DD，本地时区）。
 * 05:00 之前返回昨天，05:00 及之后返回今天。
 */
export function getInvestDate(): string {
  const now = new Date();
  if (now.getHours() < INVEST_DATE_CUTOFF_HOUR) {
    now.setDate(now.getDate() - 1);
  }
  const y = now.getFullYear();
  const m = String(now.getMonth() + 1).padStart(2, "0");
  const d = String(now.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}
