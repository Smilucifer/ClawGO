//! Python runtime lifecycle management.
//!
//! Manages a long-lived Python subprocess (`server.py`) that handles data-fetching
//! requests via JSON-RPC over stdin/stdout. The runtime auto-restarts on crash.

mod bootstrap;
pub mod bridge;

use bridge::JsonRpcClient;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

// ---------------------------------------------------------------------------
// Global singleton
// ---------------------------------------------------------------------------

static PYTHON_RUNTIME: OnceCell<Arc<PythonRuntime>> = OnceCell::const_new();

// ---------------------------------------------------------------------------
// Path resolution (single source of truth)
// ---------------------------------------------------------------------------

/// Resolve the embedded Python exe and server script paths from the Tauri resource dir.
///
/// This is the **only** place these paths are constructed — bootstrap and init both
/// call this function to avoid divergence.
pub(crate) fn resolve_python_paths(resource_dir: &std::path::Path) -> Result<(PathBuf, PathBuf), String> {
    let python_dir = resource_dir.join("python-runtime");

    #[cfg(target_os = "windows")]
    let python_exe = python_dir.join("python").join("python.exe");
    #[cfg(not(target_os = "windows"))]
    let python_exe = python_dir.join("python").join("bin").join("python3");

    let server_script = python_dir.join("scripts").join("server.py");

    if !python_exe.exists() {
        return Err(format!(
            "Python exe not found at: {}",
            python_exe.display()
        ));
    }
    if !server_script.exists() {
        return Err(format!(
            "server.py not found at: {}",
            server_script.display()
        ));
    }

    Ok((python_exe, server_script))
}

// ---------------------------------------------------------------------------
// PythonRuntime
// ---------------------------------------------------------------------------

/// Long-lived Python subprocess managed via JSON-RPC over stdin/stdout.
///
/// Spawns `server.py` on first use and auto-restarts on crash.
pub struct PythonRuntime {
    /// Mutable client handle — `None` when the subprocess is not running.
    client: Mutex<Option<JsonRpcClient>>,
    /// Path to the embedded Python executable (kept for restarts).
    python_exe: PathBuf,
    /// Path to the JSON-RPC server script (kept for restarts).
    server_script: PathBuf,
    /// Version info captured at init time (python version, yfinance version).
    versions: Mutex<Option<Versions>>,
}

/// Version info from the Python environment, captured once at init.
#[derive(Debug, Clone)]
pub struct Versions {
    pub python_version: String,
    pub yfinance_version: String,
}

impl PythonRuntime {
    fn new(python_exe: PathBuf, server_script: PathBuf) -> Self {
        Self {
            client: Mutex::new(None),
            python_exe,
            server_script,
            versions: Mutex::new(None),
        }
    }

    /// Ensure the subprocess is running and return a MutexGuard over the client.
    ///
    /// Spawns the subprocess if it is not running or has crashed. The returned
    /// guard keeps the lock held so callers can use the client without a TOCTOU
    /// window. Use `get_client().await?; drop(guard);` when you only need to
    /// ensure the process is alive (e.g. during init or restart).
    async fn get_client(&self) -> Result<tokio::sync::MutexGuard<'_, Option<JsonRpcClient>>, String> {
        let mut guard = self.client.lock().await;
        if let Some(ref client) = *guard {
            if client.is_alive() {
                return Ok(guard);
            }
            log::warn!("[python] Subprocess died, respawning");
        }

        let new_client = JsonRpcClient::spawn(&self.python_exe, &self.server_script).await?;
        *guard = Some(new_client);
        Ok(guard)
    }

    /// Send a JSON-RPC request to the Python server and return the result.
    ///
    /// Automatically spawns the subprocess if it is not running. Returns `Err`
    /// if the subprocess cannot be started or if the RPC call fails.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let guard = self.get_client().await?;
        let client = guard.as_ref().ok_or("Python runtime not initialized")?;

        client
            .call(method, params)
            .await
            .map_err(|e| format!("Python RPC error: {}", e))
    }

    /// Ping the subprocess to verify it is alive and responsive.
    pub async fn health_check(&self) -> Result<(), String> {
        let result = self.call("ping", serde_json::json!({})).await?;
        if result == "pong" {
            Ok(())
        } else {
            Err(format!("Unexpected ping response: {}", result))
        }
    }

    /// Restart the Python subprocess.
    ///
    /// Shuts down the current subprocess (if any) and spawns a fresh one.
    pub async fn restart(&self) -> Result<(), String> {
        {
            let mut guard = self.client.lock().await;
            if let Some(client) = guard.take() {
                client.shutdown().await;
            }
        }
        // Re-acquire via get_client which will spawn a new process.
        let _guard = self.get_client().await?;
        Ok(())
    }

    /// Get the cached version info (available after init).
    pub async fn versions(&self) -> Option<Versions> {
        self.versions.lock().await.clone()
    }
}

// ---------------------------------------------------------------------------
// Init / access
// ---------------------------------------------------------------------------

/// Initialize the global Python runtime.
///
/// 1. Resolves Python exe and server script paths from the Tauri resource dir.
/// 2. Spawns the long-lived `server.py` subprocess.
/// 3. Pings the server to verify it is alive.
/// 4. Queries Python/yfinance versions via `sys.version` RPC.
/// 5. Stores the runtime in the global `OnceCell`.
///
/// Emits `python://setup-progress` events to the frontend during the process.
pub async fn init(app_handle: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;

    let resource_dir = app_handle
        .path()
        .resource_dir()
        .map_err(|e| format!("Failed to resolve resource dir: {}", e))?;

    let (python_exe, server_script) = resolve_python_paths(&resource_dir)?;

    bootstrap::emit_progress(app_handle, "starting", None);

    let runtime = PythonRuntime::new(python_exe, server_script);

    // Ensure the subprocess is running (this spawns server.py), then release the lock.
    {
        let _guard = runtime.get_client().await.inspect_err(|e| {
            bootstrap::emit_progress(app_handle, "error", Some(e));
        })?;
    }

    // Health check — ping the server.
    runtime.health_check().await.inspect_err(|e| {
        bootstrap::emit_progress(app_handle, "error", Some(e));
    })?;

    // Query version info via the running server instead of spawning a separate process.
    let (py_ver, yf_ver) = query_versions(&runtime).await;
    {
        let mut v = runtime.versions.lock().await;
        *v = Some(Versions {
            python_version: py_ver,
            yfinance_version: yf_ver,
        });
    }

    bootstrap::emit_progress(app_handle, "ready", None);

    PYTHON_RUNTIME
        .set(Arc::new(runtime))
        .map_err(|_| "Python runtime already initialized".to_string())?;

    log::info!("[python] Runtime initialized successfully");
    Ok(())
}

/// Query Python and yfinance versions from the running server.
async fn query_versions(runtime: &PythonRuntime) -> (String, String) {
    /// Extract a string version from an RPC result, falling back to "unknown".
    async fn extract_version(runtime: &PythonRuntime, method: &str) -> String {
        runtime
            .call(method, serde_json::json!({}))
            .await
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown".to_string())
    }

    let py_ver = extract_version(runtime, "sys.version").await;
    let yf_ver = extract_version(runtime, "yfinance.version").await;
    (py_ver, yf_ver)
}

/// Get a reference to the global Python runtime, if initialized.
pub fn get() -> Option<&'static Arc<PythonRuntime>> {
    PYTHON_RUNTIME.get()
}

/// Get a reference to the global Python runtime, or return an error.
pub fn require() -> Result<&'static Arc<PythonRuntime>, String> {
    PYTHON_RUNTIME
        .get()
        .ok_or_else(|| "Python runtime not initialized — call python::init() first".to_string())
}
