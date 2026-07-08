//! 盘前报告生成器：采集 → 内联归一化 → 打分 → AI 点评 → 组装 md+json 存盘。
//!
//! 尽力而为：任一数据源失败标注"数据缺失"不中断。写盘路径与 `daily_report::generate_daily_report`
//! 保持一致：`{data_dir}/invest/reports/premarket_{date}.md(+.json)`。
//!
//! Task 6：四因子真实计算 + 结构化 AI 点评。
//! - 舆论/催化：`storage::invest::sentiment::list_sentiment_by_symbol`
//! - 资金：`tushare::moneyflow_dc`（主力净流入）+ `moneyflow_hsgt`（北向）
//! - 技术：`invest::regime::compute_regime_for_symbol`（RSI/MA/趋势）
//! - AI 点评：`invest::event_analyzer::cli_complete` 输出结构化 JSON
//!
//! 补丁 M3：grade **只**来自 `scoring::score()`；AI 输出仅进入 aiCommentary 字段，绝不参与打分。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::invest::premarket::scoring::{
    get_premarket_config, score, FactorBreakdown, PremarketConfig, SymbolScore,
};
use crate::storage::invest::macro_cache::{build_macro_snapshot, MacroSnapshot};
use crate::storage::invest::portfolio::{self, Holding, HoldingKind};

/// 单标的输入：`(symbol, name)`。默认从 holdings（Hold + Watch）聚合。
fn collect_pool() -> Vec<(String, String)> {
    let holdings: Vec<Holding> = portfolio::list_holdings().unwrap_or_default();
    holdings
        .into_iter()
        .filter(|h| matches!(h.kind, HoldingKind::Hold | HoldingKind::Watch))
        .map(|h| {
            let name = h.name.clone().unwrap_or_else(|| h.symbol.clone());
            (h.symbol, name)
        })
        .collect()
}

/// 用真实 `MacroSnapshot` 字段渲染成中文 md 片段。缺失字段写"—"。
fn render_macro_md(snap: &MacroSnapshot) -> String {
    fn fmt_opt(v: Option<f64>) -> String {
        v.map(|x| format!("{x:.2}")).unwrap_or_else(|| "—".to_string())
    }

    let mut md = String::new();
    md.push_str(&format!(
        "- 上证指数：{}（20 日波动率 {}%）\n",
        fmt_opt(snap.sh_composite_close),
        fmt_opt(snap.sh_composite_vol20)
    ));
    md.push_str(&format!(
        "- 两市成交额：{} 亿\n",
        fmt_opt(snap.two_market_volume)
    ));
    md.push_str(&format!(
        "- 北向资金：{} 亿\n",
        fmt_opt(snap.northbound_net)
    ));
    md.push_str(&format!(
        "- 涨/跌家数：{} / {}\n",
        fmt_opt(snap.advance_count),
        fmt_opt(snap.decline_count)
    ));
    md.push_str(&format!(
        "- 涨停 / 跌停 / 涨幅>3% / 平盘：{} / {} / {} / {}\n",
        fmt_opt(snap.limit_up_count),
        fmt_opt(snap.limit_down_count),
        fmt_opt(snap.up_over_3pct_count),
        fmt_opt(snap.flat_count)
    ));
    md.push_str(&format!(
        "- VIX：{}  |  国际金价：{}\n",
        fmt_opt(snap.vix),
        fmt_opt(snap.gold)
    ));
    md
}

fn render_scores_md(scores: &[SymbolScore]) -> String {
    if scores.is_empty() {
        return "（观察池为空）\n".to_string();
    }
    let mut md = String::new();
    md.push_str("| 标的 | 名称 | 总分 | 评级 | 情绪 | 资金 | 技术 | 催化 | 缺失 |\n");
    md.push_str("|------|------|------|------|------|------|------|------|------|\n");
    for s in scores {
        let missing = if s.missing_factors.is_empty() {
            "—".to_string()
        } else {
            s.missing_factors.join(",")
        };
        md.push_str(&format!(
            "| {} | {} | {:.2} | {:?} | {:.0} | {:.0} | {:.0} | {:.0} | {} |\n",
            s.symbol,
            s.name,
            s.total,
            s.grade,
            s.factors.sentiment,
            s.factors.capital,
            s.factors.technical,
            s.factors.catalyst,
            missing,
        ));
    }
    md
}

// ---------------------------------------------------------------------------
// 四因子真实计算
// ---------------------------------------------------------------------------

/// 舆论 + 催化因子（同一批 sentiment_items 输入，一次查询）。
///
/// - **舆论**：sentiment_hint 均值（-1..1 → 0-100）。
/// - **催化**：3 日内关联条数 × 10，上限 100（10 条即封顶，避免噪声堆积）。
fn compute_sentiment_and_catalyst(code: &str) -> (Option<f64>, Option<f64>) {
    let since = (chrono::Local::now() - chrono::Duration::days(3))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let items = match crate::storage::invest::sentiment::list_sentiment_by_symbol(code, &since, 40)
    {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "[premarket] list_sentiment_by_symbol({code}) failed: {e}; treat as missing"
            );
            return (None, None);
        }
    };
    if items.is_empty() {
        return (None, None);
    }

    // 情绪：只取有 sentiment_hint 的条目均值；全为 None → 情绪缺失（催化仍算）。
    let hints: Vec<f64> = items.iter().filter_map(|i| i.sentiment_hint).collect();
    let sentiment = if hints.is_empty() {
        None
    } else {
        let avg = hints.iter().sum::<f64>() / hints.len() as f64;
        Some(((avg + 1.0) / 2.0 * 100.0).clamp(0.0, 100.0))
    };

    let catalyst = Some((items.len() as f64 * 10.0).min(100.0));
    (sentiment, catalyst)
}

/// 资金因子：`moneyflow_dc.net_amount`（个股主力净流入，万元）+ `moneyflow_hsgt.north_money`（北向，亿元）。
///
/// 归一策略（尽量鲁棒，缺一半仍可给分）：
/// - 主力净流入率：近 5 交易日 net_amount 之和取 tanh(sum/5e5) 映射到 0-100
///   （5e5 万元 = 50 亿主力净流入，接近顶部；负值同样对称压到 0）。
/// - 北向：昨日 north_money（亿）用 tanh(x/50) 映射 0-100（50 亿是较强流入）。
/// - 有一个可用即算，两个都算取平均；两个都缺 → None。
async fn compute_capital(symbol: &str) -> Option<f64> {
    let end = chrono::Local::now().format("%Y%m%d").to_string();
    let start = (chrono::Local::now() - chrono::Duration::days(14))
        .format("%Y%m%d")
        .to_string();

    let client = match crate::tushare::client::TushareClient::from_settings() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[premarket] tushare unavailable for capital({symbol}): {e}");
            return None;
        }
    };

    // ── 个股主力资金：近 5 日 net_amount 求和 ─────────────────────────
    let stock_score: Option<f64> = match client.moneyflow_dc(symbol, &start, &end).await {
        Ok(rows) if !rows.is_empty() => {
            // Tushare 通常返回最新在前；取前 5 条求 net_amount 之和（万元）
            let sum: f64 = rows
                .iter()
                .take(5)
                .filter_map(|r| r.net_amount)
                .sum();
            // tanh(sum/5e5)：5e5 万 = 50 亿；50 亿净流入 → tanh(1)=0.76 → 88 分
            let normalized = (sum / 5.0e5).tanh(); // -1..1
            Some((normalized + 1.0) / 2.0 * 100.0)
        }
        Ok(_) => None,
        Err(e) => {
            log::warn!("[premarket] moneyflow_dc({symbol}) failed: {e}");
            None
        }
    };

    // ── 北向资金（大盘层面）：昨日 north_money（亿元）─────────────────
    let hsgt_score: Option<f64> = match client.moneyflow_hsgt(&start, &end).await {
        Ok(rows) if !rows.is_empty() => {
            let latest = rows.first().and_then(|r| {
                // north_money 亿元；净流出为负
                Some(r.north_money)
            })?;
            let normalized = (latest / 50.0).tanh(); // ±50 亿是较强
            Some((normalized + 1.0) / 2.0 * 100.0)
        }
        Ok(_) => None,
        Err(e) => {
            log::warn!("[premarket] moneyflow_hsgt failed: {e}");
            None
        }
    };

    match (stock_score, hsgt_score) {
        (Some(a), Some(b)) => Some(((a * 0.7) + (b * 0.3)).clamp(0.0, 100.0)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// 技术因子：从 `compute_regime_for_symbol` 的 regime 字符串 + RSI + 分位合成 0-100。
///
/// - regime 基分：uptrend=75 / range_bound=55 / downtrend=35 / crash=15 / unknown=50。
/// - RSI 修正：50±(rsi-50)×0.4（RSI 极端往中位靠拢，避免超买/超卖时过高分）。
/// - 2年分位加成：分位 <0.2 或 >0.8 时轻微扣分（过热/过冷）。
async fn compute_technical(symbol: &str) -> Option<f64> {
    let client = match crate::tushare::client::TushareClient::from_settings() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[premarket] tushare unavailable for technical({symbol}): {e}");
            return None;
        }
    };
    let result = match crate::invest::regime::compute_regime_for_symbol(&client, symbol).await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("[premarket] compute_regime_for_symbol({symbol}) failed: {e}");
            return None;
        }
    };

    // regime 为 "unknown" 说明数据不足 60 根 K，视为缺失让 grade 走 missing 分支
    if result.regime == "unknown" {
        return None;
    }

    let base = match result.regime {
        "uptrend" => 75.0,
        "range_bound" => 55.0,
        "downtrend" => 35.0,
        "crash" => 15.0,
        _ => 50.0,
    };

    // RSI 拉回中位：base + (rsi-50) * 0.4，但 RSI>70 或 <30 时轻微惩罚
    let rsi = result.metrics.rsi14;
    let rsi_adj = if rsi > 70.0 || rsi < 30.0 {
        -5.0
    } else {
        (rsi - 50.0) * 0.2
    };

    // 极端分位轻微扣分：<0.2 过冷（可能是反弹机会但风险大）,>0.8 过热
    let q = result.metrics.price_quantile_2y;
    let q_adj: f64 = if q > 0.8 {
        -5.0
    } else if q < 0.2 {
        -3.0
    } else {
        0.0
    };

    Some((base + rsi_adj + q_adj).clamp(0.0, 100.0))
}

/// 组装一档 `(FactorBreakdown, missing)`：单档缺失填 50（中性），并记 missing。
async fn compute_factors(symbol: &str) -> (FactorBreakdown, Vec<String>) {
    // symbol 形如 "600519.SH"；sentiment 存 6 位裸码
    let code = symbol.split('.').next().unwrap_or(symbol);
    let mut missing = Vec::new();

    let (sent_opt, cat_opt) = compute_sentiment_and_catalyst(code);
    let sentiment = sent_opt.unwrap_or_else(|| {
        missing.push("sentiment".to_string());
        50.0
    });
    let catalyst = cat_opt.unwrap_or_else(|| {
        missing.push("catalyst".to_string());
        50.0
    });

    let capital = compute_capital(symbol).await.unwrap_or_else(|| {
        missing.push("capital".to_string());
        50.0
    });

    let technical = compute_technical(symbol).await.unwrap_or_else(|| {
        missing.push("technical".to_string());
        50.0
    });

    (
        FactorBreakdown {
            sentiment,
            capital,
            technical,
            catalyst,
        },
        missing,
    )
}

// ---------------------------------------------------------------------------
// AI 点评（结构化 JSON，不改档）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSector {
    pub name: String,
    pub tag: String,
    pub count: u32,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCommentary {
    pub sectors: Vec<AiSector>,
    pub tone: String,
}

/// 采样最近舆情条目，拼一段供 AI 聚合板块 + 打标签 + 给基调。
///
/// AI 失败或 JSON 解析失败均返回 None；调用方在 md 中显示占位文案，SABC 分级不受影响。
async fn ai_commentary(news_block: &str) -> Option<AiCommentary> {
    if news_block.trim().is_empty() {
        return None;
    }
    let prompt = format!(
        "你是A股盘前分析师。把以下新闻聚合成3-5个板块，每个给：name、tag(只能选:新闻强/催化强/情绪强/分歧大/风险预警)、count、note(一句话)。\
         风险预警专收监管/政策转向/处罚退市/地缘扰动。输出JSON: {{\"sectors\":[...],\"tone\":\"基调总述\"}}。只输出JSON。\n\n{}",
        news_block
    );
    let resp = crate::invest::event_analyzer::cli_complete(
        "你是严谨的金融分析师，只输出JSON。",
        &prompt,
    )
    .await
    .ok()?;
    let cleaned = resp
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    match serde_json::from_str::<AiCommentary>(cleaned) {
        Ok(v) => Some(v),
        Err(e) => {
            log::warn!("[premarket] ai_commentary parse failed: {e}; raw len={}", cleaned.len());
            None
        }
    }
}

/// 从最近 sentiment_items 拼一段新闻文本喂给 AI。上限 40 条，正文截断到 80 字避免 prompt 超长。
fn build_news_block_for_ai() -> String {
    let since = (chrono::Local::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let items = crate::storage::invest::sentiment::list_recent_sentiment(&since, 40)
        .unwrap_or_default();
    let mut buf = String::new();
    for it in items.iter().take(40) {
        let summary = it.summary.as_deref().unwrap_or("");
        let short = summary.chars().take(80).collect::<String>();
        buf.push_str(&format!(
            "- [{}] {} | {}\n",
            it.stance, it.title, short
        ));
    }
    buf
}

fn render_ai_commentary_md(ai: &AiCommentary) -> String {
    let mut md = String::new();
    md.push_str(&format!("**基调**：{}\n\n", ai.tone));
    for s in &ai.sectors {
        md.push_str(&format!(
            "- **{}**（{}，{} 条）：{}\n",
            s.name, s.tag, s.count, s.note
        ));
    }
    md
}

// ---------------------------------------------------------------------------
// 主入口
// ---------------------------------------------------------------------------

/// 生成盘前观察报告。返回 md 文件绝对路径。
///
/// 时序（CP2）：先跑 Plan A 四源归一化 → 再拉雪球独立通道 → 再读宏观快照 → 再打分。
/// 各段独立降级，绝不因单源失败中断。
///
/// **grade 隔离**：SABC 评级只在 `score()` 内根据总分算出；AI 点评仅进入
/// `aiCommentary`/md 展示层，任何 AI 失败都不影响 scores。
pub async fn generate_premarket_report(data_dir: &Path) -> Result<String, String> {
    let date = crate::invest::date_utils::get_invest_date();

    // 1. 采集 + 内联归一化（CP2 时序保证）——含雪球独立通道
    let _ = crate::invest::sentiment::collect_all_sentiment(None, 20).await;
    let _ = crate::invest::sentiment::fetch_xueqiu_market(15).await; // 降级不阻断

    // 2. 宏观快照（真实字段渲染）
    let (macro_md, macro_snapshot) = match build_macro_snapshot() {
        Some(snap) => (render_macro_md(&snap), Some(snap)),
        None => (
            "宏观快照：数据缺失（macro_cache 未初始化）\n".to_string(),
            None,
        ),
    };

    // 3. 股票池 SABC 打分（四因子真实计算）
    let cfg: PremarketConfig = get_premarket_config();
    let pool = collect_pool();
    let mut scores: Vec<SymbolScore> = Vec::with_capacity(pool.len());
    for (symbol, name) in &pool {
        let (factors, missing) = compute_factors(symbol).await;
        // 关键：grade 完全由 score() 计算，AI 无法触及
        scores.push(score(symbol, name, factors, missing, &cfg));
    }

    // 4. AI 点评（结构化 JSON；失败 → None，不影响分数）
    let news_block = build_news_block_for_ai();
    let ai = ai_commentary(&news_block).await;
    let ai_md = ai
        .as_ref()
        .map(render_ai_commentary_md)
        .unwrap_or_else(|| "AI 点评生成失败（不影响 SABC 分级）。\n".to_string());

    // 5. 组装 md + json 落盘
    let reports_dir = data_dir.join("invest").join("reports");
    std::fs::create_dir_all(&reports_dir).map_err(|e| format!("mkdir reports: {e}"))?;

    let mut md = String::new();
    md.push_str(&format!("# 盘前观察 {date}\n\n"));
    md.push_str("## 宏观快照\n\n");
    md.push_str(&macro_md);
    md.push('\n');
    md.push_str("## SABC 观察池\n\n");
    md.push_str(&format!("共 {} 标的\n\n", scores.len()));
    md.push_str(&render_scores_md(&scores));
    md.push_str("\n## AI 点评\n\n");
    md.push_str(&ai_md);
    md.push('\n');

    let md_path = reports_dir.join(format!("premarket_{date}.md"));
    std::fs::write(&md_path, &md).map_err(|e| format!("write md: {e}"))?;

    let json_path = reports_dir.join(format!("premarket_{date}.json"));
    let json = serde_json::json!({
        "date": date,
        "macro": macro_snapshot,
        "scores": scores,
        "config": cfg,
        "aiCommentary": ai,
    });
    std::fs::write(
        &json_path,
        serde_json::to_string_pretty(&json).map_err(|e| format!("serialize json: {e}"))?,
    )
    .map_err(|e| format!("write json: {e}"))?;

    Ok(md_path.to_string_lossy().to_string())
}
