/**
 * Async save guard — prevents double-submit and ensures the flag
 * is always reset, even on error.
 *
 * Usage (Svelte 5):
 *   let saving = $state(false);
 *   async function handleSave() {
 *     await guardedSave(saving, (v) => saving = v, async () => {
 *       await store.upsert(...);
 *       onclose();
 *     });
 *   }
 *
 * Returns true if the save ran, false if it was a no-op (already saving).
 */
export async function guardedSave(
  saving: boolean,
  setSaving: (v: boolean) => void,
  fn: () => Promise<void>,
  onError?: (e: unknown) => void,
): Promise<boolean> {
  if (saving) return false;
  setSaving(true);
  try {
    await fn();
  } catch (e) {
    (onError ?? defaultOnError)(e);
  } finally {
    setSaving(false);
  }
  return true;
}

function defaultOnError(e: unknown): void {
  console.error("[guardedSave] save failed:", e);
}
