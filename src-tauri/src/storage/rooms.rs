use crate::models::now_iso;
use crate::room::models::{Room, RoomKind, RoomParticipant, RoomSummary, RoomTurn};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

static ROOM_LOCKS: Lazy<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn room_lock(id: &str) -> Arc<Mutex<()>> {
    let mut locks = ROOM_LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    locks
        .entry(id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn rooms_dir() -> PathBuf {
    super::data_dir().join("rooms")
}

fn room_dir(id: &str) -> PathBuf {
    rooms_dir().join(id)
}

fn room_file(id: &str) -> PathBuf {
    room_dir(id).join("room.json")
}

fn public_timeline_file(id: &str) -> PathBuf {
    room_dir(id).join("timeline.jsonl")
}

fn private_file(id: &str) -> PathBuf {
    room_dir(id).join("private.json")
}

fn validate_room_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id == "." || id == ".." {
        return Err(format!("Invalid room id: {}", id));
    }
    Ok(())
}

fn save_room(room: &Room) -> Result<(), String> {
    validate_room_id(&room.id)?;
    let dir = room_dir(&room.id);
    super::ensure_dir(&dir).map_err(|e| e.to_string())?;
    let path = room_file(&room.id);
    let tmp = dir.join(format!(
        "room.json.{}.{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let json = serde_json::to_string_pretty(room).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| format!("write tmp: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("rename room file: {e}")
    })
}

pub fn create_room(name: String, description: String, cwd: Option<String>) -> Result<Room, String> {
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() {
        return Err("Room name is required".to_string());
    }

    let now = now_iso();
    let room = Room {
        id: uuid::Uuid::new_v4().to_string(),
        kind: RoomKind::Roundtable,
        name: trimmed_name.to_string(),
        description: description.trim().to_string(),
        cwd: cwd.filter(|s| !s.trim().is_empty()),
        memo: String::new(),
        participants: vec![],
        created_at: now.clone(),
        updated_at: now,
    };
    save_room(&room)?;
    Ok(room)
}

pub fn get_room(id: &str) -> Option<Room> {
    if validate_room_id(id).is_err() {
        return None;
    }
    let path = room_file(id);
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn list_rooms() -> Vec<RoomSummary> {
    let entries = match fs::read_dir(rooms_dir()) {
        Ok(entries) => entries,
        Err(e) => {
            log::debug!("[rooms] cannot read rooms dir: {}", e);
            return vec![];
        }
    };

    let mut rooms = entries
        .flatten()
        .filter_map(|entry| {
            if !entry.path().is_dir() {
                return None;
            }
            let id = entry.file_name().to_string_lossy().to_string();
            get_room(&id).map(|room| {
                let memo_preview = memo_preview(&room.memo);
                RoomSummary {
                    id: room.id,
                    kind: room.kind,
                    name: room.name,
                    description: room.description,
                    cwd: room.cwd,
                    participant_count: room.participants.len(),
                    memo_preview,
                    updated_at: room.updated_at,
                }
            })
        })
        .collect::<Vec<_>>();
    rooms.sort_by_key(|r| std::cmp::Reverse(r.updated_at.clone()));
    rooms
}

pub fn attach_run(
    room_id: &str,
    run_id: &str,
    label: Option<String>,
    role: Option<String>,
) -> Result<Room, String> {
    validate_room_id(room_id)?;
    let run = super::runs::get_run(run_id).ok_or_else(|| format!("Run {} not found", run_id))?;
    let room_lock = room_lock(room_id);
    let _guard = room_lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut room = get_room(room_id).ok_or_else(|| format!("Room {} not found", room_id))?;

    let label = label
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let role = role.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    if let Some(participant) = room.participants.iter_mut().find(|p| p.run_id == run_id) {
        let mut changed = false;
        if let Some(label) = label {
            if participant.label != label {
                participant.label = label;
                changed = true;
            }
        }
        if let Some(role) = role {
            if participant.role != role {
                participant.role = role;
                changed = true;
            }
        }
        if changed {
            room.updated_at = now_iso();
            save_room(&room)?;
        }
        return Ok(room);
    }

    let participant = RoomParticipant {
        id: uuid::Uuid::new_v4().to_string(),
        run_id: run_id.to_string(),
        agent: run.agent,
        label: label.unwrap_or_else(|| "Claude".to_string()),
        role: role.unwrap_or_else(|| "participant".to_string()),
        joined_at: now_iso(),
    };
    room.participants.push(participant);
    room.updated_at = now_iso();
    save_room(&room)?;
    Ok(room)
}

pub fn update_memo(room_id: &str, memo: String) -> Result<Room, String> {
    validate_room_id(room_id)?;
    let room_lock = room_lock(room_id);
    let _guard = room_lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut room = get_room(room_id).ok_or_else(|| format!("Room {} not found", room_id))?;
    room.memo = memo;
    room.updated_at = now_iso();
    save_room(&room)?;
    Ok(room)
}

pub fn append_public_turn(room_id: &str, turn: &RoomTurn) -> Result<(), String> {
    append_turn_jsonl(room_id, public_timeline_file(room_id), turn)
}

pub fn list_public_turns(room_id: &str) -> Result<Vec<RoomTurn>, String> {
    list_turns_jsonl(room_id, public_timeline_file(room_id))
}

pub fn append_private_turn(room_id: &str, turn: &RoomTurn) -> Result<(), String> {
    validate_room_id(room_id)?;
    let room_lock = room_lock(room_id);
    let _guard = room_lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = read_private_file(room_id)?;
    file.turns.push(turn.clone());
    write_private_file(room_id, &file)?;
    touch_room_updated_at(room_id)
}

pub fn list_private_turns(room_id: &str) -> Result<Vec<RoomTurn>, String> {
    Ok(read_private_file(room_id)?.turns)
}

fn append_turn_jsonl(room_id: &str, path: PathBuf, turn: &RoomTurn) -> Result<(), String> {
    validate_room_id(room_id)?;
    let room_lock = room_lock(room_id);
    let _guard = room_lock.lock().unwrap_or_else(|e| e.into_inner());
    let dir = room_dir(room_id);
    super::ensure_dir(&dir).map_err(|e| e.to_string())?;
    let line = serde_json::to_string(turn).map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("open room timeline: {e}"))?;
    writeln!(file, "{}", line).map_err(|e| format!("write room timeline: {e}"))?;
    touch_room_updated_at(room_id)
}

fn touch_room_updated_at(room_id: &str) -> Result<(), String> {
    let mut room = get_room(room_id).ok_or_else(|| format!("Room {} not found", room_id))?;
    room.updated_at = now_iso();
    save_room(&room)
}

fn list_turns_jsonl(room_id: &str, path: PathBuf) -> Result<Vec<RoomTurn>, String> {
    validate_room_id(room_id)?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(path).map_err(|e| format!("read room timeline: {e}"))?;
    Ok(content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<RoomTurn>(line).ok())
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PrivateTurnsFile {
    schema_version: u32,
    turns: Vec<RoomTurn>,
}

fn read_private_file(room_id: &str) -> Result<PrivateTurnsFile, String> {
    validate_room_id(room_id)?;
    let path = private_file(room_id);
    if !path.exists() {
        return Ok(PrivateTurnsFile {
            schema_version: 1,
            turns: vec![],
        });
    }
    let content = fs::read_to_string(path).map_err(|e| format!("read private turns: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("parse private turns: {e}"))
}

fn write_private_file(room_id: &str, file: &PrivateTurnsFile) -> Result<(), String> {
    validate_room_id(room_id)?;
    let dir = room_dir(room_id);
    super::ensure_dir(&dir).map_err(|e| e.to_string())?;
    let path = private_file(room_id);
    let tmp = dir.join(format!(
        "private.json.{}.{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let json = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| format!("write private tmp: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("rename private file: {e}")
    })
}

pub fn delete_room(room_id: &str) -> Result<(), String> {
    validate_room_id(room_id)?;
    let room_lock = room_lock(room_id);
    let _guard = room_lock.lock().unwrap_or_else(|e| e.into_inner());
    let dir = room_dir(room_id);
    if !dir.exists() {
        return Err(format!("Room {} not found", room_id));
    }
    fs::remove_dir_all(&dir).map_err(|e| format!("delete room: {e}"))
}

fn memo_preview(memo: &str) -> Option<String> {
    let trimmed = memo.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() > 120 {
        let end = trimmed
            .char_indices()
            .nth(120)
            .map(|(i, _)| i)
            .unwrap_or(trimmed.len());
        Some(format!("{}...", &trimmed[..end]))
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn room_can_be_created_listed_and_reopened() {
        with_temp_data_dir(|| {
            let room = create_room(
                "Design Review".to_string(),
                "Compare implementation options".to_string(),
                Some("D:/work/app".to_string()),
            )
            .unwrap();

            let reopened = get_room(&room.id).unwrap();
            assert_eq!(reopened.name, "Design Review");
            assert_eq!(reopened.description, "Compare implementation options");
            assert_eq!(reopened.cwd.as_deref(), Some("D:/work/app"));

            let rooms = list_rooms();
            assert_eq!(rooms.len(), 1);
            assert_eq!(rooms[0].id, room.id);
            assert_eq!(rooms[0].participant_count, 0);
        });
    }

    #[test]
    fn room_attaches_existing_run_by_reference() {
        with_temp_data_dir(|| {
            let room = create_room("Room".to_string(), "".to_string(), None).unwrap();
            let run = crate::storage::runs::create_run(
                "run-1",
                "hello",
                "D:/work/app",
                "claude",
                RunStatus::Idle,
                Some("sonnet".to_string()),
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();

            let updated = attach_run(
                &room.id,
                &run.id,
                Some("Reviewer".to_string()),
                Some("reviewer".to_string()),
            )
            .unwrap();

            assert_eq!(updated.participants.len(), 1);
            assert_eq!(updated.participants[0].run_id, "run-1");
            assert_eq!(updated.participants[0].agent, "claude");
            assert_eq!(updated.participants[0].label, "Reviewer");
            assert_eq!(
                crate::storage::runs::get_run("run-1").unwrap().prompt,
                "hello"
            );
        });
    }

    #[test]
    fn duplicate_attach_updates_existing_participant_metadata() {
        with_temp_data_dir(|| {
            let room = create_room("Room".to_string(), "".to_string(), None).unwrap();
            let run = crate::storage::runs::create_run(
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

            attach_run(&room.id, &run.id, Some("Old".to_string()), None).unwrap();
            let updated = attach_run(
                &room.id,
                &run.id,
                Some("Reviewer".to_string()),
                Some("reviewer".to_string()),
            )
            .unwrap();

            assert_eq!(updated.participants.len(), 1);
            assert_eq!(updated.participants[0].label, "Reviewer");
            assert_eq!(updated.participants[0].role, "reviewer");
        });
    }

    #[test]
    fn delete_room_does_not_delete_referenced_run() {
        with_temp_data_dir(|| {
            let room = create_room("Room".to_string(), "".to_string(), None).unwrap();
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
            attach_run(&room.id, "run-1", None, None).unwrap();

            delete_room(&room.id).unwrap();

            assert!(get_room(&room.id).is_none());
            assert!(crate::storage::runs::get_run("run-1").is_some());
        });
    }

    #[test]
    fn room_memo_updates_summary_preview() {
        with_temp_data_dir(|| {
            let room = create_room("Room".to_string(), "".to_string(), None).unwrap();
            update_memo(&room.id, "Remember the API boundary.".to_string()).unwrap();

            let reopened = get_room(&room.id).unwrap();
            assert_eq!(reopened.memo, "Remember the API boundary.");
            assert_eq!(
                list_rooms()[0].memo_preview.as_deref(),
                Some("Remember the API boundary.")
            );
        });
    }

    #[test]
    fn room_defaults_to_roundtable_kind_and_lists_timeline() {
        with_temp_data_dir(|| {
            let room = create_room("Room".to_string(), "".to_string(), None).unwrap();
            let original_updated_at = room.updated_at.clone();
            assert_eq!(room.kind, crate::room::models::RoomKind::Roundtable);
            std::thread::sleep(std::time::Duration::from_millis(2));

            let turn = crate::room::models::RoomTurn {
                id: "turn-1".to_string(),
                idx: 1,
                mode: crate::room::models::RoomTurnMode::Fanout,
                user_input: "Compare options".to_string(),
                target_participant_ids: vec!["p1".to_string()],
                responses: vec![crate::room::models::RoomResponseRef {
                    participant_id: "p1".to_string(),
                    run_id: "run-1".to_string(),
                    event_seq_start: 2,
                    event_seq_end: 5,
                    preview: Some("Use the smaller API.".to_string()),
                    status: "complete".to_string(),
                    error: None,
                }],
                started_at: "2026-04-30T00:00:00Z".to_string(),
                completed_at: Some("2026-04-30T00:00:01Z".to_string()),
            };

            append_public_turn(&room.id, &turn).unwrap();
            let turns = list_public_turns(&room.id).unwrap();

            assert_eq!(turns, vec![turn]);
            assert_ne!(get_room(&room.id).unwrap().updated_at, original_updated_at);
        });
    }

    #[test]
    fn private_turns_are_stored_separately_from_public_timeline() {
        with_temp_data_dir(|| {
            let room = create_room("Room".to_string(), "".to_string(), None).unwrap();
            let private_turn = crate::room::models::RoomTurn {
                id: "private-1".to_string(),
                idx: 1,
                mode: crate::room::models::RoomTurnMode::Private,
                user_input: "@Reviewer check this privately".to_string(),
                target_participant_ids: vec!["p1".to_string()],
                responses: vec![],
                started_at: "2026-04-30T00:00:00Z".to_string(),
                completed_at: Some("2026-04-30T00:00:01Z".to_string()),
            };

            append_private_turn(&room.id, &private_turn).unwrap();

            assert!(list_public_turns(&room.id).unwrap().is_empty());
            assert_eq!(list_private_turns(&room.id).unwrap(), vec![private_turn]);
        });
    }

    #[test]
    fn legacy_room_json_without_kind_reopens_as_roundtable() {
        with_temp_data_dir(|| {
            let id = "legacy-room";
            let dir = room_dir(id);
            super::super::ensure_dir(&dir).unwrap();
            fs::write(
                room_file(id),
                r#"{
  "id": "legacy-room",
  "name": "Legacy",
  "description": "",
  "cwd": null,
  "memo": "",
  "participants": [],
  "created_at": "2026-04-30T00:00:00Z",
  "updated_at": "2026-04-30T00:00:00Z"
}"#,
            )
            .unwrap();

            let room = get_room(id).unwrap();

            assert_eq!(room.kind, crate::room::models::RoomKind::Roundtable);
        });
    }
}
