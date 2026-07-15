# Sentiment 分析 DeepSeek V4 Pro "全中性" 修复

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 DeepSeek V4 Pro 作为 sentiment 分析 provider 时，LLM 响应解析失败导致全部新闻被标记为 `stance: "neutral"` 的问题。

**Architecture:** (1) 根因：`--permission-mode plan` 触发 V4 Pro Plan Mode，配合 `--max-turns 1` 耗尽 turn → 空响应 → 全部 fallback neutral。`plan` → `bypassPermissions` 根治。(2) 防御：`try_extract_json` 用 `trim_start_matches`/`trim_end_matches` + `find("[{")` 提取 JSON，比当前 `starts_with("```")` 更容错。(3) 兜底：`fallback_normalize_from` 新增简单关键词 stance 检测，不再硬编码 neutral。

**Tech Stack:** Rust (src-tauri), serde_json

## Global Constraints

- `run_role` 函数签名不变 —— 所有调用者（sentiment、committee 角色、宏观判断、盘前等）都使用 `--print` 非交互模式，`bypassPermissions` 对全部路径是正确行为
- `parse_normalized_response` 保持泛型接口 `fn<T>(content, items, fallback) -> Vec<NormalizedEvent>`
- 不引入新依赖，不引入 UTF-8 字节边界算术（避免中文 panic）
- 不引入循环/递归（避免潜在的无限循环）

---

## 上下文

### 根因

`cli_executor.rs:108` 的 `--permission-mode plan`。DeepSeek V4 Pro 将 `plan` 解释为 Plan Mode，配合 `--max-turns 1`：大批量（5-50条）→ 1 turn 耗尽 → 空响应或 Plan 文本 → `cli_complete` 返回 Err → `normalize_events` 全部 fallback → `stance: "neutral"`。

### 模拟验证结果

| 配置 | 结果 |
|------|------|
| `plan` + `--max-turns 1` + 5条新闻 | ❌ `Reached max turns (1)` |
| `bypassPermissions` + 3条新闻 | ✅ 干净的 ` ```json [{...}] ``` ` |

### 受影响的文件

- `src-tauri/src/invest/committee/cli_executor.rs:108` — 根因修复
- `src-tauri/src/invest/event_scanner.rs:502-554` — `parse_normalized_response` + `fallback_normalize_from`

---

### Task 0: 迁移 plan 文件到项目目录

**操作：** 将本 plan 文件从临时位置迁移到项目的 `docs/superpowers/plans/` 并按内容重命名。

- [ ] **Step 1: 移动并重命名**

```bash
mkdir -p docs/superpowers/plans
mv C:/Users/InBlu/.claude/plans/giggly-napping-seahorse.md \
   docs/superpowers/plans/2026-07-15-sentiment-deepseek-all-neutral-fix.md
```

---

### Task 1: 根因修复 — `plan` → `bypassPermissions`

**Files:**
- Modify: `src-tauri/src/invest/committee/cli_executor.rs:108`

- [ ] **Step 1: 修改 `--permission-mode` 参数**

```rust
// Before:
"--permission-mode",
"plan",

// After:
"--permission-mode",
"bypassPermissions",
```

> `bypassPermissions` 禁止所有交互式权限提示（包括 Plan Mode），已在模拟中验证 V4 Pro 在此模式下直接输出 JSON。该标志影响 `run_role` 的所有 6 个调用者（sentiment、committee Quant/Risk/CIO、宏观判断、行业敏感度、盘前复盘、盈记解读），这些全部是 `--print` 非交互场景，不需要 write 权限，`bypassPermissions` 对全部路径是正确行为。此外 settings JSON 模板（`provider_claude_config.rs:699`）已硬编码 `"bypassPermissions"`，CLI 层面保持一致。

- [ ] **Step 2: 构建验证**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/invest/committee/cli_executor.rs
git commit -m "fix: use bypassPermissions instead of plan mode for committee CLI executor"
```

---

### Task 2: 防御 — 增强 JSON 提取

**Files:**
- Modify: `src-tauri/src/invest/event_scanner.rs:500-538`

- [ ] **Step 1: 实现 `try_extract_json` 辅助函数**

在 `parse_normalized_response` 上方（~499行）添加：

```rust
/// Attempt to extract a JSON array from LLM response text that may
/// contain conversational wrapper text and/or markdown code fences.
///
/// Strategies (tried in order):
/// 1. Trim markdown fences (` ```json ` / ` ``` ` / ` ```JSON `)
/// 2. If the result still isn't pure JSON, search for `[{` and try
///    parsing from that position
/// 3. Fall through — return the trimmed/cleaned text for the caller
///    to attempt parsing or fallback
///
/// Uses only safe string operations: no byte-index arithmetic, no
/// loops that could fail to terminate.
fn try_extract_json(content: &str) -> String {
    // Strategy 1: trim markdown fences from the response.
    // trim_start_matches / trim_end_matches are safe — they work on
    // char boundaries and never panic.
    let cleaned = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```JSON")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string();

    // Strategy 2: find JSON array start marker [{ in the cleaned text.
    // If found and the substring from that position parses as valid JSON,
    // use it. This handles cases where conversational text appears before
    // the JSON and markdown-fence trimming didn't fully clean it.
    if let Some(pos) = cleaned.find("[{") {
        let candidate = &cleaned[pos..];
        if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
            return candidate.to_string();
        }
    }

    cleaned
}
```

> 与旧版 plan 的 `extract_json_array` + `strip_markdown_fence` 相比，这个版本：
> - 无 while 循环 → 无无限循环风险
> - 无字节索引切片 → 无 UTF-8 边界 panic 风险
> - 无 `find("```")` + `after_fence[3..]`  → 不会取错代码块（V4 Pro think block）
> - `trim_start_matches("```json")` 比手动剥 fence 更简洁
> - 两个策略覆盖所有实际场景，fall through 返回 cleaned 文本由调用方兜底

- [ ] **Step 2: 重写 `parse_normalized_response`**

替换第 502-538 行：

```rust
pub fn parse_normalized_response<T>(
    content: &str,
    items: &[T],
    fallback: impl Fn(&T) -> NormalizedEvent,
) -> Vec<NormalizedEvent> {
    let json_str = try_extract_json(content);

    match serde_json::from_str::<Vec<NormalizedEvent>>(&json_str) {
        Ok(mut results) => {
            // Truncate or pad to match input length
            results.truncate(items.len());
            while results.len() < items.len() {
                let idx = results.len();
                results.push(fallback(&items[idx]));
            }
            results
        }
        Err(e) => {
            log::warn!(
                "Failed to parse normalizer response ({} bytes): {}. \
                 First 400 chars of extracted: {}",
                content.len(),
                e,
                &json_str[..json_str.len().min(400)]
            );
            items.iter().map(fallback).collect()
        }
    }
}
```

- [ ] **Step 3: 构建验证**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/invest/event_scanner.rs
git commit -m "fix: robust JSON extraction from LLM responses with try_extract_json"
```

---

### Task 3: 兜底 — 简单关键词 stance 检测

**Files:**
- Modify: `src-tauri/src/invest/event_scanner.rs:125-155`（关键词常量）、`540-555`（fallback 函数）

**设计决策**：不做否定前缀处理。理由：这是 LLM 完全失败时的最后兜底路径，简单关键词匹配的正收益远大于偶尔的误判（且"不构成利好"误判为 bullish 也比永远 neutral 好）。移除否定处理消除了 `is_negated` 函数 —— 及其 UTF-8 字节边界 panic 和 8-byte 窗口遗漏两个 critical bug 的来源。

- [ ] **Step 1: 添加 stance 关键词常量**

在 `MEDIUM_KEYWORDS` 下方（~142行）添加：

```rust
/// Keywords that hint at bullish (positive) sentiment.
/// Used only in the fallback path when LLM parsing fails.
const BULLISH_KEYWORDS: &[&str] = &[
    "利好", "看多", "增持", "买入", "大涨", "涨停", "飙升",
    "预增", "扭亏", "盈利", "降准", "降息", "放水", "突破",
    "超预期", "创新高", "回暖", "反弹", "复苏",
];

/// Keywords that hint at bearish (negative) sentiment.
/// Used only in the fallback path when LLM parsing fails.
const BEARISH_KEYWORDS: &[&str] = &[
    "利空", "看空", "减持", "卖出", "大跌", "跌停", "暴跌",
    "预亏", "亏损", "暴雷", "违约", "处罚", "调查", "退市",
    "不及预期", "创新低", "下滑", "萎缩", "下行",
];
```

- [ ] **Step 2: 实现 `detect_stance` 函数**

在 `classify_severity` 附近添加：

```rust
/// Simple keyword-based stance detection for the fallback path.
/// No negation handling — this is a last-resort heuristic, and
/// false positives are preferable to the prior behavior of
/// hard-coding "neutral" for every item.
fn detect_stance(title: &str, body: &str) -> &'static str {
    let text = format!("{title} {body}");
    let has_bullish = BULLISH_KEYWORDS.iter().any(|kw| text.contains(kw));
    let has_bearish = BEARISH_KEYWORDS.iter().any(|kw| text.contains(kw));
    match (has_bullish, has_bearish) {
        (true, false) => "bullish",
        (false, true) => "bearish",
        _ => "neutral", // both present or neither → neutral
    }
}
```

- [ ] **Step 3: 修改 `fallback_normalize_from`**

替换第 546-554 行：

```rust
pub fn fallback_normalize_from(title: &str, body: &str) -> NormalizedEvent {
    let severity = classify_severity(title, body)
        .unwrap_or(Severity::Low);
    let stance = detect_stance(title, body).to_string();

    NormalizedEvent {
        one_line_claim: title.chars().take(30).collect(),
        stance,
        severity,
        affected_symbols: vec![],
        summary: String::new(),
        sectors: vec![],
        topics: vec![],
    }
}
```

- [ ] **Step 4: 构建验证**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/event_scanner.rs
git commit -m "feat: keyword-based stance detection in sentiment fallback"
```

---

### Task 4: 测试

**Files:**
- Modify: `src-tauri/src/invest/event_scanner.rs`（`#[cfg(test)] mod tests` 块末尾）

- [ ] **Step 1: 添加测试**

```rust
// ── try_extract_json tests ──

#[test]
fn test_extract_clean_fenced_json() {
    let input = "```json\n[{\"stance\":\"bullish\"}]\n```";
    let result = try_extract_json(input);
    assert_eq!(result, "[{\"stance\":\"bullish\"}]");
}

#[test]
fn test_extract_json_with_chinese_prefix() {
    let input = "好的，以下是分析结果：\n\n```json\n[{\"stance\":\"bullish\"}]\n```";
    let result = try_extract_json(input);
    assert_eq!(result, "[{\"stance\":\"bullish\"}]");
}

#[test]
fn test_extract_json_bare_fence() {
    let input = "```\n[{\"stance\":\"bearish\"}]\n```";
    let result = try_extract_json(input);
    assert_eq!(result, "[{\"stance\":\"bearish\"}]");
}

#[test]
fn test_extract_json_uppercase_fence() {
    let input = "```JSON\n[{\"stance\":\"neutral\"}]\n```";
    let result = try_extract_json(input);
    assert_eq!(result, "[{\"stance\":\"neutral\"}]");
}

#[test]
fn test_extract_json_with_text_before_and_after() {
    let input = "分析如下：[{\"stance\":\"bullish\"}] 仅供参考。";
    let result = try_extract_json(input);
    assert_eq!(result, "[{\"stance\":\"bullish\"}]");
}

#[test]
fn test_extract_json_multiple_objects() {
    let input = "```json\n[{\"a\":1},{\"b\":2}]\n```";
    let result = try_extract_json(input);
    assert_eq!(result, "[{\"a\":1},{\"b\":2}]");
}

#[test]
fn test_extract_json_no_json_present() {
    let input = "计划文件已创建。是否批准此计划？";
    let result = try_extract_json(input);
    // Should return cleaned text (fall through), not panic or loop
    assert_eq!(result, input);
}

#[test]
fn test_extract_json_fence_without_closing() {
    // LLM might omit closing fence
    let input = "```json\n[{\"stance\":\"bullish\"}]";
    let result = try_extract_json(input);
    assert_eq!(result, "[{\"stance\":\"bullish\"}]");
}

// ── detect_stance tests ──

#[test]
fn test_detect_bullish() {
    assert_eq!(detect_stance("央行宣布降准50个基点", "释放万亿流动性"), "bullish");
    assert_eq!(detect_stance("公司业绩预增200%", ""), "bullish");
}

#[test]
fn test_detect_bearish() {
    assert_eq!(detect_stance("公司债务违约超百亿", "面临退市风险"), "bearish");
    assert_eq!(detect_stance("业绩预亏", "大幅下滑"), "bearish");
}

#[test]
fn test_detect_neutral() {
    assert_eq!(detect_stance("公司发布日常公告", ""), "neutral");
}

#[test]
fn test_detect_both_keywords() {
    // Both bullish and bearish keywords present → neutral (contradictory)
    assert_eq!(detect_stance("利好消息发布但大股东减持", ""), "neutral");
}
```

- [ ] **Step 2: 构建 + 测试**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
# 如果可运行测试：
node scripts/rust-test.mjs --lib event_scanner
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/invest/event_scanner.rs
git commit -m "test: unit tests for try_extract_json and detect_stance"
```

---

### Task 5: PR 准备 — 构建 + 文档

**Files:**
- Modify: `docs/changelog.md`

- [ ] **Step 1: 全量构建验证**

```bash
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 2: 更新 changelog**

在 `docs/changelog.md` 顶部添加：

```markdown
## v5.7.3

### fix: DeepSeek V4 Pro sentiment analysis returns all-neutral

- `--permission-mode plan` 触发 V4 Pro Plan Mode → `--max-turns 1` 耗尽 turn 导致空响应
- 切换为 `bypassPermissions`（所有 `run_role` 调用者均为 `--print` 非交互场景，不需要 write 权限）
- 新增 `try_extract_json`：`trim_start_matches`/`trim_end_matches` 剥 fence + `find("[{")` 回退，不依赖 fence 在开头
- `fallback_normalize_from` 新增关键词 stance 检测（`BULLISH_KEYWORDS` / `BEARISH_KEYWORDS`），不再硬编码 neutral
```

- [ ] **Step 3: Commit**

```bash
git add docs/changelog.md
git commit -m "docs: changelog for v5.7.3 sentiment fix"
```

---

## Verification

### 端到端验证

1. **启动应用** `npm run tauri dev`
2. **等待 sentiment_collector cron 触发**（每小时一次），或手动触发 sentiment 分析
3. **检查数据库**：
   ```sql
   SELECT stance, COUNT(*) FROM sentiment_items
   WHERE analyzed = 1 AND created_at > datetime('now', '-1 hour')
   GROUP BY stance;
   ```
   期望：bullish、bearish、neutral 均有合理分布，不再出现整批全部 neutral
4. **检查日志**：没有 `"Reached max turns (1)"` 或 `"Failed to parse normalizer response"` 警告

### CI 验证

```bash
npm run verify
```

如果 `cargo fmt` 步骤因历史债大量文件失败，跳过 fmt 单独跑：
```bash
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run build
```
