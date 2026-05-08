use crate::agent::session_actor::{ActorCommand, AttachmentData};
use crate::models::{ExecutionPath, RunMeta, RunStatus, TaskRun};
use crate::storage;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc;

/// Default inactivity timeout for room turns (10 minutes).
const DEFAULT_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(600);
/// Default hard deadline for room turns (30 minutes).
const DEFAULT_HARD_DEADLINE: Duration = Duration::from_secs(1800);

pub type AdapterResult<T> = Result<T, AgentAdapterError>;
pub type AdapterFuture<'a, T> = Pin<Box<dyn Future<Output = AdapterResult<T>> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq)]
pub struct AgentAdapterError {
    pub message: String,
}

impl AgentAdapterError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for AgentAdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AgentAdapterError {}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Claude,
    Codex,
    Unknown,
}

impl AgentKind {
    pub fn from_agent(agent: &str) -> Self {
        match agent.to_ascii_lowercase().as_str() {
            "claude" => Self::Claude,
            "codex" => Self::Codex,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResumeCapability {
    SessionId,
    Latest,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptInjection {
    SystemPrompt,
    AppendFile,
    InstructionFile,
    Env,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptScope {
    Room,
    Participant,
    Turn,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentCapabilities {
    pub kind: AgentKind,
    pub stream_session: bool,
    pub pipe_exec: bool,
    pub interactive_pty: bool,
    pub resume: ResumeCapability,
    pub prompt_injection: Option<PromptInjection>,
    pub mcp_config: bool,
    pub context_usage: bool,
    pub permission_protocol: bool,
}

impl AgentCapabilities {
    pub fn for_agent(agent: &str) -> Self {
        Self::for_kind(AgentKind::from_agent(agent))
    }

    pub fn for_kind(kind: AgentKind) -> Self {
        match kind {
            AgentKind::Claude => Self {
                kind,
                stream_session: true,
                pipe_exec: true,
                interactive_pty: false,
                resume: ResumeCapability::SessionId,
                prompt_injection: Some(PromptInjection::SystemPrompt),
                mcp_config: true,
                context_usage: true,
                permission_protocol: true,
            },
            AgentKind::Codex => Self {
                kind,
                stream_session: false,
                pipe_exec: true,
                interactive_pty: false,
                resume: ResumeCapability::Latest,
                prompt_injection: None,
                mcp_config: false,
                context_usage: false,
                permission_protocol: false,
            },
            AgentKind::Unknown => Self {
                kind,
                stream_session: false,
                pipe_exec: false,
                interactive_pty: false,
                resume: ResumeCapability::None,
                prompt_injection: None,
                mcp_config: false,
                context_usage: false,
                permission_protocol: false,
            },
        }
    }

    pub fn can_stream_message(&self) -> bool {
        self.stream_session
    }

    pub fn can_use_room_actor(&self) -> bool {
        self.stream_session
    }

    pub fn can_wait_turn_complete(&self) -> bool {
        self.stream_session || self.pipe_exec
    }
}

pub fn can_use_room_actor_run(run: &RunMeta) -> bool {
    AgentCapabilities::for_agent(&run.agent).can_use_room_actor()
        && matches!(run.resolved_execution_path(), ExecutionPath::SessionActor)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcomeStatus {
    Pending,
    Running,
    Complete,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnOutcome {
    pub run_id: String,
    pub status: TurnOutcomeStatus,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

pub trait AgentAdapter: Send {
    fn wait_turn_complete<'a>(&'a mut self) -> AdapterFuture<'a, TurnOutcome>;
    fn stream_message<'a>(&'a mut self, msg: &'a str) -> AdapterFuture<'a, ()>;
    fn inject_prompt(&mut self, scope: PromptScope, body: &str) -> AdapterResult<()>;
    fn capabilities(&self) -> AgentCapabilities;
}

#[derive(Debug, Clone)]
pub struct RunBackedAgentAdapter {
    run_id: String,
    kind: AgentKind,
    cmd_tx: Option<mpsc::Sender<ActorCommand>>,
    poll_interval: Duration,
    inactivity_timeout: Duration,
    hard_deadline: Duration,
}

impl RunBackedAgentAdapter {
    pub fn new(run: &RunMeta) -> Self {
        Self {
            run_id: run.id.clone(),
            kind: AgentKind::from_agent(&run.agent),
            cmd_tx: None,
            poll_interval: Duration::from_secs(5),
            inactivity_timeout: DEFAULT_INACTIVITY_TIMEOUT,
            hard_deadline: DEFAULT_HARD_DEADLINE,
        }
    }

    pub fn with_command_sender(mut self, cmd_tx: mpsc::Sender<ActorCommand>) -> Self {
        self.cmd_tx = Some(cmd_tx);
        self
    }

    #[cfg(test)]
    fn with_polling(mut self, poll_interval: Duration, inactivity_timeout: Duration) -> Self {
        self.poll_interval = poll_interval;
        self.inactivity_timeout = inactivity_timeout;
        self
    }

    #[cfg(test)]
    fn with_deadlines(
        mut self,
        poll_interval: Duration,
        inactivity_timeout: Duration,
        hard_deadline: Duration,
    ) -> Self {
        self.poll_interval = poll_interval;
        self.inactivity_timeout = inactivity_timeout;
        self.hard_deadline = hard_deadline;
        self
    }

}

impl AgentAdapter for RunBackedAgentAdapter {
    fn wait_turn_complete<'a>(&'a mut self) -> AdapterFuture<'a, TurnOutcome> {
        Box::pin(async move {
            let started = std::time::Instant::now();
            loop {
                // Read RunMeta once per iteration (avoids redundant meta.json I/O)
                let run = storage::runs::get_run(&self.run_id)
                    .ok_or_else(|| AgentAdapterError::new(format!("Run {} not found", self.run_id)))?;

                // Check for terminal state
                let outcome = outcome_from_run(&run);
                if matches!(
                    outcome.status,
                    TurnOutcomeStatus::Complete
                        | TurnOutcomeStatus::Failed
                        | TurnOutcomeStatus::Stopped
                ) {
                    return Ok(outcome);
                }

                // Check hard deadline (absolute, never resets)
                if started.elapsed() >= self.hard_deadline {
                    return Err(AgentAdapterError::new(format!(
                        "Timed out waiting for run {} to complete a turn (hard limit: {}s)",
                        self.run_id,
                        self.hard_deadline.as_secs()
                    )));
                }

                // Check inactivity deadline (resets on activity via active_at).
                // Falls back to started_at when active_at is None (no bus events yet).
                // This means a run with zero bus events is considered active for up to
                // inactivity_timeout after creation — the hard_deadline is the only
                // safety net for truly stuck runs that emit no events at all.
                let ref_time = run
                    .active_at
                    .as_deref()
                    .or(Some(run.started_at.as_str()));

                let is_active = ref_time
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| {
                        let elapsed = chrono::Utc::now()
                            .signed_duration_since(dt.with_timezone(&chrono::Utc));
                        elapsed.num_seconds() < self.inactivity_timeout.as_secs() as i64
                    })
                    .unwrap_or(true);

                if !is_active {
                    return Err(AgentAdapterError::new(format!(
                        "No activity for {}min on run {}",
                        self.inactivity_timeout.as_secs() / 60,
                        self.run_id
                    )));
                }

                tokio::time::sleep(self.poll_interval).await;
            }
        })
    }

    fn stream_message<'a>(&'a mut self, _msg: &'a str) -> AdapterFuture<'a, ()> {
        Box::pin(async move {
            let cmd_tx = self.cmd_tx.clone().ok_or_else(|| {
                AgentAdapterError::new("Room participant is not attached to an active session")
            })?;
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            cmd_tx
                .send(ActorCommand::SendMessage {
                    text: _msg.to_string(),
                    attachments: Vec::<AttachmentData>::new(),
                    reply: reply_tx,
                })
                .await
                .map_err(|_| AgentAdapterError::new("Room participant actor is not available"))?;
            reply_rx
                .await
                .map_err(|_| AgentAdapterError::new("Room participant actor dropped reply"))?
                .map_err(AgentAdapterError::new)
        })
    }

    fn inject_prompt(&mut self, _scope: PromptScope, _body: &str) -> AdapterResult<()> {
        Err(AgentAdapterError::new(
            "Room prompt injection is not wired in Phase 2",
        ))
    }

    fn capabilities(&self) -> AgentCapabilities {
        AgentCapabilities::for_kind(self.kind)
    }
}

pub fn adapter_for_run(run: &RunMeta) -> RunBackedAgentAdapter {
    RunBackedAgentAdapter::new(run)
}

pub fn adapter_for_task_run(run: &TaskRun) -> RunBackedAgentAdapter {
    RunBackedAgentAdapter {
        run_id: run.id.clone(),
        kind: AgentKind::from_agent(&run.agent),
        cmd_tx: None,
        poll_interval: Duration::from_secs(5),
        inactivity_timeout: DEFAULT_INACTIVITY_TIMEOUT,
        hard_deadline: DEFAULT_HARD_DEADLINE,
    }
}

fn outcome_from_run(run: &RunMeta) -> TurnOutcome {
    TurnOutcome {
        run_id: run.id.clone(),
        status: match run.status {
            RunStatus::Pending => TurnOutcomeStatus::Pending,
            RunStatus::Running => TurnOutcomeStatus::Running,
            RunStatus::Idle | RunStatus::Completed => TurnOutcomeStatus::Complete,
            RunStatus::Failed => TurnOutcomeStatus::Failed,
            RunStatus::Stopped => TurnOutcomeStatus::Stopped,
        },
        exit_code: run.exit_code,
        error: run.error_message.clone(),
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
    fn wait_turn_complete_treats_stopped_run_as_terminal() {
        let outcome = with_temp_data_dir(|| {
            crate::storage::runs::create_run(
                "run-stop",
                "hello",
                "D:/work/app",
                "codex",
                RunStatus::Stopped,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            let run = crate::storage::runs::get_run("run-stop").unwrap();
            let mut adapter = adapter_for_run(&run)
                .with_polling(Duration::from_millis(1), Duration::from_secs(600));

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(adapter.wait_turn_complete())
        })
        .unwrap();

        assert_eq!(outcome.status, TurnOutcomeStatus::Stopped);
        assert_eq!(outcome.run_id, "run-stop");
    }

    #[test]
    fn wait_turn_complete_treats_failed_run_as_terminal() {
        let outcome = with_temp_data_dir(|| {
            crate::storage::runs::create_run(
                "run-fail",
                "hello",
                "D:/work/app",
                "codex",
                RunStatus::Failed,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            let run = crate::storage::runs::get_run("run-fail").unwrap();
            let mut adapter = adapter_for_run(&run)
                .with_polling(Duration::from_millis(1), Duration::from_secs(600));

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(adapter.wait_turn_complete())
        })
        .unwrap();

        assert_eq!(outcome.status, TurnOutcomeStatus::Failed);
        assert_eq!(outcome.run_id, "run-fail");
    }

    #[test]
    fn maps_known_agent_capabilities() {
        let claude = AgentCapabilities::for_kind(AgentKind::Claude);
        assert!(claude.stream_session);
        assert!(claude.pipe_exec);
        assert!(!claude.interactive_pty);
        assert_eq!(claude.resume, ResumeCapability::SessionId);
        assert_eq!(claude.prompt_injection, Some(PromptInjection::SystemPrompt));
        assert!(claude.mcp_config);
        assert!(claude.context_usage);
        assert!(claude.permission_protocol);

        let codex = AgentCapabilities::for_kind(AgentKind::Codex);
        assert!(!codex.stream_session);
        assert!(codex.pipe_exec);
        assert!(!codex.interactive_pty);
        assert_eq!(codex.resume, ResumeCapability::Latest);
        assert_eq!(codex.prompt_injection, None);
        assert!(!codex.mcp_config);
        assert!(!codex.context_usage);
        assert!(!codex.permission_protocol);

    }

    #[test]
    fn waits_until_run_is_turn_complete() {
        let outcome = with_temp_data_dir(|| {
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
            let run = crate::storage::runs::get_run("run-1").unwrap();
            let mut adapter = adapter_for_run(&run)
                .with_polling(Duration::from_millis(1), Duration::from_secs(600));

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(adapter.wait_turn_complete())
        });

        let outcome = outcome.unwrap();
        assert_eq!(outcome.status, TurnOutcomeStatus::Complete);
        assert_eq!(outcome.run_id, "run-1");
    }

    #[test]
    fn active_actor_adapter_streams_message_through_mailbox() {
        with_temp_data_dir(|| {
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
            let run = crate::storage::runs::get_run("run-1").unwrap();
            let (tx, mut rx) =
                tokio::sync::mpsc::channel::<crate::agent::session_actor::ActorCommand>(1);
            let mut adapter = adapter_for_run(&run).with_command_sender(tx);

            assert!(adapter.capabilities().can_stream_message());

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let send_task =
                    tokio::spawn(async move { adapter.stream_message("Hello peers").await });
                let command = rx.recv().await.expect("message should be sent");
                match command {
                    crate::agent::session_actor::ActorCommand::SendMessage {
                        text,
                        attachments,
                        reply,
                    } => {
                        assert_eq!(text, "Hello peers");
                        assert!(attachments.is_empty());
                        reply.send(Ok(())).unwrap();
                    }
                    _ => panic!("expected SendMessage command"),
                }
                send_task.await.unwrap().unwrap();
            });
        });
    }

    #[test]
    fn wait_turn_complete_times_out_on_hard_deadline() {
        let result = with_temp_data_dir(|| {
            crate::storage::runs::create_run(
                "run-timeout",
                "hello",
                "D:/work/app",
                "claude",
                RunStatus::Running, // non-terminal — will never become idle
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            let run = crate::storage::runs::get_run("run-timeout").unwrap();
            // 1ms poll, 600s inactivity (won't trigger), 50ms hard deadline
            let mut adapter = adapter_for_run(&run)
                .with_deadlines(Duration::from_millis(1), Duration::from_secs(600), Duration::from_millis(50));

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(adapter.wait_turn_complete())
        });

        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("hard limit"), "expected hard limit timeout, got: {}", msg);
        assert!(msg.contains("run-timeout"), "expected run id in message, got: {}", msg);
    }

    #[test]
    fn wait_turn_complete_times_out_on_inactivity() {
        let result = with_temp_data_dir(|| {
            crate::storage::runs::create_run(
                "run-inactive",
                "hello",
                "D:/work/app",
                "claude",
                RunStatus::Running, // non-terminal
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            // Set active_at to 2 minutes ago so inactivity check triggers quickly
            crate::storage::runs::with_meta("run-inactive", |meta| {
                meta.active_at = Some("2020-01-01T00:00:00Z".to_string());
                Ok(())
            })
            .unwrap();

            let run = crate::storage::runs::get_run("run-inactive").unwrap();
            // 1ms poll, 1s inactivity (will trigger since active_at is 2020), 60s hard deadline
            let mut adapter = adapter_for_run(&run)
                .with_deadlines(Duration::from_millis(1), Duration::from_secs(1), Duration::from_secs(60));

            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(adapter.wait_turn_complete())
        });

        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("No activity"), "expected inactivity timeout, got: {}", msg);
    }
}
