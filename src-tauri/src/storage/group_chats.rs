use crate::models::now_iso;
use crate::group_chat::models::{
    GroupChat, GroupChatParticipant, GroupChatSummary, GroupChatTurn,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

static GROUP_CHAT_LOCKS: Lazy<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn group_chat_lock(id: &str) -> Arc<Mutex<()>> {
    let mut locks = GROUP_CHAT_LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    locks
        .entry(id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn group_chats_dir() -> PathBuf {
    super::data_dir().join("group-chats")
}

fn group_chat_dir(id: &str) -> PathBuf {
    group_chats_dir().join(id)
}

fn group_chat_file(id: &str) -> PathBuf {
    group_chat_dir(id).join("group_chat.json")
}

fn public_timeline_file(id: &str) -> PathBuf {
    group_chat_dir(id).join("timeline.jsonl")
}

fn private_file(id: &str) -> PathBuf {
    group_chat_dir(id).join("private.json")
}

fn validate_group_chat_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id == "." || id == ".." {
        return Err(format!("Invalid group chat id: {}", id));
    }
    Ok(())
}

pub(crate) fn save_group_chat(room: &GroupChat) -> Result<(), String> {
    validate_group_chat_id(&room.id)?;
    let dir = group_chat_dir(&room.id);
    super::ensure_dir(&dir).map_err(|e| e.to_string())?;
    let path = group_chat_file(&room.id);
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
        format!("rename group chat file: {e}")
    })
}

pub fn create_group_chat(name: String, cwd: Option<String>) -> Result<GroupChat, String> {
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() {
        return Err("GroupChat name is required".to_string());
    }

    let now = now_iso();
    let room = GroupChat {
        id: uuid::Uuid::new_v4().to_string(),
        name: trimmed_name.to_string(),
        cwd: cwd.filter(|s| !s.trim().is_empty()),
        memo: String::new(),
        participants: vec![],
        created_at: now.clone(),
        updated_at: now,
        auto_chain: false,
    };
    save_group_chat(&room)?;
    Ok(room)
}

pub fn get_group_chat(id: &str) -> Option<GroupChat> {
    if validate_group_chat_id(id).is_err() {
        return None;
    }
    let path = group_chat_file(id);
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn list_group_chats() -> Vec<GroupChatSummary> {
    let entries = match fs::read_dir(group_chats_dir()) {
        Ok(entries) => entries,
        Err(e) => {
            log::debug!("[rooms] cannot read group chats dir: {}", e);
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
            get_group_chat(&id).map(|room| {
                let memo_preview = memo_preview(&room.memo);
                GroupChatSummary {
                    id: room.id,
                    name: room.name,
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

pub fn attach_group_chat_run(
    room_id: &str,
    run_id: &str,
    label: Option<String>,
    role: Option<String>,
    character_id: Option<String>,
) -> Result<GroupChat, String> {
    validate_group_chat_id(room_id)?;
    let run = super::runs::get_run(run_id).ok_or_else(|| format!("Run {} not found", run_id))?;
    let group_chat_lock = group_chat_lock(room_id);
    let _guard = group_chat_lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut room = get_group_chat(room_id).ok_or_else(|| format!("GroupChat {} not found", room_id))?;

    let label = label
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let existing = room.participants.iter().any(|p| p.run_id == run_id);
    let role = if existing && role.is_none() {
        None
    } else {
        Some(normalize_participant_role(&room, role))
    };
    if role.as_deref() == Some("driver") {
        for participant in &mut room.participants {
            if participant.run_id != run_id && participant.role == "driver" {
                participant.role = "copilot".to_string();
            }
        }
    }

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
        if let Some(cid) = character_id {
            if participant.character_id != cid {
                participant.character_id = cid;
                changed = true;
            }
        }
        if changed {
            room.updated_at = now_iso();
            save_group_chat(&room)?;
        }
        return Ok(room);
    }

    let participant = GroupChatParticipant {
        id: uuid::Uuid::new_v4().to_string(),
        run_id: run_id.to_string(),
        agent: run.agent,
        label: label.unwrap_or_else(|| "Claude".to_string()),
        role: role.unwrap_or_else(|| "participant".to_string()),
        character_id: character_id.unwrap_or_default(),
        joined_at: now_iso(),
    };
    room.participants.push(participant);
    room.updated_at = now_iso();
    save_group_chat(&room)?;
    Ok(room)
}

fn normalize_participant_role(_room: &GroupChat, role: Option<String>) -> String {
    let requested = role
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty());

    requested.unwrap_or_else(|| "participant".to_string())
}

pub fn detach_group_chat_run(room_id: &str, run_id: &str) -> Result<GroupChat, String> {
    validate_group_chat_id(room_id)?;
    let group_chat_lock = group_chat_lock(room_id);
    let _guard = group_chat_lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut room = get_group_chat(room_id).ok_or_else(|| format!("GroupChat {} not found", room_id))?;
    if room.participants.len() <= 1 {
        return Err("Cannot remove the last participant. Delete the group chat instead.".to_string());
    }
    let before = room.participants.len();
    room.participants.retain(|p| p.run_id != run_id);
    if room.participants.len() == before {
        return Err(format!("Run {} not found in group chat {}", run_id, room_id));
    }
    room.updated_at = now_iso();
    save_group_chat(&room)?;
    // Clean up orphaned participant meta file. Validate run_id before joining it into
    // a path (it comes from the frontend) so a crafted id can't delete an arbitrary
    // *.meta.json outside this group chat. Don't silently swallow the delete error —
    // a missing file is fine (NotFound), but anything else is worth a log line.
    if super::validate_run_id(run_id).is_ok() {
        let meta_path = group_chat_dir(room_id)
            .join("participants")
            .join(format!("{}.meta.json", run_id));
        if let Err(e) = std::fs::remove_file(&meta_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!(
                    "[storage/group_chats] detach: failed to remove participant meta {}: {}",
                    meta_path.display(),
                    e
                );
            }
        }
    } else {
        log::warn!("[storage/group_chats] detach: skipped meta cleanup for invalid run_id");
    }
    Ok(room)
}

pub fn update_group_chat_memo(room_id: &str, memo: String) -> Result<GroupChat, String> {
    validate_group_chat_id(room_id)?;
    let group_chat_lock = group_chat_lock(room_id);
    let _guard = group_chat_lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut room = get_group_chat(room_id).ok_or_else(|| format!("GroupChat {} not found", room_id))?;
    room.memo = memo;
    room.updated_at = now_iso();
    save_group_chat(&room)?;
    Ok(room)
}

pub fn append_group_chat_public_turn(room_id: &str, turn: &GroupChatTurn) -> Result<(), String> {
    append_turn_jsonl(room_id, public_timeline_file(room_id), turn)
}

pub fn list_group_chat_public_turns(room_id: &str) -> Result<Vec<GroupChatTurn>, String> {
    list_turns_jsonl(room_id, public_timeline_file(room_id))
}

pub fn append_group_chat_private_turn(room_id: &str, turn: &GroupChatTurn) -> Result<(), String> {
    validate_group_chat_id(room_id)?;
    let group_chat_lock = group_chat_lock(room_id);
    let _guard = group_chat_lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = read_private_file(room_id)?;
    file.turns.push(turn.clone());
    write_private_file(room_id, &file)?;
    touch_group_chat_updated_at(room_id)
}

pub fn list_group_chat_private_turns(room_id: &str) -> Result<Vec<GroupChatTurn>, String> {
    Ok(read_private_file(room_id)?.turns)
}

fn append_turn_jsonl(room_id: &str, path: PathBuf, turn: &GroupChatTurn) -> Result<(), String> {
    validate_group_chat_id(room_id)?;
    let group_chat_lock = group_chat_lock(room_id);
    let _guard = group_chat_lock.lock().unwrap_or_else(|e| e.into_inner());
    let dir = group_chat_dir(room_id);
    super::ensure_dir(&dir).map_err(|e| e.to_string())?;
    let line = serde_json::to_string(turn).map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("open group chat timeline: {e}"))?;
    writeln!(file, "{}", line).map_err(|e| format!("write group chat timeline: {e}"))?;
    touch_group_chat_updated_at(room_id)
}

fn touch_group_chat_updated_at(room_id: &str) -> Result<(), String> {
    let mut room = get_group_chat(room_id).ok_or_else(|| format!("GroupChat {} not found", room_id))?;
    room.updated_at = now_iso();
    save_group_chat(&room)
}

fn list_turns_jsonl(room_id: &str, path: PathBuf) -> Result<Vec<GroupChatTurn>, String> {
    validate_group_chat_id(room_id)?;
    let group_chat_lock = group_chat_lock(room_id);
    let _guard = group_chat_lock.lock().unwrap_or_else(|e| e.into_inner());
    if !path.exists() {
        return Ok(vec![]);
    }
    let content = fs::read_to_string(path).map_err(|e| format!("read group chat timeline: {e}"))?;
    // Dedup by turn_id — incremental snapshots from orchestrator share the
    // same id; the last line for each id carries the completed turn state.
    let mut dedup: std::collections::HashMap<String, GroupChatTurn> = std::collections::HashMap::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        if let Ok(turn) = serde_json::from_str::<GroupChatTurn>(line) {
            dedup.insert(turn.id.clone(), turn);
        }
    }
    let mut turns: Vec<GroupChatTurn> = dedup.into_values().collect();
    turns.sort_by_key(|t| t.idx);
    Ok(turns)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PrivateTurnsFile {
    schema_version: u32,
    turns: Vec<GroupChatTurn>,
}

fn read_private_file(room_id: &str) -> Result<PrivateTurnsFile, String> {
    validate_group_chat_id(room_id)?;
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
    validate_group_chat_id(room_id)?;
    let dir = group_chat_dir(room_id);
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

pub fn delete_group_chat(room_id: &str) -> Result<(), String> {
    validate_group_chat_id(room_id)?;
    let group_chat_lock = group_chat_lock(room_id);
    let _guard = group_chat_lock.lock().unwrap_or_else(|e| e.into_inner());
    let dir = group_chat_dir(room_id);
    if !dir.exists() {
        return Err(format!("GroupChat {} not found", room_id));
    }
    fs::remove_dir_all(&dir).map_err(|e| format!("delete group chat: {e}"))
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
    fn group_chat_can_be_created_listed_and_reopened() {
        with_temp_data_dir(|| {
            let room = create_group_chat(
                "Design Review".to_string(),
                Some("D:/work/app".to_string()),
            )
            .unwrap();

            let reopened = get_group_chat(&room.id).unwrap();
            assert_eq!(reopened.name, "Design Review");
            assert_eq!(reopened.cwd.as_deref(), Some("D:/work/app"));

            let rooms = list_group_chats();
            assert_eq!(rooms.len(), 1);
            assert_eq!(rooms[0].id, room.id);
            assert_eq!(rooms[0].participant_count, 0);
        });
    }

    #[test]
    fn group_chat_attaches_existing_run_by_reference() {
        with_temp_data_dir(|| {
            let room = create_group_chat("GroupChat".to_string(), None).unwrap();
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

            let updated = attach_group_chat_run(
                &room.id,
                &run.id,
                Some("Reviewer".to_string()),
                Some("reviewer".to_string()),
                None,
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
            let room = create_group_chat("GroupChat".to_string(), None).unwrap();
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

            attach_group_chat_run(&room.id, &run.id, Some("Old".to_string()), None, None).unwrap();
            let updated = attach_group_chat_run(
                &room.id,
                &run.id,
                Some("Reviewer".to_string()),
                Some("reviewer".to_string()),
                None,
            )
            .unwrap();

            assert_eq!(updated.participants.len(), 1);
            assert_eq!(updated.participants[0].label, "Reviewer");
            assert_eq!(updated.participants[0].role, "reviewer");
        });
    }

    #[test]
    fn delete_group_chat_does_not_delete_referenced_run() {
        with_temp_data_dir(|| {
            let room = create_group_chat("GroupChat".to_string(), None).unwrap();
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
            attach_group_chat_run(&room.id, "run-1", None, None, None).unwrap();

            delete_group_chat(&room.id).unwrap();

            assert!(get_group_chat(&room.id).is_none());
            assert!(crate::storage::runs::get_run("run-1").is_some());
        });
    }

    #[test]
    fn group_chat_memo_updates_summary_preview() {
        with_temp_data_dir(|| {
            let room = create_group_chat("GroupChat".to_string(), None).unwrap();
            update_group_chat_memo(&room.id, "Remember the API boundary.".to_string()).unwrap();

            let reopened = get_group_chat(&room.id).unwrap();
            assert_eq!(reopened.memo, "Remember the API boundary.");
            assert_eq!(
                list_group_chats()[0].memo_preview.as_deref(),
                Some("Remember the API boundary.")
            );
        });
    }

    #[test]
    fn group_chat_lists_timeline() {
        with_temp_data_dir(|| {
            let room = create_group_chat("GroupChat".to_string(), None).unwrap();
            let original_updated_at = room.updated_at.clone();
            std::thread::sleep(std::time::Duration::from_millis(2));

            let turn = crate::group_chat::models::GroupChatTurn {
                id: "turn-1".to_string(),
                idx: 1,
                mode: crate::group_chat::models::GroupChatTurnMode::Fanout,
                user_input: "Compare options".to_string(),
                target_participant_ids: vec!["p1".to_string()],
                responses: vec![crate::group_chat::models::GroupChatResponseRef {
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

            append_group_chat_public_turn(&room.id, &turn).unwrap();
            let turns = list_group_chat_public_turns(&room.id).unwrap();

            assert_eq!(turns, vec![turn]);
            assert_ne!(get_group_chat(&room.id).unwrap().updated_at, original_updated_at);
        });
    }

    #[test]
    fn private_turns_are_stored_separately_from_public_timeline() {
        with_temp_data_dir(|| {
            let room = create_group_chat("GroupChat".to_string(), None).unwrap();
            let private_turn = crate::group_chat::models::GroupChatTurn {
                id: "private-1".to_string(),
                idx: 1,
                mode: crate::group_chat::models::GroupChatTurnMode::Private,
                user_input: "@Reviewer check this privately".to_string(),
                target_participant_ids: vec!["p1".to_string()],
                responses: vec![],
                started_at: "2026-04-30T00:00:00Z".to_string(),
                completed_at: Some("2026-04-30T00:00:01Z".to_string()),
            };

            append_group_chat_private_turn(&room.id, &private_turn).unwrap();

            assert!(list_group_chat_public_turns(&room.id).unwrap().is_empty());
            assert_eq!(list_group_chat_private_turns(&room.id).unwrap(), vec![private_turn]);
        });
    }

}
