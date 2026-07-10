//! stock_board_map：个股 → 板块（行业/概念）映射表。
//!
//! 每个 ts_code 可属于多个板块，每个板块有 board_type（如 "industry"、"concept"）。
//! 全量刷新时按 board_type 整体替换（先 DELETE 再 INSERT）。

use super::{with_conn, with_conn_mut};
use rusqlite::{params, Connection};

/// 建表 SQL（供 `init_db_inner` 迁移期调用）。
pub const CREATE_STOCK_BOARD_MAP_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS stock_board_map (
    ts_code TEXT NOT NULL,
    board_name TEXT NOT NULL,
    board_type TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (ts_code, board_type, board_name)
);
CREATE INDEX IF NOT EXISTS idx_board_map_name ON stock_board_map(board_name);
"#;

/// 按 board_type 全量替换板块映射（事务内先 DELETE 再 INSERT OR REPLACE）。
///
/// `rows` 为 (ts_code, board_name) 元组列表，全部标记为同一 `board_type`。
pub fn replace_board_type_on(
    conn: &mut Connection,
    board_type: &str,
    rows: &[(String, String)],
) -> Result<(), String> {
    let tx = conn
        .transaction()
        .map_err(|e| format!("begin replace_board_type tx: {e}"))?;
    tx.execute(
        "DELETE FROM stock_board_map WHERE board_type = ?1",
        params![board_type],
    )
    .map_err(|e| format!("delete board_type={board_type}: {e}"))?;
    for (ts_code, board_name) in rows {
        tx.execute(
            "INSERT OR REPLACE INTO stock_board_map (ts_code, board_name, board_type, updated_at) \
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![ts_code, board_name, board_type],
        )
        .map_err(|e| format!("insert ({ts_code},{board_name}): {e}"))?;
    }
    tx.commit()
        .map_err(|e| format!("commit replace_board_type: {e}"))?;
    Ok(())
}

/// 查询指定个股所属的全部板块，返回 (board_name, board_type) 列表。
pub fn boards_of_on(
    conn: &Connection,
    code6: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT board_name, board_type FROM stock_board_map \
             WHERE ts_code = ?1 ORDER BY board_type, board_name",
        )
        .map_err(|e| format!("prepare boards_of: {e}"))?;
    let rows = stmt
        .query_map(params![code6], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| format!("query boards_of: {e}"))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| format!("row boards_of: {e}"))?);
    }
    Ok(out)
}

/// 返回全量映射：code6 → Vec<(board_name, board_type)>。
pub fn all_board_maps_on(
    conn: &Connection,
) -> Result<std::collections::HashMap<String, Vec<(String, String)>>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT ts_code, board_name, board_type FROM stock_board_map \
             ORDER BY ts_code, board_type, board_name",
        )
        .map_err(|e| format!("prepare all_board_maps: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("query all_board_maps: {e}"))?;
    let mut map: std::collections::HashMap<String, Vec<(String, String)>> =
        std::collections::HashMap::new();
    for r in rows {
        let (code, name, btype) = r.map_err(|e| format!("row all_board_maps: {e}"))?;
        map.entry(code).or_default().push((name, btype));
    }
    Ok(map)
}

/// 表中总行数（用于覆盖度判断）。
pub fn board_map_count_on(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT COUNT(*) FROM stock_board_map", [], |r| r.get(0))
        .map_err(|e| format!("board_map_count: {e}"))
}

// ── with_conn / with_conn_mut wrappers ──────────────────────────────────────

/// 按 board_type 全量替换板块映射（通过全局静态连接）。
pub fn replace_board_type(board_type: &str, rows: &[(String, String)]) -> Result<(), String> {
    with_conn_mut(|conn| replace_board_type_on(conn, board_type, rows))
}

/// 建表入口（供 init_db_inner 使用）。
pub fn create_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(CREATE_STOCK_BOARD_MAP_TABLE)
        .map_err(|e| format!("create stock_board_map table: {e}"))
}

/// 查询指定个股所属板块（通过全局静态连接）。
pub fn boards_of(code6: &str) -> Result<Vec<(String, String)>, String> {
    with_conn(|conn| boards_of_on(conn, code6))
}

/// 全量映射（通过全局静态连接）。
pub fn all_board_maps() -> Result<std::collections::HashMap<String, Vec<(String, String)>>, String> {
    with_conn(|conn| all_board_maps_on(conn))
}

/// 表中总行数（通过全局静态连接）。
pub fn board_map_count() -> Result<i64, String> {
    with_conn(|conn| board_map_count_on(conn))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CREATE_STOCK_BOARD_MAP_TABLE).unwrap();
        conn
    }

    #[test]
    fn test_replace_board_type_and_query() {
        let mut conn = setup_db();

        // Insert industry rows
        let rows = vec![
            ("600519.SH".to_string(), "白酒".to_string()),
            ("000858.SZ".to_string(), "白酒".to_string()),
        ];
        replace_board_type_on(&mut conn, "industry", &rows).unwrap();

        // Query boards for 600519
        let boards = boards_of_on(&conn, "600519.SH").unwrap();
        assert_eq!(boards.len(), 1);
        assert_eq!(boards[0].0, "白酒");
        assert_eq!(boards[0].1, "industry");

        // Full replace same board_type — old rows deleted, new rows inserted
        let new_rows = vec![
            ("600519.SH".to_string(), "消费".to_string()),
            ("300750.SZ".to_string(), "新能源".to_string()),
        ];
        replace_board_type_on(&mut conn, "industry", &new_rows).unwrap();

        // 000858.SZ should be gone (not in new batch)
        let boards_000858 = boards_of_on(&conn, "000858.SZ").unwrap();
        assert!(boards_000858.is_empty());

        // 600519.SH should now be 消费
        let boards_600519 = boards_of_on(&conn, "600519.SH").unwrap();
        assert_eq!(boards_600519.len(), 1);
        assert_eq!(boards_600519[0].0, "消费");
    }

    #[test]
    fn test_multiple_board_types_and_all_maps() {
        let mut conn = setup_db();

        replace_board_type_on(
            &mut conn,
            "industry",
            &[("600519.SH".into(), "白酒".into())],
        )
        .unwrap();
        replace_board_type_on(
            &mut conn,
            "concept",
            &[("600519.SH".into(), "MSCI概念".into())],
        )
        .unwrap();

        // boards_of returns both types
        let boards = boards_of_on(&conn, "600519.SH").unwrap();
        assert_eq!(boards.len(), 2);
        // Ordered by board_type, board_name
        assert_eq!(boards[0], ("MSCI概念".to_string(), "concept".to_string()));
        assert_eq!(boards[1], ("白酒".to_string(), "industry".to_string()));

        // all_board_maps returns full map
        let map = all_board_maps_on(&conn).unwrap();
        assert_eq!(map.len(), 1);
        let v = map.get("600519.SH").unwrap();
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn test_board_map_count_and_empty() {
        let mut conn = setup_db();

        assert_eq!(board_map_count_on(&conn).unwrap(), 0);

        replace_board_type_on(
            &mut conn,
            "industry",
            &[
                ("600519.SH".into(), "白酒".into()),
                ("000858.SZ".into(), "白酒".into()),
            ],
        )
        .unwrap();
        assert_eq!(board_map_count_on(&conn).unwrap(), 2);

        // Replace same type with 3 rows → count becomes 3
        replace_board_type_on(
            &mut conn,
            "industry",
            &[
                ("600519.SH".into(), "白酒".into()),
                ("000858.SZ".into(), "白酒".into()),
                ("300750.SZ".into(), "新能源".into()),
            ],
        )
        .unwrap();
        assert_eq!(board_map_count_on(&conn).unwrap(), 3);
    }
}
