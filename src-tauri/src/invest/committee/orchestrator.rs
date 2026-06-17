use super::analysis::{
    check_convergence, check_sentinel, cio_sanity_check, RoundOutput, SanityCheckResult,
    SentinelOverride,
};
use super::archive::archive_decision_full;
use super::events::{step_index_for_role, CommitteeEvent};
use super::parser::{detect_fallback_reason, parse_role_output, ParsedFields};
use super::roles::{
    hard_truncate, length_constraint_suffix, load_prompt_for_round, CommitteeRole,
};
use super::tools::{execute_tool, role_tool_defs, tool_result_message};
use crate::invest::llm::governor::global_governor;
use crate::invest::llm::{
    collect_stream, CollectedResponse, InvestLlmClient, LlmConfig, Message, ProviderId, ToolDef,
};
use crate::invest::regime;
use crate::storage::invest::stock_data_cache;
use crate::tushare::client::TushareClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Maximum number of symbol pipelines running concurrently.
const MAX_CONCURRENT_SYMBOLS: usize = 5;

/// 单个标的的上下文数据，注入到各角色 prompt 中
#[derive(Debug, Clone, Default, Serialize)]
pub struct AssetContext {
    pub asset_type: String,              // "stock" | "etf"
    pub industry: Option<String>,        // tushare stock_basic → 申万二级行业
    pub money_flow_summary: Option<String>,       // 近5日主力/散户净流入摘要（工具用）
    pub money_flow_daily_summary: Option<String>, // 当日主力/散户净流入摘要（prompt 注入）
    pub pe_ttm: Option<f64>,
    pub pb: Option<f64>,
    pub total_mv_yi: Option<f64>,        // 总市值（亿元）
    pub roe: Option<f64>,                // 最新季度 ROE
    pub or_yoy: Option<f64>,             // 营收增速%
    pub np_yoy: Option<f64>,             // 净利增速%
    pub rating_summary: Option<String>,  // "买入15/增持3/中性1/减持0/卖出0"
    pub risk_news: Option<String>,       // 最多5条风险新闻
    pub turnover_rate: Option<f64>,

    // ── 8 段补充字段 ──
    pub circ_mv_yi: Option<f64>,         // 流通市值（亿元）
    pub roa: Option<f64>,                // ROA%
    pub debt_to_assets: Option<f64>,     // 资产负债率%
    pub latest_close: Option<f64>,       // 最新价（rt_k 实时，不缓存）
    pub pre_close: Option<f64>,          // 昨收价
    pub data_quality: Vec<String>,       // 缺失字段清单，如 ["PE=N/A", "评级=N/A"]

    // ── 预计算技术指标（仅 Quant R1 注入）──
    pub precomputed_indicators: Option<String>,
}

/// Callback for emitting committee streaming events.
pub type EventEmitter = Arc<dyn Fn(CommitteeEvent) + Send + Sync>;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitteeConfig {
    /// Number of debate rounds (default 2 = Quant/R1+R2 + Risk/R1+R2).
    pub debate_rounds: u8,
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
#[derive(Clone)]
pub(crate) struct PortfolioData {
    holdings: Vec<crate::storage::invest::portfolio::Holding>,
    cash: f64,
    total_notional: f64,
    /// `true` when at least one holding's notional was estimated from avg_cost
    /// rather than fetched from a live market price.
    notional_is_estimated: bool,
}

impl PortfolioData {
    /// 总资产 = 总持仓市值 + 现金。
    /// 集中度分母与前端 CommitteeLiveTab 的 `totalAssets` 公式对齐。
    fn total_assets(&self) -> f64 {
        self.total_notional + self.cash
    }

    /// Load portfolio data and refresh notional with current market prices.
    /// NOTE: When `dry_run=false`, writes updated notional values back to the
    /// DB for holdings whose price changed by >0.01 CNY.  When `dry_run=true`,
    /// prices are still fetched (for accurate notional) but nothing is persisted.
    async fn load_and_refresh_prices(dry_run: bool) -> Self {
        use crate::storage::invest::portfolio::{get_cash, list_holdings, update_holding_notional};
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
                            if !dry_run {
                                if let Err(e) = update_holding_notional(&h.symbol, &h.currency, &h.kind, h.notional) {
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
        let mut notional_is_estimated = false;
        for h in &mut holdings {
            if h.notional.abs() < 0.01 {
                if let (Some(avg_cost), Some(shares)) = (h.avg_cost, h.shares) {
                    if avg_cost > 0.0 && shares > 0.0 {
                        h.notional = avg_cost * shares;
                        notional_is_estimated = true;
                        log::debug!(
                            "portfolio: fallback notional for {}: {:.2} (avg_cost={:.4} * shares={:.0})",
                            h.symbol, h.notional, avg_cost, shares
                        );
                    }
                }
            }
        }

        let total_notional = holdings.iter().map(|h| h.notional.abs()).sum();
        Self { holdings, cash, total_notional, notional_is_estimated }
    }

    /// Load portfolio data with a 30-second timeout. Returns an empty portfolio
    /// on timeout, preventing the pipeline from hanging on unresponsive APIs.
    async fn load_with_timeout(dry_run: bool) -> Self {
        match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Self::load_and_refresh_prices(dry_run),
        )
        .await
        {
            Ok(data) => data,
            Err(_) => {
                log::warn!("portfolio: load_and_refresh_prices timed out after 30s, using empty portfolio");
                Self::default()
            }
        }
    }
}

impl Default for PortfolioData {
    fn default() -> Self {
        Self {
            holdings: Vec::new(),
            cash: 0.0,
            total_notional: 0.0,
            notional_is_estimated: false,
        }
    }
}

/// Build a structured portfolio summary from pre-loaded portfolio data.
/// Returns an empty string if no holdings or cash data is available.
fn build_portfolio_summary(data: &PortfolioData) -> String {
    if data.holdings.is_empty() && data.cash <= 0.0 {
        return String::new();
    }

    let mut out = String::from("【组合持仓概览】\n");

    let total_assets = data.total_assets();

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
            let concentration = if total_assets > 0.0 {
                format!("{:.1}%", h.notional.abs() / total_assets * 100.0)
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

    // Derive risk preference from account purpose — no separate setting needed.
    let risk_label = match profile.account_purpose.as_str() {
        "pocket_money" => "激进型：可承受高波动，追求短期高收益，允许集中持仓",
        "long_term" => "稳健型：注重价值和分红，能承受中等波动，偏好分散配置",
        "retirement" => "保守型：优先保本，严格控制回撤，偏好蓝筹高股息",
        "education" => "保守型：安全性优先，有明确用款时间约束",
        _ => "中性型：默认风险偏好，平衡收益与安全",
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
    out.push_str(&format!("风险偏好: {}\n", risk_label));
    out.push_str(&format!("家族支持: {}\n", support_label));

    if !profile.lifestyle_notes.is_empty() {
        out.push_str(&format!("用户备注: {}\n", profile.lifestyle_notes));
    }

    out.push_str("\n请根据上述用户档案调整裁决的激进程度和仓位建议。风险偏好为激进型时可更灵活地推荐高波动标的；保守型时应优先考虑安全边际和回撤控制。\n");

    out
}

// ---------------------------------------------------------------------------
// Concentration helper
// ---------------------------------------------------------------------------

/// 计算指定标的的集中度百分比。
/// 分母 = 总资产（含现金），与前端 CommitteeLiveTab 对齐。
fn concentration_for_symbol(symbol: &str, portfolio_data: &PortfolioData) -> f64 {
    let total_assets = portfolio_data.total_assets();
    if total_assets <= 0.0 {
        return 0.0;
    }
    let symbol_notional: f64 = portfolio_data
        .holdings
        .iter()
        .filter(|h| h.symbol == symbol)
        .map(|h| h.notional.abs())
        .sum();
    symbol_notional / total_assets * 100.0
}

// ---------------------------------------------------------------------------
// Per-symbol risk metrics
// ---------------------------------------------------------------------------

/// 构建 Risk 角色的预计算风险指标上下文
/// 包括集中度、可用子弹、盈亏比、最大回撤、标的风险摘要
fn build_risk_metrics_context(
    portfolio_data: &PortfolioData,
    symbol: &str,
    asset_context: &AssetContext,
) -> String {
    let holding = portfolio_data.holdings.iter().find(|h| h.symbol == symbol);

    let concentration_pct = concentration_for_symbol(symbol, portfolio_data);

    let (pnl_pct, current_price, avg_cost, shares) = holding
        .and_then(|h| {
            let shares = h.shares?;
            let avg_cost = h.avg_cost?;
            if shares > 0.0 && avg_cost > 0.0 {
                let current_price = h.notional / shares;
                let pnl = (current_price - avg_cost) / avg_cost * 100.0;
                Some((pnl, current_price, avg_cost, shares))
            } else {
                None
            }
        })
        .unwrap_or((0.0, 0.0, 0.0, 0.0));

    // 最大回撤（假设价格跌 20%）
    let max_dd = crate::storage::invest::portfolio::max_drawdown_for_symbol(
        symbol, current_price, avg_cost, shares,
    );

    // 标的风险摘要
    let mut risk_notes = Vec::new();
    if let Some(pe) = asset_context.pe_ttm {
        if pe > 100.0 {
            risk_notes.push(format!("PE_TTM={:.1}（偏高）", pe));
        }
    }
    if let Some(pb) = asset_context.pb {
        if pb > 10.0 {
            risk_notes.push(format!("PB={:.2}（偏高）", pb));
        }
    }
    if let Some(or_yoy) = asset_context.or_yoy {
        if or_yoy < 0.0 {
            risk_notes.push(format!("营收增速{:.1}%（转负）", or_yoy));
        }
    }
    if let Some(np_yoy) = asset_context.np_yoy {
        if np_yoy < 0.0 {
            risk_notes.push(format!("净利增速{:.1}%（转负）", np_yoy));
        }
    }

    let mut out = format!(
        "【预计算风险指标】\n集中度: {:.1}%\n盈亏比: {:.1}%\n可用子弹: {:.2} CNY\n最大回撤(20%假设): {:.1}%\n",
        concentration_pct, pnl_pct, portfolio_data.cash, max_dd * 100.0
    );

    if !risk_notes.is_empty() {
        out.push_str(&format!("标的风险: {}\n", risk_notes.join("，")));
    }

    if portfolio_data.notional_is_estimated {
        out.push_str("⚠️ 注意：部分持仓市值为成本估算值（未获取到实时价格），盈亏比和集中度数据可能不准确。\n");
    }

    out.push_str("\n请在分析中直接使用上述预计算指标，无需重新计算。\n");
    out
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
// Asset context builder
// ---------------------------------------------------------------------------

/// 定向刷新资金流向缓存（核心数据齐全但 moneyflow_dc 缺失时调用）。
///
/// 成功时写入缓存并返回更新后的 cache_entries（含新增条目）；
/// 失败或数据为空时返回原始 entries（调用方通过检查 moneyflow_dc 是否存在来判断结果）。
async fn refresh_moneyflow_cache(
    client: &TushareClient,
    symbol: &str,
    mut entries: Vec<(String, String, String)>,
) -> Vec<(String, String, String)> {
    use crate::tushare::client::MoneyflowDc;

    log::info!(
        "build_asset_context: moneyflow_dc missing for {}, attempting targeted refresh",
        symbol
    );
    let now = chrono::Utc::now();
    let today = now.format("%Y%m%d").to_string();
    let five_days_ago = (now - chrono::Duration::days(5))
        .format("%Y%m%d")
        .to_string();

    match client.moneyflow_dc(symbol, &five_days_ago, &today).await {
        Ok(mf) if !mf.is_empty() => {
            let s = MoneyflowDc::to_cache_json(&mf);
            let _ = stock_data_cache::batch_upsert(&[(
                symbol,
                "moneyflow_dc",
                today.as_str(),
                s.as_str(),
            )]);
            // 内存追加新条目，避免 DB 重读
            entries.push(("moneyflow_dc".to_string(), today, s));
            log::info!(
                "build_asset_context: moneyflow_dc refreshed for {}",
                symbol
            );
        }
        Ok(_) => {
            log::info!(
                "build_asset_context: moneyflow_dc returned empty for {} (API may lack data for this symbol)",
                symbol
            );
        }
        Err(e) => {
            log::warn!(
                "build_asset_context: moneyflow_dc refresh failed for {}: {} (check Tushare API permission for moneyflow_dc endpoint)",
                symbol, e
            );
        }
    }
    entries
}

/// 构建标的上下文数据，注入到角色 prompt 中
///
/// 独立 API 调用通过 `tokio::join!` 并行执行，减少总耗时。
async fn build_asset_context(
    client: &TushareClient,
    symbol: &str,
    asset_type: &str,
) -> AssetContext {
    use crate::tushare::client::{DailyBasic, FinaIndicator, ReportRc};

    let mut data_quality: Vec<String> = Vec::new();

    // ── 1. 尝试从 cache 读取所有数据类型 ──
    let mut cache_entries = stock_data_cache::load_all_latest_for_symbol(symbol)
        .unwrap_or_default();

    let has_type = |t: &str| cache_entries.iter().any(|(dt, _, _)| dt == t);
    let has_daily_basic = has_type("daily_basic");
    let has_fina = has_type("fina_indicator");
    let has_moneyflow = has_type("moneyflow_dc");

    // ── 2. 核心数据缺失 → 全量刷新 ──
    if !has_daily_basic || !has_fina {
        if let Err(e) = refresh_asset_data(client, symbol, asset_type).await {
            log::warn!("build_asset_context: refresh failed for {}: {}", symbol, e);
        }
        cache_entries = stock_data_cache::load_all_latest_for_symbol(symbol)
            .unwrap_or_default();
    } else if asset_type == "stock" && !has_moneyflow {
        cache_entries = refresh_moneyflow_cache(client, symbol, cache_entries).await;
    }

    let find_json = |dt: &str| -> Option<String> {
        cache_entries.iter().find(|(t, _, _)| t == dt).map(|(_, _, j)| j.clone())
    };

    // ── 3. 实时价：每次都从 rt_k 获取（不缓存） ──
    let (latest_close, pre_close) = match client.realtime_quotes(&[symbol]).await {
        Ok(quotes) => {
            if let Some(q) = quotes.first() {
                (Some(q.close), Some(q.pre_close))
            } else {
                data_quality.push("最新价=N/A".to_string());
                (None, None)
            }
        }
        Err(e) => {
            log::warn!("build_asset_context: realtime_quotes failed for {}: {}", symbol, e);
            data_quality.push("最新价=N/A".to_string());
            (None, None)
        }
    };

    // ── 4. 解析 daily_basic（typed deserialization）──
    let (pe_ttm, pb, turnover_rate, total_mv_yi, circ_mv_yi) =
        if let Some(json) = find_json("daily_basic") {
            if let Ok(b) = serde_json::from_str::<DailyBasic>(&json) {
                (
                    b.pe_ttm,
                    b.pb,
                    b.turnover_rate_f.or(b.turnover_rate),
                    b.total_mv.map(|v| v / 10000.0),
                    b.circ_mv.map(|v| v / 10000.0),
                )
            } else {
                data_quality.push("PE=N/A".to_string());
                (None, None, None, None, None)
            }
        } else {
            data_quality.push("PE=N/A".to_string());
            (None, None, None, None, None)
        };

    // ── 5. 解析 fina_indicator（typed deserialization）──
    let (roe, roa, or_yoy, np_yoy, debt_to_assets) =
        if let Some(json) = find_json("fina_indicator") {
            if let Ok(f) = serde_json::from_str::<FinaIndicator>(&json) {
                (f.roe, f.roa, f.or_yoy, f.netprofit_yoy, f.debt_to_assets)
            } else {
                (None, None, None, None, None)
            }
        } else {
            data_quality.push("财务指标=N/A".to_string());
            (None, None, None, None, None)
        };

    // ── 6. 解析 report_rc（typed deserialization）──
    let rating_summary = if let Some(json) = find_json("report_rc") {
        if let Ok(r) = serde_json::from_str::<ReportRc>(&json) {
            let buy = r.buy_num.unwrap_or(0.0) as i64 + r.strong_buy_num.unwrap_or(0.0) as i64;
            let hold = r.hold_num.unwrap_or(0.0) as i64;
            let reduce = r.reduce_num.unwrap_or(0.0) as i64;
            let sell = r.sell_num.unwrap_or(0.0) as i64;
            let total = buy + hold + reduce + sell;
            let neutral = (total - buy - reduce - sell).max(0);
            Some(format!("买入{}/增持{}/中性{}/减持{}/卖出{}", buy, 0, neutral, reduce, sell))
        } else {
            data_quality.push("评级=N/A".to_string());
            None
        }
    } else {
        data_quality.push("评级=N/A".to_string());
        None
    };

    // ── 7. 解析 moneyflow_dc ──
    use crate::tushare::client::MoneyflowCachePayload;
    let moneyflow_cache: Option<MoneyflowCachePayload> = find_json("moneyflow_dc")
        .and_then(|json| serde_json::from_str(&json).ok());
    // 股票无资金流向数据 → 记录数据质量问题（非 ETF 且缓存缺失）
    if moneyflow_cache.is_none() && asset_type == "stock" {
        data_quality.push("资金流向=N/A".to_string());
    }
    // 解构避免多次 clone：daily_summary 优先，fallback 到 summary（旧缓存兼容）
    let (money_flow_summary, money_flow_daily_summary) = match moneyflow_cache {
        Some(c) => {
            let daily = if c.daily_summary.is_empty() { c.summary.clone() } else { c.daily_summary };
            (Some(c.summary), Some(daily))
        }
        None => (None, None),
    };

    // ── 8. 解析 industry ──
    let industry = if let Some(json) = find_json("industry") {
        serde_json::from_str::<serde_json::Value>(&json)
            .ok()
            .and_then(|v| v["industry"].as_str().map(|s| s.to_string()))
    } else if asset_type == "etf" {
        None
    } else {
        data_quality.push("行业=N/A".to_string());
        None
    };

    // ── 9. 预计算技术指标（Quant R1 专用）──
    let precomputed_indicators = {
        use crate::invest::indicators;
        let end_date = chrono::Local::now().format("%Y%m%d").to_string();
        let start_date = (chrono::Local::now() - chrono::Duration::days(750))
            .format("%Y%m%d")
            .to_string();

        match client.daily(symbol, &start_date, &end_date).await {
            Ok(bars) if bars.len() >= 20 => {
                // bars from daily() are newest-first
                let closes_desc: Vec<f64> = bars.iter().map(|b| b.close).collect();
                let latest = closes_desc[0];

                let mean_all = closes_desc.iter().sum::<f64>() / closes_desc.len() as f64;
                let ma5 = indicators::compute_ma(&closes_desc, 5);
                let ma20 = indicators::compute_ma(&closes_desc, 20);
                let ma60 = indicators::compute_ma(&closes_desc, 60);
                let ma120 = indicators::compute_ma(&closes_desc, 120);

                // RSI and volatility need chronological order
                let mut closes_chrono = closes_desc.clone();
                closes_chrono.reverse();
                let rsi14 = indicators::compute_rsi14(&closes_chrono);
                let volatility = indicators::compute_volatility(&closes_chrono);

                let window_len = closes_desc.len().min(500);
                let percentile =
                    indicators::compute_price_percentile(latest, &closes_desc[..window_len]);

                let trend = indicators::classify_trend(
                    latest,
                    ma5.unwrap_or(mean_all),
                    ma20.unwrap_or(mean_all),
                    ma60.unwrap_or(mean_all),
                    ma120,
                );

                let fmt_ma = |v: Option<f64>| v.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "N/A".into());

                let hv20_text = if volatility == 0.0 {
                    format!("N/A (仅{}日数据)", bars.len())
                } else {
                    format!("{:.1}%", volatility * 100.0)
                };

                let pct_window = closes_desc.len().min(750);
                Some(format!(
                    "MA5={} | MA20={} | MA60={} | MA120={}\n\
                     RSI14={:.1} | HV20(年化)={} | 价格分位({}日)={:.0}%\n\
                     趋势={}",
                    fmt_ma(ma5),
                    fmt_ma(ma20),
                    fmt_ma(ma60),
                    fmt_ma(ma120),
                    rsi14,
                    hv20_text,
                    pct_window,
                    percentile,
                    trend,
                ))
            }
            _ => None,
        }
    };

    AssetContext {
        asset_type: asset_type.to_string(),
        industry,
        money_flow_summary,
        money_flow_daily_summary,
        pe_ttm,
        pb,
        total_mv_yi,
        roe,
        or_yoy,
        np_yoy,
        rating_summary,
        risk_news: None, // 由 get_company_news tool 获取，不缓存
        turnover_rate,
        circ_mv_yi,
        roa,
        debt_to_assets,
        latest_close,
        pre_close,
        data_quality,
        precomputed_indicators,
    }
}

/// 批量刷新标的数据到 stock_data_cache。
///
/// 并行调用 tushare API，结果写入永久缓存。
/// ETF 标的自动跳过 stock_basic 和 report_rc（无数据）。
async fn refresh_asset_data(
    client: &TushareClient,
    symbol: &str,
    asset_type: &str,
) -> Result<(), String> {
    use crate::tushare::client::MoneyflowDc;

    let today = chrono::Utc::now().format("%Y%m%d").to_string();
    let five_days_ago = (chrono::Utc::now() - chrono::Duration::days(5))
        .format("%Y%m%d")
        .to_string();

    // 并行调用独立 API
    let (basic_result, fina_result, rc_result, mf_result) = tokio::join!(
        client.daily_basic(symbol, None, None, None),
        client.fina_indicator(symbol, None, None, None),
        client.report_rc(symbol, None),
        client.moneyflow_dc(symbol, &five_days_ago, &today),
    );

    // ── 批量写入缓存（单事务，一次 fsync）──
    // (data_type, data_date, value_json) — symbol is constant
    let mut entries: Vec<(String, String, String)> = Vec::new();

    if let Ok(basics) = &basic_result {
        if let Some(latest) = basics.first() {
            if let Ok(json) = serde_json::to_string(latest) {
                entries.push(("daily_basic".into(), latest.trade_date.clone(), json));
            }
        }
    }
    if let Ok(finas) = &fina_result {
        if let Some(latest) = finas.first() {
            if let Ok(json) = serde_json::to_string(latest) {
                let date = latest.end_date.clone().unwrap_or_else(|| today.clone());
                entries.push(("fina_indicator".into(), date, json));
            }
        }
    }
    if let Ok(rcs) = &rc_result {
        if let Some(latest) = rcs.first() {
            if let Ok(json) = serde_json::to_string(latest) {
                let date = latest.report_date.clone().unwrap_or_else(|| today.clone());
                entries.push(("report_rc".into(), date, json));
            }
        }
    }
    if let Ok(mf) = &mf_result {
        if !mf.is_empty() {
            entries.push(("moneyflow_dc".into(), today.clone(), MoneyflowDc::to_cache_json(mf)));
        }
    }
    if asset_type == "stock" {
        if let Ok(stocks) = client.stock_basic(Some(symbol)).await {
            if let Some(s) = stocks.first() {
                let json = serde_json::json!({ "industry": s.industry });
                entries.push(("industry".into(), today.clone(), json.to_string()));
            }
        }
    }

    if !entries.is_empty() {
        let batch: Vec<(&str, &str, &str, &str)> = entries
            .iter()
            .map(|(dt, date, json)| (symbol, dt.as_str(), date.as_str(), json.as_str()))
            .collect();
        let _ = stock_data_cache::batch_upsert(&batch);
    }

    Ok(())
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
    min_cash_reserve: f64,
    portfolio_summary: &str,
    regime_context: Option<&str>,
    _asset_context: &AssetContext,
) -> Vec<Message> {
    if round_outputs.is_empty() {
        let mut user_msg = format!("请分析 {} 的投资机会。", symbol);
        if !portfolio_summary.is_empty() {
            user_msg.push_str(&format!("\n\n{}", portfolio_summary));
        }
        return vec![Message::user(user_msg)];
    }

    let mut context = format!(
        "【标的: {}】\nMacro SIGNAL: {}\n最低现金储备: {:.2} CNY\n",
        symbol, macro_signal, min_cash_reserve
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
        // Use WORKER_UNAVAILABLE marker for fallback outputs so the CIO prompt's
        // safety-valve rule ("任何 worker 输出含 [WORKER_UNAVAILABLE]") triggers correctly.
        // For non-fallback outputs, use raw_text (pre-merge original) to preserve
        // the LLM's full reasoning for downstream context.
        let content = if output.parsed.fallback_reason.is_some() {
            "[WORKER_UNAVAILABLE]"
        } else {
            &output.parsed.raw_text
        };
        context.push_str(&format!(
            "\n=== {} Round {} ===\n{}\n",
            output.role.label(),
            output.round,
            content,
        ));
    }

    vec![Message::user(format!(
        "以下是委员会之前的分析结果：{}\n\n请基于以上信息给出你的分析。",
        context
    ))]
}

// ---------------------------------------------------------------------------
// Shared retry-on-fallback helper
// ---------------------------------------------------------------------------

/// Retry an LLM call once if the parsed output has missing critical fields.
/// Appends a format-reminder user message, re-calls the LLM, and uses the
/// retry result only if it resolves the fallback. Returns the (possibly
/// upgraded) parsed output and additional tokens consumed.
async fn retry_on_fallback(
    client: &dyn InvestLlmClient,
    role: CommitteeRole,
    round: u8,
    system_prompt: &str,
    messages: &mut Vec<Message>,
    llm_config: &LlmConfig,
    parsed: &mut ParsedFields,
    total_tokens: &mut u32,
) {
    if parsed.fallback_reason.is_none() {
        return;
    }
    log::info!("Parse fallback for {:?} R{}, retrying with format reminder", role, round);
    let retry_prompt = format!(
        "你的上一次输出缺少关键字段或格式不正确。请严格按照 KEY: value 格式重新输出，确保包含所有必需字段。角色：{}，轮次：{}。",
        role.label(), round
    );
    messages.push(Message::user(retry_prompt));
    if let Ok(retry_resp) = llm_call_with_retry(client, system_prompt, messages, None, llm_config).await {
        *total_tokens += retry_resp.usage.total_tokens;
        let (retry_text, retry_truncated) = hard_truncate(&retry_resp.content, role, 0);
        let mut retry_parsed = parse_role_output(role, &retry_text, retry_truncated);
        retry_parsed.fallback_reason = detect_fallback_reason(role, &retry_parsed);
        if retry_parsed.fallback_reason.is_none() {
            *parsed = retry_parsed;
            log::info!("Retry resolved fallback for {:?} R{}", role, round);
        } else {
            log::warn!("Retry still has fallback for {:?} R{}: {:?}", role, round, retry_parsed.fallback_reason);
        }
    }
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
///
/// Note: DSML (DeepSeek/MiMo native) and plain `<tool_call>` tool-call
/// normalization is handled upstream in `collect_stream()`, so
/// `response1.tool_calls` is already populated correctly regardless of the
/// provider's wire format.
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
                    parsed: {
                        let mut p = ParsedFields {
                            raw_text: "[WORKER_UNAVAILABLE]".to_string(),
                            ..Default::default()
                        };
                        p.fallback_reason = detect_fallback_reason(role, &p);
                        p
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
        // Tool calls present (already normalized by collect_stream) — execute them
        let CollectedResponse { content, tool_calls, .. } = response1;

        let tool_calls_json: Vec<serde_json::Value> = tool_calls
            .iter()
            .map(|tc| {
                let args_str = tc.arguments.to_string();
                serde_json::json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        "arguments": args_str
                    }
                })
            })
            .collect();

        messages.push(Message {
            role: "assistant".to_string(),
            content,
            tool_call_id: None,
            tool_calls: Some(tool_calls_json),
            name: None,
        });

        // Execute each tool call and append results
        for tc in &tool_calls {
            let tool_start = std::time::Instant::now();
            let args_str = tc.arguments.to_string();
            let tool_result = execute_tool(&tc.name, &args_str, symbol).await;
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
                    arguments: args_str,
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
                    return Ok({
                        let mut p = ParsedFields {
                            raw_text: "[WORKER_UNAVAILABLE]".to_string(),
                            ..Default::default()
                        };
                        p.fallback_reason = detect_fallback_reason(role, &p);
                        (
                            RoundOutput {
                                role,
                                round,
                                parsed: p,
                                latency_ms,
                                tokens_used: total_tokens,
                            },
                            total_tokens,
                        )
                    });
                }
            };
        total_tokens += response2.usage.total_tokens;

        let (text, truncated) = hard_truncate(&response2.content, role, 0);
        let mut parsed = parse_role_output(role, &text, truncated);
        parsed.fallback_reason = detect_fallback_reason(role, &parsed);

        retry_on_fallback(client, role, round, system_prompt, messages, llm_config, &mut parsed, &mut total_tokens).await;

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
        // No tool calls — use first-pass content directly
        let (text, truncated) = hard_truncate(&response1.content, role, 0);
        let mut parsed = parse_role_output(role, &text, truncated);
        parsed.fallback_reason = detect_fallback_reason(role, &parsed);

        retry_on_fallback(client, role, round, system_prompt, messages, llm_config, &mut parsed, &mut total_tokens).await;

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

                if parsed.fallback_reason.is_some() {
                    log::warn!(
                        "run_macro_phase: CLI output has fallback reason for {}: {:?}",
                        symbol, parsed.fallback_reason
                    );
                }

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
                log::warn!(
                    "run_macro_phase: CLI failed for {}: {}, falling back to API",
                    symbol, e
                );
                // Fall through to API path below
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

// ---------------------------------------------------------------------------
// L4 Officer phase (behavioral health check)
// ---------------------------------------------------------------------------

/// 运行 L4 Officer 阶段（在 Risk R2 之后、CIO 之前）
///
/// L4 Officer 不参与辩论轮次，只在所有前序分析完成后执行一次行为健康度检查。
/// Rust 端计算行为红灯评分，LLM 只负责卫语句判定、情绪评估和买点合理性。
async fn run_l4_officer_phase(
    client: &dyn InvestLlmClient,
    symbol: &str,
    config: &CommitteeConfig,
    round_outputs: &[RoundOutput],
    macro_signal: &str,
    min_cash_reserve: f64,
    portfolio_summary: &str,
    regime_context: Option<&str>,
    emitter: &Option<EventEmitter>,
    portfolio_data: &PortfolioData,
    asset_context: &AssetContext,
) -> RoundOutput {
    let role = CommitteeRole::L4Officer;
    let si = step_index_for_role(role, 1);

    if let Some(ref emit) = emitter {
        emit(CommitteeEvent::RoleStart {
            symbol: symbol.to_string(),
            role,
            round: 1,
            step_index: si,
        });
    }

    // 使用通用 role_phase 执行 LLM 调用
    let output = match run_role_phase(
        client,
        symbol,
        role,
        1,
        config,
        round_outputs,
        macro_signal,
        min_cash_reserve,
        portfolio_summary,
        regime_context,
        emitter,
        portfolio_data,
        asset_context,
    )
    .await
    {
        Ok(o) => o,
        Err(e) => {
            log::warn!("L4 Officer phase failed for {}: {}", symbol, e);
            let mut p = super::parser::ParsedFields {
                raw_text: "[WORKER_UNAVAILABLE]".to_string(),
                ..Default::default()
            };
            p.fallback_reason = detect_fallback_reason(role, &p);
            RoundOutput {
                role,
                round: 1,
                parsed: p,
                latency_ms: 0,
                tokens_used: 0,
            }
        }
    };

    // ── Rust 端计算行为红灯评分 ──
    let emotional_state = output
        .parsed
        .l4_emotion_assessment
        .as_deref()
        .unwrap_or("stable");

    let concentration_pct = output
        .parsed
        .concentration_pct
        .or_else(|| {
            // 从 Risk R1/R2 输出中提取集中度
            round_outputs
                .iter()
                .filter(|o| o.role == CommitteeRole::Risk)
                .rev()
                .find_map(|o| o.parsed.concentration_pct)
        })
        .unwrap_or_else(|| concentration_for_symbol(symbol, portfolio_data));

    let dry_powder_cny = output
        .parsed
        .dry_powder_cny
        .or_else(|| {
            // 从 Risk R1/R2 输出中提取可用子弹
            round_outputs
                .iter()
                .filter(|o| o.role == CommitteeRole::Risk)
                .rev()
                .find_map(|o| o.parsed.dry_powder_cny)
        })
        .unwrap_or(min_cash_reserve);

    // 获取近7天交易次数
    let recent_trade_count = crate::storage::invest::portfolio::count_recent_trades(Some(symbol), 7)
        .unwrap_or(0);

    // 计算行为红灯评分
    let (score, level) = super::parser::compute_red_light_score(
        emotional_state,
        concentration_pct,
        dry_powder_cny,
        recent_trade_count,
    );

    // 将评分写入 parsed 字段
    let mut output = output;
    output.parsed.execution_red_light_score = Some(score);

    // 覆盖 L4 的行为红灯字段（Rust 端确定性计算优先于 LLM 输出）
    log::info!(
        "L4 Officer red light for {}: score={:.1}, level={}, trades_7d={}",
        symbol,
        score,
        level,
        recent_trade_count
    );
    output.parsed.l4_red_light = Some(level);

    if let Some(ref emit) = emitter {
        emit(CommitteeEvent::RoleComplete {
            symbol: symbol.to_string(),
            role,
            round: 1,
            summary: RoundOutputSummary::from(&output),
            step_index: si,
        });
    }

    output
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
    min_cash_reserve: f64,
    portfolio_summary: &str,
    regime_context: Option<&str>,
    emitter: &Option<EventEmitter>,
    portfolio_data: &PortfolioData,
    asset_context: &AssetContext,
) -> Result<RoundOutput, String> {
    let provider = resolve_provider(config, role);
    let llm_config = build_llm_config(provider, role, config.timeout_secs, config.model_override.as_deref());
    let tool_defs = role_tool_defs(role, round);

    let asset_name = get_asset_name(symbol).unwrap_or_else(|| symbol.to_string());
    let mut system_prompt = format!(
        "{}{}",
        load_prompt_for_round(role, round, &asset_name, symbol, asset_context),
        length_constraint_suffix(role)
    );

    // For CIO and Risk roles, inject active strategy constraints into the system prompt
    if role == CommitteeRole::Cio || role == CommitteeRole::Risk {
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

    // CIO: inject asset data summary + data quality into system prompt
    if role == CommitteeRole::Cio {
        let mut cio_data = Vec::new();
        if let Some(ref industry) = asset_context.industry {
            cio_data.push(format!("行业: {}", industry));
        }
        if let Some(ref mf) = asset_context.money_flow_daily_summary {
            cio_data.push(format!("资金: {}", mf));
        }
        if let Some(pe) = asset_context.pe_ttm {
            cio_data.push(format!("PE: {:.1}", pe));
        }
        if let Some(ref rating) = asset_context.rating_summary {
            cio_data.push(format!("评级: {}", rating));
        }
        if !cio_data.is_empty() {
            system_prompt.push_str(&format!(
                "\n\n【标的数据摘要】\n{}",
                cio_data.join("\n")
            ));
        }
        if !asset_context.data_quality.is_empty() {
            system_prompt.push_str(&format!(
                "\n\n【数据质量警告】以下字段缺失，请在置信度评估中考虑：\n{}",
                asset_context.data_quality.join("，")
            ));
        }
    }

    let governor = global_governor();
    let _permit = governor.acquire(provider).await;

    let mut messages = build_context_messages(round_outputs, symbol, macro_signal, min_cash_reserve, portfolio_summary, regime_context, asset_context);

    // Risk R1: 注入组合层面风险指标（集中度/浮盈/回撤）到 user message
    if role == CommitteeRole::Risk && round == 1 {
        if let Some(last) = messages.last_mut() {
            let risk_ctx = build_risk_metrics_context(portfolio_data, symbol, asset_context);
            if !risk_ctx.is_empty() {
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
    min_cash_reserve: f64,
    emitter: &Option<EventEmitter>,
    portfolio_summary: &str,
    regime_context: Option<&str>,
    portfolio_data: &PortfolioData,
    asset_context: &AssetContext,
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
                min_cash_reserve,
                portfolio_summary,
                regime_context,
                emitter,
                portfolio_data,
                asset_context,
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
/// Pipeline (8 steps):
/// 1. Macro (with tool-call loop) -> signal + strength
/// 2. Regime computation (quantitative: RSI-14, MA, volatility, price quantile)
/// 3. Debate rounds: Quant/R1 + Risk/R1, then Quant/R2 + Risk/R2, early convergence exit
/// 4. L4 Officer (behavioral health check: guard clause, emotion, red light)
/// 5. CIO verdict
/// 6. Post-analysis: sentinel, convergence, sanity check
/// 7. Archive (fire-and-forget)
///
/// Portfolio data is built once as a shared context block and injected into
/// Macro and subsequent roles — it is not a separate pipeline step.
///
/// When `emitter` is `Some`, events are emitted at each pipeline step boundary
/// for real-time frontend streaming via `"committee-event"` Tauri event channel.
pub(crate) async fn run_committee(
    client: &dyn InvestLlmClient,
    symbol: &str,
    config: &CommitteeConfig,
    emitter: Option<EventEmitter>,
    dry_run: bool,
    portfolio_override: Option<std::sync::Arc<PortfolioData>>,
) -> Result<CommitteeResult, String> {
    let start = std::time::Instant::now();

    let mut round_outputs: Vec<RoundOutput> = Vec::new();
    let mut total_tokens: u32 = 0;

    // Load portfolio data with current prices for injection into Macro and Risk R1.
    // In batch mode the caller pre-loads once and passes an Arc to avoid redundant
    // DB reads and API calls.
    let portfolio_data = match portfolio_override {
        Some(arc) => (*arc).clone(),
        None => PortfolioData::load_with_timeout(dry_run).await,
    };
    let portfolio_summary = build_portfolio_summary(&portfolio_data);

    // Compute effective buffer from strategy's min_cash_pct × total_assets.
    // Use the maximum min_cash_pct across all strategies (most conservative).
    // Falls back to 0.0 if no strategy is configured — Gate 3 becomes a no-op.
    let effective_buffer = {
        let strategies = crate::storage::invest::strategy::list_strategies().unwrap_or_default();
        let max_pct = strategies.iter().filter_map(|s| s.min_cash_pct).fold(0.0_f64, f64::max);
        if max_pct > 0.0 {
            portfolio_data.total_assets() * max_pct / 100.0
        } else {
            log::warn!("run_committee: no strategy with min_cash_pct configured, Gate 3 buffer = 0");
            0.0
        }
    };

    // 构建标的上下文数据（行业、估值、资金流向、评级等）
    let asset_context = {
        let asset_type = portfolio_data
            .holdings
            .iter()
            .find(|h| h.symbol == symbol)
            .and_then(|h| h.asset_type.clone())
            .unwrap_or_else(|| "stock".to_string());
        match TushareClient::from_settings() {
            Ok(client) => build_asset_context(&client, symbol, &asset_type).await,
            Err(e) => {
                log::warn!("run_committee: TushareClient init failed, using empty AssetContext: {}", e);
                AssetContext {
                    asset_type,
                    ..Default::default()
                }
            }
        }
    };

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
            run_macro_phase(client, symbol, config, &portfolio_summary, &emitter, &asset_context).await?;
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
        effective_buffer,
        &emitter,
        &portfolio_summary,
        regime_context.as_deref(),
        &portfolio_data,
        &asset_context,
    )
    .await?;

    // ── Step 4: L4 Officer phase (behavioral health check) ────────────
    {
        let l4_output = run_l4_officer_phase(
            client,
            symbol,
            config,
            &round_outputs,
            &macro_signal,
            effective_buffer,
            &portfolio_summary,
            regime_context.as_deref(),
            &emitter,
            &portfolio_data,
            &asset_context,
        )
        .await;
        total_tokens += l4_output.tokens_used;
        round_outputs.push(l4_output);
    }

    // ── Step 5: CIO verdict ────────────────────────────────────────────
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
            effective_buffer,
            &portfolio_summary,
            regime_context.as_deref(),
            &emitter,
            &portfolio_data,
            &asset_context,
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

    // ── Step 6: Post-analysis ──────────────────────────────────────────
    let sentinel = check_sentinel(&round_outputs);

    let cio_parsed = round_outputs
        .iter()
        .rev()
        .find(|o| o.role == CommitteeRole::Cio)
        .map(|o| o.parsed.clone())
        .unwrap_or_default();

    let actual_concentration = concentration_for_symbol(symbol, &portfolio_data);
    let sanity = cio_sanity_check(
        &cio_parsed,
        &round_outputs,
        &macro_signal,
        effective_buffer,
        Some(portfolio_data.cash),
        Some(actual_concentration),
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

    // ── Step 7: Archive (fire-and-forget) ──────────────────────────────
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
    // Pre-load portfolio once and share across all tasks to avoid redundant
    // DB reads and price-fetch API calls.
    let portfolio_arc = std::sync::Arc::new(PortfolioData::load_with_timeout(dry_run).await);
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_SYMBOLS));

    let mut handles = Vec::with_capacity(symbols.len());

    for symbol in symbols {
        let client = client.clone();
        let config = config.clone();
        let symbol = symbol.clone();
        let portfolio = portfolio_arc.clone();
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        handles.push(tokio::spawn(async move {
            let _permit = permit; // hold until task completes
            run_committee(&*client, &symbol, &config, None, dry_run, Some(portfolio)).await
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

    // Pre-load portfolio once and share across all tasks.
    let portfolio_arc = std::sync::Arc::new(PortfolioData::load_with_timeout(dry_run).await);
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_SYMBOLS));

    let mut handles: Vec<(String, _)> = Vec::with_capacity(symbols.len());

    for symbol in symbols {
        let client = client.clone();
        let config = config.clone();
        let symbol = symbol.clone();
        let emitter = emitter.clone();
        let portfolio = portfolio_arc.clone();
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        handles.push((symbol.clone(), tokio::spawn(async move {
            let _permit = permit; // hold until task completes
            run_committee(&*client, &symbol, &config, Some(emitter), dry_run, Some(portfolio)).await
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
