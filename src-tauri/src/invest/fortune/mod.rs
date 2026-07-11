//! 每日盈记（openInvest 娱乐子系统）。
//!
//! 按天干地支记录每日股市收益率，累积后发掘干支与收益的规律。
//! - [`calendar`] 公历 → 干支纯公式；
//! - [`stats`] 层分/综合分评分算法（常数标定自真实数据）。

pub mod aggregate;
pub mod calendar;
pub mod reading;
pub mod stats;
