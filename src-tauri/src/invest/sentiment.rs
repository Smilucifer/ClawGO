//! 舆情采集接口层：调 Python 桥抓取多源 → 写入 sentiment_items 表。
//! 通用接口，报告生成器 / 委员会催化 / 独立命令都能复用。

use crate::storage::invest::sentiment::{save_sentiment_item, SentimentItem};
use sha2::{Digest, Sha256};

/// Python 契约（`sentiment.fetch` 返回的每条）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RawSentimentItem {
    pub provider: String,
    pub symbol: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<String>,
    pub read_count: Option<i64>,
    pub comment_count: Option<i64>,
    pub source_type: String,
    pub sentiment_hint: Option<f64>,
}

/// 稳定去重 id：provider + (url 优先，无则 title)。
pub fn make_sentiment_id(provider: &str, url: &str, title: &str) -> String {
    let key = if url.is_empty() { title } else { url };
    let mut hasher = Sha256::new();
    hasher.update(provider.as_bytes());
    hasher.update(b"|");
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// 抓取指定 provider 并写入 sentiment_items。返回写入（尝试）条数。
/// 单点失败不抛——Python 层已保证单 provider 失败返回空列表。
pub async fn fetch_and_store(
    provider: &str,
    symbol: Option<&str>,
    limit: u32,
) -> Result<usize, String> {
    let runtime = crate::python::require()?;
    let params = serde_json::json!({
        "provider": provider,
        "symbol": symbol,
        "limit": limit,
    });
    let value = runtime.call("sentiment.fetch", params).await?;
    let raws: Vec<RawSentimentItem> = serde_json::from_value(value)
        .map_err(|e| format!("parse sentiment.fetch: {e}"))?;

    let mut count = 0usize;
    for r in &raws {
        let url = r.url.clone().unwrap_or_default();
        let item = SentimentItem {
            id: make_sentiment_id(&r.provider, &url, &r.title),
            provider: r.provider.clone(),
            symbol: r.symbol.clone(),
            title: r.title.clone(),
            summary: r.summary.clone(),
            url: r.url.clone(),
            published_at: r.published_at.clone(),
            read_count: r.read_count,
            comment_count: r.comment_count,
            source_type: r.source_type.clone(),
            sentiment_hint: r.sentiment_hint,
            affected_symbols: None, // 归一化后填（Task 5）
            sectors: None,
            topics: None,
            stance: "pending".to_string(),
            severity: "pending".to_string(),
            analyzed: false,
            created_at: String::new(), // save 时 COALESCE 到 now
        };
        if let Err(e) = save_sentiment_item(&item) {
            log::warn!("save sentiment_item {} failed: {}", item.id, e);
        } else {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_id_stable_and_distinct() {
        let a = make_sentiment_id("ths", "https://x.com/1", "标题A");
        let b = make_sentiment_id("ths", "https://x.com/1", "标题A");
        let c = make_sentiment_id("sina", "https://x.com/1", "标题A");
        assert_eq!(a, b, "同输入应稳定");
        assert_ne!(a, c, "不同 provider 应不同");
        assert_eq!(a.len(), 64, "sha256 hex 长度");
    }

    #[test]
    fn test_raw_item_deserialize() {
        let json = r#"{"provider":"ths","symbol":null,"title":"t","summary":"s","url":"u","published_at":"2026-07-08T09:00:00","read_count":10,"comment_count":2,"source_type":"news","sentiment_hint":0.5}"#;
        let item: RawSentimentItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.provider, "ths");
        assert_eq!(item.sentiment_hint, Some(0.5));
    }
}
