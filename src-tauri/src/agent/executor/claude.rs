// src-tauri/src/agent/executor/claude.rs
use super::{Executor, ExecutorRequest};
use crate::agent::stream::ProcessMap;
use tauri::AppHandle;

pub struct ClaudeExecutor;

#[async_trait::async_trait]
impl Executor for ClaudeExecutor {
    async fn run(
        &self,
        _app: AppHandle,
        _process_map: ProcessMap,
        _req: ExecutorRequest,
    ) -> Result<(), String> {
        Err("ClaudeExecutor not yet wired".to_string())
    }
}
