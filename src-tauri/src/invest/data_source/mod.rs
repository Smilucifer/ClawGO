//! 数据源编排层：按指标类别选源、判空降级、记录命中源。
//!
//! 只管"选源 + 降级 + 记录"，不碰各源内部字段解析与单位换算。

pub mod registry;
pub mod validity;

use std::future::Future;

/// 数据源标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceId {
    MiniQmt,
    Tushare,
    Akshare,
    Tencent,
}

impl SourceId {
    /// 写入 macro_cache.source 列的字符串标识。
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceId::MiniQmt => "miniqmt",
            SourceId::Tushare => "tushare",
            SourceId::Akshare => "akshare",
            SourceId::Tencent => "tencent",
        }
    }
}

/// 一次取数的结果，带命中源信息。
#[derive(Debug, Clone)]
pub struct Fetched<T> {
    pub value: T,
    pub source: SourceId,
}

/// 按源链依次尝试取数：成功且通过判空则返回；否则降级到下一个源。
///
/// - `chain`: 有序源链（来自 `registry::chain_for`）。
/// - `is_valid`: 判空函数，返回 false 触发降级。
/// - `try_source`: 对某个源发起取数的异步闭包。
pub async fn fetch_with_chain<T, F, Fut>(
    chain: &[SourceId],
    is_valid: impl Fn(&T) -> bool,
    try_source: F,
) -> Result<Fetched<T>, String>
where
    F: Fn(SourceId) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let mut last_err = String::from("empty source chain");
    for &source in chain {
        match try_source(source).await {
            Ok(value) if is_valid(&value) => {
                return Ok(Fetched { value, source });
            }
            Ok(_) => {
                log::debug!(
                    "data_source: {} returned invalid value, falling back",
                    source.as_str()
                );
                last_err = format!("{} returned invalid value", source.as_str());
            }
            Err(e) => {
                log::debug!("data_source: {} failed: {e}, falling back", source.as_str());
                last_err = format!("{}: {e}", source.as_str());
            }
        }
    }
    Err(format!("all sources exhausted: {last_err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn run(
        chain: Vec<SourceId>,
        behavior: impl Fn(SourceId) -> Result<f64, String>,
    ) -> Result<Fetched<f64>, String> {
        fetch_with_chain(
            &chain,
            |v: &f64| *v != 0.0,
            |s| {
                let r = behavior(s);
                async move { r }
            },
        )
        .await
    }

    #[tokio::test]
    async fn first_source_success() {
        let got = run(vec![SourceId::Tushare, SourceId::Akshare], |_| Ok(1.4))
            .await
            .unwrap();
        assert_eq!(got.source, SourceId::Tushare);
        assert_eq!(got.value, 1.4);
    }

    #[tokio::test]
    async fn falls_back_on_invalid() {
        let got = run(vec![SourceId::Tushare, SourceId::Akshare], |s| {
            if s == SourceId::Tushare {
                Ok(0.0)
            } else {
                Ok(1.7)
            }
        })
        .await
        .unwrap();
        assert_eq!(got.source, SourceId::Akshare);
        assert_eq!(got.value, 1.7);
    }

    #[tokio::test]
    async fn falls_back_on_error() {
        let got = run(vec![SourceId::Tushare, SourceId::Akshare], |s| {
            if s == SourceId::Tushare {
                Err("boom".into())
            } else {
                Ok(2.2)
            }
        })
        .await
        .unwrap();
        assert_eq!(got.source, SourceId::Akshare);
    }

    #[tokio::test]
    async fn all_fail_returns_err() {
        let r = run(vec![SourceId::Tushare, SourceId::Akshare], |_| {
            Err("x".into())
        })
        .await;
        assert!(r.is_err());
    }
}
