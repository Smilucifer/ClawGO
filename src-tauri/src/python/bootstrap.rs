//! Python environment bootstrap helpers.
//!
//! Emits Tauri events for the frontend loading overlay. The actual verification
//! (path resolution, subprocess spawn, health check, version query) is now done
//! entirely in `python::init()` — no separate Python process is spawned.

use tauri::Emitter;

/// Event name emitted during Python environment setup.
pub const SETUP_EVENT: &str = "python://setup-progress";

/// Emit a setup progress event to the frontend.
///
/// `stage` is one of: "starting", "ready", "error".
/// `message` is optional detail (used for error messages).
pub fn emit_progress(app_handle: &tauri::AppHandle, stage: &str, message: Option<&str>) {
    let payload = serde_json::json!({
        "stage": stage,
        "message": message,
    });
    if let Err(e) = app_handle.emit(SETUP_EVENT, payload) {
        log::warn!("[python-bootstrap] Failed to emit progress event: {}", e);
    }
}
