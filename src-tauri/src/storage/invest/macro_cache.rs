use crate::storage::invest::{with_conn, with_conn_mut};
use rusqlite::{params, Connection};

const CREATE_TABLE_SQL: &str = "
CREATE TABLE IF NOT EXISTS macro_cache (
    indicator   TEXT PRIMARY KEY,
    value       REAL,
    extra_json  TEXT,
    source      TEXT NOT NULL,
    fetched_at  TEXT NOT NULL DEFAULT (datetime('now'))
);";

/// The 19 canonical macro indicators stored in this table.
pub const ALL_INDICATORS: &[&str] = &[
    "sh_composite_close",
    "sh_composite_vol20",
    "northbound_net",
    "margin_balance",
    "shibor_on",
    "cgb_10y",
    "vix",
    "tnx",
    "dxy",
    "gold",
    "oil",
    "usdcny",
    "limit_up_count",
    "limit_down_count",
    "two_market_volume",
    "advance_count",
    "decline_count",
    "up_over_3pct_count",
    "flat_count",
];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroCacheEntry {
    pub indicator: String,
    pub value: Option<f64>,
    pub extra_json: Option<String>,
    pub source: String,
    pub fetched_at: String,
}

/// Create table using a local connection (for use during init_db before static DB is set).
pub fn create_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(CREATE_TABLE_SQL)
        .map_err(|e| format!("create macro_cache table: {e}"))
}

/// Create table using the static DB connection (for use after init_db).
pub fn create_table_if_not_exists() -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute_batch(CREATE_TABLE_SQL)
            .map_err(|e| format!("create macro_cache table: {e}"))
    })
}

/// UPSERT a single macro indicator value.
pub fn save_macro_cache(
    indicator: &str,
    value: Option<f64>,
    extra_json: Option<&str>,
    source: &str,
) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO macro_cache (indicator, value, extra_json, source, fetched_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(indicator) DO UPDATE SET
                value = excluded.value,
                extra_json = excluded.extra_json,
                source = excluded.source,
                fetched_at = excluded.fetched_at",
            params![indicator, value, extra_json, source],
        )
        .map_err(|e| format!("upsert macro_cache {indicator}: {e}"))?;
        Ok(())
    })
}

/// Read a single macro indicator by name.
pub fn load_macro_cache(indicator: &str) -> Result<Option<MacroCacheEntry>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT indicator, value, extra_json, source, fetched_at
                 FROM macro_cache WHERE indicator = ?1",
            )
            .map_err(|e| format!("prepare load_macro_cache: {e}"))?;
        let mut rows = stmt
            .query_map([indicator], |row| {
                Ok(MacroCacheEntry {
                    indicator: row.get(0)?,
                    value: row.get(1)?,
                    extra_json: row.get(2)?,
                    source: row.get(3)?,
                    fetched_at: row.get(4)?,
                })
            })
            .map_err(|e| format!("query load_macro_cache: {e}"))?;
        rows.next()
            .transpose()
            .map_err(|e| format!("read load_macro_cache row: {e}"))
    })
}

/// Read all macro indicators.
pub fn load_all_macro_cache() -> Result<Vec<MacroCacheEntry>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT indicator, value, extra_json, source, fetched_at
                 FROM macro_cache ORDER BY indicator",
            )
            .map_err(|e| format!("prepare load_all_macro_cache: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(MacroCacheEntry {
                    indicator: row.get(0)?,
                    value: row.get(1)?,
                    extra_json: row.get(2)?,
                    source: row.get(3)?,
                    fetched_at: row.get(4)?,
                })
            })
            .map_err(|e| format!("query load_all_macro_cache: {e}"))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("read load_all_macro_cache row: {e}"))?);
        }
        Ok(items)
    })
}

/// 宏观指标快照（从 macro_cache 直接注入，非 LLM 解析）。
/// 10 个核心指标，用于前端直接展示精确数值。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroSnapshot {
    /// 上证指数
    pub sh_composite_close: Option<f64>,
    /// 上证指数 20 日波动率 (%)
    pub sh_composite_vol20: Option<f64>,
    /// 北向资金净流入 (亿)
    pub northbound_net: Option<f64>,
    /// VIX 恐慌指数
    pub vix: Option<f64>,
    /// 国际金价 (USD)
    pub gold: Option<f64>,
    /// 上涨家数
    pub advance_count: Option<f64>,
    /// 下跌家数
    pub decline_count: Option<f64>,
    /// 两市成交额 (亿)
    pub two_market_volume: Option<f64>,
    /// 涨停家数
    pub limit_up_count: Option<f64>,
    /// 跌停家数
    pub limit_down_count: Option<f64>,
    /// 涨幅 > 3% 家数（赚钱效应基础）
    pub up_over_3pct_count: Option<f64>,
    /// 平盘家数
    pub flat_count: Option<f64>,
}

/// 从 macro_cache 读取 10 个核心指标，构建快照。
pub fn build_macro_snapshot() -> Option<MacroSnapshot> {
    let entries = load_all_macro_cache().ok()?;
    let get = |key: &str| entries.iter().find(|e| e.indicator == key).and_then(|e| e.value);
    Some(MacroSnapshot {
        sh_composite_close: get("sh_composite_close"),
        sh_composite_vol20: get("sh_composite_vol20"),
        northbound_net: get("northbound_net"),
        vix: get("vix"),
        gold: get("gold"),
        advance_count: get("advance_count"),
        decline_count: get("decline_count"),
        two_market_volume: get("two_market_volume"),
        limit_up_count: get("limit_up_count"),
        limit_down_count: get("limit_down_count"),
        up_over_3pct_count: get("up_over_3pct_count"),
        flat_count: get("flat_count"),
    })
}

/// 拼接广度批次戳 + macro_refresh 批次戳为确定性版本串。
pub fn compose_data_version(breadth: Option<&str>, macro_batch: Option<&str>) -> String {
    format!("b:{}|m:{}", breadth.unwrap_or("none"), macro_batch.unwrap_or("none"))
}

/// 读两个批次标记行的 fetched_at，组合为当前数据版本串。
pub fn current_data_version() -> Result<String, String> {
    let b = load_macro_cache("_breadth_batch")?.map(|e| e.fetched_at);
    let m = load_macro_cache("_macro_batch")?.map(|e| e.fetched_at);
    Ok(compose_data_version(b.as_deref(), m.as_deref()))
}

/// Check whether a cache entry is older than `max_age_minutes`.
///
/// Compares `fetched_at` (stored as UTC datetime string) against the current time.
/// Returns `true` if the entry is stale or if the timestamp cannot be parsed.
///
/// Weekend exception: markets are closed Sat/Sun, so no fresher macro data can exist.
/// If it is currently the weekend and the entry was fetched on or after the most recent
/// Friday, it is NOT stale regardless of wall-clock age — otherwise every Monday-morning
/// (and all weekend) read would wrongly flag the last trading day's data as stale.
pub fn is_stale(entry: &MacroCacheEntry, max_age_minutes: u32) -> bool {
    use chrono::{Datelike, NaiveDateTime, Utc, Weekday};

    let Ok(fetched) = NaiveDateTime::parse_from_str(&entry.fetched_at, "%Y-%m-%d %H:%M:%S") else {
        // Unparseable timestamp => treat as stale
        return true;
    };
    let fetched_utc = fetched.and_utc();
    let now = Utc::now();

    // Weekend freshness in Beijing time (A-share market timezone). If it is Sat/Sun in
    // CST and the data was fetched on or after the most recent Friday (CST), accept it.
    let cst = chrono::FixedOffset::east_opt(8 * 3600).unwrap();
    let now_cst = now.with_timezone(&cst);
    let days_since_friday = match now_cst.weekday() {
        Weekday::Sat => 1,
        Weekday::Sun => 2,
        // Weekday — markets may have fresher data, fall through to the age check.
        _ => return age_is_stale(now, fetched_utc, max_age_minutes),
    };
    let last_friday = (now_cst - chrono::Duration::days(days_since_friday)).date_naive();
    if fetched_utc.with_timezone(&cst).date_naive() >= last_friday {
        return false;
    }
    age_is_stale(now, fetched_utc, max_age_minutes)
}

fn age_is_stale(
    now: chrono::DateTime<chrono::Utc>,
    fetched_utc: chrono::DateTime<chrono::Utc>,
    max_age_minutes: u32,
) -> bool {
    now.signed_duration_since(fetched_utc).num_minutes() > max_age_minutes as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_stale_parsing() {
        // An entry with a clearly old timestamp should be stale.
        let entry = MacroCacheEntry {
            indicator: "gold".into(),
            value: Some(3200.0),
            extra_json: None,
            source: "tushare".into(),
            fetched_at: "2020-01-01 00:00:00".into(),
        };
        assert!(is_stale(&entry, 60));
    }

    #[test]
    fn test_is_stale_unparseable() {
        let entry = MacroCacheEntry {
            indicator: "gold".into(),
            value: Some(3200.0),
            extra_json: None,
            source: "tushare".into(),
            fetched_at: "not-a-date".into(),
        };
        assert!(is_stale(&entry, 60));
    }

    #[test]
    fn test_compose_data_version() {
        assert_eq!(
            compose_data_version(Some("2026-07-08 01:35:00"), Some("2026-07-08 01:30:00")),
            "b:2026-07-08 01:35:00|m:2026-07-08 01:30:00"
        );
        // 任一缺失 → 该段为 none，整体仍确定性可比
        assert_eq!(compose_data_version(None, Some("2026-07-08 01:30:00")), "b:none|m:2026-07-08 01:30:00");
        assert_eq!(compose_data_version(None, None), "b:none|m:none");
    }
}
