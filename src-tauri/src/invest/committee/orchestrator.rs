use super::analysis::{
    check_convergence, check_sentinel, cio_sanity_check, RoundOutput, SanityCheckResult,
    SentinelOverride,
};
use super::archive::archive_decision_full;
use super::events::{step_index_for_role, CommitteeEvent};
use super::parser::{parse_role_output, ParsedFields};
use super::roles::{
    hard_truncate, length_constraint_suffix, load_prompt_for_round, CommitteeRole,
};
use super::tools::{execute_tool, role_tool_defs, tool_result_message};
use crate::invest::llm::governor::global_governor;
use crate::invest::llm::{
    collect_stream, CollectedResponse, InvestLlmClient, LlmConfig, Message, ProviderId, ToolDef,
};
use crate::invest::regime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Callback for emitting committee streaming events.
pub type EventEmitter = Arc<dyn Fn(CommitteeEvent) + Send + Sync>;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitteeConfig {
    /// Number of debate rounds (default 2 = Quant/R1+R2 + Risk/R1+R2).
    pub debate_rounds: u8,
    /// Minimum dry powder (CNY) required to avoid Gate 3 downgrade.
    pub emergency_buffer_cny: f64,
    /// Per-LLM-call timeout in seconds.
    pub timeout_secs: u64,
    /// Per-role provider override. Roles not present use the default.
    pub role_providers: HashMap<CommitteeRole, ProviderId>,
    /// User-configured model override (if set, overrides provider defaults).
    #[serde(default)]
    pub model_override: Option<String>,
}

impl Default for CommitteeConfig {
    fn default() -> Self {
        let mut role_providers = HashMap::new();
        for role in CommitteeRole::all() {
            role_providers.insert(*role, ProviderId::DeepSeek);
        }
        Self {
            debate_rounds: 2,
            emergency_buffer_cny: 100_000.0,
            timeout_secs: 120,
            role_providers,
            model_override: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// Per-role summary for frontend display / serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundOutputSummary {
    pub role: CommitteeRole,
    pub round: u8,
    pub label: String,
    pub parsed: ParsedFields,
    pub latency_ms: u64,
    pub tokens_used: u32,
}

impl From<&RoundOutput> for RoundOutputSummary {
    fn from(output: &RoundOutput) -> Self {
        Self {
            role: output.role,
            round: output.round,
            label: format!("{} R{}", output.role.label(), output.round),
            parsed: output.parsed.clone(),
            latency_ms: output.latency_ms,
            tokens_used: output.tokens_used,
        }
    }
}

/// Complete committee decision output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitteeResult {
    pub symbol: String,
    pub final_verdict: String,
    pub final_confidence: f64,
    pub macro_signal: String,
    pub macro_strength: Option<f64>,
    /// CIO raw reasoning text (preserved for archiving).
    pub reasoning: String,
    /// All role outputs (Macro, Quant(R1/R2), Risk(R1/R2), CIO).
    pub rounds: Vec<RoundOutputSummary>,
    pub total_tokens: u32,
    pub total_latency_ms: u64,
    pub converged: bool,
    pub sentinel_override: Option<SentinelOverride>,
    pub sanity_check: SanityCheckResult,
}

// ---------------------------------------------------------------------------
// Provider defaults
// ---------------------------------------------------------------------------

/// Default provider for a role (all DeepSeek for now).
fn default_role_provider(_role: CommitteeRole) -> ProviderId {
    ProviderId::DeepSeek
}

/// Look up the human-readable asset name from the holdings table for a given
/// symbol. Returns `None` if the symbol is not found or if the DB query fails.
fn get_asset_name(symbol: &str) -> Option<String> {
    use crate::storage::invest::with_conn;
    with_conn(|conn| {
        conn.query_row(
            "SELECT name FROM holdings WHERE symbol = ?1 AND name IS NOT NULL LIMIT 1",
            [symbol],
            |row| row.get::<_, Option<String>>(0),
        )
        .map_err(|e| format!("get_asset_name query: {e}"))
    })
    .ok()
    .flatten()
    .filter(|s| !s.is_empty())
}

/// Pre-loaded portfolio data shared across multiple context builders.
/// Loaded once in `run_committee` and passed by reference to avoid redundant DB reads.
struct PortfolioData {
    holdings: Vec<crate::storage::invest::portfolio::Holding>,
    cash: f64,
    total_notional: f64,
}

impl PortfolioData {
    /// Load portfolio data and refresh notional with current market prices.
    /// NOTE: This function has a side effect — it writes updated notional values
    /// back to the DB for holdings whose price changed by >0.01 CNY.
    async fn load_and_refresh_prices() -> Self {
        use crate::storage::invest::portfolio::{get_cash, list_holdings, upsert_holding};
        use futures_util::StreamExt;

        let mut holdings = list_holdings().unwrap_or_else(|e| {
            log::warn!("portfolio: failed to list holdings: {}", e);
            Vec::new()
        });
        let cash = get_cash().unwrap_or(0.0);

        // Fetch current prices with bounded concurrency (3 parallel requests)
        if let Ok(client) = crate::tushare::client::TushareClient::from_settings() {
            let symbols_with_idx: Vec<(usize, String, f64)> = holdings
                .iter()
                .enumerate()
                .filter_map(|(i, h)| {
                    let shares = h.shares?;
                    if shares > 0.0 {
                        Some((i, h.symbol.clone(), shares))
                    } else {
                        None
                    }
                })
                .collect();

            // Collect futures into a vec first to avoid lifetime issues with async closures
            let mut price_futures = Vec::new();
            for (i, symbol, _shares) in &symbols_with_idx {
                let symbol = symbol.clone();
                let i = *i;
                let c = client.clone();
                price_futures.push(async move {
                    let result = c.get_latest_price(&symbol).await;
                    (i, result.map_err(|e| e.to_string()))
                });
            }
            let prices: Vec<(usize, Result<f64, String>)> =
                futures_util::stream::iter(price_futures)
                    .buffer_unordered(3)
                    .collect()
                    .await;

            for (i, result) in prices {
                let h = &mut holdings[i];
                let shares = h.shares.unwrap_or(0.0);
                match result {
                    Ok(current_price) => {
                        let new_notional = current_price * shares;
                        let old_notional = h.notional;
                        if (new_notional - old_notional).abs() > 0.01 {
                            h.notional = new_notional;
                            if let Err(e) = upsert_holding(h) {
                                log::warn!(
                                    "portfolio: failed to update notional for {}: {}",
                                    h.symbol, e
                                );
                            } else {
                                log::debug!(
                                    "portfolio: updated notional for {}: {:.0} -> {:.0}",
                                    h.symbol, old_notional, new_notional
                                );
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "portfolio: price fetch failed for {}, keeping stale notional: {}",
                            h.symbol, e
                        );
                    }
                }
            }
        } else {
            log::warn!("portfolio: tushare not configured, using stored notional values");
        }

        // Fallback: if notional is 0 but avg_cost and shares are available,
        // compute notional from cost basis. This handles the case where
        // record_trade was called without triggering recalculate_holdings.
        for h in &mut holdings {
            if h.notional.abs() < 0.01 {
                if let (Some(avg_cost), Some(shares)) = (h.avg_cost, h.shares) {
                    if avg_cost > 0.0 && shares > 0.0 {
                        h.notional = avg_cost * shares;
                        log::debug!(
                            "portfolio: fallback notional for {}: {:.2} (avg_cost={:.4} * shares={:.0})",
                            h.symbol, h.notional, avg_cost, shares
                        );
                    }
                }
            }
        }

        let total_notional = holdings.iter().map(|h| h.notional.abs()).sum();
        Self { holdings, cash, total_notional }
    }
}

/// Build a structured portfolio summary from pre-loaded portfolio data.
/// Returns an empty string if no holdings or cash data is available.
fn build_portfolio_summary(data: &PortfolioData) -> String {
    if data.holdings.is_empty() && data.cash <= 0.0 {
        return String::new();
    }

    let mut out = String::from("【组合持仓概览】\n");

    if !data.holdings.is_empty() {
        out.push_str("| 标的 | 名称 | 股数 | 均价 | 市值(CNY) | 集中度 |\n");
        out.push_str("|------|------|------|------|----------|--------|\n");
        for h in &data.holdings {
            let name = h.name.as_deref().unwrap_or("-");
            let shares = h
                .shares
                .map(|s| format!("{:.0}", s))
                .unwrap_or_else(|| "-".to_string());
            let avg_cost = h
                .avg_cost
                .map(|c| format!("{:.3}", c))
                .unwrap_or_else(|| "-".to_string());
            let notional = format!("{:.2}", h.notional);
            let concentration = if data.total_notional > 0.0 {
                format!("{:.1}%", h.notional.abs() / data.total_notional * 100.0)
            } else {
                "-".to_string()
            };
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} |\n",
                h.symbol, name, shares, avg_cost, notional, concentration
            ));
        }
        out.push_str(&format!("总市值: {:.2} CNY\n", data.total_notional));
    }

    out.push_str(&format!("现金: {:.2} CNY", data.cash));

    out
}

/// Default temperature for a role.
fn default_role_temperature(role: CommitteeRole) -> f64 {
    match role {
        CommitteeRole::Cio => 0.1,
        _ => 0.7,
    }
}

/// Load all active strategies and format them as a context block for prompt
/// injection. Returns an empty string if no strategies are configured.
fn build_strategy_context() -> String {
    let strategies = match crate::storage::invest::strategy::list_strategies() {
        Ok(s) => s,
        Err(e) => {
            log::warn!("build_strategy_context: failed to list strategies: {}", e);
            return String::new();
        }
    };

    if strategies.is_empty() {
        return String::new();
    }

    let mut out = String::from("【当前投资策略配置】\n");

    for (i, s) in strategies.iter().enumerate() {
        out.push_str(&format!("\n策略 {}: {}\n", i + 1, s.name));

        // Targets summary
        if !s.targets.is_empty() {
            out.push_str("  目标配置:\n");
            for t in &s.targets {
                if let Some(obj) = t.as_object() {
                    let label = obj
                        .get("label")
                        .or_else(|| obj.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("未命名");
                    let weight = obj
                        .get("weight")
                        .or_else(|| obj.get("target_pct"))
                        .and_then(|v| v.as_f64())
                        .map(|w| format!("{:.1}%", w))
                        .unwrap_or_else(|| "N/A".to_string());
                    out.push_str(&format!("    - {}: 权重 {}\n", label, weight));
                }
            }
        }

        // Constraints
        if let Some(max_pct) = s.max_single_pct {
            out.push_str(&format!("  单一资产上限: {:.1}%\n", max_pct));
        }
        if let Some(min_cash) = s.min_cash_pct {
            out.push_str(&format!("  最低现金仓位: {:.1}%\n", min_cash));
        }
    }

    out.push_str("\n请在裁决时遵循上述策略约束。如策略配置与当前分析存在冲突，在 PERSONAL_NOTE 中说明。\n");

    out
}

/// Load user profile and format as a context block for Risk/CIO prompt injection.
/// Includes account purpose, family support, and lifestyle notes.
/// Returns an empty string if no meaningful profile data is configured.
fn build_user_profile_context() -> String {
    let profile = match crate::storage::invest::user_profile::get_profile() {
        Ok(Some(p)) => p,
        Ok(None) => return String::new(),
        Err(e) => {
            log::warn!("build_user_profile_context: failed to load profile: {e}");
            return String::new();
        }
    };

    let purpose_label = match profile.account_purpose.as_str() {
        "default" => "默认（无特定目标约束）",
        "pocket_money" => "零花钱账户（小额闲钱，灵活进出，亏损不影响生活）",
        "long_term" => "长期投资账户（3-5年以上周期，能承受较大波动）",
        "retirement" => "退休金（安全性优先，严格控制回撤，偏好蓝筹高股息）",
        "education" => "教育金（有明确用款时间，稳健与成长平衡）",
        "other" => "其他",
        _ => "未设置",
    };

    let support_label = match profile.family_support.as_deref() {
        Some("none") | None => "无家族经济支持",
        Some("occasional") => "偶尔有家族经济支持",
        Some("partial") => "有部分家族经济支持",
        Some("full") => "有全面家族经济支持",
        _ => "未设置",
    };

    let mut out = String::from("【用户投资档案】\n");
    out.push_str(&format!("账户用途: {}\n", purpose_label));
    out.push_str(&format!("家族支持: {}\n", support_label));

    if !profile.lifestyle_notes.is_empty() {
        out.push_str(&format!("用户备注: {}\n", profile.lifestyle_notes));
    }

    out.push_str("\n请根据上述用户档案调整裁决的激进程度和仓位建议。例如：退休金账户应更保守，零花钱账户可更灵活，无家族支持时需更注重安全边际。\n");

    out
}

// ---------------------------------------------------------------------------
// Per-symbol risk metrics
// ---------------------------------------------------------------------------

/// Pre-compute CONCENTRATION_PCT and PNL_PCT for a specific symbol.
/// Returns a context string to inject into Risk R1 messages.
/// Uses pre-loaded portfolio data to avoid redundant DB reads.
fn build_risk_metrics_context(symbol: &str, data: &PortfolioData) -> String {
    let holding = data.holdings.iter().find(|h| h.symbol == symbol);

    let concentration_pct = holding
        .map(|h| {
            if data.total_notional > 0.0 {
                h.notional.abs() / data.total_notional * 100.0
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);

    let pnl_pct = holding
        .and_then(|h| {
            let shares = h.shares?;
            let avg_cost = h.avg_cost?;
            if shares > 0.0 && avg_cost > 0.0 {
                let current_price = h.notional / shares;
                Some((current_price - avg_cost) / avg_cost * 100.0)
            } else {
                None
            }
        })
        .unwrap_or(0.0);

    format!(
        "【预计算风险指标】\n集中度: {:.1}\n盈亏比: {:.1}\n可用子弹: {:.2}\n\n请在分析中直接使用上述预计算指标，无需重新计算。\n",
        concentration_pct, pnl_pct, data.cash
    )
}

// ---------------------------------------------------------------------------
// LLM call helpers
// ---------------------------------------------------------------------------

/// Build an LlmConfig for the given role and provider.
fn build_llm_config(
    provider: ProviderId,
    role: CommitteeRole,
    timeout_secs: u64,
    model_override: Option<&str>,
) -> LlmConfig {
    LlmConfig {
        provider,
        model: model_override
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| provider.default_model())
            .to_string(),
        temperature: default_role_temperature(role),
        max_tokens: 4096,
        timeout_secs,
    }
}

/// Resolve the provider for a role from config (falling back to default).
fn resolve_provider(config: &CommitteeConfig, role: CommitteeRole) -> ProviderId {
    config
        .role_providers
        .get(&role)
        .copied()
        .unwrap_or_else(|| default_role_provider(role))
}

/// LLM call with simple retry (mirrors `call_with_retry` logic but takes
/// direct references instead of a closure, avoiding async-closure lifetime
/// issues).
async fn llm_call_with_retry(
    client: &dyn InvestLlmClient,
    system: &str,
    messages: &[Message],
    tools: Option<&[ToolDef]>,
    config: &LlmConfig,
) -> Result<CollectedResponse, String> {
    let mut delay = std::time::Duration::from_millis(500);
    let mut last_err = String::new();

    for attempt in 0..3 {
        match client.chat_stream(system, messages, tools, config).await {
            Ok(stream) => return Ok(collect_stream(stream).await),
            Err(crate::invest::llm::LlmError::RateLimit { retry_after_ms }) => {
                let d = retry_after_ms
                    .map(std::time::Duration::from_millis)
                    .unwrap_or(delay);
                last_err = "Rate limited".to_string();
                log::warn!(
                    "LLM rate limited on attempt {}, retrying in {:?}",
                    attempt + 1,
                    d
                );
                tokio::time::sleep(d).await;
                delay *= 2;
            }
            Err(
                e @ (crate::invest::llm::LlmError::Timeout
                | crate::invest::llm::LlmError::NetworkError(_)
                | crate::invest::llm::LlmError::ServerError(_)
                | crate::invest::llm::LlmError::ParseError(_)),
            ) => {
                log::warn!(
                    "LLM call attempt {} failed: {}, retrying in {:?}",
                    attempt + 1,
                    e,
                    delay
                );
                last_err = format!("{}", e);
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
            Err(e) => {
                // 401 / 400 — do not retry
                return Err(format!("LLM call failed (no retry): {}", e));
            }
        }
    }

    Err(format!(
        "LLM call failed after 3 retries: {}",
        last_err
    ))
}

// ---------------------------------------------------------------------------
// Context builder
// ---------------------------------------------------------------------------

/// Build the messages array for a role, injecting all prior round outputs as
/// context, plus macro signal, regime data, emergency buffer, and portfolio
/// summary.
fn build_context_messages(
    round_outputs: &[RoundOutput],
    symbol: &str,
    macro_signal: &str,
    emergency_buffer_cny: f64,
    portfolio_summary: &str,
    regime_context: Option<&str>,
) -> Vec<Message> {
    if round_outputs.is_empty() {
        let mut user_msg = format!("请分析 {} 的投资机会。", symbol);
        if !portfolio_summary.is_empty() {
            user_msg.push_str(&format!("\n\n{}", portfolio_summary));
        }
        return vec![Message::user(user_msg)];
    }

    let mut context = format!(
        "【标的: {}】\nMacro SIGNAL: {}\nEmergency Buffer: {:.2} CNY\n",
        symbol, macro_signal, emergency_buffer_cny
    );
    if !portfolio_summary.is_empty() {
        context.push_str(&portfolio_summary);
        context.push('\n');
    }
    // Inject regime data (RSI-14, price quantile, trend classification)
    if let Some(rc) = regime_context {
        context.push_str("\n");
        context.push_str(rc);
        context.push('\n');
    }
    for output in round_outputs {
        context.push_str(&format!(
            "\n=== {} Round {} ===\n{}\n",
            output.role.label(),
            output.round,
            output.parsed.raw_text,
        ));
    }

    vec![Message::user(format!(
        "以下是委员会之前的分析结果：{}\n\n请基于以上信息给出你的分析。",
        context
    ))]
}

// ---------------------------------------------------------------------------
// Shared tool-call loop (used by Macro, Quant, Risk)
// ---------------------------------------------------------------------------

/// Run an LLM turn with an optional tool-call loop.
///
/// When `tool_defs` is `Some`, the first LLM call is made with tools. If the
/// model requests tool calls, they are executed and a second call (without
/// tools) produces the final text. When `tool_defs` is `None`, a single call
/// is made without tools.
async fn run_with_tool_loop(
    client: &dyn InvestLlmClient,
    symbol: &str,
    role: CommitteeRole,
    round: u8,
    system_prompt: &str,
    messages: &mut Vec<Message>,
    tool_defs: Option<&[ToolDef]>,
    llm_config: &LlmConfig,
    start: std::time::Instant,
    emitter: &Option<EventEmitter>,
) -> Result<(RoundOutput, u32), String> {
    let mut total_tokens: u32 = 0;

    // First call — with or without tools depending on tool_defs
    let response1 = match llm_call_with_retry(
        client,
        system_prompt,
        messages,
        tool_defs,
        llm_config,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("LLM first-pass call failed for {:?} R{}: {}", role, round, e);
            let latency_ms = start.elapsed().as_millis() as u64;
            return Ok((
                RoundOutput {
                    role,
                    round,
                    parsed: ParsedFields {
                        raw_text: "[WORKER_UNAVAILABLE]".to_string(),
                        ..Default::default()
                    },
                    latency_ms,
                    tokens_used: 0,
                },
                0,
            ));
        }
    };
    total_tokens += response1.usage.total_tokens;

    if !response1.tool_calls.is_empty() && tool_defs.is_some() {
        // Build assistant message carrying tool-call metadata (OpenAI format)
        let tool_calls_json: Vec<serde_json::Value> = response1
            .tool_calls
            .iter()
            .map(|tc| {
                serde_json::json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        "arguments": tc.arguments.to_string()
                    }
                })
            })
            .collect();

        messages.push(Message {
            role: "assistant".to_string(),
            content: response1.content.clone(),
            tool_call_id: None,
            tool_calls: Some(tool_calls_json),
            name: None,
        });

        // Execute each tool call and append results
        for tc in &response1.tool_calls {
            let tool_start = std::time::Instant::now();
            let tool_result = execute_tool(&tc.name, &tc.arguments.to_string(), symbol).await;
            let tool_latency = tool_start.elapsed().as_millis() as u64;
            let (success, result_msg) = match &tool_result {
                Ok(r) => (true, r.clone()),
                Err(e) => (false, format!("Error: {}", e)),
            };
            if let Some(ref emit) = emitter {
                emit(CommitteeEvent::ToolCall {
                    symbol: symbol.to_string(),
                    role,
                    round,
                    tool_name: tc.name.clone(),
                    arguments: tc.arguments.to_string(),
                    result: Some(result_msg.clone()),
                    success,
                    latency_ms: tool_latency,
                });
            }
            messages.push(tool_result_message(&tc.id, &result_msg));
        }

        // Second call — without tools — to get final text
        let response2 =
            match llm_call_with_retry(client, system_prompt, messages, None, llm_config).await {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("LLM second-pass call failed for {:?} R{}: {}", role, round, e);
                    let latency_ms = start.elapsed().as_millis() as u64;
                    return Ok((
                        RoundOutput {
                            role,
                            round,
                            parsed: ParsedFields {
                                raw_text: "[WORKER_UNAVAILABLE]".to_string(),
                                ..Default::default()
                            },
                            latency_ms,
                            tokens_used: total_tokens,
                        },
                        total_tokens,
                    ));
                }
            };
        total_tokens += response2.usage.total_tokens;

        let (text, truncated) = hard_truncate(&response2.content, role, 0);
        let parsed = parse_role_output(role, &text, truncated);
        let latency_ms = start.elapsed().as_millis() as u64;

        Ok((
            RoundOutput {
                role,
                round,
                parsed,
                latency_ms,
                tokens_used: total_tokens,
            },
            total_tokens,
        ))
    } else {
        // No tool calls or no tools provided — use first-pass content directly
        let (text, truncated) = hard_truncate(&response1.content, role, 0);
        let parsed = parse_role_output(role, &text, truncated);
        let latency_ms = start.elapsed().as_millis() as u64;

        Ok((
            RoundOutput {
                role,
                round,
                parsed,
                latency_ms,
                tokens_used: total_tokens,
            },
            total_tokens,
        ))
    }
}

// ---------------------------------------------------------------------------
// Macro phase
// ---------------------------------------------------------------------------

async fn run_macro_phase(
    client: &dyn InvestLlmClient,
    symbol: &str,
    config: &CommitteeConfig,
    portfolio_summary: &str,
    emitter: &Option<EventEmitter>,
) -> Result<(RoundOutput, u32), String> {
    let role = CommitteeRole::Macro;
    let provider = resolve_provider(config, role);
    let llm_config = build_llm_config(provider, role, config.timeout_secs, config.model_override.as_deref());

    let asset_name = get_asset_name(symbol).unwrap_or_else(|| symbol.to_string());
    let system_prompt = format!(
        "{}{}",
        load_prompt_for_round(role, 1, &asset_name, symbol),
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

// ---------------------------------------------------------------------------
// Generic role phase (Quant, Risk, CIO)
// ---------------------------------------------------------------------------

async fn run_role_phase(
    client: &dyn InvestLlmClient,
    symbol: &str,
    role: CommitteeRole,
    round: u8,
    config: &CommitteeConfig,
    round_outputs: &[RoundOutput],
    macro_signal: &str,
    emergency_buffer_cny: f64,
    portfolio_summary: &str,
    regime_context: Option<&str>,
    emitter: &Option<EventEmitter>,
    portfolio_data: &PortfolioData,
) -> Result<RoundOutput, String> {
    let provider = resolve_provider(config, role);
    let llm_config = build_llm_config(provider, role, config.timeout_secs, config.model_override.as_deref());
    let tool_defs = role_tool_defs(role, round);

    let asset_name = get_asset_name(symbol).unwrap_or_else(|| symbol.to_string());
    let mut system_prompt = format!(
        "{}{}",
        load_prompt_for_round(role, round, &asset_name, symbol),
        length_constraint_suffix(role)
    );

    // For CIO role, inject active strategy constraints and user profile into the system prompt
    if role == CommitteeRole::Cio {
        let strategy_ctx = build_strategy_context();
        if !strategy_ctx.is_empty() {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&strategy_ctx);
        }
    }

    // Inject user profile for roles that need user context (CIO always, Risk R1 for liquidity assessment)
    if role == CommitteeRole::Cio || (role == CommitteeRole::Risk && round == 1) {
        let profile_ctx = build_user_profile_context();
        if !profile_ctx.is_empty() {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&profile_ctx);
        }
    }

    let governor = global_governor();
    let _permit = governor.acquire(provider).await;

    let mut messages = build_context_messages(round_outputs, symbol, macro_signal, emergency_buffer_cny, portfolio_summary, regime_context);

    // For Risk R1, inject pre-computed risk metrics (CONCENTRATION_PCT, PNL_PCT, DRY_POWDER_CNY)
    if role == CommitteeRole::Risk && round == 1 {
        let risk_ctx = build_risk_metrics_context(symbol, portfolio_data);
        if !risk_ctx.is_empty() {
            if let Some(last) = messages.last_mut() {
                last.content.push_str(&format!("\n\n{}", risk_ctx));
            }
        }
    }

    // For Round 2 rebuttal roles, append a rebuttal-specific instruction
    if round >= 2 && matches!(role, CommitteeRole::Quant | CommitteeRole::Risk) {
        if let Some(last) = messages.last_mut() {
            last.content.push_str(
                "\n\n这是反驳轮（Round 2），请基于之前的分析给出你的反驳或确认。",
            );
        }
    }

    let start = std::time::Instant::now();

    let (output, _tokens) = run_with_tool_loop(
        client,
        symbol,
        role,
        round,
        &system_prompt,
        &mut messages,
        tool_defs.as_deref(),
        &llm_config,
        start,
        emitter,
    )
    .await?;

    Ok(output)
}

// ---------------------------------------------------------------------------
// Debate rounds
// ---------------------------------------------------------------------------

/// Run Quant + Risk debate rounds. Returns `true` if early convergence was
/// detected after round 2+.
async fn run_debate_rounds(
    client: &dyn InvestLlmClient,
    symbol: &str,
    config: &CommitteeConfig,
    round_outputs: &mut Vec<RoundOutput>,
    total_tokens: &mut u32,
    macro_signal: &str,
    emergency_buffer_cny: f64,
    emitter: &Option<EventEmitter>,
    portfolio_summary: &str,
    regime_context: Option<&str>,
    portfolio_data: &PortfolioData,
) -> Result<bool, String> {
    let max_rounds = config.debate_rounds;
    let mut converged = false;

    for round in 1..=max_rounds {
        // Both Quant and Risk participate in each round
        let roles = vec![CommitteeRole::Quant, CommitteeRole::Risk];

        for role in roles {
            let si = step_index_for_role(role, round);
            if let Some(ref emit) = emitter {
                emit(CommitteeEvent::RoleStart {
                    symbol: symbol.to_string(),
                    role,
                    round,
                    step_index: si,
                });
            }

            let output = run_role_phase(
                client,
                symbol,
                role,
                round,
                config,
                round_outputs,
                macro_signal,
                emergency_buffer_cny,
                portfolio_summary,
                regime_context,
                emitter,
                &portfolio_data,
            )
            .await?;
            *total_tokens += output.tokens_used;

            if let Some(ref emit) = emitter {
                emit(CommitteeEvent::RoleComplete {
                    symbol: symbol.to_string(),
                    role,
                    round,
                    summary: RoundOutputSummary::from(&output),
                    step_index: si,
                });
            }

            round_outputs.push(output);
        }

        // Check convergence after round 2+
        if round >= 2 && check_convergence(round_outputs) {
            converged = true;
            log::info!(
                "Committee converged after round {} for {}",
                round,
                symbol
            );
            break;
        }
    }

    Ok(converged)
}

// ---------------------------------------------------------------------------
// Main pipeline
// ---------------------------------------------------------------------------

/// Run the full committee pipeline for a single symbol.
///
/// Pipeline (7 steps):
/// 1. Macro (with tool-call loop) -> signal + strength
/// 2. Regime computation (quantitative: RSI-14, MA, volatility, price quantile)
/// 3. Debate rounds: Quant/R1 + Risk/R1, then Quant/R2 + Risk/R2, early convergence exit
/// 4. CIO verdict
/// 5. Post-analysis: sentinel, convergence, sanity check
/// 6. Archive (fire-and-forget)
///
/// Portfolio data is built once as a shared context block and injected into
/// Macro and subsequent roles — it is not a separate pipeline step.
///
/// When `emitter` is `Some`, events are emitted at each pipeline step boundary
/// for real-time frontend streaming via `"committee-event"` Tauri event channel.
pub async fn run_committee(
    client: &dyn InvestLlmClient,
    symbol: &str,
    config: &CommitteeConfig,
    emitter: Option<EventEmitter>,
    dry_run: bool,
) -> Result<CommitteeResult, String> {
    let start = std::time::Instant::now();

    // Override emergency_buffer_cny from user profile if explicitly saved
    let mut config_owned = config.clone();
    if let Ok(Some(profile)) = crate::storage::invest::user_profile::get_profile() {
        config_owned.emergency_buffer_cny = profile.emergency_buffer_cny;
    }
    let config = &config_owned;
    let mut round_outputs: Vec<RoundOutput> = Vec::new();
    let mut total_tokens: u32 = 0;

    // Load portfolio data with current prices for injection into Macro and Risk R1
    let portfolio_data = PortfolioData::load_and_refresh_prices().await;
    let portfolio_summary = build_portfolio_summary(&portfolio_data);

    // ── Step 1: Macro phase (with tool-call loop) ──────────────────────
    {
        let si = step_index_for_role(CommitteeRole::Macro, 1);
        if let Some(ref emit) = emitter {
            emit(CommitteeEvent::RoleStart {
                symbol: symbol.to_string(),
                role: CommitteeRole::Macro,
                round: 1,
                step_index: si,
            });
        }

        let (macro_output, macro_tokens) =
            run_macro_phase(client, symbol, config, &portfolio_summary, &emitter).await?;
        total_tokens += macro_tokens;

        if let Some(ref emit) = emitter {
            emit(CommitteeEvent::RoleComplete {
                symbol: symbol.to_string(),
                role: CommitteeRole::Macro,
                round: 1,
                summary: RoundOutputSummary::from(&macro_output),
                step_index: si,
            });
        }

        round_outputs.push(macro_output);
    }

    let macro_signal = round_outputs[0]
        .parsed
        .signal
        .clone()
        .unwrap_or_else(|| "neutral".to_string());
    let macro_strength = round_outputs[0].parsed.strength;

    // ── Step 2: REGIME computation ─────────────────────────────────────
    // Compute quantitative regime metrics (RSI-14, MA, volatility, price
    // quantile) after Macro and inject into Quant/Risk/CIO context.
    let regime_si = 1; // step_index for REGIME node
    let regime_context: Option<String> = {
        let regime_result = match crate::tushare::client::TushareClient::from_settings() {
            Ok(client) => regime::compute_regime_for_symbol(&client, symbol).await,
            Err(e) => Err(e),
        };

        // Compute structured fields + context in one pass
        let (success, context_preview, regime_fields, ctx) = match regime_result {
            Ok(result) => {
                let ctx = regime::format_regime_context(&result);
                log::info!("REGIME computed for {}: {}", symbol, result.regime);
                let preview = ctx.lines().next().unwrap_or("").to_string();
                (
                    true,
                    preview,
                    (
                        Some(result.regime.to_string()),
                        Some(result.reason),
                        Some(result.strategy_hint.to_string()),
                        Some(result.metrics),
                    ),
                    Some(ctx),
                )
            }
            Err(e) => {
                log::warn!("REGIME computation failed for {}: {}", symbol, e);
                (false, format!("Error: {}", e), (None, None, None, None), None)
            }
        };

        if let Some(ref emit) = emitter {
            emit(CommitteeEvent::RegimeStep {
                symbol: symbol.to_string(),
                success,
                context_preview,
                step_index: regime_si,
                regime: regime_fields.0,
                reason: regime_fields.1,
                strategy_hint: regime_fields.2,
                metrics: regime_fields.3,
            });
        }

        ctx
    };

    // ── Step 3: Debate rounds ──────────────────────────────────────────
    let converged = run_debate_rounds(
        client,
        symbol,
        config,
        &mut round_outputs,
        &mut total_tokens,
        &macro_signal,
        config.emergency_buffer_cny,
        &emitter,
        &portfolio_summary,
        regime_context.as_deref(),
        &portfolio_data,
    )
    .await?;

    // ── Step 4: CIO verdict ────────────────────────────────────────────
    {
        let si = step_index_for_role(CommitteeRole::Cio, 1);
        if let Some(ref emit) = emitter {
            emit(CommitteeEvent::RoleStart {
                symbol: symbol.to_string(),
                role: CommitteeRole::Cio,
                round: 1,
                step_index: si,
            });
        }

        let cio_output = run_role_phase(
            client,
            symbol,
            CommitteeRole::Cio,
            1,
            config,
            &round_outputs,
            &macro_signal,
            config.emergency_buffer_cny,
            &portfolio_summary,
            regime_context.as_deref(),
            &emitter,
            &portfolio_data,
        )
        .await?;
        total_tokens += cio_output.tokens_used;

        if let Some(ref emit) = emitter {
            emit(CommitteeEvent::RoleComplete {
                symbol: symbol.to_string(),
                role: CommitteeRole::Cio,
                round: 1,
                summary: RoundOutputSummary::from(&cio_output),
                step_index: si,
            });
        }

        round_outputs.push(cio_output);
    }

    // ── Step 5: Post-analysis ──────────────────────────────────────────
    let sentinel = check_sentinel(&round_outputs);

    let cio_parsed = round_outputs
        .iter()
        .rev()
        .find(|o| o.role == CommitteeRole::Cio)
        .map(|o| o.parsed.clone())
        .unwrap_or_default();

    let sanity = cio_sanity_check(
        &cio_parsed,
        &round_outputs,
        &macro_signal,
        config.emergency_buffer_cny,
    );

    // Determine final verdict — sentinel override takes priority
    let (final_verdict, final_confidence) = if let Some(ref s) = sentinel {
        log::info!("SENTINEL override for {}: {}", symbol, s.reason);
        (s.forced_verdict.clone(), s.forced_confidence)
    } else {
        (sanity.final_verdict.clone(), sanity.final_confidence)
    };

    let total_latency_ms = start.elapsed().as_millis() as u64;
    let reasoning = cio_parsed.raw_text.clone();

    // ── Step 6: Archive (fire-and-forget) ──────────────────────────────
    // Skip archiving in dry_run mode — results are returned but not persisted.
    // Uses daily-overwrite strategy: each symbol keeps only the latest
    // verdict per calendar day.
    if !dry_run {
        let cio_provider = resolve_provider(config, CommitteeRole::Cio);
        let asset_name = get_asset_name(symbol);

        if let Err(e) = crate::storage::invest::committees::archive_verdict(
            symbol,
            asset_name.as_deref(),
            &final_verdict,
            final_confidence,
            Some(&macro_signal),
            macro_strength,
            &reasoning,
            cio_provider.default_model(),
            &cio_provider.to_string(),
            total_tokens,
            total_latency_ms,
        ) {
            log::warn!("archive_verdict failed for {}: {}", symbol, e);
        }
    }

    let result = CommitteeResult {
        symbol: symbol.to_string(),
        final_verdict,
        final_confidence,
        macro_signal,
        macro_strength,
        reasoning,
        rounds: round_outputs.iter().map(RoundOutputSummary::from).collect(),
        total_tokens,
        total_latency_ms,
        converged,
        sentinel_override: sentinel,
        sanity_check: sanity,
    };

    // Archive full report (markdown + events.jsonl) — fire-and-forget
    // Skip in dry_run mode.
    if !dry_run {
        if let Err(e) = archive_decision_full(symbol, &result) {
            log::warn!("archive_decision_full failed for {}: {}", symbol, e);
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Batch mode (concurrent multi-symbol execution)
// ---------------------------------------------------------------------------

/// Run committee analysis for multiple symbols concurrently, respecting
/// per-provider concurrency limits via the governor.
/// Non-streaming wrapper — no events emitted.
pub async fn run_committee_batch(
    client: Arc<dyn InvestLlmClient>,
    symbols: &[String],
    config: &CommitteeConfig,
    dry_run: bool,
) -> Vec<Result<CommitteeResult, String>> {
    let mut handles = Vec::with_capacity(symbols.len());

    for symbol in symbols {
        let client = client.clone();
        let config = config.clone();
        let symbol = symbol.clone();
        handles.push(tokio::spawn(async move {
            run_committee(&*client, &symbol, &config, None, dry_run).await
        }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(r) => results.push(r),
            Err(e) => results.push(Err(format!("task join error: {}", e))),
        }
    }
    results
}

/// Run committee analysis for multiple symbols concurrently with real-time
/// event emission. Each symbol's pipeline emits `CommitteeEvent`s via the
/// provided emitter as roles start/complete.
pub async fn run_committee_batch_stream(
    client: Arc<dyn InvestLlmClient>,
    symbols: &[String],
    config: &CommitteeConfig,
    emitter: EventEmitter,
    dry_run: bool,
) -> Vec<Result<CommitteeResult, String>> {
    // Emit batch-start event
    emitter(CommitteeEvent::CommitteeStart {
        symbols: symbols.to_vec(),
        total: symbols.len(),
    });

    let mut handles: Vec<(String, _)> = Vec::with_capacity(symbols.len());

    for symbol in symbols {
        let client = client.clone();
        let config = config.clone();
        let symbol = symbol.clone();
        let emitter = emitter.clone();
        handles.push((symbol.clone(), tokio::spawn(async move {
            run_committee(&*client, &symbol, &config, Some(emitter), dry_run).await
        })));
    }

    let mut results = Vec::with_capacity(handles.len());
    let mut completed = 0usize;
    let total = handles.len();

    for (sym, handle) in handles {
        match handle.await {
            Ok(r) => {
                match &r {
                    Ok(result) => {
                        emitter(CommitteeEvent::SymbolComplete {
                            symbol: result.symbol.clone(),
                            result: result.clone(),
                        });
                    }
                    Err(e) => {
                        emitter(CommitteeEvent::Error {
                            symbol: sym.clone(),
                            error: e.clone(),
                        });
                        log::warn!("committee batch task error for {}: {}", sym, e);
                    }
                }
                completed += 1;
                results.push(r);
            }
            Err(e) => {
                emitter(CommitteeEvent::Error {
                    symbol: sym.clone(),
                    error: format!("task join error: {}", e),
                });
                completed += 1;
                results.push(Err(format!("task join error: {}", e)));
            }
        }
    }

    emitter(CommitteeEvent::Done {
        completed,
        total,
    });

    results
}
