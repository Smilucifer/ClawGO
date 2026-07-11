import type { FortuneLevel } from "$lib/stores/fortune-store.svelte";

const LABELS: Record<FortuneLevel, string> = {
  great_fortune: "大吉", fortune: "吉", neutral: "平",
  misfortune: "凶", great_misfortune: "大凶",
};
// 吉凶色纳入暖色系：大吉/吉=红→琥珀，平=灰，凶/大凶=浅绿→深绿（红涨绿跌一致）
const COLORS: Record<FortuneLevel, string> = {
  great_fortune: "var(--up)", fortune: "#c99a5e", neutral: "var(--flat)",
  misfortune: "#7f9d6d", great_misfortune: "#5f7a52",
};
export function levelLabel(l: FortuneLevel): string { return LABELS[l]; }
export function levelColor(l: FortuneLevel): string { return COLORS[l]; }
export function fmtScore(n: number): string { return n.toFixed(0); }
export function fmtReturn(pct: number): string {
  const s = pct >= 0 ? "+" : "";
  return `${s}${pct.toFixed(2)}%`;
}
/** 收益正负 → 红涨绿跌颜色 */
export function returnColor(pct: number): string {
  if (pct > 0) return "var(--up)";
  if (pct < 0) return "var(--down)";
  return "var(--flat)";
}
