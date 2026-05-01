use crate::agent::adapter::ActorSessionMap;
use crate::agent::session_actor::ActorCommand;
use crate::room::adapter::{adapter_for_run, AgentAdapter, TurnOutcomeStatus};
use crate::room::models::{RoomParticipant, RoomResponseRef, RoomTurn, RoomTurnMode};
use crate::{models::now_iso, storage};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, Mutex as AsyncMutex};

static ROOM_ORCHESTRATION_LOCKS: Lazy<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static RUN_ORCHESTRATION_LOCKS: Lazy<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
struct RoundtableTarget {
    participant: RoomParticipant,
    cmd_tx: mpsc::Sender<ActorCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoundtableCommand {
    Fanout { input: String },
    Debate { input: String },
    Summary { target: String },
    Private { target: String, message: String },
}

pub async fn run_roundtable_turn(
    room_id: &str,
    message: &str,
    sessions: &ActorSessionMap,
) -> Result<RoomTurn, String> {
    let room_lock = room_orchestration_lock(room_id);
    let _room_guard = room_lock.lock().await;

    let room =
        storage::rooms::get_room(room_id).ok_or_else(|| format!("Room {room_id} not found"))?;
    let command = parse_roundtable_command(message);
    let public_turns = storage::rooms::list_public_turns(room_id)?;

    let (mode, user_input, prompt, targets, is_private) = match command {
        RoundtableCommand::Fanout { input } => {
            let targets = active_targets(&room.participants, sessions).await;
            (RoomTurnMode::Fanout, input.clone(), input, targets, false)
        }
        RoundtableCommand::Debate { input } => {
            let targets = active_targets(&room.participants, sessions).await;
            (
                RoomTurnMode::Debate,
                message.trim().to_string(),
                input,
                targets,
                false,
            )
        }
        RoundtableCommand::Summary { target } => {
            let participant = find_participant(&room.participants, &target)
                .ok_or_else(|| format!("Room participant @{target} not found"))?
                .clone();
            let target = active_target_for_participant(participant, sessions).await?;
            let prompt = build_summary_prompt(&public_turns, &room.participants);
            (
                RoomTurnMode::Summary,
                message.trim().to_string(),
                prompt,
                vec![target],
                false,
            )
        }
        RoundtableCommand::Private {
            target,
            message: private_message,
        } => {
            let participant = find_participant(&room.participants, &target)
                .ok_or_else(|| format!("Room participant @{target} not found"))?
                .clone();
            let target = active_target_for_participant(participant, sessions).await?;
            (
                RoomTurnMode::Private,
                message.trim().to_string(),
                private_message,
                vec![target],
                true,
            )
        }
    };

    if targets.is_empty() {
        return Err("No active room participants are available".to_string());
    }

    let started_at = now_iso();
    let mut response_tasks = Vec::with_capacity(targets.len());

    for target in &targets {
        let participant = target.participant.clone();
        let run = storage::runs::get_run(&participant.run_id)
            .ok_or_else(|| format!("Run {} not found", participant.run_id))?;
        let target_prompt = if mode == RoomTurnMode::Debate {
            build_debate_prompt(&participant, &prompt, &public_turns, &room.participants)
        } else {
            prompt.clone()
        };
        let mut adapter = adapter_for_run(&run).with_command_sender(target.cmd_tx.clone());

        response_tasks.push(tokio::spawn(async move {
            let run_lock = run_orchestration_lock(&participant.run_id);
            let _run_guard = run_lock.lock().await;
            let event_seq_start = storage::events::next_seq(&participant.run_id);

            match adapter.stream_message(&target_prompt).await {
                Ok(()) => match adapter.wait_turn_complete().await {
                    Ok(outcome) => {
                        let event_seq_end =
                            storage::events::next_seq(&participant.run_id).saturating_sub(1);
                        RoomResponseRef {
                            participant_id: participant.id.clone(),
                            run_id: participant.run_id.clone(),
                            event_seq_start,
                            event_seq_end,
                            preview: run_preview(&participant.run_id),
                            status: outcome_status_label(outcome.status).to_string(),
                            error: outcome.error,
                        }
                    }
                    Err(error) => RoomResponseRef {
                        participant_id: participant.id.clone(),
                        run_id: participant.run_id.clone(),
                        event_seq_start,
                        event_seq_end: event_seq_start.saturating_sub(1),
                        preview: None,
                        status: "failed".to_string(),
                        error: Some(error.message),
                    },
                },
                Err(error) => RoomResponseRef {
                    participant_id: participant.id.clone(),
                    run_id: participant.run_id.clone(),
                    event_seq_start,
                    event_seq_end: event_seq_start.saturating_sub(1),
                    preview: None,
                    status: "failed".to_string(),
                    error: Some(error.message),
                },
            }
        }));
    }

    let mut responses = Vec::with_capacity(response_tasks.len());
    for task in response_tasks {
        responses.push(
            task.await
                .map_err(|e| format!("Room participant task failed: {e}"))?,
        );
    }

    let idx = if is_private {
        storage::rooms::list_private_turns(room_id)?.len() as u64 + 1
    } else {
        public_turns.len() as u64 + 1
    };
    let turn = RoomTurn {
        id: uuid::Uuid::new_v4().to_string(),
        idx,
        mode,
        user_input,
        target_participant_ids: targets
            .iter()
            .map(|target| target.participant.id.clone())
            .collect(),
        responses,
        started_at,
        completed_at: Some(now_iso()),
    };

    if is_private {
        storage::rooms::append_private_turn(room_id, &turn)?;
    } else {
        storage::rooms::append_public_turn(room_id, &turn)?;
    }

    Ok(turn)
}

fn room_orchestration_lock(room_id: &str) -> Arc<AsyncMutex<()>> {
    let mut locks = ROOM_ORCHESTRATION_LOCKS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    locks
        .entry(room_id.to_string())
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

fn run_orchestration_lock(run_id: &str) -> Arc<AsyncMutex<()>> {
    let mut locks = RUN_ORCHESTRATION_LOCKS
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    locks
        .entry(run_id.to_string())
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

pub fn parse_roundtable_command(input: &str) -> RoundtableCommand {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("@debate") {
        return RoundtableCommand::Debate {
            input: rest.trim().to_string(),
        };
    }

    if let Some(rest) = trimmed.strip_prefix("@summary") {
        let target = rest
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_start_matches('@')
            .to_string();
        return RoundtableCommand::Summary { target };
    }

    if let Some(rest) = trimmed.strip_prefix('@') {
        let mut parts = rest.splitn(2, char::is_whitespace);
        let target = parts.next().unwrap_or_default().to_string();
        let message = parts.next().unwrap_or_default().trim().to_string();
        if !target.is_empty() && !message.is_empty() {
            return RoundtableCommand::Private { target, message };
        }
    }

    RoundtableCommand::Fanout {
        input: trimmed.to_string(),
    }
}

pub fn build_debate_prompt(
    target: &RoomParticipant,
    instruction: &str,
    previous_turns: &[RoomTurn],
    participants: &[RoomParticipant],
) -> String {
    let mut body = String::from(
        "Debate the room's previous answers. Challenge assumptions, compare tradeoffs, and respond with your own position.",
    );
    let instruction = instruction.trim();
    if !instruction.is_empty() {
        body.push_str("\n\nFocus:\n");
        body.push_str(instruction);
    }
    body.push_str("\n\nPrevious peer responses:\n");

    for turn in previous_turns
        .iter()
        .filter(|turn| !matches!(turn.mode, RoomTurnMode::Private))
    {
        for response in turn
            .responses
            .iter()
            .filter(|response| response.participant_id != target.id)
        {
            let label = participant_label(participants, &response.participant_id);
            body.push_str("- ");
            body.push_str(&label);
            body.push_str(": ");
            body.push_str(response.preview.as_deref().unwrap_or("(no preview)"));
            body.push('\n');
        }
    }

    body
}

pub fn build_summary_prompt(
    previous_turns: &[RoomTurn],
    participants: &[RoomParticipant],
) -> String {
    let mut body = String::from(
        "Summarize the public room discussion. Capture the main points, disagreements, decisions, and useful next steps.",
    );
    body.push_str("\n\nPublic room history:\n");

    for turn in previous_turns
        .iter()
        .filter(|turn| !matches!(turn.mode, RoomTurnMode::Private))
    {
        body.push_str("\nUser: ");
        body.push_str(&turn.user_input);
        body.push('\n');
        for response in &turn.responses {
            let label = participant_label(participants, &response.participant_id);
            body.push_str("- ");
            body.push_str(&label);
            body.push_str(": ");
            body.push_str(response.preview.as_deref().unwrap_or("(no preview)"));
            body.push('\n');
        }
    }

    body
}

fn participant_label(participants: &[RoomParticipant], participant_id: &str) -> String {
    participants
        .iter()
        .find(|participant| participant.id == participant_id)
        .map(|participant| participant.label.clone())
        .unwrap_or_else(|| participant_id.to_string())
}

async fn active_targets(
    participants: &[RoomParticipant],
    sessions: &ActorSessionMap,
) -> Vec<RoundtableTarget> {
    let map = sessions.lock().await;
    participants
        .iter()
        .filter_map(|participant| {
            map.get(&participant.run_id).map(|handle| RoundtableTarget {
                participant: participant.clone(),
                cmd_tx: handle.cmd_tx.clone(),
            })
        })
        .collect()
}

async fn active_target_for_participant(
    participant: RoomParticipant,
    sessions: &ActorSessionMap,
) -> Result<RoundtableTarget, String> {
    let map = sessions.lock().await;
    let cmd_tx = map
        .get(&participant.run_id)
        .map(|handle| handle.cmd_tx.clone())
        .ok_or_else(|| {
            format!(
                "Room participant {} is not attached to an active session",
                participant.label
            )
        })?;

    Ok(RoundtableTarget {
        participant,
        cmd_tx,
    })
}

fn find_participant<'a>(
    participants: &'a [RoomParticipant],
    target: &str,
) -> Option<&'a RoomParticipant> {
    let normalized = target.trim().trim_start_matches('@').to_ascii_lowercase();
    participants.iter().find(|participant| {
        participant.id.eq_ignore_ascii_case(&normalized)
            || participant.run_id.eq_ignore_ascii_case(&normalized)
            || participant.label.to_ascii_lowercase() == normalized
    })
}

fn outcome_status_label(status: TurnOutcomeStatus) -> &'static str {
    match status {
        TurnOutcomeStatus::Pending => "pending",
        TurnOutcomeStatus::Running => "running",
        TurnOutcomeStatus::Complete => "complete",
        TurnOutcomeStatus::Failed => "failed",
        TurnOutcomeStatus::Stopped => "stopped",
    }
}

fn run_preview(run_id: &str) -> Option<String> {
    crate::commands::runs::get_run(run_id.to_string())
        .ok()
        .and_then(|run| run.last_message_preview)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::adapter::ActorSessionMap;
    use crate::agent::session_actor::{ActorCommand, SessionActorHandle};
    use crate::models::RunStatus;
    use crate::room::models::RoomResponseRef;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn participant(id: &str, label: &str) -> RoomParticipant {
        RoomParticipant {
            id: id.to_string(),
            run_id: format!("run-{id}"),
            agent: "claude".to_string(),
            label: label.to_string(),
            role: "participant".to_string(),
            joined_at: "2026-04-30T00:00:00Z".to_string(),
        }
    }

    fn public_turn() -> RoomTurn {
        RoomTurn {
            id: "turn-1".to_string(),
            idx: 1,
            mode: RoomTurnMode::Fanout,
            user_input: "Which API should we use?".to_string(),
            target_participant_ids: vec!["p1".to_string(), "p2".to_string()],
            responses: vec![
                RoomResponseRef {
                    participant_id: "p1".to_string(),
                    run_id: "run-p1".to_string(),
                    event_seq_start: 1,
                    event_seq_end: 3,
                    preview: Some("Alice answer".to_string()),
                    status: "complete".to_string(),
                    error: None,
                },
                RoomResponseRef {
                    participant_id: "p2".to_string(),
                    run_id: "run-p2".to_string(),
                    event_seq_start: 4,
                    event_seq_end: 6,
                    preview: Some("Bob answer".to_string()),
                    status: "complete".to_string(),
                    error: None,
                },
            ],
            started_at: "2026-04-30T00:00:00Z".to_string(),
            completed_at: Some("2026-04-30T00:00:01Z".to_string()),
        }
    }

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

    fn create_run(id: &str) {
        crate::storage::runs::create_run(
            id,
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
    }

    fn actor_handle(
        run_id: &str,
        cmd_tx: tokio::sync::mpsc::Sender<ActorCommand>,
    ) -> SessionActorHandle {
        let (_shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        SessionActorHandle {
            cmd_tx,
            run_id: run_id.to_string(),
            tag: Arc::new(()),
            join_handle: tokio::spawn(async {}),
            shutdown_rx,
        }
    }

    async fn sessions_for_two_runs(
        run_a: &str,
        run_b: &str,
    ) -> (
        ActorSessionMap,
        tokio::sync::mpsc::Receiver<ActorCommand>,
        tokio::sync::mpsc::Receiver<ActorCommand>,
    ) {
        let sessions: ActorSessionMap = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        let (tx_a, rx_a) = tokio::sync::mpsc::channel(1);
        let (tx_b, rx_b) = tokio::sync::mpsc::channel(1);
        sessions
            .lock()
            .await
            .insert(run_a.to_string(), actor_handle(run_a, tx_a));
        sessions
            .lock()
            .await
            .insert(run_b.to_string(), actor_handle(run_b, tx_b));
        (sessions, rx_a, rx_b)
    }

    async fn receive_text(rx: &mut tokio::sync::mpsc::Receiver<ActorCommand>) -> String {
        match rx.recv().await.unwrap() {
            ActorCommand::SendMessage { text, reply, .. } => {
                reply.send(Ok(())).unwrap();
                text
            }
            _ => panic!("expected SendMessage"),
        }
    }

    #[test]
    fn parses_roundtable_commands() {
        assert_eq!(
            parse_roundtable_command("Compare the APIs"),
            RoundtableCommand::Fanout {
                input: "Compare the APIs".to_string()
            }
        );
        assert_eq!(
            parse_roundtable_command("@debate focus on risks"),
            RoundtableCommand::Debate {
                input: "focus on risks".to_string()
            }
        );
        assert_eq!(
            parse_roundtable_command("@summary @Alice"),
            RoundtableCommand::Summary {
                target: "Alice".to_string()
            }
        );
        assert_eq!(
            parse_roundtable_command("@Alice check this privately"),
            RoundtableCommand::Private {
                target: "Alice".to_string(),
                message: "check this privately".to_string()
            }
        );
    }

    #[test]
    fn debate_prompt_excludes_target_previous_response() {
        let prompt = build_debate_prompt(
            &participant("p1", "Alice"),
            "challenge assumptions",
            &[public_turn()],
            &[participant("p1", "Alice"), participant("p2", "Bob")],
        );

        assert!(prompt.contains("challenge assumptions"));
        assert!(prompt.contains("Bob"));
        assert!(prompt.contains("Bob answer"));
        assert!(!prompt.contains("Alice answer"));
    }

    #[test]
    fn summary_prompt_includes_public_history() {
        let prompt = build_summary_prompt(&[public_turn()], &[participant("p1", "Alice")]);

        assert!(prompt.contains("Which API should we use?"));
        assert!(prompt.contains("Alice"));
        assert!(prompt.contains("Alice answer"));
    }

    #[test]
    fn fanout_sends_same_message_to_active_peers_and_records_public_turn() {
        with_temp_data_dir(|| {
            let room = crate::storage::rooms::create_room("Room".into(), "".into(), None).unwrap();
            create_run("run-a");
            create_run("run-b");
            crate::storage::rooms::attach_run(&room.id, "run-a", Some("Alice".to_string()), None)
                .unwrap();
            crate::storage::rooms::attach_run(&room.id, "run-b", Some("Bob".to_string()), None)
                .unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let sessions: ActorSessionMap = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
                let (tx_a, mut rx_a) = tokio::sync::mpsc::channel(1);
                let (tx_b, mut rx_b) = tokio::sync::mpsc::channel(1);
                sessions
                    .lock()
                    .await
                    .insert("run-a".to_string(), actor_handle("run-a", tx_a));
                sessions
                    .lock()
                    .await
                    .insert("run-b".to_string(), actor_handle("run-b", tx_b));

                let send_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room.id.clone();
                    async move { run_roundtable_turn(&room_id, "Compare options", &sessions).await }
                });

                for rx in [&mut rx_a, &mut rx_b] {
                    match rx.recv().await.unwrap() {
                        ActorCommand::SendMessage { text, reply, .. } => {
                            assert_eq!(text, "Compare options");
                            reply.send(Ok(())).unwrap();
                        }
                        _ => panic!("expected SendMessage"),
                    }
                }

                let turn = send_task.await.unwrap().unwrap();
                assert_eq!(turn.mode, RoomTurnMode::Fanout);
                assert_eq!(turn.responses.len(), 2);

                let stored = crate::storage::rooms::list_public_turns(&room.id).unwrap();
                assert_eq!(stored.len(), 1);
                assert_eq!(stored[0].user_input, "Compare options");
            });
        });
    }

    #[test]
    fn fanout_dispatches_to_all_targets_before_waiting_for_completion() {
        with_temp_data_dir(|| {
            let room = crate::storage::rooms::create_room("Room".into(), "".into(), None).unwrap();
            create_run("run-a");
            create_run("run-b");
            crate::storage::rooms::attach_run(&room.id, "run-a", Some("Alice".to_string()), None)
                .unwrap();
            crate::storage::rooms::attach_run(&room.id, "run-b", Some("Bob".to_string()), None)
                .unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let (sessions, mut rx_a, mut rx_b) = sessions_for_two_runs("run-a", "run-b").await;
                let send_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room.id.clone();
                    async move { run_roundtable_turn(&room_id, "Compare options", &sessions).await }
                });

                let first = rx_a.recv().await.unwrap();
                let second =
                    tokio::time::timeout(std::time::Duration::from_millis(100), rx_b.recv())
                        .await
                        .expect("second participant should receive before first completes")
                        .unwrap();

                match first {
                    ActorCommand::SendMessage { text, reply, .. } => {
                        assert_eq!(text, "Compare options");
                        reply.send(Ok(())).unwrap();
                    }
                    _ => panic!("expected SendMessage"),
                }
                match second {
                    ActorCommand::SendMessage { text, reply, .. } => {
                        assert_eq!(text, "Compare options");
                        reply.send(Ok(())).unwrap();
                    }
                    _ => panic!("expected SendMessage"),
                }

                send_task.await.unwrap().unwrap();
            });
        });
    }

    #[test]
    fn explicit_target_without_active_actor_returns_error_without_turn() {
        with_temp_data_dir(|| {
            let room = crate::storage::rooms::create_room("Room".into(), "".into(), None).unwrap();
            create_run("run-p1");
            crate::storage::rooms::attach_run(&room.id, "run-p1", Some("Alice".to_string()), None)
                .unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let sessions: ActorSessionMap = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
                let error = run_roundtable_turn(&room.id, "@Alice check privately", &sessions)
                    .await
                    .unwrap_err();

                assert!(error.contains("not attached to an active session"));
                assert!(crate::storage::rooms::list_private_turns(&room.id)
                    .unwrap()
                    .is_empty());
            });
        });
    }

    #[test]
    fn concurrent_room_sends_allocate_unique_turn_indexes() {
        with_temp_data_dir(|| {
            let room = crate::storage::rooms::create_room("Room".into(), "".into(), None).unwrap();
            create_run("run-a");
            crate::storage::rooms::attach_run(&room.id, "run-a", Some("Alice".to_string()), None)
                .unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let sessions: ActorSessionMap = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
                let (tx, mut rx) = tokio::sync::mpsc::channel(2);
                sessions
                    .lock()
                    .await
                    .insert("run-a".to_string(), actor_handle("run-a", tx));

                let first_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room.id.clone();
                    async move { run_roundtable_turn(&room_id, "First", &sessions).await }
                });
                let second_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room.id.clone();
                    async move { run_roundtable_turn(&room_id, "Second", &sessions).await }
                });

                tokio::task::yield_now().await;

                let first = rx.recv().await.unwrap();
                let maybe_second =
                    tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv())
                        .await
                        .ok()
                        .flatten();

                match first {
                    ActorCommand::SendMessage { reply, .. } => {
                        reply.send(Ok(())).unwrap();
                    }
                    _ => panic!("expected SendMessage"),
                }
                if let Some(second) = maybe_second {
                    match second {
                        ActorCommand::SendMessage { reply, .. } => {
                            reply.send(Ok(())).unwrap();
                        }
                        _ => panic!("expected SendMessage"),
                    }
                } else {
                    match rx.recv().await.unwrap() {
                        ActorCommand::SendMessage { reply, .. } => {
                            reply.send(Ok(())).unwrap();
                        }
                        _ => panic!("expected SendMessage"),
                    }
                }

                first_task.await.unwrap().unwrap();
                second_task.await.unwrap().unwrap();

                let turns = crate::storage::rooms::list_public_turns(&room.id).unwrap();
                let mut indexes = turns.iter().map(|turn| turn.idx).collect::<Vec<_>>();
                indexes.sort_unstable();
                assert_eq!(indexes, vec![1, 2]);
            });
        });
    }

    #[test]
    fn shared_run_across_rooms_is_not_sent_concurrently() {
        with_temp_data_dir(|| {
            let room_a =
                crate::storage::rooms::create_room("Room A".into(), "".into(), None).unwrap();
            let room_b =
                crate::storage::rooms::create_room("Room B".into(), "".into(), None).unwrap();
            create_run("run-shared");
            crate::storage::rooms::attach_run(
                &room_a.id,
                "run-shared",
                Some("Shared".to_string()),
                None,
            )
            .unwrap();
            crate::storage::rooms::attach_run(
                &room_b.id,
                "run-shared",
                Some("Shared".to_string()),
                None,
            )
            .unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let sessions: ActorSessionMap = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
                let (tx, mut rx) = tokio::sync::mpsc::channel(2);
                sessions
                    .lock()
                    .await
                    .insert("run-shared".to_string(), actor_handle("run-shared", tx));

                let first_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room_a.id.clone();
                    async move { run_roundtable_turn(&room_id, "First", &sessions).await }
                });
                let second_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room_b.id.clone();
                    async move { run_roundtable_turn(&room_id, "Second", &sessions).await }
                });

                let first = rx.recv().await.unwrap();
                let second_before_first_completes =
                    tokio::time::timeout(std::time::Duration::from_millis(50), rx.recv()).await;
                assert!(
                    second_before_first_completes.is_err(),
                    "second room should wait for the shared run turn to finish"
                );

                match first {
                    ActorCommand::SendMessage { reply, .. } => {
                        reply.send(Ok(())).unwrap();
                    }
                    _ => panic!("expected SendMessage"),
                }

                match rx.recv().await.unwrap() {
                    ActorCommand::SendMessage { reply, .. } => {
                        reply.send(Ok(())).unwrap();
                    }
                    _ => panic!("expected SendMessage"),
                }

                first_task.await.unwrap().unwrap();
                second_task.await.unwrap().unwrap();
            });
        });
    }

    #[test]
    fn debate_sends_peer_context_excluding_each_targets_own_response() {
        with_temp_data_dir(|| {
            let room = crate::storage::rooms::create_room("Room".into(), "".into(), None).unwrap();
            create_run("run-p1");
            create_run("run-p2");
            crate::storage::rooms::attach_run(&room.id, "run-p1", Some("Alice".to_string()), None)
                .unwrap();
            crate::storage::rooms::attach_run(&room.id, "run-p2", Some("Bob".to_string()), None)
                .unwrap();
            let room = crate::storage::rooms::get_room(&room.id).unwrap();
            let alice_id = room.participants[0].id.clone();
            let bob_id = room.participants[1].id.clone();
            crate::storage::rooms::append_public_turn(
                &room.id,
                &RoomTurn {
                    id: "turn-1".to_string(),
                    idx: 1,
                    mode: RoomTurnMode::Fanout,
                    user_input: "Which API should we use?".to_string(),
                    target_participant_ids: vec![alice_id.clone(), bob_id.clone()],
                    responses: vec![
                        RoomResponseRef {
                            participant_id: alice_id,
                            run_id: "run-p1".to_string(),
                            event_seq_start: 1,
                            event_seq_end: 2,
                            preview: Some("Alice answer".to_string()),
                            status: "complete".to_string(),
                            error: None,
                        },
                        RoomResponseRef {
                            participant_id: bob_id,
                            run_id: "run-p2".to_string(),
                            event_seq_start: 3,
                            event_seq_end: 4,
                            preview: Some("Bob answer".to_string()),
                            status: "complete".to_string(),
                            error: None,
                        },
                    ],
                    started_at: "2026-04-30T00:00:00Z".to_string(),
                    completed_at: Some("2026-04-30T00:00:01Z".to_string()),
                },
            )
            .unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let (sessions, mut rx_a, mut rx_b) =
                    sessions_for_two_runs("run-p1", "run-p2").await;
                let send_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room.id.clone();
                    async move { run_roundtable_turn(&room_id, "@debate risks", &sessions).await }
                });

                let alice_prompt = receive_text(&mut rx_a).await;
                let bob_prompt = receive_text(&mut rx_b).await;
                send_task.await.unwrap().unwrap();

                assert!(alice_prompt.contains("risks"));
                assert!(alice_prompt.contains("Bob answer"));
                assert!(!alice_prompt.contains("Alice answer"));
                assert!(bob_prompt.contains("Alice answer"));
                assert!(!bob_prompt.contains("Bob answer"));
            });
        });
    }

    #[test]
    fn summary_routes_to_exactly_one_target() {
        with_temp_data_dir(|| {
            let room = crate::storage::rooms::create_room("Room".into(), "".into(), None).unwrap();
            create_run("run-p1");
            create_run("run-p2");
            crate::storage::rooms::attach_run(&room.id, "run-p1", Some("Alice".to_string()), None)
                .unwrap();
            crate::storage::rooms::attach_run(&room.id, "run-p2", Some("Bob".to_string()), None)
                .unwrap();
            crate::storage::rooms::append_public_turn(&room.id, &public_turn()).unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let (sessions, mut rx_a, mut rx_b) =
                    sessions_for_two_runs("run-p1", "run-p2").await;
                let send_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room.id.clone();
                    async move { run_roundtable_turn(&room_id, "@summary @Alice", &sessions).await }
                });

                let summary_prompt = receive_text(&mut rx_a).await;
                let turn = send_task.await.unwrap().unwrap();

                assert!(summary_prompt.contains("Summarize"));
                assert!(summary_prompt.contains("Which API should we use?"));
                assert!(rx_b.try_recv().is_err());
                assert_eq!(turn.mode, RoomTurnMode::Summary);
                assert_eq!(turn.target_participant_ids.len(), 1);
            });
        });
    }

    #[test]
    fn private_message_writes_private_store_only() {
        with_temp_data_dir(|| {
            let room = crate::storage::rooms::create_room("Room".into(), "".into(), None).unwrap();
            create_run("run-p1");
            create_run("run-p2");
            crate::storage::rooms::attach_run(&room.id, "run-p1", Some("Alice".to_string()), None)
                .unwrap();
            crate::storage::rooms::attach_run(&room.id, "run-p2", Some("Bob".to_string()), None)
                .unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let (sessions, mut rx_a, mut rx_b) =
                    sessions_for_two_runs("run-p1", "run-p2").await;
                let send_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room.id.clone();
                    async move {
                        run_roundtable_turn(&room_id, "@Alice check privately", &sessions).await
                    }
                });

                let private_prompt = receive_text(&mut rx_a).await;
                let turn = send_task.await.unwrap().unwrap();

                assert_eq!(private_prompt, "check privately");
                assert!(rx_b.try_recv().is_err());
                assert_eq!(turn.mode, RoomTurnMode::Private);
                assert!(crate::storage::rooms::list_public_turns(&room.id)
                    .unwrap()
                    .is_empty());
                assert_eq!(
                    crate::storage::rooms::list_private_turns(&room.id)
                        .unwrap()
                        .len(),
                    1
                );
            });
        });
    }
}
