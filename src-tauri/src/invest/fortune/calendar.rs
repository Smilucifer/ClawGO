//! 干支历法计算：公历日期 → 天干地支。
//!
//! 采用「儒略日数 + 偏移」的纯公式，不依赖任何外部库。
//! 基准锚点：2026-07-10 为乙酉日、2026-07-09 为甲申日
//! （偏移常量 [`GANZHI_OFFSET`] = 49 由此标定）。

/// 十天干。
pub const STEMS: [&str; 10] = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];

/// 十二地支。
pub const BRANCHES: [&str; 12] = [
    "子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥",
];

/// 儒略日数 → 干支索引的偏移，使 (JDN + OFFSET) % 60 对齐真实干支。
const GANZHI_OFFSET: i64 = 49;

/// 公历 (year, month, day) → 儒略日数（Fliegel-Van Flandern 公式）。
pub fn julian_day_number(year: i64, month: i64, day: i64) -> i64 {
    let a = (14 - month) / 12;
    let y = year + 4800 - a;
    let m = month + 12 * a - 3;
    day + (153 * m + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045
}

/// 公历日期 → 干支索引 (0..60)，甲子=0、癸亥=59。
pub fn ganzhi_index(year: i64, month: i64, day: i64) -> usize {
    let raw = (julian_day_number(year, month, day) + GANZHI_OFFSET) % 60;
    raw.rem_euclid(60) as usize
}

/// 公历日期 → (天干, 地支) 字符串对。
pub fn ganzhi(year: i64, month: i64, day: i64) -> (&'static str, &'static str) {
    let idx = ganzhi_index(year, month, day);
    (STEMS[idx % 10], BRANCHES[idx % 12])
}

/// 公历日期 → 干支合并字符串，如 "乙酉"。
pub fn ganzhi_str(year: i64, month: i64, day: i64) -> String {
    let (s, b) = ganzhi(year, month, day);
    format!("{s}{b}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_anchor_dates_map_to_correct_ganzhi() {
        // 真实历法锚点。
        assert_eq!(ganzhi_str(2026, 7, 10), "乙酉");
        assert_eq!(ganzhi_str(2026, 7, 9), "甲申");
        assert_eq!(ganzhi_str(2026, 7, 8), "癸未");
        assert_eq!(ganzhi_str(2026, 7, 7), "壬午");
        assert_eq!(ganzhi_str(2026, 7, 2), "丁丑");
        assert_eq!(ganzhi_str(2026, 7, 3), "戊寅");
        assert_eq!(ganzhi_str(2026, 6, 29), "甲戌");
        assert_eq!(ganzhi_str(2026, 6, 30), "乙亥");
    }

    #[test]
    fn ganzhi_cycles_every_sixty_days() {
        // 连续 60 天后干支应回到同一个。
        for (y, m, d) in [(2026, 7, 10), (2025, 1, 1)] {
            let base = ganzhi_index(y, m, d);
            let jdn60 = julian_day_number(y, m, d) + 60;
            // 反解 60 天后：直接用 index 公式验证周期性。
            let after = ((jdn60 + GANZHI_OFFSET) % 60).rem_euclid(60) as usize;
            assert_eq!(base, after, "干支应以 60 天为周期");
        }
    }

    #[test]
    fn index_is_always_in_range() {
        for d in 1..=28 {
            let idx = ganzhi_index(2026, 2, d);
            assert!(idx < 60);
        }
    }
}
