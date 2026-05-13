use crate::agent::adapter::ActorSessionMap;
use crate::agent::session_actor::ActorCommand;
use crate::agent::spawn::build_agent_command;
use crate::agent::stream::{run_agent, ProcessMap};
use crate::agent::windows_msvc_env::{
    merge_extra_env_into_spawn_env_plan, resolve_spawn_env_plan_with_policy, MsvcPolicy,
    SpawnPathPolicy,
};
use crate::group_chat::adapter::{
    adapter_for_run, can_use_group_chat_actor_run, AgentAdapter, AgentCapabilities, TurnOutcomeStatus,
};
use crate::group_chat::context::{check_handoff, record_participant_turn, HandoffDecision};
use crate::group_chat::memory_injection::{search_memories_for_injection, format_memory_injection};
use crate::group_chat::models::{
    GroupChatParticipant, GroupChatResponseRef, GroupChatTurn, GroupChatTurnMode,
};
use crate::{
    models::{now_iso, ExecutionPath, RunEventType, RunMeta, RunStatus},
    storage,
};
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use tokio::sync::{mpsc, Mutex as AsyncMutex};
use tokio_util::sync::CancellationToken;

static GROUP_CHAT_ORCHESTRATION_LOCKS: Lazy<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static RUN_ORCHESTRATION_LOCKS: Lazy<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
const MAX_DEBATE_OPINION_CHARS: usize = 5_000;
const MAX_DEBATE_OPINION_KEEP: usize = 2_000;
const MAX_SUMMARY_PER_VIEW_CHARS: usize = 3_000;
const MAX_AUTO_CHAIN_HOPS: usize = 3;

#[derive(Clone)]
struct GroupChatTarget {
    participant: GroupChatParticipant,
    runtime: GroupChatTargetRuntime,
}

#[derive(Clone)]
enum GroupChatTargetRuntime {
    Actor { cmd_tx: mpsc::Sender<ActorCommand> },
    Pipe,
}

impl GroupChatTarget {
    #[cfg(test)]
    fn is_pipe(&self) -> bool {
        matches!(self.runtime, GroupChatTargetRuntime::Pipe)
    }
}

#[derive(Clone)]
pub struct GroupChatPipeRuntime {
    pub app: AppHandle,
    pub process_map: ProcessMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupChatCommand {
    Fanout { input: String },
    Debate { input: String },
    Summary { target: String },
    Private { target: String, message: String },
    SingleTarget { target: String, message: String },
}

pub async fn run_group_chat_turn(
    room_id: &str,
    message: &str,
    sessions: &ActorSessionMap,
) -> Result<GroupChatTurn, String> {
    run_group_chat_turn_with_runtime(room_id, message, sessions, None, None).await
}

/// Inner execution: spawn participant tasks, collect responses, save incremental turns.
/// Extracted so the caller can write a terminal failed turn on ANY error path.
#[allow(clippy::too_many_arguments)]
async fn execute_turn_inner(
    room_id: &str,
    turn_id: String,
    idx: u64,
    mode: &GroupChatTurnMode,
    user_input: &str,
    participant_ids: &[String],
    targets: &[GroupChatTarget],
    pipe_runtime: &Option<GroupChatPipeRuntime>,
    turn_num: u64,
    prompt: &str,
    public_turns: &[GroupChatTurn],
    participants: &[GroupChatParticipant],
    started_at: &str,
    is_private: bool,
) -> Result<GroupChatTurn, String> {
    let mut join_set = tokio::task::JoinSet::new();
    let user_msg = user_input.to_string();
    for target in targets {
        let target = target.clone();
        let participant = target.participant.clone();
        let run = storage::runs::get_run(&participant.run_id)
            .ok_or_else(|| format!("Run {} not found", participant.run_id))?;
        let target_prompt = match mode {
            GroupChatTurnMode::Fanout => build_fanout_prompt(turn_num, prompt, None),
            GroupChatTurnMode::Debate => {
                build_debate_prompt(&participant, prompt, public_turns, participants)
            }
            _ => prompt.to_string(),
        };
        let pipe_runtime = pipe_runtime.clone();

        join_set.spawn({
            let user_msg = user_msg.clone();
            async move {
                let run_lock = run_orchestration_lock(&participant.run_id);
                let _run_guard = run_lock.lock().await;
                execute_group_chat_target(
                    target,
                    &participant,
                    &run,
                    &target_prompt,
                    &user_msg,
                    pipe_runtime,
                )
                .await
            }
        });
    }

    let mut responses: Vec<GroupChatResponseRef> = Vec::with_capacity(targets.len());
    while let Some(result) = join_set.join_next().await {
        let response = result.map_err(|e| format!("GroupChat participant task failed: {e}"))?;
        responses.push(response);

        let incremental_turn = GroupChatTurn {
            id: turn_id.clone(),
            idx,
            mode: mode.clone(),
            user_input: user_input.to_string(),
            target_participant_ids: participant_ids.to_vec(),
            responses: responses.clone(),
            started_at: started_at.to_string(),
            completed_at: None,
        };
        if is_private {
            storage::group_chats::append_group_chat_private_turn(room_id, &incremental_turn)?;
        } else {
            storage::group_chats::append_group_chat_public_turn(room_id, &incremental_turn)?;
        }
    }

    let turn = GroupChatTurn {
        id: turn_id,
        idx,
        mode: mode.clone(),
        user_input: user_input.to_string(),
        target_participant_ids: participant_ids.to_vec(),
        responses,
        started_at: started_at.to_string(),
        completed_at: Some(now_iso()),
    };

    if is_private {
        storage::group_chats::append_group_chat_private_turn(room_id, &turn)?;
    } else {
        storage::group_chats::append_group_chat_public_turn(room_id, &turn)?;
    }

    Ok(turn)
}

pub async fn run_group_chat_turn_with_runtime(
    room_id: &str,
    message: &str,
    sessions: &ActorSessionMap,
    pipe_runtime: Option<GroupChatPipeRuntime>,
    cancel_token: Option<CancellationToken>,
) -> Result<GroupChatTurn, String> {
    let room_lock = group_chat_orchestration_lock(room_id);
    let _room_guard = room_lock.lock().await;

    let room =
        storage::group_chats::get_group_chat(room_id).ok_or_else(|| format!("GroupChat {room_id} not found"))?;
    let command = parse_group_chat_command(message);
    let public_turns = storage::group_chats::list_group_chat_public_turns(room_id)?;

    let (mode, user_input, prompt, targets, is_private) = match command {
        GroupChatCommand::Fanout { input } => {
            let targets = active_targets(&room.participants, sessions).await;
            (GroupChatTurnMode::Fanout, input.clone(), input, targets, false)
        }
        GroupChatCommand::Debate { input } => {
            let targets = active_targets(&room.participants, sessions).await;
            (
                GroupChatTurnMode::Debate,
                message.trim().to_string(),
                input,
                targets,
                false,
            )
        }
        GroupChatCommand::Summary { target } => {
            let participant = find_participant(&room.participants, &target)
                .ok_or_else(|| format!("GroupChat participant @{target} not found"))?
                .clone();
            let target = active_target_for_participant(participant, sessions).await?;
            let prompt = build_summary_prompt(&public_turns, &room.participants);
            (
                GroupChatTurnMode::Summary,
                message.trim().to_string(),
                prompt,
                vec![target],
                false,
            )
        }
        GroupChatCommand::Private {
            target,
            message: private_message,
        } => {
            let participant = find_participant_unique(&room.participants, &target)?.clone();
            let target = active_target_for_participant(participant, sessions).await?;
            (
                GroupChatTurnMode::Private,
                message.trim().to_string(),
                private_message,
                vec![target],
                true,
            )
        }
        GroupChatCommand::SingleTarget {
            target,
            message: target_message,
        } => {
            let participant = find_participant_unique(&room.participants, &target)?.clone();
            let target_ref = active_target_for_participant(participant.clone(), sessions).await?;
            let prompt = build_singletarget_prompt(
                public_turns.len() as u64 + 1,
                &participant.label,
                &target_message,
            );
            (
                GroupChatTurnMode::SingleTarget,
                message.trim().to_string(),
                prompt,
                vec![target_ref],
                false,
            )
        }
    };

    if targets.is_empty() {
        return Err("No active group chat participants are available".to_string());
    }

    let started_at = now_iso();
    let idx = if is_private {
        storage::group_chats::list_group_chat_private_turns(room_id)?.len() as u64 + 1
    } else {
        public_turns.len() as u64 + 1
    };
    let turn_id = uuid::Uuid::new_v4().to_string();
    let participant_ids: Vec<String> = targets
        .iter()
        .map(|target| target.participant.id.clone())
        .collect();

    let turn_num = public_turns.len() as u64 + 1;

    // Pre-write empty turn so user message appears immediately via polling.
    let initial_turn = GroupChatTurn {
        id: turn_id.clone(),
        idx,
        mode: mode.clone(),
        user_input: user_input.clone(),
        target_participant_ids: participant_ids.clone(),
        responses: vec![],
        started_at: started_at.clone(),
        completed_at: None,
    };
    if is_private {
        storage::group_chats::append_group_chat_private_turn(room_id, &initial_turn)?;
    } else {
        storage::group_chats::append_group_chat_public_turn(room_id, &initial_turn)?;
    }

    // Execute in a helper so ALL errors route through the failed-turn cleanup below.
    let exec_result = execute_turn_inner(
        room_id,
        turn_id.clone(),
        idx,
        &mode,
        &user_input,
        &participant_ids,
        &targets,
        &pipe_runtime,
        turn_num,
        &prompt,
        &public_turns,
        &room.participants,
        &started_at,
        is_private,
    )
    .await;

    match exec_result {
        Ok(turn) => {
            // ── Session handoff trigger ──
            let public_turns_after = if is_private {
                storage::group_chats::list_group_chat_public_turns(room_id)?
            } else {
                let mut turns = storage::group_chats::list_group_chat_public_turns(room_id)?;
                if turns.last().map(|t| t.id.as_str()) != Some(&turn.id) {
                    turns.push(turn.clone());
                }
                turns
            };
            for resp in &turn.responses {
                let _ = record_participant_turn(room_id, &resp.participant_id);
                if let Some(participant) = room.participants.iter().find(|p| p.id == resp.participant_id) {
                    match check_handoff(&room, participant, &public_turns_after) {
                        HandoffDecision::Handoff { bootstrap_context } => {
                            log::info!(
                                "[group-chat] handoff triggered for participant {} (session_seq={}): context length={}",
                                participant.label,
                                crate::group_chat::context::load_participant_meta(room_id, &participant.id).session_seq,
                                bootstrap_context.len(),
                            );
                            let _ = crate::group_chat::context::reset_session_after_handoff(
                                room_id,
                                &participant.id,
                            );
                        }
                        HandoffDecision::Continue => {}
                    }
                }
            }

            // ── Auto-chain: after SingleTarget, scan response for @mentions ──
            if room.auto_chain && matches!(turn.mode, GroupChatTurnMode::SingleTarget) {
                let mut chained_ids: HashSet<String> =
                    turn.target_participant_ids.iter().cloned().collect();
                let mut current_turn = turn.clone();
                let mut hop = 0usize;

                while hop < MAX_AUTO_CHAIN_HOPS {
                    if let Some(ref ct) = cancel_token {
                        if ct.is_cancelled() {
                            log::info!("[group-chat] auto-chain cancelled at hop {hop}");
                            break;
                        }
                    }

                    let Some(mention) = extract_first_mention(
                        &current_turn,
                        &room.participants,
                        &chained_ids,
                    ) else {
                        break;
                    };

                    let participant = match find_participant_unique(&room.participants, &mention) {
                        Ok(p) => p.clone(),
                        Err(e) => {
                            log::debug!("[group-chat] auto-chain: {e}");
                            break;
                        }
                    };

                    let target = match active_target_for_participant(participant.clone(), sessions).await {
                        Ok(t) => t,
                        Err(e) => {
                            log::debug!("[group-chat] auto-chain: no active target for {}: {e}", participant.label);
                            break;
                        }
                    };

                    hop += 1;
                    chained_ids.insert(participant.id.clone());

                    let auto_prompt = build_singletarget_prompt(
                        public_turns_after.len() as u64 + hop as u64,
                        &participant.label,
                        current_turn
                            .responses
                            .first()
                            .and_then(|r| r.preview.as_deref())
                            .unwrap_or("(empty)"),
                    );

                    let auto_started = now_iso();
                    let auto_turn_id = uuid::Uuid::new_v4().to_string();
                    let auto_idx = public_turns_after.len() as u64 + hop as u64;

                    let run_lock = run_orchestration_lock(&participant.run_id);
                    let _run_guard = run_lock.lock().await;
                    let run = match storage::runs::get_run(&participant.run_id) {
                        Some(r) => r,
                        None => break,
                    };

                    let response =
                        execute_group_chat_target(
                            target,
                            &participant,
                            &run,
                            &auto_prompt,
                            &user_input,
                            pipe_runtime.clone(),
                        )
                        .await;

                    let auto_turn = GroupChatTurn {
                        id: auto_turn_id,
                        idx: auto_idx,
                        mode: GroupChatTurnMode::SingleTarget,
                        user_input: format!("[auto-chain hop {hop}]"),
                        target_participant_ids: vec![participant.id.clone()],
                        responses: vec![response],
                        started_at: auto_started,
                        completed_at: Some(now_iso()),
                    };
                    if let Err(e) = storage::group_chats::append_group_chat_public_turn(room_id, &auto_turn) {
                        log::warn!("[group-chat] auto-chain: failed to persist turn: {e}");
                        break;
                    }

                    log::info!(
                        "[group-chat] auto-chain hop {hop}: @{} → {}",
                        mention,
                        participant.label,
                    );

                    current_turn = auto_turn;
                }
            }

            Ok(turn)
        }
        Err(e) => {
            // Write a terminal failed turn so the frontend doesn't show
            // an orphaned empty turn with bouncing dots forever.
            let failed_turn = GroupChatTurn {
                id: turn_id,
                idx,
                mode,
                user_input,
                target_participant_ids: participant_ids,
                responses: vec![],
                started_at,
                completed_at: Some(now_iso()),
            };
            let _ = if is_private {
                storage::group_chats::append_group_chat_private_turn(room_id, &failed_turn)
            } else {
                storage::group_chats::append_group_chat_public_turn(room_id, &failed_turn)
            };
            Err(e)
        }
    }
}

async fn execute_group_chat_target(
    target: GroupChatTarget,
    participant: &GroupChatParticipant,
    run: &RunMeta,
    target_prompt: &str,
    user_message: &str,
    pipe_runtime: Option<GroupChatPipeRuntime>,
) -> GroupChatResponseRef {
    match target.runtime {
        GroupChatTargetRuntime::Actor { cmd_tx } => {
            execute_actor_turn(participant, run, target_prompt, user_message, cmd_tx).await
        }
        GroupChatTargetRuntime::Pipe => match pipe_runtime {
            Some(runtime) => {
                execute_pipe_turn(
                    participant,
                    run,
                    target_prompt,
                    user_message,
                    runtime.app,
                    runtime.process_map,
                )
                .await
            }
            None => failed_response(
                participant,
                storage::events::next_seq(&participant.run_id),
                "Native CLI GroupChat participant runtime is unavailable",
            ),
        },
    }
}

async fn execute_actor_turn(
    participant: &GroupChatParticipant,
    run: &RunMeta,
    target_prompt: &str,
    user_message: &str,
    cmd_tx: mpsc::Sender<ActorCommand>,
) -> GroupChatResponseRef {
    let event_seq_start = storage::events::next_seq(&participant.run_id);
    let mut adapter = adapter_for_run(run).with_command_sender(cmd_tx);

    // Resolve role system prompt for the Actor path
    let user_settings = storage::settings::get_user_settings();
    let mut system_prompt =
        match resolve_participant_system_prompt(participant, &user_settings.ai_characters) {
            Some(role_prompt) if !role_prompt.is_empty() => role_prompt,
            _ => String::new(),
        };

    // Inject character memory after the role prompt
    if !participant.character_id.is_empty() && participant.character_id != "__orphan__" {
        let (memories, _tier) = search_memories_for_injection(
            &participant.character_id,
            user_message,
            5,
            0.6,
            1,
        )
        .await;
        if !memories.is_empty() {
            let memory_prompt = format_memory_injection(&memories, 2000, 300);
            if !memory_prompt.is_empty() {
                if !system_prompt.is_empty() {
                    system_prompt = format!("{}\n\n{}", system_prompt, memory_prompt);
                } else {
                    system_prompt = memory_prompt;
                }
            }
        }
    }

    let full_prompt = if !system_prompt.is_empty() {
        format!("{}\n\n---\n\n{}", system_prompt, target_prompt)
    } else {
        target_prompt.to_string()
    };

    // Transition to Running so wait_turn_complete does not mistake a
    // pre-existing Idle status (from a previous turn) for an immediate
    // completion.  The session actor also calls persist_idle_running later,
    // but that call is a no-op when the status is already Running.
    let _ = storage::runs::update_status(&participant.run_id, RunStatus::Running, None, None);
    match adapter.stream_message(&full_prompt).await {
        Ok(()) => match adapter.wait_turn_complete().await {
            Ok(outcome) => {
                let event_seq_end =
                    storage::events::next_seq(&participant.run_id).saturating_sub(1);
                GroupChatResponseRef {
                    participant_id: participant.id.clone(),
                    run_id: participant.run_id.clone(),
                    event_seq_start,
                    event_seq_end,
                    preview: run_preview(&participant.run_id),
                    status: outcome_status_label(outcome.status).to_string(),
                    error: outcome.error,
                }
            }
            Err(error) => failed_response(participant, event_seq_start, error.message),
        },
        Err(error) => {
            // Revert Running status if the message could not be delivered
            let _ = storage::runs::update_status(
                &participant.run_id,
                RunStatus::Failed,
                Some(1),
                Some(error.message.clone()),
            );
            failed_response(participant, event_seq_start, error.message)
        }
    }
}

async fn execute_pipe_turn(
    participant: &GroupChatParticipant,
    run: &RunMeta,
    target_prompt: &str,
    user_message: &str,
    app: AppHandle,
    process_map: ProcessMap,
) -> GroupChatResponseRef {
    let event_seq_start = storage::events::next_seq(&participant.run_id);
    let full_prompt = compose_pipe_turn_prompt(&run.prompt, target_prompt);
    if let Err(e) = storage::events::append_event(
        &participant.run_id,
        RunEventType::User,
        serde_json::json!({
            "text": target_prompt,
            "source": "room",
            "participant_id": participant.id
        }),
    ) {
        log::warn!("[room] failed to append pipe user event: {}", e);
    }

    if let Err(e) =
        storage::runs::update_status(&participant.run_id, RunStatus::Running, None, None)
    {
        return failed_response(participant, event_seq_start, e);
    }

    let agent_settings = storage::settings::get_agent_settings(&run.agent);
    let user_settings = storage::settings::get_user_settings();
    let mut adapter_settings = crate::agent::adapter::build_adapter_settings(
        &agent_settings,
        &user_settings,
        run.model.clone(),
    );
    // GroupChat sessions run in plan/read-only mode.
    adapter_settings.permission_mode = Some("plan".to_string());

    // Resolve role-based system prompt from the participant's linked character
    // and inject character memory.
    let mut pipe_system_prompt = resolve_participant_system_prompt(
        participant,
        &user_settings.ai_characters,
    )
    .unwrap_or_default();

    if !participant.character_id.is_empty() && participant.character_id != "__orphan__" {
        let (memories, _tier) = search_memories_for_injection(
            &participant.character_id,
            user_message,
            5,
            0.6,
            1,
        )
        .await;
        if !memories.is_empty() {
            let memory_prompt = format_memory_injection(&memories, 2000, 300);
            if !memory_prompt.is_empty() {
                if !pipe_system_prompt.is_empty() {
                    pipe_system_prompt =
                        format!("{}\n\n{}", pipe_system_prompt, memory_prompt);
                } else {
                    pipe_system_prompt = memory_prompt;
                }
            }
        }
    }

    if !pipe_system_prompt.is_empty() {
        adapter_settings.append_system_prompt = Some(pipe_system_prompt);
    }

    let profile = match storage::settings::find_connection_profile(
        &user_settings,
        &run.agent,
        run.connection_profile_id.as_deref(),
    ) {
        Ok(profile) => profile,
        Err(e) => {
            let _ = storage::runs::update_status(
                &participant.run_id,
                RunStatus::Failed,
                Some(1),
                Some(e.clone()),
            );
            return failed_response(participant, event_seq_start, e);
        }
    };
    let profile_env = if let Some(profile) = profile.as_ref() {
        crate::agent::adapter::apply_connection_profile(
            &mut adapter_settings,
            profile,
            run.model.is_some(),
        )
    } else {
        std::collections::HashMap::new()
    };
    let (command, args) =
        match build_agent_command(&run.agent, &full_prompt, &adapter_settings, true) {
            Ok(command) => command,
            Err(e) => {
                let _ = storage::runs::update_status(
                    &participant.run_id,
                    RunStatus::Failed,
                    Some(1),
                    Some(e.clone()),
                );
                return failed_response(participant, event_seq_start, e);
            }
        };

    let inherited_path = crate::agent::claude_stream::augmented_path();
    let mut spawn_env_plan = resolve_spawn_env_plan_with_policy(
        Path::new(&run.cwd),
        false,
        user_settings.windows_msvc_env_mode,
        SpawnPathPolicy::AlwaysUseAugmentedPath,
        Some(&inherited_path),
        MsvcPolicy::Disabled,
    );
    if !profile_env.is_empty() {
        merge_extra_env_into_spawn_env_plan(&mut spawn_env_plan, &profile_env);
    }

    if let Err(e) = run_agent(
        app.clone(),
        process_map,
        participant.run_id.clone(),
        command,
        args,
        run.cwd.clone(),
        run.agent.clone(),
        spawn_env_plan,
    )
    .await
    {
        let _ = storage::runs::update_status(
            &participant.run_id,
            RunStatus::Failed,
            Some(1),
            Some(e.clone()),
        );
        return failed_response(participant, event_seq_start, e);
    }

    let event_seq_end = storage::events::next_seq(&participant.run_id).saturating_sub(1);
    let completed_run = storage::runs::get_run(&participant.run_id);
    GroupChatResponseRef {
        participant_id: participant.id.clone(),
        run_id: participant.run_id.clone(),
        event_seq_start,
        event_seq_end,
        preview: run_preview(&participant.run_id),
        status: completed_run
            .as_ref()
            .map(|run| run_status_label(run.status.clone()).to_string())
            .unwrap_or_else(|| "complete".to_string()),
        error: completed_run.and_then(|run| run.error_message),
    }
}

/// Build a system prompt for a participant based on their role type and optional custom instruction.
fn build_role_system_prompt(role_type: &str, role_instruction: &Option<String>) -> String {
    let base = match role_type {
        "planner" => concat!(
            "You are a strategic planner in a multi-agent group chat. Your responsibilities:\n\n",
            "1. TASK DECOMPOSITION: Break complex user requests into concrete, ordered sub-tasks.\n",
            "2. CONTEXT ANALYSIS: Read relevant project files and search the web to understand the codebase before planning.\n",
            "3. COORDINATION: Assign tasks to appropriate participants. Use @mentions to route subtasks.\n",
            "4. PLAN OUTPUT: Produce a numbered checklist with clear success criteria for each item.\n\n",
            "CONSTRAINTS:\n",
            "- You can READ files, search code, and search the web, but you CANNOT modify the filesystem, run commands, or execute tools that change state.\n",
            "- Do NOT implement - only plan. The executors will carry out your instructions.\n",
            "- When uncertain about the codebase, request more context rather than guessing.\n\n",
            "IMPORTANT: You are in a group chat context. Do NOT initiate any work, analyze files, or execute tools until the user sends an explicit message to this group chat. Wait for instructions. Your first response should only acknowledge readiness.\n\n",
            "OUTPUT CONSTRAINT: Your response must be NO MORE THAN 300 Chinese characters or 500 English words. Be concise. Use bullet points when listing tasks. Omit explanations.",
        ),
        "executor" => concat!(
            "You are a task executor in a multi-agent group chat. Your responsibilities:\n\n",
            "1. FOLLOW THE PLAN: Execute only the tasks assigned to you. Do not deviate from the plan.\n",
            "2. REPORT PROGRESS: Clearly state which task you completed and what the result was.\n",
            "3. ASK FOR CLARIFICATION: If a task is ambiguous, ask the planner for clarification before executing.\n",
            "4. SIGNAL COMPLETION: End your response with a brief summary of what was accomplished.\n\n",
            "CONSTRAINTS:\n",
            "- Stay within the scope of your assigned task. Do not expand or reinterpret the plan.\n",
            "- If you encounter an obstacle, report it rather than working around it silently.\n",
            "- Coordinate with other executors - reference their outputs when relevant.\n\n",
            "IMPORTANT: You are in a group chat context. Do NOT initiate any work until the planner assigns you a task. Wait for instructions.",
        ),
        _ => "",
    };
    let custom = role_instruction.as_deref().unwrap_or("");
    if custom.is_empty() {
        base.to_string()
    } else {
        format!("{}\n\n{}", base, custom)
    }
}

/// Look up the AiCharacter linked via participant.character_id,
/// then build a role system prompt from its role_type and role_instruction.
/// ID-based lookup — no label fallback.
fn resolve_participant_system_prompt(
    participant: &GroupChatParticipant,
    ai_characters: &[crate::models::AiCharacter],
) -> Option<String> {
    if participant.character_id.is_empty() || participant.character_id == "__orphan__" {
        return None;
    }
    let character = ai_characters
        .iter()
        .find(|c| c.id == participant.character_id)?;
    let prompt = build_role_system_prompt(&character.role_type, &character.role_instruction);
    if prompt.is_empty() {
        None
    } else {
        Some(prompt)
    }
}

fn compose_pipe_turn_prompt(base_prompt: &str, target_prompt: &str) -> String {
    let base = base_prompt.trim();
    let target = target_prompt.trim();
    if base.is_empty() {
        target.to_string()
    } else if target.is_empty() {
        base.to_string()
    } else {
        format!("{base}\n\n{target}")
    }
}

fn failed_response(
    participant: &GroupChatParticipant,
    event_seq_start: u64,
    error: impl Into<String>,
) -> GroupChatResponseRef {
    GroupChatResponseRef {
        participant_id: participant.id.clone(),
        run_id: participant.run_id.clone(),
        event_seq_start,
        event_seq_end: event_seq_start.saturating_sub(1),
        preview: None,
        status: "failed".to_string(),
        error: Some(error.into()),
    }
}

fn group_chat_orchestration_lock(room_id: &str) -> Arc<AsyncMutex<()>> {
    let mut locks = GROUP_CHAT_ORCHESTRATION_LOCKS
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

pub fn parse_group_chat_command(input: &str) -> GroupChatCommand {
    let trimmed = input.trim();
    if let Some(rest) = strip_command_word(trimmed, "@debate") {
        return GroupChatCommand::Debate {
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
        return GroupChatCommand::Summary { target };
    }

    // /dm @Name msg → Private turn (written to private.json)
    if let Some(rest) = strip_command_word(trimmed, "/dm") {
        if let Some(at_rest) = rest.strip_prefix('@') {
            let mut parts = at_rest.splitn(2, char::is_whitespace);
            let target = parts.next().unwrap_or_default().to_string();
            let message = parts.next().unwrap_or_default().trim().to_string();
            if !target.is_empty() && !message.is_empty() {
                return GroupChatCommand::Private { target, message };
            }
        }
    }

    // @Name msg → SingleTarget public turn (only named participant answers)
    if let Some(rest) = trimmed.strip_prefix('@') {
        let mut parts = rest.splitn(2, char::is_whitespace);
        let target = parts.next().unwrap_or_default().to_string();
        let message = parts.next().unwrap_or_default().trim().to_string();
        if !target.is_empty() && !message.is_empty() {
            return GroupChatCommand::SingleTarget { target, message };
        }
    }

    GroupChatCommand::Fanout {
        input: trimmed.to_string(),
    }
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
    target: &GroupChatParticipant,
    instruction: &str,
    previous_turns: &[GroupChatTurn],
    participants: &[GroupChatParticipant],
) -> String {
    let turn_num = previous_turns
        .iter()
        .filter(|turn| !matches!(turn.mode, GroupChatTurnMode::Private))
        .count() as u64
        + 1;
    let mut body = format!("[通用圆桌 · 第 {turn_num} 轮 · @debate]\n");
    let instruction = instruction.trim();
    if !instruction.is_empty() {
        body.push_str("\n## 用户在本轮补充的新信息\n");
        body.push_str(instruction);
        body.push('\n');
    }
    body.push_str("\n## 另两家上一轮观点\n");

    let Some(last_turn) = previous_turns
        .iter()
        .rev()
        .find(|turn| matches!(turn.mode, GroupChatTurnMode::Fanout | GroupChatTurnMode::Debate))
    else {
        body.push_str("(无另两家上轮记录)\n");
        body.push_str("\n## 你的任务\n请基于用户问题发表新观点。\n");
        return body;
    };

    let mut appended = 0usize;
    let mut seen = std::collections::HashSet::new();
    for response in &last_turn.responses {
        seen.insert(response.participant_id.as_str());
        if response.participant_id == target.id {
            continue;
        }
        let label = participant_label(participants, &response.participant_id);
        append_group_chat_view(
            &mut body,
            &label,
            response.preview.as_deref(),
            &response.status,
            response.error.as_deref(),
            MAX_DEBATE_OPINION_CHARS,
            DebateViewStyle::MiddleEllipsis,
        );
        appended += 1;
    }
    for participant_id in &last_turn.target_participant_ids {
        if participant_id == &target.id || seen.contains(participant_id.as_str()) {
            continue;
        }
        let label = participant_label(participants, participant_id);
        body.push_str("\n### ");
        body.push_str(&label);
        body.push_str(" 的观点\n");
        body.push('（');
        body.push_str(&label);
        body.push_str(" 本轮因故未参与，请勿引用）\n");
        appended += 1;
    }
    if appended == 0 {
        body.push_str("(无另两家上轮记录)\n");
    }

    body.push_str("\n## 你的任务\n");
    body.push_str("请基于另两家观点 + 用户补充信息，发表新观点：可以继承、可以反驳，但要明示引用对方哪一点。\n");
    body
}

pub fn build_fanout_prompt(
    turn_num: u64,
    user_input: &str,
    data_pack: Option<&str>,
) -> String {
    let mut body = format!("[通用圆桌 · 第 {turn_num} 轮 · 默认提问]\n");
    if let Some(data_pack) = data_pack.map(str::trim).filter(|value| !value.is_empty()) {
        body.push_str("\n## 数据接入\n");
        body.push_str(data_pack);
        body.push('\n');
    }
    body.push_str("\n## 用户问题\n");
    body.push_str(user_input.trim());
    body.push_str("\n\n请独立回答（你看不到另两家观点，本色发挥即可）。");
    body
}

pub fn build_singletarget_prompt(turn_num: u64, target_label: &str, user_message: &str) -> String {
    format!(
        "[通用圆桌 · 第 {turn_num} 轮 · @single-target → {target_label}]\n\n\
         ## 用户指名提问\n\
         {}\n\n\
         你是本轮唯一被指名回答的参与者。请给出你的完整观点。",
        user_message.trim()
    )
}

pub fn build_summary_prompt(
    previous_turns: &[GroupChatTurn],
    participants: &[GroupChatParticipant],
) -> String {
    let public_turns = previous_turns
        .iter()
        .filter(|turn| !matches!(turn.mode, GroupChatTurnMode::Private))
        .collect::<Vec<_>>();
    let turn_num = public_turns.len() as u64 + 1;
    let mut body = format!("[通用圆桌 · 第 {turn_num} 轮 · @summary]\n\n");
    body.push_str("## 你的任务\n");
    body.push_str("请直接基于圆桌上下文给出最终意见，不需要逐轮复述。\n\n");
    body.push_str("输出格式建议：\n");
    body.push_str("  1) 结论先行（推荐 / 不推荐 / 中性 / 观望，附简短理由）\n");
    body.push_str("  2) 三方共识与关键分歧\n");
    body.push_str("  3) 具体行动建议（按讨论话题自适应）\n");
    body.push_str("\nPublic room history:\n");

    for turn in public_turns {
        body.push_str("\nUser: ");
        body.push_str(&turn.user_input);
        body.push('\n');
        for response in &turn.responses {
            let label = participant_label(participants, &response.participant_id);
            append_group_chat_view(
                &mut body,
                &label,
                response.preview.as_deref(),
                &response.status,
                response.error.as_deref(),
                MAX_SUMMARY_PER_VIEW_CHARS,
                DebateViewStyle::TailEllipsis,
            );
        }
    }

    body
}

enum DebateViewStyle {
    MiddleEllipsis,
    TailEllipsis,
}

fn append_group_chat_view(
    body: &mut String,
    label: &str,
    preview: Option<&str>,
    status: &str,
    error: Option<&str>,
    max_chars: usize,
    style: DebateViewStyle,
) {
    body.push_str("\n### ");
    body.push_str(label);
    body.push_str(" 的观点\n");
    if is_error_status(status) {
        body.push('（');
        body.push_str(label);
        body.push_str(" 本轮发生错误未输出，请勿引用");
        if let Some(error) = error.map(str::trim).filter(|value| !value.is_empty()) {
            body.push_str("：");
            body.push_str(error);
        }
        body.push_str("）\n");
        return;
    }

    let text = preview.unwrap_or("(无输出)");
    body.push_str(&truncate_group_chat_text(text, max_chars, style));
    body.push('\n');
}

fn is_error_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "failed" | "error" | "errored"
    )
}

fn truncate_group_chat_text(text: &str, max_chars: usize, style: DebateViewStyle) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    match style {
        DebateViewStyle::MiddleEllipsis => {
            let head = text
                .chars()
                .take(MAX_DEBATE_OPINION_KEEP)
                .collect::<String>();
            let tail = text
                .chars()
                .rev()
                .take(MAX_DEBATE_OPINION_KEEP)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<String>();
            format!("{head}\n\n…[中段已省略以控制 prompt 长度]…\n\n{tail}")
        }
        DebateViewStyle::TailEllipsis => {
            let head = text.chars().take(max_chars).collect::<String>();
            format!("{head}…[已截断]")
        }
    }
}

fn participant_label(participants: &[GroupChatParticipant], participant_id: &str) -> String {
    participants
        .iter()
        .find(|participant| participant.id == participant_id)
        .map(|participant| participant.label.clone())
        .unwrap_or_else(|| participant_id.to_string())
}

async fn active_targets(
    participants: &[GroupChatParticipant],
    sessions: &ActorSessionMap,
) -> Vec<GroupChatTarget> {
    let map = sessions.lock().await;
    participants
        .iter()
        .filter_map(|participant| {
            let run = storage::runs::get_run(&participant.run_id)?;
            let capabilities = AgentCapabilities::for_agent(&run.agent);
            if capabilities.stream_session && can_use_group_chat_actor_run(&run) {
                return map.get(&participant.run_id).map(|handle| GroupChatTarget {
                    participant: participant.clone(),
                    runtime: GroupChatTargetRuntime::Actor {
                        cmd_tx: handle.cmd_tx.clone(),
                    },
                });
            }
            if capabilities.pipe_exec
                && matches!(run.resolved_execution_path(), ExecutionPath::PipeExec)
            {
                return Some(GroupChatTarget {
                    participant: participant.clone(),
                    runtime: GroupChatTargetRuntime::Pipe,
                });
            }
            None
        })
        .collect()
}

async fn active_target_for_participant(
    participant: GroupChatParticipant,
    sessions: &ActorSessionMap,
) -> Result<GroupChatTarget, String> {
    let run = storage::runs::get_run(&participant.run_id)
        .ok_or_else(|| format!("Run {} not found", participant.run_id))?;
    let capabilities = AgentCapabilities::for_agent(&run.agent);
    if capabilities.stream_session && can_use_group_chat_actor_run(&run) {
        let map = sessions.lock().await;
        let cmd_tx = map
            .get(&participant.run_id)
            .map(|handle| handle.cmd_tx.clone())
            .ok_or_else(|| {
                format!(
                    "GroupChat participant {} is not attached to an active session",
                    participant.label
                )
            })?;
        return Ok(GroupChatTarget {
            participant,
            runtime: GroupChatTargetRuntime::Actor { cmd_tx },
        });
    }
    if capabilities.pipe_exec && matches!(run.resolved_execution_path(), ExecutionPath::PipeExec) {
        return Ok(GroupChatTarget {
            participant,
            runtime: GroupChatTargetRuntime::Pipe,
        });
    }

    Err(format!(
        "GroupChat participant {} uses agent '{}' which does not support this GroupChat execution path",
        participant.label, participant.agent
    ))
}

fn find_participant<'a>(
    participants: &'a [GroupChatParticipant],
    target: &str,
) -> Option<&'a GroupChatParticipant> {
    let normalized = target.trim().trim_start_matches('@').to_ascii_lowercase();
    participants.iter().find(|participant| {
        participant.id.eq_ignore_ascii_case(&normalized)
            || participant.run_id.eq_ignore_ascii_case(&normalized)
            || participant.label.to_ascii_lowercase() == normalized
    })
}

/// Like `find_participant` but returns an error if multiple participants match by label.
fn find_participant_unique<'a>(
    participants: &'a [GroupChatParticipant],
    target: &str,
) -> Result<&'a GroupChatParticipant, String> {
    let normalized = target.trim().trim_start_matches('@').to_ascii_lowercase();
    let matches: Vec<&GroupChatParticipant> = participants
        .iter()
        .filter(|participant| {
            participant.id.eq_ignore_ascii_case(&normalized)
                || participant.run_id.eq_ignore_ascii_case(&normalized)
                || participant.label.to_ascii_lowercase() == normalized
        })
        .collect();
    match matches.len() {
        0 => Err(format!("GroupChat participant @{target} not found")),
        1 => Ok(matches[0]),
        _ => Err(format!(
            "Ambiguous participant name '{target}' — please use a unique label"
        )),
    }
}

/// Scan a turn's response previews for `@Label` mentions of participants
/// that haven't already been chained to. Returns the first valid mention.
fn extract_first_mention(
    turn: &GroupChatTurn,
    participants: &[GroupChatParticipant],
    exclude_ids: &HashSet<String>,
) -> Option<String> {
    for response in &turn.responses {
        let text = response.preview.as_deref()?;
        for word in text.split_whitespace() {
            let candidate = word.trim_start_matches('@');
            if candidate == word || candidate.is_empty() {
                continue;
            }
            let normalized = candidate.to_ascii_lowercase();
            let matched = participants.iter().find(|p| {
                p.label.to_ascii_lowercase() == normalized
                    && !exclude_ids.contains(&p.id)
            });
            if let Some(participant) = matched {
                return Some(participant.label.clone());
            }
        }
    }
    None
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

fn run_status_label(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Pending => "pending",
        RunStatus::Running => "running",
        RunStatus::Idle | RunStatus::Completed => "complete",
        RunStatus::Failed => "failed",
        RunStatus::Stopped => "stopped",
    }
}

fn run_preview(run_id: &str) -> Option<String> {
    // Pass 1: RunEvent format (append_event via native PTY for Codex).
    let run_events = crate::storage::events::list_events(run_id, 0);
    let assistant_count = run_events
        .iter()
        .filter(|e| matches!(e.event_type, crate::models::RunEventType::Assistant))
        .count();
    log::debug!(
        "[room] run_preview pass1: run_id={}, total_events={}, assistant_events={}",
        run_id,
        run_events.len(),
        assistant_count
    );
    if let Some(text) = run_events.into_iter().rev().find_map(|event| {
        if !matches!(event.event_type, crate::models::RunEventType::Assistant) {
            return None;
        }
        // Skip thinking and tool_use subtypes — only capture final text responses
        let subtype = event.payload.get("subtype").and_then(|v| v.as_str()).unwrap_or("");
        if subtype == "thinking" || subtype == "tool_use" {
            return None;
        }
        event
            .payload
            .get("text")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToString::to_string)
    }) {
        log::debug!(
            "[room] run_preview pass1 hit: run_id={}, len={}",
            run_id,
            text.len()
        );
        return Some(text);
    }

    // Pass 2: bus envelope format (SessionActor writes {"_bus":true,"event":{...}}).
    let events_path = crate::storage::run_dir(run_id).join("events.jsonl");
    if let Ok(content) = std::fs::read_to_string(&events_path) {
        let mut bus_delta_count = 0u32;
        for line in content.lines().rev() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if v.get("_bus").and_then(|b| b.as_bool()) != Some(true) {
                continue;
            }
            let Some(event) = v.get("event") else {
                continue;
            };
            let etype = event.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if etype == "message_delta" || etype == "message_complete" {
                bus_delta_count += 1;
                if let Some(text) = event
                    .get("text")
                    .and_then(|t| t.as_str())
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                {
                    log::debug!(
                        "[room] run_preview pass2 hit: run_id={}, len={}, scanned_deltas={}",
                        run_id,
                        text.len(),
                        bus_delta_count
                    );
                    return Some(text.to_string());
                }
            }
        }
        log::debug!(
            "[room] run_preview pass2 miss: run_id={}, bus_deltas_scanned={}, file_exists=true",
            run_id,
            bus_delta_count
        );
    } else {
        log::debug!(
            "[room] run_preview pass2 miss: run_id={}, events.jsonl not found",
            run_id
        );
    }

    // Pass 3: last_message_preview fallback.
    log::debug!(
        "[room] run_preview pass3 fallback: run_id={}, checking last_message_preview",
        run_id
    );
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
    use crate::group_chat::models::GroupChatResponseRef;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn participant(id: &str, label: &str) -> GroupChatParticipant {
        GroupChatParticipant {
            id: id.to_string(),
            run_id: format!("run-{id}"),
            agent: "claude".to_string(),
            label: label.to_string(),
            role: "participant".to_string(),
            character_id: String::new(),
            joined_at: "2026-04-30T00:00:00Z".to_string(),
        }
    }

    fn public_turn() -> GroupChatTurn {
        GroupChatTurn {
            id: "turn-1".to_string(),
            idx: 1,
            mode: GroupChatTurnMode::Fanout,
            user_input: "Which API should we use?".to_string(),
            target_participant_ids: vec!["p1".to_string(), "p2".to_string()],
            responses: vec![
                GroupChatResponseRef {
                    participant_id: "p1".to_string(),
                    run_id: "run-p1".to_string(),
                    event_seq_start: 1,
                    event_seq_end: 3,
                    preview: Some("Alice answer".to_string()),
                    status: "complete".to_string(),
                    error: None,
                },
                GroupChatResponseRef {
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
        let previous = std::env::var_os("CLAW_GO_DATA_DIR");
        std::env::set_var("CLAW_GO_DATA_DIR", tmp.path());
        let result = f();
        match previous {
            Some(value) => std::env::set_var("CLAW_GO_DATA_DIR", value),
            None => std::env::remove_var("CLAW_GO_DATA_DIR"),
        }
        result
    }

    fn create_run(id: &str) {
        create_run_for_agent(id, "claude");
    }

    fn create_run_for_agent(id: &str, agent: &str) {
        crate::storage::runs::create_run(
            id,
            "hello",
            "D:/work/app",
            agent,
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

    fn create_run_with_execution_path(id: &str, execution_path: crate::models::ExecutionPath) {
        let mut run = crate::storage::runs::create_run(
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
        run.execution_path = Some(execution_path);
        crate::storage::runs::save_meta(&run).unwrap();
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
    fn active_targets_require_stream_session_capability() {
        with_temp_data_dir(|| {
            create_run("run-claude");
            create_run_for_agent("run-codex", "codex");
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let claude = participant("claude", "Claude");
                let mut codex = participant("codex", "Codex");
                codex.agent = "codex".to_string();

                let sessions: ActorSessionMap = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
                let (claude_tx, _claude_rx) = tokio::sync::mpsc::channel(1);
                let (codex_tx, _codex_rx) = tokio::sync::mpsc::channel(1);
                sessions.lock().await.insert(
                    claude.run_id.clone(),
                    actor_handle(&claude.run_id, claude_tx),
                );
                sessions
                    .lock()
                    .await
                    .insert(codex.run_id.clone(), actor_handle(&codex.run_id, codex_tx));

                let targets = active_targets(&[claude, codex], &sessions).await;

                assert_eq!(targets.len(), 1);
                assert_eq!(targets[0].participant.agent, "claude");
            });
        });
    }

    #[test]
    fn active_targets_require_session_actor_execution_path() {
        with_temp_data_dir(|| {
            create_run_with_execution_path(
                "run-session",
                crate::models::ExecutionPath::SessionActor,
            );
            create_run_with_execution_path("run-pipe", crate::models::ExecutionPath::PipeExec);
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let mut session = participant("session", "Session");
                session.run_id = "run-session".to_string();
                let mut pipe = participant("pipe", "Pipe");
                pipe.run_id = "run-pipe".to_string();

                let sessions: ActorSessionMap = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
                let (session_tx, _session_rx) = tokio::sync::mpsc::channel(1);
                let (pipe_tx, _pipe_rx) = tokio::sync::mpsc::channel(1);
                sessions.lock().await.insert(
                    session.run_id.clone(),
                    actor_handle(&session.run_id, session_tx),
                );
                sessions
                    .lock()
                    .await
                    .insert(pipe.run_id.clone(), actor_handle(&pipe.run_id, pipe_tx));

                let targets = active_targets(&[session, pipe], &sessions).await;

                assert_eq!(targets.len(), 1);
                assert_eq!(targets[0].participant.run_id, "run-session");
            });
        });
    }

    #[test]
    fn active_targets_include_pipe_exec_runs_without_actor_session() {
        with_temp_data_dir(|| {
            create_run_for_agent("run-codex", "codex");
            let mut run = crate::storage::runs::get_run("run-codex").unwrap();
            run.execution_path = Some(crate::models::ExecutionPath::PipeExec);
            crate::storage::runs::save_meta(&run).unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let mut pipe = participant("pipe", "Pipe");
                pipe.run_id = "run-codex".to_string();
                pipe.agent = "codex".to_string();

                let sessions: ActorSessionMap = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
                let targets = active_targets(&[pipe], &sessions).await;

                assert_eq!(targets.len(), 1);
                assert!(targets[0].is_pipe());
            });
        });
    }

    #[test]
    fn parses_group_chat_commands() {
        assert_eq!(
            parse_group_chat_command("Compare the APIs"),
            GroupChatCommand::Fanout {
                input: "Compare the APIs".to_string()
            }
        );
        assert_eq!(
            parse_group_chat_command("@debate focus on risks"),
            GroupChatCommand::Debate {
                input: "focus on risks".to_string()
            }
        );
        assert_eq!(
            parse_group_chat_command("@summary @Alice"),
            GroupChatCommand::Summary {
                target: "Alice".to_string()
            }
        );
        assert_eq!(
            parse_group_chat_command("@Alice check this"),
            GroupChatCommand::SingleTarget {
                target: "Alice".to_string(),
                message: "check this".to_string()
            }
        );
        assert_eq!(
            parse_group_chat_command("/dm @Alice check this privately"),
            GroupChatCommand::Private {
                target: "Alice".to_string(),
                message: "check this privately".to_string()
            }
        );
    }

    #[test]
    fn command_words_do_not_capture_similarly_named_singletargets() {
        assert_eq!(
            parse_group_chat_command("@debateAlice check this"),
            GroupChatCommand::SingleTarget {
                target: "debateAlice".to_string(),
                message: "check this".to_string()
            }
        );
        assert_eq!(
            parse_group_chat_command("@summaryBot check this"),
            GroupChatCommand::SingleTarget {
                target: "summaryBot".to_string(),
                message: "check this".to_string()
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
    fn fanout_prompt_uses_group_chat_header_and_independent_instruction() {
        let prompt = build_fanout_prompt(2, "Compare the APIs", None);

        assert!(prompt.contains("第 2 轮"));
        assert!(prompt.contains("## 用户问题"));
        assert!(prompt.contains("Compare the APIs"));
        assert!(prompt.contains("请独立回答"));
    }

    #[test]
    fn debate_prompt_marks_absent_and_errored_peers_and_truncates_long_outputs() {
        let mut turn = public_turn();
        turn.target_participant_ids = vec![
            "p1".to_string(),
            "p2".to_string(),
            "p3".to_string(),
            "p4".to_string(),
        ];
        turn.responses[1].preview = Some("x".repeat(5_200));
        turn.responses.push(GroupChatResponseRef {
            participant_id: "p3".to_string(),
            run_id: "run-p3".to_string(),
            event_seq_start: 7,
            event_seq_end: 7,
            preview: None,
            status: "failed".to_string(),
            error: Some("tool crashed".to_string()),
        });

        let prompt = build_debate_prompt(
            &participant("p1", "Alice"),
            "",
            &[turn],
            &[
                participant("p1", "Alice"),
                participant("p2", "Bob"),
                participant("p3", "Cara"),
                participant("p4", "Dana"),
            ],
        );

        assert!(prompt.contains("## 另两家上一轮观点"));
        assert!(prompt.contains("Bob 的观点"));
        assert!(prompt.contains("中段已省略以控制 prompt 长度"));
        assert!(prompt.contains("Cara 本轮发生错误未输出"));
        assert!(prompt.contains("Dana 本轮因故未参与"));
        assert!(!prompt.contains("Alice answer"));
    }

    #[test]
    fn debate_prompt_uses_latest_fanout_or_debate_turn_not_summary() {
        let fanout = public_turn();
        let summary = GroupChatTurn {
            id: "turn-summary".to_string(),
            idx: 2,
            mode: GroupChatTurnMode::Summary,
            user_input: "@summary @Alice".to_string(),
            target_participant_ids: vec!["p1".to_string()],
            responses: vec![GroupChatResponseRef {
                participant_id: "p1".to_string(),
                run_id: "run-p1".to_string(),
                event_seq_start: 7,
                event_seq_end: 8,
                preview: Some("Alice summary should not be debated".to_string()),
                status: "complete".to_string(),
                error: None,
            }],
            started_at: "2026-04-30T00:00:02Z".to_string(),
            completed_at: Some("2026-04-30T00:00:03Z".to_string()),
        };

        let prompt = build_debate_prompt(
            &participant("p1", "Alice"),
            "",
            &[fanout, summary],
            &[participant("p1", "Alice"), participant("p2", "Bob")],
        );

        assert!(prompt.contains("Bob answer"));
        assert!(!prompt.contains("Alice summary should not be debated"));
    }

    #[test]
    fn summary_prompt_includes_public_history() {
        let prompt = build_summary_prompt(&[public_turn()], &[participant("p1", "Alice")]);

        assert!(prompt.contains("Which API should we use?"));
        assert!(prompt.contains("Alice"));
        assert!(prompt.contains("Alice answer"));
        assert!(prompt.contains("结论先行"));
        assert!(prompt.contains("三方共识与关键分歧"));
    }

    #[test]
    fn fanout_sends_same_message_to_active_peers_and_records_public_turn() {
        with_temp_data_dir(|| {
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();
            create_run("run-a");
            create_run("run-b");
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-a", Some("Alice".to_string()), None)
                .unwrap();
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-b", Some("Bob".to_string()), None)
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
                    async move { run_group_chat_turn(&room_id, "Compare options", &sessions).await }
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
                assert_eq!(turn.mode, GroupChatTurnMode::Fanout);
                assert_eq!(turn.responses.len(), 2);

                let stored = crate::storage::group_chats::list_group_chat_public_turns(&room.id).unwrap();
                assert_eq!(stored.len(), 1);
                assert_eq!(stored[0].user_input, "Compare options");
            });
        });
    }

    #[test]
    fn fanout_dispatches_to_all_targets_before_waiting_for_completion() {
        with_temp_data_dir(|| {
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();
            create_run("run-a");
            create_run("run-b");
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-a", Some("Alice".to_string()), None)
                .unwrap();
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-b", Some("Bob".to_string()), None)
                .unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let (sessions, mut rx_a, mut rx_b) = sessions_for_two_runs("run-a", "run-b").await;
                let send_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room.id.clone();
                    async move { run_group_chat_turn(&room_id, "Compare options", &sessions).await }
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
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();
            create_run("run-p1");
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-p1", Some("Alice".to_string()), None)
                .unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let sessions: ActorSessionMap = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
                let error = run_group_chat_turn(&room.id, "@Alice check privately", &sessions)
                    .await
                    .unwrap_err();

                assert!(error.contains("not attached to an active session"));
                assert!(crate::storage::group_chats::list_group_chat_private_turns(&room.id)
                    .unwrap()
                    .is_empty());
            });
        });
    }

    #[test]
    fn concurrent_room_sends_allocate_unique_turn_indexes() {
        with_temp_data_dir(|| {
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();
            create_run("run-a");
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-a", Some("Alice".to_string()), None)
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
                    async move { run_group_chat_turn(&room_id, "First", &sessions).await }
                });
                let second_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room.id.clone();
                    async move { run_group_chat_turn(&room_id, "Second", &sessions).await }
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

                let turns = crate::storage::group_chats::list_group_chat_public_turns(&room.id).unwrap();
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
                crate::storage::group_chats::create_group_chat("Room A".into(), None).unwrap();
            let room_b =
                crate::storage::group_chats::create_group_chat("Room B".into(), None).unwrap();
            create_run("run-shared");
            crate::storage::group_chats::attach_group_chat_run(
                &room_a.id,
                "run-shared",
                Some("Shared".to_string()),
                None,
            )
            .unwrap();
            crate::storage::group_chats::attach_group_chat_run(
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
                    async move { run_group_chat_turn(&room_id, "First", &sessions).await }
                });
                let second_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room_b.id.clone();
                    async move { run_group_chat_turn(&room_id, "Second", &sessions).await }
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
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();
            create_run("run-p1");
            create_run("run-p2");
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-p1", Some("Alice".to_string()), None)
                .unwrap();
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-p2", Some("Bob".to_string()), None)
                .unwrap();
            let room = crate::storage::group_chats::get_group_chat(&room.id).unwrap();
            let alice_id = room.participants[0].id.clone();
            let bob_id = room.participants[1].id.clone();
            crate::storage::group_chats::append_group_chat_public_turn(
                &room.id,
                &GroupChatTurn {
                    id: "turn-1".to_string(),
                    idx: 1,
                    mode: GroupChatTurnMode::Fanout,
                    user_input: "Which API should we use?".to_string(),
                    target_participant_ids: vec![alice_id.clone(), bob_id.clone()],
                    responses: vec![
                        GroupChatResponseRef {
                            participant_id: alice_id,
                            run_id: "run-p1".to_string(),
                            event_seq_start: 1,
                            event_seq_end: 2,
                            preview: Some("Alice answer".to_string()),
                            status: "complete".to_string(),
                            error: None,
                        },
                        GroupChatResponseRef {
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
                    async move { run_group_chat_turn(&room_id, "@debate risks", &sessions).await }
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
    fn actor_turn_propagates_stopped_and_failed_terminal_statuses() {
        with_temp_data_dir(|| {
            create_run("run-p1");

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                for (status, expected_status, expected_error, exit_code) in [
                    (
                        RunStatus::Stopped,
                        "stopped",
                        Some("Stopped by user".to_string()),
                        -1,
                    ),
                    (
                        RunStatus::Failed,
                        "failed",
                        Some("Timed out waiting for Codex transcript completion".to_string()),
                        1,
                    ),
                ] {
                    crate::storage::runs::update_status(
                        "run-p1",
                        status.clone(),
                        Some(exit_code),
                        expected_error.clone(),
                    )
                    .unwrap();

                    let sessions: ActorSessionMap =
                        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
                    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
                    sessions
                        .lock()
                        .await
                        .insert("run-p1".to_string(), actor_handle("run-p1", tx));
                    let run = crate::storage::runs::get_run("run-p1").unwrap();
                    let cmd_tx = sessions
                        .lock()
                        .await
                        .get("run-p1")
                        .expect("session handle")
                        .cmd_tx
                        .clone();
                    let participant = participant("p1", "Alice");

                    let send_task = tokio::spawn({
                        let participant = participant.clone();
                        async move {
                            execute_actor_turn(
                                &participant,
                                &run,
                                "check status",
                                "check status",
                                cmd_tx,
                            )
                            .await
                        }
                    });
                    let prompt = receive_text(&mut rx).await;
                    let response = send_task.await.unwrap();

                    assert_eq!(prompt, "check status");
                    assert_eq!(response.status, expected_status);
                    assert_eq!(response.error, expected_error);
                }
            });
        });
    }

    #[test]
    fn failed_response_marks_room_response_failed_with_error() {
        let participant = participant("p1", "Alice");
        let response = failed_response(
            &participant,
            7,
            "Timed out waiting for Codex transcript completion",
        );

        assert_eq!(response.participant_id, participant.id);
        assert_eq!(response.run_id, participant.run_id);
        assert_eq!(response.event_seq_start, 7);
        assert_eq!(response.event_seq_end, 6);
        assert_eq!(response.status, "failed");
        assert_eq!(
            response.error,
            Some("Timed out waiting for Codex transcript completion".to_string())
        );
        assert_eq!(response.preview, None);
    }

    #[test]
    fn summary_routes_to_exactly_one_target() {
        with_temp_data_dir(|| {
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();
            create_run("run-p1");
            create_run("run-p2");
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-p1", Some("Alice".to_string()), None)
                .unwrap();
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-p2", Some("Bob".to_string()), None)
                .unwrap();
            crate::storage::group_chats::append_group_chat_public_turn(&room.id, &public_turn()).unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let (sessions, mut rx_a, mut rx_b) =
                    sessions_for_two_runs("run-p1", "run-p2").await;
                let send_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room.id.clone();
                    async move { run_group_chat_turn(&room_id, "@summary @Alice", &sessions).await }
                });

                let summary_prompt = receive_text(&mut rx_a).await;
                let turn = send_task.await.unwrap().unwrap();

                assert!(summary_prompt.contains("Summarize"));
                assert!(summary_prompt.contains("Which API should we use?"));
                assert!(rx_b.try_recv().is_err());
                assert_eq!(turn.mode, GroupChatTurnMode::Summary);
                assert_eq!(turn.target_participant_ids.len(), 1);
            });
        });
    }

    #[test]
    fn private_message_writes_private_store_only() {
        with_temp_data_dir(|| {
            let room = crate::storage::group_chats::create_group_chat("Room".into(), None).unwrap();
            create_run("run-p1");
            create_run("run-p2");
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-p1", Some("Alice".to_string()), None)
                .unwrap();
            crate::storage::group_chats::attach_group_chat_run(&room.id, "run-p2", Some("Bob".to_string()), None)
                .unwrap();

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let (sessions, mut rx_a, mut rx_b) =
                    sessions_for_two_runs("run-p1", "run-p2").await;
                let send_task = tokio::spawn({
                    let sessions = sessions.clone();
                    let room_id = room.id.clone();
                    async move {
                        run_group_chat_turn(&room_id, "@Alice check privately", &sessions).await
                    }
                });

                let private_prompt = receive_text(&mut rx_a).await;
                let turn = send_task.await.unwrap().unwrap();

                assert_eq!(private_prompt, "check privately");
                assert!(rx_b.try_recv().is_err());
                assert_eq!(turn.mode, GroupChatTurnMode::Private);
                assert!(crate::storage::group_chats::list_group_chat_public_turns(&room.id)
                    .unwrap()
                    .is_empty());
                assert_eq!(
                    crate::storage::group_chats::list_group_chat_private_turns(&room.id)
                        .unwrap()
                        .len(),
                    1
                );
            });
        });
    }

}
