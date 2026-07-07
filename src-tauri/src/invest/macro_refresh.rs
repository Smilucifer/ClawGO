//! Scheduler job: refresh the 19 canonical macro indicators in macro_cache.
//!
//! Runs on `*/15 8-22 * * 1-5` (every 15 minutes during 8-22 on weekdays).
//! Partial failure strategy: failed indicators keep stale data, logged as warn.
//!
//! Data sources: Tushare (primary), Tencent Finance (Shanghai Composite fallback + market volume),
//! AkShare (bond yield fallback + limit up/down stats + advance/decline), Yahoo Finance (international).

use crate::storage::invest::macro_cache;
use crate::tushare::client::TushareClient;
use std::future::Future;
use std::pin::Pin;

/// (indicator, value, extra_json, source)
type MacroEntry = (String, Option<f64>, Option<String>, &'static str);
type MacroResult = Result<Vec<MacroEntry>, String>;
type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Refresh all 19 macro indicators. Called from the scheduler runner.
///
/// Each indicator group is fetched independently. On failure, the existing cache
/// entry is preserved and a warning is logged.
pub async fn refresh_macro_cache(client: &TushareClient) -> Result<String, String> {
    let end_date = chrono::Local::now().format("%Y%m%d").to_string();
    let start_date =
        (chrono::Local::now() - chrono::Duration::days(90)).format("%Y%m%d").to_string();

    // 行情类编排：是否启用 miniQMT 优先源（默认关闭，行为与改造前一致）。
    let miniqmt_on = crate::storage::settings::load().user.invest_miniqmt_enabled;

    // Clone the client for each task to satisfy 'static requirement on BoxFuture.
    // reqwest::Client is cheap to clone (shares the connection pool).
    let tasks: Vec<BoxFuture<MacroResult>> = vec![
        Box::pin(fetch_sh_composite(
            client.clone(),
            start_date.clone(),
            end_date.clone(),
            miniqmt_on,
        )),
        Box::pin(fetch_northbound(client.clone(), start_date.clone(), end_date.clone())),
        Box::pin(fetch_margin(client.clone(), start_date.clone(), end_date.clone())),
        Box::pin(fetch_shibor(client.clone(), start_date.clone(), end_date.clone())),
        Box::pin(fetch_cgb_10y(client.clone(), start_date.clone(), end_date.clone())),
        Box::pin(fetch_international()),
        Box::pin(fetch_breadth(miniqmt_on)),
        Box::pin(fetch_two_market_volume()),
    ];

    let results = futures_util::future::join_all(tasks).await;

    let mut ok_count = 0u32;
    let mut fail_count = 0u32;

    for result in results {
        match result {
            Ok(entries) => {
                for (indicator, value, extra, source) in entries {
                    if let Err(e) =
                        macro_cache::save_macro_cache(&indicator, value, extra.as_deref(), source)
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

    // 批次戳：供全局宏观判断的 based_on_data_version 比对（§8.2-G）。
    // 广度与大盘数据同批刷新，两行 fetched_at 均为当刻。
    // 广度真实源：回读 advance_count 本轮实际写入的 source（降级时为 akshare），
    // 避免哨兵按配置意图误标（miniqmt_on=true 但运行时降级的情形）。
    let breadth_source = macro_cache::load_macro_cache("advance_count")
        .ok()
        .flatten()
        .map(|e| e.source)
        .unwrap_or_else(|| "unknown".to_string());
    let _ = macro_cache::save_macro_cache("_breadth_batch", None, None, &breadth_source);
    let _ = macro_cache::save_macro_cache("_macro_batch", None, None, "macro_refresh");

    Ok(format!(
        "macro_refresh complete: {ok_count} saved, {fail_count} failed"
    ))
}

// ---------------------------------------------------------------------------
// Tushare-based indicators
// ---------------------------------------------------------------------------

/// sh_composite_close + sh_composite_vol20 via 编排层 Quote 链。
///
/// `miniqmt_on=true` → `[MiniQmt, Tushare, Tencent]`；否则 `[Tushare, Tencent]`，
/// 与改造前的 tushare-primary + tencent-fallback 行为完全一致。
/// 编排闭包返回 `(close, Option<vol20>)`，判空仅看 close 有效（!=0 且有限）。
async fn fetch_sh_composite(
    client: TushareClient,
    start_date: String,
    end_date: String,
    miniqmt_on: bool,
) -> MacroResult {
    use crate::invest::data_source::{
        fetch_with_chain,
        registry::{chain_for, Category},
        SourceId,
    };

    let chain = chain_for(Category::Quote, miniqmt_on); // on: [MiniQmt,Tushare,Tencent] / off: [Tushare,Tencent]

    let fetched = fetch_with_chain(
        &chain,
        // Require BOTH close and vol20: a source that returns close but vol20=None
        // (e.g. MiniQmt with insufficient bars) must NOT short-circuit the chain and
        // block Tushare fallback, which leaves vol20 missing and drifts source/age
        // between the two rows (H-data-2).
        |(close, vol20): &(f64, Option<f64>)| {
            *close != 0.0 && close.is_finite() && matches!(vol20, Some(v) if v.is_finite())
        },
        |source| {
            let client = client.clone();
            let (sd, ed) = (start_date.clone(), end_date.clone());
            async move {
                match source {
                    SourceId::MiniQmt => {
                        let intl =
                            crate::invest::international::InternationalClient::from_settings();
                        let h = intl.fetch_xtdata_health().await?;
                        if !h.available {
                            return Err(format!("miniqmt offline: {}", h.reason));
                        }
                        let bars = intl.fetch_xtdata_kline("000001.SH", "1d", 25).await?;
                        if bars.is_empty() {
                            return Err("miniqmt: empty kline".into());
                        }
                        // miniQMT 升序（最新在末），用 .rev().take(21) 取最近窗口。
                        let closes: Vec<f64> =
                            bars.iter().rev().take(21).map(|b| b.close).collect();
                        let vol20 = crate::tencent_quotes::compute_vol20(&closes);
                        let latest_close = bars.last().unwrap().close;
                        Ok((latest_close, vol20))
                    }
                    SourceId::Tushare => {
                        let bars = client.daily("000001.SH", &sd, &ed).await?;
                        if bars.is_empty() {
                            return Err("sh_composite: tushare empty".into());
                        }
                        // tushare 日线降序（最新在前），bars[0] 为最新，take(21) 即最近窗口。
                        let latest_close = bars[0].close;
                        let closes: Vec<f64> = bars.iter().take(21).map(|b| b.close).collect();
                        let vol20 = crate::tencent_quotes::compute_vol20(&closes);
                        Ok((latest_close, vol20))
                    }
                    SourceId::Tencent => {
                        let http = reqwest::Client::new();
                        let kline =
                            crate::tencent_quotes::fetch_index_kline(&http, "sh000001", 25)
                                .await?;
                        Ok((kline.close, kline.vol20))
                    }
                    _ => Err("sh_composite: unsupported source".into()),
                }
            }
        },
    )
    .await?;

    let (close, vol20) = fetched.value;
    let source = fetched.source.as_str();
    let mut entries = vec![("sh_composite_close".to_string(), Some(close), None, source)];
    if let Some(v) = vol20 {
        entries.push(("sh_composite_vol20".to_string(), Some(v), None, source));
    }
    Ok(entries)
}

/// northbound_net via 编排层 Capital 链（tushare → akshare）。
///
/// tushare 金额字段单位为百万元；换算到亿元需 ÷ 100（1 亿元 = 100 百万元）。
/// net_money 字段已于 2024-08 停更，改用 north_money（= 沪股通 + 深股通）。
/// AkShare 北向源第一期未实现（Capital 链降级路径预留但暂为 inert）。
async fn fetch_northbound(
    client: TushareClient,
    start_date: String,
    end_date: String,
) -> MacroResult {
    use crate::invest::data_source::{
        fetch_with_chain,
        registry::{chain_for, Category},
        validity::is_present_finite,
        SourceId,
    };

    /// tushare 百万元 → 亿元换算系数。
    const MILLION_TO_YI: f64 = 100.0;

    let chain = chain_for(Category::Capital, false); // [Tushare, Akshare]
    let fetched = fetch_with_chain(
        &chain,
        // 北向资金净流入 0 是合法平盘态，用宽松判空避免误降级。
        |v: &Option<f64>| is_present_finite(v),
        |source| {
            let client = client.clone();
            let (sd, ed) = (start_date.clone(), end_date.clone());
            async move {
                match source {
                    SourceId::Tushare => {
                        let flows = client.moneyflow_hsgt(&sd, &ed).await?;
                        let latest = flows
                            .iter()
                            .max_by_key(|f| &f.trade_date)
                            .ok_or("northbound: no data")?;
                        // north_money 单位百万元，转亿元。
                        Ok(Some(latest.north_money / MILLION_TO_YI))
                    }
                    SourceId::Akshare => {
                        Err("northbound: akshare source not implemented (phase 2)".to_string())
                    }
                    _ => Err("northbound: unsupported source".to_string()),
                }
            }
        },
    )
    .await?;

    Ok(vec![(
        "northbound_net".to_string(),
        fetched.value,
        Some(serde_json::json!({ "unit": "亿元" }).to_string()),
        fetched.source.as_str(),
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

    // Reject a zero/non-finite balance: margin balance is never legitimately 0, so a 0
    // here means a missing/garbage field that must not be persisted as real data (H-data-3).
    if latest.rzye == 0.0 || !latest.rzye.is_finite() {
        return Err(format!(
            "margin: invalid rzye {} for trade_date {}",
            latest.rzye, latest.trade_date
        ));
    }

    Ok(vec![(
        "margin_balance".to_string(),
        Some(latest.rzye),
        Some(serde_json::json!({
            "trade_date": latest.trade_date,
            "rzmre": latest.rzmre,
            "rzche": latest.rzche,
        }).to_string()),
        "tushare",
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

    // Reject a zero/non-finite overnight rate: SHIBOR is never legitimately 0, so a 0
    // means a missing/garbage field that must not be persisted as real data (H-data-3).
    if latest.on == 0.0 || !latest.on.is_finite() {
        return Err(format!(
            "shibor: invalid overnight rate {} for date {}",
            latest.on, latest.date
        ));
    }

    Ok(vec![(
        "shibor_on".to_string(),
        Some(latest.on),
        Some(serde_json::json!({
            "date": latest.date,
            "w1": latest.w1,
            "m1": latest.m1,
            "m3": latest.m3,
        }).to_string()),
        "tushare",
    )])
}

/// cgb_10y via 编排层 Macro 链（tushare → akshare）。
///
/// tushare `cn_bond_yield` 经代理常返回 502；失败或为 0 自动降级 AkShare。
async fn fetch_cgb_10y(
    client: TushareClient,
    start_date: String,
    end_date: String,
) -> MacroResult {
    use crate::invest::data_source::{
        fetch_with_chain,
        registry::{chain_for, Category},
        validity::is_valid_number,
        SourceId,
    };

    let chain = chain_for(Category::Macro, false); // [Tushare, Akshare]
    let fetched = fetch_with_chain(
        &chain,
        |v: &Option<f64>| is_valid_number(v),
        |source| {
            let client = client.clone();
            let (sd, ed) = (start_date.clone(), end_date.clone());
            async move {
                match source {
                    SourceId::Tushare => {
                        let rows = client.cn_bond_yield(&sd, &ed).await?;
                        let latest = rows
                            .iter()
                            .find(|y| y.ts_code.contains("10"))
                            .or_else(|| rows.last())
                            .ok_or("cgb_10y: no data")?;
                        Ok(Some(latest.yield_10y))
                    }
                    SourceId::Akshare => {
                        let intl =
                            crate::invest::international::InternationalClient::from_settings();
                        let bond = intl.fetch_akshare_bond_yield().await?;
                        if bond.yield_10y <= 0.0 {
                            return Err("cgb_10y akshare: invalid yield value".to_string());
                        }
                        Ok(Some(bond.yield_10y))
                    }
                    _ => Err("cgb_10y: unsupported source".to_string()),
                }
            }
        },
    )
    .await?;

    Ok(vec![(
        "cgb_10y".to_string(),
        fetched.value,
        Some(serde_json::json!({ "unit": "%" }).to_string()),
        fetched.source.as_str(),
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
    for (i, (yahoo_sym, indicator)) in symbols.iter().enumerate() {
        // 500ms spacing between requests to avoid Yahoo rate limiter (429).
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
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
                "yahoo",
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
// 市场广度（miniQMT 优先 + akshare 降级）
// ---------------------------------------------------------------------------

/// 市场广度同源采集：miniQMT 开启且在线 → 一次取全(涨/平/跌/涨跌停/涨幅>3%)；
/// 否则降级 akshare(仅涨跌家数 + 涨跌停，flat/up_over_3pct 缺失不写避免混源)。
async fn fetch_breadth(miniqmt_on: bool) -> MacroResult {
    let client = crate::invest::international::InternationalClient::from_settings();
    if miniqmt_on {
        match client.fetch_xtdata_breadth().await {
            Ok(b) if b.available && b.valid > 0 => {
                return Ok(vec![
                    ("advance_count".into(), Some(b.up as f64), None, "miniqmt"),
                    ("decline_count".into(), Some(b.down as f64), None, "miniqmt"),
                    ("flat_count".into(), Some(b.flat as f64), None, "miniqmt"),
                    ("limit_up_count".into(), Some(b.limit_up as f64), None, "miniqmt"),
                    ("limit_down_count".into(), Some(b.limit_down as f64), None, "miniqmt"),
                    ("up_over_3pct_count".into(), Some(b.up_over_3pct as f64), None, "miniqmt"),
                ]);
            }
            Ok(b) => log::warn!("breadth: miniqmt unavailable ({}), 降级 akshare", b.reason),
            Err(e) => log::warn!("breadth: miniqmt error ({e}), 降级 akshare"),
        }
    }
    // 降级：akshare 仅有涨跌家数 + 涨跌停；flat/up_over_3pct 不写（保留旧值/缺失）。
    let today = chrono::Local::now().format("%Y%m%d").to_string();
    let stats = client.fetch_akshare_market_stats(&today).await
        .map_err(|e| format!("market_stats: {e}"))?;
    let ad = client.fetch_akshare_advance_decline(&today).await
        .map_err(|e| format!("advance_decline: {e}"))?;
    Ok(vec![
        ("advance_count".into(), Some(ad.advance_count as f64), None, "akshare"),
        ("decline_count".into(), Some(ad.decline_count as f64), None, "akshare"),
        ("limit_up_count".into(), Some(stats.limit_up_count as f64), None, "akshare"),
        ("limit_down_count".into(), Some(stats.limit_down_count as f64), None, "akshare"),
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
        "tencent",
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_indicators_count() {
        // 17 + up_over_3pct_count + flat_count = 19
        assert_eq!(macro_cache::ALL_INDICATORS.len(), 19);
    }
}
