use crate::storage::invest::{with_conn, with_conn_mut};
use rusqlite::{params, Connection};

const CREATE_TABLE_SQL: &str = "
CREATE TABLE IF NOT EXISTS premarket_factor_cache (
    trade_date      TEXT NOT NULL,
    symbol          TEXT NOT NULL,
    name            TEXT NOT NULL,
    change_pct      REAL NOT NULL DEFAULT 0,
    amount          REAL NOT NULL DEFAULT 0,
    sentiment       REAL NOT NULL DEFAULT 50,
    capital         REAL NOT NULL DEFAULT 50,
    technical       REAL NOT NULL DEFAULT 50,
    catalyst        REAL NOT NULL DEFAULT 50,
    sector_strength REAL NOT NULL DEFAULT 50,
    missing         TEXT NOT NULL DEFAULT '',
    cached_at       TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (trade_date, symbol)
);
CREATE INDEX IF NOT EXISTS idx_pmcache_date ON premarket_factor_cache(trade_date);";

#[derive(Debug, Clone)]
pub struct CachedFactor {
    pub symbol: String,
    pub name: String,
    pub change_pct: f64,
    pub amount: f64,
    pub sentiment: f64,
    pub capital: f64,
    pub technical: f64,
    pub catalyst: f64,
    pub sector_strength: f64,
    pub missing: Vec<String>,
}

pub fn create_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(CREATE_TABLE_SQL)
        .map_err(|e| format!("create premarket_factor_cache: {e}"))?;
    super::sentiment::ensure_column(conn, "premarket_factor_cache", "sector_strength", "REAL NOT NULL DEFAULT 50")
}

/// 缓存新鲜度:trade_date 与 today(均 "YYYY-MM-DD")相差 <= max_age_days 自然日视为新鲜。
/// 解析失败一律视为不新鲜(保守走兜底)。
pub fn is_fresh(trade_date: &str, today: &str, max_age_days: i64) -> bool {
    let parse = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    match (parse(trade_date), parse(today)) {
        (Some(td), Some(now)) => {
            let diff = (now - td).num_days();
            (0..=max_age_days).contains(&diff)
        }
        _ => false,
    }
}

/// 批量 upsert 一个交易日的整批候选。空 rows 直接返回 Ok。
pub fn save_factor_cache(trade_date: &str, rows: &[CachedFactor]) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    with_conn_mut(|conn| {
        let tx = conn.transaction().map_err(|e| format!("tx begin: {e}"))?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO premarket_factor_cache
                     (trade_date, symbol, name, change_pct, amount, sentiment, capital, technical, catalyst, sector_strength, missing, cached_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11, datetime('now'))
                     ON CONFLICT(trade_date, symbol) DO UPDATE SET
                        name=excluded.name, change_pct=excluded.change_pct, amount=excluded.amount,
                        sentiment=excluded.sentiment, capital=excluded.capital, technical=excluded.technical,
                        catalyst=excluded.catalyst, sector_strength=excluded.sector_strength,
                        missing=excluded.missing, cached_at=excluded.cached_at",
                )
                .map_err(|e| format!("prepare upsert: {e}"))?;
            for r in rows {
                stmt.execute(params![
                    trade_date, r.symbol, r.name, r.change_pct, r.amount,
                    r.sentiment, r.capital, r.technical, r.catalyst, r.sector_strength, r.missing.join(","),
                ])
                .map_err(|e| format!("upsert row {}: {e}", r.symbol))?;
            }
        }
        tx.commit().map_err(|e| format!("tx commit: {e}"))
    })
}

/// 读缓存表中最新 trade_date 的整批候选。表空返回 Ok(None)。
pub fn load_latest_cache() -> Result<Option<(String, Vec<CachedFactor>)>, String> {
    with_conn(|conn| {
        let latest: Option<String> = conn
            .query_row(
                "SELECT MAX(trade_date) FROM premarket_factor_cache",
                [],
                |row| row.get(0),
            )
            .map_err(|e| format!("query max trade_date: {e}"))?;
        let Some(td) = latest else { return Ok(None) };
        let mut stmt = conn
            .prepare(
                "SELECT symbol, name, change_pct, amount, sentiment, capital, technical, catalyst, sector_strength, missing
                 FROM premarket_factor_cache WHERE trade_date = ?1 ORDER BY change_pct DESC",
            )
            .map_err(|e| format!("prepare load latest: {e}"))?;
        let rows = stmt
            .query_map([&td], |row| {
                let missing_str: String = row.get(9)?;
                Ok(CachedFactor {
                    symbol: row.get(0)?,
                    name: row.get(1)?,
                    change_pct: row.get(2)?,
                    amount: row.get(3)?,
                    sentiment: row.get(4)?,
                    capital: row.get(5)?,
                    technical: row.get(6)?,
                    catalyst: row.get(7)?,
                    sector_strength: row.get(8)?,
                    missing: if missing_str.is_empty() {
                        vec![]
                    } else {
                        missing_str.split(',').map(|s| s.to_string()).collect()
                    },
                })
            })
            .map_err(|e| format!("query load latest: {e}"))?;
        let out: Vec<CachedFactor> = rows
            .collect::<Result<_, _>>()
            .map_err(|e| format!("read latest rows: {e}"))?;
        Ok(Some((td, out)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_fresh_within_window() {
        assert!(is_fresh("2026-07-06", "2026-07-09", 4)); // 周五缓存,周一读,差3天
        assert!(is_fresh("2026-07-09", "2026-07-09", 4)); // 同日
    }

    #[test]
    fn is_fresh_rejects_stale_and_future() {
        assert!(!is_fresh("2026-07-01", "2026-07-09", 4)); // 差8天,过期
        assert!(!is_fresh("2026-07-10", "2026-07-09", 4)); // 未来日期
        assert!(!is_fresh("garbage", "2026-07-09", 4));    // 解析失败
    }
}
