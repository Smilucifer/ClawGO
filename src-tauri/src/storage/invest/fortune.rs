//! 每日盈记存储：手动录入的每日收益率 + AI 解读。复用 invest.db。
use rusqlite::Connection;
use super::{with_conn, with_conn_mut};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReturn {
    pub date: String,
    pub return_pct: f64,
    pub note: String,
    pub created_at: String,
    pub updated_at: String,
}

pub fn create_table(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS fortune_daily_returns (
            date TEXT PRIMARY KEY, return_pct REAL NOT NULL,
            note TEXT DEFAULT '', created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS fortune_ai_readings (
            id INTEGER PRIMARY KEY AUTOINCREMENT, date TEXT NOT NULL,
            content TEXT NOT NULL, created_at TEXT NOT NULL);",
    )
    .map_err(|e| format!("建 fortune 表失败: {e}"))
}

fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn upsert_return(date: &str, return_pct: f64, note: &str) -> Result<(), String> {
    let ts = now_iso();
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO fortune_daily_returns (date, return_pct, note, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(date) DO UPDATE SET return_pct=?2, note=?3, updated_at=?4",
            rusqlite::params![date, return_pct, note, ts],
        )
        .map_err(|e| format!("upsert 收益失败: {e}"))?;
        Ok(())
    })
}

pub fn delete_return(date: &str) -> Result<(), String> {
    with_conn_mut(|c| {
        c.execute("DELETE FROM fortune_daily_returns WHERE date=?1", [date])
            .map_err(|e| format!("删除收益失败: {e}"))?;
        Ok(())
    })
}

pub fn list_returns() -> Result<Vec<DailyReturn>, String> {
    with_conn(|c| {
        let mut stmt = c
            .prepare("SELECT date, return_pct, note, created_at, updated_at
                      FROM fortune_daily_returns ORDER BY date ASC")
            .map_err(|e| format!("查询收益失败: {e}"))?;
        let rows = stmt
            .query_map([], |r| Ok(DailyReturn {
                date: r.get(0)?, return_pct: r.get(1)?, note: r.get(2)?,
                created_at: r.get(3)?, updated_at: r.get(4)?,
            }))
            .map_err(|e| format!("映射收益失败: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("收集收益失败: {e}"))
    })
}

pub fn insert_reading(date: &str, content: &str) -> Result<i64, String> {
    let ts = now_iso();
    with_conn_mut(|c| {
        c.execute(
            "INSERT INTO fortune_ai_readings (date, content, created_at) VALUES (?1,?2,?3)",
            rusqlite::params![date, content, ts],
        )
        .map_err(|e| format!("插入解读失败: {e}"))?;
        Ok(c.last_insert_rowid())
    })
}

pub fn get_latest_reading(date: &str) -> Result<Option<String>, String> {
    with_conn(|c| {
        c.query_row(
            "SELECT content FROM fortune_ai_readings WHERE date=?1
             ORDER BY id DESC LIMIT 1",
            [date],
            |r| r.get::<_, String>(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(format!("查询解读失败: {other}")),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_table_and_upsert_semantics() {
        let conn = Connection::open_in_memory().unwrap();
        create_table(&conn).unwrap();
        // upsert 两次同日期：第二次应覆盖 return_pct + note，行数保持 1
        conn.execute("INSERT INTO fortune_daily_returns (date,return_pct,note,created_at,updated_at) VALUES ('2026-07-01',1.5,'a','t','t')", []).unwrap();
        conn.execute("INSERT INTO fortune_daily_returns (date,return_pct,note,created_at,updated_at) VALUES ('2026-07-01',2.5,'b','t','t2') ON CONFLICT(date) DO UPDATE SET return_pct=2.5, note='b', updated_at='t2'", []).unwrap();
        let (n, ret, note): (i64, f64, String) = conn.query_row(
            "SELECT COUNT(*), MAX(return_pct), MAX(note) FROM fortune_daily_returns",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!(n, 1);
        assert_eq!(ret, 2.5);
        assert_eq!(note, "b");
    }

    #[test]
    fn readings_latest_is_highest_id() {
        let conn = Connection::open_in_memory().unwrap();
        create_table(&conn).unwrap();
        conn.execute("INSERT INTO fortune_ai_readings (date,content,created_at) VALUES ('2026-07-01','old','t1')", []).unwrap();
        conn.execute("INSERT INTO fortune_ai_readings (date,content,created_at) VALUES ('2026-07-01','new','t2')", []).unwrap();
        let latest: String = conn.query_row(
            "SELECT content FROM fortune_ai_readings WHERE date='2026-07-01' ORDER BY id DESC LIMIT 1",
            [], |r| r.get(0)).unwrap();
        assert_eq!(latest, "new");
    }
}
