//! Scheduler job: refresh the 12 canonical macro indicators in macro_cache.
//!
//! Runs on `*/15 8-22 * * 1-5` (every 15 minutes during 8-22 on weekdays).
//! Partial failure strategy: failed indicators keep stale data, logged as warn.

use crate::storage::invest::macro_cache;
use crate::tushare::client::TushareClient;
use std::future::Future;
use std::pin::Pin;

type MacroEntry = (String, Option<f64>, Option<String>);
type MacroResult = Result<Vec<MacroEntry>, String>;
type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Refresh all 12 macro indicators. Called from the scheduler runner.
///
/// Each indicator is fetched independently. On failure, the existing cache
/// entry is preserved and a warning is logged.
pub async fn refresh_macro_cache(client: &TushareClient) -> Result<String, String> {
    let end_date = chrono::Local::now().format("%Y%m%d").to_string();
    let start_date =
        (chrono::Local::now() - chrono::Duration::days(90)).format("%Y%m%d").to_string();

    // Clone the client for each task to satisfy 'static requirement on BoxFuture.
    // reqwest::Client is cheap to clone (shares the connection pool).
    let tasks: Vec<BoxFuture<MacroResult>> = vec![
        Box::pin(fetch_csi300(client.clone(), start_date.clone(), end_date.clone())),
        Box::pin(fetch_northbound(client.clone(), start_date.clone(), end_date.clone())),
        Box::pin(fetch_margin(client.clone(), start_date.clone(), end_date.clone())),
        Box::pin(fetch_shibor(client.clone(), start_date.clone(), end_date.clone())),
        Box::pin(fetch_cgb_10y(client.clone(), start_date.clone(), end_date.clone())),
        Box::pin(fetch_international()),
    ];

    let results = futures_util::future::join_all(tasks).await;

    let mut ok_count = 0u32;
    let mut fail_count = 0u32;

    for result in results {
        match result {
            Ok(entries) => {
                for (indicator, value, extra) in entries {
                    if let Err(e) =
                        macro_cache::save_macro_cache(&indicator, value, extra.as_deref(), "scheduler")
                    {
                        log::warn!("macro_refresh: failed to save {indicator}: {e}");
                        fail_count += 1;
                    } else {
                        ok_count += 1;
                    }
                }
            }
            Err(e) => {
                log::warn!("macro_refresh: batch failed: {e}");
                fail_count += 1;
            }
        }
    }

    Ok(format!(
        "macro_refresh complete: {ok_count} saved, {fail_count} failed"
    ))
}

// ---------------------------------------------------------------------------
// Tushare-based indicators
// ---------------------------------------------------------------------------

/// csi300_close + csi300_vol20 from Tushare daily bars.
async fn fetch_csi300(
    client: TushareClient,
    start_date: String,
    end_date: String,
) -> MacroResult {
    let mut bars = client
        .daily("000300.SH", &start_date, &end_date)
        .await
        .map_err(|e| format!("csi300 daily: {e}"))?;

    // daily() returns ascending; reverse to newest-first
    bars.reverse();

    if bars.is_empty() {
        return Err("csi300: no data".into());
    }

    let latest_close = bars[0].close;

    // 20-day volatility (annualized)
    let vol20 = if bars.len() >= 21 {
        let returns: Vec<f64> = bars
            .windows(2)
            .take(20)
            .filter_map(|w| {
                if w[1].close > 0.0 {
                    Some((w[0].close - w[1].close) / w[1].close)
                } else {
                    None
                }
            })
            .collect();
        if returns.is_empty() {
            None
        } else {
            let mean = returns.iter().sum::<f64>() / returns.len() as f64;
            let var = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
            Some(var.sqrt() * 252.0_f64.sqrt() * 100.0)
        }
    } else {
        None
    };

    let mut entries = vec![
        ("csi300_close".to_string(), Some(latest_close), None),
    ];
    if let Some(v) = vol20 {
        entries.push(("csi300_vol20".to_string(), Some(v), None));
    }
    Ok(entries)
}

/// northbound_net from Tushare moneyflow_hsgt.
async fn fetch_northbound(
    client: TushareClient,
    start_date: String,
    end_date: String,
) -> MacroResult {
    let flows = client
        .moneyflow_hsgt(&start_date, &end_date)
        .await
        .map_err(|e| format!("northbound moneyflow_hsgt: {e}"))?;

    // Latest entry's net_money (unit: 亿元)
    let latest = flows
        .iter()
        .max_by_key(|f| &f.trade_date)
        .ok_or("northbound: no data")?;

    Ok(vec![(
        "northbound_net".to_string(),
        Some(latest.net_money),
        Some(serde_json::json!({
            "trade_date": latest.trade_date,
            "north_money": latest.north_money,
            "south_money": latest.south_money,
        }).to_string()),
    )])
}

/// margin_balance from Tushare margin_detail.
async fn fetch_margin(
    client: TushareClient,
    start_date: String,
    end_date: String,
) -> MacroResult {
    let details = client
        .margin_detail(&start_date, &end_date)
        .await
        .map_err(|e| format!("margin margin_detail: {e}"))?;

    // Latest entry's rzye (融资余额, unit: 元)
    let latest = details
        .iter()
        .max_by_key(|d| &d.trade_date)
        .ok_or("margin: no data")?;

    Ok(vec![(
        "margin_balance".to_string(),
        Some(latest.rzye),
        Some(serde_json::json!({
            "trade_date": latest.trade_date,
            "rzmre": latest.rzmre,
            "rzche": latest.rzche,
        }).to_string()),
    )])
}

/// shibor_on from Tushare shibor.
async fn fetch_shibor(
    client: TushareClient,
    start_date: String,
    end_date: String,
) -> MacroResult {
    let rates = client
        .shibor(&start_date, &end_date)
        .await
        .map_err(|e| format!("shibor: {e}"))?;

    let latest = rates
        .iter()
        .max_by_key(|s| &s.date)
        .ok_or("shibor: no data")?;

    Ok(vec![(
        "shibor_on".to_string(),
        Some(latest.on),
        Some(serde_json::json!({
            "date": latest.date,
            "w1": latest.w1,
            "m1": latest.m1,
            "m3": latest.m3,
        }).to_string()),
    )])
}

/// cgb_10y from Tushare cn_bond_yield.
async fn fetch_cgb_10y(
    client: TushareClient,
    start_date: String,
    end_date: String,
) -> MacroResult {
    let yields = client
        .cn_bond_yield(&start_date, &end_date)
        .await
        .map_err(|e| format!("cgb_10y cn_bond_yield: {e}"))?;

    // Filter for 10Y bond (ts_code typically "10" or "10Y")
    let latest = yields
        .iter()
        .find(|y| y.ts_code.contains("10"))
        .or_else(|| yields.last())
        .ok_or("cgb_10y: no data")?;

    Ok(vec![(
        "cgb_10y".to_string(),
        Some(latest.yield_10y),
        Some(serde_json::json!({
            "ts_code": latest.ts_code,
        }).to_string()),
    )])
}

// ---------------------------------------------------------------------------
// Yahoo Finance international indicators
// ---------------------------------------------------------------------------

/// Fetch VIX, TNX, DXY, Gold, Oil, USDCNY from Yahoo Finance.
async fn fetch_international() -> MacroResult {
    let client = crate::invest::international::InternationalClient::new();

    let symbols: &[(&str, &str)] = &[
        ("^VIX", "vix"),
        ("^TNX", "tnx"),
        ("DX-Y.NYB", "dxy"),
        ("GC=F", "gold"),
        ("CL=F", "oil"),
        ("USDCNY=X", "usdcny"),
    ];

    let futures: Vec<_> = symbols
        .iter()
        .map(|(yahoo_sym, indicator)| {
            let client = &client;
            let yahoo_sym = yahoo_sym.to_string();
            let indicator = indicator.to_string();
            async move {
                match client.fetch_yahoo_quote(&yahoo_sym).await {
                    Ok(quote) => Ok((
                        indicator,
                        Some(quote.price),
                        Some(serde_json::json!({
                            "change_pct": quote.change_pct,
                            "previous_close": quote.previous_close,
                        }).to_string()),
                    )),
                    Err(e) => Err(format!("yahoo {yahoo_sym}: {e}")),
                }
            }
        })
        .collect();

    let results = futures_util::future::join_all(futures).await;

    let mut entries = Vec::new();
    for result in results {
        match result {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                log::warn!("macro_refresh: {e}");
            }
        }
    }

    if entries.is_empty() {
        return Err("international: all Yahoo fetches failed".into());
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_indicators_count() {
        assert_eq!(macro_cache::ALL_INDICATORS.len(), 12);
    }
}
