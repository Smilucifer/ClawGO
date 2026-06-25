//! 指标类别 → 源链优先级登记表。
//!
//! 这是**唯一**决定取数源优先级的地方。调整顺序或新增源只改这里。

use super::SourceId;

/// 指标类别。决定源链构成。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// 行情类（K线/实时报价/指数行情）。miniQMT 开关影响此类。
    Quote,
    /// 资金面（北向/两融）。
    Capital,
    /// 宏观面（shibor/国债）。
    Macro,
    /// tushare 独有（moneyflow_dc/report_rc），无降级意义。
    TushareOnly,
    /// 海外（VIX/美债/黄金/原油/汇率）。Yahoo 专属。
    Overseas,
}

/// 返回某类别的有序源链。`miniqmt_on` 仅影响 Quote 类。
pub fn chain_for(category: Category, miniqmt_on: bool) -> Vec<SourceId> {
    use SourceId::*;
    match category {
        Category::Quote => {
            if miniqmt_on {
                vec![MiniQmt, Tushare, Tencent]
            } else {
                vec![Tushare, Tencent]
            }
        }
        Category::Capital => vec![Tushare, Akshare],
        Category::Macro => vec![Tushare, Akshare],
        Category::TushareOnly => vec![Tushare],
        Category::Overseas => vec![Yahoo],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invest::data_source::SourceId;

    #[test]
    fn quote_prepends_miniqmt_when_on() {
        assert_eq!(
            chain_for(Category::Quote, true),
            vec![SourceId::MiniQmt, SourceId::Tushare, SourceId::Tencent]
        );
    }

    #[test]
    fn quote_omits_miniqmt_when_off() {
        assert_eq!(
            chain_for(Category::Quote, false),
            vec![SourceId::Tushare, SourceId::Tencent]
        );
    }

    #[test]
    fn capital_ignores_miniqmt_flag() {
        assert_eq!(chain_for(Category::Capital, true), vec![SourceId::Tushare, SourceId::Akshare]);
        assert_eq!(chain_for(Category::Capital, false), vec![SourceId::Tushare, SourceId::Akshare]);
    }

    #[test]
    fn tushare_only_is_single_source() {
        assert_eq!(chain_for(Category::TushareOnly, true), vec![SourceId::Tushare]);
    }

    #[test]
    fn overseas_is_yahoo_only() {
        assert_eq!(chain_for(Category::Overseas, true), vec![SourceId::Yahoo]);
    }
}
