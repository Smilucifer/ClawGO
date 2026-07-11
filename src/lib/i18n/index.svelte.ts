/**
 * i18n runtime for Claw GO — Chinese-only.
 *
 * The app ships a single locale (zh-CN). The `t()` / `currentLocale()` API is
 * kept so the ~2200 call sites and Intl formatting in format.ts stay untouched;
 * `t()` simply looks up zh-CN messages with `{variable}` interpolation.
 */
import zhCN from "$messages/zh-CN.json";
import { LOCALE_REGISTRY, SUPPORTED_LOCALES, BASE_LOCALE, isLocale, getEntry } from "./registry";
import type { Locale } from "./registry";
import type { MessageKey, MessageParams } from "./types";
import { dbgWarn } from "$lib/utils/debug";

// ── Re-exports from registry ────────────────────────────────────
export { LOCALE_REGISTRY, SUPPORTED_LOCALES, BASE_LOCALE, isLocale, getEntry };
export type { Locale };

// ── Backward-compat aliases ─────────────────────────────────────
export const baseLocale = BASE_LOCALE;
export const locales = SUPPORTED_LOCALES;

const messages = zhCN as Record<string, string>;

/**
 * Set `<html lang>` / `dir`. Call once in the root layout.
 */
export function initLocale(): void {
  if (typeof document !== "undefined") {
    document.documentElement.lang = BASE_LOCALE;
    document.documentElement.dir = getEntry(BASE_LOCALE)?.dir ?? "ltr";
  }
}

/** Reactive read of the current locale. Always zh-CN. */
export function currentLocale(): string {
  return BASE_LOCALE;
}

/**
 * Translate a message key with optional `{variable}` interpolation.
 * Falls back to the raw key when missing.
 */
export function t(key: MessageKey, params?: MessageParams): string {
  const value = messages[key];
  if (value === undefined) {
    if (import.meta.env.DEV) dbgWarn("i18n", `missing key: "${key}"`);
    return key;
  }
  if (params) {
    return value.replace(/\{(\w+)\}/g, (_, name: string) =>
      params[name] !== undefined ? params[name] : `{${name}}`,
    );
  }
  return value;
}
