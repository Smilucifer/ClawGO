use super::analysis::{
    check_convergence, check_sentinel, cio_sanity_check, RoundOutput, SanityCheckResult,
    SentinelOverride,
};
use super::archive::{archive_decision, archive_decision_full};
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
            let result = execute_tool(&tc.name, &tc.arguments.to_string(), symbol).await;
            match result {
                Ok(r) => messages.push(tool_result_message(&tc.id, &r)),
                Err(e) => messages.push(tool_result_message(&tc.id, &format!("Error: {}", e))),
            }
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
) -> Result<(RoundOutput, u32), String> {
    let role = CommitteeRole::Macro;
    let provider = resolve_provider(config, role);
    let llm_config = build_llm_config(provider, role, config.timeout_secs);

    let asset_name = get_asset_name(symbol).unwrap_or_else(|| symbol.to_string());
    let system_prompt = format!(
        "{}{}",
        load_prompt_for_round(role, 1, &asset_name, symbol),
        length_constraint_suffix(role)
    );
    let tool_defs = role_tool_defs(role, 1);

    let governor = global_governor();
    let _permit = governor.acquire(provider).await;

    let mut messages: Vec<Message> = vec![Message::user(format!(
        "请分析 {} 的宏观环境和技术面，给出风险信号判断。",
        symbol
    ))];

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
) -> Result<RoundOutput, String> {
    let provider = resolve_provider(config, role);
    let llm_config = build_llm_config(provider, role, config.timeout_secs);
    let tool_defs = role_tool_defs(role, round);

    let asset_name = get_asset_name(symbol).unwrap_or_else(|| symbol.to_string());
    let system_prompt = format!(
        "{}{}",
        load_prompt_for_round(role, round, &asset_name, symbol),
        length_constraint_suffix(role)
    );

    let governor = global_governor();
    let _permit = governor.acquire(provider).await;

    let mut messages = build_context_messages(round_outputs, symbol, macro_signal, emergency_buffer_cny);

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
/// Pipeline (6 steps):
/// 1. Macro (with tool-call loop) -> signal + strength
/// 2. Debate rounds: Quant/R1 + Risk/R1, then Quant/R2 + Risk/R2, early convergence exit
/// 3. CIO verdict
/// 4. Post-analysis: sentinel, convergence, sanity check
/// 5. Archive (fire-and-forget)
///
/// When `emitter` is `Some`, events are emitted at each pipeline step boundary
/// for real-time frontend streaming via `"committee-event"` Tauri event channel.
pub async fn run_committee(
    client: &dyn InvestLlmClient,
    symbol: &str,
    config: &CommitteeConfig,
    emitter: Option<EventEmitter>,
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

        let (macro_output, macro_tokens) = run_macro_phase(client, symbol, config).await?;
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

    // ── Step 2: Debate rounds ──────────────────────────────────────────
    let converged = run_debate_rounds(
        client,
        symbol,
        config,
        &mut round_outputs,
        &mut total_tokens,
        &macro_signal,
        config.emergency_buffer_cny,
        &emitter,
    )
    .await?;

    // ── Step 3: CIO verdict ────────────────────────────────────────────
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

    // ── Step 4: Post-analysis ──────────────────────────────────────────
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

    // ── Step 5: Archive (fire-and-forget) ──────────────────────────────
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
    if let Err(e) = archive_decision_full(symbol, &result) {
        log::warn!("archive_decision_full failed for {}: {}", symbol, e);
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
) -> Vec<Result<CommitteeResult, String>> {
    let mut handles = Vec::with_capacity(symbols.len());

    for symbol in symbols {
        let client = client.clone();
        let config = config.clone();
        let symbol = symbol.clone();
        handles.push(tokio::spawn(async move {
            run_committee(&*client, &symbol, &config, None).await
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
            run_committee(&*client, &symbol, &config, Some(emitter)).await
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
