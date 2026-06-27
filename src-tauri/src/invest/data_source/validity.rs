//! 数据源取数结果的判空规则。
//!
//! 用户要求："实测取不到或为 0 则降级"。判空函数返回 false 即触发源链降级。

/// 默认数值判空：None / 0.0 / 非有限（NaN/Inf）视为无效。
/// 适用于"0 即异常"的字段（收益率、价格、成交额等）。
pub fn is_valid_number(v: &Option<f64>) -> bool {
    matches!(v, Some(x) if *x != 0.0 && x.is_finite())
}

/// 宽松判空：仅 None / 非有限（NaN/Inf）视为无效，**0.0 视为合法**。
/// 适用于"0 是正常平衡态"的字段（北向资金净流入平盘、净额持平等），
/// 避免把合法的 0 误判为缺失而触发不必要的源链降级。
pub fn is_present_finite(v: &Option<f64>) -> bool {
    matches!(v, Some(x) if x.is_finite())
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

    #[test]
    fn present_finite_accepts_zero() {
        // 宽松判空：0 是合法平衡态（如北向资金平盘），不应触发降级。
        assert!(is_present_finite(&Some(0.0)));
        assert!(is_present_finite(&Some(-42.0)));
        assert!(is_present_finite(&Some(1.408)));
    }

    #[test]
    fn present_finite_rejects_none_and_nonfinite() {
        assert!(!is_present_finite(&None));
        assert!(!is_present_finite(&Some(f64::NAN)));
        assert!(!is_present_finite(&Some(f64::INFINITY)));
    }
}
