use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

use crate::models::{MemoryNode, MemorySource};

// ── SQLite connection singleton ──

static DB: Mutex<Option<Connection>> = Mutex::new(None);

/// Initialize or open the memory database. Idempotent.
pub fn init_db(data_dir: &Path) -> Result<(), String> {
    let db_path = data_dir.join("memory.db");
    let conn = Connection::open(&db_path).map_err(|e| format!("open memory.db: {}", e))?;

    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA busy_timeout = 5000;
         PRAGMA foreign_keys = ON;",
    )
    .map_err(|e| format!("set pragmas: {}", e))?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS memories (
            id          TEXT PRIMARY KEY,
            content     TEXT NOT NULL,
            memory_type TEXT NOT NULL DEFAULT 'fact',
            confidence  REAL NOT NULL DEFAULT 0.8,
            source_kind TEXT NOT NULL DEFAULT 'extraction',
            source_run_id TEXT,
            source_group_chat_id TEXT,
            tags        TEXT NOT NULL DEFAULT '[]',
            status      TEXT NOT NULL DEFAULT 'approved',
            scope       TEXT NOT NULL DEFAULT 'global',
            project_id  TEXT,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_memories_status ON memories(status);
        CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(memory_type);
        CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories(scope);
        CREATE INDEX IF NOT EXISTS idx_memories_project_id ON memories(project_id) WHERE project_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);

        CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            content,
            tags,
            content='memories',
            content_rowid='rowid'
        );

        -- Triggers to keep FTS in sync
        CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
            INSERT INTO memories_fts(rowid, content, tags)
            VALUES (new.rowid, new.content, new.tags);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, tags)
            VALUES ('delete', old.rowid, old.content, old.tags);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content, tags)
            VALUES ('delete', old.rowid, old.content, old.tags);
            INSERT INTO memories_fts(rowid, content, tags)
            VALUES (new.rowid, new.content, new.tags);
        END;",
    )
    .map_err(|e| format!("create tables: {}", e))?;

    // Migration: add scope and project_id if missing (for existing databases)
    let has_scope: bool = conn
        .prepare("SELECT COUNT(*) FROM pragma_table_info('memories') WHERE name='scope'")
        .map_err(|e| format!("prepare migration check: {}", e))?
        .query_row([], |row| row.get::<_, i32>(0))
        .map_err(|e| format!("migration check query: {}", e))?
        > 0;

    if !has_scope {
        conn.execute_batch(
            "ALTER TABLE memories ADD COLUMN scope TEXT NOT NULL DEFAULT 'global';
             ALTER TABLE memories ADD COLUMN project_id TEXT;
             CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories(scope);
             CREATE INDEX IF NOT EXISTS idx_memories_project_id ON memories(project_id) WHERE project_id IS NOT NULL;"
        ).map_err(|e| format!("migrate memories: {}", e))?;
        log::info!("Migrated memories table: added scope + project_id columns");
    }

    let mut guard = DB.lock().map_err(|e| format!("lock db: {}", e))?;
    *guard = Some(conn);
    Ok(())
}

fn with_conn<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&Connection) -> Result<R, String>,
{
    let guard = DB.lock().map_err(|e| format!("lock db: {}", e))?;
    let conn = guard.as_ref().ok_or_else(|| "memory db not initialized".to_string())?;
    f(conn)
}

fn with_conn_mut<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut Connection) -> Result<R, String>,
{
    let mut guard = DB.lock().map_err(|e| format!("lock db: {}", e))?;
    let conn = guard.as_mut().ok_or_else(|| "memory db not initialized".to_string())?;
    f(conn)
}

// ── CRUD ──

pub fn insert_memory(node: &MemoryNode) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute(
            "INSERT INTO memories (id, content, memory_type, confidence, source_kind,
                source_run_id, source_group_chat_id, tags, status, scope, project_id,
                created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                node.id,
                node.content,
                node.memory_type,
                node.confidence,
                node.source.kind,
                node.source.run_id,
                node.source.group_chat_id,
                serde_json::to_string(&node.tags).unwrap_or_default(),
                node.status,
                node.scope,
                node.project_id,
                node.created_at,
                node.updated_at,
            ],
        )
        .map_err(|e| format!("insert memory: {}", e))?;
        Ok(())
    })
}

/// Convenience function: build a `MemoryNode` from individual fields and insert it.
/// Returns the new memory's ID on success.
pub fn save_memory(
    content: &str,
    memory_type: &str,
    source_run_id: Option<&str>,
    confidence: Option<f64>,
    scope: Option<&str>,
    project_id: Option<&str>,
) -> Result<String, String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let node = MemoryNode {
        id: id.clone(),
        character_id: String::new(),
        content: content.to_string(),
        memory_type: memory_type.to_string(),
        confidence: confidence.unwrap_or(0.8),
        source: crate::models::MemorySource {
            kind: "extraction".to_string(),
            run_id: source_run_id.map(|s| s.to_string()),
            group_chat_id: None,
        },
        tags: Vec::new(),
        status: "approved".to_string(),
        scope: scope.unwrap_or("global").to_string(),
        project_id: project_id.map(|s| s.to_string()),
        created_at: now.clone(),
        updated_at: now,
    };
    insert_memory(&node)?;
    Ok(id)
}

pub fn get_memory(id: &str) -> Result<Option<MemoryNode>, String> {
    with_conn(|conn| {
        conn.query_row(
            "SELECT id, content, memory_type, confidence, source_kind, source_run_id,
                source_group_chat_id, tags, status, scope, project_id, created_at, updated_at
             FROM memories WHERE id = ?1",
            params![id],
            row_to_memory,
        )
        .optional()
        .map_err(|e| format!("get memory: {}", e))
    })
}

pub fn list_memories(
    status_filter: Option<&str>,
    memory_type_filter: Option<&str>,
    scope_filter: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<Vec<MemoryNode>, String> {
    with_conn(|conn| {
        let mut sql = String::from(
            "SELECT id, content, memory_type, confidence, source_kind, source_run_id,
                source_group_chat_id, tags, status, scope, project_id, created_at, updated_at
             FROM memories WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if let Some(s) = status_filter {
            sql.push_str(&format!(" AND status = ?{}", param_idx));
            param_values.push(Box::new(s.to_string()));
            param_idx += 1;
        }
        if let Some(t) = memory_type_filter {
            sql.push_str(&format!(" AND memory_type = ?{}", param_idx));
            param_values.push(Box::new(t.to_string()));
            param_idx += 1;
        }
        if let Some(sc) = scope_filter {
            sql.push_str(&format!(" AND scope = ?{}", param_idx));
            param_values.push(Box::new(sc.to_string()));
            param_idx += 1;
        }

        sql.push_str(" ORDER BY created_at DESC");
        sql.push_str(&format!(" LIMIT ?{} OFFSET ?{}", param_idx, param_idx + 1));
        param_values.push(Box::new(limit as i64));
        param_values.push(Box::new(offset as i64));

        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(params_ref.as_slice(), row_to_memory)
            .map_err(|e| format!("query: {}", e))?;

        let mut results = Vec::new();
        for row in rows {
            match row {
                Ok(m) => results.push(m),
                Err(e) => log::warn!("skip row: {}", e),
            }
        }
        Ok(results)
    })
}

pub fn update_memory(node: &MemoryNode) -> Result<(), String> {
    with_conn_mut(|conn| {
        let rows = conn
            .execute(
                "UPDATE memories SET content = ?2, memory_type = ?3, confidence = ?4,
                    source_kind = ?5, source_run_id = ?6, source_group_chat_id = ?7,
                    tags = ?8, status = ?9, scope = ?10, project_id = ?11, updated_at = ?12
                 WHERE id = ?1",
                params![
                    node.id,
                    node.content,
                    node.memory_type,
                    node.confidence,
                    node.source.kind,
                    node.source.run_id,
                    node.source.group_chat_id,
                    serde_json::to_string(&node.tags).unwrap_or_default(),
                    node.status,
                    node.scope,
                    node.project_id,
                    Utc::now().to_rfc3339(),
                ],
            )
            .map_err(|e| format!("update memory: {}", e))?;
        if rows == 0 {
            Err(format!("memory {} not found", node.id))
        } else {
            Ok(())
        }
    })
}

pub fn delete_memory(id: &str) -> Result<(), String> {
    with_conn_mut(|conn| {
        conn.execute("DELETE FROM memories WHERE id = ?1", params![id])
            .map_err(|e| format!("delete memory: {}", e))?;
        Ok(())
    })
}

// ── Search ──

/// Sanitize a user query for FTS5 MATCH — escapes or strips special operators.
fn sanitize_fts_query(query: &str) -> String {
    // Wrap each word in double quotes to prevent FTS5 operator interpretation
    // (OR, AND, NOT, NEAR, *, "phrases")
    query
        .split_whitespace()
        .map(|w| {
            let clean = w.trim_matches(|c: char| c == '"' || c == '*' || c == '(' || c == ')');
            if clean.is_empty() {
                String::new()
            } else {
                format!("\"{}\"", clean)
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn search_fts(query: &str, top_k: usize, status: &str) -> Result<Vec<MemoryNode>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let safe_query = sanitize_fts_query(query);
    if safe_query.is_empty() {
        return Ok(Vec::new());
    }
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT m.id, m.content, m.memory_type, m.confidence, m.source_kind,
                    m.source_run_id, m.source_group_chat_id, m.tags, m.status,
                    m.scope, m.project_id, m.created_at, m.updated_at
                 FROM memories_fts f
                 JOIN memories m ON m.rowid = f.rowid
                 WHERE memories_fts MATCH ?1 AND m.status = ?2
                 ORDER BY rank
                 LIMIT ?3",
            )
            .map_err(|e| format!("prepare fts: {}", e))?;

        let rows = stmt
            .query_map(params![safe_query, status, top_k as i64], row_to_memory)
            .map_err(|e| format!("fts query: {}", e))?;

        let mut results = Vec::new();
        for row in rows {
            match row {
                Ok(m) => results.push(m),
                Err(e) => log::warn!("skip fts row: {}", e),
            }
        }
        Ok(results)
    })
}

pub fn search_by_tags(tags: &[String], top_k: usize) -> Result<Vec<MemoryNode>, String> {
    if tags.is_empty() {
        return Ok(Vec::new());
    }
    with_conn(|conn| {
        // Match any memory whose tags JSON array contains at least one of the requested tags
        let mut conditions = Vec::new();
        for (i, _) in tags.iter().enumerate() {
            conditions.push(format!("m.tags LIKE ?{}", i + 1));
        }
        let where_clause = conditions.join(" OR ");
        let sql = format!(
            "SELECT m.id, m.content, m.memory_type, m.confidence, m.source_kind,
                m.source_run_id, m.source_group_chat_id, m.tags, m.status,
                m.scope, m.project_id, m.created_at, m.updated_at
             FROM memories m
             WHERE m.status = 'approved' AND ({})
             ORDER BY m.created_at DESC
             LIMIT ?{}",
            where_clause,
            tags.len() + 1
        );

        let like_patterns: Vec<String> = tags.iter().map(|t| format!("%{}%", t)).collect();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        for p in &like_patterns {
            param_values.push(Box::new(p.clone()));
        }
        param_values.push(Box::new(top_k as i64));

        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare tag search: {}", e))?;
        let rows = stmt
            .query_map(params_ref.as_slice(), row_to_memory)
            .map_err(|e| format!("tag search: {}", e))?;

        let mut results = Vec::new();
        for row in rows {
            match row {
                Ok(m) => results.push(m),
                Err(e) => log::warn!("skip tag row: {}", e),
            }
        }
        Ok(results)
    })
}

/// Hybrid search: FTS for content + tag matching, merged by relevance.
/// Returns up to `top_k` results sorted by combined score.
/// Both queries run in a single lock acquisition to avoid TOCTOU inconsistencies.
pub fn search_hybrid(
    query: &str,
    tags: &[String],
    top_k: usize,
) -> Result<Vec<MemoryNode>, String> {
    let want_fts = !query.trim().is_empty();
    let want_tags = !tags.is_empty();

    if !want_fts && !want_tags {
        return Ok(Vec::new());
    }

    with_conn(|conn| {
        let mut seen = std::collections::HashSet::new();
        let mut merged = Vec::new();

        // FTS leg
        if want_fts {
            let safe_query = sanitize_fts_query(query);
            if !safe_query.is_empty() {
                if let Ok(mut stmt) = conn.prepare(
                    "SELECT m.id, m.content, m.memory_type, m.confidence, m.source_kind,
                        m.source_run_id, m.source_group_chat_id, m.tags, m.status,
                        m.scope, m.project_id, m.created_at, m.updated_at
                     FROM memories_fts f
                     JOIN memories m ON m.rowid = f.rowid
                     WHERE memories_fts MATCH ?1 AND m.status = 'approved'
                     ORDER BY rank LIMIT ?2",
                ) {
                    if let Ok(rows) =
                        stmt.query_map(params![safe_query, (top_k * 2) as i64], row_to_memory)
                    {
                        for row in rows.flatten() {
                            if seen.insert(row.id.clone()) {
                                merged.push(row);
                            }
                        }
                    }
                }
            }
        }

        // Tag leg
        if want_tags && merged.len() < top_k {
            let like_patterns: Vec<String> = tags.iter().map(|t| format!("%{}%", t)).collect();
            let conditions: Vec<String> = (1..=tags.len())
                .map(|i| format!("m.tags LIKE ?{}", i))
                .collect();
            let sql = format!(
                "SELECT m.id, m.content, m.memory_type, m.confidence, m.source_kind,
                    m.source_run_id, m.source_group_chat_id, m.tags, m.status,
                    m.scope, m.project_id, m.created_at, m.updated_at
                 FROM memories m
                 WHERE m.status = 'approved' AND ({})
                 ORDER BY m.created_at DESC LIMIT ?{}",
                conditions.join(" OR "),
                tags.len() + 1
            );
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            for p in &like_patterns {
                param_values.push(Box::new(p.clone()));
            }
            param_values.push(Box::new((top_k * 2) as i64));
            let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(|p| p.as_ref()).collect();

            if let Ok(mut stmt) = conn.prepare(&sql) {
                if let Ok(rows) = stmt.query_map(params_ref.as_slice(), row_to_memory) {
                    for row in rows.flatten() {
                        if seen.insert(row.id.clone()) {
                            merged.push(row);
                        }
                    }
                }
            }
        }

        merged.truncate(top_k);
        Ok(merged)
    })
}

/// Find duplicate candidates by FTS match on content.
pub fn find_duplicates(content: &str, threshold: f64) -> Result<Vec<MemoryNode>, String> {
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    // Use first 100 chars as FTS query to find similar memories
    let query: String = content.chars().take(100).collect();
    search_fts(&query, 5, "approved").map(|results| {
        results
            .into_iter()
            .filter(|m| {
                // Simple text similarity: ratio of shared words
                let a_words: std::collections::HashSet<&str> =
                    m.content.split_whitespace().collect();
                let b_words: std::collections::HashSet<&str> =
                    content.split_whitespace().collect();
                if a_words.is_empty() || b_words.is_empty() {
                    return false;
                }
                let intersection = a_words.intersection(&b_words).count();
                let union = a_words.union(&b_words).count();
                (intersection as f64 / union as f64) >= threshold
            })
            .collect()
    })
}

// ── Stats ──

pub fn count_memories(status: Option<&str>) -> Result<usize, String> {
    with_conn(|conn| {
        match status {
            Some(s) => conn
                .query_row(
                    "SELECT COUNT(*) FROM memories WHERE status = ?1",
                    params![s],
                    |row| row.get::<_, usize>(0),
                )
                .map_err(|e| format!("count: {}", e)),
            None => conn
                .query_row("SELECT COUNT(*) FROM memories", [], |row| {
                    row.get::<_, usize>(0)
                })
                .map_err(|e| format!("count: {}", e)),
        }
    })
}

// ── Row mapper ──

fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<MemoryNode> {
    let tags_json: String = row.get(7)?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    Ok(MemoryNode {
        id: row.get(0)?,
        character_id: String::new(),
        content: row.get(1)?,
        memory_type: row.get(2)?,
        confidence: row.get(3)?,
        source: MemorySource {
            kind: row.get(4)?,
            run_id: row.get(5)?,
            group_chat_id: row.get(6)?,
        },
        tags,
        status: row.get(8)?,
        scope: row.get(9)?,
        project_id: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

// ── Migration support ──

pub fn import_from_jsonl(path: &Path) -> Result<usize, String> {
    use std::io::{BufRead, BufReader};

    let file = std::fs::File::open(path).map_err(|e| format!("open jsonl: {}", e))?;
    let reader = BufReader::new(file);
    let mut imported = 0;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("read line: {}", e))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let node: MemoryNode = match serde_json::from_str(line) {
            Ok(n) => n,
            Err(e) => {
                log::warn!("skip invalid jsonl line: {}", e);
                continue;
            }
        };
        // Check if already exists
        if get_memory(&node.id)?.is_some() {
            continue;
        }
        insert_memory(&node)?;
        imported += 1;
    }

    Ok(imported)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MemoryNode, MemorySource};

    fn test_node(id: &str, content: &str, tags: Vec<&str>) -> MemoryNode {
        MemoryNode {
            id: id.to_string(),
            character_id: String::new(),
            content: content.to_string(),
            memory_type: "fact".to_string(),
            confidence: 0.9,
            source: MemorySource {
                kind: "extraction".to_string(),
                run_id: None,
                group_chat_id: None,
            },
            tags: tags.into_iter().map(String::from).collect(),
            status: "approved".to_string(),
            scope: "global".to_string(),
            project_id: None,
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn test_crud_cycle() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        init_db(&data_dir).unwrap();

        let node = test_node("m1", "用户是资深 Go 开发者", vec!["go", "developer"]);
        insert_memory(&node).unwrap();

        let fetched = get_memory("m1").unwrap().unwrap();
        assert_eq!(fetched.content, "用户是资深 Go 开发者");
        assert_eq!(fetched.tags, vec!["go", "developer"]);

        let all = list_memories(None, None, None, 10, 0).unwrap();
        assert_eq!(all.len(), 1);

        delete_memory("m1").unwrap();
        assert!(get_memory("m1").unwrap().is_none());
    }

    #[test]
    fn test_fts_search() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        init_db(&data_dir).unwrap();

        insert_memory(&test_node("m1", "用户喜欢用 Rust 写后端", vec!["rust"])).unwrap();
        insert_memory(&test_node("m2", "用户是前端 React 开发者", vec!["react"])).unwrap();

        let results = search_fts("Rust", 10, "approved").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "m1");
    }

    #[test]
    fn test_tag_search() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        init_db(&data_dir).unwrap();

        insert_memory(&test_node("m1", "喜欢 Go", vec!["go", "backend"])).unwrap();
        insert_memory(&test_node("m2", "喜欢 React", vec!["react", "frontend"])).unwrap();

        let results = search_by_tags(&["backend".to_string()], 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "m1");
    }

    #[test]
    fn test_find_duplicates() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_path_buf();
        init_db(&data_dir).unwrap();

        insert_memory(&test_node("m1", "用户是资深 Go 开发者", vec!["go"])).unwrap();
        insert_memory(&test_node("m2", "用户喜欢用 TypeScript", vec!["ts"])).unwrap();

        let dups = find_duplicates("用户是 Go 开发者", 0.3).unwrap();
        assert!(dups.iter().any(|m| m.id == "m1"));
    }
}
