use crate::models::ConversationRef;
use serde_json::Value;
use std::{
    env,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use tokio::{
    fs,
    io::{AsyncBufReadExt, BufReader},
    time::{sleep, Instant},
};

pub struct NativeTranscriptResult {
    pub text: String,
    pub conversation_ref: Option<ConversationRef>,
}

#[derive(Debug, Clone)]
pub struct NativeTranscriptBaseline {
    path: PathBuf,
    byte_len: u64,
}

pub async fn capture_native_transcript_baseline(
    agent: &str,
    cwd: &str,
) -> Option<NativeTranscriptBaseline> {
    let path = match agent {
        "codex" => find_latest_codex_rollout(cwd).await,
        "gemini" => find_latest_gemini_session(cwd).await,
        _ => None,
    }?;
    let byte_len = fs::metadata(&path).await.ok()?.len();
    Some(NativeTranscriptBaseline { path, byte_len })
}

pub async fn wait_for_native_transcript_turn(
    agent: &str,
    cwd: &str,
    spawned_at: SystemTime,
    baseline: Option<NativeTranscriptBaseline>,
) -> Result<NativeTranscriptResult, String> {
    match agent {
        "codex" => wait_for_codex_turn(cwd, spawned_at, baseline).await,
        "gemini" => wait_for_gemini_turn(cwd, spawned_at, baseline).await,
        _ => Err(format!(
            "native transcript adapter does not support {agent}"
        )),
    }
}

fn home_dir() -> Result<PathBuf, String> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| "Could not resolve user home directory".to_string())
}

fn normalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/").trim_end_matches('/').to_string();
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
}

async fn read_first_json_line(path: &Path) -> Option<Value> {
    let file = fs::File::open(path).await.ok()?;
    let mut lines = BufReader::new(file).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return serde_json::from_str::<Value>(trimmed).ok();
    }
    None
}

fn text_from_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(text.clone())
            }
        }
        Value::Array(items) => {
            let text = items
                .iter()
                .filter_map(text_from_value)
                .collect::<Vec<_>>()
                .join("");
            if text.trim().is_empty() {
                None
            } else {
                Some(text)
            }
        }
        Value::Object(map) => {
            for key in ["text", "content", "message", "response", "parts"] {
                if let Some(text) = map.get(key).and_then(text_from_value) {
                    return Some(text);
                }
            }
            None
        }
        _ => None,
    }
}

fn codex_candidate_dirs(root: &Path) -> Vec<PathBuf> {
    let today = chrono::Local::now().date_naive();
    [0_i64, -1]
        .into_iter()
        .filter_map(|offset| today.checked_add_signed(chrono::Duration::days(offset)))
        .map(|date| {
            root.join(format!("{}", date.format("%Y")))
                .join(format!("{}", date.format("%m")))
                .join(format!("{}", date.format("%d")))
        })
        .collect()
}

async fn find_codex_rollout(cwd: &str, spawned_at: SystemTime) -> Option<PathBuf> {
    let target_cwd = normalize_path(cwd);
    let root = home_dir().ok()?.join(".codex").join("sessions");
    let mut best: Option<(PathBuf, Duration)> = None;

    for dir in codex_candidate_dirs(&root) {
        let mut entries = match fs::read_dir(dir).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
            if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
                continue;
            }
            let Some(first) = read_first_json_line(&path).await else {
                continue;
            };
            if first.get("type").and_then(Value::as_str) != Some("session_meta") {
                continue;
            }
            let payload = first.get("payload").unwrap_or(&Value::Null);
            let meta_cwd = payload.get("cwd").and_then(Value::as_str).unwrap_or("");
            if normalize_path(meta_cwd) != target_cwd {
                continue;
            }
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            let delta = if modified >= spawned_at {
                modified.duration_since(spawned_at).ok()?
            } else {
                spawned_at.duration_since(modified).ok()?
            };
            if delta > Duration::from_secs(300) {
                continue;
            }
            if best
                .as_ref()
                .map(|(_, current)| delta < *current)
                .unwrap_or(true)
            {
                best = Some((path, delta));
            }
        }
    }

    best.map(|(path, _)| path)
}

async fn find_latest_codex_rollout(cwd: &str) -> Option<PathBuf> {
    let target_cwd = normalize_path(cwd);
    let root = home_dir().ok()?.join(".codex").join("sessions");
    let mut best: Option<(PathBuf, SystemTime)> = None;

    for dir in codex_candidate_dirs(&root) {
        let mut entries = match fs::read_dir(dir).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
            if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
                continue;
            }
            let Some(first) = read_first_json_line(&path).await else {
                continue;
            };
            if first.get("type").and_then(Value::as_str) != Some("session_meta") {
                continue;
            }
            let payload = first.get("payload").unwrap_or(&Value::Null);
            let meta_cwd = payload.get("cwd").and_then(Value::as_str).unwrap_or("");
            if normalize_path(meta_cwd) != target_cwd {
                continue;
            }
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            if best
                .as_ref()
                .map(|(_, current)| modified > *current)
                .unwrap_or(true)
            {
                best = Some((path, modified));
            }
        }
    }

    best.map(|(path, _)| path)
}

fn parse_codex_turn(raw: &str) -> Option<String> {
    let mut latest = None;
    for line in raw.lines() {
        let Ok(obj) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if obj.get("type").and_then(Value::as_str) != Some("event_msg") {
            continue;
        }
        let payload = obj.get("payload").unwrap_or(&Value::Null);
        if payload.get("type").and_then(Value::as_str) != Some("task_complete") {
            continue;
        }
        if let Some(text) = payload.get("last_agent_message").and_then(Value::as_str) {
            if !text.trim().is_empty() {
                latest = Some(text.trim().to_string());
            }
        }
    }
    latest
}

fn raw_after_byte(raw: &str, baseline_byte_len: u64) -> &str {
    let offset = baseline_byte_len.min(raw.len() as u64) as usize;
    if raw.is_char_boundary(offset) {
        &raw[offset..]
    } else {
        let next = raw
            .char_indices()
            .map(|(idx, _)| idx)
            .find(|idx| *idx > offset)
            .unwrap_or(raw.len());
        &raw[next..]
    }
}

fn parse_codex_turn_after(raw: &str, baseline_byte_len: u64) -> Option<String> {
    parse_codex_turn(raw_after_byte(raw, baseline_byte_len))
}

fn codex_ref_from_rollout(path: &Path) -> Option<ConversationRef> {
    let stem = path.file_stem()?.to_str()?;
    let id = stem.strip_prefix("rollout-").unwrap_or(stem);
    if id.trim().is_empty() {
        None
    } else {
        Some(ConversationRef::CodexThread(id.to_string()))
    }
}

async fn wait_for_codex_turn(
    cwd: &str,
    spawned_at: SystemTime,
    baseline: Option<NativeTranscriptBaseline>,
) -> Result<NativeTranscriptResult, String> {
    let deadline = Instant::now() + Duration::from_secs(1800);
    let mut rollout = None;
    while Instant::now() < deadline {
        if rollout.is_none() {
            rollout = find_codex_rollout(cwd, spawned_at).await;
        }
        if let Some(path) = rollout.as_ref() {
            if let Ok(raw) = fs::read_to_string(path).await {
                let baseline_len = matching_baseline_len(path, baseline.as_ref());
                if let Some(text) = parse_codex_turn_after(&raw, baseline_len) {
                    sleep(Duration::from_secs(3)).await;
                    let final_text = fs::read_to_string(path)
                        .await
                        .ok()
                        .and_then(|again| parse_codex_turn_after(&again, baseline_len))
                        .unwrap_or(text);
                    return Ok(NativeTranscriptResult {
                        text: final_text,
                        conversation_ref: codex_ref_from_rollout(path),
                    });
                }
            }
        }
        sleep(Duration::from_millis(1000)).await;
    }
    Err("Timed out waiting for Codex transcript completion".to_string())
}

fn matching_baseline_len(path: &Path, baseline: Option<&NativeTranscriptBaseline>) -> u64 {
    baseline
        .filter(|baseline| baseline.path == path)
        .map(|baseline| baseline.byte_len)
        .unwrap_or(0)
}

async fn find_gemini_session(cwd: &str, spawned_at: SystemTime) -> Option<PathBuf> {
    let target_cwd = normalize_path(cwd);
    let root = home_dir().ok()?.join(".gemini").join("tmp");
    let mut projects = fs::read_dir(root).await.ok()?;
    let mut best: Option<(PathBuf, Duration)> = None;

    while let Ok(Some(project)) = projects.next_entry().await {
        let project_dir = project.path();
        let Ok(project_root) = fs::read_to_string(project_dir.join(".project_root")).await else {
            continue;
        };
        if normalize_path(project_root.trim()) != target_cwd {
            continue;
        }
        let chats_dir = project_dir.join("chats");
        let mut chats = match fs::read_dir(chats_dir).await {
            Ok(chats) => chats,
            Err(_) => continue,
        };
        while let Ok(Some(chat)) = chats.next_entry().await {
            let path = chat.path();
            let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
            if !name.starts_with("session-")
                || !(name.ends_with(".jsonl") || name.ends_with(".json"))
            {
                continue;
            }
            let Ok(metadata) = chat.metadata().await else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            let delta = if modified >= spawned_at {
                modified.duration_since(spawned_at).ok()?
            } else {
                spawned_at.duration_since(modified).ok()?
            };
            if delta > Duration::from_secs(300) {
                continue;
            }
            if best
                .as_ref()
                .map(|(_, current)| delta < *current)
                .unwrap_or(true)
            {
                best = Some((path, delta));
            }
        }
    }

    best.map(|(path, _)| path)
}

async fn find_latest_gemini_session(cwd: &str) -> Option<PathBuf> {
    let target_cwd = normalize_path(cwd);
    let root = home_dir().ok()?.join(".gemini").join("tmp");
    let mut projects = fs::read_dir(root).await.ok()?;
    let mut best: Option<(PathBuf, SystemTime)> = None;

    while let Ok(Some(project)) = projects.next_entry().await {
        let project_dir = project.path();
        let Ok(project_root) = fs::read_to_string(project_dir.join(".project_root")).await else {
            continue;
        };
        if normalize_path(project_root.trim()) != target_cwd {
            continue;
        }
        let chats_dir = project_dir.join("chats");
        let mut chats = match fs::read_dir(chats_dir).await {
            Ok(chats) => chats,
            Err(_) => continue,
        };
        while let Ok(Some(chat)) = chats.next_entry().await {
            let path = chat.path();
            let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
            if !name.starts_with("session-")
                || !(name.ends_with(".jsonl") || name.ends_with(".json"))
            {
                continue;
            }
            let Ok(metadata) = chat.metadata().await else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            if best
                .as_ref()
                .map(|(_, current)| modified > *current)
                .unwrap_or(true)
            {
                best = Some((path, modified));
            }
        }
    }

    best.map(|(path, _)| path)
}

fn parse_gemini_turn(raw: &str) -> Option<String> {
    let mut chunks: Vec<String> = vec![];
    let mut complete = false;
    let lines: Vec<&str> = if raw.contains('\n') {
        raw.lines().collect()
    } else {
        vec![raw]
    };
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(obj) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let typ = obj.get("type").and_then(Value::as_str).unwrap_or("");
        if typ == "gemini" {
            if let Some(content) = obj.get("content").and_then(Value::as_str) {
                if !content.trim().is_empty() && chunks.last().map(String::as_str) != Some(content)
                {
                    chunks.push(content.to_string());
                }
            }
            if obj
                .get("tokens")
                .and_then(|v| v.get("total"))
                .and_then(Value::as_i64)
                .is_some()
            {
                complete = true;
            }
        } else if typ == "message_update" || typ == "result" {
            if let Some(text) = text_from_value(&obj) {
                chunks.clear();
                chunks.push(text);
            }
            if typ == "result" || obj.get("status").and_then(Value::as_str) == Some("finalized") {
                complete = true;
            }
        }
    }
    let text = chunks.join("").trim().to_string();
    if complete && !text.is_empty() {
        Some(text)
    } else {
        None
    }
}

fn parse_gemini_turn_after(raw: &str, baseline_byte_len: u64) -> Option<String> {
    parse_gemini_turn(raw_after_byte(raw, baseline_byte_len))
}

async fn wait_for_gemini_turn(
    cwd: &str,
    spawned_at: SystemTime,
    baseline: Option<NativeTranscriptBaseline>,
) -> Result<NativeTranscriptResult, String> {
    let deadline = Instant::now() + Duration::from_secs(1800);
    let mut session = None;
    while Instant::now() < deadline {
        if session.is_none() {
            session = find_gemini_session(cwd, spawned_at).await;
        }
        if let Some(path) = session.as_ref() {
            if let Ok(raw) = fs::read_to_string(path).await {
                let baseline_len = matching_baseline_len(path, baseline.as_ref());
                if let Some(text) = parse_gemini_turn_after(&raw, baseline_len) {
                    return Ok(NativeTranscriptResult {
                        text,
                        conversation_ref: None,
                    });
                }
            }
        }
        sleep(Duration::from_millis(1000)).await;
    }
    Err("Timed out waiting for Gemini transcript completion".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_baseline_len_only_applies_to_same_path() {
        let baseline = NativeTranscriptBaseline {
            path: PathBuf::from("same.jsonl"),
            byte_len: 42,
        };

        assert_eq!(matching_baseline_len(Path::new("same.jsonl"), Some(&baseline)), 42);
        assert_eq!(matching_baseline_len(Path::new("other.jsonl"), Some(&baseline)), 0);
    }

    #[test]
    fn codex_parser_prefers_latest_completion_after_baseline_in_reused_rollout() {
        let raw = concat!(
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"last_agent_message\":\"old\"}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"next\"}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"last_agent_message\":\"new-1\"}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"last_agent_message\":\"new-2\"}}\n"
        );
        let baseline = raw.find("{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\"")
            .unwrap() as u64;

        assert_eq!(parse_codex_turn_after(raw, baseline).as_deref(), Some("new-2"));
    }

    #[test]
    fn gemini_parser_prefers_latest_finalized_completion_after_baseline_in_reused_session() {
        let raw = concat!(
            "{\"type\":\"gemini\",\"content\":\"old\",\"tokens\":{\"total\":10}}\n",
            "{\"type\":\"user\",\"content\":[{\"text\":\"next\"}]}\n",
            "{\"type\":\"message_update\",\"content\":\"new-1\",\"status\":\"finalized\"}\n",
            "{\"type\":\"result\",\"response\":\"new-2\"}\n"
        );
        let baseline = raw.find("{\"type\":\"user\",\"content\"").unwrap() as u64;

        assert_eq!(parse_gemini_turn_after(raw, baseline).as_deref(), Some("new-2"));
    }

    #[test]
    fn parses_latest_codex_task_complete() {
        let raw = r#"{"type":"event_msg","payload":{"type":"task_complete","last_agent_message":"first"}}
{"type":"event_msg","payload":{"type":"task_complete","last_agent_message":"second"}}"#;
        assert_eq!(parse_codex_turn(raw).as_deref(), Some("second"));
    }

    #[test]
    fn codex_turn_parser_ignores_completed_answers_before_baseline() {
        let old =
            r#"{"type":"event_msg","payload":{"type":"task_complete","last_agent_message":"old"}}"#;
        let user_only =
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"new prompt"}}"#;
        let raw_without_new_complete = format!("{old}\n{user_only}\n");
        let baseline = old.len() as u64 + 1;

        assert_eq!(
            parse_codex_turn_after(&raw_without_new_complete, baseline),
            None
        );

        let raw_with_new_complete = format!(
            "{raw_without_new_complete}{}\n",
            r#"{"type":"event_msg","payload":{"type":"task_complete","last_agent_message":"new"}}"#
        );
        assert_eq!(
            parse_codex_turn_after(&raw_with_new_complete, baseline).as_deref(),
            Some("new")
        );
    }

    #[test]
    fn parses_gemini_tokens_complete_chunks() {
        let raw = r#"{"type":"gemini","content":"hello "}
{"type":"gemini","content":"world","tokens":{"total":12}}"#;
        assert_eq!(parse_gemini_turn(raw).as_deref(), Some("hello world"));
    }

    #[test]
    fn ignores_gemini_user_prompt_lines() {
        let raw = r#"{"sessionId":"7d205018-3212-4bd0-927c-92518fc45c9a","kind":"main"}
{"type":"user","content":[{"text":"Reply with exactly OCV_SMOKE_OK and nothing else."}]}
{"$set":{"lastUpdated":"2026-05-05T02:08:27.913Z"}}
{"type":"gemini","content":"OCV_SMOKE_OK","tokens":{"total":17313}}"#;
        assert_eq!(parse_gemini_turn(raw).as_deref(), Some("OCV_SMOKE_OK"));
    }

    #[test]
    fn ignores_incomplete_gemini_chunks() {
        let raw = r#"{"type":"gemini","content":"hello"}"#;
        assert_eq!(parse_gemini_turn(raw), None);
    }

    #[test]
    fn gemini_turn_parser_ignores_completed_answers_before_baseline() {
        let old = r#"{"type":"gemini","content":"old","tokens":{"total":10}}"#;
        let user_only = r#"{"type":"user","content":[{"text":"new prompt"}]}"#;
        let raw_without_new_complete = format!("{old}\n{user_only}\n");
        let baseline = old.len() as u64 + 1;

        assert_eq!(
            parse_gemini_turn_after(&raw_without_new_complete, baseline),
            None
        );

        let raw_with_new_complete = format!(
            "{raw_without_new_complete}{}\n",
            r#"{"type":"gemini","content":"new","tokens":{"total":11}}"#
        );
        assert_eq!(
            parse_gemini_turn_after(&raw_with_new_complete, baseline).as_deref(),
            Some("new")
        );
    }
}
