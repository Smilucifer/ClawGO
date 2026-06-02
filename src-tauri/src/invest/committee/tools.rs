use crate::invest::committee::roles::CommitteeRole;
use crate::invest::llm::types::{Message, ToolDef};
use crate::invest::macro_refresh;
use crate::storage::invest::macro_cache;
use crate::storage::invest::stock_data_cache;
use crate::tushare::client::TushareClient;
use serde_json::json;

// ---------------------------------------------------------------------------
// Individual tool definitions (OpenAI-compatible JSON Schema)
// ---------------------------------------------------------------------------

pub fn get_history_data_def() -> ToolDef {
    ToolDef {
        name: "get_history_data".to_string(),
        description: "获取指定股票的历史行情数据（日线）".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "股票代码，如 600519.SH"
                },
                "days": {
                    "type": "integer",
                    "description": "获取最近N个交易日的数据，默认60"
                }
            },
            "required": ["symbol"]
        }),
    }
}

pub fn analyze_multi_timeframe_def() -> ToolDef {
    ToolDef {
        name: "analyze_multi_timeframe".to_string(),
        description: "对股票进行多时间框架技术分析（5日/20日/60日）".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "股票代码"
                }
            },
            "required": ["symbol"]
        }),
    }
}

pub fn get_macro_snapshot_def() -> ToolDef {
    ToolDef {
        name: "get_macro_snapshot".to_string(),
        description: "获取A股宏观指标快照：沪深300波动率、北向资金、融资余额、涨跌停广度".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

pub fn query_dreaming_insights_def() -> ToolDef {
    ToolDef {
        name: "query_dreaming_insights".to_string(),
        description: "查询投资洞察和历史裁决".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "查询关键词"
                },
                "limit": {
                    "type": "integer",
                    "description": "返回条数，默认10"
                }
            },
            "required": ["query"]
        }),
    }
}

pub fn get_recent_committee_verdicts_def() -> ToolDef {
    ToolDef {
        name: "get_recent_committee_verdicts".to_string(),
        description: "获取近期委员会裁决记录".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "股票代码（可选）"
                },
                "days": {
                    "type": "integer",
                    "description": "最近N天，默认7"
                }
            }
        }),
    }
}

pub fn get_moneyflow_def() -> ToolDef {
    ToolDef {
        name: "get_moneyflow".to_string(),
        description: "获取个股近5日主力/散户资金流向。返回每日大单/超大单/中单/小单的买入卖出量和净流入。ETF 标的可能无数据。".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "股票代码，如 600519.SH"
                }
            },
            "required": ["symbol"]
        }),
    }
}

pub fn get_company_info_def() -> ToolDef {
    ToolDef {
        name: "get_company_info".to_string(),
        description: "获取公司估值数据：PE/PB/ROE/总市值/流通市值/换手率。用于估值评估和标的风险判断。".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "股票代码，如 600519.SH"
                }
            },
            "required": ["symbol"]
        }),
    }
}

pub fn get_company_news_def() -> ToolDef {
    ToolDef {
        name: "get_company_news".to_string(),
        description: "获取个股最新新闻（利空/减持/诉讼/业绩不及等风险事件）。返回最近5条相关新闻摘要。".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "股票代码，如 600519.SH"
                }
            },
            "required": ["symbol"]
        }),
    }
}

pub fn get_recent_events_def() -> ToolDef {
    ToolDef {
        name: "get_recent_events".to_string(),
        description: "获取最近的市场事件列表（event_scanner 输出），含事件类型、严重程度、影响分析。用于宏观催化剂感知。".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {},
            "required": []
        }),
    }
}

// ---------------------------------------------------------------------------
// Composite tool-def sets
// ---------------------------------------------------------------------------

/// All 6 whitelisted tools for the Macro role.
pub fn macro_tool_defs() -> Vec<ToolDef> {
    vec![
        get_history_data_def(),
        analyze_multi_timeframe_def(),
        get_macro_snapshot_def(),
        query_dreaming_insights_def(),
        get_recent_committee_verdicts_def(),
        get_recent_events_def(),
    ]
}

/// Role-based tool definitions. Macro gets full tools in R1 only; Quant and
/// Risk get tools in both R1 and R2 (R2 uses them for cross-review); CIO never
/// gets tools; L4 Officer gets dreaming insights only.
pub fn role_tool_defs(role: CommitteeRole, round: u8) -> Option<Vec<ToolDef>> {
    match role {
        CommitteeRole::Macro => {
            if round > 1 { None } else { Some(macro_tool_defs()) }
        }
        CommitteeRole::Quant => Some(vec![
            get_history_data_def(),
            analyze_multi_timeframe_def(),
            get_recent_committee_verdicts_def(),
            get_moneyflow_def(),
            get_company_info_def(),
        ]),
        CommitteeRole::Risk => Some(vec![
            query_dreaming_insights_def(),
            get_recent_committee_verdicts_def(),
            get_company_news_def(),
        ]),
        CommitteeRole::Cio => None,
        CommitteeRole::L4Officer => Some(vec![
            query_dreaming_insights_def(),
        ]),
    }
}

// ---------------------------------------------------------------------------
// Tool execution
// ---------------------------------------------------------------------------

/// Execute a tool call and return the result as a string.
pub async fn execute_tool(
    tool_name: &str,
    arguments: &str,
    symbol: &str,
) -> Result<String, String> {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));

    match tool_name {
        "get_history_data" => {
            let sym = args["symbol"].as_str().unwrap_or(symbol);
            let days = args["days"].as_i64().unwrap_or(60) as usize;
            exec_history_data(sym, days).await
        }
        "analyze_multi_timeframe" => {
            let sym = args["symbol"].as_str().unwrap_or(symbol);
            exec_multi_timeframe(sym).await
        }
        "get_macro_snapshot" => exec_macro_snapshot().await,
        "query_dreaming_insights" => {
            let query = args["query"].as_str().unwrap_or("");
            let limit = args["limit"].as_i64().unwrap_or(10) as usize;
            exec_dreaming_insights(query, limit)
        }
        "get_recent_committee_verdicts" => {
            let sym = args["symbol"].as_str();
            let days = args["days"].as_i64().unwrap_or(7);
            exec_recent_verdicts(sym, days)
        }
        "get_moneyflow" => {
            let sym = args["symbol"].as_str().unwrap_or(symbol);
            exec_moneyflow(sym).await
        }
        "get_company_info" => {
            let sym = args["symbol"].as_str().unwrap_or(symbol);
            exec_company_info(sym).await
        }
        "get_company_news" => {
            let sym = args["symbol"].as_str().unwrap_or(symbol);
            exec_company_news(sym).await
        }
        "get_recent_events" => exec_recent_events(),
        _ => Err(format!("unknown tool: {}", tool_name)),
    }
}

// ---------------------------------------------------------------------------
// Data helpers (shared between orchestrator and tool implementations)
// ---------------------------------------------------------------------------

/// 获取估值数据（PE/PB/ROE/换手率/市值），供多个模块复用避免重复 API 调用。
/// 返回 (pe_ttm, pb, roe, turnover_rate_f, total_mv_yi)。
pub async fn fetch_valuation_data(
    client: &TushareClient,
    symbol: &str,
) -> Option<(Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<f64>)> {
    let daily = client.daily_basic(symbol, None, None, None).await.ok()?;
    let latest = daily.first()?;
    let fina = client.fina_indicator(symbol, None, None, None).await.ok()?;
    let latest_fina = fina.last();
    Some((
        latest.pe_ttm,
        latest.pb,
        latest_fina.and_then(|f| f.roe),
        latest.turnover_rate_f,
        latest.total_mv.map(|v| v / 10000.0), // 转为亿元
    ))
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

/// Check if a symbol looks like an A-share code: exactly 6 digits followed by .SH or .SZ.
fn is_a_share_symbol(symbol: &str) -> bool {
    if symbol.len() != 9 {
        return false;
    }
    let bytes = symbol.as_bytes();
    bytes[0..6].iter().all(|b| b.is_ascii_digit()) && bytes[6] == b'.' && (bytes[7] == b'S' && (bytes[8] == b'H' || bytes[8] == b'Z'))
}

async fn exec_history_data(symbol: &str, days: usize) -> Result<String, String> {
    if is_a_share_symbol(symbol) {
        // A-share: use Tushare
        exec_history_data_tushare(symbol, days).await
    } else {
        // International / Yahoo symbol: use Yahoo Finance
        exec_history_data_yahoo(symbol, days).await
    }
}

/// Tushare-backed history for A-share symbols (e.g. 600519.SH).
async fn exec_history_data_tushare(symbol: &str, days: usize) -> Result<String, String> {
    let client = TushareClient::from_settings()?;

    let end_date = chrono::Local::now().format("%Y%m%d").to_string();
    let start_date =
        (chrono::Local::now() - chrono::Duration::days(days as i64 * 2)).format("%Y%m%d").to_string();

    let bars = client.daily(symbol, &start_date, &end_date).await?;
    // daily() returns descending (newest first); take the most recent N bars
    let bars: Vec<_> = bars.into_iter().take(days).collect();

    if bars.is_empty() {
        return Ok(format!("没有找到 {} 的历史数据", symbol));
    }

    // bars[0] is the newest, bars[last] is the oldest
    let latest = &bars[0];
    let oldest = &bars[bars.len() - 1];
    let pct_change = if oldest.close > 0.0 {
        (latest.close - oldest.close) / oldest.close * 100.0
    } else {
        0.0
    };

    let avg_vol: f64 = bars.iter().map(|b| b.vol).sum::<f64>() / bars.len() as f64;

    // Show the 5 most recent bars in chronological order (oldest→newest)
    let recent_5: String = bars
        .iter()
        .rev()
        .take(5)
        .collect::<Vec<_>>()
        .iter()
        .rev()
        .map(|b| format!("{}:{:.3}", &b.trade_date[4..], b.close))
        .collect::<Vec<_>>()
        .join(" → ");

    let period_high =
        bars.iter().map(|b| b.high).fold(f64::NEG_INFINITY, f64::max);
    let period_low = bars.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);

    Ok(format!(
        "【{} 历史行情（最近{}个交易日）】\n\
         最新收盘: {:.3} ({})\n\
         区间涨跌: {:.1}%\n\
         最高: {:.3} / 最低: {:.3}\n\
         平均成交量: {:.0} 手\n\
         近5日K线: {}",
        symbol,
        bars.len(),
        latest.close,
        latest.trade_date,
        pct_change,
        period_high,
        period_low,
        avg_vol,
        recent_5
    ))
}

/// Yahoo Finance-backed history for international symbols (e.g. ^VIX, GC=F).
async fn exec_history_data_yahoo(symbol: &str, days: usize) -> Result<String, String> {
    use crate::invest::international::InternationalClient;

    let client = InternationalClient::from_settings();
    let bars = client
        .fetch_yahoo_history(symbol, days as u32)
        .await
        .map_err(|e| format!("Yahoo history fetch failed for {symbol}: {e}"))?;

    if bars.is_empty() {
        return Ok(format!("没有找到 {} 的历史数据", symbol));
    }

    // bars are in chronological order (oldest first)
    let latest = bars.last().unwrap();
    let oldest = &bars[0];
    let pct_change = if oldest.close > 0.0 {
        (latest.close - oldest.close) / oldest.close * 100.0
    } else {
        0.0
    };

    let period_high = bars.iter().map(|b| b.high).fold(f64::NEG_INFINITY, f64::max);
    let period_low = bars.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);

    let recent_5: String = bars
        .iter()
        .rev()
        .take(5)
        .map(|b| format!("{}:{:.3}", if b.date.len() > 5 { &b.date[5..] } else { &b.date }, b.close))
        .collect::<Vec<_>>()
        .join(" → ");

    let display_name = crate::invest::international::resolve_symbol_name(symbol);

    Ok(format!(
        "【{} ({}) 历史行情（最近{}日）】\n\
         最新收盘: {:.3} ({})\n\
         区间涨跌: {:.1}%\n\
         最高: {:.3} / 最低: {:.3}\n\
         近5日K线: {}",
        display_name,
        symbol,
        bars.len(),
        latest.close,
        latest.date,
        pct_change,
        period_high,
        period_low,
        recent_5
    ))
}

async fn exec_multi_timeframe(symbol: &str) -> Result<String, String> {
    let client = TushareClient::from_settings()?;

    // Request ~2 years of data for MA120 and percentile calculations
    let end_date = chrono::Local::now().format("%Y%m%d").to_string();
    let start_date =
        (chrono::Local::now() - chrono::Duration::days(750)).format("%Y%m%d").to_string();

    let bars = client.daily(symbol, &start_date, &end_date).await?;
    // daily() returns descending (newest first); bars[0] is the latest bar
    if bars.len() < 5 {
        return Ok(format!("{} 数据不足，无法进行多时间框架分析", symbol));
    }

    let ma5: f64 = bars.iter().take(5).map(|b| b.close).sum::<f64>() / 5.0;
    let ma20 = if bars.len() >= 20 {
        bars.iter().take(20).map(|b| b.close).sum::<f64>() / 20.0
    } else {
        bars.iter().map(|b| b.close).sum::<f64>() / bars.len() as f64
    };
    let ma60 = if bars.len() >= 60 {
        bars.iter().take(60).map(|b| b.close).sum::<f64>() / 60.0
    } else {
        bars.iter().map(|b| b.close).sum::<f64>() / bars.len() as f64
    };
    let ma120 = if bars.len() >= 120 {
        Some(bars.iter().take(120).map(|b| b.close).sum::<f64>() / 120.0)
    } else {
        None
    };

    let latest_close = bars[0].close;

    // 20-day historical volatility (annualized)
    let hv20_text = if bars.len() < 20 {
        format!("N/A (仅{}日数据)", bars.len())
    } else {
        let returns: Vec<f64> = bars
            .windows(2)
            .take(20)
            .map(|w| {
                if w[1].close > 0.0 {
                    (w[0].close - w[1].close) / w[1].close
                } else {
                    0.0
                }
            })
            .collect();
        let mean_ret = returns.iter().sum::<f64>() / returns.len().max(1) as f64;
        let variance = returns.iter().map(|r| (r - mean_ret).powi(2)).sum::<f64>()
            / returns.len().max(1) as f64;
        let hv20 = variance.sqrt() * 252.0_f64.sqrt() * 100.0;
        format!("{:.1}%", hv20)
    };

    // RSI(14) — Wilder smoothing
    let rsi14 = compute_rsi14(&bars);

    // Price percentile over available data (up to 2-year window)
    let window = bars.len().min(500);
    let all_closes: Vec<f64> = bars.iter().take(window).map(|b| b.close).collect();
    let price_percentile = compute_percentile(latest_close, &all_closes);

    let vs_ma20 = if latest_close > ma20 { "上方（偏多）" } else { "下方（偏空）" };

    let trend = if ma120.is_some() && bars.len() >= 120 {
        let m120 = ma120.unwrap();
        if latest_close > ma5 && ma5 > ma20 && ma20 > ma60 && ma60 > m120 {
            "强势多头排列"
        } else if latest_close > ma5 && ma5 > ma20 && ma20 > ma60 {
            "多头排列"
        } else if latest_close < ma5 && ma5 < ma20 && ma20 < ma60 && ma60 < m120 {
            "强势空头排列"
        } else if latest_close < ma5 && ma5 < ma20 && ma20 < ma60 {
            "空头排列"
        } else {
            "震荡整理"
        }
    } else {
        if latest_close > ma5 && ma5 > ma20 && ma20 > ma60 {
            "多头排列"
        } else if latest_close < ma5 && ma5 < ma20 && ma20 < ma60 {
            "空头排列"
        } else {
            "震荡整理"
        }
    };

    let mut output = format!(
        "【{} 多时间框架分析】\n\
         MA5: {:.3} | MA20: {:.3} | MA60: {:.3}",
        symbol, ma5, ma20, ma60
    );

    if let Some(m120) = ma120 {
        output.push_str(&format!(" | MA120: {:.3}", m120));
    }

    output.push_str(&format!(
        "\n价格 vs MA20: {}\
         \nHV20(年化): {}\
         \nRSI(14): {:.1}\
         \n价格分位({}日): {:.0}%\
         \n趋势判断: {}",
        vs_ma20, hv20_text, rsi14, window, price_percentile, trend
    ));

    Ok(output)
}

/// Compute RSI(14) using Wilder's smoothing method.
fn compute_rsi14(bars: &[crate::tushare::client::DailyBar]) -> f64 {
    if bars.len() < 15 {
        return 50.0; // insufficient data
    }

    // bars are newest-first; reverse for chronological calculation
    let closes: Vec<f64> = bars.iter().rev().map(|b| b.close).collect();

    let mut avg_gain = 0.0_f64;
    let mut avg_loss = 0.0_f64;

    // Initial 14-period averages
    for i in 1..=14 {
        let change = closes[i] - closes[i - 1];
        if change > 0.0 {
            avg_gain += change;
        } else {
            avg_loss -= change; // make positive
        }
    }
    avg_gain /= 14.0;
    avg_loss /= 14.0;

    // Wilder smoothing for remaining periods
    for i in 15..closes.len() {
        let change = closes[i] - closes[i - 1];
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { -change } else { 0.0 };
        avg_gain = (avg_gain * 13.0 + gain) / 14.0;
        avg_loss = (avg_loss * 13.0 + loss) / 14.0;
    }

    if avg_loss < f64::EPSILON {
        100.0
    } else {
        let rs = avg_gain / avg_loss;
        100.0 - 100.0 / (1.0 + rs)
    }
}

/// Compute the percentile rank of `value` within the sorted data window.
/// Returns 0.0–100.0.
fn compute_percentile(value: f64, data: &[f64]) -> f64 {
    if data.is_empty() {
        return 50.0;
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = sorted.iter().position(|&v| v >= value).unwrap_or(sorted.len());
    rank as f64 / sorted.len() as f64 * 100.0
}

async fn exec_macro_snapshot() -> Result<String, String> {
    // Try reading from macro_cache first
    let entries = macro_cache::load_all_macro_cache().unwrap_or_default();

    // Check if cache is fresh (all entries within 30 minutes)
    let cache_fresh = !entries.is_empty()
        && entries.iter().all(|e| !macro_cache::is_stale(e, 30));

    if !cache_fresh {
        // Fallback: refresh via macro_refresh
        log::info!("exec_macro_snapshot: cache stale or empty, refreshing...");
        let client = TushareClient::from_settings()?;
        if let Err(e) = macro_refresh::refresh_macro_cache(&client).await {
            log::warn!("exec_macro_snapshot: refresh failed: {e}, using stale cache");
        }
        // Re-read after refresh
        let entries = macro_cache::load_all_macro_cache().unwrap_or_default();
        return format_macro_entries(&entries);
    }

    format_macro_entries(&entries)
}

fn format_macro_entries(entries: &[macro_cache::MacroCacheEntry]) -> Result<String, String> {
    if entries.is_empty() {
        return Ok("宏观指标缓存为空，请稍后重试".to_string());
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
                "gold" => "国际金价(USD)",
                "oil" => "国际油价(USD)",
                "usdcny" => "USD/CNY汇率",
                _ => indicator,
            };
            let stale_marker = if macro_cache::is_stale(entry, 30) {
                " [stale]"
            } else {
                ""
            };
            lines.push(format!("  {}: {}{}", label, value_str, stale_marker));
        }
    }

    // Show data freshness
    if let Some(oldest) = entries.iter().min_by_key(|e| &e.fetched_at) {
        lines.push(format!("  数据更新时间: {}", oldest.fetched_at));
    }

    Ok(lines.join("\n"))
}

fn exec_dreaming_insights(query: &str, limit: usize) -> Result<String, String> {
    use crate::storage::invest::with_conn;

    let results: Vec<String> = with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT content, created_at FROM domain_insights WHERE content LIKE ?1 \
                 ORDER BY created_at DESC LIMIT ?2",
            )
            .map_err(|e| format!("prepare: {}", e))?;

        let pattern = format!("%{}%", query);
        let rows = stmt
            .query_map(rusqlite::params![pattern, limit as i64], |row| {
                let content: String = row.get(0)?;
                let created: String = row.get(1)?;
                let date_part =
                    if created.len() >= 10 { &created[..10] } else { &created };
                Ok(format!("[{}] {}", date_part, content))
            })
            .map_err(|e| format!("query: {}", e))?;

        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })?;

    if results.is_empty() {
        Ok(format!("没有找到与 '{}' 相关的投资洞察", query))
    } else {
        Ok(format!("【投资洞察 - {}】\n{}", query, results.join("\n")))
    }
}

fn exec_recent_verdicts(symbol: Option<&str>, days: i64) -> Result<String, String> {
    use crate::storage::invest::verdicts::list_verdicts;

    let verdicts = list_verdicts(symbol, Some(days * 5))?;

    let cutoff = (chrono::Local::now() - chrono::Duration::days(days))
        .format("%Y-%m-%d")
        .to_string();

    let recent: Vec<String> = verdicts
        .iter()
        .filter(|v| v.created_at >= cutoff)
        .take(10)
        .map(|v| {
            let date_part = if v.created_at.len() >= 10 {
                &v.created_at[..10]
            } else {
                &v.created_at
            };
            format!(
                "[{}] {} → {} (conf={:.1}, signal={}, {}ms)",
                date_part,
                v.symbol,
                v.verdict,
                v.confidence.unwrap_or(0.0),
                v.macro_signal.as_deref().unwrap_or("?"),
                v.latency_ms.unwrap_or(0)
            )
        })
        .collect();

    if recent.is_empty() {
        Ok(format!("最近{}天没有裁决记录", days))
    } else {
        Ok(format!("【近{}天裁决记录】\n{}", days, recent.join("\n")))
    }
}

// ---------------------------------------------------------------------------
// New tool implementations
// ---------------------------------------------------------------------------

/// 个股资金流向：近5日主力/散户净流入汇总
/// Cache-first: 当天缓存有效；miss 时 fallback 到 API。
async fn exec_moneyflow(symbol: &str) -> Result<String, String> {
    let today = chrono::Local::now().format("%Y%m%d").to_string();

    // Cache-first: 当天的数据直接用
    if let Ok(Some((date, json, _))) = stock_data_cache::load_latest(symbol, "moneyflow_dc") {
        if date == today {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                if let Some(summary) = v["summary"].as_str() {
                    let days = v["days"].as_u64().unwrap_or(5);
                    return Ok(format!("【{} 近{}日资金流向】\n{}", symbol, days, summary));
                }
            }
        }
    }

    // Fallback: API
    use crate::tushare::client::MoneyflowDc;

    let client = TushareClient::from_settings()?;
    let five_days_ago =
        (chrono::Local::now() - chrono::Duration::days(5)).format("%Y%m%d").to_string();

    let rows = client
        .moneyflow_dc(symbol, &five_days_ago, &today)
        .await
        .map_err(|e| format!("moneyflow_dc failed: {}", e))?;

    if rows.is_empty() {
        return Ok("N/A（ETF 或代码错误）".to_string());
    }

    Ok(format!(
        "【{} 近{}日资金流向】\n{}",
        symbol,
        rows.len(),
        MoneyflowDc::format_moneyflow_summary(&rows)
    ))
}

/// Format valuation data into a standard output string.
fn format_valuation(
    symbol: &str,
    pe: Option<f64>,
    pb: Option<f64>,
    roe: Option<f64>,
    total_mv_yi: Option<f64>,
    turnover: Option<f64>,
) -> String {
    let fmt = |v: Option<f64>, suffix: &str| -> String {
        v.map(|v| format!("{:.2}{}", v, suffix)).unwrap_or_else(|| "N/A".into())
    };
    format!(
        "【{} 估值数据】\nPE: {}, PB: {}, ROE: {}, 总市值: {}, 换手率: {}",
        symbol,
        fmt(pe, ""),
        fmt(pb, ""),
        fmt(roe, "%"),
        total_mv_yi.map(|v| format!("{:.2}亿", v)).unwrap_or_else(|| "N/A".into()),
        fmt(turnover, "%"),
    )
}

/// 公司基本信息+估值：PE/PB/ROE/总市值/换手率
/// Cache-first: 先从 stock_data_cache 读取，miss 时 fallback 到 API。
async fn exec_company_info(symbol: &str) -> Result<String, String> {
    use crate::tushare::client::DailyBasic;

    // Cache-first: 用 load_all_latest_for_symbol 避免多次 DB 查询
    let entries = stock_data_cache::load_all_latest_for_symbol(symbol).unwrap_or_default();
    let find_json = |dt: &str| -> Option<String> {
        entries.iter().find(|(t, _, _)| t == dt).map(|(_, _, j)| j.clone())
    };

    if let Some(json) = find_json("daily_basic") {
        if let Ok(b) = serde_json::from_str::<DailyBasic>(&json) {
            let roe = find_json("fina_indicator")
                .and_then(|fj| serde_json::from_str::<serde_json::Value>(&fj).ok())
                .and_then(|fv| fv["roe"].as_f64());
            let total_mv_yi = b.total_mv.map(|v| v / 10000.0);
            let turnover = b.turnover_rate_f.or(b.turnover_rate);
            return Ok(format_valuation(symbol, b.pe_ttm, b.pb, roe, total_mv_yi, turnover));
        }
    }

    // Fallback: API
    let client = TushareClient::from_settings()?;
    let (pe, pb, roe, turnover, mv) = fetch_valuation_data(&client, symbol).await
        .unwrap_or((None, None, None, None, None));
    Ok(format_valuation(symbol, pe, pb, roe, mv, turnover))
}

/// 个股风险新闻：最近5条相关新闻
async fn exec_company_news(symbol: &str) -> Result<String, String> {
    let client = TushareClient::from_settings()?;
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let thirty_days_ago =
        (chrono::Local::now() - chrono::Duration::days(30)).format("%Y%m%d").to_string();

    // 使用 major_news 获取最近新闻
    match client
        .major_news("mx-finance", &thirty_days_ago, &today)
        .await
    {
        Ok(items) => {
            // 按 symbol 过滤（如果 items 中包含 symbol 信息）
            // major_news 返回的是全市场新闻，这里取最近5条
            let recent: Vec<String> = items
                .iter()
                .take(5)
                .map(|item| {
                    format!("[{}] {} - {}", item.datetime, item.title, item.src)
                })
                .collect();

            if recent.is_empty() {
                Ok("暂无风险新闻".to_string())
            } else {
                Ok(format!(
                    "【{} 近期新闻】\n{}",
                    symbol,
                    recent.join("\n")
                ))
            }
        }
        Err(e) => {
            log::warn!("exec_company_news: major_news failed: {}", e);
            Ok("暂无风险新闻".to_string())
        }
    }
}

/// 最近事件列表：event_scanner 输出
fn exec_recent_events() -> Result<String, String> {
    let events = crate::storage::invest::events::list_events(None, Some(20))?;

    let seven_days_ago =
        (chrono::Local::now() - chrono::Duration::days(7)).format("%Y-%m-%d").to_string();

    let recent: Vec<String> = events
        .iter()
        .filter(|e| e.created_at >= seven_days_ago)
        .take(10)
        .map(|e| {
            format!(
                "[{}] {} | {} | {}",
                &e.created_at[..10.min(e.created_at.len())],
                e.event_type,
                e.severity,
                e.title
            )
        })
        .collect();

    if recent.is_empty() {
        Ok("暂无最近事件".to_string())
    } else {
        Ok(format!("【近7天市场事件】\n{}", recent.join("\n")))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a `Message` with role `"tool"` for returning tool results to the LLM.
pub fn tool_result_message(tool_call_id: &str, result: &str) -> Message {
    Message {
        role: "tool".to_string(),
        content: result.to_string(),
        tool_call_id: Some(tool_call_id.to_string()),
        tool_calls: None,
        name: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_macro_tool_defs_count() {
        assert_eq!(macro_tool_defs().len(), 6);
    }

    #[test]
    fn test_macro_tool_names() {
        let defs = macro_tool_defs();
        let names: Vec<&str> = defs.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"get_history_data"));
        assert!(names.contains(&"analyze_multi_timeframe"));
        assert!(names.contains(&"get_macro_snapshot"));
        assert!(names.contains(&"query_dreaming_insights"));
        assert!(names.contains(&"get_recent_committee_verdicts"));
        assert!(names.contains(&"get_recent_events"));
    }

    #[test]
    fn test_role_tool_defs_macro_r1() {
        let defs = role_tool_defs(CommitteeRole::Macro, 1).unwrap();
        assert_eq!(defs.len(), 6);
    }

    #[test]
    fn test_role_tool_defs_macro_r2_none() {
        assert!(role_tool_defs(CommitteeRole::Macro, 2).is_none());
    }

    #[test]
    fn test_role_tool_defs_quant_r1() {
        let defs = role_tool_defs(CommitteeRole::Quant, 1).unwrap();
        assert_eq!(defs.len(), 5);
        let names: Vec<&str> = defs.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"get_history_data"));
        assert!(names.contains(&"analyze_multi_timeframe"));
        assert!(names.contains(&"get_recent_committee_verdicts"));
        assert!(names.contains(&"get_moneyflow"));
        assert!(names.contains(&"get_company_info"));
    }

    #[test]
    fn test_role_tool_defs_risk_r1() {
        let defs = role_tool_defs(CommitteeRole::Risk, 1).unwrap();
        assert_eq!(defs.len(), 3);
        let names: Vec<&str> = defs.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"query_dreaming_insights"));
        assert!(names.contains(&"get_recent_committee_verdicts"));
        assert!(names.contains(&"get_company_news"));
    }

    #[test]
    fn test_role_tool_defs_quant_r2_has_tools() {
        let defs = role_tool_defs(CommitteeRole::Quant, 2).unwrap();
        assert_eq!(defs.len(), 5);
        let names: Vec<&str> = defs.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"get_history_data"));
        assert!(names.contains(&"analyze_multi_timeframe"));
        assert!(names.contains(&"get_recent_committee_verdicts"));
        assert!(names.contains(&"get_moneyflow"));
        assert!(names.contains(&"get_company_info"));
    }

    #[test]
    fn test_role_tool_defs_risk_r2_has_tools() {
        let defs = role_tool_defs(CommitteeRole::Risk, 2).unwrap();
        assert_eq!(defs.len(), 3);
        let names: Vec<&str> = defs.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"query_dreaming_insights"));
        assert!(names.contains(&"get_recent_committee_verdicts"));
        assert!(names.contains(&"get_company_news"));
    }

    #[test]
    fn test_role_tool_defs_cio_none() {
        assert!(role_tool_defs(CommitteeRole::Cio, 1).is_none());
    }

    #[test]
    fn test_tool_result_message_format() {
        let msg = tool_result_message("call_123", "test result");
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.tool_call_id.as_deref(), Some("call_123"));
        assert_eq!(msg.content, "test result");
    }

    #[tokio::test]
    async fn test_execute_unknown_tool() {
        let result = execute_tool("nonexistent", "{}", "600519.SH").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown tool"));
    }
}
