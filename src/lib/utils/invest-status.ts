/**
 * Shared status helpers for invest module (Scheduler, Dreaming, etc.)
 * Single lookup table for dot and text CSS classes.
 */

type StatusEntry = { dot: string; text: string };

const STATUS_MAP: Record<string, StatusEntry> = {
  ok:          { dot: 'ok',      text: 'text-[var(--color-ok)]' },
  completed:   { dot: 'ok',      text: 'text-[var(--color-ok)]' },
  error:       { dot: 'error',   text: 'text-[var(--color-error)]' },
  failed:      { dot: 'error',   text: 'text-[var(--color-error)]' },
  rolled_back: { dot: 'skipped', text: 'text-[var(--color-warning)]' },
  skipped:     { dot: 'skipped', text: 'text-[var(--text-tertiary)]' },
};

const DEFAULT: StatusEntry = { dot: 'pending', text: 'text-[var(--text-tertiary)]' };

/** Dot class: used for `.status-dot` and `.timeline-dot` backgrounds. */
export function investStatusDotClass(status?: string): string {
  return (status ? STATUS_MAP[status] : DEFAULT)?.dot ?? DEFAULT.dot;
}

/** Text class: Tailwind utility for status labels. */
export function investStatusTextClass(status?: string): string {
  return (status ? STATUS_MAP[status] : DEFAULT)?.text ?? DEFAULT.text;
}
