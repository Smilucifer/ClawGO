use super::orchestrator::{CommitteeResult, RoundOutputSummary};
use super::roles::CommitteeRole;
use crate::invest::regime::RegimeMetrics;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Committee streaming events — emitted via Tauri's `app.emit()`
// ---------------------------------------------------------------------------

/// Pipeline step index for a role (used by frontend PipelineFlow).
/// Maps each role+round to its position in the 8-node pipeline:
///   Macro(0) -> Regime(1) -> Quant/R1(2) -> Risk/R1(3) -> Quant/R2(4) -> Risk/R2(5) -> L4(6) -> CIO(7)
pub fn step_index_for_role(role: CommitteeRole, round: u8) -> usize {
    match (role, round) {
        (CommitteeRole::Macro, _) => 0,
        // Regime is a computed (non-LLM) step at index 1; not emitted as RoleStart
        (CommitteeRole::Quant, 1) => 2,
        (CommitteeRole::Risk, 1) => 3,
        (CommitteeRole::Quant, _) => 4,  // R2+
        (CommitteeRole::Risk, _) => 5,   // R2+
        (CommitteeRole::L4Officer, _) => 6,
        (CommitteeRole::Cio, _) => 7,
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
    /// Regime computation step completed (quantitative metrics, not LLM).
    #[serde(rename_all = "camelCase")]
    RegimeStep {
        symbol: String,
        success: bool,
        context_preview: String,
        step_index: usize,
        regime: Option<String>,
        reason: Option<String>,
        strategy_hint: Option<String>,
        metrics: Option<RegimeMetrics>,
    },
    /// A tool was called during a role's LLM turn.
    #[serde(rename_all = "camelCase")]
    ToolCall {
        symbol: String,
        role: CommitteeRole,
        round: u8,
        tool_name: String,
        arguments: String,
        result: Option<String>,
        success: bool,
        latency_ms: u64,
    },
    /// A symbol's pipeline errored (non-retryable).
    Error {
        symbol: String,
        error: String,
    },
}
