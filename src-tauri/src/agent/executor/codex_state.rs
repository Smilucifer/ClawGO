// src-tauri/src/agent/executor/codex_state.rs
use crate::models::BusEvent;
use serde_json::Value;
use std::collections::HashSet;

const EVT_THREAD_STARTED: &str = "thread.started";
const EVT_TURN_STARTED: &str = "turn.started";
const EVT_TURN_COMPLETED: &str = "turn.completed";
const EVT_TURN_FAILED: &str = "turn.failed";
const EVT_ITEM_STARTED: &str = "item.started";
const EVT_ITEM_COMPLETED: &str = "item.completed";
const ITEM_COMMAND_EXECUTION: &str = "command_execution";
const ITEM_AGENT_MESSAGE: &str = "agent_message";

pub struct CodexProtocolState {
    run_id: String,
    pending_tools: HashSet<String>,
    /// One-shot guard: SessionInit is emitted at most once per run,
    /// even if Codex re-announces `thread.started`.
    sent_session_init: bool,
    /// Set on `thread.started`, drained by the caller after each `map_event`.
    pending_thread_id: Option<String>,
    seen_turn_completed: bool,
}

impl CodexProtocolState {
    pub fn new(run_id: String) -> Self {
        Self {
            run_id,
            pending_tools: HashSet::new(),
            sent_session_init: false,
            pending_thread_id: None,
            seen_turn_completed: false,
        }
    }

    pub fn take_new_thread_id(&mut self) -> Option<String> {
        self.pending_thread_id.take()
    }

    pub fn has_seen_turn_completed(&self) -> bool {
        self.seen_turn_completed
    }

    pub fn map_event(&mut self, raw: &Value) -> Vec<BusEvent> {
        let type_str = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match type_str {
            EVT_THREAD_STARTED => self.handle_thread_started(raw),
            EVT_TURN_STARTED => self.handle_turn_started(),
            EVT_TURN_COMPLETED => self.handle_turn_completed(raw),
            EVT_TURN_FAILED => self.handle_turn_failed(raw),
            EVT_ITEM_STARTED => self.handle_item_started(raw),
            EVT_ITEM_COMPLETED => self.handle_item_completed(raw),
            _ => Vec::new(),
        }
    }

    fn handle_thread_started(&mut self, raw: &Value) -> Vec<BusEvent> {
        let Some(tid) = raw.get("thread_id").and_then(|v| v.as_str()) else {
            return Vec::new();
        };
        let tid_owned = tid.to_string();
        self.pending_thread_id = Some(tid_owned.clone());
        if self.sent_session_init {
            return Vec::new();
        }
        self.sent_session_init = true;
        vec![BusEvent::SessionInit {
            run_id: self.run_id.clone(),
            session_id: Some(tid_owned),
            model: None,
            tools: Vec::new(),
            cwd: String::new(),
            slash_commands: Vec::new(),
            mcp_servers: Vec::new(),
            permission_mode: None,
            api_key_source: None,
            claude_code_version: None,
            output_style: None,
            agents: Vec::new(),
            skills: Vec::new(),
            plugins: Vec::new(),
            plugin_errors: Vec::new(),
            fast_mode_state: None,
            msvc_injected: None,
        }]
    }

    fn handle_turn_started(&mut self) -> Vec<BusEvent> {
        vec![BusEvent::RunState {
            run_id: self.run_id.clone(),
            state: "running".to_string(),
            exit_code: None,
            error: None,
        }]
    }

    fn handle_turn_completed(&mut self, raw: &Value) -> Vec<BusEvent> {
        let usage = raw.get("usage");
        let input_tokens = usage
            .and_then(|u| u.get("input_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let output_tokens = usage
            .and_then(|u| u.get("output_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_read = usage
            .and_then(|u| u.get("cached_input_tokens"))
            .and_then(|v| v.as_u64());

        self.pending_tools.clear();
        self.seen_turn_completed = true;

        vec![
            BusEvent::UsageUpdate {
                run_id: self.run_id.clone(),
                input_tokens,
                output_tokens,
                cache_read_tokens: cache_read,
                cache_write_tokens: None,
                total_cost_usd: 0.0,
                turn_index: None,
                model_usage: None,
                duration_api_ms: None,
                duration_ms: None,
                num_turns: None,
                stop_reason: None,
                service_tier: None,
                speed: None,
                web_fetch_requests: None,
                cache_creation_5m: None,
                cache_creation_1h: None,
            },
            BusEvent::RunState {
                run_id: self.run_id.clone(),
                state: "idle".to_string(),
                exit_code: None,
                error: None,
            },
        ]
    }

    fn handle_turn_failed(&mut self, raw: &Value) -> Vec<BusEvent> {
        let error = raw
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("Codex turn failed")
            .to_string();
        self.pending_tools.clear();
        vec![BusEvent::RunState {
            run_id: self.run_id.clone(),
            state: "failed".to_string(),
            exit_code: None,
            error: Some(error),
        }]
    }

    fn handle_item_started(&mut self, raw: &Value) -> Vec<BusEvent> {
        let Some(item) = raw.get("item") else {
            return Vec::new();
        };
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if item_type != ITEM_COMMAND_EXECUTION {
            return Vec::new();
        }
        let Some(item_id) = item.get("id").and_then(|v| v.as_str()) else {
            return Vec::new();
        };
        let command = item.get("command").and_then(|v| v.as_str()).unwrap_or("");
        self.pending_tools.insert(item_id.to_string());
        vec![BusEvent::ToolStart {
            run_id: self.run_id.clone(),
            tool_use_id: item_id.to_string(),
            tool_name: "bash".to_string(),
            input: serde_json::json!({ "command": command }),
            parent_tool_use_id: None,
        }]
    }

    fn handle_item_completed(&mut self, raw: &Value) -> Vec<BusEvent> {
        let Some(item) = raw.get("item") else {
            return Vec::new();
        };
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let Some(item_id) = item.get("id").and_then(|v| v.as_str()) else {
            return Vec::new();
        };
        match item_type {
            ITEM_COMMAND_EXECUTION => {
                let exit_code = item.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(-1);
                let aggregated_output = item
                    .get("aggregated_output")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let status = if exit_code == 0 { "success" } else { "error" };
                self.pending_tools.remove(item_id);
                vec![BusEvent::ToolEnd {
                    run_id: self.run_id.clone(),
                    tool_use_id: item_id.to_string(),
                    tool_name: "bash".to_string(),
                    output: serde_json::json!({
                        "aggregated_output": aggregated_output,
                        "exit_code": exit_code,
                    }),
                    status: status.to_string(),
                    duration_ms: None,
                    parent_tool_use_id: None,
                    tool_use_result: None,
                }]
            }
            ITEM_AGENT_MESSAGE => {
                let text = item
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if text.is_empty() {
                    return Vec::new();
                }
                vec![
                    BusEvent::MessageDelta {
                        run_id: self.run_id.clone(),
                        text: text.clone(),
                        parent_tool_use_id: None,
                    },
                    BusEvent::MessageComplete {
                        run_id: self.run_id.clone(),
                        message_id: item_id.to_string(),
                        text,
                        parent_tool_use_id: None,
                        model: None,
                        stop_reason: None,
                        message_usage: None,
                    },
                ]
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn state() -> CodexProtocolState {
        CodexProtocolState::new("run-1".to_string())
    }

    #[test]
    fn thread_started_emits_session_init_and_surfaces_tid() {
        let mut s = state();
        let events = s.map_event(&json!({"type":"thread.started","thread_id":"019e4159-ca48"}));
        assert_eq!(events.len(), 1);
        match &events[0] {
            BusEvent::SessionInit { session_id, .. } => {
                assert_eq!(session_id.as_deref(), Some("019e4159-ca48"));
            }
            other => panic!("expected SessionInit, got {:?}", other),
        }
        assert_eq!(s.take_new_thread_id().as_deref(), Some("019e4159-ca48"));
        assert!(s.take_new_thread_id().is_none(), "take should be one-shot");
    }

    #[test]
    fn thread_started_session_init_is_only_sent_once() {
        let mut s = state();
        let _ = s.map_event(&json!({"type":"thread.started","thread_id":"a"}));
        let events2 = s.map_event(&json!({"type":"thread.started","thread_id":"a"}));
        assert!(events2.is_empty());
        assert_eq!(s.take_new_thread_id().as_deref(), Some("a"));
    }

    #[test]
    fn turn_started_emits_run_state_running() {
        let mut s = state();
        let events = s.map_event(&json!({"type":"turn.started"}));
        assert_eq!(events.len(), 1);
        match &events[0] {
            BusEvent::RunState { state, .. } => assert_eq!(state, "running"),
            other => panic!("expected RunState, got {:?}", other),
        }
    }

    #[test]
    fn item_started_command_execution_emits_tool_start() {
        let mut s = state();
        let raw = json!({
            "type":"item.started",
            "item":{
                "id":"item_1",
                "type":"command_execution",
                "command":"cargo build",
                "aggregated_output":"",
                "exit_code":null,
                "status":"in_progress"
            }
        });
        let events = s.map_event(&raw);
        assert_eq!(events.len(), 1);
        match &events[0] {
            BusEvent::ToolStart {
                tool_use_id,
                tool_name,
                input,
                ..
            } => {
                assert_eq!(tool_use_id, "item_1");
                assert_eq!(tool_name, "bash");
                assert_eq!(
                    input.get("command").and_then(|v| v.as_str()),
                    Some("cargo build")
                );
            }
            other => panic!("expected ToolStart, got {:?}", other),
        }
    }

    #[test]
    fn item_started_agent_message_is_ignored() {
        let mut s = state();
        let raw = json!({"type":"item.started","item":{"id":"item_0","type":"agent_message"}});
        assert!(s.map_event(&raw).is_empty());
    }

    #[test]
    fn item_completed_command_execution_success_emits_tool_end() {
        let mut s = state();
        let raw = json!({
            "type":"item.completed",
            "item":{
                "id":"item_1",
                "type":"command_execution",
                "command":"ls",
                "aggregated_output":"file.txt\n",
                "exit_code":0,
                "status":"completed"
            }
        });
        let events = s.map_event(&raw);
        assert_eq!(events.len(), 1);
        match &events[0] {
            BusEvent::ToolEnd {
                tool_use_id,
                tool_name,
                status,
                output,
                ..
            } => {
                assert_eq!(tool_use_id, "item_1");
                assert_eq!(tool_name, "bash");
                assert_eq!(status, "success");
                assert_eq!(
                    output.get("aggregated_output").and_then(|v| v.as_str()),
                    Some("file.txt\n")
                );
                assert_eq!(output.get("exit_code").and_then(|v| v.as_i64()), Some(0));
            }
            other => panic!("expected ToolEnd, got {:?}", other),
        }
    }

    #[test]
    fn item_completed_command_execution_nonzero_emits_error_status() {
        let mut s = state();
        let raw = json!({
            "type":"item.completed",
            "item":{"id":"item_2","type":"command_execution","aggregated_output":"err","exit_code":2,"status":"completed"}
        });
        let events = s.map_event(&raw);
        match &events[0] {
            BusEvent::ToolEnd { status, .. } => assert_eq!(status, "error"),
            other => panic!("expected ToolEnd, got {:?}", other),
        }
    }

    #[test]
    fn item_completed_agent_message_emits_delta_and_complete() {
        let mut s = state();
        let raw = json!({
            "type":"item.completed",
            "item":{"id":"item_3","type":"agent_message","text":"Hello world"}
        });
        let events = s.map_event(&raw);
        assert_eq!(events.len(), 2);
        match &events[0] {
            BusEvent::MessageDelta { text, .. } => assert_eq!(text, "Hello world"),
            other => panic!("expected MessageDelta, got {:?}", other),
        }
        match &events[1] {
            BusEvent::MessageComplete {
                message_id, text, ..
            } => {
                assert_eq!(message_id, "item_3");
                assert_eq!(text, "Hello world");
            }
            other => panic!("expected MessageComplete, got {:?}", other),
        }
    }

    #[test]
    fn turn_completed_emits_usage_and_idle_and_clears_pending() {
        let mut s = state();
        let _ = s.map_event(&json!({"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"x"}}));

        let raw = json!({
            "type":"turn.completed",
            "usage":{"input_tokens":100,"cached_input_tokens":50,"output_tokens":20,"reasoning_output_tokens":10}
        });
        let events = s.map_event(&raw);
        assert_eq!(events.len(), 2);
        match &events[0] {
            BusEvent::UsageUpdate {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                ..
            } => {
                assert_eq!(*input_tokens, 100);
                assert_eq!(*output_tokens, 20);
                assert_eq!(*cache_read_tokens, Some(50));
            }
            other => panic!("expected UsageUpdate, got {:?}", other),
        }
        match &events[1] {
            BusEvent::RunState { state, .. } => assert_eq!(state, "idle"),
            other => panic!("expected RunState idle, got {:?}", other),
        }
        assert!(s.has_seen_turn_completed());
    }

    #[test]
    fn turn_failed_emits_failed_state_and_clears_pending() {
        let mut s = state();
        let _ = s.map_event(&json!({"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"x"}}));

        let events = s.map_event(&json!({"type":"turn.failed","error":{"message":"oops"}}));
        assert_eq!(events.len(), 1);
        match &events[0] {
            BusEvent::RunState { state, error, .. } => {
                assert_eq!(state, "failed");
                assert!(error.as_deref().unwrap_or("").contains("oops"));
            }
            other => panic!("expected RunState failed, got {:?}", other),
        }
    }

    #[test]
    fn cross_turn_item_id_reuse_after_failure_does_not_panic() {
        let mut s = state();
        let _ = s.map_event(&json!({"type":"turn.started"}));
        let _ = s.map_event(&json!({"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"x"}}));
        let _ = s.map_event(&json!({"type":"turn.failed","error":{"message":"e"}}));
        let _ = s.map_event(&json!({"type":"turn.started"}));
        let events = s.map_event(&json!({"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"y"}}));
        assert_eq!(events.len(), 1);
        match &events[0] {
            BusEvent::ToolStart { input, .. } => {
                assert_eq!(input.get("command").and_then(|v| v.as_str()), Some("y"));
            }
            other => panic!("expected ToolStart, got {:?}", other),
        }
    }

    #[test]
    fn unknown_event_type_returns_empty() {
        let mut s = state();
        assert!(s
            .map_event(&json!({"type":"some.unknown.thing"}))
            .is_empty());
    }
}
