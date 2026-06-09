use super::with_conn;
use super::with_conn_mut;
use chrono::{Datelike, NaiveDate, Timelike};
use rusqlite::params;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerLog {
    pub id: i64,
    pub task_name: String,
    pub status: String,
    pub message: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
}

pub fn log_task_start(task_name: &str) -> Result<i64, String> {
    with_conn_mut(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO scheduler_logs (task_name, status, started_at) VALUES (?1, 'running', ?2)",
            params![task_name, now],
        )
        .map_err(|e| format!("log start: {}", e))?;
        Ok(conn.last_insert_rowid())
    })
}

pub fn log_task_end(id: i64, status: &str, message: Option<&str>) -> Result<(), String> {
    with_conn(|conn| {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE scheduler_logs SET status = ?1, message = ?2, finished_at = ?3, duration_ms = CAST((julianday(?3) - julianday(started_at)) * 86400000 AS INTEGER) WHERE id = ?4",
            params![status, message, now, id],
        )
        .map_err(|e| format!("log end: {}", e))?;
        Ok(())
    })
}

pub fn list_scheduler_logs(task_name: Option<&str>, limit: Option<i64>) -> Result<Vec<SchedulerLog>, String> {
    with_conn(|conn| {
        let limit_val = limit.unwrap_or(50);
        let (sql, query_params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match task_name {
            Some(t) => (
                "SELECT id, task_name, status, message, started_at, finished_at, duration_ms FROM scheduler_logs WHERE task_name = ?1 ORDER BY started_at DESC LIMIT ?2",
                vec![Box::new(t.to_string()), Box::new(limit_val)],
            ),
            None => (
                "SELECT id, task_name, status, message, started_at, finished_at, duration_ms FROM scheduler_logs ORDER BY started_at DESC LIMIT ?1",
                vec![Box::new(limit_val)],
            ),
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(query_params.iter()), |row| {
                Ok(SchedulerLog {
                    id: row.get(0)?,
                    task_name: row.get(1)?,
                    status: row.get(2)?,
                    message: row.get(3)?,
                    started_at: row.get(4)?,
                    finished_at: row.get(5)?,
                    duration_ms: row.get(6)?,
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

pub fn get_task_logs(task: &str, limit: Option<i64>) -> Result<Vec<SchedulerLog>, String> {
    list_scheduler_logs(Some(task), limit)
}

pub fn is_trading_day(date: &str) -> Result<bool, String> {
    with_conn(|conn| {
        let result = conn.query_row(
            "SELECT is_open FROM trade_calendar WHERE cal_date = ?1",
            params![date],
            |row| row.get::<_, i32>(0),
        );
        match result {
            Ok(v) => Ok(v != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(is_weekday(date)),
            Err(e) => Err(format!("check trading day: {}", e)),
        }
    })
}

fn is_weekday(date: &str) -> bool {
    if let Ok(d) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        let weekday = d.weekday();
        weekday != chrono::Weekday::Sat && weekday != chrono::Weekday::Sun
    } else if let Ok(d) = NaiveDate::parse_from_str(date, "%Y%m%d") {
        let weekday = d.weekday();
        weekday != chrono::Weekday::Sat && weekday != chrono::Weekday::Sun
    } else {
        false
    }
}

/// 判断 A 股市场当前是否在盘中交易时段。
/// 使用 `trade_calendar` 表判断今天是否为交易日（含节假日/调休），再检查北京时间是否在 9:15-11:30 / 13:00-15:00。
pub fn is_a_share_market_open() -> bool {
    let now = chrono::Utc::now();
    let cst = now + chrono::Duration::hours(8);
    let today = cst.format("%Y-%m-%d").to_string();

    if !is_trading_day(&today).unwrap_or(false) {
        return false;
    }

    let minutes = cst.hour() * 60 + cst.minute();
    // 集合竞价 9:15 - 上午收盘 11:30
    if minutes >= 9 * 60 + 15 && minutes <= 11 * 60 + 30 {
        return true;
    }
    // 下午开盘 13:00 - 收盘 15:00
    minutes >= 13 * 60 && minutes <= 15 * 60
}

pub fn upsert_trade_calendar(date: &str, is_open: bool, pretrade_date: Option<&str>) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT INTO trade_calendar (cal_date, is_open, pretrade_date) VALUES (?1, ?2, ?3) ON CONFLICT(cal_date) DO UPDATE SET is_open=?2, pretrade_date=?3",
            params![date, is_open as i32, pretrade_date],
        )
        .map_err(|e| format!("upsert calendar: {}", e))?;
        Ok(())
    })
}
