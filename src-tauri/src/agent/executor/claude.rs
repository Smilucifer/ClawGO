// src-tauri/src/agent/executor/claude.rs
use super::{Executor, ExecutorRequest};
use crate::agent::stream::{run_claude_pipe_or_session, ProcessMap};
use tauri::AppHandle;

pub struct ClaudeExecutor;

#[async_trait::async_trait]
impl Executor for ClaudeExecutor {
    async fn run(
        &self,
        app: AppHandle,
        process_map: ProcessMap,
        req: ExecutorRequest,
    ) -> Result<(), String> {
        run_claude_pipe_or_session(
            app,
            process_map,
            req.run_id,
            req.process_command,
            req.args,
            req.cwd,
            req.agent,
            req.spawn_env_plan,
            req.display_command,
        )
        .await
    }
}
