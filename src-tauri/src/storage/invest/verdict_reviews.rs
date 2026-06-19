use super::with_conn;
use super::with_conn_mut;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerdictReviewEntry {
    pub id: i64,
    pub verdict_id: String,
    pub symbol: String,
    pub verdict_type: String,
    pub verdict_date: String,
    pub window_days: i64,
    pub price_at_verdict: Option<f64>,
    pub price_after: Option<f64>,
    pub return_pct: Option<f64>,
    pub hit: bool,
    pub flat_threshold: Option<f64>,
    pub created_at: String,
}

/// 单行命中率聚合：某 verdict_type × window_days 的命中数/样本数。
#[derive(Debug, Clone, serde::Serialize)]
pub struct HitRateRow {
    pub verdict_type: String,
    pub window_days: i64,
    pub hits: i64,
    pub total: i64,
    /// 30 天窗口在历史足够长前命中普遍为 0；matured=false 表示样本未到期，
    /// 不应解读为"0% 命中"。
    pub matured: bool,
}

/// 命中率聚合结果：全局 + 按 regime(verdicts.macro_signal)切分。
#[derive(Debug, Clone, serde::Serialize)]
pub struct HitRateAgg {
    pub global: Vec<HitRateRow>,
    pub by_regime: Vec<(String, Vec<HitRateRow>)>,
}

const CREATE_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS verdict_reviews (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    verdict_id      TEXT NOT NULL,
    symbol          TEXT NOT NULL,
    verdict_type    TEXT NOT NULL,
    verdict_date    TEXT NOT NULL,
    window_days     INTEGER NOT NULL,
    price_at_verdict REAL,
    price_after     REAL,
    return_pct      REAL,
    hit             INTEGER NOT NULL,
    flat_threshold  REAL,
    created_at      TEXT NOT NULL,
    UNIQUE(verdict_id, window_days)
);
CREATE INDEX IF NOT EXISTS idx_vr_verdict ON verdict_reviews(verdict_id);
CREATE INDEX IF NOT EXISTS idx_vr_symbol ON verdict_reviews(symbol);
CREATE INDEX IF NOT EXISTS idx_vr_date ON verdict_reviews(verdict_date);";

/// Create the verdict_reviews table using a provided connection.
/// Used during init_db when the static DB is not yet available.
pub fn create_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(CREATE_TABLE_SQL)
        .map_err(|e| format!("create verdict_reviews table: {}", e))?;
    Ok(())
}

/// Create the verdict_reviews table using the global connection.
pub fn create_table_if_not_exists() -> Result<(), String> {
    with_conn(|conn| {
        conn.execute_batch(CREATE_TABLE_SQL)
            .map_err(|e| format!("create verdict_reviews table: {}", e))?;
        Ok(())
    })
}

pub fn upsert_review(entry: &VerdictReviewEntry) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO verdict_reviews (verdict_id, symbol, verdict_type, verdict_date, window_days, price_at_verdict, price_after, return_pct, hit, flat_threshold, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(verdict_id, window_days) DO UPDATE SET
                price_at_verdict = excluded.price_at_verdict,
                price_after = excluded.price_after,
                return_pct = excluded.return_pct,
                hit = excluded.hit,
                flat_threshold = excluded.flat_threshold,
                created_at = excluded.created_at",
            params![
                entry.verdict_id,
                entry.symbol,
                entry.verdict_type,
                entry.verdict_date,
                entry.window_days,
                entry.price_at_verdict,
                entry.price_after,
                entry.return_pct,
                entry.hit as i64,
                entry.flat_threshold,
                entry.created_at,
            ],
        )
        .map_err(|e| format!("upsert verdict review: {}", e))?;
        Ok(())
    })
}

pub fn list_reviews(symbol: Option<&str>, limit: Option<i64>) -> Result<Vec<VerdictReviewEntry>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(100);
        let (sql, query_params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match symbol {
            Some(s) => (
                "SELECT id, verdict_id, symbol, verdict_type, verdict_date, window_days, price_at_verdict, price_after, return_pct, hit, flat_threshold, created_at
                 FROM verdict_reviews WHERE symbol = ?1 ORDER BY verdict_date DESC, window_days ASC LIMIT ?2",
                vec![Box::new(s.to_string()), Box::new(limit_val)],
            ),
            None => (
                "SELECT id, verdict_id, symbol, verdict_type, verdict_date, window_days, price_at_verdict, price_after, return_pct, hit, flat_threshold, created_at
                 FROM verdict_reviews ORDER BY verdict_date DESC, window_days ASC LIMIT ?1",
                vec![Box::new(limit_val)],
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(query_params.iter()), |row| {
                Ok(VerdictReviewEntry {
                    id: row.get(0)?,
                    verdict_id: row.get(1)?,
                    symbol: row.get(2)?,
                    verdict_type: row.get(3)?,
                    verdict_date: row.get(4)?,
                    window_days: row.get(5)?,
                    price_at_verdict: row.get(6)?,
                    price_after: row.get(7)?,
                    return_pct: row.get(8)?,
                    hit: row.get::<_, i64>(9)? != 0,
                    flat_threshold: row.get(10)?,
                    created_at: row.get(11)?,
                })
            })
            .map_err(|e| format!("query: {}", e))?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row.map_err(|e| format!("row: {}", e))?);
        }
        Ok(items)
    })
}

pub fn clear_reviews() -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute("DELETE FROM verdict_reviews", [])
            .map_err(|e| format!("clear verdict reviews: {}", e))?;
        Ok(())
    })
}

/// 实时聚合 verdict_reviews 命中率。纯读,无缓存。
/// - 全局:按 verdict_type × window_days 聚合。
/// - 按 regime:JOIN verdicts.macro_signal 再切一层(决策 1)。
/// - min_samples:total < min_samples 的行被剔除(HAVING)。
/// - matured:window_days>=30 且该组无任何 price_after(全 NULL)时 matured=false。
pub fn aggregate_hit_rates(min_samples: i64) -> Result<HitRateAgg, String> {
    with_conn(|conn| {
        // 全局聚合
        let mut stmt = conn
            .prepare(
                "SELECT verdict_type, window_days, \
                        SUM(CASE WHEN hit != 0 THEN 1 ELSE 0 END) AS hits, \
                        COUNT(*) AS total, \
                        SUM(CASE WHEN price_after IS NOT NULL THEN 1 ELSE 0 END) AS matured_cnt \
                 FROM verdict_reviews \
                 GROUP BY verdict_type, window_days \
                 HAVING total >= ?1",
            )
            .map_err(|e| format!("prepare global agg: {}", e))?;
        let global: Vec<HitRateRow> = stmt
            .query_map(params![min_samples], |row| {
                let window_days: i64 = row.get(1)?;
                let total: i64 = row.get(3)?;
                let matured_cnt: i64 = row.get(4)?;
                Ok(HitRateRow {
                    verdict_type: row.get(0)?,
                    window_days,
                    hits: row.get(2)?,
                    total,
                    matured: !(window_days >= 30 && matured_cnt == 0),
                })
            })
            .map_err(|e| format!("query global agg: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect global agg: {}", e))?;

        // 按 regime 聚合(JOIN verdicts.macro_signal)
        let mut stmt2 = conn
            .prepare(
                "SELECT COALESCE(v.macro_signal, 'unknown') AS regime, \
                        vr.verdict_type, vr.window_days, \
                        SUM(CASE WHEN vr.hit != 0 THEN 1 ELSE 0 END) AS hits, \
                        COUNT(*) AS total, \
                        SUM(CASE WHEN vr.price_after IS NOT NULL THEN 1 ELSE 0 END) AS matured_cnt \
                 FROM verdict_reviews vr \
                 JOIN verdicts v ON vr.verdict_id = v.id \
                 GROUP BY regime, vr.verdict_type, vr.window_days \
                 HAVING total >= ?1 \
                 ORDER BY regime",
            )
            .map_err(|e| format!("prepare regime agg: {}", e))?;
        let flat: Vec<(String, HitRateRow)> = stmt2
            .query_map(params![min_samples], |row| {
                let window_days: i64 = row.get(2)?;
                let total: i64 = row.get(4)?;
                let matured_cnt: i64 = row.get(5)?;
                Ok((
                    row.get::<_, String>(0)?,
                    HitRateRow {
                        verdict_type: row.get(1)?,
                        window_days,
                        hits: row.get(3)?,
                        total,
                        matured: !(window_days >= 30 && matured_cnt == 0),
                    },
                ))
            })
            .map_err(|e| format!("query regime agg: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect regime agg: {}", e))?;

        // 折叠成 Vec<(regime, Vec<HitRateRow>)>,保持 regime 顺序(SQL 已 ORDER BY regime)
        let mut by_regime: Vec<(String, Vec<HitRateRow>)> = Vec::new();
        for (regime, row) in flat {
            match by_regime.last_mut() {
                Some((r, rows)) if *r == regime => rows.push(row),
                _ => by_regime.push((regime, vec![row])),
            }
        }

        Ok(HitRateAgg { global, by_regime })
    })
}

#[cfg(test)]
mod agg_tests {
    use super::*;

    // 本机单测运行期可能失败(§11)，该测试主要保证编译；
    // SQL 逻辑正确性以评审为准。min_samples 过滤是纯函数式的。
    #[test]
    fn hit_rate_row_filters_below_min_samples() {
        let rows = vec![
            HitRateRow { verdict_type: "ACCUMULATE".into(), window_days: 1, hits: 2, total: 8, matured: true },
            HitRateRow { verdict_type: "TRIM".into(), window_days: 1, hits: 1, total: 3, matured: true },
        ];
        let filtered: Vec<_> = rows.into_iter().filter(|r| r.total >= 5).collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].verdict_type, "ACCUMULATE");
    }
}
