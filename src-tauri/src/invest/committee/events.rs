use super::orchestrator::{CommitteeResult, RoundOutputSummary};
use super::roles::CommitteeRole;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Committee streaming events — emitted via Tauri's `app.emit()`
// ---------------------------------------------------------------------------

/// Pipeline step index for a role (used by frontend PipelineFlow).
/// Maps each role to its position in the 7-node pipeline.
pub fn step_index_for_role(role: CommitteeRole, round: u8) -> usize {
    match role {
        CommitteeRole::Macro => 0,
        CommitteeRole::QuantR1 => 1,
        CommitteeRole::RiskR1 => 2,
        CommitteeRole::Wealth => 3,
        CommitteeRole::QuantR2 => 4,
        CommitteeRole::RiskR2 => 5,
        CommitteeRole::Cio => 6,
    }
}

/// Events emitted during a committee pipeline run.
/// Frontend listens on `"committee-event"` Tauri event channel.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommitteeEvent {
    /// Batch started — one or more symbols queued.
    CommitteeStart {
        symbols: Vec<String>,
        total: usize,
    },
    /// A role's LLM call is about to begin.
    #[serde(rename_all = "camelCase")]
    RoleStart {
        symbol: String,
        role: CommitteeRole,
        round: u8,
        step_index: usize,
    },
    /// A role's LLM call completed (or fell back to unavailable).
    #[serde(rename_all = "camelCase")]
    RoleComplete {
        symbol: String,
        role: CommitteeRole,
        round: u8,
        summary: RoundOutputSummary,
        step_index: usize,
    },
    /// One symbol's full pipeline finished.
    SymbolComplete {
        symbol: String,
        result: CommitteeResult,
    },
    /// All symbols processed (success or partial).
    Done {
        completed: usize,
        total: usize,
    },
    /// A symbol's pipeline errored (non-retryable).
    Error {
        symbol: String,
        error: String,
    },
}
