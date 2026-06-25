//! 数据源取数结果的判空规则。
//!
//! 用户要求："实测取不到或为 0 则降级"。判空函数返回 false 即触发源链降级。

/// 默认数值判空：None / 0.0 / 非有限（NaN/Inf）视为无效。
pub fn is_valid_number(v: &Option<f64>) -> bool {
    matches!(v, Some(x) if *x != 0.0 && x.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_is_invalid() {
        assert!(!is_valid_number(&None));
    }

    #[test]
    fn zero_is_invalid() {
        assert!(!is_valid_number(&Some(0.0)));
    }

    #[test]
    fn nan_is_invalid() {
        assert!(!is_valid_number(&Some(f64::NAN)));
    }

    #[test]
    fn inf_is_invalid() {
        assert!(!is_valid_number(&Some(f64::INFINITY)));
    }

    #[test]
    fn positive_is_valid() {
        assert!(is_valid_number(&Some(1.408)));
    }

    #[test]
    fn negative_is_valid() {
        // 负值（如净流出）是有效数据，不应降级。
        assert!(is_valid_number(&Some(-42.0)));
    }
}
