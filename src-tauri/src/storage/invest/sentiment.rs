use super::with_conn;
use rusqlite::params;

pub const CREATE_SENTIMENT_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS sentiment_items (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    symbol TEXT,
    title TEXT NOT NULL,
    summary TEXT,
    url TEXT,
    published_at TEXT,
    read_count INTEGER,
    comment_count INTEGER,
    source_type TEXT NOT NULL DEFAULT 'news',
    sentiment_hint REAL,
    affected_symbols TEXT,
    sectors TEXT,
    topics TEXT,
    stance TEXT NOT NULL DEFAULT 'neutral',
    severity TEXT NOT NULL DEFAULT 'pending',
    analyzed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_sentiment_created ON sentiment_items(created_at);
CREATE INDEX IF NOT EXISTS idx_sentiment_analyzed ON sentiment_items(analyzed);
"#;

/// 幂等添加列：列不存在才 ALTER TABLE ADD COLUMN。
pub fn ensure_column(
    conn: &rusqlite::Connection,
    table: &str,
    col: &str,
    coltype: &str,
) -> Result<(), String> {
    let exists: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = ?1", table),
            params![col],
            |r| r.get(0),
        )
        .map_err(|e| format!("check column {}.{}: {}", table, col, e))?;
    if exists == 0 {
        conn.execute(&format!("ALTER TABLE {} ADD COLUMN {} {}", table, col, coltype), [])
            .map_err(|e| format!("add column {}.{}: {}", table, col, e))?;
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentimentItem {
    pub id: String,
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
    pub affected_symbols: Option<String>,
    pub sectors: Option<String>,
    pub topics: Option<String>,
    pub stance: String,
    pub severity: String,
    pub analyzed: bool,
    pub created_at: String,
}

fn row_to_item(row: &rusqlite::Row) -> rusqlite::Result<SentimentItem> {
    Ok(SentimentItem {
        id: row.get(0)?,
        provider: row.get(1)?,
        symbol: row.get(2)?,
        title: row.get(3)?,
        summary: row.get(4)?,
        url: row.get(5)?,
        published_at: row.get(6)?,
        read_count: row.get(7)?,
        comment_count: row.get(8)?,
        source_type: row.get(9)?,
        sentiment_hint: row.get(10)?,
        affected_symbols: row.get(11)?,
        sectors: row.get(12)?,
        topics: row.get(13)?,
        stance: row.get(14)?,
        severity: row.get(15)?,
        analyzed: row.get::<_, i32>(16)? != 0,
        created_at: row.get(17)?,
    })
}

const COLS: &str = "id, provider, symbol, title, summary, url, published_at, \
    read_count, comment_count, source_type, sentiment_hint, affected_symbols, \
    sectors, topics, stance, severity, analyzed, created_at";

pub fn save_sentiment_item(item: &SentimentItem) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT OR IGNORE INTO sentiment_items (id, provider, symbol, title, summary, url, published_at, read_count, comment_count, source_type, sentiment_hint, affected_symbols, sectors, topics, stance, severity, analyzed, created_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17, COALESCE(?18, datetime('now')))",
            params![
                item.id, item.provider, item.symbol, item.title, item.summary, item.url,
                item.published_at, item.read_count, item.comment_count, item.source_type,
                item.sentiment_hint, item.affected_symbols, item.sectors, item.topics,
                item.stance, item.severity, item.analyzed as i32,
                if item.created_at.is_empty() { None } else { Some(item.created_at.clone()) },
            ],
        )
        .map_err(|e| format!("save sentiment_item: {}", e))?;
        Ok(())
    })
}

pub fn list_unanalyzed_sentiment(limit: Option<i64>) -> Result<Vec<SentimentItem>, String> {
    with_conn(|conn| {
        let lim = limit.unwrap_or(100);
        let sql = format!(
            "SELECT {} FROM sentiment_items WHERE analyzed = 0 ORDER BY created_at DESC LIMIT ?1",
            COLS
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(params![lim], row_to_item)
            .map_err(|e| format!("query: {}", e))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| format!("row: {}", e))?);
        }
        Ok(out)
    })
}

pub fn update_sentiment_analysis(
    id: &str,
    summary: Option<&str>,
    severity: &str,
    stance: &str,
    affected_symbols: Option<&str>,
    sectors: Option<&str>,
    topics: Option<&str>,
) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "UPDATE sentiment_items SET summary = ?1, severity = ?2, stance = ?3, affected_symbols = ?4, sectors = ?5, topics = ?6, analyzed = 1 WHERE id = ?7",
            params![summary, severity, stance, affected_symbols, sectors, topics, id],
        )
        .map_err(|e| format!("update sentiment analysis: {}", e))?;
        Ok(())
    })
}

/// 拉取近 `since` 起的最新舆情条目（不区分是否已分析），用于聚合展示 / AI 点评输入。
pub fn list_recent_sentiment(since: &str, limit: i64) -> Result<Vec<SentimentItem>, String> {
    with_conn(|conn| {
        let sql = format!(
            "SELECT {} FROM sentiment_items WHERE created_at >= ?1 ORDER BY created_at DESC LIMIT ?2",
            COLS
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(params![since, limit], row_to_item)
            .map_err(|e| format!("query: {}", e))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| format!("row: {}", e))?);
        }
        Ok(out)
    })
}

pub fn list_sentiment_by_symbol(
    code: &str,
    since: &str,
    limit: i64,
) -> Result<Vec<SentimentItem>, String> {
    with_conn(|conn| {
        let pat = format!("%,{},%", code);
        let sql = format!(
            "SELECT {} FROM sentiment_items WHERE affected_symbols LIKE ?1 AND created_at >= ?2 ORDER BY created_at DESC LIMIT ?3",
            COLS
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(params![pat, since, limit], row_to_item)
            .map_err(|e| format!("query: {}", e))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| format!("row: {}", e))?);
        }
        Ok(out)
    })
}

pub fn list_sentiment_by_sectors(
    sectors: &[String],
    since: &str,
    limit: i64,
) -> Result<Vec<SentimentItem>, String> {
    if sectors.is_empty() {
        return Ok(vec![]);
    }
    with_conn(|conn| {
        // 每个 sector 一个 LIKE '%,{sector},%' OR 条件
        let mut clauses = Vec::new();
        let mut binds: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        for s in sectors {
            clauses.push(format!("sectors LIKE ?{}", binds.len() + 1));
            binds.push(Box::new(format!("%,{},%", s)));
        }
        let since_idx = binds.len() + 1;
        binds.push(Box::new(since.to_string()));
        let limit_idx = binds.len() + 1;
        binds.push(Box::new(limit));
        let sql = format!(
            "SELECT {} FROM sentiment_items WHERE ({}) AND created_at >= ?{} ORDER BY created_at DESC LIMIT ?{}",
            COLS,
            clauses.join(" OR "),
            since_idx,
            limit_idx
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(binds.iter()), row_to_item)
            .map_err(|e| format!("query: {}", e))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| format!("row: {}", e))?);
        }
        Ok(out)
    })
}
