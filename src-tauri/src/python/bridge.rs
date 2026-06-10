//! JSON-RPC 2.0 client over stdin/stdout of a child process.

use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{oneshot, Mutex};

const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Pending request map: request ID → oneshot sender for the response.
type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, JsonRpcError>>>>>;

/// Errors from JSON-RPC communication.
///
/// `pub(crate)` — callers outside the `python` module only see `Result<Value, String>`
/// from `PythonRuntime::call()`. This enum is an internal implementation detail.
#[derive(Debug)]
pub(crate) enum JsonRpcError {
    Timeout,
    Io(std::io::Error),
    Protocol(String),
    Remote { code: i64, message: String },
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonRpcError::Timeout => write!(f, "Python RPC call timed out"),
            JsonRpcError::Io(e) => write!(f, "Python RPC IO error: {}", e),
            JsonRpcError::Protocol(msg) => write!(f, "Python RPC protocol error: {}", msg),
            JsonRpcError::Remote { code, message } => {
                write!(f, "Python RPC error {}: {}", code, message)
            }
        }
    }
}

impl From<std::io::Error> for JsonRpcError {
    fn from(e: std::io::Error) -> Self {
        JsonRpcError::Io(e)
    }
}

/// A JSON-RPC 2.0 client that communicates with a subprocess over stdin/stdout.
pub struct JsonRpcClient {
    /// Only accessed in `shutdown()` which consumes `self` — no concurrent access.
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: PendingMap,
    next_id: AtomicU64,
    /// Shared with the stdout reader task — set to `false` when the process exits.
    alive: Arc<AtomicBool>,
}

impl JsonRpcClient {
    /// Spawn a child process and set up the JSON-RPC communication channel.
    pub(crate) async fn spawn(exe: &Path, script: &Path) -> Result<Self, String> {
        use crate::process_ext::HideConsole;

        let mut cmd = tokio::process::Command::new(exe);
        cmd.arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Force UTF-8 for stdin/stdout/stderr — Windows defaults to the
            // system ANSI code page (e.g. GBK on zh-CN), which cannot encode
            // characters like U+200B (zero-width space) that appear in Jin10
            // and AkShare news data.  Without this, `print()` in server.py
            // raises UnicodeEncodeError and the process crashes.
            .env("PYTHONIOENCODING", "utf-8")
            .hide_console()
            // Prevent orphan Python processes on restart/panic — matches codebase
            // convention used by all other child-process spawns.
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn Python process: {}", e))?;

        let stdin = child.stdin.take().ok_or("Failed to capture stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let alive = Arc::new(AtomicBool::new(true));

        // Spawn stdout reader task — passes `alive` so it can mark the client
        // as dead when stdout closes (process exit/crash).
        let pending_clone = pending.clone();
        let alive_clone = alive.clone();
        tokio::spawn(async move {
            Self::read_responses(stdout, pending_clone, alive_clone).await;
        });

        // Spawn stderr reader task (for Python logs) — info level so startup
        // errors and tracebacks are visible without debug logging.
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log::info!("[python] {}", line);
            }
        });

        Ok(Self {
            child,
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            next_id: AtomicU64::new(1),
            alive,
        })
    }

    /// Background task: read JSON-RPC responses from stdout and dispatch to pending requests.
    async fn read_responses(
        stdout: ChildStdout,
        pending: PendingMap,
        alive: Arc<AtomicBool>,
    ) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parsed: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("[python-bridge] Failed to parse response: {} — raw: {}", e, line);
                    continue;
                }
            };

            // Extract response ID
            let id = match parsed.get("id").and_then(|v| v.as_u64()) {
                Some(id) => id,
                None => {
                    log::warn!("[python-bridge] Response missing id: {}", line);
                    continue;
                }
            };

            // Resolve the pending request
            let tx = {
                let mut map = pending.lock().await;
                map.remove(&id)
            };

            if let Some(tx) = tx {
                let result = if let Some(error) = parsed.get("error") {
                    let code = error.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
                    let message = error
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown error")
                        .to_string();
                    Err(JsonRpcError::Remote { code, message })
                } else {
                    Ok(parsed.get("result").cloned().unwrap_or(Value::Null))
                };

                let _ = tx.send(result);
            } else {
                log::warn!("[python-bridge] Response for unknown id: {}", id);
            }
        }

        // Stdout closed — mark the client as dead so get_client() will restart it.
        alive.store(false, Ordering::SeqCst);
        log::warn!("[python-bridge] Python process exited");

        // Pre-format the error message once, then share it across all pending requests.
        let error_msg = "Python process exited".to_string();
        let mut map = pending.lock().await;
        for (id, tx) in map.drain() {
            log::warn!("[python-bridge] Failing pending request {}", id);
            let _ = tx.send(Err(JsonRpcError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                error_msg.clone(),
            ))));
        }
    }

    /// Send a JSON-RPC request and wait for the response.
    pub(crate) async fn call(&self, method: &str, params: Value) -> Result<Value, JsonRpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // Register pending response handler
        let rx = {
            let (tx, rx) = oneshot::channel();
            let mut map = self.pending.lock().await;
            map.insert(id, tx);
            rx
        };

        // Encode and send request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id,
        });

        let mut line = serde_json::to_string(&request).map_err(|e| {
            JsonRpcError::Protocol(format!("Failed to serialize request: {}", e))
        })?;
        line.push('\n');

        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(line.as_bytes()).await?;
            stdin.flush().await?;
        }

        // Wait for response with timeout
        match tokio::time::timeout(
            std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            rx,
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(JsonRpcError::Protocol(
                "Response channel dropped".to_string(),
            )),
            Err(_) => {
                // Clean up the pending entry
                self.pending.lock().await.remove(&id);
                Err(JsonRpcError::Timeout)
            }
        }
    }

    /// Check if the subprocess is still alive.
    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// Gracefully shut down the subprocess.
    pub(crate) async fn shutdown(mut self) {
        self.alive.store(false, Ordering::Relaxed);

        // Close stdin to signal the Python process to exit
        {
            let mut stdin = self.stdin.lock().await;
            let _ = stdin.shutdown().await;
        }

        // Wait briefly for clean exit
        match tokio::time::timeout(
            std::time::Duration::from_secs(3),
            self.child.wait(),
        )
        .await
        {
            Ok(Ok(status)) => {
                log::info!("[python-bridge] process exited: {}", status);
            }
            Ok(Err(e)) => {
                log::warn!("[python-bridge] process wait error: {}", e);
            }
            Err(_) => {
                log::warn!("[python-bridge] process did not exit in 3s, killing");
                let _ = self.child.kill().await;
            }
        }
    }
}
