//! 盘前报告生成器：采集 → 内联归一化 → 打分 → AI 点评（Task 6）→ 组装 md+json 存盘。
//!
//! 尽力而为：任一数据源失败标注"数据缺失"不中断。写盘路径与 `daily_report::generate_daily_report`
//! 保持一致：`{data_dir}/invest/reports/premarket_{date}.md(+.json)`。
//!
//! 骨架说明：本 Task 5 只落地采集 → 时序（CP2）→ 空 SABC 打分 → 落盘管线。
//! 四因子真实计算与结构化 AI 点评见 Task 6。

use std::path::Path;

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

/// 生成盘前观察报告。返回 md 文件绝对路径。
///
/// 时序（CP2）：先跑 Plan A 四源归一化 → 再拉雪球独立通道 → 再读宏观快照 → 再打分。
/// 各段独立降级，绝不因单源失败中断。
pub async fn generate_premarket_report(data_dir: &Path) -> Result<String, String> {
    let date = crate::invest::date_utils::get_invest_date();

    // 1. 采集 + 内联归一化（CP2 时序保证）——含雪球独立通道
    let _ = crate::invest::sentiment::collect_all_sentiment(None, 20).await;
    let _ = crate::invest::sentiment::fetch_xueqiu_market(15).await; // 降级不阻断

    // 2. 宏观快照（真实字段渲染）
    let (macro_md, macro_snapshot) = match build_macro_snapshot() {
        Some(snap) => (render_macro_md(&snap), Some(snap)),
        None => ("宏观快照：数据缺失（macro_cache 未初始化）\n".to_string(), None),
    };

    // 3. 股票池 SABC 打分
    let cfg: PremarketConfig = get_premarket_config();
    let pool = collect_pool();
    let mut scores: Vec<SymbolScore> = Vec::new();
    for (symbol, name) in &pool {
        // Task 6 才实现的四因子真实计算；本骨架用中性 50 占位并 missing 全标注，
        // 以便 md 中一眼可见"这是占位"，避免误读为真实评分。
        let factors = FactorBreakdown {
            sentiment: 50.0,
            capital: 50.0,
            technical: 50.0,
            catalyst: 50.0,
        };
        let missing = vec![
            "sentiment".to_string(),
            "capital".to_string(),
            "technical".to_string(),
            "catalyst".to_string(),
        ];
        scores.push(score(symbol, name, factors, missing, &cfg));
    }

    // 4. AI 点评占位（Task 6 填充结构化 JSON）
    let ai_commentary = "（AI 点评待 Task 6 接入）";

    // 5. 组装 md + json 落盘
    let reports_dir = data_dir.join("invest").join("reports");
    std::fs::create_dir_all(&reports_dir).map_err(|e| format!("mkdir reports: {e}"))?;

    let mut md = String::new();
    md.push_str(&format!("# 盘前观察 {date}\n\n"));
    md.push_str("## 宏观快照\n\n");
    md.push_str(&macro_md);
    md.push('\n');
    md.push_str("## SABC 观察池\n\n");
    md.push_str(&format!("共 {} 标的（四因子真实计算见 Task 6）\n\n", scores.len()));
    md.push_str(&render_scores_md(&scores));
    md.push_str("\n## AI 点评\n\n");
    md.push_str(ai_commentary);
    md.push('\n');

    let md_path = reports_dir.join(format!("premarket_{date}.md"));
    std::fs::write(&md_path, &md).map_err(|e| format!("write md: {e}"))?;

    let json_path = reports_dir.join(format!("premarket_{date}.json"));
    let json = serde_json::json!({
        "date": date,
        "macro": macro_snapshot,
        "scores": scores,
        "config": cfg,
        "aiCommentary": ai_commentary,
    });
    std::fs::write(
        &json_path,
        serde_json::to_string_pretty(&json).map_err(|e| format!("serialize json: {e}"))?,
    )
    .map_err(|e| format!("write json: {e}"))?;

    Ok(md_path.to_string_lossy().to_string())
}
