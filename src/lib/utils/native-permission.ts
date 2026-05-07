const NATIVE_AGENTS = new Set(["codex"]);
const NATIVE_NO_REVIEW_MODES = new Set(["bypassPermissions", "dontAsk", "yolo", "auto_all"]);
const NATIVE_VISIBLE_MODES = new Set(["bypassPermissions"]);

export function isNativeAgent(agent: string): boolean {
  return NATIVE_AGENTS.has(agent);
}

export function permissionModeToNativeYolo(mode: string): boolean {
  return NATIVE_NO_REVIEW_MODES.has(mode) || mode === "default";
}

export function nativeYoloToPermissionMode(yoloMode?: boolean | null): string {
  void yoloMode;
  return "bypassPermissions";
}

export function isPermissionModeVisibleForAgent(agent: string, mode: string): boolean {
  if (!isNativeAgent(agent)) return true;
  return NATIVE_VISIBLE_MODES.has(mode);
}
