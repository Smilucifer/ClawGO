export function guardedSave<T extends unknown[]>(
  action: (...args: T) => Promise<void>,
  options?: { onError?: (e: unknown) => void },
): (...args: T) => Promise<void> {
  let saving = false;
  return async (...args: T) => {
    if (saving) return;
    saving = true;
    try {
      await action(...args);
    } catch (e) {
      if (options?.onError) {
        options.onError(e);
      } else {
        console.error(e);
      }
    } finally {
      saving = false;
    }
  };
}
