use crate::agent::adapter::ActorSessionMap;
use crate::agent::spawn_locks::SpawnLocks;
use crate::models::{ExecutionPath, RunStatus};
use crate::room::models::{RoomDetail, RoomParticipantDetail, RoomSummary};
use crate::storage;
use crate::web_server::broadcaster::BroadcastEmitter;
use std::sync::Arc;
use tauri::State;
use tokio_util::sync::CancellationToken;

fn room_detail(room_id: &str) -> Result<RoomDetail, String> {
    let room =
        storage::rooms::get_room(room_id).ok_or_else(|| format!("Room {} not found", room_id))?;
    let participants = room
        .participants
        .iter()
        .map(|participant| RoomParticipantDetail {
            participant: participant.clone(),
            run: crate::commands::runs::get_run(participant.run_id.clone()).ok(),
        })
        .collect();

    Ok(RoomDetail {
        id: room.id,
        kind: room.kind,
        name: room.name,
        description: room.description,
        cwd: room.cwd,
        memo: room.memo,
        participants,
        turns: storage::rooms::list_public_turns(room_id)?,
        created_at: room.created_at,
        updated_at: room.updated_at,
    })
}

#[tauri::command]
pub fn list_rooms() -> Result<Vec<RoomSummary>, String> {
    Ok(storage::rooms::list_rooms())
}

#[tauri::command]
pub fn get_room(id: String) -> Result<RoomDetail, String> {
    room_detail(&id)
}

#[tauri::command]
pub fn create_room(
    name: String,
    description: Option<String>,
    cwd: Option<String>,
) -> Result<RoomDetail, String> {
    let room = storage::rooms::create_room(name, description.unwrap_or_default(), cwd)?;
    room_detail(&room.id)
}

#[tauri::command]
pub fn attach_room_run(
    room_id: String,
    run_id: String,
    label: Option<String>,
    role: Option<String>,
) -> Result<RoomDetail, String> {
    let run = storage::runs::get_run(&run_id).ok_or_else(|| format!("Run {} not found", run_id))?;
    if run.agent != "claude" {
        return Err("Phase 2 rooms only support Claude participants".to_string());
    }
    storage::rooms::attach_run(&room_id, &run_id, label, role)?;
    room_detail(&room_id)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_room_claude_participant(
    emitter: State<'_, Arc<BroadcastEmitter>>,
    sessions: State<'_, ActorSessionMap>,
    spawn_locks: State<'_, SpawnLocks>,
    cancel_token: State<'_, CancellationToken>,
    room_id: String,
    prompt: String,
    cwd: String,
    model: Option<String>,
    platform_id: Option<String>,
    label: Option<String>,
    role: Option<String>,
) -> Result<RoomDetail, String> {
    let run_id = create_claude_participant_run(&room_id, prompt, cwd, model, platform_id.clone())?;
    if let Err(e) = crate::commands::session::start_session_impl(
        emitter.inner(),
        sessions.inner(),
        spawn_locks.inner(),
        cancel_token.inner(),
        run_id.clone(),
        None,
        None,
        None,
        None,
        platform_id,
        None,
    )
    .await
    {
        cleanup_unattached_participant_run(
            emitter.inner(),
            sessions.inner(),
            spawn_locks.inner(),
            &run_id,
            &format!("Room participant startup failed: {e}"),
        )
        .await
        .map_err(|cleanup_error| format!("{e}; cleanup failed: {cleanup_error}"))?;
        return Err(e);
    }

    if let Err(e) = storage::rooms::attach_run(
        &room_id,
        &run_id,
        label.or_else(|| Some("Claude".to_string())),
        role,
    ) {
        cleanup_unattached_participant_run(
            emitter.inner(),
            sessions.inner(),
            spawn_locks.inner(),
            &run_id,
            &format!("Room participant attach failed: {e}"),
        )
        .await
        .map_err(|cleanup_error| format!("{e}; cleanup failed: {cleanup_error}"))?;
        return Err(e);
    }

    room_detail(&room_id)
}

fn create_claude_participant_run(
    room_id: &str,
    prompt: String,
    cwd: String,
    model: Option<String>,
    platform_id: Option<String>,
) -> Result<String, String> {
    let _room =
        storage::rooms::get_room(room_id).ok_or_else(|| format!("Room {} not found", room_id))?;
    let run_id = uuid::Uuid::new_v4().to_string();
    let mut meta = storage::runs::create_run(
        &run_id,
        &prompt,
        &cwd,
        "claude",
        RunStatus::Pending,
        model,
        None,
        None,
        None,
        None,
        platform_id,
    )?;
    meta.execution_path = Some(ExecutionPath::SessionActor);
    storage::runs::save_meta(&meta)?;
    Ok(run_id)
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
pub fn update_room_memo(room_id: String, memo: String) -> Result<RoomDetail, String> {
    storage::rooms::update_memo(&room_id, memo)?;
    room_detail(&room_id)
}

#[tauri::command]
pub async fn send_room_message(
    sessions: State<'_, ActorSessionMap>,
    room_id: String,
    message: String,
) -> Result<RoomDetail, String> {
    crate::room::orchestrator::run_roundtable_turn(&room_id, &message, sessions.inner()).await?;
    room_detail(&room_id)
}

#[tauri::command]
pub fn delete_room(id: String) -> Result<(), String> {
    storage::rooms::delete_room(&id)
}

#[cfg(test)]
mod tests {
    use crate::models::RunStatus;

    fn with_temp_data_dir<T>(f: impl FnOnce() -> T) -> T {
        let _guard = crate::storage::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("OPENCOVIBE_DATA_DIR");
        std::env::set_var("OPENCOVIBE_DATA_DIR", tmp.path());
        let result = f();
        match previous {
            Some(value) => std::env::set_var("OPENCOVIBE_DATA_DIR", value),
            None => std::env::remove_var("OPENCOVIBE_DATA_DIR"),
        }
        result
    }

    #[test]
    fn get_room_detail_reads_referenced_run_without_copying() {
        with_temp_data_dir(|| {
            let room = crate::storage::rooms::create_room("Room".into(), "".into(), None).unwrap();
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
            crate::storage::rooms::attach_run(&room.id, "run-1", None, None).unwrap();

            let detail = super::room_detail(&room.id).unwrap();

            assert_eq!(detail.participants.len(), 1);
            assert!(detail.turns.is_empty());
            assert_eq!(detail.participants[0].run.as_ref().unwrap().prompt, "hello");

            crate::storage::runs::rename_run("run-1", "Renamed").unwrap();
            let detail = super::room_detail(&room.id).unwrap();
            assert_eq!(
                detail.participants[0].run.as_ref().unwrap().name.as_deref(),
                Some("Renamed")
            );
        });
    }

    #[test]
    fn create_claude_participant_creates_referenced_run() {
        with_temp_data_dir(|| {
            let room = crate::storage::rooms::create_room("Room".into(), "".into(), None).unwrap();

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
    fn participant_cleanup_soft_deletes_created_run() {
        with_temp_data_dir(|| {
            let room = crate::storage::rooms::create_room("Room".into(), "".into(), None).unwrap();
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
