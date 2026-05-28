use crate::models::{MemoryExtractionConfig, MemoryNode, MemorySource};
use crate::storage::memory_store;
use crate::storage::settings;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::Mutex;
use std::time::Instant;

static LOG_WRITER: Lazy<Mutex<Option<BufWriter<File>>>> = Lazy::new(|| Mutex::new(None));

pub fn log_to_file(msg: &str) {
    let mut guard = LOG_WRITER.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        let data_dir = crate::storage::data_dir();
        let log_dir = data_dir.join("logs");
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join("memory-extraction.log");
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(f) => *guard = Some(BufWriter::new(f)),
            Err(_) => return,
        }
    }
    if let Some(ref mut w) = *guard {
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(w, "[{ts}] {msg}");
    }
}

static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(reqwest::Client::new);

// Debounce: per group-chat, last extraction time
static LAST_EXTRACTION: Lazy<Mutex<HashMap<String, Instant>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// Daily caps: count of extractions today
static DAILY_EXTRACTION_COUNT: Lazy<Mutex<(String, u32)>> =
    Lazy::new(|| Mutex::new((String::new(), 0)));

pub fn can_extract(group_chat_id: &str) -> bool {
    // Debounce: 5 min per group_chat
    {
        let map = LAST_EXTRACTION.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(last) = map.get(group_chat_id) {
            if last.elapsed().as_secs() < 300 {
                return false;
            }
        }
    }

    // Daily cap: 50 per day (global, not per-character)
    {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let mut guard = DAILY_EXTRACTION_COUNT.lock().unwrap_or_else(|e| e.into_inner());
        if guard.0 != today {
            guard.0 = today;
            guard.1 = 0;
        }
        if guard.1 >= 50 {
            return false;
        }
    }

    true
}

pub fn record_extraction(group_chat_id: &str) {
    {
        let mut map = LAST_EXTRACTION.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(group_chat_id.to_string(), Instant::now());
    }
    {
        let mut guard = DAILY_EXTRACTION_COUNT.lock().unwrap_or_else(|e| e.into_inner());
        guard.1 += 1;
    }
}

/// Get extraction config from settings.
/// Reads `embedding_config` and converts to MemoryExtractionConfig.
fn get_extraction_config() -> Option<MemoryExtractionConfig> {
    let all = settings::load();

    let Some(legacy) = &all.user.embedding_config else {
        return None;
    };
    if !legacy.enabled {
        return None;
    }

    let chat_endpoint = legacy
        .chat_endpoint
        .clone()
        .unwrap_or_else(|| derive_chat_endpoint(&legacy.endpoint));
    let chat_model = legacy
        .chat_model
        .clone()
        .unwrap_or_else(|| legacy.model.clone());
    let chat_api_key = legacy
        .chat_api_key
        .clone()
        .or_else(|| legacy.api_key.clone());

    Some(MemoryExtractionConfig {
        enabled: true,
        chat_endpoint: Some(chat_endpoint),
        chat_model: Some(chat_model),
        chat_api_key,
    })
}

/// Auto-extract memories from group chat turns via LLM.
/// Returns extracted MemoryNodes (stored in SQLite, not per-character JSONL).
pub async fn auto_extract_memories(turns: &[String]) -> Vec<MemoryNode> {
    log_to_file(&format!(
        "[memory-extraction] ENTER auto_extract_memories turns={}",
        turns.len()
    ));

    let Some(config) = get_extraction_config() else {
        log_to_file("[memory-extraction] SKIP no extraction config");
        return Vec::new();
    };

    let Some(ref api_key) = config.chat_api_key else {
        log_to_file("[memory-extraction] SKIP no api_key");
        return Vec::new();
    };
    let chat_endpoint = config
        .chat_endpoint
        .as_deref()
        .unwrap_or("http://localhost:8080/v1/chat/completions");
    let chat_model = config.chat_model.as_deref().unwrap_or("gpt-4o-mini");

    // Build conversation text
    let conversation: String = turns
        .iter()
        .enumerate()
        .map(|(i, t)| format!("Turn {}: {}", i + 1, t))
        .collect::<Vec<_>>()
        .join("\n\n");

    let truncated_conv: String = conversation.chars().take(4000).collect();
    let prompt = format!(
        r#"分析以下群聊对话，提取关于用户的信息（身份、偏好、技能、规则）。
每条对话前标注了 [说话人]，请提取用户本人表达的内容或从对话中可以推断出的关于用户的事实。

返回一个 JSON 数组。每个对象包含：
- "content"：提取的信息（简洁，一句话）
- "type"：以下之一 "fact"、"preference"、"skill"、"feedback"
- "tags"：相关关键词数组
- "confidence"：0-100 的整数，表示这条信息的置信度

要求：
- 内容必须使用与对话相同的语言
- 只返回 JSON 数组，不要其他文本。如果没有值得提取的内容，返回 []

对话：
{text}"#,
        text = truncated_conv
    );

    let body = serde_json::json!({
        "model": chat_model,
        "messages": [
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.1,
        "max_tokens": 2000,
    });

    log_to_file(&format!(
        "[memory-extraction] LLM request: endpoint={} model={}",
        chat_endpoint, chat_model
    ));

    let resp = match HTTP_CLIENT
        .post(chat_endpoint)
        .timeout(std::time::Duration::from_secs(30))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log_to_file(&format!("[memory-extraction] LLM_HTTP_ERROR err={}", e));
            return Vec::new();
        }
    };

    let status = resp.status();
    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        log_to_file(&format!(
            "[memory-extraction] LLM_HTTP_ERROR status={} body={}",
            status,
            body_text.chars().take(500).collect::<String>()
        ));
        return Vec::new();
    }

    let resp_text = resp.text().await.unwrap_or_default();
    log_to_file(&format!(
        "[memory-extraction] LLM_RAW_RESPONSE body={}",
        resp_text.chars().take(500).collect::<String>()
    ));

    let json: Value = match serde_json::from_str(&resp_text) {
        Ok(v) => v,
        Err(e) => {
            log_to_file(&format!("[memory-extraction] LLM_JSON_PARSE_ERROR err={}", e));
            return Vec::new();
        }
    };

    let message = &json["choices"][0]["message"];
    let finish_reason = json["choices"][0]["finish_reason"]
        .as_str()
        .unwrap_or("");
    let content = message["content"]
        .as_str()
        .filter(|c| !c.trim().is_empty());
    let Some(content) = content else {
        let reasoning_len = message["reasoning_content"]
            .as_str()
            .map(|s| s.len())
            .unwrap_or(0);
        log_to_file(&format!(
            "[memory-extraction] LLM_NO_CONTENT finish_reason={} reasoning_len={}",
            finish_reason, reasoning_len
        ));
        return Vec::new();
    };
    log_to_file(&format!(
        "[memory-extraction] LLM_GOT_CONTENT len={}",
        content.len()
    ));

    let json_str = content.trim();
    let json_str = json_str
        .strip_prefix("```json")
        .or_else(|| json_str.strip_prefix("```"))
        .unwrap_or(json_str)
        .trim_end_matches("```")
        .trim();

    let items: Vec<Value> = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            log_to_file(&format!(
                "[memory-extraction] JSON_PARSE_ERROR err={} json_str={}",
                e,
                json_str.chars().take(300).collect::<String>()
            ));
            return Vec::new();
        }
    };
    log_to_file(&format!(
        "[memory-extraction] PARSED_ITEMS count={}",
        items.len()
    ));

    let now = crate::models::now_iso();
    let mut results = Vec::new();

    for item in &items {
        let content_text = match item["content"].as_str() {
            Some(c) if !c.trim().is_empty() => c.trim().to_string(),
            _ => continue,
        };

        let raw_type = item["type"].as_str().unwrap_or("fact");
        let memory_type = match raw_type {
            "fact" | "preference" | "skill" | "feedback" => raw_type.to_string(),
            _ => "fact".to_string(),
        };

        let confidence = item["confidence"]
            .as_f64()
            .unwrap_or(70.0)
            .clamp(0.0, 100.0);

        let tags: Vec<String> = item["tags"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // FTS5 dedup: skip if a very similar memory already exists
        if memory_store::find_duplicates(&content_text, 0.8)
            .map(|dups| !dups.is_empty())
            .unwrap_or(false)
        {
            log_to_file(&format!(
                "[memory-extraction] SKIP duplicate preview={}",
                &content_text.chars().take(50).collect::<String>()
            ));
            continue;
        }

        let memory = MemoryNode {
            id: uuid::Uuid::new_v4().to_string(),
            character_id: String::new(),
            content: content_text,
            memory_type,
            confidence,
            source: MemorySource {
                kind: "auto_extract".to_string(),
                run_id: None,
                group_chat_id: None,
            },
            tags,
            created_at: now.clone(),
            updated_at: now.clone(),
            status: "pending".to_string(),
        };

        // Store in SQLite
        if let Err(e) = memory_store::insert_memory(&memory) {
            log_to_file(&format!("[memory-extraction] DB_INSERT_ERROR err={}", e));
            continue;
        }

        results.push(memory);
    }

    log_to_file(&format!(
        "[memory-extraction] RESULT extracted={} after_dedup={}",
        items.len(),
        results.len()
    ));
    results
}

fn derive_chat_endpoint(embedding_endpoint: &str) -> String {
    if embedding_endpoint.ends_with("/embeddings") {
        format!(
            "{}/chat/completions",
            &embedding_endpoint[..embedding_endpoint.len() - "/embeddings".len()]
        )
    } else {
        format!(
            "{}/chat/completions",
            embedding_endpoint.trim_end_matches('/')
        )
    }
}
