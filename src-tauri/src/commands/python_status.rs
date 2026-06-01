//! Tauri commands for Python runtime status and management.

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonStatus {
    pub ready: bool,
    pub python_version: String,
    pub yfinance_version: String,
    pub error: Option<String>,
    /// Latest progress snapshot from bootstrap (for late-joining frontend).
    pub progress: Option<crate::python::bootstrap::ProgressSnapshot>,
}

/// Get the current Python runtime status.
///
/// When the runtime is healthy, returns version info captured at init time
/// (queried from the actual Python process, not hardcoded).
/// Also returns the latest bootstrap progress snapshot for late-joining frontend listeners.
#[tauri::command]
pub async fn get_python_status() -> PythonStatus {
    let progress = crate::python::bootstrap::get_last_progress();

    match crate::python::get() {
        Some(runtime) => match runtime.health_check().await {
            Ok(()) => {
                let (py, yf) = match runtime.versions().await {
                    Some(v) => (v.python_version, v.yfinance_version),
                    None => ("unknown".to_string(), "unknown".to_string()),
                };
                PythonStatus {
                    ready: true,
                    python_version: py,
                    yfinance_version: yf,
                    error: None,
                    progress,
                }
            }
            Err(e) => PythonStatus {
                ready: false,
                python_version: "unknown".to_string(),
                yfinance_version: "unknown".to_string(),
                error: Some(e),
                progress,
            },
        },
        None => PythonStatus {
            ready: false,
            python_version: "not initialized".to_string(),
            yfinance_version: "unknown".to_string(),
            error: Some("Python runtime not initialized".to_string()),
            progress,
        },
    }
}

/// Restart the Python runtime subprocess.
#[tauri::command]
pub async fn restart_python_runtime() -> Result<(), String> {
    match crate::python::get() {
        Some(runtime) => runtime.restart().await,
        None => Err("Python runtime not initialized".to_string()),
    }
}
