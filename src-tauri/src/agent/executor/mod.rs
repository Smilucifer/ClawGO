// src-tauri/src/agent/executor/mod.rs
use crate::agent::adapter;
use crate::agent::stream::ProcessMap;
use crate::agent::windows_msvc_env::SpawnEnvPlan;
use crate::models::RunEventType;
use crate::process_ext::HideConsole;
use crate::storage;
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{ChildStderr, Command};
use tokio::task::JoinHandle;

pub mod claude;
pub mod codex;
pub mod codex_state;

/// Inputs an Executor needs to spawn one turn.
/// Command-line and resume thread_id are baked into `args` by the caller
/// (commands/chat.rs via build_agent_command / build_agent_resume_command);
/// executors do not re-derive them.
pub struct ExecutorRequest {
    pub run_id: String,
    pub cwd: String,
    pub agent: String,
    pub spawn_env_plan: SpawnEnvPlan,
    pub display_command: String,
    pub process_command: String,
    pub args: Vec<String>,
}

#[async_trait::async_trait]
pub trait Executor: Send + Sync {
    async fn run(
        &self,
        app: AppHandle,
        process_map: ProcessMap,
        request: ExecutorRequest,
    ) -> Result<(), String>;
}

pub fn for_agent(agent: &str) -> Result<Arc<dyn Executor>, String> {
    match agent {
        "claude" => Ok(Arc::new(claude::ClaudeExecutor) as Arc<dyn Executor>),
        "codex" => Ok(Arc::new(codex::CodexExecutor) as Arc<dyn Executor>),
        other => Err(format!("Unsupported executor: {other}")),
    }
}

/// Build a [`Command`] with shared spawn plumbing (cwd, piped stdio, PATH override,
/// MSVC env injection, Claw GO task/run env vars, hidden console, kill-on-drop).
/// The caller must chain any additional env removals (e.g. `env_remove("CLAUDECODE")`)
/// and call `.spawn()` with its own error mapping.
pub(super) fn build_child_command(
    process_command: &str,
    args: &[String],
    cwd: &str,
    run_id: &str,
    spawn_env_plan: &SpawnEnvPlan,
) -> Command {
    let mut cmd = Command::new(process_command);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(path) = &spawn_env_plan.path_override {
        cmd.env("PATH", path);
    }
    for key in adapter::auth_env_removals_for_extra_env(&spawn_env_plan.msvc_env) {
        cmd.env_remove(key);
    }
    for (key, value) in &spawn_env_plan.msvc_env {
        cmd.env(key, value);
    }

    cmd.env("CLAW_GO_TASK_ID", run_id)
        .env("CLAW_GO_RUN_ID", run_id)
        .env_remove("CLAUDECODE")
        .hide_console()
        .kill_on_drop(true);

    cmd
}

/// Spawn a background task that reads stderr line-by-line, persisting each line
/// as a `Stderr` event and emitting a `run-event` bus message.
pub(super) fn spawn_stderr_reader(
    app: AppHandle,
    run_id: String,
    stderr: ChildStderr,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Err(e) = storage::events::append_event(
                &run_id,
                RunEventType::Stderr,
                serde_json::json!({"text": line, "source": "ui_chat"}),
            ) {
                log::warn!("[executor] stderr append failed: {}", e);
            }
            let _ = app.emit(
                "run-event",
                serde_json::json!({"run_id": run_id, "type": "stderr", "text": line}),
            );
        }
    })
}
