# Event Watch 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 openInvest Event Watch — 从 Tushare 获取上市公司新闻和公告，经规则初筛 + LLM 归一化后存储，提供事件流 UI 和委员会触发能力。

**Architecture:** 独立 Rust 模块 `invest/event_scanner.rs`，复用 `TushareClient`（HTTP）+ `InvestLlmClient`（OpenAI 兼容协议）。事件存入已有 `events` 表（新增 `stance` 字段）。前端 `EventWatchTab` 组件替换占位符。

**Tech Stack:** Rust (TushareClient, InvestLlmClient, tokio), Svelte 5 runes, SQLite (invest.db), Tauri IPC

**Design Spec:** `docs/superpowers/specs/2026-05-29-event-watch-design.md`

---

## 文件清单

### 新增文件

| 文件 | 职责 |
|------|------|
| `src-tauri/src/invest/event_scanner.rs` | 事件扫描器：规则初筛 + LLM 归一化 + 扫描编排 |
| `src/lib/components/invest/EventWatchTab.svelte` | 事件监控 Tab UI |
| `src/lib/components/invest/EventTriggerDialog.svelte` | 触发委员会确认对话框 |

### 修改文件

| 文件 | 改动 |
|------|------|
| `src-tauri/src/tushare/client.rs` | 新增 `MajorNewsItem`/`Announcement` structs + `major_news()`/`anns_d()` 方法 |
| `src-tauri/src/storage/invest/mod.rs` | `init_db` 加 `stance` 列 migration |
| `src-tauri/src/storage/invest/events.rs` | `Event` struct 加 `stance` 字段，更新 `save_event` SQL |
| `src-tauri/src/invest/mod.rs` | 注册 `event_scanner` 子模块 |
| `src-tauri/src/commands/invest.rs` | 新增 `scan_events` / `get_scan_status` Tauri 命令 |
| `src-tauri/src/lib.rs` | 注册新命令 + 启动事件扫描 cron |
| `src/routes/invest/+page.svelte` | 替换 events 占位符为 EventWatchTab |
| `src/lib/stores/invest-store.svelte.ts` | 新增事件相关状态和方法 |
| `messages/en.json` | 新增 `invest_event_*` i18n keys |
| `messages/zh-CN.json` | 新增 `invest_event_*` i18n keys |

---

### Task 1: Tushare API — `major_news` + `anns_d`

**Files:**
- Modify: `src-tauri/src/tushare/client.rs`

- [ ] **Step 1: 新增 `MajorNewsItem` struct**

在 `client.rs` 的 domain structs 区域（`TradeCal` 之后）添加：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MajorNewsItem {
    pub datetime: String,
    pub title: String,
    pub content: String,
    pub src: String,
}
```

- [ ] **Step 2: 新增 `Announcement` struct**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announcement {
    pub ann_date: String,
    pub ts_code: String,
    pub name: String,
    pub title: String,
    pub url: String,
}
```

- [ ] **Step 3: 新增 `major_news()` 方法**

在 `impl TushareClient` 中添加（遵循现有 `daily()` 的 5 步模式）：

```rust
pub async fn major_news(&self, src: &str, start_date: &str, end_date: &str) -> Result<Vec<MajorNewsItem>, String> {
    let params = serde_json::json!({
        "src": src,
        "start_date": start_date,
        "end_date": end_date
    });
    let resp = self.call_api("major_news", params, "").await?;

    let fields = &resp.data.fields;
    let datetime_idx = fields.iter().position(|f| f == "datetime");
    let title_idx = fields.iter().position(|f| f == "title");
    let content_idx = fields.iter().position(|f| f == "content");
    let src_idx = fields.iter().position(|f| f == "src");

    let mut items = Vec::with_capacity(resp.data.items.len());
    for row in &resp.data.items {
        items.push(MajorNewsItem {
            datetime: datetime_idx.and_then(|i| get_str(row, i)).unwrap_or_default(),
            title: title_idx.and_then(|i| get_str(row, i)).unwrap_or_default(),
            content: content_idx.and_then(|i| get_str(row, i)).unwrap_or_default(),
            src: src_idx.and_then(|i| get_str(row, i)).unwrap_or_default(),
        });
    }
    Ok(items)
}
```

- [ ] **Step 4: 新增 `anns_d()` 方法**

```rust
pub async fn anns_d(&self, ts_code: &str, start_date: &str, end_date: &str) -> Result<Vec<Announcement>, String> {
    let params = serde_json::json!({
        "ts_code": ts_code,
        "start_date": start_date,
        "end_date": end_date
    });
    let resp = self.call_api("anns_d", params, "").await?;

    let fields = &resp.data.fields;
    let ann_date_idx = fields.iter().position(|f| f == "ann_date");
    let ts_code_idx = fields.iter().position(|f| f == "ts_code");
    let name_idx = fields.iter().position(|f| f == "name");
    let title_idx = fields.iter().position(|f| f == "title");
    let url_idx = fields.iter().position(|f| f == "url");

    let mut items = Vec::with_capacity(resp.data.items.len());
    for row in &resp.data.items {
        items.push(Announcement {
            ann_date: ann_date_idx.and_then(|i| get_str(row, i)).unwrap_or_default(),
            ts_code: ts_code_idx.and_then(|i| get_str(row, i)).unwrap_or_default(),
            name: name_idx.and_then(|i| get_str(row, i)).unwrap_or_default(),
            title: title_idx.and_then(|i| get_str(row, i)).unwrap_or_default(),
            url: url_idx.and_then(|i| get_str(row, i)).unwrap_or_default(),
        });
    }
    Ok(items)
}
```

- [ ] **Step 5: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 无错误

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/tushare/client.rs
git commit -m "feat(invest): add Tushare major_news and anns_d API methods"
```

---

### Task 2: Storage — `stance` 字段 migration + Event struct 更新

**Files:**
- Modify: `src-tauri/src/storage/invest/mod.rs`
- Modify: `src-tauri/src/storage/invest/events.rs`

- [ ] **Step 1: 在 `init_db` 中添加 stance migration**

在 `src-tauri/src/storage/invest/mod.rs` 的 `init_db` 函数中，在 `CREATE_TABLES_SQL` 执行之后、trades migration 之前添加：

```rust
// Migration: add stance column to events table if missing
{
    let has_stance: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('events') WHERE name='stance'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if has_stance == 0 {
        conn.execute_batch("ALTER TABLE events ADD COLUMN stance TEXT DEFAULT 'neutral';")
            .map_err(|e| format!("Failed to add stance column: {}", e))?;
    }
}
```

- [ ] **Step 2: 更新 Event struct**

在 `src-tauri/src/storage/invest/events.rs` 的 `Event` struct 中添加 `stance` 字段：

```rust
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub source: String,
    pub event_type: String,
    pub title: String,
    pub body: Option<String>,
    pub symbols: Option<String>,
    pub severity: String,
    pub stance: String,
    pub triggered: bool,
    pub trigger_verdict_id: Option<String>,
    pub created_at: String,
}
```

- [ ] **Step 3: 更新 `save_event` SQL**

在 `events.rs` 的 `save_event` 函数中，更新 INSERT SQL 以包含 `stance` 列：

```rust
conn.execute(
    "INSERT OR REPLACE INTO events (id, source, event_type, title, body, symbols, severity, stance, triggered, trigger_verdict_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    rusqlite::params![
        e.id, e.source, e.event_type, e.title, e.body, e.symbols, e.severity, e.stance, e.triggered as i32, e.trigger_verdict_id, e.created_at
    ],
)
```

- [ ] **Step 4: 更新 `list_events` 查询**

在 `list_events` 函数中，确保 SELECT 包含 `stance` 列。检查现有 SQL 的列列表，添加 `stance`。

- [ ] **Step 5: 更新 Tauri 命令 `save_event`**

在 `src-tauri/src/commands/invest.rs` 的 `save_event` 命令中，添加 `stance` 参数：

```rust
#[tauri::command]
pub fn save_event(
    id: Option<String>,
    source: String,
    event_type: String,
    title: String,
    body: Option<String>,
    symbols: Option<String>,
    severity: Option<String>,
    stance: Option<String>,
) -> Result<(), String> {
    let e = Event {
        id: id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        source,
        event_type,
        title,
        body,
        symbols,
        severity: severity.unwrap_or_else(|| "info".to_string()),
        stance: stance.unwrap_or_else(|| "neutral".to_string()),
        triggered: false,
        trigger_verdict_id: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    crate::storage::invest::events::save_event(&e)
}
```

- [ ] **Step 6: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 无错误

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/storage/invest/mod.rs src-tauri/src/storage/invest/events.rs src-tauri/src/commands/invest.rs
git commit -m "feat(invest): add stance field to events table with migration"
```

---

### Task 3: Event Scanner 模块 — 规则初筛

**Files:**
- Create: `src-tauri/src/invest/event_scanner.rs`
- Modify: `src-tauri/src/invest/mod.rs`

- [ ] **Step 1: 创建 event_scanner.rs 骨架**

创建 `src-tauri/src/invest/event_scanner.rs`：

```rust
use serde::{Deserialize, Serialize};

/// Severity classification from rule-based keyword filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    High,
    Medium,
}

/// Result of LLM normalization for a single event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedEvent {
    pub one_line_claim: String,
    pub stance: String,
    pub severity: String,
    pub affected_symbols: Vec<String>,
}

/// Raw event before normalization.
#[derive(Debug, Clone)]
pub struct RawEvent {
    pub source: String,
    pub event_type: String,
    pub title: String,
    pub body: String,
    pub url: Option<String>,
    pub created_at: String,
}

// ── Rule-based keyword filtering ──

const HIGH_KEYWORDS: &[&str] = &[
    "央行", "降准", "降息", "加息", "MLF", "LPR", "逆回购",
    "暴跌", "熔断", "ST", "退市", "暂停上市", "重大违法",
    "关税", "制裁", "禁令", "反垄断", "行业整顿",
];

const MEDIUM_KEYWORDS: &[&str] = &[
    "财报", "业绩预告", "净利润", "营收",
    "增持", "减持", "回购", "定增", "分红",
    "产能", "订单", "并购", "重组",
];

/// Classify severity by keyword matching.
/// Returns None for LOW (irrelevant) events that should be filtered out.
pub fn classify_severity(title: &str, body: &str) -> Option<Severity> {
    let text = format!("{} {}", title, body);
    if HIGH_KEYWORDS.iter().any(|k| text.contains(k)) {
        Some(Severity::High)
    } else if MEDIUM_KEYWORDS.iter().any(|k| text.contains(k)) {
        Some(Severity::Medium)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_high() {
        assert_eq!(classify_severity("央行宣布降准50个基点", ""), Some(Severity::High));
        assert_eq!(classify_severity("某股暴跌触发熔断", ""), Some(Severity::High));
    }

    #[test]
    fn test_classify_medium() {
        assert_eq!(classify_severity("公司发布财报", ""), Some(Severity::Medium));
        assert_eq!(classify_severity("大股东减持公告", ""), Some(Severity::Medium));
    }

    #[test]
    fn test_classify_low_filtered() {
        assert_eq!(classify_severity("今日天气晴朗", ""), None);
        assert_eq!(classify_severity("体育新闻", ""), None);
    }
}
```

- [ ] **Step 2: 注册模块**

在 `src-tauri/src/invest/mod.rs` 中添加：

```rust
pub mod event_scanner;
```

- [ ] **Step 3: 运行测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::event_scanner::tests -- --nocapture`
Expected: 3 tests PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/invest/event_scanner.rs src-tauri/src/invest/mod.rs
git commit -m "feat(invest): add event scanner module with keyword severity filter"
```

---

### Task 4: Event Scanner — LLM 归一化

**Files:**
- Modify: `src-tauri/src/invest/event_scanner.rs`

- [ ] **Step 1: 添加 LLM 归一化函数**

在 `event_scanner.rs` 中添加（依赖 `InvestLlmClient`、`collect_stream`、`LlmConfig`、`Message`）：

```rust
use crate::invest::llm::{InvestLlmClient, LlmConfig, Message, collect_stream};

/// Default system prompt for event normalization.
const DEFAULT_NORMALIZER_PROMPT: &str = r#"你是一个A股财经新闻分析师。对以下新闻/公告进行结构化提取。

对每条新闻输出一个JSON数组，每个元素包含：
- one_line_claim: 一句话摘要（≤30字）
- stance: bullish / bearish / neutral
- severity: high / medium / low
- affected_symbols: 涉及的A股代码数组（6位数字格式，如 "600519"）

只输出JSON数组，不要其他文字。"#;

/// Normalize a batch of raw events using LLM.
/// Returns normalized results in the same order as input.
/// Falls back to rule-based severity on parse failure.
pub async fn normalize_events(
    client: &dyn InvestLlmClient,
    config: &LlmConfig,
    raw_events: &[RawEvent],
    system_prompt: Option<&str>,
) -> Vec<NormalizedEvent> {
    if raw_events.is_empty() {
        return vec![];
    }

    // Build batch prompt
    let mut items = String::new();
    for (i, ev) in raw_events.iter().enumerate() {
        items.push_str(&format!(
            "\n[{}] source={} title={}\n{}\n",
            i + 1,
            ev.source,
            ev.title,
            if ev.body.is_empty() { &ev.title } else { &ev.body }
        ));
    }

    let system = system_prompt.unwrap_or(DEFAULT_NORMALIZER_PROMPT);
    let messages = vec![Message {
        role: "user".to_string(),
        content: Some(items),
        tool_call_id: None,
        tool_calls: None,
        name: None,
    }];

    // Call LLM
    let stream = match client.chat_stream(system, &messages, None, config).await {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Event normalizer LLM call failed: {}, falling back to rule-based", e);
            return raw_events.iter().map(|ev| fallback_normalize(ev)).collect();
        }
    };

    let collected = collect_stream(stream).await;
    let content = collected.content.unwrap_or_default();

    // Parse JSON response
    parse_normalized_response(&content, raw_events)
}

/// Parse LLM JSON response, matching results to raw events by index.
fn parse_normalized_response(content: &str, raw_events: &[RawEvent]) -> Vec<NormalizedEvent> {
    // Extract JSON array from response (handle markdown code blocks)
    let json_str = content.trim();
    let json_str = if json_str.starts_with("```") {
        json_str
            .lines()
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end_matches("```")
            .trim()
            .to_string()
    } else {
        json_str.to_string()
    };

    match serde_json::from_str::<Vec<NormalizedEvent>>(&json_str) {
        Ok(results) => {
            // Pad or truncate to match input length
            let mut normalized = results;
            normalized.truncate(raw_events.len());
            while normalized.len() < raw_events.len() {
                let idx = normalized.len();
                normalized.push(fallback_normalize(&raw_events[idx]));
            }
            normalized
        }
        Err(e) => {
            log::warn!("Failed to parse normalizer response: {}, falling back to rule-based", e);
            raw_events.iter().map(|ev| fallback_normalize(ev)).collect()
        }
    }
}

/// Fallback: use rule-based severity, neutral stance, no symbols.
fn fallback_normalize(ev: &RawEvent) -> NormalizedEvent {
    let severity = classify_severity(&ev.title, &ev.body)
        .map(|s| match s {
            Severity::High => "high",
            Severity::Medium => "medium",
        })
        .unwrap_or("low")
        .to_string();

    NormalizedEvent {
        one_line_claim: ev.title.chars().take(30).collect(),
        stance: "neutral".to_string(),
        severity,
        affected_symbols: vec![],
    }
}

#[cfg(test)]
mod tests {
    // ... existing tests ...

    #[test]
    fn test_parse_normalized_response() {
        let raw = vec![RawEvent {
            source: "test".into(),
            event_type: "news".into(),
            title: "央行降准".into(),
            body: "央行宣布降准50个基点".into(),
            url: None,
            created_at: "2026-05-29T10:00:00Z".into(),
        }];
        let json = r#"[{"one_line_claim":"央行降准50基点","stance":"bullish","severity":"high","affected_symbols":["600519"]}"#;
        let result = parse_normalized_response(json, &raw);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].stance, "bullish");
        assert_eq!(result[0].affected_symbols, vec!["600519"]);
    }
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 无错误（`collect_stream` 和相关类型需确认在 `invest::llm` 模块中可用）

- [ ] **Step 3: 运行测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::event_scanner::tests -- --nocapture`
Expected: 4 tests PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/invest/event_scanner.rs
git commit -m "feat(invest): add LLM event normalization with batch processing"
```

---

### Task 5: Event Scanner — 扫描编排

**Files:**
- Modify: `src-tauri/src/invest/event_scanner.rs`

- [ ] **Step 1: 添加扫描编排函数**

在 `event_scanner.rs` 中添加：

```rust
use crate::tushare::TushareClient;
use crate::storage::invest::events::{Event, save_event};
use crate::storage::invest::portfolio;

/// Run a full event scan: fetch → filter → normalize → store.
/// Returns the number of new events saved.
pub async fn run_event_scan(
    tushare_token: &str,
    llm_client: &dyn InvestLlmClient,
    llm_config: &LlmConfig,
    system_prompt: Option<&str>,
) -> Result<usize, String> {
    let client = TushareClient::new(tushare_token.to_string());
    let now = chrono::Local::now();

    // 1. Fetch raw events from both sources
    let mut raw_events: Vec<RawEvent> = Vec::new();

    // major_news: last 2 hours, sina + cls sources
    let two_hours_ago = (now - chrono::Duration::hours(2)).format("%Y%m%d%H%M%S").to_string();
    let today = now.format("%Y%m%d").to_string();
    for src in &["sina", "cls"] {
        match client.major_news(src, &two_hours_ago, &today).await {
            Ok(items) => {
                for item in items {
                    raw_events.push(RawEvent {
                        source: "tushare_major_news".into(),
                        event_type: "news".into(),
                        title: item.title,
                        body: item.content,
                        url: None,
                        created_at: item.datetime,
                    });
                }
            }
            Err(e) => {
                log::warn!("major_news({}) failed: {}", src, e);
            }
        }
    }

    // anns_d: last 24 hours for holdings
    let yesterday = (now - chrono::Duration::hours(24)).format("%Y%m%d").to_string();
    let holdings = portfolio::list_holdings().unwrap_or_default();
    for holding in &holdings {
        let ts_code = format!("{}.{}", &holding.symbol[..6], if holding.symbol.starts_with('6') { "SH" } else { "SZ" });
        match client.anns_d(&ts_code, &yesterday, &today).await {
            Ok(items) => {
                for item in items {
                    raw_events.push(RawEvent {
                        source: "tushare_anns_d".into(),
                        event_type: "announcement".into(),
                        title: item.title,
                        body: String::new(),
                        url: Some(item.url),
                        created_at: item.ann_date,
                    });
                }
            }
            Err(e) => {
                log::warn!("anns_d({}) failed: {}", ts_code, e);
            }
        }
    }

    if raw_events.is_empty() {
        return Ok(0);
    }

    // 2. Rule-based filter
    let candidates: Vec<(usize, RawEvent)> = raw_events
        .into_iter()
        .enumerate()
        .filter(|(_, ev)| classify_severity(&ev.title, &ev.body).is_some())
        .collect();

    if candidates.is_empty() {
        return Ok(0);
    }

    // 3. Deduplicate against existing events
    let candidate_events: Vec<RawEvent> = candidates.into_iter().map(|(_, e)| e).collect();
    let new_events = deduplicate_events(&candidate_events)?;

    if new_events.is_empty() {
        return Ok(0);
    }

    // 4. LLM normalization (batch)
    let normalized = normalize_events(llm_client, llm_config, &new_events, system_prompt).await;

    // 5. Get current holdings symbols for matching
    let holding_symbols: Vec<String> = holdings.iter().map(|h| h.symbol.clone()).collect();

    // 6. Save to database
    let mut saved_count = 0;
    for (raw, norm) in new_events.iter().zip(normalized.iter()) {
        // Match affected symbols with holdings
        let matched_symbols: Vec<String> = norm.affected_symbols
            .iter()
            .filter(|s| holding_symbols.iter().any(|h| h.starts_with(&s[..6])))
            .cloned()
            .collect();

        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            source: raw.source.clone(),
            event_type: raw.event_type.clone(),
            title: raw.title.clone(),
            body: Some(norm.one_line_claim.clone()),
            symbols: if matched_symbols.is_empty() {
                None
            } else {
                Some(matched_symbols.join(","))
            },
            severity: norm.severity.clone(),
            stance: norm.stance.clone(),
            triggered: false,
            trigger_verdict_id: None,
            created_at: raw.created_at.clone(),
        };

        if let Err(e) = save_event(&event) {
            log::warn!("Failed to save event: {}", e);
        } else {
            saved_count += 1;
        }
    }

    Ok(saved_count)
}

/// Deduplicate raw events against existing DB events by (source, title).
fn deduplicate_events(raw_events: &[RawEvent]) -> Result<Vec<RawEvent>, String> {
    use crate::storage::invest::events::list_events;

    // Get recent events for dedup check
    let existing = list_events(None, Some(500))?;
    let existing_keys: std::collections::HashSet<String> = existing
        .iter()
        .map(|e| format!("{}|{}", e.source, e.title))
        .collect();

    let new_events: Vec<RawEvent> = raw_events
        .iter()
        .filter(|ev| {
            let key = format!("{}|{}", ev.source, ev.title);
            !existing_keys.contains(&key)
        })
        .cloned()
        .collect();

    Ok(new_events)
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 无错误（`portfolio::list_holdings` 返回类型需确认）

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/invest/event_scanner.rs
git commit -m "feat(invest): add event scan orchestration with dedup and holdings matching"
```

---

### Task 6: Tauri 命令 + 注册

**Files:**
- Modify: `src-tauri/src/commands/invest.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 添加 `scan_events` Tauri 命令**

在 `src-tauri/src/commands/invest.rs` 中添加：

```rust
/// Trigger an immediate event scan. Returns the number of new events found.
#[tauri::command]
pub async fn scan_events(token: String) -> Result<usize, String> {
    let llm_config = crate::invest::llm::load_llm_config_from_file()?;
    let llm_client = crate::invest::llm::OpenAiCompatClient::new();

    // Load custom prompt if exists
    let prompt_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".claw-go/invest/prompts/event_normalizer.md");
    let system_prompt = std::fs::read_to_string(&prompt_path).ok();

    crate::invest::event_scanner::run_event_scan(
        &token,
        &llm_client,
        &llm_config,
        system_prompt.as_deref(),
    )
    .await
}

/// Get event scan status (last scan time from event_sources).
#[tauri::command]
pub fn get_scan_status() -> Result<ScanStatus, String> {
    let sources = crate::storage::invest::events::list_event_sources()?;
    let last_poll = sources
        .iter()
        .filter_map(|s| s.last_poll_at.as_deref())
        .max()
        .map(|s| s.to_string());

    Ok(ScanStatus {
        last_poll_at: last_poll,
        sources_enabled: sources.iter().any(|s| s.enabled),
    })
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub last_poll_at: Option<String>,
    pub sources_enabled: bool,
}
```

- [ ] **Step 2: 在 `lib.rs` 注册新命令**

在 `generate_handler!` 块中（`save_event_source` 之后）添加：

```rust
commands::invest::scan_events,
commands::invest::get_scan_status,
```

- [ ] **Step 3: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 无错误

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/invest.rs src-tauri/src/lib.rs
git commit -m "feat(invest): add scan_events and get_scan_status Tauri commands"
```

---

### Task 7: 事件扫描 Cron Job

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 添加事件扫描 cron**

在 `lib.rs` 中，紧接 PnL snapshot cron 之后添加（遵循相同的 spawn 模式）：

```rust
// Start event scanner cron job.
// Weekdays 8-22: every 30 min. Weekends: 9:00 and 18:00.
{
    tauri::async_runtime::spawn(async {
        loop {
            let now = chrono::Local::now();
            let hour = now.hour();
            let minute = now.minute();
            let weekday = now.weekday();

            let is_weekday = weekday != chrono::Weekday::Sat && weekday != chrono::Weekday::Sun;

            // Calculate next run time
            let next = if is_weekday {
                // Weekday: every 30 min from 8:00-22:00
                let next_minute = if minute < 30 { 30 } else { 0 };
                let next_hour = if minute < 30 { hour } else { hour + 1 };
                if next_hour >= 22 {
                    // Sleep until tomorrow 8:00
                    let tomorrow = now.date_naive() + chrono::Duration::days(1);
                    chrono::Local.from_local_datetime(&tomorrow.and_hms_opt(8, 0, 0).unwrap()).unwrap()
                } else {
                    let target = now.date_naive().and_hms_opt(next_hour, next_minute, 0).unwrap();
                    chrono::Local.from_local_datetime(&target).unwrap()
                }
            } else {
                // Weekend: 9:00 and 18:00
                if hour < 9 {
                    let target = now.date_naive().and_hms_opt(9, 0, 0).unwrap();
                    chrono::Local.from_local_datetime(&target).unwrap()
                } else if hour < 18 {
                    let target = now.date_naive().and_hms_opt(18, 0, 0).unwrap();
                    chrono::Local.from_local_datetime(&target).unwrap()
                } else {
                    let tomorrow = now.date_naive() + chrono::Duration::days(1);
                    chrono::Local.from_local_datetime(&tomorrow.and_hms_opt(9, 0, 0).unwrap()).unwrap()
                }
            };

            let sleep_duration = (next - now).to_std().unwrap_or(std::time::Duration::from_secs(300));
            tokio::time::sleep(sleep_duration).await;

            // Read token from settings
            let token = match crate::storage::settings::read_settings() {
                Ok(s) => s.tushare_token.unwrap_or_default(),
                Err(_) => continue,
            };
            if token.is_empty() {
                continue;
            }

            // Log task start
            let task_id = match crate::storage::invest::scheduler::log_task_start("event_scan") {
                Ok(id) => id,
                Err(_) => continue,
            };

            // Load LLM config and run scan
            let llm_config = match crate::invest::llm::load_llm_config_from_file() {
                Ok(c) => c,
                Err(e) => {
                    let _ = crate::storage::invest::scheduler::log_task_end(task_id, "error", Some(&format!("LLM config error: {}", e)));
                    continue;
                }
            };
            let llm_client = crate::invest::llm::OpenAiCompatClient::new();
            let prompt_path = dirs::home_dir()
                .unwrap_or_default()
                .join(".claw-go/invest/prompts/event_normalizer.md");
            let system_prompt = std::fs::read_to_string(&prompt_path).ok();

            let result = crate::invest::event_scanner::run_event_scan(
                &token,
                &llm_client,
                &llm_config,
                system_prompt.as_deref(),
            )
            .await;

            match &result {
                Ok(count) => {
                    let msg = format!("Scanned events, {} new", count);
                    let _ = crate::storage::invest::scheduler::log_task_end(task_id, "success", Some(&msg));
                    // Update last_poll_at on event sources
                    let _ = crate::storage::invest::events::update_last_poll();
                }
                Err(e) => {
                    let _ = crate::storage::invest::scheduler::log_task_end(task_id, "error", Some(e));
                }
            }
        }
    });
}
```

- [ ] **Step 2: 添加 `update_last_poll` 辅助函数**

在 `src-tauri/src/storage/invest/events.rs` 中添加：

```rust
/// Update last_poll_at on all enabled event sources.
pub fn update_last_poll() -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    super::with_conn(|conn| {
        conn.execute(
            "UPDATE event_sources SET last_poll_at = ?1 WHERE enabled = 1",
            rusqlite::params![now],
        )
        .map_err(|e| format!("update_last_poll: {}", e))?;
        Ok(())
    })
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 无错误

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/storage/invest/events.rs
git commit -m "feat(invest): add event scanner background cron job"
```

---

### Task 8: 前端 Store — 事件状态

**Files:**
- Modify: `src/lib/stores/invest-store.svelte.ts`

- [ ] **Step 1: 添加事件类型定义**

在 `invest-store.svelte.ts` 顶部（现有类型之后）添加：

```typescript
interface InvestEvent {
  id: string;
  source: string;
  eventType: string;
  title: string;
  body: string | null;
  symbols: string | null;
  severity: string;
  stance: string;
  triggered: boolean;
  triggerVerdictId: string | null;
  createdAt: string;
}

interface ScanStatus {
  lastPollAt: string | null;
  sourcesEnabled: boolean;
}
```

- [ ] **Step 2: 添加事件状态字段**

在 `InvestStore` class 的 `$state` 区域添加：

```typescript
events = $state<InvestEvent[]>([]);
scanStatus = $state<ScanStatus>({ lastPollAt: null, sourcesEnabled: false });
scanning = $state(false);
```

- [ ] **Step 3: 添加事件加载方法**

在 `InvestStore` class 中添加：

```typescript
async loadEvents(limit = 100): Promise<void> {
  try {
    this.events = await invoke<InvestEvent[]>('get_events', { source: null, limit });
  } catch (e) {
    console.error('Failed to load events:', e);
  }
}

async loadScanStatus(): Promise<void> {
  try {
    this.scanStatus = await invoke<ScanStatus>('get_scan_status');
  } catch (e) {
    console.error('Failed to load scan status:', e);
  }
}

async scanEvents(token: string): Promise<number> {
  this.scanning = true;
  try {
    const count = await invoke<number>('scan_events', { token });
    await this.loadEvents();
    await this.loadScanStatus();
    return count;
  } finally {
    this.scanning = false;
  }
}

async triggerCommittee(eventId: string, verdictId: string): Promise<void> {
  await invoke('mark_event_triggered', { id: eventId, verdictId });
  await this.loadEvents();
}
```

- [ ] **Step 4: 在 `loadAll` 中加载事件**

在 `loadAll()` 方法的 `Promise.all` 中添加：

```typescript
invoke<InvestEvent[]>('get_events', { source: null, limit: 100 }).catch(() => []),
invoke<ScanStatus>('get_scan_status').catch(() => ({ lastPollAt: null, sourcesEnabled: false })),
```

并在解构后赋值：

```typescript
const [holdings, trades, snapshots, verdicts, cash, strategies, events, scanStatus] = await Promise.all([...]);
this.events = events;
this.scanStatus = scanStatus;
```

- [ ] **Step 5: 验证前端编译**

Run: `npm run check`
Expected: 无错误

- [ ] **Step 6: Commit**

```bash
git add src/lib/stores/invest-store.svelte.ts
git commit -m "feat(invest): add event state and methods to invest store"
```

---

### Task 9: EventWatchTab 组件

**Files:**
- Create: `src/lib/components/invest/EventWatchTab.svelte`
- Modify: `src/routes/invest/+page.svelte`

- [ ] **Step 1: 创建 EventWatchTab.svelte**

创建 `src/lib/components/invest/EventWatchTab.svelte`：

```svelte
<script lang="ts">
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { t } from '$lib/i18n';

  let timeWindow = $state<'24h' | '48h' | '7d'>('24h');
  let severityFilter = $state<'all' | 'high' | 'medium'>('all');
  let searchQuery = $state('');
  let showConfig = $state(false);
  let expandedEventId = $state<string | null>(null);

  const { events, scanStatus, scanning } = $derived(investStore);

  // Derived filtered events
  const filteredEvents = $derived(() => {
    const now = Date.now();
    const windowMs = timeWindow === '24h' ? 86400000 : timeWindow === '48h' ? 172800000 : 604800000;

    return events.filter((ev) => {
      // Time window filter
      const evTime = new Date(ev.createdAt).getTime();
      if (now - evTime > windowMs) return false;

      // Severity filter
      if (severityFilter !== 'all' && ev.severity !== severityFilter) return false;

      // Search filter
      if (searchQuery && !ev.title.includes(searchQuery) && !(ev.body?.includes(searchQuery))) return false;

      return true;
    });
  });

  // Sort: HIGH untriggered first, then by date desc
  const sortedEvents = $derived(() => {
    return [...filteredEvents()].sort((a, b) => {
      // HIGH untriggered first
      const aPriority = a.severity === 'high' && !a.triggered ? 0 : 1;
      const bPriority = b.severity === 'high' && !b.triggered ? 0 : 1;
      if (aPriority !== bPriority) return aPriority - bPriority;
      // Then by date desc
      return new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime();
    });
  });

  function handleScan() {
    // Token is read from settings on the backend
    investStore.scanEvents('');
  }

  function handleTrigger(ev: InvestEvent) {
    // Navigate to committee tab - trigger dialog is in +page.svelte
    expandedEventId = ev.id;
  }

  function formatTime(dateStr: string): string {
    const d = new Date(dateStr);
    return d.toLocaleString();
  }

  function severityColor(severity: string): string {
    switch (severity) {
      case 'high': return 'bg-red-500/20 text-red-400 border-red-500/30';
      case 'medium': return 'bg-yellow-500/20 text-yellow-400 border-yellow-500/30';
      default: return 'bg-muted text-muted-foreground';
    }
  }

  function stanceColor(stance: string): string {
    switch (stance) {
      case 'bullish': return 'text-green-400';
      case 'bearish': return 'text-red-400';
      default: return 'text-muted-foreground';
    }
  }
</script>

<div class="flex flex-col gap-4 p-4">
  <!-- Status bar -->
  <div class="flex items-center justify-between">
    <div class="text-sm text-muted-foreground">
      {#if scanStatus.lastPollAt}
        {t('invest_event_last_scan')}: {formatTime(scanStatus.lastPollAt)}
      {:else}
        {t('invest_event_never_scanned')}
      {/if}
    </div>
    <button
      class="px-3 py-1.5 text-sm rounded-md bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
      onclick={handleScan}
      disabled={scanning}
    >
      {scanning ? t('invest_event_scanning') : t('invest_event_scan_now')}
    </button>
  </div>

  <!-- Filter bar -->
  <div class="flex flex-wrap items-center gap-2">
    <div class="flex rounded-md overflow-hidden border border-border">
      {#each (['24h', '48h', '7d'] as const) as tw}
        <button
          class="px-3 py-1 text-sm {timeWindow === tw ? 'bg-primary text-primary-foreground' : 'bg-muted hover:bg-muted/80'}"
          onclick={() => timeWindow = tw}
        >{tw}</button>
      {/each}
    </div>

    <div class="flex rounded-md overflow-hidden border border-border">
      {#each (['all', 'high', 'medium'] as const) as sf}
        <button
          class="px-3 py-1 text-sm {severityFilter === sf ? 'bg-primary text-primary-foreground' : 'bg-muted hover:bg-muted/80'}"
          onclick={() => severityFilter = sf}
        >{sf === 'all' ? t('invest_event_filter_all') : sf.toUpperCase()}</button>
      {/each}
    </div>

    <input
      type="text"
      placeholder={t('invest_event_search')}
      class="px-3 py-1 text-sm rounded-md border border-border bg-background"
      bind:value={searchQuery}
    />
  </div>

  <!-- Event list -->
  <div class="flex flex-col gap-2">
    {#if sortedEvents().length === 0}
      <div class="text-center py-8 text-muted-foreground">
        {t('invest_event_empty')}
      </div>
    {:else}
      {#each sortedEvents() as ev (ev.id)}
        <div
          class="border rounded-md p-3 {ev.severity === 'high' && !ev.triggered ? 'border-l-4 border-l-red-500' : 'border-border'}"
          role="button"
          tabindex="0"
          onclick={() => expandedEventId = expandedEventId === ev.id ? null : ev.id}
          onkeydown={(e) => e.key === 'Enter' && (expandedEventId = expandedEventId === ev.id ? null : ev.id)}
        >
          <div class="flex items-start justify-between gap-2">
            <div class="flex items-center gap-2 flex-wrap">
              <span class="px-2 py-0.5 text-xs rounded border {severityColor(ev.severity)}">
                {ev.severity.toUpperCase()}
              </span>
              <span class="text-xs {stanceColor(ev.stance)}">
                {ev.stance}
              </span>
              <span class="text-xs text-muted-foreground px-1.5 py-0.5 bg-muted rounded">
                {ev.source === 'tushare_major_news' ? '新闻' : '公告'}
              </span>
            </div>
            <div class="flex items-center gap-2">
              {#if ev.symbols}
                <div class="flex gap-1">
                  {#each ev.symbols.split(',') as sym}
                    <span class="text-xs px-1.5 py-0.5 bg-blue-500/20 text-blue-400 rounded">{sym}</span>
                  {/each}
                </div>
              {/if}
              {#if ev.triggered}
                <span class="text-xs text-green-400">✓ {t('invest_event_triggered')}</span>
              {:else if ev.severity === 'high'}
                <button
                  class="px-2 py-1 text-xs rounded bg-primary text-primary-foreground hover:bg-primary/90"
                  onclick|stopPropagation={() => handleTrigger(ev)}
                >{t('invest_event_trigger')}</button>
              {/if}
            </div>
          </div>

          <div class="mt-2 text-sm">
            {ev.body || ev.title}
          </div>

          <div class="mt-1 text-xs text-muted-foreground">
            {formatTime(ev.createdAt)}
          </div>

          {#if expandedEventId === ev.id}
            <div class="mt-3 pt-3 border-t border-border text-sm">
              <div class="text-muted-foreground mb-1">{t('invest_event_original_title')}:</div>
              <div>{ev.title}</div>
              {#if ev.body && ev.body !== ev.title}
                <div class="mt-2 text-muted-foreground mb-1">{t('invest_event_summary')}:</div>
                <div>{ev.body}</div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>

  <!-- Config section -->
  <div class="mt-4 border-t border-border pt-4">
    <button
      class="text-sm text-muted-foreground hover:text-foreground"
      onclick={() => showConfig = !showConfig}
    >
      {showConfig ? '▼' : '▶'} {t('invest_event_config')}
    </button>

    {#if showConfig}
      <div class="mt-2 text-sm text-muted-foreground">
        {t('invest_event_config_hint')}
      </div>
    {/if}
  </div>
</div>
```

- [ ] **Step 2: 在 `+page.svelte` 中替换占位符**

在 `src/routes/invest/+page.svelte` 中：

1. 添加 import：
```svelte
import EventWatchTab from '$lib/components/invest/EventWatchTab.svelte';
```

2. 替换 line 169-170 的占位符：
```svelte
{:else if activeTab === 'events'}
  <EventWatchTab />
```

- [ ] **Step 3: 验证前端编译**

Run: `npm run check`
Expected: 无错误

- [ ] **Step 4: Commit**

```bash
git add src/lib/components/invest/EventWatchTab.svelte src/routes/invest/+page.svelte
git commit -m "feat(invest): add EventWatchTab component with filtering and event list"
```

---

### Task 10: 触发委员会确认对话框

**Files:**
- Create: `src/lib/components/invest/EventTriggerDialog.svelte`
- Modify: `src/lib/components/invest/EventWatchTab.svelte`
- Modify: `src/routes/invest/+page.svelte`

- [ ] **Step 1: 创建 EventTriggerDialog.svelte**

```svelte
<script lang="ts">
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import { t } from '$lib/i18n';

  let { event, onClose, onTriggered }: {
    event: { id: string; body: string | null; title: string; severity: string; stance: string; symbols: string | null };
    onClose: () => void;
    onTriggered: () => void;
  } = $props();

  let debateRounds = $state(4);
  const DEBATE_OPTIONS = [1, 2, 3, 4, 6, 8];

  const symbolList = $derived(
    event.symbols ? event.symbols.split(',').map(s => s.trim()).filter(Boolean) : []
  );

  async function handleConfirm() {
    if (symbolList.length === 0) return;

    // Mark event as triggered
    await investStore.triggerCommittee(event.id, '');

    // Switch to committee live tab
    // This is handled by the parent page
    onTriggered();

    // Start committee run
    await investCommitteeStore.runCommittee(symbolList, debateRounds);
  }
</script>

<!-- Backdrop -->
<div
  class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center"
  role="dialog"
  aria-modal="true"
  onclick={onClose}
  onkeydown={(e) => e.key === 'Escape' && onClose()}
>
  <div
    class="bg-background border border-border rounded-lg p-6 max-w-md w-full mx-4 shadow-lg"
    onclick|stopPropagation
    onkeydown|stopPropagation
  >
    <h3 class="text-lg font-semibold mb-4">
      ⚠️ {t('invest_event_trigger_title')}
    </h3>

    <div class="space-y-3 text-sm">
      <div class="text-muted-foreground">{t('invest_event_trigger_detected')}:</div>
      <div class="p-2 bg-muted rounded">
        「{event.body || event.title}」
      </div>

      <div class="flex gap-2">
        <span>severity: {event.severity.toUpperCase()}</span>
        <span>|</span>
        <span>stance: {event.stance}</span>
      </div>

      {#if symbolList.length > 0}
        <div>
          {t('invest_event_trigger_holdings')}: {symbolList.join(', ')}
        </div>
      {/if}

      <div class="flex items-center gap-2">
        <span>{t('invest_event_trigger_rounds')}:</span>
        <select bind:value={debateRounds} class="px-2 py-1 rounded border border-border bg-background text-sm">
          {#each DEBATE_OPTIONS as opt}
            <option value={opt}>{opt}</option>
          {/each}
        </select>
      </div>

      <div class="text-muted-foreground">
        {t('invest_event_trigger_confirm')}
      </div>
    </div>

    <div class="flex justify-end gap-3 mt-6">
      <button
        class="px-4 py-2 text-sm rounded-md border border-border hover:bg-muted"
        onclick={onClose}
      >{t('invest_event_trigger_cancel')}</button>
      <button
        class="px-4 py-2 text-sm rounded-md bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
        onclick={handleConfirm}
        disabled={symbolList.length === 0}
      >{t('invest_event_trigger_confirm_btn')}</button>
    </div>
  </div>
</div>
```

- [ ] **Step 2: 在 EventWatchTab 中集成对话框**

在 `EventWatchTab.svelte` 中：

1. 添加 import 和状态：
```svelte
import EventTriggerDialog from './EventTriggerDialog.svelte';
let triggerEvent = $state<InvestEvent | null>(null);
```

2. 替换 `handleTrigger` 函数：
```typescript
function handleTrigger(ev: InvestEvent) {
  triggerEvent = ev;
}
```

3. 在模板底部添加对话框：
```svelte
{#if triggerEvent}
  <EventTriggerDialog
    event={triggerEvent}
    onClose={() => triggerEvent = null}
    onTriggered={() => {
      triggerEvent = null;
      // Switch to committee tab - emit event or use callback
    }}
  />
{/if}
```

- [ ] **Step 3: 在 `+page.svelte` 中处理 Tab 切换**

在 `+page.svelte` 中，给 EventWatchTab 添加回调 prop 以切换到委员会 Tab：

```svelte
<EventWatchTab onNavigateToCommittee={() => { activeTab = 'committee'; committeeSubTab = 'live'; }} />
```

在 EventWatchTab 中添加 prop 并在 `onTriggered` 中调用。

- [ ] **Step 4: 验证前端编译**

Run: `npm run check`
Expected: 无错误

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/invest/EventTriggerDialog.svelte src/lib/components/invest/EventWatchTab.svelte src/routes/invest/+page.svelte
git commit -m "feat(invest): add event trigger committee confirmation dialog"
```

---

### Task 11: i18n 更新

**Files:**
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

- [ ] **Step 1: 添加英文 i18n keys**

在 `messages/en.json` 的 `invest_` 区域添加：

```json
"invest_event_last_scan": "Last scan",
"invest_event_never_scanned": "Never scanned",
"invest_event_scan_now": "Scan Now",
"invest_event_scanning": "Scanning...",
"invest_event_filter_all": "All",
"invest_event_search": "Search events...",
"invest_event_empty": "No events found. Click Scan Now to fetch.",
"invest_event_triggered": "Triggered",
"invest_event_trigger": "Trigger Committee",
"invest_event_original_title": "Original title",
"invest_event_summary": "Summary",
"invest_event_config": "Data source configuration",
"invest_event_config_hint": "Configure Tushare data sources for event scanning.",
"invest_event_trigger_title": "Event Triggers Committee",
"invest_event_trigger_detected": "High-impact event detected",
"invest_event_trigger_holdings": "Related holdings",
"invest_event_trigger_rounds": "Debate rounds",
"invest_event_trigger_confirm": "Start committee evaluation now?",
"invest_event_trigger_cancel": "Cancel",
"invest_event_trigger_confirm_btn": "Confirm & Start"
```

- [ ] **Step 2: 添加中文 i18n keys**

在 `messages/zh-CN.json` 的 `invest_` 区域添加：

```json
"invest_event_last_scan": "上次扫描",
"invest_event_never_scanned": "从未扫描",
"invest_event_scan_now": "立即扫描",
"invest_event_scanning": "扫描中...",
"invest_event_filter_all": "全部",
"invest_event_search": "搜索事件...",
"invest_event_empty": "暂无事件，点击立即扫描获取",
"invest_event_triggered": "已触发",
"invest_event_trigger": "触发委员会",
"invest_event_original_title": "原始标题",
"invest_event_summary": "摘要",
"invest_event_config": "数据源配置",
"invest_event_config_hint": "配置 Tushare 数据源以进行事件扫描",
"invest_event_trigger_title": "事件触发委员会",
"invest_event_trigger_detected": "检测到高影响事件",
"invest_event_trigger_holdings": "关联持仓",
"invest_event_trigger_rounds": "辩论轮数",
"invest_event_trigger_confirm": "是否立即启动委员会评估？",
"invest_event_trigger_cancel": "取消",
"invest_event_trigger_confirm_btn": "确认启动"
```

- [ ] **Step 3: 验证 i18n 同步**

Run: `npm run i18n:check`
Expected: 无错误

- [ ] **Step 4: Commit**

```bash
git add messages/en.json messages/zh-CN.json
git commit -m "feat(invest): add i18n keys for Event Watch"
```

---

### Task 12: 全量验证

- [ ] **Step 1: Rust 编译检查**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 无错误

- [ ] **Step 2: Rust 测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::event_scanner::tests -- --nocapture`
Expected: 所有测试 PASS

- [ ] **Step 3: 前端检查**

Run: `npm run check`
Expected: 无错误

- [ ] **Step 4: i18n 检查**

Run: `npm run i18n:check`
Expected: 无错误

- [ ] **Step 5: Lint**

Run: `npm run lint`
Expected: 无错误

- [ ] **Step 6: 最终 Commit（如有修复）**

```bash
git add -A
git commit -m "chore(invest): fix findings from full verification"
```
