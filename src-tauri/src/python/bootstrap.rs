//! Python environment bootstrap helpers.
//!
//! Emits Tauri events for the frontend loading overlay. The actual verification
//! (path resolution, subprocess spawn, health check, version query) is now done
//! entirely in `python::init()` — no separate Python process is spawned.

use std::sync::Mutex;
use tauri::Emitter;

/// Event name emitted during Python environment setup.
pub const SETUP_EVENT: &str = "python://setup-progress";

/// Stored progress state for late-joining frontend listeners.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressSnapshot {
    pub stage: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Global storage for the latest progress snapshot.
static LAST_PROGRESS: Mutex<Option<ProgressSnapshot>> = Mutex::new(None);

/// Emit a setup progress event to the frontend and store the snapshot.
///
/// `stage` is one of: "starting", "ready", "error".
/// `message` is optional detail (used for error messages).
pub fn emit_progress(app_handle: &tauri::AppHandle, stage: &str, message: Option<&str>) {
    let snapshot = ProgressSnapshot {
        stage: stage.to_string(),
        message: message.unwrap_or("").to_string(),
        error: None,
    };

    // Store for late-joining listeners
    if let Ok(mut last) = LAST_PROGRESS.lock() {
        *last = Some(snapshot);
    }

    let payload = serde_json::json!({
        "stage": stage,
        "message": message,
    });
    if let Err(e) = app_handle.emit(SETUP_EVENT, payload) {
        log::warn!("[python-bootstrap] Failed to emit progress event: {}", e);
    }
}

/// Get the latest progress snapshot (for poll-based queries).
pub fn get_last_progress() -> Option<ProgressSnapshot> {
    LAST_PROGRESS.lock().ok().and_then(|guard| guard.clone())
}
