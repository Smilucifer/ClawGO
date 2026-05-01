use crate::models::TaskRun;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoomKind {
    #[default]
    Roundtable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Room {
    pub id: String,
    #[serde(default)]
    pub kind: RoomKind,
    pub name: String,
    pub description: String,
    pub cwd: Option<String>,
    pub memo: String,
    pub participants: Vec<RoomParticipant>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomParticipant {
    pub id: String,
    pub run_id: String,
    pub agent: String,
    pub label: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoomTurnMode {
    Fanout,
    Debate,
    Summary,
    Private,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomResponseRef {
    pub participant_id: String,
    pub run_id: String,
    pub event_seq_start: u64,
    pub event_seq_end: u64,
    pub preview: Option<String>,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomTurn {
    pub id: String,
    pub idx: u64,
    pub mode: RoomTurnMode,
    pub user_input: String,
    pub target_participant_ids: Vec<String>,
    pub responses: Vec<RoomResponseRef>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomSummary {
    pub id: String,
    pub kind: RoomKind,
    pub name: String,
    pub description: String,
    pub cwd: Option<String>,
    pub participant_count: usize,
    pub memo_preview: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomParticipantDetail {
    pub participant: RoomParticipant,
    pub run: Option<TaskRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomDetail {
    pub id: String,
    pub kind: RoomKind,
    pub name: String,
    pub description: String,
    pub cwd: Option<String>,
    pub memo: String,
    pub participants: Vec<RoomParticipantDetail>,
    pub turns: Vec<RoomTurn>,
    pub created_at: String,
    pub updated_at: String,
}
