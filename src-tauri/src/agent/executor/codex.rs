// src-tauri/src/agent/executor/codex.rs
use super::{Executor, ExecutorRequest};
use crate::agent::stream::ProcessMap;
use tauri::AppHandle;

pub struct CodexExecutor;

#[async_trait::async_trait]
impl Executor for CodexExecutor {
    async fn run(
        &self,
        _app: AppHandle,
        _process_map: ProcessMap,
        _req: ExecutorRequest,
    ) -> Result<(), String> {
        Err("CodexExecutor not yet wired".to_string())
    }
}
