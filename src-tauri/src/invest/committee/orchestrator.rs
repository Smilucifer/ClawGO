use super::analysis::{
    check_convergence, check_sentinel, cio_sanity_check, RoundOutput, SanityCheckResult,
    SentinelOverride,
};
use super::archive::archive_decision;
use super::parser::{parse_role_output, ParsedFields};
use super::roles::{hard_truncate, length_constraint_suffix, load_prompt, CommitteeRole};
use super::tools::{execute_tool, macro_tool_defs, tool_result_message};
use crate::invest::llm::governor::global_governor;
use crate::invest::llm::{
    collect_stream, CollectedResponse, InvestLlmClient, LlmConfig, Message, ProviderId, ToolDef,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitteeConfig {
    /// Number of debate rounds (default 2 = QuantR1/R1 + QuantR2/R2).
    pub debate_rounds: u8,
    /// Minimum dry powder (CNY) required to avoid Gate 3 downgrade.
    pub emergency_buffer_cny: f64,
    /// Per-LLM-call timeout in seconds.
    pub timeout_secs: u64,
    /// Per-role provider override. Roles not present use the default.
    pub role_providers: HashMap<CommitteeRole, ProviderId>,
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
        }
    }
}

// ---------------------------------------------------------------------------
// Results
// ---------------------------------------------------------------------------

/// Per-role summary for frontend display / serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct CommitteeResult {
    pub symbol: String,
    pub final_verdict: String,
    pub final_confidence: f64,
    pub macro_signal: String,
    pub macro_strength: Option<f64>,
    /// CIO raw reasoning text (preserved for archiving).
    pub reasoning: String,
    /// All role outputs (Macro, QuantR1, RiskR1, Wealth, QuantR2, RiskR2, CIO).
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

/// Default temperature for a role.
fn default_role_temperature(role: CommitteeRole) -> f64 {
    match role {
        CommitteeRole::Cio => 0.1,
        _ => 0.7,
    }
}

// ---------------------------------------------------------------------------
// LLM call helpers
// ---------------------------------------------------------------------------

/// Build an LlmConfig for the given role and provider.
fn build_llm_config(
    provider: ProviderId,
    role: CommitteeRole,
    timeout_secs: u64,
) -> LlmConfig {
    LlmConfig {
        provider,
        model: provider.default_model().to_string(),
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
/// context, plus macro signal and emergency buffer.
fn build_context_messages(
    round_outputs: &[RoundOutput],
    symbol: &str,
    macro_signal: &str,
    emergency_buffer_cny: f64,
) -> Vec<Message> {
    if round_outputs.is_empty() {
        return vec![Message::user(format!(
            "请分析 {} 的投资机会。",
            symbol
        ))];
    }

    let mut context = format!(
        "【标的: {}】\nMacro SIGNAL: {}\nEmergency Buffer: {:.0} CNY\n",
        symbol, macro_signal, emergency_buffer_cny
    );
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
// Macro phase (with tool-call loop)
// ---------------------------------------------------------------------------

async fn run_macro_phase(
    client: &dyn InvestLlmClient,
    symbol: &str,
    config: &CommitteeConfig,
) -> Result<(RoundOutput, u32), String> {
    let role = CommitteeRole::Macro;
    let provider = resolve_provider(config, role);
    let llm_config = build_llm_config(provider, role, config.timeout_secs);

    let system_prompt = format!("{}{}", load_prompt(role), length_constraint_suffix(role));
    let tool_defs = macro_tool_defs();

    let governor = global_governor();
    let _permit = governor.acquire(provider).await;

    let mut messages: Vec<Message> = vec![Message::user(format!(
        "请分析 {} 的宏观环境和技术面，给出风险信号判断。",
        symbol
    ))];

    let start = std::time::Instant::now();
    let mut total_tokens: u32 = 0;

    // First call — with tools
    let response1 = match llm_call_with_retry(
        client,
        &system_prompt,
        &messages,
        Some(&tool_defs),
        &llm_config,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("Macro first-pass LLM call failed: {}", e);
            let latency_ms = start.elapsed().as_millis() as u64;
            return Ok((
                RoundOutput {
                    role,
                    round: 1,
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

    if !response1.tool_calls.is_empty() {
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
            let result = execute_tool(&tc.name, &tc.arguments.to_string(), symbol).await;
            match result {
                Ok(r) => messages.push(tool_result_message(&tc.id, &r)),
                Err(e) => messages.push(tool_result_message(&tc.id, &format!("Error: {}", e))),
            }
        }

        // Second call — without tools — to get final text
        let response2 =
            match llm_call_with_retry(client, &system_prompt, &messages, None, &llm_config).await {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("Macro second-pass LLM call failed: {}", e);
                    let latency_ms = start.elapsed().as_millis() as u64;
                    return Ok((
                        RoundOutput {
                            role,
                            round: 1,
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
                round: 1,
                parsed,
                latency_ms,
                tokens_used: total_tokens,
            },
            total_tokens,
        ))
    } else {
        // No tool calls — use first-pass content directly
        let (text, truncated) = hard_truncate(&response1.content, role, 0);
        let parsed = parse_role_output(role, &text, truncated);
        let latency_ms = start.elapsed().as_millis() as u64;

        Ok((
            RoundOutput {
                role,
                round: 1,
                parsed,
                latency_ms,
                tokens_used: total_tokens,
            },
            total_tokens,
        ))
    }
}

// ---------------------------------------------------------------------------
// Generic role phase (Quant, Risk, Wealth, CIO — no tool calls)
// ---------------------------------------------------------------------------

async fn run_role_phase(
    client: &dyn InvestLlmClient,
    symbol: &str,
    role: CommitteeRole,
    config: &CommitteeConfig,
    round_outputs: &[RoundOutput],
    macro_signal: &str,
    emergency_buffer_cny: f64,
) -> Result<RoundOutput, String> {
    let provider = resolve_provider(config, role);
    let llm_config = build_llm_config(provider, role, config.timeout_secs);

    let system_prompt = format!("{}{}", load_prompt(role), length_constraint_suffix(role));

    let governor = global_governor();
    let _permit = governor.acquire(provider).await;

    let round = match role {
        CommitteeRole::QuantR2 | CommitteeRole::RiskR2 => 2,
        _ => 1,
    };

    let mut messages = build_context_messages(round_outputs, symbol, macro_signal, emergency_buffer_cny);

    // For Round 2 rebuttal roles, append a rebuttal-specific instruction
    if matches!(role, CommitteeRole::QuantR2 | CommitteeRole::RiskR2) {
        if let Some(last) = messages.last_mut() {
            last.content.push_str(
                "\n\n这是反驳轮（Round 2），请基于之前的分析给出你的反驳或确认。",
            );
        }
    }

    let start = std::time::Instant::now();

    let response = match llm_call_with_retry(client, &system_prompt, &messages, None, &llm_config)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("LLM call failed for {:?}: {}", role, e);
            let latency_ms = start.elapsed().as_millis() as u64;
            return Ok(RoundOutput {
                role,
                round,
                parsed: ParsedFields {
                    raw_text: "[WORKER_UNAVAILABLE]".to_string(),
                    ..Default::default()
                },
                latency_ms,
                tokens_used: 0,
            });
        }
    };

    let (text, truncated) = hard_truncate(&response.content, role, 0);
    let parsed = parse_role_output(role, &text, truncated);
    let latency_ms = start.elapsed().as_millis() as u64;

    Ok(RoundOutput {
        role,
        round,
        parsed,
        latency_ms,
        tokens_used: response.usage.total_tokens,
    })
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
) -> Result<bool, String> {
    let max_rounds = config.debate_rounds;
    let mut converged = false;

    for round in 1..=max_rounds {
        let roles = if round == 1 {
            vec![CommitteeRole::QuantR1, CommitteeRole::RiskR1]
        } else {
            vec![CommitteeRole::QuantR2, CommitteeRole::RiskR2]
        };

        for role in roles {
            let output = run_role_phase(client, symbol, role, config, round_outputs, macro_signal, emergency_buffer_cny).await?;
            *total_tokens += output.tokens_used;
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
/// Pipeline:
/// 1. Macro (with tool-call loop) → signal + strength
/// 2. Debate rounds: QuantR1/R1 + QuantR2/R2, early convergence exit
/// 3. Wealth context
/// 4. CIO verdict
/// 5. Post-analysis: sentinel, convergence, sanity check
/// 6. Archive (fire-and-forget)
pub async fn run_committee(
    client: &dyn InvestLlmClient,
    symbol: &str,
    config: &CommitteeConfig,
) -> Result<CommitteeResult, String> {
    let start = std::time::Instant::now();
    let mut round_outputs: Vec<RoundOutput> = Vec::new();
    let mut total_tokens: u32 = 0;

    // ── Step 1: Macro phase (with tool-call loop) ──────────────────────
    let (macro_output, macro_tokens) = run_macro_phase(client, symbol, config).await?;
    total_tokens += macro_tokens;
    round_outputs.push(macro_output);

    let macro_signal = round_outputs[0]
        .parsed
        .signal
        .clone()
        .unwrap_or_else(|| "neutral".to_string());
    let macro_strength = round_outputs[0].parsed.strength;

    // ── Step 2: Debate rounds ──────────────────────────────────────────
    let converged =
        run_debate_rounds(client, symbol, config, &mut round_outputs, &mut total_tokens, &macro_signal, config.emergency_buffer_cny).await?;

    // ── Step 3: Wealth context ─────────────────────────────────────────
    let wealth_output =
        run_role_phase(client, symbol, CommitteeRole::Wealth, config, &round_outputs, &macro_signal, config.emergency_buffer_cny).await?;
    total_tokens += wealth_output.tokens_used;
    round_outputs.push(wealth_output);

    // ── Step 4: CIO verdict ────────────────────────────────────────────
    let cio_output =
        run_role_phase(client, symbol, CommitteeRole::Cio, config, &round_outputs, &macro_signal, config.emergency_buffer_cny).await?;
    total_tokens += cio_output.tokens_used;
    round_outputs.push(cio_output);

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
    let cio_provider = resolve_provider(config, CommitteeRole::Cio);

    if let Err(e) = archive_decision(
        symbol,
        &final_verdict,
        final_confidence,
        Some(&macro_signal),
        macro_strength,
        &reasoning,
        cio_provider.default_model(),
        &cio_provider.to_string(),
        total_tokens,
        total_latency_ms,
    )
    .await
    {
        log::warn!("archive_decision failed for {}: {}", symbol, e);
    }

    Ok(CommitteeResult {
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
    })
}

// ---------------------------------------------------------------------------
// Batch mode (concurrent multi-symbol execution)
// ---------------------------------------------------------------------------

/// Run committee analysis for multiple symbols concurrently, respecting
/// per-provider concurrency limits via the governor.
pub async fn run_committee_batch(
    client: Arc<dyn InvestLlmClient>,
    symbols: &[String],
    config: &CommitteeConfig,
) -> Vec<Result<CommitteeResult, String>> {
    let mut handles = Vec::with_capacity(symbols.len());

    for symbol in symbols {
        let client = client.clone();
        let config = config.clone();
        let symbol = symbol.clone();
        handles.push(tokio::spawn(async move {
            run_committee(&*client, &symbol, &config).await
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
