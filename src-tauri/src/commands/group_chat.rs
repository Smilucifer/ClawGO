use crate::agent::adapter::ActorSessionMap;
use crate::agent::spawn_locks::SpawnLocks;
use crate::agent::stream::ProcessMap;
use crate::models::{ExecutionPath, RunMeta, RunStatus};
use crate::group_chat::adapter::{can_use_group_chat_actor_run, AgentCapabilities};
use crate::group_chat::models::{GroupChatDetail, GroupChatParticipantDetail, GroupChatSummary};
use crate::storage;
use crate::web_server::broadcaster::BroadcastEmitter;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tokio_util::sync::CancellationToken;

fn group_chat_detail(room_id: &str) -> Result<GroupChatDetail, String> {
    let room =
        storage::group_chats::get_group_chat(room_id).ok_or_else(|| format!("GroupChat {} not found", room_id))?;
    let participants = room
        .participants
        .iter()
        .map(|participant| GroupChatParticipantDetail {
            participant: participant.clone(),
            run: crate::commands::runs::get_run(participant.run_id.clone()).ok(),
            capabilities: AgentCapabilities::for_agent(&participant.agent),
        })
        .collect();

    Ok(GroupChatDetail {
        id: room.id,
        name: room.name,
        cwd: room.cwd,
        memo: room.memo,
        participants,
        turns: storage::group_chats::list_group_chat_public_turns(room_id)?,
        created_at: room.created_at,
        updated_at: room.updated_at,
    })
}

#[tauri::command]
pub fn list_group_chats() -> Result<Vec<GroupChatSummary>, String> {
    Ok(storage::group_chats::list_group_chats())
}

#[tauri::command]
pub fn get_group_chat(id: String) -> Result<GroupChatDetail, String> {
    group_chat_detail(&id)
}

#[tauri::command]
pub fn create_group_chat(
    name: String,
    cwd: Option<String>,
) -> Result<GroupChatDetail, String> {
    let room = storage::group_chats::create_group_chat(name, cwd)?;
    group_chat_detail(&room.id)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GroupChatRunIndexEntry {
    pub room_id: String,
    pub room_name: String,
    pub run_ids: Vec<String>,
}

#[tauri::command]
pub fn list_group_chat_run_index() -> Result<Vec<GroupChatRunIndexEntry>, String> {
    let summaries = storage::group_chats::list_group_chats();
    let mut entries = Vec::new();
    for summary in summaries {
        let room = match storage::group_chats::get_group_chat(&summary.id) {
            Some(r) => r,
            None => continue,
        };
        entries.push(GroupChatRunIndexEntry {
            room_id: room.id,
            room_name: room.name,
            run_ids: room.participants.iter().map(|p| p.run_id.clone()).collect(),
        });
    }
    Ok(entries)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParticipantSnapshot {
    pub participant_id: String,
    pub label: String,
    pub content: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GroupChatTurnSnapshot {
    pub turn: crate::group_chat::models::GroupChatTurn,
    pub participant_contents: Vec<ParticipantSnapshot>,
}

#[tauri::command]
pub fn get_group_chat_turn_snapshot(
    room_id: String,
    turn_id: String,
) -> Result<GroupChatTurnSnapshot, String> {
    let room =
        storage::group_chats::get_group_chat(&room_id).ok_or_else(|| format!("GroupChat {room_id} not found"))?;
    let turns = storage::group_chats::list_group_chat_public_turns(&room_id)?;
    let turn = turns
        .iter()
        .find(|t| t.id == turn_id)
        .ok_or_else(|| format!("Turn {turn_id} not found"))?;

    let mut participant_contents = Vec::new();
    for response in &turn.responses {
        let label = room
            .participants
            .iter()
            .find(|p| p.id == response.participant_id)
            .map(|p| p.label.clone())
            .unwrap_or_else(|| response.participant_id.clone());

        let content = if response.status == "deleted" {
            response.preview.clone().unwrap_or_default()
        } else {
            let events = storage::events::list_events(&response.run_id, 0);
            let filtered: Vec<_> = events
                .into_iter()
                .filter(|e| e.seq >= response.event_seq_start && e.seq <= response.event_seq_end)
                .collect();
            extract_assistant_text(&filtered)
                .or_else(|| response.preview.clone())
                .unwrap_or_default()
        };

        participant_contents.push(ParticipantSnapshot {
            participant_id: response.participant_id.clone(),
            label,
            content,
            status: response.status.clone(),
            error: response.error.clone(),
        });
    }

    Ok(GroupChatTurnSnapshot {
        turn: turn.clone(),
        participant_contents,
    })
}

fn extract_assistant_text(events: &[crate::models::RunEvent]) -> Option<String> {
    let mut texts = Vec::new();
    for event in events {
        if let Some(text) = event.payload.get("text").and_then(|v| v.as_str()) {
            texts.push(text.to_string());
        }
    }
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

#[tauri::command]
pub fn attach_group_chat_run(
    room_id: String,
    run_id: String,
    label: Option<String>,
    role: Option<String>,
) -> Result<GroupChatDetail, String> {
    let run = storage::runs::get_run(&run_id).ok_or_else(|| format!("Run {} not found", run_id))?;
    validate_group_chat_participant_run(&run)?;
    storage::group_chats::attach_group_chat_run(&room_id, &run_id, label, role)?;
    group_chat_detail(&room_id)
}

fn validate_group_chat_participant_run(run: &RunMeta) -> Result<(), String> {
    let capabilities = AgentCapabilities::for_agent(&run.agent);
    let path = run.resolved_execution_path();
    let supported = match &path {
        ExecutionPath::SessionActor => capabilities.stream_session && can_use_group_chat_actor_run(run),
        ExecutionPath::PipeExec => capabilities.pipe_exec,
    };
    if supported {
        Ok(())
    } else {
        Err(format!(
            "Run {} uses agent '{}' with {:?}, which is not supported in GroupChats",
            run.id, run.agent, path
        ))
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_group_chat_participant(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    room_id: String,
    agent: Option<String>,
    prompt: String,
    cwd: String,
    model: Option<String>,
    platform_id: Option<String>,
    connection_profile_id: Option<String>,
    label: Option<String>,
    role: Option<String>,
) -> Result<GroupChatDetail, String> {
    create_group_chat_participant_impl(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        room_id,
        agent.unwrap_or_else(|| "claude".to_string()),
        prompt,
        cwd,
        model,
        platform_id,
        connection_profile_id,
        label,
        role,
    )
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_group_chat_claude_participant(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    room_id: String,
    prompt: String,
    cwd: String,
    model: Option<String>,
    platform_id: Option<String>,
    connection_profile_id: Option<String>,
    label: Option<String>,
    role: Option<String>,
) -> Result<GroupChatDetail, String> {
    create_group_chat_participant_impl(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        room_id,
        "claude".to_string(),
        prompt,
        cwd,
        model,
        platform_id,
        connection_profile_id,
        label,
        role,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn create_group_chat_participant_impl(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    cancel_token: &CancellationToken,
    room_id: String,
    agent: String,
    prompt: String,
    cwd: String,
    model: Option<String>,
    platform_id: Option<String>,
    connection_profile_id: Option<String>,
    label: Option<String>,
    role: Option<String>,
) -> Result<GroupChatDetail, String> {
    let normalized_agent = normalize_agent(&agent)?;
    let run_id = create_group_chat_participant_run(
        &room_id,
        normalized_agent.clone(),
        prompt,
        cwd,
        model,
        platform_id.clone(),
        connection_profile_id.clone(),
    )?;
    let run = storage::runs::get_run(&run_id).ok_or_else(|| format!("Run {} not found", run_id))?;
    if matches!(run.resolved_execution_path(), ExecutionPath::SessionActor) {
        if let Err(e) = crate::commands::session::start_session_impl(
            emitter,
            sessions,
            spawn_locks,
            cancel_token,
            run_id.clone(),
            None,
            None,
            None,
            None,
            platform_id,
            Some("plan".to_string()),
        )
        .await
        {
            cleanup_unattached_participant_run(
                emitter,
                sessions,
                spawn_locks,
                &run_id,
                &format!("GroupChat participant startup failed: {e}"),
            )
            .await
            .map_err(|cleanup_error| format!("{e}; cleanup failed: {cleanup_error}"))?;
            return Err(e);
        }
    }

    if let Err(e) = storage::group_chats::attach_group_chat_run(
        &room_id,
        &run_id,
        label.or_else(|| Some(default_participant_label(&normalized_agent))),
        role,
    ) {
        if matches!(run.resolved_execution_path(), ExecutionPath::SessionActor) {
            cleanup_unattached_participant_run(
                emitter,
                sessions,
                spawn_locks,
                &run_id,
                &format!("GroupChat participant attach failed: {e}"),
            )
            .await
            .map_err(|cleanup_error| format!("{e}; cleanup failed: {cleanup_error}"))?;
        } else {
            mark_participant_run_failed_and_deleted(
                &run_id,
                &format!("GroupChat participant attach failed: {e}"),
            )?;
        }
        return Err(e);
    }

    group_chat_detail(&room_id)
}

fn create_group_chat_participant_run(
    room_id: &str,
    agent: String,
    prompt: String,
    cwd: String,
    model: Option<String>,
    platform_id: Option<String>,
    connection_profile_id: Option<String>,
) -> Result<String, String> {
    let _room =
        storage::group_chats::get_group_chat(room_id).ok_or_else(|| format!("GroupChat {} not found", room_id))?;
    let run_id = uuid::Uuid::new_v4().to_string();
    let execution_path = default_group_chat_execution_path(&agent)?;
    let mut meta = storage::runs::create_run_with_connection_profile(
        &run_id,
        &prompt,
        &cwd,
        &agent,
        RunStatus::Pending,
        model,
        None,
        None,
        None,
        None,
        platform_id,
        connection_profile_id,
    )?;
    meta.execution_path = Some(execution_path);
    storage::runs::save_meta(&meta)?;
    Ok(run_id)
}

#[cfg(test)]
fn create_claude_participant_run(
    room_id: &str,
    prompt: String,
    cwd: String,
    model: Option<String>,
    platform_id: Option<String>,
) -> Result<String, String> {
    create_group_chat_participant_run(
        room_id,
        "claude".to_string(),
        prompt,
        cwd,
        model,
        platform_id,
        None,
    )
}

fn normalize_agent(agent: &str) -> Result<String, String> {
    let normalized = agent.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "claude" | "codex" => Ok(normalized),
        _ => Err(format!(
            "Unsupported GroupChat participant agent: {agent}. Supported: claude, codex"
        )),
    }
}

fn default_group_chat_execution_path(agent: &str) -> Result<ExecutionPath, String> {
    let capabilities = AgentCapabilities::for_agent(agent);
    if capabilities.stream_session {
        Ok(ExecutionPath::SessionActor)
    } else if capabilities.pipe_exec {
        Ok(ExecutionPath::PipeExec)
    } else {
        Err(format!("Agent '{agent}' is not supported in GroupChats"))
    }
}

fn default_participant_label(agent: &str) -> String {
    match agent {
        "codex" => "Codex".to_string(),
        _ => "Claude".to_string(),
    }
}

async fn cleanup_unattached_participant_run(
    emitter: &Arc<BroadcastEmitter>,
    sessions: &ActorSessionMap,
    spawn_locks: &SpawnLocks,
    run_id: &str,
    reason: &str,
) -> Result<(), String> {
    crate::commands::session::stop_session_impl(emitter, sessions, spawn_locks, run_id.to_string())
        .await?;
    mark_participant_run_failed_and_deleted(run_id, reason)
}

fn mark_participant_run_failed_and_deleted(run_id: &str, reason: &str) -> Result<(), String> {
    storage::runs::update_status(run_id, RunStatus::Failed, None, Some(reason.to_string()))?;
    storage::runs::soft_delete_runs(&[run_id.to_string()])?;
    Ok(())
}

#[tauri::command]
pub fn update_group_chat_memo(room_id: String, memo: String) -> Result<GroupChatDetail, String> {
    storage::group_chats::update_group_chat_memo(&room_id, memo)?;
    group_chat_detail(&room_id)
}

#[tauri::command]
pub async fn send_group_chat_message(
    app: AppHandle,
    sessions: State<'_, ActorSessionMap>,
    process_map: State<'_, ProcessMap>,
    room_id: String,
    message: String,
) -> Result<GroupChatDetail, String> {
    let _room =
        storage::group_chats::get_group_chat(&room_id).ok_or_else(|| format!("GroupChat {} not found", room_id))?;
    let pipe_runtime = Some(crate::group_chat::orchestrator::GroupChatPipeRuntime {
        app,
        process_map: process_map.inner().clone(),
    });
    crate::group_chat::orchestrator::run_group_chat_turn_with_runtime(
        &room_id,
        &message,
        sessions.inner(),
        pipe_runtime,
        None,
    )
    .await?;
    group_chat_detail(&room_id)
}

#[tauri::command]
pub async fn delete_group_chat(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    id: String,
) -> Result<(), String> {
    let room = storage::group_chats::get_group_chat(&id).ok_or_else(|| format!("GroupChat {} not found", id))?;

    let run_ids: Vec<String> = room.participants.iter().map(|p| p.run_id.clone()).collect();
    for run_id in &run_ids {
        // Stop the session if it's running; ignore errors (session may not exist)
        let _ = crate::commands::session::stop_session_impl(
            emitter.inner(),
            sessions.inner(),
            spawn_locks.inner(),
            run_id.clone(),
        )
        .await;
    }

    // Batch soft-delete so runs disappear from sidebar
    if let Err(e) = storage::runs::soft_delete_runs(&run_ids) {
        log::warn!(
            "[group-chat] delete_group_chat: soft_delete_runs failed (runs may still appear in sidebar): {e}"
        );
    }

    storage::group_chats::delete_group_chat(&id)
}

#[tauri::command]
pub async fn cancel_group_chat_turn(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    room_id: String,
) -> Result<bool, String> {
    log::debug!("[group-chat] cancel_group_chat_turn: room_id={}", room_id);
    let room =
        storage::group_chats::get_group_chat(&room_id).ok_or_else(|| format!("GroupChat {} not found", room_id))?;

    for participant in &room.participants {
        // Only stop participants that are actually running
        let is_running = storage::runs::get_run(&participant.run_id)
            .map(|r| r.status == RunStatus::Running)
            .unwrap_or(false);
        if !is_running {
            continue;
        }
        if let Err(e) = crate::commands::session::stop_session_impl(
            emitter.inner(),
            sessions.inner(),
            spawn_locks.inner(),
            participant.run_id.clone(),
        )
        .await
        {
            log::warn!(
                "[group-chat] cancel_group_chat_turn: failed to stop {}: {}",
                participant.run_id,
                e
            );
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use crate::models::{ExecutionPath, RunStatus};

    fn with_temp_data_dir<T>(f: impl FnOnce() -> T) -> T {
        let _guard = crate::storage::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("CLAW_GO_DATA_DIR");
        std::env::set_var("CLAW_GO_DATA_DIR", tmp.path());
        let result = f();
        match previous {
            Some(value) => std::env::set_var("CLAW_GO_DATA_DIR", value),
            None => std::env::remove_var("CLAW_GO_DATA_DIR"),
        }
        result
    }

    #[test]
    fn get_group_chat_detail_reads_referenced_run_without_copying() {
        with_temp_data_dir(|| {
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();
            crate::storage::runs::create_run(
                "run-1",
                "hello",
                "D:/work/app",
                "claude",
                RunStatus::Idle,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-1", None, None).unwrap();

            let detail = super::group_chat_detail(&room.id).unwrap();

            assert_eq!(detail.participants.len(), 1);
            assert!(detail.participants[0].capabilities.stream_session);
            assert!(detail.turns.is_empty());
            assert_eq!(detail.participants[0].run.as_ref().unwrap().prompt, "hello");

            crate::storage::runs::rename_run("run-1", "Renamed").unwrap();
            let detail = super::group_chat_detail(&room.id).unwrap();
            assert_eq!(
                detail.participants[0].run.as_ref().unwrap().name.as_deref(),
                Some("Renamed")
            );
        });
    }

    #[test]
    fn group_chat_detail_includes_participant_capabilities() {
        with_temp_data_dir(|| {
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();
            crate::storage::runs::create_run(
                "run-codex",
                "hello",
                "D:/work/app",
                "codex",
                RunStatus::Idle,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-codex", Some("Codex".into()), None)
                .unwrap();

            let detail = super::group_chat_detail(&room.id).unwrap();

            assert_eq!(detail.participants.len(), 1);
            assert_eq!(
                detail.participants[0].capabilities.kind,
                crate::group_chat::adapter::AgentKind::Codex
            );
            assert!(!detail.participants[0].capabilities.stream_session);
            assert!(detail.participants[0].capabilities.pipe_exec);
        });
    }

    #[test]
    fn attach_group_chat_run_accepts_codex_pipe_exec_runs() {
        with_temp_data_dir(|| {
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();
            crate::storage::runs::create_run(
                "run-codex",
                "hello",
                "D:/work/app",
                "codex",
                RunStatus::Idle,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();

            let detail = super::attach_group_chat_run(room.id, "run-codex".into(), None, None).unwrap();

            assert_eq!(detail.participants.len(), 1);
            assert_eq!(detail.participants[0].participant.agent, "codex");
        });
    }

    #[test]
    fn attach_group_chat_run_accepts_claude_pipe_exec_runs() {
        with_temp_data_dir(|| {
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();
            let mut run = crate::storage::runs::create_run(
                "run-claude-pipe",
                "hello",
                "D:/work/app",
                "claude",
                RunStatus::Idle,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            run.execution_path = Some(ExecutionPath::PipeExec);
            crate::storage::runs::save_meta(&run).unwrap();

            let detail =
                super::attach_group_chat_run(room.id, "run-claude-pipe".into(), None, None).unwrap();

            assert_eq!(detail.participants.len(), 1);
            assert_eq!(detail.participants[0].participant.run_id, "run-claude-pipe");
        });
    }

    #[test]
    fn attach_group_chat_run_accepts_claude_session_actor_runs() {
        with_temp_data_dir(|| {
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();
            let mut run = crate::storage::runs::create_run(
                "run-claude-session",
                "hello",
                "D:/work/app",
                "claude",
                RunStatus::Idle,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            run.execution_path = Some(ExecutionPath::SessionActor);
            crate::storage::runs::save_meta(&run).unwrap();

            let detail =
                super::attach_group_chat_run(room.id, "run-claude-session".into(), None, None).unwrap();

            assert_eq!(detail.participants.len(), 1);
            assert_eq!(
                detail.participants[0].participant.run_id,
                "run-claude-session"
            );
        });
    }

    #[test]
    fn create_claude_participant_creates_referenced_run() {
        with_temp_data_dir(|| {
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();

            let run_id = super::create_claude_participant_run(
                &room.id,
                "Investigate".to_string(),
                "D:/work/app".to_string(),
                Some("sonnet".to_string()),
                None,
            )
            .unwrap();

            let run = crate::storage::runs::get_run(&run_id).unwrap();
            assert_eq!(run.agent, "claude");
            assert_eq!(run.prompt, "Investigate");
            assert_eq!(run.cwd, "D:/work/app");
            assert_eq!(
                run.execution_path,
                Some(crate::models::ExecutionPath::SessionActor)
            );
        });
    }

    #[test]
    fn create_group_chat_participant_run_defaults_codex_to_pipe_exec() {
        with_temp_data_dir(|| {
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();

            let run_id = super::create_group_chat_participant_run(
                &room.id,
                "codex".to_string(),
                "Investigate".to_string(),
                "D:/work/app".to_string(),
                Some("gpt-5.5".to_string()),
                None,
                None,
            )
            .unwrap();

            let run = crate::storage::runs::get_run(&run_id).unwrap();
            assert_eq!(run.agent, "codex");
            assert_eq!(run.execution_path, Some(ExecutionPath::PipeExec));
        });
    }

    #[test]
    fn participant_cleanup_soft_deletes_created_run() {
        with_temp_data_dir(|| {
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();
            let run_id = super::create_claude_participant_run(
                &room.id,
                "Investigate".to_string(),
                "D:/work/app".to_string(),
                None,
                None,
            )
            .unwrap();

            super::mark_participant_run_failed_and_deleted(&run_id, "startup failed").unwrap();

            assert!(crate::storage::runs::get_run(&run_id).is_none());
        });
    }

}
