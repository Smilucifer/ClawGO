use crate::models::TaskRun;
use crate::group_chat::adapter::AgentCapabilities;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroupChat {
    pub id: String,
    pub name: String,
    pub cwd: Option<String>,
    pub memo: String,
    pub participants: Vec<GroupChatParticipant>,
    pub active_plan_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub auto_chain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroupChatParticipant {
    pub id: String,
    pub run_id: String,
    pub agent: String,
    pub label: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GroupChatTurnMode {
    Fanout,
    Debate,
    Summary,
    Private,
    SingleTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupChatResponseRef {
    pub participant_id: String,
    pub run_id: String,
    pub event_seq_start: u64,
    pub event_seq_end: u64,
    pub preview: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupChatTurn {
    pub id: String,
    pub idx: u64,
    pub mode: GroupChatTurnMode,
    pub user_input: String,
    pub target_participant_ids: Vec<String>,
    pub responses: Vec<GroupChatResponseRef>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroupChatSummary {
    pub id: String,
    pub name: String,
    pub cwd: Option<String>,
    pub participant_count: usize,
    pub memo_preview: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupChatParticipantDetail {
    pub participant: GroupChatParticipant,
    pub run: Option<TaskRun>,
    pub capabilities: AgentCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupChatDetail {
    pub id: String,
    pub name: String,
    pub cwd: Option<String>,
    pub memo: String,
    pub participants: Vec<GroupChatParticipantDetail>,
    pub turns: Vec<GroupChatTurn>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Draft,
    Active,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanTask {
    pub id: String,
    pub description: String,
    pub assignee_id: Option<String>,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanArtifact {
    pub id: String,
    pub group_chat_id: String,
    pub title: String,
    pub tasks: Vec<PlanTask>,
    pub status: PlanStatus,
    pub user_notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
