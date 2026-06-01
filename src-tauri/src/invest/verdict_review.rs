use chrono::{Duration as ChronoDuration, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::storage::invest::verdict_reviews::{self, VerdictReviewEntry};
use crate::storage::invest::verdict_tracking;
use crate::storage::invest::verdicts;
use crate::tushare::client::TushareClient;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// ATR multiplier for the flat-zone threshold.
const K_FLAT: f64 = 1.0;

/// Maximum flat threshold (8%).
const MAX_FLAT_THRESHOLD: f64 = 0.08;

/// Review windows in days.
const WINDOWS: &[i64] = &[1, 7, 30];

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Aggregate summary of verdict review results.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictReviewSummary {
    pub total_verdicts: usize,
    pub overall_hit_rate: f64,
    pub directional_hit_rate: f64,
    pub by_window: Vec<WindowStats>,
    pub by_verdict: Vec<VerdictStats>,
    pub last_review_at: Option<String>,
}

/// Per-window hit rate statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowStats {
    pub window_days: i64,
    pub sample_count: usize,
    pub hit_rate: f64,
}

/// Per-verdict-type hit rate statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictStats {
    pub verdict_type: String,
    pub sample_count: usize,
    pub avg_confidence: Option<f64>,
    pub hit_rate_1d: Option<f64>,
    pub hit_rate_7d: Option<f64>,
    pub hit_rate_30d: Option<f64>,
}

// ---------------------------------------------------------------------------
// Core logic
// ---------------------------------------------------------------------------

/// Calculate the flat-zone threshold: K_FLAT * atr_pct * sqrt(days), capped at 8%.
///
/// `atr_pct` is ATR14 / price as a fraction (e.g. 0.03 for 3%).
pub fn flat_threshold(atr_pct: f64, days: i64) -> f64 {
    let raw = K_FLAT * atr_pct * (days as f64).sqrt();
    raw.min(MAX_FLAT_THRESHOLD)
}

/// Determine if a verdict was "hit" given the actual return.
///
/// Directional logic:
/// - BUY / ACCUMULATE: hit if return > 0 (raw comparison, no threshold)
/// - SELL / TRIM: hit if return < 0 (raw comparison, no threshold)
/// - HOLD: hit if abs(return) < threshold (stayed within flat zone)
pub fn is_hit(verdict_type: &str, return_pct: f64, threshold: f64) -> bool {
    match verdict_type {
        "BUY" | "ACCUMULATE" => return_pct > 0.0,
        "SELL" | "TRIM" => return_pct < 0.0,
        "HOLD" => return_pct.abs() < threshold,
        // Unknown verdict types: conservative — treat as not hit
        _ => false,
    }
}

/// Convert a symbol in plain 6-digit format (e.g. "600519") to Tushare ts_code
/// format (e.g. "600519.SH"). If already in ts_code format, returns as-is.
pub fn to_ts_code(symbol: &str) -> String {
    if symbol.contains('.') {
        return symbol.to_string();
    }
    let suffix = match symbol.chars().next() {
        Some('6') | Some('9') => ".SH",
        Some('0') | Some('3') => ".SZ",
        Some('4') | Some('8') => ".BJ",
        _ => {
            log::warn!("Unknown stock prefix for '{}', defaulting to .SH", symbol);
            ".SH"
        }
    };
    format!("{}{}", symbol, suffix)
}

/// Extract the date string from a verdict's created_at field.
/// created_at is in ISO format like "2026-05-29 14:30:00" or "2026-05-29T14:30:00".
/// Returns YYYYMMDD format for Tushare, or None if parsing fails.
pub fn extract_date(created_at: &str) -> Option<String> {
    // Take first 10 chars (YYYY-MM-DD), remove hyphens
    let date_part = created_at.get(..10)?;
    let yyyymmdd: String = date_part.chars().filter(|c| *c != '-').collect();
    if yyyymmdd.len() == 8 {
        Some(yyyymmdd)
    } else {
        None
    }
}

/// Calculate ATR14 / price as a percentage (fraction).
///
/// ATR14 = average of True Range over the last 14 bars.
/// True Range = max(high - low, abs(high - prev_close), abs(low - prev_close)).
pub fn calc_atr_pct(bars: &[crate::tushare::client::DailyBar], price: f64) -> f64 {
    if bars.len() < 2 || price <= 0.0 {
        return 0.02; // default 2% if insufficient data
    }

    let n = bars.len();
    let window = 14.min(n - 1);
    let start = n - window;

    let mut tr_sum = 0.0;
    for i in start..n {
        let high = bars[i].high;
        let low = bars[i].low;
        let prev_close = if i > 0 { bars[i - 1].close } else { bars[i].pre_close };

        let tr = (high - low)
            .max((high - prev_close).abs())
            .max((low - prev_close).abs());
        tr_sum += tr;
    }

    let atr = tr_sum / window as f64;
    atr / price
}

/// Add calendar days to a YYYYMMDD date string, returning a new YYYYMMDD string.
pub fn date_from_offset(date: &str, days: i64) -> Option<String> {
    let naive = NaiveDate::parse_from_str(date, "%Y%m%d").ok()?;
    let offset = naive + ChronoDuration::days(days);
    Some(offset.format("%Y%m%d").to_string())
}

/// Find the bar with a matching trade_date, or the nearest bar after the given date.
fn find_bar<'a>(bars: &'a [crate::tushare::client::DailyBar], target_date: &str) -> Option<&'a crate::tushare::client::DailyBar> {
    // First try exact match
    if let Some(bar) = bars.iter().find(|b| b.trade_date == target_date) {
        return Some(bar);
    }
    // Find the nearest bar after the target date
    bars.iter()
        .filter(|b| b.trade_date.as_str() >= target_date)
        .min_by_key(|b| b.trade_date.clone())
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

/// Run the full verdict review pipeline.
///
/// This is a tracking-aware pipeline:
/// 1. Load only verdicts that have active tracking entries
/// 2. For each tracked verdict, check if the symbol is still in holdings
/// 3. If sold/removed from watch → stop tracking, but still compute final review
/// 4. Fetch daily bars, calculate returns for each window (1d, 7d, 30d)
/// 5. Determine hits based on verdict type and ATR-based flat threshold
/// 6. Write reviews to verdict_reviews table
/// 7. Aggregate and return summary
pub async fn run_verdict_review(tushare_token: &str) -> Result<VerdictReviewSummary, String> {
    let client = TushareClient::with_token(tushare_token.to_string());

    // 1. Load active tracked verdicts
    let tracked = verdict_tracking::list_active_tracking()?;

    if tracked.is_empty() {
        // No active tracking — still aggregate from stored reviews for UI display
        let reviews = verdict_reviews::list_reviews(None, None)?;
        return Ok(aggregate_from_stored(&reviews));
    }

    // Build a set of symbols currently in holdings (hold or watch)
    let holdings = crate::storage::invest::portfolio::list_holdings()?;
    let held_symbols: std::collections::HashSet<String> = holdings
        .iter()
        .map(|h| h.symbol.clone())
        .collect();

    // 2-5. Process each tracked verdict
    let mut reviews: Vec<VerdictReviewEntry> = Vec::new();
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let mut verdicts_list: Vec<verdicts::Verdict> = Vec::new();

    for tracked_entry in &tracked {
        // Check if symbol is still in holdings
        let still_held = held_symbols.contains(&tracked_entry.symbol);
        if !still_held {
            // Position sold or removed from watch — stop tracking
            if let Err(e) = verdict_tracking::stop_tracking(&tracked_entry.verdict_id) {
                log::warn!("Failed to stop tracking {}: {}", tracked_entry.verdict_id, e);
            }
            log::info!(
                "Stopped tracking {} ({}) — no longer in holdings",
                tracked_entry.verdict_id,
                tracked_entry.symbol
            );
        }

        // Load the full verdict from DB by ID
        let verdict = match verdicts::get_verdict_by_id(&tracked_entry.verdict_id) {
            Ok(opt) => opt,
            Err(e) => {
                log::warn!("Failed to load verdict {}: {}", tracked_entry.verdict_id, e);
                continue;
            }
        };

        let v = match verdict {
            Some(v) => v,
            None => {
                log::warn!("Verdict {} not found in DB", tracked_entry.verdict_id);
                continue;
            }
        };

        verdicts_list.push(v.clone());

        let verdict_date = match extract_date(&v.created_at) {
            Some(d) => d,
            None => {
                log::warn!("Skipping verdict {}: unable to parse created_at '{}'", v.id, v.created_at);
                continue;
            }
        };

        let ts_code = to_ts_code(&v.symbol);

        // Fetch bars from (verdict_date - 1) to (verdict_date + 35)
        let start_date = match date_from_offset(&verdict_date, -1) {
            Some(d) => d,
            None => continue,
        };
        let end_date = match date_from_offset(&verdict_date, 35) {
            Some(d) => d,
            None => continue,
        };

        let bars = match client.daily(&ts_code, &start_date, &end_date).await {
            Ok(b) => b,
            Err(e) => {
                log::warn!("Failed to fetch daily bars for {} ({}): {}", v.symbol, ts_code, e);
                continue;
            }
        };

        if bars.is_empty() {
            log::warn!("No daily bars returned for {} ({}) around {}", v.symbol, ts_code, verdict_date);
            continue;
        }

        // Find the verdict date bar → price_at_verdict (use close price)
        let price_at_verdict = match find_bar(&bars, &verdict_date) {
            Some(bar) => bar.close,
            None => {
                log::warn!("No bar found for {} on or after {}", v.symbol, verdict_date);
                continue;
            }
        };

        if price_at_verdict <= 0.0 {
            log::warn!("Invalid price_at_verdict={} for {}", price_at_verdict, v.symbol);
            continue;
        }

        // Calculate ATR14%
        let atr_pct = calc_atr_pct(&bars, price_at_verdict);

        // For each window, find the price and calculate return
        for &window_days in WINDOWS {
            let target_date = match date_from_offset(&verdict_date, window_days) {
                Some(d) => d,
                None => continue,
            };

            let price_after = find_bar(&bars, &target_date).map(|b| b.close);

            let threshold = flat_threshold(atr_pct, window_days);

            let (return_pct, hit) = match price_after {
                Some(pa) if pa > 0.0 => {
                    let ret = (pa - price_at_verdict) / price_at_verdict;
                    let h = is_hit(&v.verdict, ret, threshold);
                    (Some(ret), h)
                }
                _ => (None, false),
            };

            let entry = VerdictReviewEntry {
                id: 0, // auto-increment
                verdict_id: v.id.clone(),
                symbol: v.symbol.clone(),
                verdict_type: v.verdict.clone(),
                verdict_date: verdict_date.clone(),
                window_days,
                price_at_verdict: Some(price_at_verdict),
                price_after,
                return_pct,
                hit,
                flat_threshold: Some(threshold),
                created_at: now.clone(),
            };

            if let Err(e) = verdict_reviews::upsert_review(&entry) {
                log::warn!("Failed to upsert review for {} window {}: {}", v.id, window_days, e);
            } else {
                reviews.push(entry);
            }
        }
    }

    // 6. Aggregate
    let summary = aggregate(&reviews, &verdicts_list);
    Ok(summary)
}

// ---------------------------------------------------------------------------
// Aggregation
// ---------------------------------------------------------------------------

/// Aggregate summary from stored `VerdictReviewEntry` rows (loaded from DB).
///
/// Uses 30-day focused semantics for the overall and directional hit rates,
/// which matches what the UI displays.
pub fn aggregate_from_stored(reviews: &[VerdictReviewEntry]) -> VerdictReviewSummary {
    if reviews.is_empty() {
        return VerdictReviewSummary {
            total_verdicts: 0,
            overall_hit_rate: 0.0,
            directional_hit_rate: 0.0,
            by_window: vec![],
            by_verdict: vec![],
            last_review_at: None,
        };
    }

    // Overall hit rate (30-day focused)
    let total_30d = reviews.iter().filter(|r| r.window_days == 30).count();
    let hits_30d = reviews.iter().filter(|r| r.window_days == 30 && r.hit).count();
    let overall_hit_rate = if total_30d > 0 {
        hits_30d as f64 / total_30d as f64
    } else {
        0.0
    };

    // Directional hit rate (30-day, exclude HOLD)
    let dir_total = reviews
        .iter()
        .filter(|r| r.verdict_type != "HOLD" && r.window_days == 30)
        .count();
    let dir_hits = reviews
        .iter()
        .filter(|r| r.verdict_type != "HOLD" && r.window_days == 30 && r.hit)
        .count();
    let directional_hit_rate = if dir_total > 0 {
        dir_hits as f64 / dir_total as f64
    } else {
        0.0
    };

    // By window
    let mut by_window: Vec<WindowStats> = WINDOWS
        .iter()
        .map(|&w| {
            let window_reviews: Vec<&VerdictReviewEntry> =
                reviews.iter().filter(|r| r.window_days == w).collect();
            let sample_count = window_reviews.len();
            let hits = window_reviews.iter().filter(|r| r.hit).count();
            WindowStats {
                window_days: w,
                sample_count,
                hit_rate: if sample_count > 0 {
                    hits as f64 / sample_count as f64
                } else {
                    0.0
                },
            }
        })
        .collect();
    by_window.sort_by_key(|w| w.window_days);

    // By verdict type
    let mut verdict_types: Vec<String> = {
        let mut types: Vec<String> = reviews.iter().map(|r| r.verdict_type.clone()).collect();
        types.sort();
        types.dedup();
        types
    };
    verdict_types.sort();

    let by_verdict: Vec<VerdictStats> = verdict_types
        .iter()
        .map(|vt| {
            let vtype_reviews: Vec<&VerdictReviewEntry> =
                reviews.iter().filter(|r| &r.verdict_type == vt).collect();
            let sample_count = vtype_reviews.len();

            let hit_rate_for_window = |window: i64| -> Option<f64> {
                let wr: Vec<&&VerdictReviewEntry> = vtype_reviews
                    .iter()
                    .filter(|r| r.window_days == window)
                    .collect();
                if wr.is_empty() {
                    None
                } else {
                    let hits = wr.iter().filter(|r| r.hit).count();
                    Some(hits as f64 / wr.len() as f64)
                }
            };

            VerdictStats {
                verdict_type: vt.clone(),
                sample_count,
                avg_confidence: None,
                hit_rate_1d: hit_rate_for_window(1),
                hit_rate_7d: hit_rate_for_window(7),
                hit_rate_30d: hit_rate_for_window(30),
            }
        })
        .collect();

    let total_verdicts = reviews
        .iter()
        .map(|r| &r.verdict_id)
        .collect::<std::collections::HashSet<_>>()
        .len();
    let last_review_at = reviews.iter().map(|r| r.created_at.clone()).max();

    VerdictReviewSummary {
        total_verdicts,
        overall_hit_rate,
        directional_hit_rate,
        by_window,
        by_verdict,
        last_review_at,
    }
}

/// Compute summary statistics from review data and verdicts.
fn aggregate(reviews: &[VerdictReviewEntry], verdicts_list: &[verdicts::Verdict]) -> VerdictReviewSummary {
    let total_verdicts = verdicts_list.len();

    // Overall hit rate (30-day focused, matching aggregate_from_stored)
    let total_30d = reviews.iter().filter(|r| r.window_days == 30).count();
    let hits_30d = reviews.iter().filter(|r| r.window_days == 30 && r.hit).count();
    let overall_hit_rate = if total_30d > 0 {
        hits_30d as f64 / total_30d as f64
    } else {
        0.0
    };

    // Directional hit rate (30-day, exclude HOLD)
    let dir_total = reviews
        .iter()
        .filter(|r| r.verdict_type != "HOLD" && r.window_days == 30)
        .count();
    let dir_hits = reviews
        .iter()
        .filter(|r| r.verdict_type != "HOLD" && r.window_days == 30 && r.hit)
        .count();
    let directional_hit_rate = if dir_total > 0 {
        dir_hits as f64 / dir_total as f64
    } else {
        0.0
    };

    // By window
    let by_window: Vec<WindowStats> = WINDOWS
        .iter()
        .map(|&w| {
            let window_reviews: Vec<&VerdictReviewEntry> = reviews.iter().filter(|r| r.window_days == w).collect();
            let window_hits = window_reviews.iter().filter(|r| r.hit).count();
            let sample_count = window_reviews.len();
            let hit_rate = if sample_count > 0 {
                window_hits as f64 / sample_count as f64
            } else {
                0.0
            };
            WindowStats {
                window_days: w,
                sample_count,
                hit_rate,
            }
        })
        .collect();

    // By verdict type
    let verdict_types: Vec<String> = {
        let mut types: Vec<String> = reviews.iter().map(|r| r.verdict_type.clone()).collect();
        types.sort();
        types.dedup();
        types
    };

    let by_verdict: Vec<VerdictStats> = verdict_types
        .iter()
        .map(|vt| {
            let vtype_reviews: Vec<&VerdictReviewEntry> = reviews.iter().filter(|r| &r.verdict_type == vt).collect();
            let sample_count = vtype_reviews.len();

            // Average confidence from the original verdicts
            let matching_verdicts: Vec<&verdicts::Verdict> = verdicts_list
                .iter()
                .filter(|v| &v.verdict == vt)
                .collect();
            let avg_confidence = if !matching_verdicts.is_empty() {
                let sum: f64 = matching_verdicts
                    .iter()
                    .filter_map(|v| v.confidence)
                    .sum();
                let count = matching_verdicts.iter().filter(|v| v.confidence.is_some()).count();
                if count > 0 {
                    Some(sum / count as f64)
                } else {
                    None
                }
            } else {
                None
            };

            let hit_rate_for_window = |window: i64| -> Option<f64> {
                let wr: Vec<&&VerdictReviewEntry> = vtype_reviews
                    .iter()
                    .filter(|r| r.window_days == window)
                    .collect();
                if wr.is_empty() {
                    None
                } else {
                    let hits = wr.iter().filter(|r| r.hit).count();
                    Some(hits as f64 / wr.len() as f64)
                }
            };

            VerdictStats {
                verdict_type: vt.clone(),
                sample_count,
                avg_confidence,
                hit_rate_1d: hit_rate_for_window(1),
                hit_rate_7d: hit_rate_for_window(7),
                hit_rate_30d: hit_rate_for_window(30),
            }
        })
        .collect();

    let last_review_at = reviews.iter().map(|r| r.created_at.clone()).max();

    VerdictReviewSummary {
        total_verdicts,
        overall_hit_rate,
        directional_hit_rate,
        by_window,
        by_verdict,
        last_review_at,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flat_threshold_basic() {
        // 3% ATR, 7 days: 1.0 * 0.03 * sqrt(7) ≈ 1.0 * 0.03 * 2.646 ≈ 0.0794
        let result = flat_threshold(0.03, 7);
        assert!((result - 0.0794).abs() < 0.001);
    }

    #[test]
    fn test_flat_threshold_cap() {
        // Very high ATR should be capped at 8%
        let result = flat_threshold(0.5, 30);
        assert!((result - MAX_FLAT_THRESHOLD).abs() < f64::EPSILON);
    }

    #[test]
    fn test_flat_threshold_one_day() {
        // 1 day: sqrt(1) = 1, so threshold = K_FLAT * atr_pct
        let result = flat_threshold(0.04, 1);
        assert!((result - 0.04).abs() < 0.001); // 1.0 * 0.04 = 0.04
    }

    #[test]
    fn test_is_hit_buy() {
        // BUY: hit if return > 0 (raw comparison, no threshold)
        assert!(is_hit("BUY", 0.05, 0.03));
        assert!(is_hit("BUY", 0.01, 0.03)); // any positive return is a hit
        assert!(!is_hit("BUY", -0.05, 0.03));
        assert!(!is_hit("BUY", 0.0, 0.03)); // exactly zero is not a hit
    }

    #[test]
    fn test_is_hit_sell() {
        // SELL: hit if return < 0 (raw comparison, no threshold)
        assert!(is_hit("SELL", -0.05, 0.03));
        assert!(is_hit("SELL", -0.01, 0.03)); // any negative return is a hit
        assert!(!is_hit("SELL", 0.05, 0.03));
        assert!(!is_hit("SELL", 0.0, 0.03)); // exactly zero is not a hit
    }

    #[test]
    fn test_is_hit_hold() {
        // HOLD: hit if abs(return) < threshold (strict less-than)
        assert!(is_hit("HOLD", 0.01, 0.03));
        assert!(is_hit("HOLD", -0.01, 0.03));
        assert!(!is_hit("HOLD", 0.05, 0.03));
        assert!(!is_hit("HOLD", 0.03, 0.03)); // exactly at threshold is not a hit
    }

    #[test]
    fn test_is_hit_accumulate() {
        // ACCUMULATE: same as BUY (raw comparison)
        assert!(is_hit("ACCUMULATE", 0.05, 0.03));
        assert!(is_hit("ACCUMULATE", 0.01, 0.03)); // any positive return
    }

    #[test]
    fn test_is_hit_trim() {
        // TRIM: same as SELL (raw comparison)
        assert!(is_hit("TRIM", -0.05, 0.03));
        assert!(is_hit("TRIM", -0.01, 0.03)); // any negative return
    }

    #[test]
    fn test_is_hit_unknown_verdict() {
        assert!(!is_hit("UNKNOWN", 0.05, 0.03));
    }

    #[test]
    fn test_to_ts_code_plain_sh() {
        assert_eq!(to_ts_code("600519"), "600519.SH");
    }

    #[test]
    fn test_to_ts_code_plain_sz() {
        assert_eq!(to_ts_code("000001"), "000001.SZ");
    }

    #[test]
    fn test_to_ts_code_plain_chinext() {
        assert_eq!(to_ts_code("300750"), "300750.SZ");
    }

    #[test]
    fn test_to_ts_code_already_ts() {
        assert_eq!(to_ts_code("600519.SH"), "600519.SH");
    }

    #[test]
    fn test_to_ts_code_star_market() {
        assert_eq!(to_ts_code("688001"), "688001.SH");
    }

    #[test]
    fn test_to_ts_code_bse_4() {
        assert_eq!(to_ts_code("430047"), "430047.BJ");
    }

    #[test]
    fn test_to_ts_code_bse_8() {
        assert_eq!(to_ts_code("830799"), "830799.BJ");
    }

    #[test]
    fn test_extract_date_standard() {
        assert_eq!(extract_date("2026-05-29 14:30:00"), Some("20260529".to_string()));
    }

    #[test]
    fn test_extract_date_iso() {
        assert_eq!(extract_date("2026-05-29T14:30:00"), Some("20260529".to_string()));
    }

    #[test]
    fn test_extract_date_short() {
        assert_eq!(extract_date("2026-05-29"), Some("20260529".to_string()));
    }

    #[test]
    fn test_extract_date_invalid() {
        assert_eq!(extract_date("invalid"), None);
    }

    #[test]
    fn test_date_from_offset() {
        assert_eq!(date_from_offset("20260529", 1), Some("20260530".to_string()));
        assert_eq!(date_from_offset("20260529", 7), Some("20260605".to_string()));
        assert_eq!(date_from_offset("20260529", -1), Some("20260528".to_string()));
    }

    #[test]
    fn test_date_from_offset_month_boundary() {
        // May 31 + 1 day = June 1
        assert_eq!(date_from_offset("20260531", 1), Some("20260601".to_string()));
    }

    #[test]
    fn test_calc_atr_pct_insufficient_data() {
        let bars = vec![];
        assert_eq!(calc_atr_pct(&bars, 100.0), 0.02); // default
    }

    #[test]
    fn test_calc_atr_pct_zero_price() {
        let bars = vec![];
        assert_eq!(calc_atr_pct(&bars, 0.0), 0.02); // default
    }
}
