use crate::storage::invest::{with_conn, with_conn_mut};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainInsight {
    pub id: String,
    pub insight_type: String,
    pub symbol: Option<String>,
    pub content: String,
    pub confidence: Option<f64>,
    pub source_verdict_ids: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Upsert a domain insight. On conflict (same id), updates content, confidence,
/// source_verdict_ids, and updated_at.
pub fn upsert_insight(insight: &DomainInsight) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO domain_insights (id, insight_type, symbol, content, confidence, source_verdict_ids, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               content = excluded.content,
               confidence = excluded.confidence,
               source_verdict_ids = excluded.source_verdict_ids,
               updated_at = excluded.updated_at",
            rusqlite::params![
                insight.id,
                insight.insight_type,
                insight.symbol,
                insight.content,
                insight.confidence,
                insight.source_verdict_ids,
                insight.status,
                insight.created_at,
                insight.updated_at,
            ],
        )
        .map_err(|e| format!("upsert insight: {e}"))?;
        Ok(())
    })
}

/// List domain insights with optional filters.
pub fn list_insights(
    status: Option<&str>,
    insight_type: Option<&str>,
    symbol: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<DomainInsight>, String> {
    with_conn(|conn| {
        let mut sql = String::from(
            "SELECT id, insight_type, symbol, content, confidence, source_verdict_ids, status, created_at, updated_at
             FROM domain_insights WHERE 1=1",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(s) = status {
            sql.push_str(&format!(" AND status = ?{idx}"));
            params.push(Box::new(s.to_string()));
            idx += 1;
        }
        if let Some(t) = insight_type {
            sql.push_str(&format!(" AND insight_type = ?{idx}"));
            params.push(Box::new(t.to_string()));
            idx += 1;
        }
        if let Some(sym) = symbol {
            sql.push_str(&format!(" AND symbol = ?{idx}"));
            params.push(Box::new(sym.to_string()));
            idx += 1;
        }
        sql.push_str(" ORDER BY updated_at DESC");
        if let Some(l) = limit {
            sql.push_str(&format!(" LIMIT ?{idx}"));
            params.push(Box::new(l));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| format!("prepare insights query: {e}"))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                Ok(DomainInsight {
                    id: row.get(0)?,
                    insight_type: row.get(1)?,
                    symbol: row.get(2)?,
                    content: row.get(3)?,
                    confidence: row.get(4)?,
                    source_verdict_ids: row.get(5)?,
                    status: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .map_err(|e| format!("query insights: {e}"))?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r.map_err(|e| format!("read insight row: {e}"))?);
        }
        Ok(result)
    })
}

/// Export all active insights as a JSON array string. Used for dream snapshots
/// (before/after comparison).
pub fn get_active_insights_json() -> Result<String, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT json_group_array(
                    json(id, insight_type, symbol, content, confidence, source_verdict_ids, status, created_at, updated_at)
                 )
                 FROM domain_insights WHERE status = 'active'",
            )
            .map_err(|e| format!("prepare active insights json: {e}"))?;
        let json: String = stmt
            .query_row([], |row| row.get(0))
            .map_err(|e| format!("query active insights json: {e}"))?;
        Ok(json)
    })
}

/// Restore active insights from a JSON snapshot. Deletes all current active
/// insights and replaces them with the snapshot contents.
/// Wrapped in a transaction so DELETE+INSERT is atomic.
pub fn restore_insight_snapshot(json: &str) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute_batch("BEGIN IMMEDIATE;")
            .map_err(|e| format!("begin transaction: {e}"))?;
        let result = (|| -> Result<(), String> {
            conn.execute("DELETE FROM domain_insights WHERE status = 'active'", [])
                .map_err(|e| format!("clear active insights: {e}"))?;
            conn.execute(
                "INSERT INTO domain_insights (id, insight_type, symbol, content, confidence, source_verdict_ids, status, created_at, updated_at)
                 SELECT
                    json_extract(value, '$[0]'),
                    json_extract(value, '$[1]'),
                    json_extract(value, '$[2]'),
                    json_extract(value, '$[3]'),
                    CAST(json_extract(value, '$[4]') AS REAL),
                    json_extract(value, '$[5]'),
                    json_extract(value, '$[6]'),
                    json_extract(value, '$[7]'),
                    json_extract(value, '$[8]')
                 FROM json_each(?1)",
                [json],
            )
            .map_err(|e| format!("restore insights snapshot: {e}"))?;
            Ok(())
        })();
        if result.is_ok() {
            conn.execute_batch("COMMIT;")
                .map_err(|e| format!("commit transaction: {e}"))?;
        } else {
            conn.execute_batch("ROLLBACK;").ok();
        }
        result
    })
}
