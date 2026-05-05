use crate::agent::adapter;
use crate::agent::native_transcript;
use crate::agent::windows_msvc_env::SpawnEnvPlan;
use crate::models::{ChatDelta, ChatDone, RunEventType, RunStatus};
use crate::storage;
use once_cell::sync::Lazy;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;

struct NativePtyProcess {
    child: Box<dyn Child + Send + Sync>,
    _master: Box<dyn MasterPty + Send>,
}

enum NativeWaitOutcome {
    Transcript(Result<native_transcript::NativeTranscriptResult, String>),
    ProcessExited(i32),
}

#[derive(Debug, Clone, PartialEq)]
enum NativeTurnTerminal {
    Completed {
        assistant_text: String,
        conversation_ref: Option<crate::models::ConversationRef>,
    },
    Stopped,
    Failed { exit_code: i32, error: String },
}

fn resolve_native_turn_terminal(
    agent: &str,
    run_id: &str,
    wait_outcome: NativeWaitOutcome,
) -> NativeTurnTerminal {
    match wait_outcome {
        NativeWaitOutcome::Transcript(Ok(result)) => NativeTurnTerminal::Completed {
            assistant_text: result.text,
            conversation_ref: result.conversation_ref,
        },
        NativeWaitOutcome::Transcript(Err(error)) => {
            if take_native_pty_stop_requested(run_id) {
                NativeTurnTerminal::Stopped
            } else {
                NativeTurnTerminal::Failed {
                    exit_code: 1,
                    error,
                }
            }
        }
        NativeWaitOutcome::ProcessExited(code) => {
            if code == -1 {
                NativeTurnTerminal::Stopped
            } else {
                NativeTurnTerminal::Failed {
                    exit_code: code,
                    error: format!(
                        "Native {agent} process exited with code {code} before transcript completion"
                    ),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::native_transcript::NativeTranscriptResult;

    #[test]
    fn transcript_success_resolves_to_completed() {
        let terminal = resolve_native_turn_terminal(
            "codex",
            "run-complete",
            NativeWaitOutcome::Transcript(Ok(NativeTranscriptResult {
                text: "done".to_string(),
                conversation_ref: None,
            })),
        );

        assert_eq!(
            terminal,
            NativeTurnTerminal::Completed {
                assistant_text: "done".to_string(),
                conversation_ref: None,
            }
        );
    }

    #[test]
    fn transcript_error_without_stop_marker_resolves_to_failed_even_if_process_missing() {
        let terminal = resolve_native_turn_terminal(
            "codex",
            "run-missing-no-stop",
            NativeWaitOutcome::Transcript(Err("transcript parse failed".to_string())),
        );

        assert_eq!(
            terminal,
            NativeTurnTerminal::Failed {
                exit_code: 1,
                error: "transcript parse failed".to_string(),
            }
        );
    }

    #[test]
    fn transcript_error_with_stop_marker_resolves_to_stopped() {
        mark_native_pty_stop_requested("run-stop-marked");
        let terminal = resolve_native_turn_terminal(
            "codex",
            "run-stop-marked",
            NativeWaitOutcome::Transcript(Err("wait cancelled".to_string())),
        );

        assert_eq!(terminal, NativeTurnTerminal::Stopped);
        assert!(!take_native_pty_stop_requested("run-stop-marked"));
    }

    #[test]
    fn process_exit_before_transcript_completion_resolves_to_failed() {
        let terminal = resolve_native_turn_terminal(
            "gemini",
            "run-failed",
            NativeWaitOutcome::ProcessExited(23),
        );

        assert_eq!(
            terminal,
            NativeTurnTerminal::Failed {
                exit_code: 23,
                error: "Native gemini process exited with code 23 before transcript completion"
                    .to_string()
            }
        );
    }

    #[test]
    fn replies_to_cursor_position_report_query() {
        assert_eq!(
            terminal_control_response("\x1b[6n").as_deref(),
            Some(b"\x1b[30;120R".as_slice())
        );
        assert_eq!(
            terminal_control_response("before \u{9b}6n after").as_deref(),
            Some(b"\x1b[30;120R".as_slice())
        );
    }

    #[test]
    fn ignores_output_without_terminal_control_queries() {
        assert!(terminal_control_response("normal output").is_none());
    }

    #[test]
    fn strips_cursor_position_report_query_from_visible_output() {
        assert_eq!(
            strip_terminal_control_queries("before \x1b[6n after"),
            "before  after"
        );
        assert_eq!(strip_terminal_control_queries("\u{9b}6n"), "");
    }

    #[test]
    fn recognizes_codex_trust_prompt_variants() {
        assert!(should_confirm_codex_trust_prompt(
            "Do you trust the files in this folder?\nYes, continue\nPress Enter to continue"
        ));
        assert!(should_confirm_codex_trust_prompt(
            "Trust this workspace? › Yes, continue"
        ));
        assert!(!should_confirm_codex_trust_prompt("normal model output"));
    }
}

static NATIVE_PTY_PROCESSES: Lazy<Arc<Mutex<HashMap<String, NativePtyProcess>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

static NATIVE_PTY_STOP_REQUESTS: Lazy<Arc<Mutex<std::collections::HashSet<String>>>> =
    Lazy::new(|| Arc::new(Mutex::new(std::collections::HashSet::new())));

fn native_stop_requests() -> Arc<Mutex<std::collections::HashSet<String>>> {
    NATIVE_PTY_STOP_REQUESTS.clone()
}

fn mark_native_pty_stop_requested(run_id: &str) {
    if let Ok(mut stops) = native_stop_requests().lock() {
        stops.insert(run_id.to_string());
    }
}

fn take_native_pty_stop_requested(run_id: &str) -> bool {
    native_stop_requests()
        .lock()
        .map(|mut stops| stops.remove(run_id))
        .unwrap_or(false)
}

fn clear_native_pty_stop_requested(run_id: &str) {
    let _ = take_native_pty_stop_requested(run_id);
}

fn native_map() -> Arc<Mutex<HashMap<String, NativePtyProcess>>> {
    NATIVE_PTY_PROCESSES.clone()
}

pub fn has_native_pty_process(run_id: &str) -> bool {
    native_map()
        .lock()
        .map(|map| map.contains_key(run_id))
        .unwrap_or(false)
}

pub fn stop_native_pty_process(run_id: &str) -> bool {
    mark_native_pty_stop_requested(run_id);
    let removed = native_map()
        .lock()
        .ok()
        .and_then(|mut map| map.remove(run_id));
    if let Some(mut proc) = removed {
        let _ = proc.child.kill();
        let _ = proc.child.wait();
        true
    } else {
        false
    }
}

fn poll_native_pty_process_exit(run_id: &str) -> Result<Option<i32>, String> {
    let map = native_map();
    let mut guard = map.lock().map_err(|e| format!("native pty lock: {e}"))?;
    let exit_code = {
        let Some(proc) = guard.get_mut(run_id) else {
            return Ok(Some(-1));
        };
        match proc.child.try_wait() {
            Ok(Some(status)) => Some(status.exit_code() as i32),
            Ok(None) => None,
            Err(e) => return Err(format!("poll native pty process: {e}")),
        }
    };
    if exit_code.is_some() {
        guard.remove(run_id);
    }
    Ok(exit_code)
}

fn terminal_control_response(text: &str) -> Option<Vec<u8>> {
    let query_count = text.matches("\x1b[6n").count() + text.matches("\u{9b}6n").count();
    if query_count == 0 {
        return None;
    }

    let mut response = Vec::with_capacity(query_count * b"\x1b[30;120R".len());
    for _ in 0..query_count {
        response.extend_from_slice(b"\x1b[30;120R");
    }
    Some(response)
}

fn strip_terminal_control_queries(text: &str) -> String {
    text.replace("\x1b[6n", "").replace("\u{9b}6n", "")
}

fn compact_prompt_text(text: &str) -> String {
    text.replace([' ', '\r', '\n'], "").to_lowercase()
}

fn should_confirm_codex_trust_prompt(text: &str) -> bool {
    let compact = compact_prompt_text(text);
    compact.contains("trust")
        && (compact.contains("yes,continue") || compact.contains("pressentertocontinue"))
}

fn emit_event(run_id: &str, rt: RunEventType, payload: serde_json::Value) {
    if let Err(e) = storage::events::append_event(run_id, rt, payload) {
        log::warn!(
            "[native-pty] failed to append event for run_id={}: {}",
            run_id,
            e
        );
    }
}

fn spawn_reader(
    agent: String,
    mut reader: Box<dyn Read + Send>,
    mut writer: Box<dyn Write + Send>,
) {
    std::thread::spawn(move || {
        let mut buf = [0_u8; 8192];
        let mut trust_prompt_confirmed = false;
        let mut terminal_control_tail = String::new();
        let mut plain_ring = String::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    let control_scan = format!("{terminal_control_tail}{text}");
                    if let Some(response) = terminal_control_response(&control_scan) {
                        let _ = writer.write_all(&response);
                        let _ = writer.flush();
                        terminal_control_tail.clear();
                    } else {
                        let mut tail = control_scan.chars().rev().take(8).collect::<Vec<_>>();
                        tail.reverse();
                        terminal_control_tail = tail.into_iter().collect();
                    }
                    if agent == "codex" && !trust_prompt_confirmed {
                        let plain = strip_ansi_escapes::strip(text.as_bytes());
                        plain_ring.push_str(&String::from_utf8_lossy(&plain));
                        if plain_ring.len() > 4000 {
                            plain_ring = plain_ring.split_off(plain_ring.len() - 4000);
                        }
                        if should_confirm_codex_trust_prompt(&plain_ring) {
                            let _ = writer.write_all(b"\r");
                            let _ = writer.flush();
                            trust_prompt_confirmed = true;
                        }
                    }
                    let _ = strip_terminal_control_queries(&text);
                }
                Err(_) => break,
            }
        }
    });
}

pub async fn run_native_pty_agent(
    app: AppHandle,
    run_id: String,
    command: String,
    args: Vec<String>,
    cwd: String,
    agent: String,
    spawn_env_plan: SpawnEnvPlan,
    display_command: String,
) -> Result<(), String> {
    emit_event(
        &run_id,
        RunEventType::System,
        serde_json::json!({
            "message": format!("Started {}", display_command),
            "source": "ui_chat"
        }),
    );

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 30,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("open native pty: {e}"))?;

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("clone native pty reader: {e}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("take native pty writer: {e}"))?;

    let mut cmd = CommandBuilder::new(command);
    cmd.args(args.iter().map(String::as_str));
    cmd.cwd(Path::new(&cwd));

    let mut child_env: HashMap<String, String> = std::env::vars().collect();
    if let Some(path) = &spawn_env_plan.path_override {
        child_env.insert("PATH".to_string(), path.clone());
    }
    for key in adapter::auth_env_removals_for_extra_env(&spawn_env_plan.msvc_env) {
        child_env.remove(key);
    }
    for (key, value) in &spawn_env_plan.msvc_env {
        child_env.insert(key.clone(), value.clone());
    }
    child_env.insert("OPENCOVIBE_TASK_ID".to_string(), run_id.clone());
    child_env.insert("OPENCOVIBE_RUN_ID".to_string(), run_id.clone());
    child_env.remove("CLAUDECODE");
    for (key, value) in child_env {
        cmd.env(key, value);
    }

    let transcript_baseline =
        native_transcript::capture_native_transcript_baseline(&agent, &cwd).await;
    let spawned_at = SystemTime::now();
    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("spawn native pty command: {e}"))?;
    drop(pair.slave);
    spawn_reader(agent.clone(), reader, writer);

    {
        let map = native_map();
        let mut guard = map.lock().map_err(|e| format!("native pty lock: {e}"))?;
        guard.insert(
            run_id.clone(),
            NativePtyProcess {
                child,
                _master: pair.master,
            },
        );
    }

    let transcript_wait = native_transcript::wait_for_native_transcript_turn(
        &agent,
        &cwd,
        spawned_at,
        transcript_baseline,
    );
    tokio::pin!(transcript_wait);
    let wait_outcome = loop {
        tokio::select! {
            result = &mut transcript_wait => break NativeWaitOutcome::Transcript(result),
            _ = sleep(Duration::from_millis(500)) => {
                match poll_native_pty_process_exit(&run_id) {
                    Ok(Some(code)) => break NativeWaitOutcome::ProcessExited(code),
                    Ok(None) => {}
                    Err(e) => break NativeWaitOutcome::Transcript(Err(e)),
                }
            }
        }
    };

    let terminal = resolve_native_turn_terminal(&agent, &run_id, wait_outcome);

    let (exit_code, assistant_text, conversation_ref, done_error) = match terminal {
        NativeTurnTerminal::Completed {
            assistant_text,
            conversation_ref,
        } => {
            clear_native_pty_stop_requested(&run_id);
            let _ = stop_native_pty_process(&run_id);
            (0, assistant_text, conversation_ref, None)
        }
        NativeTurnTerminal::Stopped => {
            let _ = stop_native_pty_process(&run_id);
            (-1, String::new(), None, Some("Stopped by user".to_string()))
        }
        NativeTurnTerminal::Failed { exit_code, error } => {
            clear_native_pty_stop_requested(&run_id);
            emit_event(
                &run_id,
                RunEventType::Stderr,
                serde_json::json!({ "text": error, "source": "ui_chat" }),
            );
            let _ = app.emit(
                "run-event",
                serde_json::json!({
                    "run_id": run_id,
                    "type": "stderr",
                    "text": error
                }),
            );
            let _ = stop_native_pty_process(&run_id);
            (exit_code, String::new(), None, Some(error))
        }
    };

    if let Some(conversation_ref) = conversation_ref {
        let rid = run_id.clone();
        if let Err(e) = storage::runs::with_meta(&rid, |meta| {
            meta.conversation_ref = Some(conversation_ref);
            Ok(())
        }) {
            log::warn!("[native-pty] failed to persist conversation_ref: {}", e);
        }
    }

    if !assistant_text.trim().is_empty() {
        let _ = app.emit(
            "chat-delta",
            ChatDelta {
                text: assistant_text.clone(),
            },
        );
        emit_event(
            &run_id,
            RunEventType::Assistant,
            serde_json::json!({ "text": assistant_text.trim(), "source": "ui_chat" }),
        );
    }

    if exit_code == 0 {
        if let Err(e) = storage::runs::update_status(&run_id, RunStatus::Completed, Some(0), None) {
            log::warn!("[native-pty] failed to update status to Completed: {}", e);
        }
    } else if exit_code == -1 {
        if let Err(e) = storage::runs::update_status(
            &run_id,
            RunStatus::Stopped,
            None,
            Some("Stopped by user".to_string()),
        ) {
            log::warn!("[native-pty] failed to update status to Stopped: {}", e);
        }
    } else if let Err(e) = storage::runs::update_status(
        &run_id,
        RunStatus::Failed,
        Some(exit_code),
        done_error.clone(),
    ) {
        log::warn!("[native-pty] failed to update status to Failed: {}", e);
    }

    emit_event(
        &run_id,
        RunEventType::System,
        serde_json::json!({ "message": format!("Process exited with code {}", exit_code), "source": "ui_chat" }),
    );
    let _ = app.emit(
        "chat-done",
        ChatDone {
            ok: exit_code == 0,
            code: exit_code,
            error: done_error,
        },
    );

    Ok(())
}
