use crate::agent::adapter;
use crate::agent::native_pty;
use crate::agent::pipe_parser::{CodexStdoutParser, PipeStdoutParser};
use crate::agent::windows_msvc_env::SpawnEnvPlan;
use crate::models::{ChatDelta, ChatDone, RunEventType};
use crate::process_ext::HideConsole;
use crate::storage;
use std::collections::HashMap;
#[cfg(windows)]
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

pub type ProcessMap = Arc<Mutex<HashMap<String, Child>>>;

pub fn new_process_map() -> ProcessMap {
    Arc::new(Mutex::new(HashMap::new()))
}

fn should_resolve_command_from_path(command: &str) -> bool {
    !command.trim().is_empty() && !command.contains('\\') && !command.contains('/')
}

fn resolve_process_command(command: &str) -> String {
    if should_resolve_command_from_path(command) {
        crate::agent::claude_stream::which_binary(command).unwrap_or_else(|| command.to_string())
    } else {
        command.to_string()
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn resolve_windows_npm_shim(command: &str) -> Option<(String, Vec<String>)> {
    #[cfg(windows)]
    {
        let path = Path::new(command);
        let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
        if ext != "cmd" && ext != "bat" {
            return None;
        }

        let base = path.parent()?;
        let node = base.join("node.exe");
        let node_bin = if node.exists() {
            node
        } else {
            PathBuf::from(crate::agent::claude_stream::which_binary("node")?)
        };

        let stem = path.file_stem()?.to_string_lossy().to_ascii_lowercase();
        let script = if stem == "codex" {
            base.join("node_modules")
                .join("@openai")
                .join("codex")
                .join("bin")
                .join("codex.js")
        } else {
            return None;
        };

        if !script.exists() {
            return None;
        }

        Some((
            node_bin.to_string_lossy().into_owned(),
            vec![script.to_string_lossy().into_owned()],
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = command;
        None
    }
}

fn resolve_spawn_invocation(command: String, args: Vec<String>) -> (String, Vec<String>) {
    #[cfg(windows)]
    if let Some((node, mut shim_args)) = resolve_windows_npm_shim(&command) {
        let mut args = args;
        shim_args.append(&mut args);
        return (node, shim_args);
    }

    (command, args)
}

fn quote_display_arg(arg: &str) -> String {
    let safe = !arg.is_empty()
        && arg.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | '\\' | ':' | '=')
        });
    if safe {
        return arg.to_string();
    }

    let escaped = arg.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

fn format_started_command(command: &str, args: &[String]) -> String {
    std::iter::once(quote_display_arg(command))
        .chain(args.iter().map(|arg| quote_display_arg(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

#[allow(clippy::too_many_arguments)]
pub async fn run_agent(
    app: AppHandle,
    process_map: ProcessMap,
    run_id: String,
    command: String,
    args: Vec<String>,
    cwd: String,
    agent: String,
    spawn_env_plan: SpawnEnvPlan,
) -> Result<(), String> {
    log::debug!(
        "[stream] run_agent: run_id={}, cmd={}, args={:?}, cwd={}, agent={}",
        run_id,
        command,
        args,
        cwd,
        agent
    );
    let display_command = format_started_command(&command, &args);
    let process_command = resolve_process_command(&command);
    let (process_command, args) = resolve_spawn_invocation(process_command, args);
    let native_transcript_mode = agent == "codex";
    if process_command != command {
        log::debug!(
            "[stream] resolved command for spawn: {} -> {}",
            command,
            process_command
        );
    }

    if native_transcript_mode {
        return native_pty::run_native_pty_agent(
            app,
            run_id,
            process_command,
            args,
            cwd,
            agent,
            spawn_env_plan,
            display_command,
        )
        .await;
    }

    let emit_run_event = |rt: RunEventType, payload: serde_json::Value| {
        if let Err(e) = storage::events::append_event(&run_id, rt, payload) {
            log::warn!(
                "[stream] failed to append event for run_id={}: {}",
                run_id,
                e
            );
        }
    };

    // Log start
    emit_run_event(
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
        .env_remove("CLAUDECODE") // Allow running inside a Claude Code session
        .hide_console()
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            let msg = if e.kind() == std::io::ErrorKind::NotFound {
                format!(
                    "Command \"{}\" not found. Is {} CLI installed and in your PATH?",
                    command, agent
                )
            } else {
                e.to_string()
            };
            log::error!("[stream] spawn failed: {}", msg);
            msg
        })?;

    let pid = child.id().unwrap_or(0);
    log::debug!("[stream] spawned process: run_id={}, pid={}", run_id, pid);

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // Store child for stop_run
    {
        let mut map = process_map.lock().await;
        map.insert(run_id.clone(), child);
    }

    let run_id_out = run_id.clone();
    let run_id_err = run_id.clone();
    let app_out = app.clone();
    let agent_clone = agent.clone();
    // Stdout reader
    let stdout_handle = tokio::spawn(async move {
        let mut assistant_text = String::new();
        let is_codex = agent_clone == "codex";

        if is_codex {
            let mut parser = CodexStdoutParser;
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Err(e) = storage::events::append_event(
                    &run_id_out,
                    RunEventType::Stdout,
                    serde_json::json!({ "text": line, "source": "ui_chat" }),
                ) {
                    log::warn!("[stream] stdout append failed: {}", e);
                }
                let _ = app_out.emit(
                    "run-event",
                    serde_json::json!({
                        "run_id": run_id_out,
                        "type": "stdout",
                        "text": line
                    }),
                );

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    // Capture thread_id as ConversationRef for Codex resume
                    let type_str = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if type_str == "thread.started" {
                        if let Some(tid) = payload.get("thread_id").and_then(|v| v.as_str()) {
                            log::debug!("[codex] captured thread_id={} as conversation_ref", tid);
                            let tid_str = tid.to_string();
                            let rid = run_id_out.clone();
                            if let Err(e) = crate::storage::runs::with_meta(&rid, |meta| {
                                meta.conversation_ref =
                                    Some(crate::models::ConversationRef::CodexThread(tid_str));
                                Ok(())
                            }) {
                                log::warn!("[codex] failed to persist conversation_ref: {}", e);
                            }
                        }
                    }

                    // Use PipeStdoutParser trait for structured event → BusEvent
                    let events = parser.parse_line(&run_id_out, &payload);
                    for ev in &events {
                        if let crate::models::BusEvent::MessageDelta { text, .. } = ev {
                            assistant_text.push_str(text);
                            let _ = app_out.emit("chat-delta", ChatDelta { text: text.clone() });
                        }
                    }
                    if events.is_empty() && !type_str.is_empty() {
                        log::debug!("[codex] unhandled event: type={}", type_str);
                    }
                } else {
                    let text = format!("{line}\n");
                    assistant_text.push_str(&text);
                    let _ = app_out.emit("chat-delta", ChatDelta { text });
                }
            }
        } else {
            // Claude: stdout is the response text
            let mut reader = BufReader::new(stdout);
            let mut buf = vec![0u8; 8192];
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&buf[..n]).to_string();
                        assistant_text.push_str(&text);
                        if let Err(e) = storage::events::append_event(
                            &run_id_out,
                            RunEventType::Stdout,
                            serde_json::json!({ "text": text, "source": "ui_chat" }),
                        ) {
                            log::warn!("[stream] stdout append failed: {}", e);
                        }
                        let _ = app_out.emit(
                            "run-event",
                            serde_json::json!({
                                "run_id": run_id_out,
                                "type": "stdout",
                                "text": text
                            }),
                        );
                        let _ = app_out.emit("chat-delta", ChatDelta { text });
                    }
                    Err(_) => break,
                }
            }
        }

        assistant_text
    });

    // Stderr reader
    let app_err = app.clone();
    let stderr_handle = tokio::spawn(async move {
        let mut stderr_text = String::new();
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            stderr_text.push_str(&line);
            stderr_text.push('\n');
            if let Err(e) = storage::events::append_event(
                &run_id_err,
                RunEventType::Stderr,
                serde_json::json!({ "text": line, "source": "ui_chat" }),
            ) {
                log::warn!("[stream] stderr append failed: {}", e);
            }
            let _ = app_err.emit(
                "run-event",
                serde_json::json!({
                    "run_id": run_id_err,
                    "type": "stderr",
                    "text": line
                }),
            );
        }
        stderr_text
    });

    let (assistant_text, exit_code) = {
        // Wait for stdout/stderr to close (= process exited or pipes broken).
        // This completes without holding the ProcessMap lock.
        let assistant_text = stdout_handle.await.unwrap_or_default();
        let _stderr_text = stderr_handle.await.unwrap_or_default();

        // Short lock: remove child from map, then wait() outside the lock.
        // If stop_process already removed+killed the child, we get None → exit_code -1.
        let removed_child = {
            let mut map = process_map.lock().await;
            map.remove(&run_id)
        };
        let exit_code = if let Some(mut child) = removed_child {
            match child.wait().await {
                Ok(status) => status.code().unwrap_or(1),
                Err(_) => 1,
            }
        } else {
            // Was killed by stop_run
            -1
        };
        (assistant_text, exit_code)
    };

    // Save assistant event
    if !assistant_text.trim().is_empty() {
        emit_run_event(
            RunEventType::Assistant,
            serde_json::json!({ "text": assistant_text.trim(), "source": "ui_chat" }),
        );
    }

    log::debug!(
        "[stream] process exited: run_id={}, exit_code={}, output_len={}",
        run_id,
        exit_code,
        assistant_text.len()
    );

    // Update run status
    if exit_code == 0 {
        if let Err(e) = storage::runs::update_status(
            &run_id,
            crate::models::RunStatus::Completed,
            Some(0),
            None,
        ) {
            log::warn!("[stream] failed to update status to Completed: {}", e);
        }
    } else if exit_code == -1 {
        if let Err(e) = storage::runs::update_status(
            &run_id,
            crate::models::RunStatus::Stopped,
            None,
            Some("Stopped by user".to_string()),
        ) {
            log::warn!("[stream] failed to update status to Stopped: {}", e);
        }
    } else if let Err(e) = storage::runs::update_status(
        &run_id,
        crate::models::RunStatus::Failed,
        Some(exit_code),
        Some(format!("Exit code {}", exit_code)),
    ) {
        log::warn!("[stream] failed to update status to Failed: {}", e);
    }

    emit_run_event(
        RunEventType::System,
        serde_json::json!({ "message": format!("Process exited with code {}", exit_code), "source": "ui_chat" }),
    );

    let _ = app.emit(
        "chat-done",
        ChatDone {
            ok: exit_code == 0,
            code: exit_code,
            error: None,
        },
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn run_claude_pipe_or_session(
    app: AppHandle,
    process_map: ProcessMap,
    run_id: String,
    process_command: String,
    args: Vec<String>,
    cwd: String,
    agent: String,
    spawn_env_plan: SpawnEnvPlan,
    display_command: String,
) -> Result<(), String> {
    let emit_run_event = |rt: RunEventType, payload: serde_json::Value| {
        if let Err(e) = storage::events::append_event(&run_id, rt, payload) {
            log::warn!(
                "[stream] failed to append event for run_id={}: {}",
                run_id,
                e
            );
        }
    };

    // Log start
    emit_run_event(
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
        .env_remove("CLAUDECODE") // Allow running inside a Claude Code session
        .hide_console()
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            let msg = if e.kind() == std::io::ErrorKind::NotFound {
                format!(
                    "Command \"{}\" not found. Is {} CLI installed and in your PATH?",
                    process_command, agent
                )
            } else {
                e.to_string()
            };
            log::error!("[stream] spawn failed: {}", msg);
            msg
        })?;

    let pid = child.id().unwrap_or(0);
    log::debug!("[stream] spawned process: run_id={}, pid={}", run_id, pid);

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // Store child for stop_run
    {
        let mut map = process_map.lock().await;
        map.insert(run_id.clone(), child);
    }

    let run_id_out = run_id.clone();
    let run_id_err = run_id.clone();
    let app_out = app.clone();
    // Stdout reader
    let stdout_handle = tokio::spawn(async move {
        let mut assistant_text = String::new();
        // Claude: stdout is the response text
        let mut reader = BufReader::new(stdout);
        let mut buf = vec![0u8; 8192];
        loop {
            match reader.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    assistant_text.push_str(&text);
                    if let Err(e) = storage::events::append_event(
                        &run_id_out,
                        RunEventType::Stdout,
                        serde_json::json!({ "text": text, "source": "ui_chat" }),
                    ) {
                        log::warn!("[stream] stdout append failed: {}", e);
                    }
                    let _ = app_out.emit(
                        "run-event",
                        serde_json::json!({
                            "run_id": run_id_out,
                            "type": "stdout",
                            "text": text
                        }),
                    );
                    let _ = app_out.emit("chat-delta", ChatDelta { text });
                }
                Err(_) => break,
            }
        }
        assistant_text
    });

    // Stderr reader
    let app_err = app.clone();
    let stderr_handle = tokio::spawn(async move {
        let mut stderr_text = String::new();
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            stderr_text.push_str(&line);
            stderr_text.push('\n');
            if let Err(e) = storage::events::append_event(
                &run_id_err,
                RunEventType::Stderr,
                serde_json::json!({ "text": line, "source": "ui_chat" }),
            ) {
                log::warn!("[stream] stderr append failed: {}", e);
            }
            let _ = app_err.emit(
                "run-event",
                serde_json::json!({
                    "run_id": run_id_err,
                    "type": "stderr",
                    "text": line
                }),
            );
        }
        stderr_text
    });

    let (assistant_text, exit_code) = {
        // Wait for stdout/stderr to close (= process exited or pipes broken).
        // This completes without holding the ProcessMap lock.
        let assistant_text = stdout_handle.await.unwrap_or_default();
        let _stderr_text = stderr_handle.await.unwrap_or_default();

        // Short lock: remove child from map, then wait() outside the lock.
        // If stop_process already removed+killed the child, we get None → exit_code -1.
        let removed_child = {
            let mut map = process_map.lock().await;
            map.remove(&run_id)
        };
        let exit_code = if let Some(mut child) = removed_child {
            match child.wait().await {
                Ok(status) => status.code().unwrap_or(1),
                Err(_) => 1,
            }
        } else {
            // Was killed by stop_run
            -1
        };
        (assistant_text, exit_code)
    };

    // Save assistant event
    if !assistant_text.trim().is_empty() {
        emit_run_event(
            RunEventType::Assistant,
            serde_json::json!({ "text": assistant_text.trim(), "source": "ui_chat" }),
        );
    }

    log::debug!(
        "[stream] process exited: run_id={}, exit_code={}, output_len={}",
        run_id,
        exit_code,
        assistant_text.len()
    );

    // Update run status
    if exit_code == 0 {
        if let Err(e) = storage::runs::update_status(
            &run_id,
            crate::models::RunStatus::Completed,
            Some(0),
            None,
        ) {
            log::warn!("[stream] failed to update status to Completed: {}", e);
        }
    } else if exit_code == -1 {
        if let Err(e) = storage::runs::update_status(
            &run_id,
            crate::models::RunStatus::Stopped,
            None,
            Some("Stopped by user".to_string()),
        ) {
            log::warn!("[stream] failed to update status to Stopped: {}", e);
        }
    } else if let Err(e) = storage::runs::update_status(
        &run_id,
        crate::models::RunStatus::Failed,
        Some(exit_code),
        Some(format!("Exit code {}", exit_code)),
    ) {
        log::warn!("[stream] failed to update status to Failed: {}", e);
    }

    emit_run_event(
        RunEventType::System,
        serde_json::json!({ "message": format!("Process exited with code {}", exit_code), "source": "ui_chat" }),
    );

    let _ = app.emit(
        "chat-done",
        ChatDone {
            ok: exit_code == 0,
            code: exit_code,
            error: None,
        },
    );

    Ok(())
}

pub async fn stop_process(process_map: &ProcessMap, run_id: &str) -> bool {
    log::debug!("[stream] stop_process: run_id={}", run_id);
    if native_pty::stop_native_pty_process(run_id) {
        log::debug!("[stream] stop_process: killed native pty run_id={}", run_id);
        return true;
    }
    // Short lock: remove child, then kill+wait outside the lock.
    let removed = {
        let mut map = process_map.lock().await;
        map.remove(run_id)
    };
    if let Some(mut child) = removed {
        let _ = child.kill().await;
        let _ = child.wait().await;
        log::debug!("[stream] stop_process: killed run_id={}", run_id);
        true
    } else {
        log::debug!("[stream] stop_process: no process for run_id={}", run_id);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_command_quotes_prompt_arguments_without_losing_boundaries() {
        let args = vec![
            "--dangerously-bypass-approvals-and-sandbox".to_string(),
            "--no-alt-screen".to_string(),
            "--model".to_string(),
            "gpt-5.5".to_string(),
            "你好".to_string(),
            "hello world".to_string(),
        ];

        assert_eq!(
            format_started_command("codex", &args),
            "codex --dangerously-bypass-approvals-and-sandbox --no-alt-screen --model gpt-5.5 \"你好\" \"hello world\""
        );
    }

    #[test]
    fn bare_cli_names_are_resolved_before_spawn_but_paths_are_left_intact() {
        assert!(should_resolve_command_from_path("codex"));
        assert!(!should_resolve_command_from_path(
            "C:\\Users\\InBlu\\AppData\\Roaming\\npm\\codex.cmd"
        ));
        assert!(!should_resolve_command_from_path("./codex"));
        assert!(!should_resolve_command_from_path(""));
    }

    #[cfg(windows)]
    #[test]
    fn npm_shim_invocation_prefers_node_without_shelling_out_to_cmd() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        std::fs::write(base.join("node.exe"), "").unwrap();
        let codex_js = base
            .join("node_modules")
            .join("@openai")
            .join("codex")
            .join("bin");
        std::fs::create_dir_all(&codex_js).unwrap();
        std::fs::write(codex_js.join("codex.js"), "").unwrap();

        let shim = base.join("codex.cmd");
        std::fs::write(&shim, "@echo off").unwrap();

        let resolved = resolve_windows_npm_shim(&shim.to_string_lossy()).expect("shim");
        assert!(resolved.0.ends_with("node.exe"));
        assert!(resolved.1[0].ends_with("codex.js"));
    }
}
