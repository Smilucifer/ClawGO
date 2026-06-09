use std::fs;
use std::path::Path;

use crate::storage::invest::events;
use crate::storage::invest::portfolio;
use crate::storage::invest::verdicts::{self, PnlSnapshot};
use crate::storage::invest::with_conn;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReportRecord {
    pub id: i64,
    pub report_date: String,
    pub summary: Option<String>,
    pub file_path: Option<String>,
}

pub fn generate_daily_report(data_dir: &Path) -> Result<String, String> {
    let today = crate::invest::date_utils::get_invest_date();
    let report_dir = data_dir.join("invest").join("reports");
    fs::create_dir_all(&report_dir)
        .map_err(|e| format!("create report dir: {e}"))?;

    let file_path = report_dir.join(format!("daily_{today}.md"));

    // Gather data — propagate errors so the scheduler logs them
    // instead of silently generating a report with fabricated empty data
    let snapshots = verdicts::list_pnl_snapshots(Some(1))
        .map_err(|e| format!("list_pnl_snapshots: {e}"))?;
    let snapshot: Option<&PnlSnapshot> = snapshots.first();
    let holdings = portfolio::list_holdings()
        .map_err(|e| format!("list_holdings: {e}"))?;
    let cash = portfolio::get_cash()
        .map_err(|e| format!("get_cash: {e}"))?;
    let verdicts = verdicts::list_verdicts(None, Some(5))
        .map_err(|e| format!("list_verdicts: {e}"))?;
    let recent_events = events::list_events(None, Some(5))
        .map_err(|e| format!("list_events: {e}"))?;

    let mut md = String::new();
    md.push_str(&format!("# Daily Report — {today}\n\n"));

    // Portfolio summary
    md.push_str("## Portfolio\n\n");
    if let Some(snap) = snapshot {
        md.push_str(&format!(
            "- **Total Value**: ¥{:.2}\n- **Cash**: ¥{:.2}\n- **Holdings**: ¥{:.2}\n",
            snap.total_value, snap.cash, snap.holdings_value
        ));
        if let (Some(daily_pnl), Some(daily_pnl_pct)) = (snap.daily_pnl, snap.daily_pnl_pct) {
            md.push_str(&format!(
                "- **Daily PnL**: ¥{:.2} ({:.2}%)\n",
                daily_pnl, daily_pnl_pct
            ));
        }
    } else {
        md.push_str(&format!("- Cash: ¥{:.2}\n- No PnL snapshot yet\n", cash));
    }
    md.push('\n');

    // Holdings
    if !holdings.is_empty() {
        md.push_str("## Holdings\n\n");
        md.push_str("| Symbol | Name | Shares | Avg Cost | Notional |\n");
        md.push_str("|--------|------|--------|----------|----------|\n");
        for h in &holdings {
            md.push_str(&format!(
                "| {} | {} | {} | {} | ¥{:.2} |\n",
                h.symbol,
                h.name.as_deref().unwrap_or("-"),
                h.shares.map(|s| s.to_string()).unwrap_or("-".into()),
                h.avg_cost.map(|c| format!("{:.3}", c)).unwrap_or("-".into()),
                h.notional
            ));
        }
        md.push('\n');
    }

    // Recent verdicts
    if !verdicts.is_empty() {
        md.push_str("## Recent Verdicts\n\n");
        for v in &verdicts {
            md.push_str(&format!(
                "- **{}** — {} (confidence: {})\n",
                v.symbol,
                v.verdict,
                v.confidence
                    .map(|c| format!("{:.0}%", c * 100.0))
                    .unwrap_or("-".into())
            ));
        }
        md.push('\n');
    }

    // Recent events
    if !recent_events.is_empty() {
        md.push_str("## Recent Events\n\n");
        for e in &recent_events {
            md.push_str(&format!(
                "- [{}] {} — {}\n",
                e.source, e.title, e.severity
            ));
        }
        md.push('\n');
    }

    // Write file
    fs::write(&file_path, &md).map_err(|e| format!("write report: {e}"))?;

    // Save to DB
    let file_path_str = file_path.to_string_lossy().to_string();
    with_conn(|conn| {
        conn.execute(
            "INSERT INTO daily_reports (report_date, summary, file_path) VALUES (?1, ?2, ?3) \
             ON CONFLICT(report_date) DO UPDATE SET summary = excluded.summary, file_path = excluded.file_path",
            rusqlite::params![today, md.lines().next().unwrap_or("").trim().trim_start_matches("# ").trim(), file_path_str],
        )
        .map_err(|e| format!("save report record: {e}"))?;
        Ok(())
    })?;

    Ok(format!("Report generated: {}", file_path.display()))
}

pub fn list_daily_reports(limit: i64) -> Result<Vec<DailyReportRecord>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, report_date, summary, file_path FROM daily_reports \
                 ORDER BY report_date DESC LIMIT ?1",
            )
            .map_err(|e| format!("prepare list_daily_reports: {e}"))?;

        let rows = stmt
            .query_map([limit], |row| {
                Ok(DailyReportRecord {
                    id: row.get(0)?,
                    report_date: row.get(1)?,
                    summary: row.get(2)?,
                    file_path: row.get(3)?,
                })
            })
            .map_err(|e| format!("query list_daily_reports: {e}"))?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r.map_err(|e| format!("read row: {e}"))?);
        }
        Ok(results)
    })
}
