//! 舆情采集接口层：调 Python 桥抓取多源 → 写入 sentiment_items 表。
//! 通用接口，报告生成器 / 委员会催化 / 独立命令都能复用。

use crate::invest::event_analyzer::{analyze_pending, AnalyzeTable, AnalyzerResult};
use crate::storage::invest::sentiment::{save_sentiment_item, SentimentItem};
use sha2::{Digest, Sha256};

/// keyring service / user 名——雪球登录 cookie 走 Windows Credential Manager (DPAPI) / macOS Keychain / Secret Service。
/// 明文绝不落 DB/文件/日志。
const XQ_COOKIE_SERVICE: &str = "clawgo-invest";
const XQ_COOKIE_USER: &str = "xueqiu-cookie";

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

/// Python 原始条目 → 未归一化的 `SentimentItem`（归一化字段留空，`event_analyzer` 后填）。
fn raw_to_item(r: &RawSentimentItem) -> SentimentItem {
    let url = r.url.clone().unwrap_or_default();
    SentimentItem {
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
    }
}

/// 批量落库：逐条 `save_sentiment_item`，单条失败仅告警。返回成功写入条数。
fn store_raws(raws: &[RawSentimentItem]) -> usize {
    let mut count = 0usize;
    for r in raws {
        let item = raw_to_item(r);
        match save_sentiment_item(&item) {
            Ok(()) => count += 1,
            Err(e) => log::warn!("save sentiment_item {} failed: {}", item.id, e),
        }
    }
    count
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

    Ok(store_raws(&raws))
}

/// 从 tushare `stock_basic` 拉全量个股行业，写入 `stock_industry` 表。
/// 返回成功写入条数。每周刷一次即可（供闭集词表 + 敏感度映射使用）。
///
/// 实现细节：
/// - `TushareClient::from_settings()` 返回 `Result<Self, String>`，未配置 token 时 Err。
/// - `client.stock_basic(None)` 拉全量上市股票（`StockBasic { ts_code, name, industry, ... }`）。
/// - ts_code 形如 `600519.SH`，`split('.').next()` 取 6 位代码入库。
/// - 单条 upsert 失败仅告警不中断（网络/DB 单点故障不应拖垮全量刷新）。
pub async fn refresh_stock_industry() -> Result<usize, String> {
    use crate::storage::invest::stock_industry::upsert_stock_industry;
    let client = crate::tushare::client::TushareClient::from_settings()?;
    let rows = client.stock_basic(None).await?;
    let mut n = 0usize;
    for sb in &rows {
        let code = sb
            .ts_code
            .split('.')
            .next()
            .unwrap_or(&sb.ts_code)
            .to_string();
        if code.is_empty() {
            continue;
        }
        match upsert_stock_industry(&code, &sb.name, &sb.industry) {
            Ok(()) => n += 1,
            Err(e) => log::warn!("upsert stock_industry {} failed: {}", code, e),
        }
    }
    log::info!(
        "refresh_stock_industry: fetched {} rows, upserted {}",
        rows.len(),
        n
    );
    Ok(n)
}

/// 盘前采集编排：抓取四源 → 立即归一化到清零。
///
/// 消除「event_analyzer 10 分钟批处理延迟导致委员会读到未归一化数据」的时序穿孔：
/// 抓取完成后直接同步调用 `analyze_pending(Sentiment, _)`，循环到 `total_pending == 0`。
///
/// 循环设计：`analyze_pending` 单批 `MAX_BATCH_SIZE = 50`，四源爆量可能多批；
/// 若某批 `analyzed + skipped == 0`（全部失败），认定无进展、跳出防死循环。
pub async fn collect_all_sentiment(
    symbol: Option<&str>,
    limit: u32,
) -> Result<AnalyzerResult, String> {
    // 1. 抓取四源（`fetch_and_store("all", ...)` 由 Python 端聚合所有已注册 provider）
    let stored = fetch_and_store("all", symbol, limit).await?;
    log::info!("collect_all_sentiment: stored {} items", stored);

    // 2. 同步归一化到清零
    let mut agg = AnalyzerResult {
        total_pending: 0,
        analyzed: 0,
        skipped: 0,
        errors: vec![],
    };
    loop {
        let r = analyze_pending(
            AnalyzeTable::Sentiment,
            crate::invest::event_scanner::DEFAULT_LANGUAGE,
        )
        .await?;
        if r.total_pending == 0 {
            break;
        }
        agg.total_pending += r.total_pending;
        agg.analyzed += r.analyzed;
        agg.skipped += r.skipped;
        agg.errors.extend(r.errors);
        // 若一批全部失败（analyzed + skipped == 0）防死循环
        if r.analyzed == 0 && r.skipped == 0 {
            log::warn!("collect_all_sentiment: batch made no progress, stopping");
            break;
        }
    }
    Ok(agg)
}

/// 保存雪球登录 cookie 到系统密钥库（Windows: DPAPI/Credential Manager）。
///
/// 绝不写入 DB / 配置文件 / 日志。前端命令直接调，透传原始 cookie 串（JSON 数组或 `k=v;` 均可）。
pub fn save_xueqiu_cookie(cookie: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(XQ_COOKIE_SERVICE, XQ_COOKIE_USER)
        .map_err(|e| format!("keyring entry: {e}"))?;
    entry
        .set_password(cookie)
        .map_err(|e| format!("keyring set: {e}"))
}

/// 读取雪球 cookie。未设置 / keyring 不可用时返回 None（调用方降级为空抓）。
pub fn get_xueqiu_cookie() -> Option<String> {
    keyring::Entry::new(XQ_COOKIE_SERVICE, XQ_COOKIE_USER)
        .ok()
        .and_then(|e| e.get_password().ok())
}

/// 雪球市场级热帖榜 → sentiment_items。
///
/// 降级契约（不抛错，返回 `Ok(0)` + warning）：
/// - cookie 未配置
/// - Python 端 scrapling 引擎缺失 / 浏览器未装
/// - WAF 拦截 / cookie 已失效 → Python 返回空列表
/// - RPC 调用失败 → warning 后 Ok(0)
///
/// 只有 JSON 反序列化失败（Python 端契约破坏）才抛 Err——那是真实 bug。
pub async fn fetch_xueqiu_market(limit: u32) -> Result<usize, String> {
    let cookie = match get_xueqiu_cookie() {
        Some(c) => c,
        None => {
            log::warn!("xueqiu cookie 未配置，跳过雪球通道");
            return Ok(0);
        }
    };
    let runtime = crate::python::require()?;
    let params = serde_json::json!({
        "cookie_json": cookie,
        "size": limit,
    });
    let value = match runtime.call("xueqiu.hot", params).await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("xueqiu.hot failed (降级跳过): {}", e);
            return Ok(0);
        }
    };
    let raws: Vec<RawSentimentItem> = serde_json::from_value(value)
        .map_err(|e| format!("parse xueqiu.hot: {e}"))?;
    if raws.is_empty() {
        log::warn!("xueqiu 返回空（WAF/cookie 可能失效），报告将标注雪球缺失");
        return Ok(0);
    }
    Ok(store_raws(&raws))
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
