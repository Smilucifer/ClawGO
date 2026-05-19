// src-tauri/src/agent/executor/codex.rs
use super::codex_state::CodexProtocolState;
use super::{Executor, ExecutorRequest};
use crate::agent::adapter;
use crate::agent::stream::ProcessMap;
use crate::models::{BusEvent, ChatDone, ConversationRef, RunEventType, RunStatus};
use crate::process_ext::HideConsole;
use crate::storage;
use serde_json::Value;
use std::process::Stdio;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub struct CodexExecutor;

#[async_trait::async_trait]
impl Executor for CodexExecutor {
    async fn run(
        &self,
        app: AppHandle,
        process_map: ProcessMap,
        req: ExecutorRequest,
    ) -> Result<(), String> {
        let ExecutorRequest {
            run_id,
            cwd,
            spawn_env_plan,
            display_command,
            process_command,
            args,
            ..
        } = req;

        let _ = storage::events::append_event(
            &run_id,
            RunEventType::System,
            serde_json::json!({
                "message": format!("Started {}", display_command),
                "source": "ui_chat"
            }),
        );

        let mut cmd = Command::new(&process_command);
        cmd.args(&args)
            .current_dir(&cwd)
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

        let mut child = cmd
            .env("CLAW_GO_TASK_ID", &run_id)
            .env("CLAW_GO_RUN_ID", &run_id)
            .env_remove("CLAUDECODE")
            .hide_console()
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    "errors_codexCliNotInstalled".to_string()
                } else {
                    e.to_string()
                }
            })?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        {
            let mut map = process_map.lock().await;
            map.insert(run_id.clone(), child);
        }

        let app_err = app.clone();
        let run_id_err = run_id.clone();
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = storage::events::append_event(
                    &run_id_err,
                    RunEventType::Stderr,
                    serde_json::json!({"text": line, "source": "ui_chat"}),
                );
                let _ = app_err.emit(
                    "run-event",
                    serde_json::json!({"run_id": run_id_err, "type": "stderr", "text": line}),
                );
            }
        });

        let mut state = CodexProtocolState::new(run_id.clone());
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = storage::events::append_event(
                &run_id,
                RunEventType::Stdout,
                serde_json::json!({"text": line, "source": "ui_chat"}),
            );
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(_) => {
                    let _ = app.emit(
                        "run-event",
                        serde_json::json!({"run_id": run_id, "type": "stdout", "text": line}),
                    );
                    continue;
                }
            };

            if value.get("type").and_then(|v| v.as_str()) == Some("thread.started") {
                if let Some(tid) = value.get("thread_id").and_then(|v| v.as_str()) {
                    let tid_str = tid.to_string();
                    let rid = run_id.clone();
                    if let Err(e) = storage::runs::with_meta(&rid, |meta| {
                        meta.conversation_ref = Some(ConversationRef::CodexThread(tid_str));
                        Ok(())
                    }) {
                        log::warn!("[codex] failed to persist conversation_ref: {}", e);
                    }
                }
            }

            for ev in state.map_event(&value) {
                emit_bus_event(&app, &run_id, ev);
            }
        }

        let _ = stderr_handle.await;

        let was_killed_by_stop = {
            let map = process_map.lock().await;
            !map.contains_key(&run_id)
        };
        let removed = {
            let mut map = process_map.lock().await;
            map.remove(&run_id)
        };
        let exit_code = if let Some(mut child) = removed {
            child.wait().await.ok().and_then(|s| s.code()).unwrap_or(1)
        } else {
            -1
        };
        let saw_turn_completed = state.has_seen_turn_completed();

        let (status, code, error) = if was_killed_by_stop {
            (RunStatus::Stopped, -1, Some("Stopped by user".to_string()))
        } else if exit_code == 0 && saw_turn_completed {
            (RunStatus::Completed, 0, None)
        } else if exit_code == 0 {
            (
                RunStatus::Failed,
                1,
                Some("Codex exited before turn completion".to_string()),
            )
        } else {
            (
                RunStatus::Failed,
                exit_code,
                Some(format!("Codex exited with code {exit_code}")),
            )
        };

        if let Err(e) =
            storage::runs::update_status(&run_id, status.clone(), Some(code), error.clone())
        {
            log::warn!("[codex] failed to update status: {}", e);
        }

        let _ = storage::events::append_event(
            &run_id,
            RunEventType::System,
            serde_json::json!({
                "message": format!("Process exited with code {}", code),
                "source": "ui_chat"
            }),
        );

        let _ = app.emit(
            "chat-done",
            ChatDone {
                ok: status == RunStatus::Completed,
                code,
                error,
            },
        );

        Ok(())
    }
}

fn emit_bus_event(app: &AppHandle, _run_id: &str, ev: BusEvent) {
    let _ = app.emit("bus-event", &ev);
    if let BusEvent::MessageDelta { text, .. } = &ev {
        let _ = app.emit(
            "chat-delta",
            crate::models::ChatDelta { text: text.clone() },
        );
    }
}
