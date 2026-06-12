use crate::invest::indicators;
use crate::tushare::client::TushareClient;
use chrono::{Duration as ChronoDur, Local};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Regime metrics computed from daily bars.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegimeMetrics {
    pub latest: f64,
    pub ma20: f64,
    pub ma60: f64,
    pub rsi14: f64,
    pub volatility_ann: f64,
    pub price_quantile_2y: f64,
}

/// Result of the regime classification for a single symbol.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegimeResult {
    pub regime: &'static str, // uptrend | downtrend | range_bound | crash | unknown
    pub reason: String,
    pub strategy_hint: &'static str,
    pub metrics: RegimeMetrics,
}

// ---------------------------------------------------------------------------
// Core computation
// ---------------------------------------------------------------------------

/// Compute the regime classification for a single symbol.
///
/// Fetches 500 trading days of daily bars from Tushare and derives:
/// - MA20 / MA60
/// - RSI-14 (Wilder smoothing)
/// - 20-day annualized volatility
/// - 2-year price percentile (500-bar window)
pub async fn compute_regime_for_symbol(
    client: &TushareClient,
    symbol: &str,
) -> Result<RegimeResult, String> {
    let end = Local::now().format("%Y%m%d").to_string();
    let start = (Local::now() - ChronoDur::days(700))
        .format("%Y%m%d")
        .to_string();

    let bars = client.daily(symbol, &start, &end).await?;
    if bars.len() < 60 {
        return Ok(RegimeResult {
            regime: "unknown",
            reason: format!(
                "Insufficient data for {symbol}: {} bars (need >= 60)",
                bars.len()
            ),
            strategy_hint: "hold",
            metrics: RegimeMetrics {
                latest: bars.last().map(|b| b.close).unwrap_or(0.0),
                ma20: 0.0,
                ma60: 0.0,
                rsi14: 50.0,
                volatility_ann: 0.0,
                price_quantile_2y: 0.5,
            },
        });
    }

    // Closes in chronological order (Tushare daily returns newest first,
    // so reverse to get oldest-to-newest).
    let mut closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    closes.reverse();

    let n = closes.len();
    let latest = closes[n - 1];

    // Newest-first for indicator helpers (which follow Tushare convention)
    let mut desc_closes = closes.clone();
    desc_closes.reverse();

    // ── MA20 / MA60 ────────────────────────────────────────────────────
    let ma20 = indicators::compute_ma(&desc_closes, 20)
        .unwrap_or(closes.iter().sum::<f64>() / n as f64);
    let ma60 = indicators::compute_ma(&desc_closes, 60)
        .unwrap_or(closes.iter().sum::<f64>() / n as f64);

    // ── RSI-14 (Wilder smoothing) ─────────────────────────────────────
    let rsi14 = indicators::compute_rsi14(&closes);

    // ── 20-day annualized volatility ──────────────────────────────────
    let volatility = indicators::compute_volatility(&closes);

    // ── 2-year price quantile (500-bar window) ────────────────────────
    // compute_price_percentile returns 0-100; RegimeMetrics stores 0.0-1.0 fraction
    let window_len = desc_closes.len().min(500);
    let price_quantile = indicators::compute_price_percentile(latest, &desc_closes[..window_len]) / 100.0;

    // ── 5-day drawdown ────────────────────────────────────────────────
    let five_day_ago = if n >= 6 { closes[n - 6] } else { closes[0] };
    let five_day_change = (latest - five_day_ago) / five_day_ago;

    // ── Classification ────────────────────────────────────────────────
    let (regime, strategy_hint, reason) = if latest < ma60 && five_day_change < -0.15 {
        (
            "crash",
            "hold",
            format!(
                "{symbol} in crash: price {latest:.3} < MA60 {ma60:.3}, 5-day drop {:.1}%",
                five_day_change * 100.0
            ),
        )
    } else if latest > ma20 && ma20 > ma60 {
        (
            "uptrend",
            "momentum",
            format!(
                "{symbol} in uptrend: price {latest:.3} > MA20 {ma20:.3} > MA60 {ma60:.3}"
            ),
        )
    } else if latest < ma20 && ma20 < ma60 {
        (
            "downtrend",
            "defensive",
            format!(
                "{symbol} in downtrend: price {latest:.3} < MA20 {ma20:.3} < MA60 {ma60:.3}"
            ),
        )
    } else if volatility < 0.35 {
        (
            "range_bound",
            "mean_reversion",
            format!(
                "{symbol} range-bound: price {latest:.3}, MA20 {ma20:.3}, MA60 {ma60:.3}, vol {:.1}%",
                volatility * 100.0
            ),
        )
    } else {
        // High-volatility mixed signal — still classified as range_bound
        (
            "range_bound",
            "cautious",
            format!(
                "{symbol} range-bound (high vol): price {latest:.3}, vol {:.1}%, mixed MA signals",
                volatility * 100.0
            ),
        )
    };

    Ok(RegimeResult {
        regime,
        reason,
        strategy_hint,
        metrics: RegimeMetrics {
            latest,
            ma20,
            ma60,
            rsi14,
            volatility_ann: volatility,
            price_quantile_2y: price_quantile,
        },
    })
}

// ---------------------------------------------------------------------------
// Formatting helper
// ---------------------------------------------------------------------------

/// Format a `RegimeResult` into a structured context string for LLM prompts.
///
/// Output format matches the QUANT_PROMPT expected structure:
/// ```text
/// REGIME: uptrend
/// REASON: <why this regime was classified>
/// INPUTS: ma20=10.20, ma60=9.80, volatility_ann=25.0%, rsi14=62.3, price_quantile_2y=72%
/// STRATEGY_HINT: momentum
/// ```
pub fn format_regime_context(result: &RegimeResult) -> String {
    let m = &result.metrics;
    format!(
        "REGIME: {}\n\
         REASON: {}\n\
         INPUTS: ma20={:.3}, ma60={:.3}, volatility_ann={:.1}%, rsi14={:.1}, price_quantile_2y={:.0}%\n\
         STRATEGY_HINT: {}",
        result.regime,
        result.reason,
        m.ma20,
        m.ma60,
        m.volatility_ann * 100.0,
        m.rsi14,
        m.price_quantile_2y * 100.0,
        result.strategy_hint,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invest::indicators;

    #[test]
    fn rsi_all_gains() {
        let closes: Vec<f64> = (0..20).map(|i| 100.0 + i as f64).collect();
        let rsi = indicators::compute_rsi14(&closes);
        assert!((rsi - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rsi_all_losses() {
        let closes: Vec<f64> = (0..20).map(|i| 120.0 - i as f64).collect();
        let rsi = indicators::compute_rsi14(&closes);
        assert!((rsi - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rsi_insufficient_data() {
        let closes = vec![100.0; 10];
        assert_eq!(indicators::compute_rsi14(&closes), 50.0);
    }

    #[test]
    fn rsi_mixed() {
        // Alternating up/down — RSI should be near 50
        let mut closes = Vec::new();
        let mut price = 100.0;
        for i in 0..30 {
            price += if i % 2 == 0 { 1.0 } else { -0.5 };
            closes.push(price);
        }
        let rsi = indicators::compute_rsi14(&closes);
        assert!(rsi > 40.0 && rsi < 70.0, "RSI was {rsi}");
    }

    #[test]
    fn format_regime_context_output() {
        let result = RegimeResult {
            regime: "uptrend",
            reason: "test reason".into(),
            strategy_hint: "momentum",
            metrics: RegimeMetrics {
                latest: 10.5,
                ma20: 10.2,
                ma60: 9.8,
                rsi14: 62.3,
                volatility_ann: 0.25,
                price_quantile_2y: 0.72,
            },
        };
        let ctx = format_regime_context(&result);
        // Must match the structured format expected by QUANT_PROMPT
        assert!(ctx.contains("REGIME: uptrend"));
        assert!(ctx.contains("REASON: test reason"));
        assert!(ctx.contains("STRATEGY_HINT: momentum"));
        assert!(ctx.contains("ma20=10.20"));
        assert!(ctx.contains("ma60=9.80"));
        assert!(ctx.contains("rsi14=62.3"));
        assert!(ctx.contains("volatility_ann=25.0%"));
        assert!(ctx.contains("price_quantile_2y=72%"));
    }
}
