/**
 * ESM default-export interop for dynamic imports.
 *
 * Some bundlers wrap CJS modules in a `{ default: ... }` namespace,
 * while ESM-native modules expose the value directly. This helper
 * normalises both shapes so callers don't repeat the `mod.default ?? mod` dance.
 */
export function interopDefault<T>(mod: { default?: T } | T): T {
  return (mod as { default?: T }).default ?? (mod as T);
}
