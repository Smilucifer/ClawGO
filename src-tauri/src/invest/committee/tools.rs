// Slimmed in Task C4: the OpenAiCompatClient/llm_config.json path was deleted,
// so the old `*_def` ToolDef builders, `execute_tool`, `tool_result_message`,
// `role_tool_defs`, `macro_tool_defs`, and the per-tool `exec_*` helpers are
// all gone with it. The CLI executor (`cli_executor.rs`) drives committee
// roles directly via the Claude CLI and does its own data fetching.
//
// Only `format_macro_entries` is retained — `cli_executor::format_macro_cache_for_prompt`
// still uses it to render the macro cache snapshot for the Macro role prompt.

use crate::storage::invest::macro_cache;

pub fn format_macro_entries(entries: &[macro_cache::MacroCacheEntry]) -> Result<String, String> {
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
                "sh_composite_close" => "上证指数",
                "sh_composite_vol20" => "上证指数 20日波动率",
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
                "limit_up_count" => "涨停家数",
                "limit_down_count" => "跌停家数",
                "two_market_volume" => "两市成交额(亿)",
                "advance_count" => "上涨家数",
                "decline_count" => "下跌家数",
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
