//! Scheduler job: refresh the 15 canonical macro indicators in macro_cache.
//!
//! Runs on `*/15 8-22 * * 1-5` (every 15 minutes during 8-22 on weekdays).
//! Partial failure strategy: failed indicators keep stale data, logged as warn.
//!
//! Data sources: Tushare (primary), Tencent Finance (CSI300 fallback + market volume),
//! AkShare (bond yield fallback + limit up/down stats), Yahoo Finance (international).

use crate::storage::invest::macro_cache;
use crate::tushare::client::TushareClient;
use std::future::Future;
use std::pin::Pin;

type MacroEntry = (String, Option<f64>, Option<String>);
type MacroResult = Result<Vec<MacroEntry>, String>;
type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Refresh all 15 macro indicators. Called from the scheduler runner.
///
/// Each indicator group is fetched independently. On failure, the existing cache
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
        Box::pin(fetch_market_stats()),
        Box::pin(fetch_two_market_volume()),
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
///
/// Falls back to Tencent Finance K-line API when Tushare fails or returns empty.
async fn fetch_csi300(
    client: TushareClient,
    start_date: String,
    end_date: String,
) -> MacroResult {
    match client.daily("000300.SH", &start_date, &end_date).await {
        Ok(bars) if !bars.is_empty() => {
            let latest_close = bars[0].close;
            let closes: Vec<f64> = bars.iter().take(21).map(|b| b.close).collect();
            let vol20 = crate::tencent_quotes::compute_vol20(&closes);

            let mut entries = vec![
                ("csi300_close".to_string(), Some(latest_close), None),
            ];
            if let Some(v) = vol20 {
                entries.push(("csi300_vol20".to_string(), Some(v), None));
            }
            Ok(entries)
        }
        Ok(_) => {
            // Tushare succeeded but returned no bars (holiday/weekend) — still try fallback
            log::info!("macro_refresh: csi300 Tushare returned empty, trying Tencent fallback");
            csi300_tencent_fallback().await
        }
        Err(e) => {
            log::warn!("macro_refresh: csi300 Tushare error: {e}, falling back to Tencent");
            csi300_tencent_fallback().await
        }
    }
}

/// Tencent Finance fallback for CSI300 close + vol20.
async fn csi300_tencent_fallback() -> MacroResult {
    let http = reqwest::Client::new();
    let kline = crate::tencent_quotes::fetch_csi300_kline(&http, 25).await
        .map_err(|e| format!("csi300 tencent fallback: {e}"))?;

    let mut entries = vec![
        ("csi300_close".to_string(), Some(kline.close), None),
    ];
    if let Some(v) = kline.vol20 {
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
///
/// Falls back to AkShare (via Python RPC) when Tushare fails or returns empty.
async fn fetch_cgb_10y(
    client: TushareClient,
    start_date: String,
    end_date: String,
) -> MacroResult {
    match client.cn_bond_yield(&start_date, &end_date).await {
        Ok(yields) if !yields.is_empty() => {
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
        Ok(_) => {
            log::info!("macro_refresh: cgb_10y Tushare returned empty, trying AkShare fallback");
            cgb_10y_akshare_fallback().await
        }
        Err(e) => {
            log::warn!("macro_refresh: cgb_10y Tushare error: {e}, falling back to AkShare");
            cgb_10y_akshare_fallback().await
        }
    }
}

/// AkShare fallback for 10Y government bond yield.
async fn cgb_10y_akshare_fallback() -> MacroResult {
    let client = crate::invest::international::InternationalClient::from_settings();
    let bond = client.fetch_akshare_bond_yield().await
        .map_err(|e| format!("cgb_10y akshare fallback: {e}"))?;

    if bond.yield_10y <= 0.0 {
        return Err("cgb_10y akshare: invalid yield value".into());
    }

    Ok(vec![(
        "cgb_10y".to_string(),
        Some(bond.yield_10y),
        Some(serde_json::json!({"date": bond.date}).to_string()),
    )])
}

// ---------------------------------------------------------------------------
// Yahoo Finance international indicators
// ---------------------------------------------------------------------------

/// Fetch VIX, TNX, DXY, Gold, Oil, USDCNY from Yahoo Finance.
///
/// Requests are sequential with 500ms spacing to avoid Yahoo's rate limiter (429).
async fn fetch_international() -> MacroResult {
    let client = crate::invest::international::InternationalClient::from_settings();

    let symbols: &[(&str, &str)] = &[
        ("^VIX", "vix"),
        ("^TNX", "tnx"),
        ("DX-Y.NYB", "dxy"),
        ("GC=F", "gold"),
        ("CL=F", "oil"),
        ("USDCNY=X", "usdcny"),
    ];

    let mut entries = Vec::new();
    for (yahoo_sym, indicator) in symbols.iter() {
        match client.fetch_yahoo_quote(yahoo_sym).await {
            Ok(quote) => entries.push((
                indicator.to_string(),
                Some(quote.price),
                Some(
                    serde_json::json!({
                        "change_pct": quote.change_pct,
                        "previous_close": quote.previous_close,
                    })
                    .to_string(),
                ),
            )),
            Err(e) => {
                log::warn!("macro_refresh: yahoo {yahoo_sym}: {e}");
            }
        }
    }

    if entries.is_empty() {
        return Err("international: all Yahoo fetches failed".into());
    }

    Ok(entries)
}

// ---------------------------------------------------------------------------
// A-share market statistics (AkShare)
// ---------------------------------------------------------------------------

/// Fetch limit-up and limit-down stock counts from AkShare.
///
/// Uses Python RPC bridge to call `akshare_market.market_stats`.
/// Returns limit_up_count and limit_down_count as two separate entries.
async fn fetch_market_stats() -> MacroResult {
    let client = crate::invest::international::InternationalClient::from_settings();
    let today = chrono::Local::now().format("%Y%m%d").to_string();

    let stats = client.fetch_akshare_market_stats(&today).await
        .map_err(|e| format!("market_stats: {e}"))?;

    Ok(vec![
        ("limit_up_count".to_string(), Some(stats.limit_up_count as f64), None),
        ("limit_down_count".to_string(), Some(stats.limit_down_count as f64), None),
    ])
}

// ---------------------------------------------------------------------------
// Two-market volume (Tencent Finance)
// ---------------------------------------------------------------------------

/// Fetch Shanghai + Shenzhen total trading volume from Tencent Finance.
///
/// Queries index quotes for sh000001 (上证指数) and sz399001 (深证成指)
/// concurrently, then sums their `amount` fields.
async fn fetch_two_market_volume() -> MacroResult {
    let http = reqwest::Client::new();

    let (sh, sz) = tokio::try_join!(
        crate::tencent_quotes::fetch_index_quote(&http, "sh000001"),
        crate::tencent_quotes::fetch_index_quote(&http, "sz399001"),
    )?;

    // amount is in yuan; convert to 亿元 for consistency with other indicators
    let total_yi = (sh.amount + sz.amount) / 1e8;

    Ok(vec![(
        "two_market_volume".to_string(),
        Some(total_yi),
        None,
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_indicators_count() {
        assert_eq!(macro_cache::ALL_INDICATORS.len(), 15);
    }
}
