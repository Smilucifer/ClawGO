use crate::invest::llm::types::{Message, ToolDef};
use crate::tushare::client::TushareClient;
use serde_json::json;

// ---------------------------------------------------------------------------
// Tool definitions (OpenAI-compatible JSON Schema)
// ---------------------------------------------------------------------------

/// All 5 whitelisted tools for the Macro role.
pub fn macro_tool_defs() -> Vec<ToolDef> {
    vec![
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
        },
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
        },
        ToolDef {
            name: "get_macro_snapshot".to_string(),
            description: "获取A股宏观指标快照：沪深300波动率、北向资金、融资余额、涨跌停广度".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        },
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
        },
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
        },
    ]
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
        _ => Err(format!("unknown tool: {}", tool_name)),
    }
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

async fn exec_history_data(symbol: &str, days: usize) -> Result<String, String> {
    let token = read_tushare_token()?;
    let client = TushareClient::new(token);

    let end_date = chrono::Local::now().format("%Y%m%d").to_string();
    let start_date =
        (chrono::Local::now() - chrono::Duration::days(days as i64 * 2)).format("%Y%m%d").to_string();

    let bars = client.daily(symbol, &start_date, &end_date).await?;
    let bars: Vec<_> = bars.into_iter().rev().take(days).collect();

    if bars.is_empty() {
        return Ok(format!("没有找到 {} 的历史数据", symbol));
    }

    let latest = &bars[0];
    let oldest = &bars[bars.len() - 1];
    let pct_change = if oldest.close > 0.0 {
        (latest.close - oldest.close) / oldest.close * 100.0
    } else {
        0.0
    };

    let avg_vol: f64 = bars.iter().map(|b| b.vol).sum::<f64>() / bars.len() as f64;

    let recent_5: String = bars
        .iter()
        .take(5)
        .map(|b| format!("{}:{:.2}", &b.trade_date[4..], b.close))
        .collect::<Vec<_>>()
        .join(" → ");

    let period_high =
        bars.iter().map(|b| b.high).fold(f64::NEG_INFINITY, f64::max);
    let period_low = bars.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);

    Ok(format!(
        "【{} 历史行情（最近{}个交易日）】\n\
         最新收盘: {:.2} ({})\n\
         区间涨跌: {:.2}%\n\
         最高: {:.2} / 最低: {:.2}\n\
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

async fn exec_multi_timeframe(symbol: &str) -> Result<String, String> {
    let token = read_tushare_token()?;
    let client = TushareClient::new(token);

    let end_date = chrono::Local::now().format("%Y%m%d").to_string();
    let start_date =
        (chrono::Local::now() - chrono::Duration::days(180)).format("%Y%m%d").to_string();

    let bars = client.daily(symbol, &start_date, &end_date).await?;
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

    let latest_close = bars[0].close;

    // 20-day historical volatility (annualized)
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
    let variance =
        returns.iter().map(|r| (r - mean_ret).powi(2)).sum::<f64>() / returns.len().max(1) as f64;
    let hv20 = variance.sqrt() * 252.0_f64.sqrt() * 100.0;

    let vs_ma20 = if latest_close > ma20 { "上方（偏多）" } else { "下方（偏空）" };

    let trend = if latest_close > ma5 && ma5 > ma20 && ma20 > ma60 {
        "多头排列"
    } else if latest_close < ma5 && ma5 < ma20 && ma20 < ma60 {
        "空头排列"
    } else {
        "震荡整理"
    };

    Ok(format!(
        "【{} 多时间框架分析】\n\
         MA5: {:.2} | MA20: {:.2} | MA60: {:.2}\n\
         价格 vs MA20: {}\n\
         HV20(年化): {:.1}%\n\
         趋势判断: {}",
        symbol, ma5, ma20, ma60, vs_ma20, hv20, trend
    ))
}

async fn exec_macro_snapshot() -> Result<String, String> {
    let token = read_tushare_token()?;
    let client = TushareClient::new(token);

    let end_date = chrono::Local::now().format("%Y%m%d").to_string();
    let start_date =
        (chrono::Local::now() - chrono::Duration::days(90)).format("%Y%m%d").to_string();

    let idx_bars = client
        .daily("000300.SH", &start_date, &end_date)
        .await
        .unwrap_or_default();

    let csi300_latest = idx_bars.first().map(|b| b.close).unwrap_or(0.0);
    let csi300_pct = idx_bars.first().map(|b| b.pct_chg).unwrap_or(0.0);

    // 60-day percentile rank
    let csi300_60d: Vec<f64> = idx_bars.iter().take(60).map(|b| b.close).collect();
    let csi300_pctile = if !csi300_60d.is_empty() {
        let mut sorted = csi300_60d.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let rank = sorted.iter().position(|&v| v >= csi300_latest).unwrap_or(0);
        rank as f64 / sorted.len() as f64 * 100.0
    } else {
        50.0
    };

    Ok(format!(
        "【A股宏观指标快照】\n\
         沪深300: {:.2} (今日 {:.2}%)\n\
         沪深300 60日分位: {:.0}%\n\
         北向资金: 需通过 moneyflow_hsgt 接口获取\n\
         融资余额: 需通过 margin 接口获取\n\
         涨跌停广度: 需通过 limit_list_d 接口获取",
        csi300_latest, csi300_pct, csi300_pctile
    ))
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
// Helpers
// ---------------------------------------------------------------------------

fn read_tushare_token() -> Result<String, String> {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let settings_path = home.join(".claw-go").join("settings.json");

    if !settings_path.exists() {
        return Err("ClawGO settings not found".to_string());
    }

    let content =
        std::fs::read_to_string(&settings_path).map_err(|e| format!("read settings: {}", e))?;
    let settings: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("parse settings: {}", e))?;

    settings["tushare_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "tushare_token not found in settings".to_string())
}

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
        assert_eq!(macro_tool_defs().len(), 5);
    }

    #[test]
    fn test_macro_tool_names() {
        let names: Vec<&str> = macro_tool_defs().iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"get_history_data"));
        assert!(names.contains(&"analyze_multi_timeframe"));
        assert!(names.contains(&"get_macro_snapshot"));
        assert!(names.contains(&"query_dreaming_insights"));
        assert!(names.contains(&"get_recent_committee_verdicts"));
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
