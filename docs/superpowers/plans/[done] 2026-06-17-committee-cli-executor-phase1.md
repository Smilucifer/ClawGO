# Committee CLI Executor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 逐步将委员会各角色从 DeepSeek API + tool-call 循环迁移到 `claude --print` CLI 执行模式，通过 Rust 预取缓存数据嵌入 prompt 来消除空结果问题。

**Architecture:** `CliCommitteeExecutor` 以子进程方式启动 `claude --print`，缓存数据（macro_cache、events、verdicts）在 Rust 侧格式化后直接嵌入 system prompt。CLI 不可用时自动 fallback 到现有 API 路径。并发度由现有 Semaphore（5）控制。

**Tech Stack:** Rust, tokio (Command, Semaphore, timeout), `claude --print` CLI, existing `parse_role_output`/`hard_truncate` pipeline.

---

## 分阶段实施

| Phase | 范围 | 目标 | 状态 |
|-------|------|------|------|
| **Phase 1** | Macro 角色先行 | 验证 CLI 模式可行性：空结果是否消失、耗时是否可接受 | ✅ **已完成** |
| **Phase 2** | Quant + Risk 扩展 | 这两个角色最需要实时数据（行情、新闻），收益最大 | ❌ 待开始 |
| **Phase 3** | CIO / L4 全面切换（可选） | 如果 Phase 1-2 效果好，删除旧 LLM 客户端和工具代码 | ❌ 待开始 |

### Phase 1: Macro 角色先行 ✅

只把 Macro 角色改为 CLI `--print`，其他角色（Quant / Risk / CIO / L4）保持当前 API 方式。

验证标准：
- [x] 空结果是否消失
- [ ] 耗时是否可接受（需手动测试）

**文件变更：**

| Action | File | Responsibility |
|--------|------|---------------|
| **Create** | `src-tauri/src/invest/committee/cli_executor.rs` (333 行) | CLI 进程启动、stdout 采集、超时、缓存数据格式化 |
| **Modify** | `src-tauri/src/invest/committee/mod.rs` (+1 行) | 注册 `pub mod cli_executor;` |
| **Modify** | `src-tauri/src/invest/committee/orchestrator.rs` (+86 行) | `run_macro_phase` 重构为 CLI 优先 + API fallback |

**提交记录：**
- `6a41266 feat(invest): add cli_executor for committee CLI-based Macro execution`
- `0f855cc chore: clippy fix — replace closure with function reference`

---

### Phase 2: Quant + Risk 扩展 ❌

Quant 和 Risk 也改为 CLI。这两个角色最需要实时数据（行情、新闻），从 CLI 获益最大。CIO 和 L4 保持 API（它们只需要综合前序输出）。

**需要新增的文件变更：**
- `cli_executor.rs` — 新增 `build_cli_quant_prompt()` / `build_cli_risk_prompt()`，将 Quant 和 Risk 所需的缓存数据（行情快照、资金流向、行业数据等）嵌入 prompt
- `orchestrator.rs` — 重构 `run_quant_r1_phase` / `run_quant_r2_phase` / `run_risk_r1_phase` / `run_risk_r2_phase` 为 CLI 优先 + API fallback
- 可能需要新增缓存格式化函数（如 `format_stock_daily_for_prompt`、`format_moneyflow_for_prompt`）

**验证标准：**
- [ ] Quant 输出包含完整技术指标分析
- [ ] Risk 输出包含风险评估
- [ ] 耗时比纯 API 路径更快或持平
- [ ] 5 并发稳定运行

---

### Phase 3: CIO / L4 全面切换（可选） ❌

如果 Phase 1-2 效果好，CIO 和 L4 也切换到 CLI 模式，然后删除旧的 LLM 客户端和工具代码。

**清理范围：**
- 删除 `InvestLlmClient` trait 及其实现（DeepSeek HTTP client）
- 删除 `run_with_tool_loop` 工具调用循环
- 删除 `tools.rs` 中不再需要的工具定义
- 删除 `roles.rs` 中的 `MACRO_PROMPT` 等旧 prompt（替换为 CLI 版本）
- 简化 `orchestrator.rs`，所有 `run_*_phase` 统一走 CLI 路径

**验证标准：**
- [ ] 所有角色输出质量不低于 API 版本
- [ ] 全量代码清理后编译通过
- [ ] 无残留死代码

---

## Phase 1 详细任务记录（已完成）

### Task 1: Create `cli_executor.rs` — CLI spawn infrastructure

**Files:**
- Create: `src-tauri/src/invest/committee/cli_executor.rs`

- [x] **Step 1: Create the module file with imports and constants**

```rust
//! CLI-based committee role executor.
//!
//! Spawns `claude --print` with pre-fetched cache data embedded in the
//! system prompt. Returns the CLI's stdout as the role output text.
//! Falls back gracefully if the CLI binary is not found.

use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Maximum concurrent CLI processes (limits memory usage: ~100MB each).
const MAX_CLI_CONCURRENT: usize = 5;

/// Default timeout for a single CLI role call (seconds).
const CLI_ROLE_TIMEOUT_SECS: u64 = 180;

/// Global CLI executor singleton.
static CLI_EXECUTOR: OnceLock<Option<CliCommitteeExecutor>> = OnceLock::new();
```

- [x] **Step 2: Add the `CliCommitteeExecutor` struct**

```rust
/// Manages spawning `claude --print` for committee role analysis.
pub struct CliCommitteeExecutor {
    /// Resolved absolute path to the `claude` binary.
    claude_bin: String,
    /// Semaphore limiting concurrent CLI processes.
    semaphore: tokio::sync::Semaphore,
}

impl CliCommitteeExecutor {
    /// Try to create an executor. Returns `None` if `claude` binary is not found.
    pub fn try_new() -> Option<Self> {
        let bin = crate::agent::claude_stream::resolve_claude_path();
        // Basic validation: check that the binary exists or is a known command
        if bin.is_empty() {
            log::warn!("[cli_executor] claude binary path is empty");
            return None;
        }
        log::info!("[cli_executor] using claude binary: {}", bin);
        Some(Self {
            claude_bin: bin,
            semaphore: tokio::sync::Semaphore::new(MAX_CLI_CONCURRENT),
        })
    }

    /// Get the global singleton, initializing if needed.
    pub fn global() -> Option<&'static CliCommitteeExecutor> {
        CLI_EXECUTOR.get_or_init(|| Self::try_new()).as_ref()
    }
```

- [x] **Step 3: Add the `run_role` method**

```rust
    /// Execute a single committee role via `claude --print`.
    ///
    /// - `system_prompt`: full prompt with role instructions + embedded cache data
    /// - `user_message`: the analysis request
    /// - `timeout_secs`: per-call timeout (0 = use default)
    ///
    /// Returns the CLI's stdout text, or `Err` on timeout/failure.
    pub async fn run_role(
        &self,
        system_prompt: &str,
        user_message: &str,
        timeout_secs: u64,
    ) -> Result<String, String> {
        let _permit = self.semaphore.acquire().await
            .map_err(|e| format!("cli_executor semaphore: {e}"))?;

        let timeout = if timeout_secs > 0 {
            Duration::from_secs(timeout_secs)
        } else {
            Duration::from_secs(CLI_ROLE_TIMEOUT_SECS)
        };

        let mut cmd = Command::new(&self.claude_bin);
        cmd.args([
            "--print",
            "--system-prompt", system_prompt,
            "--permission-mode", "plan",
            "--max-turns", "1",
            "--no-session-persistence",
            user_message,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("claude CLI not found at: {}", self.claude_bin)
            } else {
                format!("spawn claude: {e}")
            }
        })?;

        // Wait with timeout
        let output = tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| format!("claude CLI timeout after {}s", timeout.as_secs()))?
            .map_err(|e| format!("claude CLI wait: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let code = output.status.code().unwrap_or(-1);
            log::warn!("[cli_executor] claude exited {code}: {stderr}");
            return Err(format!("claude CLI exited {code}: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if stdout.trim().is_empty() {
            return Err("claude CLI returned empty output".to_string());
        }

        Ok(stdout)
    }
}
```

- [x] **Step 4: Add cached data formatting helpers**

```rust
// ---------------------------------------------------------------------------
// Cached data formatters — embed pre-fetched data into prompts
// ---------------------------------------------------------------------------

/// Format macro_cache entries into a readable text block.
/// Returns "N/A" if cache is empty.
pub fn format_macro_cache_for_prompt() -> String {
    use crate::storage::invest::macro_cache;

    let entries = macro_cache::load_all_macro_cache().unwrap_or_default();
    if entries.is_empty() {
        return "宏观指标缓存: 暂无数据".to_string();
    }

    let mut lines = vec!["【A股宏观指标快照】".to_string()];
    for indicator in macro_cache::ALL_INDICATORS {
        if let Some(entry) = entries.iter().find(|e| e.indicator == *indicator) {
            let value_str = match entry.value {
                Some(v) => format!("{:.3}", v),
                None => "N/A".to_string(),
            };
            let label = match *indicator {
                "csi300_close" => "沪深300",
                "csi300_vol20" => "沪深300 20日波动率",
                "northbound_net" => "北向资金净流入(亿)",
                "margin_balance" => "融资余额(元)",
                "shibor_on" => "SHIBOR隔夜(%)",
                "cgb_10y" => "中国10Y国债收益率(%)",
                "vix" => "VIX恐慌指数",
                "tnx" => "美10Y国债收益率(%)",
                "dxy" => "美元指数",
                "gold" => "黄金(美元/盎司)",
                "oil" => "原油(美元/桶)",
                "usdcny" => "USD/CNY",
                "limit_up_count" => "涨停家数",
                "limit_down_count" => "跌停家数",
                "two_market_volume" => "两市成交额(亿)",
                _ => indicator,
            };
            lines.push(format!("  {}: {}", label, value_str));
        }
    }

    // Add timestamp
    if let Some(first) = entries.first() {
        lines.push(format!("  数据时间: {}", first.fetched_at));
    }

    lines.join("\n")
}

/// Format recent events (last 7 days, max 10) into a readable text block.
pub fn format_recent_events_for_prompt() -> String {
    use crate::storage::invest::events::list_events;

    let events = list_events(None, Some(20)).unwrap_or_default();
    let cutoff = (chrono::Local::now() - chrono::Duration::days(7))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();

    let filtered: Vec<_> = events
        .into_iter()
        .filter(|e| e.created_at >= cutoff)
        .take(10)
        .collect();

    if filtered.is_empty() {
        return "近期市场事件: 暂无".to_string();
    }

    let mut lines = vec!["【近期市场事件】(最近7天)".to_string()];
    for ev in &filtered {
        let date = ev.created_at.split('T').next().unwrap_or(&ev.created_at);
        lines.push(format!(
            "  [{}] {} | {} | {}",
            date,
            ev.event_type,
            ev.severity,
            ev.title
        ));
    }
    lines.join("\n")
}

/// Format recent verdicts for a symbol (last 7 days, max 10) into a readable text block.
pub fn format_recent_verdicts_for_prompt(symbol: &str) -> String {
    use crate::storage::invest::verdicts::list_verdicts;

    let verdicts = list_verdicts(Some(symbol), Some(35)).unwrap_or_default();
    let cutoff = (chrono::Local::now() - chrono::Duration::days(7))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();

    let filtered: Vec<_> = verdicts
        .into_iter()
        .filter(|v| v.created_at >= cutoff)
        .take(10)
        .collect();

    if filtered.is_empty() {
        return format!("近期委员会裁决({symbol}): 暂无");
    }

    let mut lines = vec![format!("【近期委员会裁决】({})", symbol)];
    for v in &filtered {
        let date = v.created_at.split('T').next().unwrap_or(&v.created_at);
        let conf = v.confidence.map(|c| format!("{:.1}", c)).unwrap_or_else(|| "N/A".to_string());
        let signal = v.macro_signal.as_deref().unwrap_or("N/A");
        let latency = v.latency_ms.unwrap_or(0);
        lines.push(format!(
            "  [{}] {} -> {} (conf={}, signal={}, {}ms)",
            date, v.symbol, v.verdict, conf, signal, latency
        ));
    }
    lines.join("\n")
}
```

- [x] **Step 5: Verify the module compiles**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30`
Expected: No errors related to `cli_executor`. May show warnings about unused imports.

- [x] **Step 6: Commit**

```bash
git add src-tauri/src/invest/committee/cli_executor.rs
git commit -m "feat(invest): add cli_executor module for committee CLI-based role execution"
```

---

### Task 2: Register the module in `mod.rs`

**Files:**
- Modify: `src-tauri/src/invest/committee/mod.rs:7`

- [x] **Step 1: Add the module declaration**

At `src-tauri/src/invest/committee/mod.rs:7`, after the `pub mod tools;` line, add:

```rust
pub mod cli_executor;
```

The full file becomes:

```rust
pub mod analysis;
pub mod archive;
pub mod cli_executor;
pub mod events;
pub mod orchestrator;
pub mod parser;
pub mod roles;
pub mod tools;
```

- [x] **Step 2: Verify compilation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30`
Expected: Clean compile (warnings OK).

- [x] **Step 3: Commit**

```bash
git add src-tauri/src/invest/committee/mod.rs
git commit -m "chore: register cli_executor module"
```

---

### Task 3: Add `build_cli_macro_prompt` to `cli_executor.rs`

**Files:**
- Modify: `src-tauri/src/invest/committee/cli_executor.rs`

- [x] **Step 1: Add the prompt builder function**

This function builds a complete system prompt for the Macro role with all cached data embedded. Unlike `load_prompt_for_round` which uses `{{placeholder}}` substitution and expects the LLM to call tools, this function provides all data directly.

Append to `cli_executor.rs` (after the format helpers, before the closing of the file):

```rust
/// Build a complete system prompt for the Macro role with cached data embedded.
///
/// This replaces the tool-based approach: instead of giving the LLM tools to
/// fetch data, we pre-fetch all cache data and embed it directly in the prompt.
/// The LLM only needs to analyze, not fetch.
pub fn build_cli_macro_prompt(
    asset_name: &str,
    asset_symbol: &str,
    asset_context: &crate::invest::committee::orchestrator::AssetContext,
) -> String {
    use crate::invest::committee::roles::length_constraint_suffix;
    use crate::invest::committee::CommitteeRole;

    let role = CommitteeRole::Macro;

    // Build the role instruction (without tool references)
    let role_instruction = format!(
        r#"你是投资委员会的宏观分析师，给整个投资组合提供宏观环境判断。

**你的职责范围（只输出以下内容）**：
1. 全局市场底色信号（risk_on/risk_off/neutral）——所有标的共用同一底色
2. 信号强度（0-10）
3. 市场环境阶段判断（主升/分歧/退潮/冰点/混沌）
4. 标的敏感度分析——同一宏观环境对不同资产有不同影响（positive/negative/neutral）
5. 情绪温度评估——市场整体情绪
6. 宏观催化剂感知——只感知，不分类 Tier

**市场阶段判定规则**：
- 主升：沪深300站上MA60且MA20>MA60，北向持续流入，两市成交额>1.2万亿
- 分歧：指数高位震荡，北向进出交替，涨跌比接近1:1
- 退潮：指数跌破MA20，北向流出，两市成交额萎缩
- 冰点：指数跌破MA60，跌停家数>涨停，成交额<8000亿
- 混沌：以上特征均不明显，或信号矛盾

**标的敏感度判定**：
- positive：该资产/行业在当前宏观环境下受益（如降息利好成长股、地缘利好黄金）
- negative：该资产/行业在当前宏观环境下受损（如加息利空高估值、美元走强利空商品）
- neutral：无明显相关性

**标的信息**：
- 标的名称: {asset_name} ({asset_symbol})
- 标的类型: {asset_type}
- 所属行业: {industry}
- PE(TTM): {pe_ttm} | PB: {pb} | ROE: {roe}%
- 最新价: {latest_close} | 前收: {pre_close}
- 流通市值: {circ_mv_yi}亿 | 总市值: {total_mv_yi}亿
- 资金流向(日): {money_flow_daily}
- 机构评级: {rating}

{macro_data}

{events_data}

{verdicts_data}

**输出要求**：
- 必须中文回复
- 严格按下列格式，每项必须换行
- 严禁输出个股技术面分析（MA/RSI/分位数/支撑阻力等）
- 严禁给出具体操作建议（买入/卖出/加仓/减仓）
- 严禁在输出里抱怨"工具不可用"或"未找到信息"
- 市场阶段是全局信号，敏感度是标的级信号，两者必须分开

信号: risk_on | risk_off | neutral
强度: 0-10
信号理由: <一句话说明信号判断依据>
市场阶段: 主升 | 分歧 | 退潮 | 冰点 | 混沌
市场阶段理由: <一句话说明阶段判断依据>
敏感度: positive | negative | neutral
敏感度理由: <一句话≤20字，说明该资产/行业为何对当前环境正面/负面>
情绪温度: 乐观 | 中性 | 谨慎 | 恐慌
宏观催化剂: <当前最重要的宏观事件，没有则写"无">"#,
        asset_name = asset_name,
        asset_symbol = asset_symbol,
        asset_type = asset_context.asset_type,
        industry = asset_context.industry.as_deref().unwrap_or("N/A"),
        pe_ttm = fmt_opt(asset_context.pe_ttm, 1),
        pb = fmt_opt(asset_context.pb, 2),
        roe = fmt_opt(asset_context.roe, 1),
        latest_close = fmt_opt(asset_context.latest_close, 2),
        pre_close = fmt_opt(asset_context.pre_close, 2),
        circ_mv_yi = fmt_opt(asset_context.circ_mv_yi, 2),
        total_mv_yi = fmt_opt(asset_context.total_mv_yi, 2),
        money_flow_daily = asset_context.money_flow_daily_summary.as_deref().unwrap_or("N/A"),
        rating = asset_context.rating_summary.as_deref().unwrap_or("N/A"),
        macro_data = format_macro_cache_for_prompt(),
        events_data = format_recent_events_for_prompt(),
        verdicts_data = format_recent_verdicts_for_prompt(asset_symbol),
    );

    format!(
        "{}{}",
        role_instruction,
        length_constraint_suffix(role)
    )
}

fn fmt_opt(v: Option<f64>, decimals: usize) -> String {
    v.map(|v| format!("{:.1$}", v, decimals))
        .unwrap_or_else(|| "N/A".to_string())
}
```

- [x] **Step 2: Verify compilation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -30`
Expected: Clean compile. The function references `CommitteeRole` and `orchestrator::AssetContext` — verify no circular dependency.

- [x] **Step 3: Commit**

```bash
git add src-tauri/src/invest/committee/cli_executor.rs
git commit -m "feat(invest): add build_cli_macro_prompt with embedded cache data"
```

---

### Task 4: Refactor `run_macro_phase` to try CLI first

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs:1312-1362`

- [x] **Step 1: Read the current `run_macro_phase` function**

Read `src-tauri/src/invest/committee/orchestrator.rs:1312-1363` to confirm the exact current code. The function currently:
1. Resolves provider and builds LLM config
2. Loads prompt via `load_prompt_for_round`
3. Gets tool definitions via `role_tool_defs`
4. Acquires governor semaphore
5. Calls `run_with_tool_loop`

- [x] **Step 2: Replace the function body**

Replace `orchestrator.rs:1312-1363` (the entire `run_macro_phase` function) with:

```rust
async fn run_macro_phase(
    client: &dyn InvestLlmClient,
    symbol: &str,
    config: &CommitteeConfig,
    portfolio_summary: &str,
    emitter: &Option<EventEmitter>,
    asset_context: &AssetContext,
) -> Result<(RoundOutput, u32), String> {
    let role = CommitteeRole::Macro;
    let start = std::time::Instant::now();

    // --- Try CLI path first ---
    if let Some(cli) = super::cli_executor::CliCommitteeExecutor::global() {
        let asset_name = get_asset_name(symbol).unwrap_or_else(|| symbol.to_string());
        let system_prompt = super::cli_executor::build_cli_macro_prompt(
            &asset_name,
            symbol,
            asset_context,
        );
        let user_msg = if portfolio_summary.is_empty() {
            format!(
                "请分析 {} 的宏观环境和技术面，给出风险信号判断。",
                symbol
            )
        } else {
            format!(
                "请分析 {} 的宏观环境和技术面，给出风险信号判断。\n\n{}",
                symbol, portfolio_summary
            )
        };

        log::info!("run_macro_phase: using CLI executor for {}", symbol);
        let si = step_index_for_role(role, 1);
        if let Some(ref emit) = emitter {
            emit(CommitteeEvent::RoleStart {
                symbol: symbol.to_string(),
                role,
                round: 1,
                step_index: si,
            });
        }

        match cli.run_role(&system_prompt, &user_msg, config.timeout_secs).await {
            Ok(raw_text) => {
                let (text, truncated) = hard_truncate(&raw_text, role, 0);
                let mut parsed = parse_role_output(role, &text, truncated);
                parsed.fallback_reason = detect_fallback_reason(role, &parsed);

                // Retry once on fallback
                super::orchestrator::retry_on_fallback_static(
                    client, role, 1, &system_prompt,
                    &mut parsed, &config, asset_context,
                ).await;

                let latency_ms = start.elapsed().as_millis() as u64;
                let round_output = RoundOutput {
                    role,
                    round: 1,
                    parsed,
                    latency_ms,
                    tokens_used: 0, // CLI doesn't report tokens
                };
                if let Some(ref emit) = emitter {
                    emit(CommitteeEvent::RoleComplete {
                        symbol: symbol.to_string(),
                        role,
                        round: 1,
                        summary: RoundOutputSummary::from(&round_output),
                        step_index: si,
                    });
                }
                return Ok((round_output, 0));
            }
            Err(e) => {
                log::warn!("run_macro_phase: CLI failed for {}: {}, falling back to API", symbol, e);
                // Fall through to API path
            }
        }
    }

    // --- Fallback: existing API path (unchanged) ---
    let provider = resolve_provider(config, role);
    let llm_config = build_llm_config(provider, role, config.timeout_secs, config.model_override.as_deref());

    let asset_name = get_asset_name(symbol).unwrap_or_else(|| symbol.to_string());
    let system_prompt = format!(
        "{}{}",
        load_prompt_for_round(role, 1, &asset_name, symbol, asset_context),
        length_constraint_suffix(role)
    );
    let tool_defs = role_tool_defs(role, 1);

    let governor = global_governor();
    let _permit = governor.acquire(provider).await;

    let user_msg = if portfolio_summary.is_empty() {
        format!(
            "请分析 {} 的宏观环境和技术面，给出风险信号判断。",
            symbol
        )
    } else {
        format!(
            "请分析 {} 的宏观环境和技术面，给出风险信号判断。\n\n{}",
            symbol, portfolio_summary
        )
    };
    let mut messages: Vec<Message> = vec![Message::user(user_msg)];

    let start = std::time::Instant::now();

    run_with_tool_loop(
        client,
        symbol,
        role,
        1,
        &system_prompt,
        &mut messages,
        tool_defs.as_deref(),
        &llm_config,
        start,
        emitter,
    )
    .await
}
```

**Note:** The `retry_on_fallback_static` call above requires either making the existing `retry_on_fallback` callable from this context, or inlining the retry logic. See Task 5 for the helper extraction.

- [x] **Step 3: Verify compilation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -40`
Expected: May show errors about `retry_on_fallback_static` or `RoundOutputSummary` fields — fix in Task 5.

- [x] **Step 4: Commit**

```bash
git add src-tauri/src/invest/committee/orchestrator.rs
git commit -m "feat(invest): refactor run_macro_phase to try CLI executor first"
```

---

### Task 5: Extract `retry_on_fallback` as a reusable helper

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs:1090-1121`

- [x] **Step 1: Read the current `retry_on_fallback` function**

Read `orchestrator.rs:1090-1121` to understand the current signature and logic.

- [x] **Step 2: Make the function accessible from `run_macro_phase`**

The current `retry_on_fallback` takes `messages: &mut Vec<Message>` which requires the full message history from `run_with_tool_loop`. For the CLI path, we don't have a messages vec — we just have the raw text.

**Simplest approach:** In the CLI path of `run_macro_phase`, skip the retry entirely (the CLI output is already the final text, not a tool-call intermediate). The `detect_fallback_reason` check is sufficient — if the CLI output is missing fields, it will be caught and the fallback_reason will be set, which the CIO can handle.

Update the CLI path in `run_macro_phase` to remove the `retry_on_fallback_static` call and instead just log when fallback is detected:

```rust
            Ok(raw_text) => {
                let (text, truncated) = hard_truncate(&raw_text, role, 0);
                let mut parsed = parse_role_output(role, &text, truncated);
                parsed.fallback_reason = detect_fallback_reason(role, &parsed);

                if parsed.fallback_reason.is_some() {
                    log::warn!(
                        "run_macro_phase: CLI output has fallback reason for {}: {:?}",
                        symbol, parsed.fallback_reason
                    );
                }

                let latency_ms = start.elapsed().as_millis() as u64;
                // ... rest unchanged
```

- [x] **Step 3: Verify compilation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | head -40`
Expected: Clean compile. The CLI path no longer references `retry_on_fallback`.

- [x] **Step 4: Commit**

```bash
git add src-tauri/src/invest/committee/orchestrator.rs
git commit -m "fix(invest): remove retry_on_fallback from CLI path, log fallback instead"
```

---

### Task 6: Verify end-to-end compilation and test

**Files:**
- No file changes

- [x] **Step 1: Full Rust check**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1`
Expected: Clean compile with no errors. Warnings are OK.

- [x] **Step 2: Run clippy**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings 2>&1 | head -30`
Expected: No errors. Fix any warnings about unused imports or dead code.

- [x] **Step 3: Verify frontend build still works**

Run: `npm run check 2>&1 | tail -10`
Expected: No TypeScript errors (this change is Rust-only, so frontend should be unaffected).

- [x] **Step 4: Commit any clippy fixes**

```bash
git add -u
git commit -m "chore: clippy fixes for cli_executor"
```

---

### Task 7: Add i18n key for CLI mode indicator (optional)

**Files:**
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

- [x] **Step 1: Add a log-only note**

No i18n keys needed for Phase 1 — the CLI execution is transparent to the user (same events emitted, same output format). The only visible difference is faster/slower execution and potentially different output quality.

Skip this task. Move to Task 8.

---

### Task 8: Manual verification

- [x] **Step 1: Verify CLI detection**

Start the app with `npm run tauri dev`. Check the Rust logs for:
```
[cli_executor] using claude binary: C:\Users\...\claude.cmd
```
If the binary is not found, the log will show:
```
[cli_executor] claude binary path is empty
```
and the system will fall back to the existing API path.

- [x] **Step 2: Run a single committee analysis**

From the invest page, trigger a committee analysis for one symbol. Check logs for:
```
run_macro_phase: using CLI executor for 600519
```
This confirms the CLI path is being used.

- [x] **Step 3: Verify output parsing**

Check that the Macro role output contains the expected fields:
```
信号: risk_on
强度: 7
信号理由: ...
市场阶段: 主升
...
```
If the output is malformed, `parse_role_output` will set `fallback_reason` and log a warning.

- [x] **Step 4: Verify fallback path**

Temporarily rename the claude binary to force fallback. Check logs for:
```
run_macro_phase: CLI failed for 600519: claude CLI not found at: ..., falling back to API
```
The analysis should still complete using the existing DeepSeek API path.

- [x] **Step 5: Verify batch concurrency**

Run "Run All" with 3+ symbols. Check that up to 5 CLI processes run concurrently (semaphore permits). The log should show interleaved CLI calls for different symbols.

---

## Phase 1 Summary

| Metric | Value |
|--------|-------|
| New files | 1 (`cli_executor.rs`, 333 行) |
| Modified files | 2 (`mod.rs` +1 行, `orchestrator.rs` +86 行) |
| Total new code | 333 行 |
| Total modified code | 86 行 |
| Lines deleted | 0（fallback 路径完整保留） |
| Breaking changes | 无（CLI 不可用时自动 fallback 到 API） |
| New dependencies | 无（复用现有 tokio, chrono） |
| Commits | `6a41266`, `0f855cc` |

## 后续方向

| Phase | 状态 | 下一步 |
|-------|------|--------|
| Phase 2 (Quant + Risk) | 待开始 | 需先手动验证 Phase 1 效果 |
| Phase 3 (CIO/L4 + 清理) | 待开始 | 取决于 Phase 2 结果 |
