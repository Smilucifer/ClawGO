# 委员会解析器增强 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 改进委员会直播功能的解析健壮性和错误提示，解决部分角色返回为空的问题。

**Architecture:** 三个独立 PR 分步实施 — PR-A 放宽解析器格式（parser.rs）、PR-B 前端错误状态提示（store + 组件）、PR-C hard_truncate 优化（roles.rs）。采用分层 strip_prefix 保持零分配，store 层处理 WORKER_UNAVAILABLE 状态。

**Tech Stack:** Rust (parser.rs, roles.rs), Svelte 5 (CommitteeLiveTab.svelte), TypeScript (pipeline-config.ts, invest-committee-store.svelte.ts)

---

## File Structure

### PR-A: 解析器格式放宽
- **Modify:** `src-tauri/src/invest/committee/parser.rs` — extract_field + extract_list_field_any

### PR-B: 前端错误状态提示
- **Modify:** `src/lib/stores/invest-committee-store.svelte.ts` — SymbolProgress 增加 failedSteps
- **Modify:** `src/lib/components/invest/pipeline-config.ts` — getStepState 增加 failed 状态
- **Modify:** `src/lib/components/invest/CommitteeLiveTab.svelte` — 前端展示逻辑
- **Modify:** `src/lib/types.ts` — RoundOutputSummary 增加 fallbackReason
- **Modify:** `src-tauri/src/invest/committee/parser.rs` — ParsedFields 增加 fallback_reason
- **Modify:** `src-tauri/src/invest/committee/orchestrator.rs` — detect_fallback_reason 调用

### PR-C: hard_truncate 优化
- **Modify:** `src-tauri/src/invest/committee/roles.rs` — hard_truncate + critical_field_keys

---

## PR-A: 解析器格式放宽

### Task 1: 更新 extract_field 支持 6 种格式

**Files:**
- Modify: `src-tauri/src/invest/committee/parser.rs:163-175`

- [ ] **Step 1: 写失败的测试**

在 `parser.rs` 的 `tests` 模块中添加：

```rust
#[test]
fn test_extract_field_equal_sign() {
    let text = "SIGNAL=risk_on";
    assert_eq!(extract_field(text, "SIGNAL"), Some("risk_on".to_string()));
}

#[test]
fn test_extract_field_equal_sign_with_space() {
    let text = "SIGNAL = risk_on";
    assert_eq!(extract_field(text, "SIGNAL"), Some("risk_on".to_string()));
}

#[test]
fn test_extract_field_markdown_bold_colon() {
    let text = "**SIGNAL**: risk_on";
    assert_eq!(extract_field(text, "SIGNAL"), Some("risk_on".to_string()));
}

#[test]
fn test_extract_field_markdown_bold_equal() {
    let text = "**SIGNAL**=risk_on";
    assert_eq!(extract_field(text, "SIGNAL"), Some("risk_on".to_string()));
}

#[test]
fn test_extract_field_number_prefix() {
    let text = "1. SIGNAL: risk_on";
    assert_eq!(extract_field(text, "SIGNAL"), Some("risk_on".to_string()));
}

#[test]
fn test_extract_field_no_space_after_colon() {
    let text = "SIGNAL:risk_on";
    assert_eq!(extract_field(text, "SIGNAL"), Some("risk_on".to_string()));
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml parser::tests::test_extract_field_equal_sign -- --nocapture`
Expected: FAIL（当前 extract_field 不支持 `=` 分隔符）

- [ ] **Step 3: 实现新的 extract_field**

替换 `parser.rs:163-175`：

```rust
fn extract_field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        // 去除行首数字编号（如 "1. SIGNAL: ..."）
        let line = line.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ');

        // 尝试所有分隔符: KEY: KEY： KEY= KEY =
        for sep in &[":", "：", "=", " = "] {
            if let Some(rest) = line.strip_prefix(key).and_then(|r| r.strip_prefix(sep)) {
                return Some(rest.trim().to_string());
            }
        }
        // Markdown 粗体: **KEY**: VALUE / **KEY**：VALUE / **KEY**=VALUE
        if let Some(rest) = line.strip_prefix("**").and_then(|r| r.strip_prefix(key)) {
            if let Some(rest) = rest.strip_prefix("**") {
                for sep in &[":", "：", "=", " = "] {
                    if let Some(rest) = rest.strip_prefix(sep) {
                        return Some(rest.trim().to_string());
                    }
                }
            }
        }
    }
    None
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml parser::tests:: -- --nocapture`
Expected: 全部 PASS（包括新增的 6 个测试和原有的 13 个测试）

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/invest/committee/parser.rs
git commit -m "feat(invest): parser extract_field support 6 format variants"
```

---

### Task 2: 更新 extract_list_field_any 复用 extract_field

**Files:**
- Modify: `src-tauri/src/invest/committee/parser.rs:236-271`

- [ ] **Step 1: 写失败的测试**

```rust
#[test]
fn test_extract_list_field_equal_sign() {
    let text = "KEY_DATA = \n - item1\n - item2";
    let result = extract_list_field_any(text, &["KEY_DATA"]);
    assert_eq!(result, Some(vec!["item1".to_string(), "item2".to_string()]));
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml parser::tests::test_extract_list_field_equal_sign -- --nocapture`
Expected: FAIL

- [ ] **Step 3: 实现新的 extract_list_field_any**

替换 `parser.rs:236-271`：

```rust
fn extract_list_field_any(text: &str, keys: &[&str]) -> Option<Vec<String>> {
    let mut items = Vec::new();
    let mut found_key = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !found_key {
            // 复用 extract_field 检测列表起始行
            for key in keys {
                if extract_field(line, key).is_some() {
                    found_key = true;
                    break;
                }
            }
            if found_key {
                continue;
            }
        } else {
            // Collect ` - item` or `- item` lines until a non-list line or empty line
            if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("— ")) {
                let item = item.trim().to_string();
                if !item.is_empty() {
                    items.push(item);
                }
            } else if trimmed.is_empty() || trimmed.contains(':') || trimmed.contains('：') {
                // Reached next section — stop collecting
                break;
            }
        }
    }
    if found_key {
        Some(items)
    } else {
        None
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml parser::tests:: -- --nocapture`
Expected: 全部 PASS

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/invest/committee/parser.rs
git commit -m "refactor(invest): extract_list_field_any reuse extract_field for key detection"
```

---

## PR-B: 前端错误状态提示

### Task 3: ParsedFields 增加 fallback_reason 字段

**Files:**
- Modify: `src-tauri/src/invest/committee/parser.rs:8-131`

- [ ] **Step 1: 在 ParsedFields 结构体中添加字段**

在 `parser.rs:131` 之前添加：

```rust
    // -- Fallback diagnostics --
    /// 降级原因: "llm_unavailable" | "format_mismatch" | "truncated_critical" | "empty_response" | "parse_error"
    pub fallback_reason: Option<String>,
```

- [ ] **Step 2: 添加 detect_fallback_reason 函数**

在 `parse_role_output` 函数之后添加：

```rust
/// 检测降级原因，按角色定义关键字段缺失条件
pub fn detect_fallback_reason(
    parsed: &ParsedFields,
    role: CommitteeRole,
    raw_text: &str,
    truncated: bool,
) -> Option<String> {
    if raw_text == "[WORKER_UNAVAILABLE]" {
        return Some("llm_unavailable".to_string());
    }
    if raw_text.is_empty() {
        return Some("empty_response".to_string());
    }
    if truncated {
        // 检查关键字段是否被截断
        let critical_missing = match role {
            CommitteeRole::Macro => parsed.signal.is_none() && parsed.market_phase.is_none(),
            CommitteeRole::Quant => parsed.signal.is_none() && parsed.strength.is_none(),
            CommitteeRole::Risk => parsed.signal.is_none() && parsed.strength.is_none(),
            CommitteeRole::Cio => parsed.verdict.is_none(),
            CommitteeRole::L4Officer => {
                parsed.l4_emotion_assessment.is_none() && parsed.l4_red_light.is_none()
            }
        };
        if critical_missing {
            return Some("truncated_critical".to_string());
        }
    }
    // 格式不匹配：关键字段全空
    let critical_missing = match role {
        CommitteeRole::Macro => {
            parsed.signal.is_none()
                && parsed.strength.is_none()
                && parsed.market_phase.is_none()
        }
        CommitteeRole::Quant => {
            parsed.signal.is_none() && parsed.strength.is_none() && parsed.regime.is_none()
        }
        CommitteeRole::Risk => parsed.signal.is_none() && parsed.strength.is_none(),
        CommitteeRole::Cio => parsed.verdict.is_none() && parsed.confidence.is_none(),
        CommitteeRole::L4Officer => {
            parsed.l4_emotion_assessment.is_none() && parsed.l4_red_light.is_none()
        }
    };
    if critical_missing {
        return Some("format_mismatch".to_string());
    }
    None
}
```

- [ ] **Step 3: 添加测试**

```rust
#[test]
fn test_detect_fallback_reason_worker_unavailable() {
    let parsed = ParsedFields {
        raw_text: "[WORKER_UNAVAILABLE]".to_string(),
        ..Default::default()
    };
    assert_eq!(
        detect_fallback_reason(&parsed, CommitteeRole::Macro, "[WORKER_UNAVAILABLE]", false),
        Some("llm_unavailable".to_string())
    );
}

#[test]
fn test_detect_fallback_reason_empty_response() {
    let parsed = ParsedFields::default();
    assert_eq!(
        detect_fallback_reason(&parsed, CommitteeRole::Macro, "", false),
        Some("empty_response".to_string())
    );
}

#[test]
fn test_detect_fallback_reason_format_mismatch() {
    let parsed = ParsedFields {
        raw_text: "some text without proper format".to_string(),
        ..Default::default()
    };
    assert_eq!(
        detect_fallback_reason(&parsed, CommitteeRole::Macro, "some text without proper format", false),
        Some("format_mismatch".to_string())
    );
}

#[test]
fn test_detect_fallback_reason_none_when_valid() {
    let parsed = ParsedFields {
        raw_text: "SIGNAL: risk_on\nSTRENGTH: 7".to_string(),
        signal: Some("risk_on".to_string()),
        strength: Some(7.0),
        ..Default::default()
    };
    assert_eq!(
        detect_fallback_reason(&parsed, CommitteeRole::Macro, "SIGNAL: risk_on\nSTRENGTH: 7", false),
        None
    );
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml parser::tests:: -- --nocapture`
Expected: 全部 PASS

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/invest/committee/parser.rs
git commit -m "feat(invest): ParsedFields add fallback_reason with detect_fallback_reason"
```

---

### Task 4: orchestrator.rs 调用 detect_fallback_reason

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs`

- [ ] **Step 1: 在 parse_role_output 调用后添加 detect_fallback_reason**

在 `orchestrator.rs` 中找到 `let parsed = parse_role_output(role, &text, truncated);` 这行，在其后添加：

```rust
let mut parsed = parse_role_output(role, &text, truncated);
parsed.fallback_reason = super::parser::detect_fallback_reason(&parsed, role, &text, truncated);
```

- [ ] **Step 2: 运行 Rust 检查**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 无错误

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/invest/committee/orchestrator.rs
git commit -m "feat(invest): wire detect_fallback_reason into orchestrator"
```

---

### Task 5: 前端类型和 store 更新

**Files:**
- Modify: `src/lib/stores/invest-committee-store.svelte.ts`
- Modify: `src/lib/components/invest/pipeline-config.ts`

- [ ] **Step 1: 更新 RoundOutputSummary 类型**

在 `invest-committee-store.svelte.ts` 的 `RoundOutputSummary` 接口中添加：

```typescript
export interface RoundOutputSummary {
  role: string;
  round: number;
  label: string;
  parsed: { rawText: string; signal?: string; strength?: number; fallbackReason?: string };
  latencyMs: number;
  tokensUsed: number;
}
```

- [ ] **Step 2: 更新 SymbolProgress 类型**

在 `SymbolProgress` 接口中添加：

```typescript
failedSteps?: Set<number>;
```

- [ ] **Step 3: 更新 getStepState 函数**

修改 `pipeline-config.ts` 的 `getStepState` 函数：

```typescript
export function getStepState(
  symProgress: SymbolProgress | undefined,
  backendIdx: number,
  pipelineStarted: boolean,
): 'pending' | 'active' | 'done' | 'error' | 'failed' {
  if (!symProgress) return 'pending';
  if (backendIdx === -1) return pipelineStarted ? 'done' : 'pending';
  if (symProgress.activeStep === backendIdx) return 'active';
  if (symProgress.failedSteps?.has(backendIdx)) return 'failed';
  for (const round of symProgress.completedRounds) {
    if (roleToBackendIdx(round.role, round.round) === backendIdx) return 'done';
  }
  if (symProgress.done && !symProgress.error) return 'done';
  if (symProgress.error && backendIdx >= symProgress.completedSteps) return 'error';
  return 'pending';
}
```

- [ ] **Step 4: 运行 TypeScript 检查**

Run: `npm run check`
Expected: 无错误

- [ ] **Step 5: 提交**

```bash
git add src/lib/stores/invest-committee-store.svelte.ts src/lib/components/invest/pipeline-config.ts
git commit -m "feat(invest): add failedSteps to SymbolProgress and failed state to getStepState"
```

---

### Task 6: CommitteeLiveTab 前端展示

**Files:**
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`

- [ ] **Step 1: 添加 getFallbackMessage 函数**

在 `<script>` 标签中添加：

```typescript
function getFallbackMessage(reason: string): string {
  const messages: Record<string, string> = {
    'llm_unavailable': 'LLM 服务不可用，已降级为 HOLD',
    'format_mismatch': '输出格式异常，部分字段可能缺失',
    'truncated_critical': '输出过长，部分内容被截断',
    'empty_response': 'LLM 返回空内容',
    'parse_error': '解析异常',
  };
  return messages[reason] || '未知异常';
}
```

- [ ] **Step 2: 更新步骤圆点样式**

找到步骤圆点的 `style` 属性，添加 `failed` 状态：

```svelte
style={state === 'done'
  ? `background:${step.color}25; color:${step.color};`
  : state === 'active'
    ? 'background:rgba(59,130,246,0.2); color:#3b82f6;'
    : state === 'error' || state === 'failed'
      ? 'background:rgba(168,122,122,0.2); color:#a87a7a;'
      : 'background:var(--bg-input); color:var(--text-tertiary);'}
```

- [ ] **Step 3: 更新步骤圆点图标**

找到步骤圆点的图标显示逻辑：

```svelte
{#if state === 'done'}✓{:else if state === 'active'}◉{:else if state === 'error' || state === 'failed'}✗{:else}{step.key === 'regime' ? 'R' : step.key.charAt(0).toUpperCase()}{/if}
```

- [ ] **Step 4: 更新步骤 body 展示逻辑**

找到步骤 body 的展示逻辑，在 `{:else if round?.parsed?.rawText}` 之前添加：

```svelte
{:else if round?.parsed?.rawText === '[WORKER_UNAVAILABLE]'}
  <div class="flex items-center gap-2 text-[12px] text-[var(--color-error)]">
    <span>⚠</span>
    <span>LLM 调用失败，请检查网络或 API Key 配置</span>
  </div>
{:else if round?.parsed?.fallbackReason}
  <div class="flex items-center gap-2 text-[12px] text-[var(--color-warning)]">
    <span>⚡</span>
    <span>{getFallbackMessage(round.parsed.fallbackReason)}</span>
  </div>
```

- [ ] **Step 5: 运行前端检查**

Run: `npm run check`
Expected: 无错误

- [ ] **Step 6: 提交**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte
git commit -m "feat(invest): CommitteeLiveTab show friendly error for WORKER_UNAVAILABLE and fallback reasons"
```

---

## PR-C: hard_truncate 优化

### Task 7: CommitteeRole 添加 critical_field_keys 方法

**Files:**
- Modify: `src-tauri/src/invest/committee/roles.rs`

- [ ] **Step 1: 添加 critical_field_keys 方法**

在 `CommitteeRole` 的 `impl` 块中添加：

```rust
/// 返回每个角色的关键字段名列表
pub fn critical_field_keys(&self) -> &[&str] {
    match self {
        CommitteeRole::Macro => &["SIGNAL", "STRENGTH", "MARKET_PHASE", "ONE_LINER"],
        CommitteeRole::Quant => &[
            "SIGNAL", "STRENGTH", "REGIME", "ONE_LINER",
            "ADJUSTED_SIGNAL", "ADJUSTED_STRENGTH",
        ],
        CommitteeRole::Risk => &[
            "SIGNAL", "STRENGTH", "ONE_LINER", "L4_VETO",
            "ADJUSTED_SIGNAL", "ADJUSTED_STRENGTH",
        ],
        CommitteeRole::Cio => &["VERDICT", "CONFIDENCE", "ONE_LINER"],
        CommitteeRole::L4Officer => &["L4_EMOTION_ASSESSMENT", "L4_RED_LIGHT", "L4_GUARD_CLAUSE"],
    }
}
```

- [ ] **Step 2: 运行 Rust 检查**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 无错误

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/invest/committee/roles.rs
git commit -m "feat(invest): CommitteeRole add critical_field_keys method"
```

---

### Task 8: 实现新的 hard_truncate

**Files:**
- Modify: `src-tauri/src/invest/committee/roles.rs:236-252`

- [ ] **Step 1: 写失败的测试**

在 `roles.rs` 或 `parser.rs` 的测试模块中添加：

```rust
#[test]
fn test_hard_truncate_preserves_critical_fields() {
    // 构造一段文本，关键字段在末尾
    let text = "这是一段很长的推理文本，包含很多细节分析。\n\
                继续分析市场走势和技术指标。\n\
                综合以上分析得出结论。\n\
                SIGNAL: risk_on\n\
                STRENGTH: 7\n\
                ONE_LINER: 技术面偏多";
    let (result, was_truncated) = hard_truncate(text, CommitteeRole::Quant, 0);
    // 关键字段应该被保留
    assert!(result.contains("SIGNAL: risk_on"));
    assert!(result.contains("STRENGTH: 7"));
}

#[test]
fn test_hard_truncate_attempt_1_fallback() {
    let long = "这是一段超过250个汉字的测试文本".repeat(50);
    let (result, was_truncated) = hard_truncate(&long, CommitteeRole::Quant, 1);
    assert!(was_truncated);
    assert!(result.chars().count() <= 250);
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml roles::tests::test_hard_truncate_preserves_critical_fields -- --nocapture`
Expected: FAIL（当前实现不支持关键字段保留）

- [ ] **Step 3: 实现辅助函数**

在 `roles.rs` 的 `hard_truncate` 函数之前添加：

```rust
/// 找到关键字段所在的行
fn find_critical_lines(text: &str, critical_keys: &[&str]) -> Vec<String> {
    let mut lines = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let is_critical = critical_keys.iter().any(|key| {
            trimmed.starts_with(key)
                || trimmed.starts_with(&format!("{}:", key))
                || trimmed.starts_with(&format!("{}：", key))
                || trimmed.starts_with(&format!("{}=", key))
                || trimmed.starts_with(&format!("**{}**", key))
        });
        if is_critical {
            lines.push(line.to_string());
        }
    }
    lines
}

/// 找到非关键字段所在的行
fn extract_non_critical_lines(text: &str, critical_keys: &[&str]) -> Vec<String> {
    let mut lines = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let is_critical = critical_keys.iter().any(|key| {
            trimmed.starts_with(key)
                || trimmed.starts_with(&format!("{}:", key))
                || trimmed.starts_with(&format!("{}：", key))
                || trimmed.starts_with(&format!("{}=", key))
                || trimmed.starts_with(&format!("**{}**", key))
        });
        if !is_critical {
            lines.push(line.to_string());
        }
    }
    lines
}

/// 按字符数截断行列表
fn truncate_lines_to_chars(lines: Vec<String>, max_chars: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut total = 0;
    for line in lines {
        let line_len = line.chars().count();
        if total + line_len <= max_chars {
            total += line_len;
            result.push(line);
        } else {
            // 截断当前行
            let remaining = max_chars - total;
            if remaining > 0 {
                let truncated: String = line.chars().take(remaining).collect();
                result.push(truncated);
            }
            break;
        }
    }
    result
}

/// 重组关键字段和非关键字段
fn reconstruct_output(critical_lines: Vec<String>, other_lines: Vec<String>) -> String {
    let mut output = String::new();
    // 关键字段在前
    for line in &critical_lines {
        output.push_str(line);
        output.push('\n');
    }
    // 非关键字段在后
    for line in &other_lines {
        output.push_str(line);
        output.push('\n');
    }
    output.trim_end().to_string()
}
```

- [ ] **Step 4: 实现新的 hard_truncate**

替换 `roles.rs:236-252`：

```rust
/// Hard-truncate output text to the role's max character count.
/// 采用"先解析后重构"策略：第一次尝试保留关键字段，失败后降级为纯位置截断。
/// Returns (truncated_text, was_truncated).
pub fn hard_truncate(text: &str, role: CommitteeRole, attempt: u32) -> (String, bool) {
    let max = role.max_chars();
    if text.chars().count() <= max {
        return (text.to_string(), false);
    }

    if attempt == 0 {
        // 第一次尝试：保留关键字段
        let critical_keys = role.critical_field_keys();
        let critical_lines = find_critical_lines(text, critical_keys);
        let critical_len: usize = critical_lines.iter().map(|l| l.chars().count()).sum();

        if critical_len < max {
            let remaining = max - critical_len;
            let other_lines = extract_non_critical_lines(text, critical_keys);
            let truncated_other = truncate_lines_to_chars(other_lines, remaining);
            let result = reconstruct_output(critical_lines, truncated_other);
            if result.chars().count() <= max {
                return (result, true);
            }
        }
        // 回退到纯位置截断
    }

    // 纯位置截断（attempt == 1 或关键字段本身超长）
    let truncated: String = text.chars().take(max).collect();
    if let Some(last_newline) = truncated.rfind('\n') {
        (truncated[..last_newline].to_string(), true)
    } else {
        (truncated, true)
    }
}
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -- --nocapture`
Expected: 全部 PASS

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/invest/committee/roles.rs
git commit -m "feat(invest): hard_truncate preserve critical fields with fallback"
```

---

### Task 9: Prompt 约束补充

**Files:**
- Modify: `src-tauri/src/invest/committee/roles.rs` (length_constraint_suffix)

- [ ] **Step 1: 更新 length_constraint_suffix 函数**

修改 `roles.rs` 的 `length_constraint_suffix` 函数：

```rust
pub fn length_constraint_suffix(role: CommitteeRole) -> String {
    format!(
        "\n\n[输出限制：你的回复必须控制在{}个中文字符以内。关键字段（SIGNAL/VERDICT/STRENGTH 等）必须放在输出的前 3 行。]",
        role.max_chars()
    )
}
```

- [ ] **Step 2: 运行 Rust 检查**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 无错误

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/invest/committee/roles.rs
git commit -m "feat(invest): prompt constraint require critical fields in first 3 lines"
```

---

## Final Verification

- [ ] **Step 1: 运行完整 Rust 测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -- --nocapture`
Expected: 全部 PASS

- [ ] **Step 2: 运行前端检查**

Run: `npm run check && npm run lint`
Expected: 无错误

- [ ] **Step 3: 运行构建**

Run: `npm run build`
Expected: 成功

- [ ] **Step 4: 更新 changelog**

在 `docs/changelog.md` 顶部添加：

```markdown
## v5.4.0 (2026-06-11)

### 委员会解析器增强

- **解析器格式放宽**: extract_field 支持 6 种格式变体（`:`、`：`、`=`、` = `、`**KEY**:`、`**KEY**=`）
- **extract_list_field_any 统一**: 复用 extract_field 检测列表起始行，消除重复逻辑
- **降级原因记录**: ParsedFields 增加 fallback_reason 字段，按角色定义关键字段缺失条件
- **hard_truncate 优化**: 采用"先解析后重构"策略，优先保留关键字段
- **Prompt 约束**: 要求关键字段放在输出前 3 行
- **前端错误提示**: getStepState 增加 failed 状态，WORKER_UNAVAILABLE 显示友好提示
- 8 个新增单元测试
```

- [ ] **Step 5: 最终提交**

```bash
git add docs/changelog.md
git commit -m "docs: update changelog for v5.4.0 committee parser enhancement"
```
