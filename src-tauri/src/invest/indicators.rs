// ---------------------------------------------------------------------------
// Shared deterministic indicator computations
//
// Pure math — no I/O, no LLM, no caching. Used by regime.rs and tools.rs
// to avoid duplicating MA/RSI/volatility/percentile logic.
// ---------------------------------------------------------------------------

/// Simple moving average of the last `period` values.
///
/// `data` is newest-first (Tushare daily convention). Returns `None` if
/// `data.len() < period`.
pub fn compute_ma(data: &[f64], period: usize) -> Option<f64> {
    if period == 0 || data.len() < period {
        return None;
    }
    Some(data.iter().take(period).sum::<f64>() / period as f64)
}

/// SMA series computed from **newest-first** data.
///
/// Returns a vector of the same length as `data`, where each element is the
/// SMA of the preceding `period` values (inclusive). Positions with fewer
/// than `period` data points use all available data (same behaviour as the
/// original `exec_multi_timeframe`).
pub fn compute_ma_series(data: &[f64], period: usize) -> Vec<f64> {
    data.iter()
        .enumerate()
        .map(|(i, _)| {
            let end = (i + period).min(data.len());
            data[i..end].iter().sum::<f64>() / (end - i) as f64
        })
        .collect()
}

/// RSI-14 using Wilder's smoothing.
///
/// `closes` must be in **chronological** order (oldest → newest).
/// Returns a value in `[0.0, 100.0]`. Returns `50.0` when fewer than 15 bars
/// are available (insufficient data for a meaningful RSI).
pub fn compute_rsi14(closes: &[f64]) -> f64 {
    if closes.len() < 15 {
        return 50.0;
    }

    let mut gains = Vec::with_capacity(closes.len() - 1);
    let mut losses = Vec::with_capacity(closes.len() - 1);
    for w in closes.windows(2) {
        let diff = w[1] - w[0];
        if diff > 0.0 {
            gains.push(diff);
            losses.push(0.0);
        } else {
            gains.push(0.0);
            losses.push(-diff);
        }
    }

    // First 14-period simple averages
    let mut avg_gain: f64 = gains[..14].iter().sum::<f64>() / 14.0;
    let mut avg_loss: f64 = losses[..14].iter().sum::<f64>() / 14.0;

    // Wilder smoothing
    for i in 14..gains.len() {
        avg_gain = (avg_gain * 13.0 + gains[i]) / 14.0;
        avg_loss = (avg_loss * 13.0 + losses[i]) / 14.0;
    }

    // Both zero → completely flat price series → neutral RSI
    if avg_gain == 0.0 && avg_loss == 0.0 {
        return 50.0;
    }
    if avg_loss == 0.0 {
        return 100.0;
    }
    let rs = avg_gain / avg_loss;
    100.0 - 100.0 / (1.0 + rs)
}

/// 20-day annualized volatility from **chronological** closes.
///
/// Uses the standard return formula `(close[i] / close[i-1]) - 1.0`,
/// annualized by `√252`. Returns `0.0` when fewer than 21 bars are
/// available.
pub fn compute_volatility(closes: &[f64]) -> f64 {
    if closes.len() < 21 {
        return 0.0;
    }
    let returns: Vec<f64> = closes.windows(2).map(|w| w[1] / w[0] - 1.0).collect();
    let recent = &returns[returns.len() - 20..];
    // Guard against zero close prices in recent window (suspension, data error)
    // Only checks the last 21 closes used for volatility, not the entire series
    let recent_closes_start = closes.len().saturating_sub(21);
    if closes[recent_closes_start..].iter().any(|&c| c == 0.0) {
        return 0.0;
    }
    let mean = recent.iter().sum::<f64>() / 20.0;
    let variance = recent.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / 20.0;
    variance.sqrt() * 252.0_f64.sqrt()
}

/// Percentile rank of `value` within `data` (0.0–100.0).
///
/// Returns `50.0` when `data` is empty.
pub fn compute_price_percentile(value: f64, data: &[f64]) -> f64 {
    if data.is_empty() {
        return 50.0;
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let rank = sorted.iter().position(|&v| v >= value).unwrap_or(sorted.len());
    rank as f64 / sorted.len() as f64 * 100.0
}

/// Trend classification from MA5/MA20/MA60/MA120.
///
/// Returns one of: "强势多头排列", "多头排列", "强势空头排列", "空头排列", "震荡整理".
/// `ma120` is `None` when insufficient data (<120 bars).
pub fn classify_trend(
    latest_close: f64,
    ma5: f64,
    ma20: f64,
    ma60: f64,
    ma120: Option<f64>,
) -> &'static str {
    if let Some(m120) = ma120 {
        if latest_close > ma5 && ma5 > ma20 && ma20 > ma60 && ma60 > m120 {
            return "强势多头排列";
        }
        if latest_close < ma5 && ma5 < ma20 && ma20 < ma60 && ma60 < m120 {
            return "强势空头排列";
        }
    }
    if latest_close > ma5 && ma5 > ma20 && ma20 > ma60 {
        "多头排列"
    } else if latest_close < ma5 && ma5 < ma20 && ma20 < ma60 {
        "空头排列"
    } else {
        "震荡整理"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ma_basic() {
        let data = vec![10.0, 20.0, 30.0]; // newest first
        let ma2 = compute_ma(&data, 2);
        assert!(ma2.is_some());
        assert!((ma2.unwrap() - 15.0).abs() < f64::EPSILON); // (20+10)/2
    }

    #[test]
    fn ma_insufficient() {
        let data = vec![10.0];
        assert!(compute_ma(&data, 5).is_none());
    }

    #[test]
    fn rsi_all_gains() {
        let closes: Vec<f64> = (0..20).map(|i| 100.0 + i as f64).collect();
        let rsi = compute_rsi14(&closes);
        assert!((rsi - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rsi_all_losses() {
        let closes: Vec<f64> = (0..20).map(|i| 120.0 - i as f64).collect();
        let rsi = compute_rsi14(&closes);
        assert!((rsi - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rsi_insufficient() {
        let closes = vec![100.0; 10];
        assert_eq!(compute_rsi14(&closes), 50.0);
    }

    #[test]
    fn rsi_mixed() {
        let mut closes = Vec::new();
        let mut price = 100.0;
        for i in 0..30 {
            price += if i % 2 == 0 { 1.0 } else { -0.5 };
            closes.push(price);
        }
        let rsi = compute_rsi14(&closes);
        assert!(rsi > 40.0 && rsi < 70.0, "RSI was {rsi}");
    }

    #[test]
    fn volatility_insufficient() {
        let closes = vec![100.0; 10];
        assert_eq!(compute_volatility(&closes), 0.0);
    }

    #[test]
    fn percentile_basic() {
        let data = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let p = compute_price_percentile(30.0, &data);
        // position(|&v| v >= 30.0) = index 2 → 2/5 * 100 = 40.0
        // (percentage of values strictly below the given value)
        assert!((p - 40.0).abs() < f64::EPSILON);
    }

    #[test]
    fn percentile_empty() {
        assert_eq!(compute_price_percentile(10.0, &[]), 50.0);
    }

    #[test]
    fn trend_strong_bull() {
        let t = classify_trend(110.0, 105.0, 100.0, 95.0, Some(90.0));
        assert_eq!(t, "强势多头排列");
    }

    #[test]
    fn trend_bull() {
        let t = classify_trend(110.0, 105.0, 100.0, 95.0, None);
        assert_eq!(t, "多头排列");
    }

    #[test]
    fn trend_strong_bear() {
        let t = classify_trend(80.0, 85.0, 90.0, 95.0, Some(100.0));
        assert_eq!(t, "强势空头排列");
    }

    #[test]
    fn trend_range() {
        let t = classify_trend(100.0, 105.0, 95.0, 100.0, None);
        assert_eq!(t, "震荡整理");
    }
}
