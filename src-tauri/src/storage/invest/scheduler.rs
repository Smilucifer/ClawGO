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
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
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
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
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
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // No calendar row: either not synced yet or no Tushare token.
                // Fall back to weekday heuristic so the scheduler keeps firing
                // — skipping every requires_trading_day job on miss would
                // silently stop dream/verdict/pnl for token-less users, which
                // is worse than the occasional holiday misfire. The warn makes
                // the fallback observable so operators can backfill.
                // Future option (NOT implemented): strict-mode skip on miss.
                log::warn!("[trade_calendar] miss for {date}, falling back to weekday heuristic");
                Ok(is_weekday(date))
            }
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

/// 返回北京时间当天的日历日期（`%Y-%m-%d`）。
///
/// 这是日历日，与 `crate::invest::date_utils::get_invest_date` 不同：
/// - get_invest_date 含 05:00 cutoff（业务日），用于 PnL 按交易日聚合；
/// - beijing_today 不含 cutoff，用于"今天是不是 A 股交易日"，并锁死东八区，
///   使行为不依赖宿主机本地时区（与 is_a_share_market_open 同口径）。
pub fn beijing_today() -> String {
    let cst = chrono::Utc::now() + chrono::Duration::hours(8);
    cst.format("%Y-%m-%d").to_string()
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

/// Retention pruning: delete scheduler_logs rows whose started_at is older
/// than keep_days days. Returns the number of rows deleted.
///
/// started_at is a UTC rfc3339 string with millisecond precision and a
/// trailing Z (written by log_task_start). UTC rfc3339 strings of identical
/// shape sort lexicographically the same as chronologically, so the string
/// filter is sound as long as the cutoff uses the same to_rfc3339_opts call.
pub fn prune_scheduler_logs(keep_days: i64) -> Result<usize, String> {
    if keep_days < 0 {
        return Err(format!("keep_days must be >= 0, got {keep_days}"));
    }
    with_conn_mut(|conn| {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(keep_days))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let deleted = conn
            .execute(
                "DELETE FROM scheduler_logs WHERE started_at < ?1",
                params![cutoff],
            )
            .map_err(|e| format!("prune scheduler_logs: {}", e))?;
        Ok(deleted)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn beijing_today_returns_parseable_yyyy_mm_dd() {
        let s = beijing_today();
        assert_eq!(s.len(), 10, "expected YYYY-MM-DD length, got {s:?}");
        assert!(NaiveDate::parse_from_str(&s, "%Y-%m-%d").is_ok());
    }

    #[test]
    fn beijing_today_is_within_one_day_of_utc() {
        let utc = chrono::Utc::now();
        let utc_date = utc.format("%Y-%m-%d").to_string();
        let utc_plus1 = (utc + chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
        let s = beijing_today();
        assert!(s == utc_date || s == utc_plus1, "beijing_today {s} vs {utc_date}/{utc_plus1}");
    }
}
