import type zhCN from "$messages/zh-CN.json";

/** Union of all valid message keys (derived from zh-CN.json). */
export type MessageKey = Extract<keyof typeof zhCN, string>;

/** Variables for interpolation: `{ variable: string }`. */
export type MessageParams = Record<string, string>;

/** Re-export Locale from registry for convenience. */
export type { Locale } from "./registry";
