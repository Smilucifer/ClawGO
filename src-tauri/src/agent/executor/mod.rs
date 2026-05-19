// src-tauri/src/agent/executor/mod.rs
use crate::agent::stream::ProcessMap;
use crate::agent::windows_msvc_env::SpawnEnvPlan;
use std::sync::Arc;
use tauri::AppHandle;

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
