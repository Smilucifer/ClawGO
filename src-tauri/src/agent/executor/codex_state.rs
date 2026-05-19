// src-tauri/src/agent/executor/codex_state.rs
use crate::models::BusEvent;
use serde_json::Value;
use std::collections::HashMap;

pub struct CodexProtocolState {
    run_id: String,
    pending_tools: HashMap<String, ()>,
    sent_session_init: bool,
    turn_active: bool,
    captured_thread_id: Option<String>,
    seen_turn_completed_count: u32,
}

impl CodexProtocolState {
    pub fn new(run_id: String) -> Self {
        Self {
            run_id,
            pending_tools: HashMap::new(),
            sent_session_init: false,
            turn_active: false,
            captured_thread_id: None,
            seen_turn_completed_count: 0,
        }
    }

    pub fn captured_thread_id(&self) -> Option<&str> {
        self.captured_thread_id.as_deref()
    }

    pub fn has_seen_turn_completed(&self) -> bool {
        self.seen_turn_completed_count > 0
    }

    pub fn map_event(&mut self, raw: &Value) -> Vec<BusEvent> {
        let type_str = raw.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match type_str {
            "thread.started" => self.handle_thread_started(raw),
            "turn.started" => self.handle_turn_started(),
            "item.started" => self.handle_item_started(raw),
            _ => Vec::new(),
        }
    }

    fn handle_thread_started(&mut self, raw: &Value) -> Vec<BusEvent> {
        let Some(tid) = raw.get("thread_id").and_then(|v| v.as_str()) else {
            return Vec::new();
        };
        self.captured_thread_id = Some(tid.to_string());
        if self.sent_session_init {
            return Vec::new();
        }
        self.sent_session_init = true;
        vec![BusEvent::SessionInit {
            run_id: self.run_id.clone(),
            session_id: Some(tid.to_string()),
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
        self.turn_active = true;
        vec![BusEvent::RunState {
            run_id: self.run_id.clone(),
            state: "running".to_string(),
            exit_code: None,
            error: None,
        }]
    }

    fn handle_item_started(&mut self, raw: &Value) -> Vec<BusEvent> {
        let Some(item) = raw.get("item") else {
            return Vec::new();
        };
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if item_type != "command_execution" {
            return Vec::new();
        }
        let Some(item_id) = item.get("id").and_then(|v| v.as_str()) else {
            return Vec::new();
        };
        let command = item.get("command").and_then(|v| v.as_str()).unwrap_or("");
        self.pending_tools.insert(item_id.to_string(), ());
        vec![BusEvent::ToolStart {
            run_id: self.run_id.clone(),
            tool_use_id: item_id.to_string(),
            tool_name: "bash".to_string(),
            input: serde_json::json!({ "command": command }),
            parent_tool_use_id: None,
        }]
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
    fn thread_started_emits_session_init_and_captures_tid() {
        let mut s = state();
        let events = s.map_event(&json!({"type":"thread.started","thread_id":"019e4159-ca48"}));
        assert_eq!(events.len(), 1);
        match &events[0] {
            BusEvent::SessionInit { session_id, .. } => {
                assert_eq!(session_id.as_deref(), Some("019e4159-ca48"));
            }
            other => panic!("expected SessionInit, got {:?}", other),
        }
        assert_eq!(s.captured_thread_id(), Some("019e4159-ca48"));
    }

    #[test]
    fn thread_started_session_init_is_only_sent_once() {
        let mut s = state();
        let _ = s.map_event(&json!({"type":"thread.started","thread_id":"a"}));
        let events2 = s.map_event(&json!({"type":"thread.started","thread_id":"a"}));
        assert!(events2.is_empty());
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
            BusEvent::ToolStart { tool_use_id, tool_name, input, .. } => {
                assert_eq!(tool_use_id, "item_1");
                assert_eq!(tool_name, "bash");
                assert_eq!(input.get("command").and_then(|v| v.as_str()), Some("cargo build"));
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
}
