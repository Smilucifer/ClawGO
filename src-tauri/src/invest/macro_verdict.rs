//! 全局宏观判断 job:取数 → MACRO_GLOBAL_PROMPT → 写 macro_verdict 对象。
//! 赚钱效应档位由固定阈值规则确定性计算(§8.1 校准),LLM 仅写理由。

use chrono::{Timelike, Weekday};

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
