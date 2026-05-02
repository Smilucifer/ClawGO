use crate::agent::adapter::ActorSessionMap;
use crate::agent::session_actor::ActorCommand;
use crate::room::adapter::{adapter_for_run, AgentAdapter, TurnOutcomeStatus};
use crate::room::models::{
    ArenaMemoryCandidate, ArenaMemoryKind, ResearchArtifact, ResearchResult, RoomKind,
    RoomParticipant, RoomResponseRef, RoomTurn, RoomTurnMode,
};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverCommand {
    Review { targets: Vec<String>, input: String },
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

pub async fn run_driver_turn(
    room_id: &str,
    message: &str,
    sessions: &ActorSessionMap,
) -> Result<RoomTurn, String> {
    let room_lock = room_orchestration_lock(room_id);
    let _room_guard = room_lock.lock().await;

    let room =
        storage::rooms::get_room(room_id).ok_or_else(|| format!("Room {room_id} not found"))?;
    if room.kind != RoomKind::Driver {
        return Err(format!("Room {room_id} is not a Driver room"));
    }

    let DriverCommand::Review {
        targets: requested_targets,
        input,
    } = parse_driver_command(message)?;
    if input.trim().is_empty() {
        return Err("Review request is required".to_string());
    }

    let public_turns = storage::rooms::list_public_turns(room_id)?;
    let targets = if requested_targets.is_empty() {
        active_copilot_targets(&room.participants, sessions).await
    } else {
        let mut resolved = Vec::with_capacity(requested_targets.len());
        for target in requested_targets {
            let participant = find_participant(&room.participants, &target)
                .ok_or_else(|| format!("Room participant @{target} not found"))?
                .clone();
            if participant.role != "copilot" {
                return Err(format!(
                    "Room participant {} is not a copilot reviewer",
                    participant.label
                ));
            }
            resolved.push(active_target_for_participant(participant, sessions).await?);
        }
        resolved
    };

    if targets.is_empty() {
        return Err("No active copilot participants are available".to_string());
    }

    let arena_dir = storage::data_dir()
        .join("rooms")
        .join(room_id)
        .join(".arena");
    let cwd = room.cwd.as_deref().unwrap_or("(no room cwd)");
    let prompt = build_driver_review_prompt(
        &input,
        &public_turns,
        &room.participants,
        &room.memo,
        cwd,
        &arena_dir.to_string_lossy(),
    );
    storage::rooms::write_driver_arena_files(&room, &public_turns, &input, &prompt)?;

    let started_at = now_iso();
    let mut response_tasks = Vec::with_capacity(targets.len());

    for target in &targets {
        let participant = target.participant.clone();
        let run = storage::runs::get_run(&participant.run_id)
            .ok_or_else(|| format!("Run {} not found", participant.run_id))?;
        let target_prompt = prompt.clone();
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

    let turn = RoomTurn {
        id: uuid::Uuid::new_v4().to_string(),
        idx: public_turns.len() as u64 + 1,
        mode: RoomTurnMode::Review,
        user_input: message.trim().to_string(),
        target_participant_ids: targets
            .iter()
            .map(|target| target.participant.id.clone())
            .collect(),
        responses,
        started_at,
        completed_at: Some(now_iso()),
    };
    storage::rooms::append_public_turn(room_id, &turn)?;

    Ok(turn)
}

pub async fn run_research_turn(
    room_id: &str,
    message: &str,
    sessions: &ActorSessionMap,
) -> Result<RoomTurn, String> {
    let room_lock = room_orchestration_lock(room_id);
    let _room_guard = room_lock.lock().await;

    let room =
        storage::rooms::get_room(room_id).ok_or_else(|| format!("Room {room_id} not found"))?;
    if room.kind != RoomKind::Research {
        return Err(format!("Room {room_id} is not a Research room"));
    }

    let topic = message.trim();
    if topic.is_empty() {
        return Err("Research topic is required".to_string());
    }

    let public_turns = storage::rooms::list_public_turns(room_id)?;
    let targets = active_targets(&room.participants, sessions).await;
    if targets.is_empty() {
        return Err("No active room participants are available".to_string());
    }

    let started_at = now_iso();
    let mut response_tasks = Vec::with_capacity(targets.len());

    for target in &targets {
        let participant = target.participant.clone();
        let run = storage::runs::get_run(&participant.run_id)
            .ok_or_else(|| format!("Run {} not found", participant.run_id))?;
        let target_prompt = build_research_prompt(
            &participant,
            topic,
            &public_turns,
            &room.participants,
            &room.memo,
        );
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

    let completed_at = now_iso();
    let turn = RoomTurn {
        id: uuid::Uuid::new_v4().to_string(),
        idx: public_turns.len() as u64 + 1,
        mode: RoomTurnMode::Research,
        user_input: topic.to_string(),
        target_participant_ids: targets
            .iter()
            .map(|target| target.participant.id.clone())
            .collect(),
        responses,
        started_at,
        completed_at: Some(completed_at.clone()),
    };
    let artifact = build_research_artifact(&room, &turn, topic, &completed_at);
    storage::rooms::write_research_artifact(room_id, &artifact)?;
    storage::rooms::append_public_turn(room_id, &turn)?;

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
    if let Some(rest) = strip_command_word(trimmed, "@debate") {
        return RoundtableCommand::Debate {
            input: rest.trim().to_string(),
        };
    }

    if let Some(rest) = strip_command_word(trimmed, "@summary") {
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

pub fn parse_driver_command(input: &str) -> Result<DriverCommand, String> {
    let trimmed = input.trim();
    let mut rest = strip_command_word(trimmed, "/review")
        .map(str::trim)
        .ok_or_else(|| "Driver rooms require an explicit /review request".to_string())?;
    let mut targets = Vec::new();

    while let Some(after_at) = rest.strip_prefix('@') {
        let end = after_at.find(char::is_whitespace).unwrap_or(after_at.len());
        if end == 0 {
            break;
        }
        targets.push(after_at[..end].to_string());
        rest = after_at[end..].trim_start();
    }

    let input = rest.trim().to_string();

    Ok(DriverCommand::Review { targets, input })
}

fn strip_command_word<'a>(input: &'a str, command: &str) -> Option<&'a str> {
    let rest = input.strip_prefix(command)?;
    if rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace) {
        Some(rest)
    } else {
        None
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

pub fn build_driver_review_prompt(
    request: &str,
    previous_turns: &[RoomTurn],
    participants: &[RoomParticipant],
    room_memo: &str,
    cwd: &str,
    arena_dir: &str,
) -> String {
    let mut body = String::from(
        "You are a copilot reviewer in a Driver room. Work in read-only review mode: inspect the request, reason from the provided context, and do not edit files, run commands, or change project state.",
    );
    body.push_str("\n\nReview request:\n");
    body.push_str(request.trim());
    body.push_str("\n\nStable room/run references:\n");
    body.push_str("- CWD: ");
    body.push_str(cwd);
    body.push('\n');
    body.push_str("- Arena files: ");
    body.push_str(arena_dir);
    body.push('\n');
    for participant in participants {
        body.push_str("- ");
        body.push_str(&participant.label);
        body.push_str(" (");
        body.push_str(&participant.role);
        body.push_str("): participant_id=");
        body.push_str(&participant.id);
        body.push_str(" run_id=");
        body.push_str(&participant.run_id);
        body.push('\n');
    }

    if !room_memo.trim().is_empty() {
        body.push_str("\nRoom memo:\n");
        body.push_str(room_memo.trim());
        body.push('\n');
    }

    body.push_str("\nRecent public room context:\n");
    for turn in previous_turns.iter().rev().take(8).rev() {
        body.push_str("\nUser: ");
        body.push_str(&turn.user_input);
        body.push('\n');
        for response in &turn.responses {
            let label = participant_label(participants, &response.participant_id);
            body.push_str("- ");
            body.push_str(&label);
            body.push_str(" [run_id=");
            body.push_str(&response.run_id);
            body.push_str("]: ");
            body.push_str(response.preview.as_deref().unwrap_or("(no preview)"));
            body.push('\n');
        }
    }

    body.push_str(
        "\nReturn review findings, risks, and concrete suggestions. Do not claim you changed files.",
    );
    body
}

pub fn build_research_prompt(
    target: &RoomParticipant,
    topic: &str,
    previous_turns: &[RoomTurn],
    participants: &[RoomParticipant],
    room_memo: &str,
) -> String {
    let mut body = String::from(
        "You are a researcher in a Research room. Split the shared topic with the other participants and return a structured research result.",
    );
    body.push_str("\n\nResearch topic:\n");
    body.push_str(topic.trim());
    body.push_str("\n\nYour scoped subtask:\n");
    body.push_str("- Participant: ");
    body.push_str(&target.label);
    body.push_str(" (");
    body.push_str(&target.role);
    body.push_str(")\n");
    body.push_str(
        "- Investigate the topic from your own angle. Include concrete findings, evidence, risks, open questions, and recommended next steps.",
    );

    let peer_labels = participants
        .iter()
        .filter(|participant| participant.id != target.id)
        .map(|participant| participant.label.as_str())
        .collect::<Vec<_>>();
    if !peer_labels.is_empty() {
        body.push_str("\n- Avoid duplicating these peers when possible: ");
        body.push_str(&peer_labels.join(", "));
    }

    if !room_memo.trim().is_empty() {
        body.push_str("\n\nRoom memo:\n");
        body.push_str(room_memo.trim());
    }

    body.push_str("\n\nRecent public room context:\n");
    for turn in previous_turns
        .iter()
        .filter(|turn| !matches!(turn.mode, RoomTurnMode::Private))
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
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

    body.push_str(
        "\nReturn markdown with sections: Findings, Evidence, Risks, Open Questions, Next Steps, Arena Memory Candidates.",
    );
    body.push_str(
        "\nIn Arena Memory Candidates, mark durable project knowledge as lines starting with [fact], [decision], or [lesson].",
    );
    body
}

fn build_research_artifact(
    room: &crate::room::models::Room,
    turn: &RoomTurn,
    topic: &str,
    generated_at: &str,
) -> ResearchArtifact {
    ResearchArtifact {
        schema_version: 2,
        room_id: room.id.clone(),
        topic: topic.to_string(),
        turn_id: turn.id.clone(),
        generated_at: generated_at.to_string(),
        results: turn
            .responses
            .iter()
            .map(|response| ResearchResult {
                participant_id: response.participant_id.clone(),
                run_id: response.run_id.clone(),
                label: participant_label(&room.participants, &response.participant_id),
                status: response.status.clone(),
                preview: response.preview.clone(),
                error: response.error.clone(),
            })
            .collect(),
        memory_candidates: extract_arena_memory_candidates(turn, generated_at),
    }
}

fn extract_arena_memory_candidates(
    turn: &RoomTurn,
    generated_at: &str,
) -> Vec<ArenaMemoryCandidate> {
    let mut candidates = Vec::new();
    for response in &turn.responses {
        let text = full_response_text(response).or_else(|| response.preview.clone());
        let Some(text) = text else {
            continue;
        };
        for line in text.lines() {
            let line = line
                .trim()
                .trim_start_matches("- ")
                .trim_start_matches("* ")
                .trim();
            let Some((kind, text)) = parse_memory_candidate_line(line) else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            candidates.push(ArenaMemoryCandidate {
                id: uuid::Uuid::new_v4().to_string(),
                kind,
                text: text.to_string(),
                source_participant_id: response.participant_id.clone(),
                source_run_id: response.run_id.clone(),
                source_turn_id: turn.id.clone(),
                created_at: generated_at.to_string(),
            });
        }
    }
    candidates
}

fn full_response_text(response: &RoomResponseRef) -> Option<String> {
    if response.event_seq_end < response.event_seq_start {
        return None;
    }

    let mut texts = Vec::new();

    for event in crate::storage::events::list_bus_events(
        &response.run_id,
        Some(response.event_seq_start.saturating_sub(1)),
    ) {
        let seq = event.get("_seq").and_then(|v| v.as_u64()).unwrap_or(0);
        if seq > response.event_seq_end {
            continue;
        }
        if event.get("type").and_then(|v| v.as_str()) == Some("message_complete") {
            if let Some(text) = event.get("text").and_then(|v| v.as_str()) {
                if !text.trim().is_empty() {
                    texts.push(text.to_string());
                }
            }
        }
    }

    for event in crate::storage::events::list_events(
        &response.run_id,
        response.event_seq_start.saturating_sub(1),
    ) {
        if event.seq > response.event_seq_end {
            continue;
        }
        if matches!(event.event_type, crate::models::RunEventType::Assistant) {
            if let Some(text) = event.payload.get("text").and_then(|v| v.as_str()) {
                if !text.trim().is_empty() {
                    texts.push(text.to_string());
                }
            }
        }
    }

    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

fn parse_memory_candidate_line(line: &str) -> Option<(ArenaMemoryKind, &str)> {
    let lower = line.to_ascii_lowercase();
    if lower.starts_with("[fact]") {
        return Some((ArenaMemoryKind::Fact, line[6..].trim()));
    }
    if lower.starts_with("[decision]") {
        return Some((ArenaMemoryKind::Decision, line[10..].trim()));
    }
    if lower.starts_with("[lesson]") {
        return Some((ArenaMemoryKind::Lesson, line[8..].trim()));
    }
    None
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

async fn active_copilot_targets(
    participants: &[RoomParticipant],
    sessions: &ActorSessionMap,
) -> Vec<RoundtableTarget> {
    let map = sessions.lock().await;
    participants
        .iter()
        .filter(|participant| participant.role == "copilot")
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
    fn command_words_do_not_capture_similarly_named_private_targets() {
        assert_eq!(
            parse_roundtable_command("@debateAlice check this privately"),
            RoundtableCommand::Private {
                target: "debateAlice".to_string(),
                message: "check this privately".to_string()
            }
        );
        assert_eq!(
            parse_roundtable_command("@summaryBot check this privately"),
            RoundtableCommand::Private {
                target: "summaryBot".to_string(),
                message: "check this privately".to_string()
            }
        );
    }

    #[test]
    fn parses_driver_review_commands() {
        assert_eq!(
            parse_driver_command("/review @Alice @Bob check the patch").unwrap(),
            DriverCommand::Review {
                targets: vec!["Alice".to_string(), "Bob".to_string()],
                input: "check the patch".to_string(),
            }
        );
        assert_eq!(
            parse_driver_command("/review check the patch").unwrap(),
            DriverCommand::Review {
                targets: vec![],
                input: "check the patch".to_string(),
            }
        );
    }

    #[test]
    fn driver_requires_explicit_review_command() {
        assert!(parse_driver_command("check the patch").is_err());
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
    fn driver_review_prompt_includes_readonly_context_and_run_refs() {
        let mut driver = participant("driver", "Driver");
        driver.role = "driver".to_string();
        let mut reviewer = participant("reviewer", "Reviewer");
        reviewer.role = "copilot".to_string();
        let prompt = build_driver_review_prompt(
            "Check the patch",
            &[public_turn()],
            &[driver, reviewer],
            "Room memo",
            "D:/work/app",
            "D:/data/rooms/room-1/.arena",
        );

        assert!(prompt.contains("read-only"));
        assert!(prompt.contains("Check the patch"));
        assert!(prompt.contains("Room memo"));
        assert!(prompt.contains("run-driver"));
        assert!(prompt.contains("run-reviewer"));
        assert!(prompt.contains(".arena"));
        assert!(prompt.contains("Which API should we use?"));
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

    #[test]
    fn driver_review_routes_to_copilots_and_records_review_turn() {
        with_temp_data_dir(|| {
            let room = crate::storage::rooms::create_room_with_kind(
                "Driver Room".into(),
                "Review implementation".into(),
                Some("D:/work/app".to_string()),
                crate::room::models::RoomKind::Driver,
            )
            .unwrap();
            create_run("run-driver");
            create_run("run-a");
            create_run("run-b");
            crate::storage::rooms::attach_run(
                &room.id,
                "run-driver",
                Some("Lead".to_string()),
                Some("driver".to_string()),
            )
            .unwrap();
            crate::storage::rooms::attach_run(
                &room.id,
                "run-a",
                Some("Alice".to_string()),
                Some("copilot".to_string()),
            )
            .unwrap();
            crate::storage::rooms::attach_run(
                &room.id,
                "run-b",
                Some("Bob".to_string()),
                Some("copilot".to_string()),
            )
            .unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let sessions: ActorSessionMap = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
                let (tx_driver, mut rx_driver) = tokio::sync::mpsc::channel(1);
                let (tx_a, mut rx_a) = tokio::sync::mpsc::channel(1);
                let (tx_b, mut rx_b) = tokio::sync::mpsc::channel(1);
                sessions.lock().await.insert(
                    "run-driver".to_string(),
                    actor_handle("run-driver", tx_driver),
                );
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
                    async move {
                        run_driver_turn(&room_id, "/review @Alice patch risk", &sessions).await
                    }
                });

                let alice_prompt = receive_text(&mut rx_a).await;
                let turn = send_task.await.unwrap().unwrap();

                assert!(alice_prompt.contains("read-only"));
                assert!(alice_prompt.contains("patch risk"));
                assert!(rx_driver.try_recv().is_err());
                assert!(rx_b.try_recv().is_err());
                assert_eq!(turn.mode, RoomTurnMode::Review);
                assert_eq!(turn.target_participant_ids.len(), 1);

                let stored = crate::storage::rooms::list_public_turns(&room.id).unwrap();
                assert_eq!(stored.len(), 1);
                assert_eq!(stored[0].mode, RoomTurnMode::Review);

                let arena = crate::storage::data_dir()
                    .join("rooms")
                    .join(&room.id)
                    .join(".arena");
                assert!(arena.join("context.md").exists());
                assert!(arena.join("state.md").exists());
                assert!(arena.join("memory").exists());
            });
        });
    }

    #[test]
    fn research_prompt_frames_participant_scoped_subtask() {
        let mut alice = participant("p1", "Alice");
        alice.role = "researcher".to_string();
        let prompt = build_research_prompt(
            &alice,
            "Compare local vector database options",
            &[public_turn()],
            &[alice.clone(), participant("p2", "Bob")],
            "Prefer local-first tools.",
        );

        assert!(prompt.contains("Research topic"));
        assert!(prompt.contains("Compare local vector database options"));
        assert!(prompt.contains("Alice"));
        assert!(prompt.contains("researcher"));
        assert!(prompt.contains("structured research result"));
        assert!(prompt.contains("Prefer local-first tools."));
        assert!(prompt.contains("Which API should we use?"));
        assert!(prompt.contains("Arena Memory Candidates"));
        assert!(prompt.contains("[fact]"));
        assert!(prompt.contains("[decision]"));
        assert!(prompt.contains("[lesson]"));
    }

    #[test]
    fn research_artifact_extracts_arena_memory_candidates_from_marked_previews() {
        let alice = participant("p1", "Alice");
        let room = crate::room::models::Room {
            id: "room-1".to_string(),
            kind: RoomKind::Research,
            name: "Research Room".to_string(),
            description: String::new(),
            cwd: Some("D:/work/app".to_string()),
            memo: String::new(),
            participants: vec![alice.clone()],
            created_at: "2026-05-02T00:00:00Z".to_string(),
            updated_at: "2026-05-02T00:00:00Z".to_string(),
        };
        let turn = RoomTurn {
            id: "turn-1".to_string(),
            idx: 1,
            mode: RoomTurnMode::Research,
            user_input: "Compare options".to_string(),
            target_participant_ids: vec![alice.id.clone()],
            responses: vec![RoomResponseRef {
                participant_id: alice.id.clone(),
                run_id: alice.run_id.clone(),
                event_seq_start: 1,
                event_seq_end: 2,
                preview: Some(
                    "[fact] SQLite is already embedded.\n- [decision] Prefer local storage first.\n* [lesson] Keep snapshots append-only."
                        .to_string(),
                ),
                status: "complete".to_string(),
                error: None,
            }],
            started_at: "2026-05-02T00:00:00Z".to_string(),
            completed_at: Some("2026-05-02T00:00:01Z".to_string()),
        };

        let artifact =
            build_research_artifact(&room, &turn, "Compare options", "2026-05-02T00:00:01Z");

        assert_eq!(artifact.schema_version, 2);
        assert_eq!(artifact.memory_candidates.len(), 3);
        assert_eq!(artifact.memory_candidates[0].kind, ArenaMemoryKind::Fact);
        assert_eq!(
            artifact.memory_candidates[1].text,
            "Prefer local storage first."
        );
        assert_eq!(
            artifact.memory_candidates[2].source_turn_id,
            "turn-1".to_string()
        );
    }

    #[test]
    fn research_artifact_extracts_memory_candidates_from_full_response_events() {
        with_temp_data_dir(|| {
            create_run("run-p1");
            let alice = participant("p1", "Alice");
            let room = crate::room::models::Room {
                id: "room-1".to_string(),
                kind: RoomKind::Research,
                name: "Research Room".to_string(),
                description: String::new(),
                cwd: Some("D:/work/app".to_string()),
                memo: String::new(),
                participants: vec![alice.clone()],
                created_at: "2026-05-02T00:00:00Z".to_string(),
                updated_at: "2026-05-02T00:00:00Z".to_string(),
            };
            let full_text = format!(
                "{}\n\nArena Memory Candidates\n[fact] SQLite is already embedded.",
                "Introductory research context. ".repeat(8)
            );
            let event = crate::storage::events::append_event(
                &alice.run_id,
                crate::models::RunEventType::Assistant,
                serde_json::json!({ "text": full_text }),
            )
            .unwrap();
            let turn = RoomTurn {
                id: "turn-1".to_string(),
                idx: 1,
                mode: RoomTurnMode::Research,
                user_input: "Compare options".to_string(),
                target_participant_ids: vec![alice.id.clone()],
                responses: vec![RoomResponseRef {
                    participant_id: alice.id.clone(),
                    run_id: alice.run_id.clone(),
                    event_seq_start: event.seq,
                    event_seq_end: event.seq,
                    preview: Some("Introductory research context.".to_string()),
                    status: "complete".to_string(),
                    error: None,
                }],
                started_at: "2026-05-02T00:00:00Z".to_string(),
                completed_at: Some("2026-05-02T00:00:01Z".to_string()),
            };

            let artifact =
                build_research_artifact(&room, &turn, "Compare options", "2026-05-02T00:00:01Z");

            assert_eq!(artifact.memory_candidates.len(), 1);
            assert_eq!(
                artifact.memory_candidates[0].text,
                "SQLite is already embedded."
            );
        });
    }

    #[test]
    fn research_artifact_extracts_memory_candidates_from_bus_message_complete_events() {
        with_temp_data_dir(|| {
            create_run("run-p1");
            let alice = participant("p1", "Alice");
            let writer = crate::storage::events::EventWriter::new();
            crate::storage::events::persist_bus_event(
                &writer,
                &alice.run_id,
                &crate::models::BusEvent::MessageComplete {
                    run_id: alice.run_id.clone(),
                    message_id: "msg-1".to_string(),
                    text: format!(
                        "{}\n\nArena Memory Candidates\n[decision] Keep research artifacts append-only.",
                        "Detailed findings before candidates. ".repeat(8)
                    ),
                    parent_tool_use_id: None,
                    model: None,
                    stop_reason: None,
                    message_usage: None,
                },
            )
            .unwrap();
            let room = crate::room::models::Room {
                id: "room-1".to_string(),
                kind: RoomKind::Research,
                name: "Research Room".to_string(),
                description: String::new(),
                cwd: Some("D:/work/app".to_string()),
                memo: String::new(),
                participants: vec![alice.clone()],
                created_at: "2026-05-02T00:00:00Z".to_string(),
                updated_at: "2026-05-02T00:00:00Z".to_string(),
            };
            let turn = RoomTurn {
                id: "turn-1".to_string(),
                idx: 1,
                mode: RoomTurnMode::Research,
                user_input: "Compare options".to_string(),
                target_participant_ids: vec![alice.id.clone()],
                responses: vec![RoomResponseRef {
                    participant_id: alice.id.clone(),
                    run_id: alice.run_id.clone(),
                    event_seq_start: 1,
                    event_seq_end: 1,
                    preview: Some("Detailed findings before candidates.".to_string()),
                    status: "complete".to_string(),
                    error: None,
                }],
                started_at: "2026-05-02T00:00:00Z".to_string(),
                completed_at: Some("2026-05-02T00:00:01Z".to_string()),
            };

            let artifact =
                build_research_artifact(&room, &turn, "Compare options", "2026-05-02T00:00:01Z");

            assert_eq!(artifact.memory_candidates.len(), 1);
            assert_eq!(
                artifact.memory_candidates[0].kind,
                ArenaMemoryKind::Decision
            );
            assert_eq!(
                artifact.memory_candidates[0].text,
                "Keep research artifacts append-only."
            );
        });
    }

    #[test]
    fn research_turn_fans_out_and_writes_structured_artifact() {
        with_temp_data_dir(|| {
            let room = crate::storage::rooms::create_room_with_kind(
                "Research Room".into(),
                "Compare approaches".into(),
                Some("D:/work/app".to_string()),
                crate::room::models::RoomKind::Research,
            )
            .unwrap();
            crate::storage::rooms::update_memo(&room.id, "Prefer local-first tools.".to_string())
                .unwrap();
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
                    async move {
                        run_research_turn(
                            &room_id,
                            "Compare local vector database options",
                            &sessions,
                        )
                        .await
                    }
                });

                let alice_prompt = receive_text(&mut rx_a).await;
                let bob_prompt = receive_text(&mut rx_b).await;
                let turn = send_task.await.unwrap().unwrap();

                assert!(alice_prompt.contains("Compare local vector database options"));
                assert!(alice_prompt.contains("Alice"));
                assert!(bob_prompt.contains("Compare local vector database options"));
                assert!(bob_prompt.contains("Bob"));
                assert_ne!(alice_prompt, bob_prompt);
                assert_eq!(turn.mode, RoomTurnMode::Research);
                assert_eq!(turn.target_participant_ids.len(), 2);

                let stored = crate::storage::rooms::list_public_turns(&room.id).unwrap();
                assert_eq!(stored.len(), 1);
                assert_eq!(stored[0].mode, RoomTurnMode::Research);

                let artifact = crate::storage::rooms::read_research_artifact(&room.id)
                    .unwrap()
                    .unwrap();
                assert_eq!(artifact.schema_version, 2);
                assert_eq!(artifact.topic, "Compare local vector database options");
                assert_eq!(artifact.turn_id, turn.id);
                assert_eq!(artifact.results.len(), 2);
                assert_eq!(artifact.results[0].label, "Alice");
                assert_eq!(artifact.results[1].label, "Bob");
                assert!(artifact.memory_candidates.is_empty());
                assert_eq!(
                    crate::storage::rooms::list_research_artifacts(&room.id)
                        .unwrap()
                        .len(),
                    1
                );
            });
        });
    }

    #[test]
    fn research_turn_does_not_persist_public_turn_when_artifact_write_fails() {
        with_temp_data_dir(|| {
            let room = crate::storage::rooms::create_room_with_kind(
                "Research Room".into(),
                "Compare approaches".into(),
                Some("D:/work/app".to_string()),
                crate::room::models::RoomKind::Research,
            )
            .unwrap();
            create_run("run-a");
            crate::storage::rooms::attach_run(&room.id, "run-a", Some("Alice".to_string()), None)
                .unwrap();

            let research_path = crate::storage::data_dir()
                .join("rooms")
                .join(&room.id)
                .join("research");
            std::fs::write(&research_path, "block writes").unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let sessions: ActorSessionMap = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
                let (tx_a, mut rx_a) = tokio::sync::mpsc::channel(1);
                sessions
                    .lock()
                    .await
                    .insert("run-a".to_string(), actor_handle("run-a", tx_a));

                let send_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room.id.clone();
                    async move {
                        run_research_turn(&room_id, "Compare local storage options", &sessions)
                            .await
                    }
                });

                let _prompt = receive_text(&mut rx_a).await;
                let error = send_task.await.unwrap().unwrap_err();

                assert!(error.contains("research"));
                assert!(crate::storage::rooms::list_public_turns(&room.id)
                    .unwrap()
                    .is_empty());
            });
        });
    }
}
