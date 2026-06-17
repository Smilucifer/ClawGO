//! CLI-based committee role executor.
//!
//! Spawns `claude --print` with pre-fetched cache data embedded in the
//! system prompt. Returns the CLI's stdout as the role output text.
//! Falls back gracefully if the CLI binary is not found.

use std::sync::OnceLock;
use std::time::Duration;
use tokio::process::Command;

/// Maximum concurrent CLI processes (limits memory usage: ~100MB each).
const MAX_CLI_CONCURRENT: usize = 5;

/// Default timeout for a single CLI role call (seconds).
const CLI_ROLE_TIMEOUT_SECS: u64 = 180;

/// Global CLI executor singleton.
static CLI_EXECUTOR: OnceLock<Option<CliCommitteeExecutor>> = OnceLock::new();

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
        CLI_EXECUTOR.get_or_init(Self::try_new).as_ref()
    }

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
            user_message,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

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
            date, ev.event_type, ev.severity, ev.title
        ));
    }
    lines.join("\n")
}

/// Format recent verdicts for a symbol (last 7 days, max 10).
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
/// Unlike the tool-based approach (where the LLM calls get_macro_snapshot etc.),
/// this function pre-fetches all cache data and embeds it directly in the prompt.
/// The LLM only needs to analyze, not fetch.
pub fn build_cli_macro_prompt(
    asset_name: &str,
    asset_symbol: &str,
    asset_context: &crate::invest::committee::orchestrator::AssetContext,
) -> String {
    use crate::invest::committee::roles::length_constraint_suffix;
    use crate::invest::committee::roles::CommitteeRole;

    let role = CommitteeRole::Macro;

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

    format!("{}{}", role_instruction, length_constraint_suffix(role))
}

fn fmt_opt(v: Option<f64>, decimals: usize) -> String {
    v.map(|v| format!("{:.1$}", v, decimals))
        .unwrap_or_else(|| "N/A".to_string())
}
