# 委员会字段解析误报修复 + Risk/CIO 卡片分段展示 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 消除委员会直播卡片高频误报的"部分字段未识别"(`missing_critical_fields`)及其引发的最终评级被无差别降级到 HOLD,并让 Risk R1 / CIO 卡片像 Macro/Quant 一样分段展示而非一坨平铺。

**Architecture:** 单根因、三层防御。根因是 `parser.rs::matches_key_line` 只认 6 种裸 `KEY:` 格式,而 Risk/CIO 的 LLM 输出把关键字段包进 markdown(`**SIGNAL: concerned**`、`- **集中度**:`、行内 `|` 多字段)。修复分三层:(1) parser 加固容错为根本兜底;(2) Risk/CIO prompt 收紧输出格式以减少触发;(3) analysis 层把"软 fallback(仅缺结构化字段)"与"硬 fallback(节点不可用)"分离,软 fallback 不再全局降级。前端再补 Risk/CIO 的分段字段渲染。

**Tech Stack:** Rust (Tauri backend, `invest/committee/`),SvelteKit + Svelte 5 runes (frontend),Vitest,i18n 双语 JSON。

## Global Constraints

- 测试运行环境限制(CLAUDE.md §11):本机 `cargo test` 运行期有 `STATUS_ENTRYPOINT_NOT_FOUND` 已知问题。每个 Rust 任务的"运行测试"步骤先尝试 `cargo test`,若运行期崩溃则降级用 `cargo check --manifest-path src-tauri/Cargo.toml --tests` 确保编译通过,测试逻辑靠 review 把关。命令里写出两条。
- Rust 质量门槛:`cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` 必须零警告;`cargo fmt --manifest-path src-tauri/Cargo.toml --check` 必须通过。
- 任何 UI 文案改动必须同时更新 `messages/en.json` 与 `messages/zh-CN.json`,并通过 `npm run i18n:check`。
- Conventional Commits(`feat:`/`fix:`/`chore:`)。
- Windows-first:不引入 WSL/POSIX 假设。
- 不破坏现有已通过的 parser/analysis 测试(回归保护)。
- 根因证据(真实归档样本):`~/.claw-go/invest/committee/2026-06-20/002384.SZ_东山精密.md`、`2026-06-19/002735.SZ_王子新材.md`。Risk R1 实际输出形如 `**SIGNAL: concerned** | **强度: 6/10**`、`**集中度:** 30.5%`、`- **集中度**: ...`;Macro/Quant 为裸 `信号: neutral`。

---

### Task 1: Parser 加固 — 行预处理(剥列表前缀 + markdown 包裹)

**Files:**
- Modify: `src-tauri/src/invest/committee/parser.rs`(`matches_key_line` 约 182-196 行;新增 helper)
- Test: 同文件 `#[cfg(test)] mod tests`(约 512 行起)

**Interfaces:**
- Produces: `fn normalize_key_line(line: &str) -> String` — 接收已 `trim()` 的单行,剥离行首列表/引用前缀(`- ` `* ` `+ ` `> ` `1. ` 等)与整体包裹的成对 `**`,返回归一化后的行供 `matches_key_line` 匹配。
- Produces: `matches_key_line` 行为升级 — 现有调用方(`extract_field` 285 行、`extract_list_field_any` 375 行、`is_structured_key_line` 277 行)签名不变,仅匹配能力增强。

- [ ] **Step 1: Write the failing test**

在 `mod tests` 内新增:

```rust
#[test]
fn test_matches_markdown_wrapped_signal() {
    // Risk R1 真实形态:整行被 ** 包裹,冒号在加粗内部
    let parsed = parse_role_output(CommitteeRole::Risk, "**SIGNAL: concerned**", false);
    assert_eq!(parsed.signal.as_deref(), Some("concerned"));
}

#[test]
fn test_matches_list_prefixed_bold_key() {
    // 002735 真实形态:列表前缀 + 加粗 key
    let parsed = parse_role_output(CommitteeRole::Risk, "- **集中度**: 30.5%", false);
    assert_eq!(parsed.concentration_pct, Some(30.5));
}

#[test]
fn test_matches_bold_key_colon_inside() {
    // 002384 真实形态:**集中度:** 30.5%
    let parsed = parse_role_output(CommitteeRole::Risk, "**集中度:** 30.5%", false);
    assert_eq!(parsed.concentration_pct, Some(30.5));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::parser::tests::test_matches_markdown_wrapped_signal -- --nocapture`
Expected: FAIL(`signal` 为 `None`,断言不等)。若运行期崩溃,改用 `cargo check --manifest-path src-tauri/Cargo.toml --tests` 确认测试已编译进去。

- [ ] **Step 3: Write minimal implementation**

在 `matches_key_line`(182 行)正上方新增 helper:

```rust
/// 归一化一行以便 key 检测:剥离行首列表/引用前缀,以及整体包裹的成对 `**`。
/// 不改变字段值本身的内部内容(strip_markdown_formatting 仍在值阶段处理)。
fn normalize_key_line(line: &str) -> String {
    let mut s = line.trim();
    // 剥列表/引用前缀(可能叠加,如 "- > "):循环剥一层
    loop {
        let stripped = s
            .strip_prefix("- ")
            .or_else(|| s.strip_prefix("* "))
            .or_else(|| s.strip_prefix("+ "))
            .or_else(|| s.strip_prefix("> "));
        match stripped {
            Some(rest) => s = rest.trim_start(),
            None => break,
        }
    }
    // 剥有序列表前缀 "N. " / "N、"
    if let Some(pos) = s.find(['.', '、']) {
        if pos > 0 && pos <= 3 && s[..pos].chars().all(|c| c.is_ascii_digit()) {
            s = s[pos + s[pos..].chars().next().map_or(1, |c| c.len_utf8())..].trim_start();
        }
    }
    // 若整行被一对 ** 包裹(首尾都是 **),剥掉首尾 ** 让冒号暴露出来
    let mut out = s.to_string();
    if out.starts_with("**") && out.ends_with("**") && out.len() >= 4 {
        out = out[2..out.len() - 2].to_string();
    }
    out
}
```

修改 `matches_key_line`(182 行)首行,先归一化再匹配。把函数体第一行改为基于归一化后的串。由于返回的是 `&'a str` 借用,需调整为先在调用处归一化——改 `extract_field`(284 行)更稳妥:

```rust
fn extract_field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let normalized = normalize_key_line(line);
        if let Some(rest) = matches_key_line(&normalized, key) {
            let val = rest.trim().to_string();
            return Some(strip_markdown_formatting(&val));
        }
    }
    None
}
```

> 注:`matches_key_line` 签名与函数体保持不变(仍按 6 种前缀匹配),归一化在 `extract_field` 调用前完成。`extract_list_field_any`(375 行)与 `is_structured_key_line`(277 行)在 Task 3 同步。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::parser::tests -- --nocapture`
Expected: 三个新测试 PASS,原有 parser 测试全部 PASS。崩溃则 `cargo check --tests` 通过即可。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/committee/parser.rs
git commit -m "fix(committee): parser 容忍 markdown 包裹与列表前缀的 key 行"
```

---

### Task 2: Parser 加固 — 行内 `|` 多字段拆分

**Files:**
- Modify: `src-tauri/src/invest/committee/parser.rs`(`extract_field` 284 行)
- Test: 同文件 `mod tests`

**Interfaces:**
- Consumes: Task 1 的 `normalize_key_line`。
- Produces: `extract_field` 支持单行内 `|`(含全角 `｜`)分隔的多个 `KEY: VALUE` 片段。

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_inline_pipe_multi_field() {
    // Risk R1 真实形态:同一行两个字段用 | 分隔,且整体加粗分段
    let text = "**SIGNAL: concerned** | **强度: 6/10**";
    let parsed = parse_role_output(CommitteeRole::Risk, text, false);
    assert_eq!(parsed.signal.as_deref(), Some("concerned"));
    assert_eq!(parsed.strength, Some(6.0));
}

#[test]
fn test_pipe_inside_value_not_split() {
    // 反例:值里含 | 但不是多字段(止损条件描述),不应被破坏
    let text = "调整止损: 跌破MA20 | 或浮盈归零即减仓";
    let parsed = parse_role_output(CommitteeRole::Risk, text, false);
    assert_eq!(parsed.adjusted_stop_loss.as_deref(), Some("跌破MA20 | 或浮盈归零即减仓"));
}
```

> 注:`强度: 6/10` 的值是 `6/10`,`extract_f64` 用 `parse::<f64>()` 会失败。需在 strength 提取处容忍 `N/M` 取分子。见 Step 3。

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::parser::tests::test_inline_pipe_multi_field -- --nocapture`
Expected: FAIL(`signal=None` 或 `strength=None`)。

- [ ] **Step 3: Write minimal implementation**

改 `extract_field`(已在 Task 1 改过一次,这里在其中增加 `|` 片段遍历):

```rust
fn extract_field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        // 整行先尝试归一化匹配
        let normalized = normalize_key_line(line);
        if let Some(rest) = matches_key_line(&normalized, key) {
            return Some(strip_markdown_formatting(rest.trim()));
        }
        // 行内多字段:仅当出现 | 且每段都像 key: value 时,按 | 拆分逐段尝试
        if line.contains('|') || line.contains('｜') {
            for seg in line.split(['|', '｜']) {
                let seg_norm = normalize_key_line(seg);
                // 段内必须自身是 KEY: VALUE 结构(冒号/等号位置合法)才接受,避免拆裂正常值
                if is_structured_key_line(&seg_norm) {
                    if let Some(rest) = matches_key_line(&seg_norm, key) {
                        return Some(strip_markdown_formatting(rest.trim()));
                    }
                }
            }
        }
    }
    None
}
```

为支持 `强度: 6/10`,改 `extract_f64`(306 行)解析容错:

```rust
fn extract_f64(text: &str, key: &str) -> Option<f64> {
    extract_field(text, key).and_then(|v| parse_leading_f64(&v))
}

/// 从值串中解析前导数字:支持 "6"、"6.5"、"6/10"(取分子)、"30.5%"(去尾符)。
fn parse_leading_f64(v: &str) -> Option<f64> {
    let v = v.trim();
    // "N/M" 形态取分子
    let head = v.split(['/', '%', ' ', '（', '(']).next().unwrap_or(v).trim();
    head.parse::<f64>().ok()
}
```

> `parse_leading_f64` 同时让 `集中度: 30.5%` 这类带 `%` 的值也能解析,与 Task 1 第三个测试协同(那里值是 `30.5%`)。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::parser::tests -- --nocapture`
Expected: 全部 PASS,含反例 `test_pipe_inside_value_not_split`(因 `或浮盈归零即减仓` 段不是 `key: value` 结构,不拆分)。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/committee/parser.rs
git commit -m "fix(committee): parser 支持行内 | 多字段拆分与 N/M、% 数值容错"
```

---

### Task 3: Parser 加固 — 续行合并同步识别包裹 key 行

**Files:**
- Modify: `src-tauri/src/invest/committee/parser.rs`(`is_structured_key_line` 277 行、`merge_continuation_lines` 234 行)
- Test: 同文件 `mod tests`

**Interfaces:**
- Consumes: Task 1 的 `normalize_key_line`。
- Produces: `is_structured_key_line` 对"被 markdown 包裹/带列表前缀的 key 行"返回 `true`,使其不被 `merge_continuation_lines` 误并入上一字段。

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_wrapped_key_not_merged_as_continuation() {
    // 多行:裸 key 行后跟一个被 ** 包裹的 key 行,后者不应被并入前者的值
    let text = "标的风险: 估值偏高\n**SIGNAL: concerned**\n强度: 6";
    let parsed = parse_role_output(CommitteeRole::Risk, text, false);
    assert_eq!(parsed.signal.as_deref(), Some("concerned"));
    assert_eq!(parsed.stock_risk_summary.as_deref(), Some("估值偏高"));
    assert_eq!(parsed.strength, Some(6.0));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::parser::tests::test_wrapped_key_not_merged_as_continuation -- --nocapture`
Expected: FAIL(`**SIGNAL: concerned**` 被当续行并入 `标的风险` 值,`signal=None`)。

- [ ] **Step 3: Write minimal implementation**

改 `is_structured_key_line`(277 行),先归一化再判断:

```rust
fn is_structured_key_line(line: &str) -> bool {
    let line = normalize_key_line(line);
    if let Some(pos) = line.find(':').or_else(|| line.find('：')).or_else(|| line.find('=')) {
        pos > 0 && pos < 30 && !line[..pos].contains(' ')
    } else {
        false
    }
}
```

`merge_continuation_lines`(234 行)内部已调用 `is_structured_key_line` 判断 `is_structured_key`,无需再改——但注意它在 246-249 行用 `result.push_str(line.trim_end())` 写入的是**原始行**(含 `**`),这没问题:`extract_field` 在 Task 1/2 已能归一化提取。确认 `is_list_item`(242 行)判断不会把 `- **集中度**:` 当普通列表项吞掉:当前 `is_list_item = starts_with("- ")` 会把它判为列表项进入 else 分支正常换行追加(不并入上行),提取阶段再靠归一化处理 — 可接受。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::parser::tests -- --nocapture`
Expected: 全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/committee/parser.rs
git commit -m "fix(committee): 续行合并识别被包裹的 key 行,避免误并字段"
```

---

### Task 4: Parser 加固 — Risk R1 完整真实样本端到端测试

**Files:**
- Modify: `src-tauri/src/invest/committee/parser.rs`(`mod tests`)
- 引用: `detect_fallback_reason`(146 行)

**Interfaces:**
- Consumes: Task 1-3 的全部加固。验证 `detect_fallback_reason(Risk, 1, ..)` 对真实样本返回 `None`。

- [ ] **Step 1: Write the failing test**

粘贴 002384 风控段的精简真实文本:

```rust
#[test]
fn test_real_risk_r1_no_false_fallback() {
    let text = "## 🛡️ Risk Officer 裁决 — 002384.SZ (东山精密)\n\
                \n---\n\n\
                **SIGNAL: concerned** | **强度: 6/10**\n\
                \n### 一、用户财务风险\n\n\
                **集中度:** 30.5%，处于策略单一资产上限70%以内\n\
                **可用子弹:** 460.63 CNY，账户现金近乎枯竭\n\
                **盈亏比:** +37.6%，浮盈丰厚\n\
                **标的风险:** PE=189.4 极高，估值透支增长预期";
    let parsed = parse_role_output(CommitteeRole::Risk, text, false);
    assert_eq!(parsed.signal.as_deref(), Some("concerned"));
    assert_eq!(parsed.strength, Some(6.0));
    // 关键:不再误报 fallback
    assert_eq!(detect_fallback_reason(CommitteeRole::Risk, 1, &parsed), None);
}
```

- [ ] **Step 2: Run test to verify it fails (or passes early)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::parser::tests::test_real_risk_r1_no_false_fallback -- --nocapture`
Expected: 若 Task 1-3 已生效则直接 PASS;若仍 FAIL 说明前序加固有缺口,回到 Task 1-3 修补(不在此新增实现)。

- [ ] **Step 3: (no new impl — 验证性测试)**

本任务只锁定端到端行为,不新增实现代码。若失败,定位是 signal/strength/集中度哪一项没提出来,回对应 Task 修。

- [ ] **Step 4: Run full parser suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::parser -- --nocapture`
Expected: 全绿。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/committee/parser.rs
git commit -m "test(committee): 锁定 Risk R1 真实样本不再误报 fallback"
```

---

### Task 5: analysis 层 — 软/硬 fallback 分离

**Files:**
- Modify: `src-tauri/src/invest/committee/analysis.rs`(`cio_sanity_check` 133 行;fallback 检查 201-214 行;高信念门槛 223 行)
- Test: 同文件 `mod tests`(调整 `test_sanity_fallback_reason_without_marker` 561 行、`test_high_conviction_skipped_on_fallback` 672 行)

**Interfaces:**
- Produces: `fn is_hard_fallback(reason: &str) -> bool` — 与前端 `CommitteeLiveTab.svelte:85` 的 `HARD_FALLBACKS` 口径一致(`worker_unavailable` / `empty_text` / `cli_executor_none` / 以 `cli_error` 开头)。
- 行为:软 fallback(`missing_critical_fields` 等)不再触发全局 HOLD 降级,也不阻止高信念升级。

- [ ] **Step 1: Update the now-inverted tests + add hard-fallback test**

把 `test_sanity_fallback_reason_without_marker`(561-581 行)语义反转:

```rust
#[test]
fn test_soft_fallback_does_not_force_hold() {
    // missing_critical_fields 是软 fallback:有原文仅缺结构化字段,不应把整盘压成 HOLD
    let cio = ParsedFields {
        verdict: Some("BUY".to_string()),
        confidence: Some(0.8),
        ..Default::default()
    };
    let outputs = vec![RoundOutput {
        role: CommitteeRole::Quant,
        round: 2,
        parsed: ParsedFields {
            raw_text: "保护触发: yes".to_string(),
            fallback_reason: Some("missing_critical_fields".to_string()),
            ..Default::default()
        },
        latency_ms: 0,
        tokens_used: 0,
    }];
    let result = cio_sanity_check(&cio, &outputs, "risk_on", None, Mode::Holding);
    assert_eq!(result.final_verdict, "BUY"); // 保留 CIO 原裁决,不再降级
}

#[test]
fn test_hard_fallback_still_forces_hold() {
    let cio = ParsedFields { verdict: Some("BUY".to_string()), confidence: Some(0.8), ..Default::default() };
    let outputs = vec![RoundOutput {
        role: CommitteeRole::Risk,
        round: 1,
        parsed: ParsedFields {
            raw_text: "[WORKER_UNAVAILABLE] cli failed".to_string(),
            fallback_reason: Some("cli_error: timeout".to_string()),
            ..Default::default()
        },
        latency_ms: 0,
        tokens_used: 0,
    }];
    let result = cio_sanity_check(&cio, &outputs, "risk_on", None, Mode::Holding);
    assert_eq!(result.final_verdict, "HOLD");
    assert!(result.final_confidence <= 0.4);
}
```

改 `test_high_conviction_skipped_on_fallback`(672-680 行):把阻止升级的 fallback 从软改硬:

```rust
#[test]
fn test_high_conviction_skipped_on_hard_fallback() {
    let cio = ParsedFields { verdict: Some("HOLD".to_string()), confidence: Some(0.35), ..Default::default() };
    let mut outputs = high_conviction_outputs("bullish", 7.0, "ok");
    outputs[0].parsed.raw_text = "[WORKER_UNAVAILABLE]".to_string();
    outputs[0].parsed.fallback_reason = Some("worker_unavailable".to_string());
    let result = cio_sanity_check(&cio, &outputs, "risk_on", Some(7.0), Mode::Holding);
    assert_eq!(result.final_verdict, "HOLD");
}

#[test]
fn test_high_conviction_not_blocked_by_soft_fallback() {
    // 软 fallback 不阻止高信念升级
    let cio = ParsedFields { verdict: Some("HOLD".to_string()), confidence: Some(0.35), ..Default::default() };
    let mut outputs = high_conviction_outputs("bullish", 7.0, "ok");
    outputs[0].parsed.fallback_reason = Some("missing_critical_fields".to_string());
    let result = cio_sanity_check(&cio, &outputs, "risk_on", Some(7.0), Mode::Holding);
    assert!(matches!(result.final_verdict.as_str(), "ACCUMULATE" | "BUY"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::analysis::tests -- --nocapture`
Expected: 新/改测试 FAIL(当前软 fallback 仍降级)。

- [ ] **Step 3: Write minimal implementation**

在 `analysis.rs` 加 helper(放 `cio_sanity_check` 之前):

```rust
/// 硬 fallback = 工作节点真不可用/输出真空,必须降级 HOLD。
/// 软 fallback(missing_critical_fields 等)= 有原文仅缺结构化字段,不全局降级。
/// 口径须与前端 CommitteeLiveTab.svelte 的 HARD_FALLBACKS 一致。
fn is_hard_fallback(reason: &str) -> bool {
    matches!(reason, "worker_unavailable" | "empty_text" | "cli_executor_none")
        || reason.starts_with("cli_error")
}
```

改 fallback 检查(201-214 行):

```rust
    // 仅硬 fallback 触发全局 HOLD;软 fallback(仅缺结构化字段,原文完整)不降级。
    // [WORKER_UNAVAILABLE] marker 始终视为硬不可用。
    let has_unavailable = round_outputs.iter().any(|o| {
        o.parsed.raw_text.contains("[WORKER_UNAVAILABLE]")
            || o.parsed
                .fallback_reason
                .as_deref()
                .map_or(false, is_hard_fallback)
    });
    if has_unavailable {
        result.final_verdict = "HOLD".to_string();
        result.final_confidence = result.final_confidence.min(0.4);
        result
            .notes
            .push("工作节点不可用或输出异常，降级为HOLD".to_string());
    }
```

高信念门槛(223 行)的 `!has_unavailable` 现在自动复用新口径(因 `has_unavailable` 已只含硬 fallback),无需再改。

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::analysis -- --nocapture`
Expected: 全部 PASS。崩溃则 `cargo check --tests`。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/committee/analysis.rs
git commit -m "fix(committee): 软 fallback 不再全局降级 HOLD,仅硬不可用降级"
```

---

### Task 6: roles.rs — Risk/CIO prompt 收紧输出格式

**Files:**
- Modify: `src-tauri/src/invest/committee/roles.rs`(`RISK_PROMPT` 412 行、`RISK_R2_PROMPT` 461 行、`CIO_PROMPT` 498 行)
- Test: 同文件 `mod tests`

**Interfaces:**
- Produces: 三个 prompt 常量在输出格式块前包含裸 key 硬约束文本,可被字符串断言验证。

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_risk_prompt_has_bare_key_constraint() {
    assert!(RISK_PROMPT.contains("禁止"));
    assert!(RISK_PROMPT.contains("独占一行"));
    assert!(CIO_PROMPT.contains("独占一行"));
    assert!(RISK_R2_PROMPT.contains("独占一行"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::roles::tests::test_risk_prompt_has_bare_key_constraint -- --nocapture`
Expected: FAIL(常量暂无该文案)。

- [ ] **Step 3: Write minimal implementation**

在每个 prompt 的输出格式块(`风险信号: ...` / `调整风险信号: ...` / CIO 的 `裁决: ...`)正上方插入统一硬约束行。例如 `RISK_PROMPT`(459 行附近,`风险信号:` 之前)插入:

```
**关键字段格式硬约束（必须遵守）**：
- 关键字段（风险信号/强度/集中度/可用子弹/盈亏比/标的风险）必须各自独占一行，使用裸 `字段名: 值` 格式
- 禁止给这些字段加 `**` 加粗、`#` 标题、`-`/`*` 列表符号
- 禁止把多个字段写在同一行用 `|` 分隔
- 报告正文的叙述/推理段落可自由使用 markdown，但上述关键字段行必须保持裸格式
```

`CIO_PROMPT` 与 `RISK_R2_PROMPT` 同理,字段名替换为各自的关键字段(CIO:裁决/置信度;Risk R2:调整风险信号)。三处都包含"独占一行""禁止"字样以满足测试。

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml invest::committee::roles -- --nocapture`
Expected: PASS,原有 roles 测试全绿。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/committee/roles.rs
git commit -m "fix(committee): 收紧 Risk/CIO prompt,强制关键字段裸 key 行"
```

---

### Task 7: 前端 — ParsedFields 类型补字段

**Files:**
- Modify: `src/lib/stores/invest-committee-store.svelte.ts`(`RoundOutputSummary.parsed` 31-61 行)

**Interfaces:**
- Produces: 前端 `parsed` 类型新增可选字段:`worstCaseLossPct?`、`adjustedStopLoss?`、`dominantView?`、`suggestedAllocCny?`、`stopLossPrice?`、`executionPlan?`、`riskPlan?`、`personalNote?`。后端 `ParsedFields` 已全字段 camelCase 序列化(`RoundOutputSummary.parsed: ParsedFields` 直接 clone),无需改后端。
- Consumes: Task 8 的 `roleFields()` 会读这些字段。

- [ ] **Step 1: Add fields (类型变更,无独立单测)**

在 31-61 行的 `parsed` 对象类型内,Risk/CIO 分区补字段:

```typescript
    // Risk
    concentrationPct?: number;
    dryPowderCny?: number;
    pnlPct?: number;
    worstCaseLossPct?: number;
    stockRiskSummary?: string;
    adjustedStopLoss?: string;
    // CIO
    catalystTier?: string;
    catalystSummary?: string;
    dominantView?: string;
    suggestedAllocCny?: number;
    stopLossPrice?: number;
    executionPlan?: string;
    riskPlan?: string;
    personalNote?: string;
```

- [ ] **Step 2: Verify type-checks**

Run: `npm run check`
Expected: 无新增类型错误。

- [ ] **Step 3: Commit**

```bash
git add src/lib/stores/invest-committee-store.svelte.ts
git commit -m "feat(committee): 前端 ParsedFields 补 Risk/CIO 结构化字段"
```

---

### Task 8: 前端 — Risk/CIO 分段字段渲染 + i18n

**Files:**
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`(`roleFields()` 53-82 行)
- Modify: `messages/en.json`、`messages/zh-CN.json`(`invest_field_*` 区,en 约 457-470 行)
- Consumes: Task 7 的类型字段。

**Interfaces:**
- 新增 i18n key:`invest_field_worst_case`、`invest_field_adjusted_stop`、`invest_field_dominant_view`、`invest_field_suggested_alloc`、`invest_field_stop_loss`、`invest_field_exec_plan`、`invest_field_risk_plan`、`invest_field_personal_note`。

- [ ] **Step 1: Add i18n keys (双语)**

`messages/en.json`(现有 `invest_field_*` 区末尾,约 470 行后)新增:

```json
  "invest_field_worst_case": "Worst-Case Loss",
  "invest_field_adjusted_stop": "Adjusted Stop",
  "invest_field_dominant_view": "Dominant View",
  "invest_field_suggested_alloc": "Suggested Alloc",
  "invest_field_stop_loss": "Stop-Loss Price",
  "invest_field_exec_plan": "Execution Plan",
  "invest_field_risk_plan": "Risk Plan",
  "invest_field_personal_note": "Note",
```

`messages/zh-CN.json` 对应区新增:

```json
  "invest_field_worst_case": "最大回撤",
  "invest_field_adjusted_stop": "调整止损",
  "invest_field_dominant_view": "主流观点",
  "invest_field_suggested_alloc": "建议配置",
  "invest_field_stop_loss": "止损价",
  "invest_field_exec_plan": "执行计划",
  "invest_field_risk_plan": "风控计划",
  "invest_field_personal_note": "备注",
```

- [ ] **Step 2: Extend roleFields()**

把 `CommitteeLiveTab.svelte` 的 `roleFields()`(53-82 行)Risk/CIO 分支扩展:

```typescript
    } else if (role === 'risk') {
      push(t('invest_field_concentration'), pf.concentrationPct, '%');
      push(t('invest_field_dry_powder'), pf.dryPowderCny != null ? `¥${pf.dryPowderCny}` : null);
      push(t('invest_field_pnl'), pf.pnlPct != null ? `${pf.pnlPct}%` : null);
      push(t('invest_field_worst_case'), pf.worstCaseLossPct != null ? `${pf.worstCaseLossPct}%` : null);
      push(t('invest_field_adjusted_stop'), pf.adjustedStopLoss);
      push(t('invest_field_stock_risk'), pf.stockRiskSummary);
    } else if (role === 'cio') {
      push(t('invest_field_catalyst_tier'), pf.catalystTier);
      push(t('invest_field_dominant_view'), pf.dominantView);
      push(t('invest_field_exec_mode'), pf.executionMode);
      push(t('invest_field_first_tranche'), pf.firstTrancheCny != null ? `¥${pf.firstTrancheCny}` : null);
      push(t('invest_field_suggested_alloc'), pf.suggestedAllocCny != null ? `¥${pf.suggestedAllocCny}` : null);
      push(t('invest_field_stop_loss'), pf.stopLossPrice != null ? `¥${pf.stopLossPrice}` : null);
      push(t('invest_field_catalyst'), pf.catalystSummary);
      push(t('invest_field_exec_plan'), pf.executionPlan);
      push(t('invest_field_risk_plan'), pf.riskPlan);
      push(t('invest_field_personal_note'), pf.personalNote);
    }
```

> `personalNote`/`executionPlan`/`riskPlan` 可能较长,`field-v` 已是自适应文本,会自然换行。`role-reasoning`(274 行)与兜底 rawText 分支(275 行)保持不变。

- [ ] **Step 3: Verify check + build + i18n**

Run: `npm run check && npm run i18n:check`
Expected: 类型 OK;i18n 双语 key 齐全无缺。

- [ ] **Step 4: Commit**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte messages/en.json messages/zh-CN.json
git commit -m "feat(committee): Risk/CIO 卡片分段展示结构化字段"
```

---

### Task 9: 全量验证 + 端到端回归

**Files:** 无代码改动,仅验证。

- [ ] **Step 1: Rust 全量质量门**

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml invest::committee -- --nocapture
```
Expected: clippy 零警告;fmt 通过;测试全绿(崩溃则 `cargo check --tests` 兜底)。

- [ ] **Step 2: 前端全量**

```bash
npm run check
npm run build
npm run i18n:check
```
Expected: 全通过。

- [ ] **Step 3: 端到端手测(最有力验证)**

```bash
npm run tauri dev
```
操作:/invest → 委员会直播 → 跑 002384.SZ(东山精密)与 002735.SZ(王子新材)。确认:
1. Risk R1 卡片不再出现 "⚠ 部分字段未识别" chip;
2. Risk R1 卡片分段展示集中度/子弹/盈亏/最大回撤/标的风险,而非一坨平铺;
3. CIO 卡片字段分段、reasoning 可读;
4. 最终裁决不再被无故压成 HOLD/40%(除非确有硬 fallback 或 Gate1/Gate2 触发);
5. 回归:跑一个 Macro/Quant 本就正常的标的,展示与降级行为无变化。

- [ ] **Step 4: 更新 changelog**

在 `docs/changelog.md` 顶部当前版本段补一条:parser 容错 markdown 包裹/列表/行内多字段 + 软 fallback 不再全局降级 + Risk/CIO 卡片分段展示。

- [ ] **Step 5: Commit**

```bash
git add docs/changelog.md
git commit -m "docs(changelog): 委员会字段解析误报修复 + Risk/CIO 分段展示"
```

---

## 风险与注意

- **自定义 prompt 覆盖**:若用户曾在设置里自定义 Risk/CIO prompt 存到磁盘(`get_prompt_dir()`),`load_prompt_for_round` 优先读磁盘,Task 6 改默认常量对其无效。Task 1-4 的 parser 加固是真正兜底,不依赖 prompt。实施后提示用户:如曾自定义 prompt,建议重置或同步加裸 key 约束。
- **行内 `|` 拆分保守性**:Task 2 必须保证只在每段都是 `key: value` 结构时才拆,反例测试 `test_pipe_inside_value_not_split` 是护栏。
- **降级语义反转**:Task 5 反转 `missing_critical_fields` 处理。已确认仅 `cio_sanity_check` 一处消费 `fallback_reason` 做降级;前端 chip(`CommitteeLiveTab.svelte:261`)仍展示软 fallback 标记,不受影响。
- **`normalize_key_line` 有序列表剥离**:Step 3 (Task 1) 中数字前缀剥离的字节切片逻辑要小心中文标点 `、` 的 UTF-8 宽度,测试需覆盖 `1. ` 和 `1、` 两种。若实现复杂可简化为只处理 `N. ` ASCII 形态。

## Self-Review

- **Spec 覆盖**:诉求1(误报降级)→ Task 1-5;诉求2(分段展示)→ Task 7-8;双管齐下(parser+prompt)→ Task 1-4 + Task 6;软/硬分离 → Task 5。全覆盖。
- **占位符扫描**:无 TBD/TODO;每个代码步骤含完整代码块。
- **类型一致性**:`is_hard_fallback`(Task 5)与前端 `HARD_FALLBACKS`(CommitteeLiveTab.svelte:85)口径一致;`normalize_key_line` 在 Task 1 定义,Task 2/3 复用;前端字段名(Task 7)与 `roleFields()` 使用(Task 8)、与后端 camelCase 序列化一致。
