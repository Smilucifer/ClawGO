use super::{DreamResult, StageResult};
use crate::storage::invest::domain_insights::{self, DomainInsight};
use crate::storage::invest::verdicts;
use crate::tushare::TushareClient;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug)]
struct DreamCandidate {
    symbol: String,
    verdict: String,
    regime: String,
    hit_rate_1d: f64,
    hit_rate_7d: f64,
    hit_rate_30d: f64,
    count: usize,
    avg_return_1d: f64,
    avg_return_7d: f64,
    avg_return_30d: f64,
    source_ids: Vec<String>,
}

fn compute_hit_rate(returns: &[f64], is_bullish: bool) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }
    let hits = returns
        .iter()
        .filter(|r| if is_bullish { **r > 0.0 } else { **r < 0.0 })
        .count();
    hits as f64 / returns.len() as f64
}

fn avg_or_zero(returns: &[f64]) -> f64 {
    if returns.is_empty() {
        0.0
    } else {
        returns.iter().sum::<f64>() / returns.len() as f64
    }
}

pub async fn run_invest_pipeline(tushare_token: &str) -> Result<DreamResult, String> {
    let pipeline_start = Instant::now();
    let client = TushareClient::new(tushare_token.to_string());
    let config = crate::invest::scheduler::config::load_dream_config();

    // Snapshot before (for rollback)
    let before_json = domain_insights::get_active_insights_json()?;

    // ── Light Sleep: Extract tuples from recent verdicts ───────────────
    let light_start = Instant::now();
    let verdicts = verdicts::list_verdicts(None, Some(200))?;
    let today = chrono::Local::now().naive_local().date();
    let cutoff = today - chrono::Duration::days(config.lookback_days);

    let recent: Vec<_> = verdicts
        .iter()
        .filter(|v| {
            // created_at is "YYYY-MM-DD HH:MM:SS"; take first 10 chars and parse
            let date_part = &v.created_at[..10.min(v.created_at.len())];
            let Ok(dt) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") else {
                return false;
            };
            dt >= cutoff
        })
        .collect();

    let tuples: Vec<(String, String, String, String, String)> = recent
        .iter()
        .map(|v| {
            let regime = v
                .macro_signal
                .clone()
                .unwrap_or_else(|| "unknown".into());
            (
                v.symbol.clone(),
                v.verdict.clone(),
                v.confidence.unwrap_or(0.5).to_string(),
                v.created_at[..10.min(v.created_at.len())].to_string(),
                regime,
            )
        })
        .collect();

    let light_dur = light_start.elapsed().as_millis() as i64;

    // ── REM Sleep: Aggregate hit rates ───────────────────────────────
    let rem_start = Instant::now();

    // Group tuples by (symbol, verdict, regime)
    let mut groups: HashMap<(String, String, String), Vec<&(String, String, String, String, String)>> =
        HashMap::new();
    for t in &tuples {
        let key = (t.0.clone(), t.1.clone(), t.4.clone());
        groups.entry(key).or_default().push(t);
    }

    // Pre-build lookup: recent verdict by (symbol, date_prefix) → ids
    let mut recent_lookup: HashMap<(&str, &str), Vec<&str>> = HashMap::new();
    for v in &recent {
        let date_prefix = &v.created_at[..10.min(v.created_at.len())];
        recent_lookup
            .entry((v.symbol.as_str(), date_prefix))
            .or_default()
            .push(v.id.as_str());
    }

    let mut candidates: Vec<DreamCandidate> = Vec::new();

    for ((symbol, verdict, regime), group) in &groups {
        // Only process directional verdicts (skip HOLD, NEUTRAL, etc.)
        let is_bullish = matches!(verdict.as_str(), "BUY" | "ACCUMULATE");
        let is_bearish = matches!(verdict.as_str(), "SELL" | "TRIM");
        if !is_bullish && !is_bearish {
            continue;
        }

        if (group.len() as i64) < config.min_count {
            continue;
        }

        // Fetch prices for this symbol
        let first_date = group
            .iter()
            .map(|t| &t.3)
            .min()
            .cloned()
            .unwrap_or_default();
        let end_date = today.format("%Y%m%d").to_string();
        let ts_code = crate::invest::verdict_review::to_ts_code(symbol);
        let Ok(bars) = client
            .daily(&ts_code, &first_date.replace('-', ""), &end_date)
            .await
        else {
            continue;
        };
        if bars.is_empty() {
            continue;
        }

        // Build date→index lookup for O(1) bar access
        let bar_index: HashMap<&str, usize> = bars
            .iter()
            .enumerate()
            .map(|(i, b)| (b.trade_date.as_str(), i))
            .collect();

        let mut returns_1d = Vec::new();
        let mut returns_7d = Vec::new();
        let mut returns_30d = Vec::new();

        for t in group {
            let date_str = t.3.replace('-', "");
            let Some(&idx) = bar_index.get(date_str.as_str()) else {
                continue;
            };
            let price = bars[idx].close;
            if price == 0.0 {
                continue;
            }

            for (days, returns) in [
                (1, &mut returns_1d),
                (7, &mut returns_7d),
                (30, &mut returns_30d),
            ] {
                let Ok(target_date) =
                    chrono::NaiveDate::parse_from_str(&date_str, "%Y%m%d")
                else {
                    continue;
                };
                let target = (target_date + chrono::Duration::days(days))
                    .format("%Y%m%d")
                    .to_string();
                // Binary-ish: bar_index is a HashMap, so O(1) lookup for exact date;
                // for "next bar >= target", fall back to sorted scan
                if let Some(&aidx) = bar_index.get(target.as_str()) {
                    returns.push((bars[aidx].close - price) / price);
                } else {
                    // Find first bar with trade_date >= target
                    if let Some(abar) = bars.iter().find(|b| b.trade_date >= target) {
                        returns.push((abar.close - price) / price);
                    }
                }
            }
        }

        // Compute hit rate per window
        let hit_rate_1d = compute_hit_rate(&returns_1d, is_bullish);
        let hit_rate_7d = compute_hit_rate(&returns_7d, is_bullish);
        let hit_rate_30d = compute_hit_rate(&returns_30d, is_bullish);

        let total = returns_30d.len();
        if total == 0 {
            continue;
        }

        // Score uses 30d hit rate (primary signal) + sample size
        let score = hit_rate_30d * 0.7 + (total.min(10) as f64 / 10.0) * 0.3;

        if score >= config.min_score {
            let source_ids: Vec<String> = group
                .iter()
                .flat_map(|t| {
                    let date_prefix = &t.3[..10.min(t.3.len())];
                    recent_lookup
                        .get(&(t.0.as_str(), date_prefix))
                        .map(|ids| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>())
                        .unwrap_or_default()
                })
                .collect();

            candidates.push(DreamCandidate {
                symbol: symbol.clone(),
                verdict: verdict.clone(),
                regime: regime.clone(),
                hit_rate_1d,
                hit_rate_7d,
                hit_rate_30d,
                count: total,
                avg_return_1d: avg_or_zero(&returns_1d),
                avg_return_7d: avg_or_zero(&returns_7d),
                avg_return_30d: avg_or_zero(&returns_30d),
                source_ids,
            });
        }
    }
    let rem_dur = rem_start.elapsed().as_millis() as i64;

    // ── Deep Sleep: Write insights to domain_insights ────────────────
    let deep_start = Instant::now();
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let mut written = 0usize;

    for c in &candidates {
        let content = format!(
            "{} {} in {}: hit rates 1d/7d/30d: {:.0}%/{:.0}%/{:.0}% over {} samples (avg ret: {:.1}%/{:.1}%/{:.1}%)",
            c.symbol,
            c.verdict,
            c.regime,
            c.hit_rate_1d * 100.0,
            c.hit_rate_7d * 100.0,
            c.hit_rate_30d * 100.0,
            c.count,
            c.avg_return_1d * 100.0,
            c.avg_return_7d * 100.0,
            c.avg_return_30d * 100.0,
        );
        let insight = DomainInsight {
            id: format!("dream_{}_{}_{}", c.symbol, c.verdict, c.regime)
                .replace(' ', "_"),
            insight_type: "pattern".into(),
            symbol: Some(c.symbol.clone()),
            content,
            confidence: Some(c.hit_rate_30d),
            source_verdict_ids: Some(c.source_ids.join(",")),
            status: "active".into(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        domain_insights::upsert_insight(&insight)?;
        written += 1;
    }

    let after_json = domain_insights::get_active_insights_json()?;

    // Snapshot (for rollback)
    if let Err(e) = super::snapshot::save_snapshot(
        "invest",
        "pipeline",
        &before_json,
        &after_json,
        &format!("{written} insights written, {} candidates found", candidates.len()),
    ) {
        log::warn!("Dream snapshot failed: {e}");
    }

    let deep_dur = deep_start.elapsed().as_millis() as i64;
    let total_dur = pipeline_start.elapsed().as_millis() as i64;

    Ok(DreamResult {
        insights_written: written,
        insights_updated: 0,
        insights_archived: 0,
        pipeline_duration_ms: total_dur,
        stages: vec![
            StageResult {
                stage: "light".into(),
                duration_ms: light_dur,
                items_processed: recent.len(),
                items_output: tuples.len(),
            },
            StageResult {
                stage: "rem".into(),
                duration_ms: rem_dur,
                items_processed: groups.len(),
                items_output: candidates.len(),
            },
            StageResult {
                stage: "deep".into(),
                duration_ms: deep_dur,
                items_processed: candidates.len(),
                items_output: written,
            },
        ],
    })
}
