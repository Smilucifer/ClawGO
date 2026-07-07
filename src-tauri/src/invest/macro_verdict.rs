//! 全局宏观判断 job:取数 → MACRO_GLOBAL_PROMPT → 写 macro_verdict 对象。
//! 赚钱效应档位由固定阈值规则确定性计算(§8.1 校准),LLM 仅写理由。

use chrono::{Timelike, Weekday};
use crate::storage::invest::{macro_cache, macro_verdict as store};

pub const MONEY_EFFECT_COLD: &str = "cold";
pub const MONEY_EFFECT_CALM: &str = "calm";
pub const MONEY_EFFECT_ACTIVE: &str = "active";
pub const MONEY_EFFECT_HOT: &str = "hot";

/// 占比(涨幅>3%家数/有效家数 ×100)→ 赚钱效应档位。阈值来自 2 年校准(§8.1)。
pub fn money_effect_tier(ratio_pct: f64) -> &'static str {
    if ratio_pct <= 6.0 {
        MONEY_EFFECT_COLD
    } else if ratio_pct <= 9.0 {
        MONEY_EFFECT_CALM
    } else if ratio_pct <= 21.0 {
        MONEY_EFFECT_ACTIVE
    } else {
        MONEY_EFFECT_HOT
    }
}

/// 是否处于 A 股连续竞价交易时段(9:30–11:30 或 13:00–15:00 的工作日)。
pub fn is_trading_session(now_cst: chrono::NaiveTime, weekday: Weekday) -> bool {
    if matches!(weekday, Weekday::Sat | Weekday::Sun) {
        return false;
    }
    let mins = now_cst.hour() * 60 + now_cst.minute();
    (570..=690).contains(&mins) || (780..=900).contains(&mins)
}

/// 组装 MACRO_GLOBAL_PROMPT 的占位填充值(MA20/MA60/波动率趋势/广度串)。
#[allow(clippy::too_many_arguments)]
fn fill_global_prompt(
    ma20: Option<f64>, ma60: Option<f64>, vol20: Option<f64>,
    up: f64, flat: f64, down: f64, lu: f64, ld: f64, up3: f64, valid: f64,
) -> String {
    let fmt = |o: Option<f64>| o.map(|v| format!("{v:.2}")).unwrap_or_else(|| "N/A".into());
    // vol20 为年化波动率(×√252,见 spec B3)。25% 为经验分界,非严格校准;
    // 仅作 prompt 的粗趋势提示("偏高/平稳"),不参与档位规则计算,魔数可接受。
    let vol_trend = match vol20 {
        Some(v) if v > 25.0 => "偏高",
        Some(_) => "平稳",
        None => "N/A",
    };
    let ratio = if valid > 0.0 { up3 / valid * 100.0 } else { 0.0 };
    let breadth = format!(
        "涨{up:.0}/平{flat:.0}/跌{down:.0}，涨停{lu:.0}/跌停{ld:.0}，涨幅>3% {up3:.0}只(占比{ratio:.1}%)",
    );
    crate::invest::committee::roles::MACRO_GLOBAL_PROMPT
        .replace("{{ma20}}", &fmt(ma20))
        .replace("{{ma60}}", &fmt(ma60))
        .replace("{{vol20_trend}}", vol_trend)
        .replace("{{breadth}}", &breadth)
}

/// 读 committee_tuning.json → 生成 CLI --settings 路径(provider 路由)。
fn resolve_settings_path() -> Option<std::path::PathBuf> {
    let p = crate::storage::data_dir().join("invest").join("committee_tuning.json");
    let (provider, model) = std::fs::read_to_string(&p).ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| {
            (
                v["selectedProvider"].as_str().unwrap_or("default").to_string(),
                v["model"].as_str().unwrap_or("").to_string(),
            )
        })
        .unwrap_or_else(|| ("default".into(), String::new()));
    let model_opt = if model.is_empty() { None } else { Some(model.as_str()) };
    crate::invest::committee::cli_executor::write_committee_settings_json(&provider, model_opt)
        .ok()
        .flatten()
}

/// 取上证 60 日线算 MA20/MA60(数据不足返回 None)。走 international kline(miniQMT 优先)。
async fn fetch_sh_ma() -> (Option<f64>, Option<f64>) {
    let client = crate::invest::international::InternationalClient::from_settings();
    let bars = match client.fetch_xtdata_kline("000001.SH", "1d", 60).await {
        Ok(b) if b.len() >= 20 => b,
        _ => return (None, None),
    };
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
    let ma = |n: usize| {
        if closes.len() >= n {
            Some(closes.iter().rev().take(n).sum::<f64>() / n as f64)
        } else {
            None
        }
    };
    (ma(20), ma(60))
}

/// 全局宏观判断主流程。scheduler 与手动命令共用。
/// 非交易时段:不跑真广度,直接复用最近收盘定版(§8.2-H)。
pub async fn run_macro_verdict(manual: bool) -> Result<String, String> {
    use chrono::Datelike;
    // 时段门禁:非交易时段不重算,保留最近收盘定版。
    let cst = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let now = chrono::Utc::now().with_timezone(&cst);
    let t = chrono::NaiveTime::from_hms_opt(now.hour(), now.minute(), 0).unwrap();
    if !is_trading_session(t, now.weekday()) {
        log::info!("macro_verdict: 非交易时段(manual={manual}),复用最近收盘定版");
        return Ok("skipped: non-trading session, reused last verdict".into());
    }

    // 取广度(同源)。miniQMT 关/离线 → 广度缺失,赚钱效应"数据不足"。
    let client = crate::invest::international::InternationalClient::from_settings();
    let miniqmt_on = crate::storage::settings::load().user.invest_miniqmt_enabled;
    let breadth = if miniqmt_on {
        client.fetch_xtdata_breadth().await.ok()
    } else {
        None
    };
    let b = breadth.filter(|b| b.available && b.valid > 0);

    let (ma20, ma60) = fetch_sh_ma().await;
    let vol20 = macro_cache::load_macro_cache("sh_composite_vol20")
        .ok()
        .flatten()
        .and_then(|e| e.value);

    let (up, flat, down, lu, ld, up3, valid) = match &b {
        Some(x) => (
            x.up as f64, x.flat as f64, x.down as f64,
            x.limit_up as f64, x.limit_down as f64,
            x.up_over_3pct as f64, x.valid as f64,
        ),
        None => (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
    };

    let sys_prompt = fill_global_prompt(ma20, ma60, vol20, up, flat, down, lu, ld, up3, valid);
    let cli = crate::invest::committee::cli_executor::CliCommitteeExecutor::global()
        .ok_or("claude CLI not found")?;
    let settings = resolve_settings_path();
    let raw = cli.run_role(
        &sys_prompt, "请给出当前 A 股全局宏观判断。", 120,
        settings.as_deref(), None,
    ).await?;

    let parsed = crate::invest::committee::parser::parse_role_output(
        crate::invest::committee::roles::CommitteeRole::Macro, &raw, false,
    );

    // 赚钱效应档位:规则确定性算(数据不足时 None)。
    let money_effect = if valid > 0.0 {
        Some(money_effect_tier(up3 / valid * 100.0).to_string())
    } else {
        None
    };

    let verdict = store::MacroVerdict {
        signal: parsed.signal,
        strength: parsed.strength,
        market_phase: parsed.market_phase,
        money_effect,
        money_effect_reason: parsed.money_effect_reason,
        signal_reason: parsed.signal_reason,
        market_phase_reason: parsed.market_phase_reason,
        based_on_data_version: macro_cache::current_data_version()?,
        updated_at: String::new(), // save 时 DB 填 datetime('now')
    };
    store::save_verdict(&verdict)?;
    Ok(format!(
        "macro_verdict updated: signal={:?} money_effect={:?}",
        verdict.signal, verdict.money_effect,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveTime, Weekday};

    #[test]
    fn test_money_effect_tier() {
        assert_eq!(money_effect_tier(4.0), "cold");
        assert_eq!(money_effect_tier(6.0), "cold");
        assert_eq!(money_effect_tier(7.5), "calm");
        assert_eq!(money_effect_tier(15.0), "active");
        assert_eq!(money_effect_tier(25.0), "hot");
    }

    #[test]
    fn test_is_trading_session() {
        let wed = Weekday::Wed;
        assert!(is_trading_session(NaiveTime::from_hms_opt(10, 0, 0).unwrap(), wed));
        assert!(is_trading_session(NaiveTime::from_hms_opt(14, 30, 0).unwrap(), wed));
        assert!(!is_trading_session(NaiveTime::from_hms_opt(12, 0, 0).unwrap(), wed));
        assert!(!is_trading_session(NaiveTime::from_hms_opt(8, 0, 0).unwrap(), wed));
        assert!(!is_trading_session(NaiveTime::from_hms_opt(16, 0, 0).unwrap(), wed));
        assert!(!is_trading_session(NaiveTime::from_hms_opt(10, 0, 0).unwrap(), Weekday::Sat));
    }
}
