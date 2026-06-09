//! Invest 统计日期工具。
//!
//! 凌晨 05:00 之前运行的任务归属前一天的统计日期。
//! 例如 6 月 9 日凌晨 3 点 → 统计日期 = 2026-06-08。

use chrono::{Local, NaiveDate, Timelike};

/// Invest 统计截止小时：每天 05:00 之前归属前一天。
pub const INVEST_DATE_CUTOFF_HOUR: u32 = 5;

/// 内部共享：获取经截止时间调整后的 `NaiveDate`。
/// 05:00 之前返回昨天，05:00 及之后返回今天。
fn invest_date_naive() -> NaiveDate {
    let now = Local::now();
    if now.hour() < INVEST_DATE_CUTOFF_HOUR {
        (now - chrono::Duration::days(1)).naive_local().date()
    } else {
        now.naive_local().date()
    }
}

/// 返回 invest 统计日期（`YYYY-MM-DD` 格式）。
///
/// 05:00 之前返回昨天，05:00 及之后返回今天。
pub fn get_invest_date() -> String {
    invest_date_naive().format("%Y-%m-%d").to_string()
}

/// 返回 invest 统计日期（`YYYYMMDD` 紧凑格式）。
///
/// 05:00 之前返回昨天，05:00 及之后返回今天。
pub fn get_invest_date_compact() -> String {
    invest_date_naive().format("%Y%m%d").to_string()
}

/// 返回 invest 统计日期的 `NaiveDate`。
///
/// 05:00 之前返回昨天，05:00 及之后返回今天。
pub fn get_invest_naive_date() -> NaiveDate {
    invest_date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invest_date_format_yyyy_mm_dd() {
        let d = get_invest_date();
        assert_eq!(d.len(), 10, "expected YYYY-MM-DD length");
        assert!(d.contains('-'), "expected hyphen separator");
        let parts: Vec<&str> = d.split('-').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].len(), 4);
        assert_eq!(parts[1].len(), 2);
        assert_eq!(parts[2].len(), 2);
    }

    #[test]
    fn invest_date_compact_format() {
        let d = get_invest_date_compact();
        assert_eq!(d.len(), 8, "expected YYYYMMDD length");
        assert!(!d.contains('-'));
        assert!(d.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn invest_naive_date_is_valid() {
        let d = get_invest_naive_date();
        let s = d.format("%Y-%m-%d").to_string();
        assert_eq!(s.len(), 10);
    }

    #[test]
    fn invest_date_consistency() {
        let date_str = get_invest_date();
        let naive = get_invest_naive_date();
        assert_eq!(date_str, naive.format("%Y-%m-%d").to_string());
    }

    #[test]
    fn invest_date_compact_consistency() {
        let compact = get_invest_date_compact();
        let naive = get_invest_naive_date();
        assert_eq!(compact, naive.format("%Y%m%d").to_string());
    }
}
