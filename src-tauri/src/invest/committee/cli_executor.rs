//! CLI-based committee role executor.
//!
//! Spawns `claude --print` with pre-fetched cache data embedded in the
//! system prompt. Returns the CLI's stdout as the role output text.
//! Falls back gracefully if the CLI binary is not found.
//!
//! **Note:** The global singleton uses `Mutex` — if the claude binary is
//! not found at first call, it retries on subsequent calls. This allows
//! late installation without requiring an app restart.

use std::sync::Mutex;
use std::time::Duration;
use tokio::process::Command;

use crate::process_ext::HideConsole;

/// Maximum concurrent CLI processes (limits memory usage: ~100MB each).
const MAX_CLI_CONCURRENT: usize = 5;

/// Default timeout for a single CLI role call (seconds).
const CLI_ROLE_TIMEOUT_SECS: u64 = 180;

/// Global CLI executor singleton.
/// Uses `Mutex` instead of `OnceLock` to allow re-initialization if the
/// claude binary was not found on first attempt (e.g., installed later).
static CLI_EXECUTOR: Mutex<Option<CliCommitteeExecutor>> = Mutex::new(None);

/// Manages spawning `claude --print` for committee role analysis.
#[derive(Clone)]
pub struct CliCommitteeExecutor {
    /// Resolved absolute path to the `claude` binary.
    claude_bin: String,
    /// Semaphore limiting concurrent CLI processes.
    semaphore: std::sync::Arc<tokio::sync::Semaphore>,
}

impl CliCommitteeExecutor {
    /// Try to create an executor. Returns `None` if `claude` binary is not found.
    pub fn try_new() -> Option<Self> {
        let bin = crate::agent::claude_stream::resolve_claude_path();
        if bin.is_empty() {
            log::warn!("[cli_executor] claude binary path is empty");
            return None;
        }
        log::info!("[cli_executor] using claude binary: {}", bin);
        Some(Self {
            claude_bin: bin,
            semaphore: std::sync::Arc::new(tokio::sync::Semaphore::new(MAX_CLI_CONCURRENT)),
        })
    }

    /// Get the global singleton, initializing if needed.
    ///
    /// Unlike `OnceLock`, this retries initialization if the claude binary
    /// was not found on a previous attempt — allowing late installation.
    pub fn global() -> Option<CliCommitteeExecutor> {
        let mut guard = CLI_EXECUTOR.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Self::try_new();
        }
        guard.clone()
    }

    /// Execute a single committee role via `claude --print`.
    ///
    /// - `system_prompt`: full prompt with role instructions + embedded cache data
    /// - `user_message`: the analysis request
    /// - `timeout_secs`: per-call timeout (0 = use default)
    /// - `settings_path`: optional path to a `--settings` JSON for third-party provider routing
    ///
    /// Returns the CLI's stdout text, or `Err` on timeout/failure.
    pub async fn run_role(
        &self,
        system_prompt: &str,
        user_message: &str,
        timeout_secs: u64,
        settings_path: Option<&std::path::Path>,
    ) -> Result<String, String> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| format!("cli_executor semaphore: {e}"))?;

        let timeout = if timeout_secs > 0 {
            Duration::from_secs(timeout_secs)
        } else {
            Duration::from_secs(CLI_ROLE_TIMEOUT_SECS)
        };

        let mut cmd = Command::new(&self.claude_bin);
        cmd.args([
            "--print",
            "--system-prompt",
            system_prompt,
            "--permission-mode",
            "plan",
            "--max-turns",
            "1",
            "--no-session-persistence",
        ]);

        // Inject --settings for third-party provider routing (P0 fix)
        if let Some(sp) = settings_path {
            cmd.args(["--settings", &sp.to_string_lossy()]);
            log::debug!("[cli_executor] --settings {}", sp.display());
        }

        cmd.arg(user_message);
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .hide_console();

        let child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("claude CLI not found at: {}", self.claude_bin)
            } else {
                format!("spawn claude: {e}")
            }
        })?;

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

// ---------------------------------------------------------------------------
// Cached data formatters — embed pre-fetched data into prompts
// ---------------------------------------------------------------------------

/// Format macro_cache entries into a readable text block.
/// Delegates to the shared `tools::format_macro_entries` for consistency.
pub fn format_macro_cache_for_prompt() -> String {
    use crate::storage::invest::macro_cache;
    use super::tools::format_macro_entries;

    let entries = macro_cache::load_all_macro_cache().unwrap_or_default();
    format_macro_entries(&entries).unwrap_or_else(|_| "宏观指标缓存: 暂无数据".to_string())
}

/// Format recent events (last 7 days, max 10) into a readable text block.
/// Uses date-only cutoff ("%Y-%m-%d") consistent with tools.rs.
pub fn format_recent_events_for_prompt() -> String {
    use crate::storage::invest::events::list_events;

    let events = list_events(None, Some(20)).unwrap_or_default();
    let cutoff = (chrono::Local::now() - chrono::Duration::days(7))
        .format("%Y-%m-%d")
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
        // Use first 10 chars as date portion (works for both "T" and " " separators)
        let date = &ev.created_at[..10.min(ev.created_at.len())];
        lines.push(format!(
            "  [{}] {} | {} | {}",
            date, ev.event_type, ev.severity, ev.title
        ));
    }
    lines.join("\n")
}

/// Format recent verdicts for a symbol (last 7 days, max 10).
/// Uses date-only cutoff ("%Y-%m-%d") consistent with tools.rs.
pub fn format_recent_verdicts_for_prompt(symbol: &str) -> String {
    use crate::storage::invest::verdicts::list_verdicts;

    let verdicts = list_verdicts(Some(symbol), Some(35)).unwrap_or_default();
    let cutoff = (chrono::Local::now() - chrono::Duration::days(7))
        .format("%Y-%m-%d")
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
        // Use first 10 chars as date portion (works for both "T" and " " separators)
        let date = &v.created_at[..10.min(v.created_at.len())];
        let conf = v
            .confidence
            .map(|c| format!("{:.1}", c))
            .unwrap_or_else(|| "N/A".to_string());
        let signal = v.macro_signal.as_deref().unwrap_or("N/A");
        let latency = v.latency_ms.unwrap_or(0);
        lines.push(format!(
            "  [{}] {} -> {} (conf={}, signal={}, {}ms)",
            date, v.symbol, v.verdict, conf, signal, latency
        ));
    }
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Prompt builder for CLI-based Macro role
// ---------------------------------------------------------------------------

/// Build a complete system prompt for the Macro role with cached data embedded.
///
/// This reuses the canonical MACRO_PROMPT from `roles.rs` (via `load_prompt_for_round`)
/// to avoid prompt drift, then:
/// 1. Strips the tool-call section (CLI doesn't use tools)
/// 2. Replaces the `{{recent_events}}` placeholder with cached events data
/// 3. Appends macro cache data, verdicts, and CLI-specific output rules
///
/// The LLM receives all data directly and only needs to analyze, not fetch.
pub fn build_cli_macro_prompt(
    asset_name: &str,
    asset_symbol: &str,
    asset_context: &crate::invest::committee::orchestrator::AssetContext,
    verdicts_data: &str,
) -> String {
    use crate::invest::committee::roles::{length_constraint_suffix, load_prompt_for_round, CommitteeRole};

    let role = CommitteeRole::Macro;

    // Start with the canonical prompt (includes {{placeholder}} substitution)
    let base_prompt = load_prompt_for_round(role, 1, asset_name, asset_symbol, asset_context);

    // Strip the tool-call section: remove everything between "**你有工具可调用**" and the next "**" section
    let stripped = strip_tool_section(&base_prompt);

    // Replace the {{recent_events}} placeholder with actual cached data
    let events_data = format_recent_events_for_prompt();
    let with_events = stripped.replace("{{recent_events}}", &events_data);

    // Build the CLI-specific additions (cache data that tools would have fetched)
    let macro_data = format_macro_cache_for_prompt();

    // Append cache data and CLI-specific instructions
    let cli_additions = format!(
        "\n\n{macro_data}\n\n{verdicts_data}\n\n\
         **CLI 模式说明**：以上宏观指标和裁决数据已由系统预取，无需调用工具。\n\
         市场阶段判定中，MA60/MA20 等均线数据无法直接获取——请根据上证指数点位、\n\
         成交额、北向资金等已有数据综合推断，不要编造具体均线数值。",
        macro_data = macro_data,
        verdicts_data = verdicts_data,
    );

    format!("{}{}{}", with_events, cli_additions, length_constraint_suffix(role))
}

/// Strip the tool-call section from a prompt template.
/// Removes everything from "**你有工具可调用**" to the next "**" section header.
fn strip_tool_section(prompt: &str) -> String {
    const MARKER: &str = "**你有工具可调用**";
    if let Some(start) = prompt.find(MARKER) {
        let marker_end = start + MARKER.len();
        // Search for the next "\n**" AFTER the marker (avoids matching the marker itself)
        if let Some(rest_offset) = prompt[marker_end..].find("\n**") {
            let end = marker_end + rest_offset;
            let mut result = String::with_capacity(prompt.len());
            result.push_str(&prompt[..start]);
            // Skip blank lines after tool section
            let rest = prompt[end..].trim_start();
            result.push_str(rest);
            return result;
        }
        // No section after tools — just strip to end
        return prompt[..start].to_string();
    }
    prompt.to_string()
}

/// Format an `Option<f64>` with given decimals, returning "N/A" for None.
pub fn fmt_opt(v: Option<f64>, decimals: usize) -> String {
    v.map(|v| format!("{:.1$}", v, decimals))
        .unwrap_or_else(|| "N/A".to_string())
}

// ---------------------------------------------------------------------------
// Additional data formatters for CLI prompt injection
// ---------------------------------------------------------------------------

/// Fetch and format company news for a symbol (AkShare source).
/// This is async because it fetches from AkShare RPC.
pub async fn fetch_company_news_for_prompt(symbol: &str) -> String {
    let code = symbol.split('.').next().unwrap_or(symbol);
    let client = crate::invest::international::InternationalClient::from_settings();

    match client.fetch_akshare_stock_news(code, 5).await {
        Ok(items) if !items.is_empty() => {
            let mut lines = vec![format!("【{} 近期风险新闻】", symbol)];
            for item in items.iter().take(5) {
                let date = chrono::DateTime::from_timestamp(item.provider_publish_time, 0)
                    .map(|dt| dt.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| "N/A".to_string());
                lines.push(format!("  [{}] {} — {}", date, item.title, item.publisher));
            }
            lines.join("\n")
        }
        _ => format!("{} 近期风险新闻: 暂无", symbol),
    }
}

/// Format risk metrics context for Risk role CLI prompt injection.
/// Delegates to the canonical `build_risk_metrics_context` in orchestrator.
/// `mode` 决定成本来源:Holding 用持仓买入均价,Research 用 watch 关注价。
pub(crate) fn format_risk_metrics_for_prompt(
    portfolio_data: &crate::invest::committee::orchestrator::PortfolioData,
    symbol: &str,
    asset_context: &crate::invest::committee::orchestrator::AssetContext,
    mode: crate::invest::committee::orchestrator::Mode,
) -> String {
    crate::invest::committee::orchestrator::build_risk_metrics_context(portfolio_data, symbol, asset_context, mode)
}

/// Format prior round outputs as a text block for CLI prompt injection.
pub fn format_round_outputs_for_prompt(
    round_outputs: &[crate::invest::committee::analysis::RoundOutput],
) -> String {
    if round_outputs.is_empty() {
        return String::new();
    }

    let mut out = String::from("【前序分析结果】\n");
    for output in round_outputs {
        // Use raw_text directly — do NOT inject "[WORKER_UNAVAILABLE]" marker
        // into the prompt, as the LLM may echo it back and trigger false
        // worker_unavailable detection on the current role's own output.
        // The post-merge safety check in analysis.rs still enforces HOLD
        // when any round output's raw_text contains the marker.
        let content = &output.parsed.raw_text;
        out.push_str(&format!(
            "\n=== {} Round {} ===\n{}\n",
            output.role.label(),
            output.round,
            content,
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// Hit-rate soft prompt injection (verdict review feedback loop)
// ---------------------------------------------------------------------------

/// 把命中率聚合渲染成软提示文本块。空聚合返回 ""。
/// current_regime 用于高亮"当前市场状态"下的同类命中率。
fn render_hit_rates(
    agg: &crate::storage::invest::verdict_reviews::HitRateAgg,
    current_regime: &str,
) -> String {
    use crate::storage::invest::verdict_reviews::HitRateRow;
    if agg.global.is_empty() && agg.by_regime.is_empty() {
        return String::new();
    }

    // 把同一 verdict_type 的多个 window 合并成一行展示,未到期窗口跳过。
    fn fmt_rows(rows: &[HitRateRow]) -> Vec<String> {
        use std::collections::BTreeMap;
        let mut by_type: BTreeMap<&str, Vec<&HitRateRow>> = BTreeMap::new();
        for r in rows {
            by_type.entry(r.verdict_type.as_str()).or_default().push(r);
        }
        let mut lines = Vec::new();
        for (vt, mut rs) in by_type {
            rs.sort_by_key(|r| r.window_days);
            let parts: Vec<String> = rs
                .iter()
                .filter(|r| r.matured)
                .map(|r| {
                    let pct = if r.total > 0 {
                        (r.hits as f64 / r.total as f64 * 100.0).round() as i64
                    } else {
                        0
                    };
                    format!("{}天 {}%(n={})", r.window_days, pct, r.total)
                })
                .collect();
            if !parts.is_empty() {
                lines.push(format!("  {}: {}", vt, parts.join(" / ")));
            }
        }
        lines
    }

    let mut out = vec![
        "[历史命中率参考 — 你过往同类判断的真实表现]".to_string(),
        "全局:".to_string(),
    ];
    out.extend(fmt_rows(&agg.global));

    // 当前 regime 段
    if let Some((_, rows)) = agg.by_regime.iter().find(|(r, _)| r == current_regime) {
        let regime_lines = fmt_rows(rows);
        if !regime_lines.is_empty() {
            out.push(format!("当前市场状态({}):", current_regime));
            out.extend(regime_lines);
        }
    }

    out.push(
        "说明:这是你过往同类判断的真实表现,供你校准信心,但不要机械套用——市场环境会变。"
            .to_string(),
    );
    // 若存在 30d 未到期组,补一句说明
    let has_unmatured_30d = agg.global.iter().any(|r| r.window_days >= 30 && !r.matured);
    if has_unmatured_30d {
        out.push("30天窗口样本尚未到期,暂不列出。".to_string());
    }

    out.join("\n")
}

/// 读库 + 渲染。供 orchestrator 预取 + build_cli_*_prompt 调用。失败或空时返回 ""。
pub(crate) fn format_hit_rates_for_prompt(current_regime: &str) -> String {
    match crate::storage::invest::verdict_reviews::aggregate_hit_rates(5) {
        Ok(agg) => render_hit_rates(&agg, current_regime),
        Err(e) => {
            log::warn!("aggregate_hit_rates failed: {}", e);
            String::new()
        }
    }
}

// ---------------------------------------------------------------------------
// CLI prompt builders for all roles
// ---------------------------------------------------------------------------

/// Build a complete system prompt for the Quant role (R1) with data embedded.
///
/// `load_prompt_for_round` already substitutes `{{precomputed_indicators}}`, `{{pe_ttm}}`,
/// `{{pb}}`, `{{money_flow_*}}`, and other asset context placeholders, so we do NOT
/// re-append `precomputed_indicators` here to avoid duplication.
pub fn build_cli_quant_r1_prompt(
    asset_name: &str,
    asset_symbol: &str,
    asset_context: &crate::invest::committee::orchestrator::AssetContext,
    regime_context: Option<&str>,
    verdicts_data: &str,
) -> String {
    use crate::invest::committee::roles::{length_constraint_suffix, load_prompt_for_round, CommitteeRole};

    let role = CommitteeRole::Quant;
    let base_prompt = load_prompt_for_round(role, 1, asset_name, asset_symbol, asset_context);
    let stripped = strip_tool_section(&base_prompt);

    let mut cli_additions = format!(
        "\n\n{verdicts_data}\n\n\
         **CLI 模式说明**：以上数据已由系统预取，无需调用工具。\
         标的估值、资金流向和技术指标已在上方 prompt 中直接提供。",
        verdicts_data = verdicts_data,
    );

    // Inject regime context if available
    if let Some(regime) = regime_context {
        cli_additions.push_str(&format!("\n\n{}", regime));
    }

    format!("{}{}{}", stripped, cli_additions, length_constraint_suffix(role))
}

/// Build a complete system prompt for the Quant R2 role with data embedded.
pub fn build_cli_quant_r2_prompt(
    asset_name: &str,
    asset_symbol: &str,
    asset_context: &crate::invest::committee::orchestrator::AssetContext,
    round_outputs: &[crate::invest::committee::analysis::RoundOutput],
    hit_rates: &str,
) -> String {
    use crate::invest::committee::roles::{length_constraint_suffix, load_prompt_for_round, CommitteeRole};

    let role = CommitteeRole::Quant;
    let base_prompt = load_prompt_for_round(role, 2, asset_name, asset_symbol, asset_context);
    let stripped = strip_tool_section(&base_prompt);

    let prior_outputs = format_round_outputs_for_prompt(round_outputs);

    let cli_additions = format!(
        "\n\n{}\n\n\
         **CLI 模式说明**：以上前序分析结果已由系统预取，无需调用工具。",
        prior_outputs,
    );

    // 历史命中率注入(软提示),由 orchestrator 预取(regime 取 Macro 信号,对齐 archive 口径)。
    let cli_additions = if hit_rates.is_empty() {
        cli_additions
    } else {
        format!("{}\n\n{}", cli_additions, hit_rates)
    };

    format!("{}{}{}", stripped, cli_additions, length_constraint_suffix(role))
}

/// Build a complete system prompt for the Risk R1 role with data embedded.
/// `company_news` should be pre-fetched via `fetch_company_news_for_prompt()`.
pub(crate) fn build_cli_risk_r1_prompt(
    asset_name: &str,
    asset_symbol: &str,
    asset_context: &crate::invest::committee::orchestrator::AssetContext,
    portfolio_data: &crate::invest::committee::orchestrator::PortfolioData,
    strategy_context: &str,
    user_profile_context: &str,
    company_news: &str,
    verdicts: &str,
    mode: crate::invest::committee::orchestrator::Mode,
) -> String {
    use crate::invest::committee::roles::{length_constraint_suffix, load_prompt_for_round, CommitteeRole};

    let role = CommitteeRole::Risk;
    let base_prompt = load_prompt_for_round(role, 1, asset_name, asset_symbol, asset_context);
    let stripped = strip_tool_section(&base_prompt);

    let risk_metrics = format_risk_metrics_for_prompt(portfolio_data, asset_symbol, asset_context, mode);

    let mut cli_additions = format!(
        "\n\n{risk_metrics}\n\n{company_news}\n\n{verdicts}\n\n\
         **CLI 模式说明**：以上数据已由系统预取，无需调用工具。",
        risk_metrics = risk_metrics,
        company_news = company_news,
        verdicts = verdicts,
    );

    if !strategy_context.is_empty() {
        cli_additions.push_str("\n\n");
        cli_additions.push_str(strategy_context);
    }
    if !user_profile_context.is_empty() {
        cli_additions.push_str("\n\n");
        cli_additions.push_str(user_profile_context);
    }

    // Research mode 风控职责调整:跳过现金/集中度,只评标的自身风险
    if mode == crate::invest::committee::orchestrator::Mode::Research {
        cli_additions.push_str(
            "\n\n【研究模式 — 风控职责调整】\n\
             本标的为研究观察,非实际持仓。请:\n\
             - 忽略现金/子弹充足度,不因无现金而提高风险信号\n\
             - 忽略组合集中度(标的不在组合内)\n\
             - 成本对比基于关注价(非真实买入均价),浮盈浮亏表示\u{201C}关注以来涨跌\u{201D}\n\
             - 只评估标的自身风险(估值/财务/流动性/利空)",
        );
    }

    format!("{}{}{}", stripped, cli_additions, length_constraint_suffix(role))
}

/// Build a complete system prompt for the Risk R2 role with data embedded.
pub fn build_cli_risk_r2_prompt(
    asset_name: &str,
    asset_symbol: &str,
    asset_context: &crate::invest::committee::orchestrator::AssetContext,
    round_outputs: &[crate::invest::committee::analysis::RoundOutput],
) -> String {
    use crate::invest::committee::roles::{length_constraint_suffix, load_prompt_for_round, CommitteeRole};

    let role = CommitteeRole::Risk;
    let base_prompt = load_prompt_for_round(role, 2, asset_name, asset_symbol, asset_context);
    let stripped = strip_tool_section(&base_prompt);

    let prior_outputs = format_round_outputs_for_prompt(round_outputs);

    let cli_additions = format!(
        "\n\n{}\n\n\
         **CLI 模式说明**：以上前序分析结果已由系统预取，无需调用工具。",
        prior_outputs,
    );

    format!("{}{}{}", stripped, cli_additions, length_constraint_suffix(role))
}

/// Build a complete system prompt for the CIO role with data embedded.
///
/// `load_prompt_for_round` already substitutes asset context placeholders, so we do NOT
/// re-append a separate asset summary here to avoid duplication.
pub fn build_cli_cio_prompt(
    asset_name: &str,
    asset_symbol: &str,
    asset_context: &crate::invest::committee::orchestrator::AssetContext,
    round_outputs: &[crate::invest::committee::analysis::RoundOutput],
    strategy_context: &str,
    user_profile_context: &str,
    portfolio_summary: &str,
    hit_rates: &str,
    mode: crate::invest::committee::orchestrator::Mode,
) -> String {
    use crate::invest::committee::roles::{length_constraint_suffix, load_prompt_for_round, CommitteeRole};

    let role = CommitteeRole::Cio;
    let base_prompt = load_prompt_for_round(role, 1, asset_name, asset_symbol, asset_context);
    let stripped = strip_tool_section(&base_prompt);

    let prior_outputs = format_round_outputs_for_prompt(round_outputs);

    let mut cli_additions = format!(
        "\n\n{}\n\n\
         **CLI 模式说明**：以上数据已由系统预取，无需调用工具。\
         标的估值和资金流向已在上方 prompt 中直接提供。\n\
         ⚠️ **禁止 tool_call**：所有必要信息都在上方。不要尝试调用任何工具。",
        prior_outputs,
    );

    // Inject portfolio summary (holdings, cash, total_assets) so the CIO can
    // compute accurate ratios like 子弹占比 instead of hallucinating them.
    if !portfolio_summary.is_empty() {
        cli_additions.push_str("\n\n");
        cli_additions.push_str(portfolio_summary);
    }

    if !strategy_context.is_empty() {
        cli_additions.push_str("\n\n");
        cli_additions.push_str(strategy_context);
    }
    if !user_profile_context.is_empty() {
        cli_additions.push_str("\n\n");
        cli_additions.push_str(user_profile_context);
    }

    // Inject data quality warnings
    if !asset_context.data_quality.is_empty() {
        cli_additions.push_str(&format!(
            "\n\n【数据质量警告】以下字段缺失，请在置信度评估中考虑：\n{}",
            asset_context.data_quality.join("，")
        ));
    }

    // 历史命中率注入(软提示),由 orchestrator 预取(regime 取 Macro 信号)。
    if !hit_rates.is_empty() {
        cli_additions.push_str("\n\n");
        cli_additions.push_str(hit_rates);
    }

    // Research mode 裁决语义重定义:BUY/HOLD/SELL = 标的吸引力,而非持仓动作
    if mode == crate::invest::committee::orchestrator::Mode::Research {
        cli_additions.push_str(
            "\n\n【研究模式 — 裁决语义重定义】\n\
             本标的为研究观察,非持仓评估。裁决语义改为\u{201C}标的吸引力\u{201D}:\n\
             - BUY/ACCUMULATE = 值得买入 / 可分批建仓\n\
             - HOLD = 观望\n\
             - TRIM/SELL = 规避 / 看空\n\
             忽略现金充足度与组合集中度,基于标的自身基本面/技术面/催化剂判断吸引力。",
        );
    }

    format!("{}{}{}", stripped, cli_additions, length_constraint_suffix(role))
}

// ---------------------------------------------------------------------------
// Provider settings JSON generation for CLI --settings
// ---------------------------------------------------------------------------

/// Generate a temp settings JSON for committee CLI execution.
///
/// Reads the selected provider's credential from `UserSettings.platform_credentials`,
/// builds the provider-specific env vars, and writes a `--settings` compatible JSON file.
/// Returns the path to the temp file, or `None` if no provider is configured (uses CC defaults).
///
/// The temp file is written to `{data_dir}/provider-claude-configs/committee-{uuid}.json`
/// and is meant to be passed as `--settings <path>` to `claude --print`.
pub fn write_committee_settings_json(
    platform_id: &str,
    model_override: Option<&str>,
) -> Result<Option<std::path::PathBuf>, String> {
    use crate::agent::provider_claude_config::{
        platform_to_provider_id, write_provider_claude_config,
    };
    use crate::agent::provider_claude_config::ManagedConfig;
    use crate::storage::settings::get_user_settings;

    // "default" means use CC native config — no --settings needed
    if platform_id == "default" || platform_id.is_empty() {
        return Ok(None);
    }

    // Clean up old committee temp files to avoid accumulation
    cleanup_old_committee_settings();

    let user_settings = get_user_settings();
    let cred = user_settings
        .platform_credentials
        .iter()
        .find(|c| c.platform_id == platform_id)
        .ok_or_else(|| {
            format!(
                "committee: platform credential '{}' not found in settings",
                platform_id
            )
        })?;

    let provider_id = platform_to_provider_id(platform_id)
        .unwrap_or("custom")
        .to_string();

    // Use a unique run_id for each generation to avoid stale cache
    let run_id = format!("committee-{}", uuid::Uuid::new_v4());

    // Empty managed config — committee CLI doesn't need hooks/plugins/MCP
    let managed = ManagedConfig {
        mcp_servers: &std::collections::HashMap::new(),
        hooks: &std::collections::HashMap::new(),
        enabled_plugins: &std::collections::HashMap::new(),
    };

    let materialized = write_provider_claude_config(
        &provider_id,
        platform_id,
        cred,
        &run_id,
        &managed,
        true, // disable_hooks_and_plugins = true for committee
    )?;

    // If model_override is set, patch the JSON to use it
    if let Some(model) = model_override.filter(|m| !m.is_empty()) {
        patch_settings_model(&materialized.json_path, model)?;
    }

    log::info!(
        "[cli_executor] wrote committee settings: {} (platform={}, provider={})",
        materialized.json_path.display(),
        platform_id,
        provider_id,
    );

    Ok(Some(materialized.json_path))
}

/// Minimum age before a committee settings file is eligible for cleanup.
///
/// A single symbol's full committee run (4 roles × 2 rounds + CIO, with
/// retries and timeouts) can take several minutes, and multiple symbols run
/// concurrently — each reusing the same `--settings` file for the whole run.
/// Deleting files indiscriminately would yank a config out from under an
/// in-flight symbol ("Settings file not found"). We only reap files that are
/// older than this window, which safely covers any active run while still
/// preventing unbounded accumulation across sessions.
const COMMITTEE_SETTINGS_MAX_AGE: Duration = Duration::from_secs(2 * 60 * 60);

/// Minimum gap between directory scans by [`cleanup_old_committee_settings`].
///
/// Cleanup runs once per `run_committee_stream` call, but the live page starts
/// up to `max_concurrent_symbols` of them at nearly the same instant — without
/// a gate that would fire N simultaneous `read_dir` + per-file `stat` scans of
/// the same directory for work that only needs doing once per batch. We record
/// the last scan time and skip if it was recent.
const COMMITTEE_CLEANUP_COOLDOWN: Duration = Duration::from_secs(5 * 60);

/// Unix-millis timestamp of the last completed cleanup scan (0 = never).
static LAST_COMMITTEE_CLEANUP: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Clean up stale committee temp settings files from prior runs.
///
/// Only removes files matching `session-committee-*.json` that are older than
/// [`COMMITTEE_SETTINGS_MAX_AGE`]. Age-based reaping avoids the concurrency
/// race where one symbol's cleanup would delete the settings file another
/// concurrently-running symbol is still using.
fn cleanup_old_committee_settings() {
    use std::sync::atomic::Ordering;

    let now = std::time::SystemTime::now();
    let now_millis = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Skip if another symbol's call already scanned within the cooldown window.
    // This collapses the concurrent start-up storm into a single scan per batch.
    let last = LAST_COMMITTEE_CLEANUP.load(Ordering::Relaxed);
    if now_millis != 0
        && last != 0
        && now_millis.saturating_sub(last) < COMMITTEE_CLEANUP_COOLDOWN.as_millis() as u64
    {
        return;
    }
    LAST_COMMITTEE_CLEANUP.store(now_millis, Ordering::Relaxed);

    let dir = crate::storage::data_dir().join("provider-claude-configs");
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !(name_str.starts_with("session-committee-") && name_str.ends_with(".json")) {
            continue;
        }
        // Only remove files clearly older than any active run. If we can't read
        // the mtime, leave the file alone rather than risk deleting a live one.
        let stale = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|mtime| now.duration_since(mtime).ok())
            .is_some_and(|age| age > COMMITTEE_SETTINGS_MAX_AGE);
        if stale {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Patch the `model` field in an existing settings JSON file.
/// Uses atomic write (write to tmp + rename) to avoid partial writes on crash.
fn patch_settings_model(path: &std::path::Path, model: &str) -> Result<(), String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("read settings for patch: {e}"))?;
    let mut json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("parse settings for patch: {e}"))?;

    // The env vars are nested under "env" in the settings JSON
    if let Some(env) = json.get_mut("env").and_then(|v| v.as_object_mut()) {
        env.insert(
            "ANTHROPIC_MODEL".to_string(),
            serde_json::Value::String(model.to_string()),
        );
    }

    let patched = serde_json::to_string_pretty(&json)
        .map_err(|e| format!("serialize patched settings: {e}"))?;

    // Atomic write: write to temp file, then rename
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &patched)
        .map_err(|e| format!("write tmp settings: {e}"))?;
    std::fs::rename(&tmp_path, path)
        .map_err(|e| format!("rename tmp settings: {e}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmt_opt_some() {
        assert_eq!(fmt_opt(Some(3.14159), 2), "3.14");
        assert_eq!(fmt_opt(Some(100.0), 0), "100");
        assert_eq!(fmt_opt(Some(0.123), 3), "0.123");
    }

    #[test]
    fn test_fmt_opt_none() {
        assert_eq!(fmt_opt(None, 2), "N/A");
        assert_eq!(fmt_opt(None, 0), "N/A");
    }

    #[test]
    fn test_strip_tool_section_present() {
        let prompt = r#"Role instruction here.

**你有工具可调用**：
- `tool1()` → desc1
- `tool2()` → desc2

**市场阶段判定规则**：
- rule1
- rule2"#;

        let stripped = strip_tool_section(prompt);
        assert!(stripped.contains("Role instruction here."));
        assert!(stripped.contains("**市场阶段判定规则**"));
        assert!(!stripped.contains("你有工具可调用"));
        assert!(!stripped.contains("tool1"));
    }

    #[test]
    fn test_strip_tool_section_absent() {
        let prompt = "Simple prompt without tools.";
        let stripped = strip_tool_section(prompt);
        assert_eq!(stripped, "Simple prompt without tools.");
    }

    #[test]
    fn test_format_recent_events_empty() {
        // This test requires DB which may not be available in unit test context.
        // Verify the function doesn't panic with empty results.
        let result = format_recent_events_for_prompt();
        // Should return either actual data or the fallback message
        assert!(!result.is_empty());
    }

    #[test]
    fn test_format_recent_verdicts_empty() {
        let result = format_recent_verdicts_for_prompt("test");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_format_macro_cache_empty() {
        let result = format_macro_cache_for_prompt();
        assert!(!result.is_empty());
    }

    #[test]
    fn hit_rates_empty_returns_blank() {
        let agg = crate::storage::invest::verdict_reviews::HitRateAgg {
            global: vec![],
            by_regime: vec![],
        };
        assert!(render_hit_rates(&agg, "neutral").is_empty());
    }

    #[test]
    fn hit_rates_renders_global_rows() {
        use crate::storage::invest::verdict_reviews::{HitRateAgg, HitRateRow};
        let agg = HitRateAgg {
            global: vec![HitRateRow {
                verdict_type: "ACCUMULATE".into(),
                window_days: 1,
                hits: 10,
                total: 21,
                matured: true,
            }],
            by_regime: vec![],
        };
        let out = render_hit_rates(&agg, "risk_off");
        assert!(out.contains("历史命中率参考"));
        assert!(out.contains("ACCUMULATE"));
        assert!(out.contains("n=21"));
    }

    #[test]
    fn test_format_round_outputs_uses_raw_text_not_marker() {
        use crate::invest::committee::analysis::RoundOutput;
        use crate::invest::committee::parser::ParsedFields;
        use crate::invest::committee::roles::CommitteeRole;

        // Simulate a failed output with [WORKER_UNAVAILABLE] in raw_text
        let failed_output = RoundOutput {
            role: CommitteeRole::Quant,
            round: 2,
            parsed: ParsedFields {
                raw_text: "[WORKER_UNAVAILABLE] CLI failed: timeout".to_string(),
                fallback_reason: Some("cli_error: timeout".to_string()),
                ..Default::default()
            },
            latency_ms: 0,
            tokens_used: 0,
        };

        let good_output = RoundOutput {
            role: CommitteeRole::Risk,
            round: 1,
            parsed: ParsedFields {
                raw_text: "SIGNAL: ok\nSTRENGTH: 5".to_string(),
                signal: Some("ok".to_string()),
                ..Default::default()
            },
            latency_ms: 100,
            tokens_used: 200,
        };

        let result = format_round_outputs_for_prompt(&[failed_output, good_output]);
        // Should contain the raw_text (including the marker as-is from the failed output)
        assert!(result.contains("[WORKER_UNAVAILABLE] CLI failed: timeout"));
        // Should NOT have replaced it with a bare "[WORKER_UNAVAILABLE]"
        assert!(result.contains("SIGNAL: ok"));
    }
}
