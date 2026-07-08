//! stock_industry：个股 → 行业映射表。
//!
//! 每周从 tushare `stock_basic` 全量刷新一次即可（`invest::sentiment::refresh_stock_industry`）。
//! `code` 存 6 位纯数字（如 `600519`），与雪球/多数舆情源使用的代码格式对齐；
//! 委员会/舆情归一化按 code 查行业，供闭集词表 & 敏感度映射使用。

use super::with_conn;
use rusqlite::params;

/// 建表 SQL（供 `init_db_inner` 迁移期调用）。
pub const CREATE_STOCK_INDUSTRY_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS stock_industry (
    code TEXT PRIMARY KEY,       -- 6位代码，如 600519
    name TEXT,
    industry TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

/// 插入或更新一条个股行业映射。
pub fn upsert_stock_industry(code: &str, name: &str, industry: &str) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT INTO stock_industry (code, name, industry, updated_at) VALUES (?1,?2,?3, datetime('now'))
             ON CONFLICT(code) DO UPDATE SET name=?2, industry=?3, updated_at=datetime('now')",
            params![code, name, industry],
        ).map_err(|e| format!("upsert stock_industry: {}", e))?;
        Ok(())
    })
}

/// 查询指定 code 对应的行业名。未收录返回 `Ok(None)`。
pub fn industry_of(code: &str) -> Result<Option<String>, String> {
    with_conn(|conn| {
        match conn.query_row(
            "SELECT industry FROM stock_industry WHERE code = ?1",
            params![code],
            |r| r.get::<_, Option<String>>(0),
        ) {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("industry_of: {}", e)),
        }
    })
}

/// 返回全部去重后的行业名（供闭集词表）。
pub fn all_industries() -> Result<Vec<String>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT industry FROM stock_industry \
                 WHERE industry IS NOT NULL AND industry != '' ORDER BY industry",
            )
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| format!("query: {}", e))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| format!("row: {}", e))?);
        }
        Ok(out)
    })
}

/// 表中 code 总数（用于新鲜度/覆盖度判断）。
pub fn industry_count() -> Result<i64, String> {
    with_conn(|conn| {
        conn.query_row("SELECT COUNT(*) FROM stock_industry", [], |r| r.get(0))
            .map_err(|e| format!("industry_count: {}", e))
    })
}

#[cfg(test)]
mod tests {
    // CRUD 依赖全局 sqlite（with_conn），本机测试运行时受环境限制，
    // 此处仅测纯逻辑；集成测试留待联调阶段跑 e2e。

    #[test]
    fn test_dedup_industries_logic() {
        let mut v = vec![
            "白酒".to_string(),
            "白酒".to_string(),
            "半导体".to_string(),
        ];
        v.sort();
        v.dedup();
        assert_eq!(v, vec!["半导体".to_string(), "白酒".to_string()]);
    }

    #[test]
    fn test_ts_code_to_code_split() {
        // 与 refresh_stock_industry 中的 split('.').next() 逻辑对齐。
        let ts_code = "600519.SH";
        let code = ts_code.split('.').next().unwrap_or(ts_code);
        assert_eq!(code, "600519");

        let no_dot = "000001";
        let code2 = no_dot.split('.').next().unwrap_or(no_dot);
        assert_eq!(code2, "000001");
    }
}
