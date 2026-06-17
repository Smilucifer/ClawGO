//! Committee live-queue persistence.
//!
//! Stores the live-tab execution queue and a portfolio snapshot to
//! `~/.claw-go/invest/committee-queue.json` using the same atomic
//! tmp+rename pattern as `storage::runs::save_meta`. The frontend store is
//! the source of truth; this module only loads/saves.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueItemStatus {
    Queued,
    Running,
    Done,
    Failed,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub symbol: String,
    pub status: QueueItemStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotHolding {
    pub symbol: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shares: Option<f64>,
    pub notional: f64,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioSnapshot {
    pub holdings: Vec<SnapshotHolding>,
    pub cash: f64,
    pub total_notional: f64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CommitteeQueueState {
    #[serde(default)]
    pub items: Vec<QueueItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<PortfolioSnapshot>,
    #[serde(default)]
    pub max_concurrent: usize,
    #[serde(default)]
    pub updated_at: String,
}

fn queue_path() -> Result<PathBuf, String> {
    let invest_dir = crate::storage::data_dir().join("invest");
    crate::storage::ensure_dir(&invest_dir).map_err(|e| format!("create invest dir: {e}"))?;
    Ok(invest_dir.join("committee-queue.json"))
}

/// Load persisted queue state. Returns default (empty) state when the file is
/// missing or fails to parse — never errors, so the live tab always opens.
pub fn load_queue() -> CommitteeQueueState {
    let path = match queue_path() {
        Ok(p) => p,
        Err(e) => {
            log::warn!("committee queue: path resolve failed: {e}");
            return CommitteeQueueState::default();
        }
    };
    if !path.exists() {
        return CommitteeQueueState::default();
    }
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|e| {
            log::warn!("committee queue: parse failed: {e}");
            CommitteeQueueState::default()
        }),
        Err(e) => {
            log::warn!("committee queue: read failed: {e}");
            CommitteeQueueState::default()
        }
    }
}

/// Persist queue state atomically (tmp write + rename, retry on
/// PermissionDenied), mirroring `storage::runs::save_meta`.
pub fn save_queue(state: &CommitteeQueueState) -> Result<(), String> {
    let path = queue_path()?;
    let dir = path
        .parent()
        .ok_or_else(|| "queue path has no parent".to_string())?;
    let tmp = dir.join(format!(
        "committee-queue.json.{}.{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let json = serde_json::to_string(state).map_err(|e| e.to_string())?;
    fs::write(&tmp, &json).map_err(|e| format!("write tmp: {e}"))?;
    for attempt in 0..3u8 {
        match fs::rename(&tmp, &path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && attempt < 2 => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                return Err(format!("rename: {e}"));
            }
        }
    }
    let _ = fs::remove_file(&tmp);
    Err("rename: PermissionDenied after 3 retries".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_state_roundtrips_through_json() {
        let state = CommitteeQueueState {
            items: vec![
                QueueItem {
                    symbol: "600519".into(),
                    status: QueueItemStatus::Done,
                    error: None,
                },
                QueueItem {
                    symbol: "000001".into(),
                    status: QueueItemStatus::Failed,
                    error: Some("boom".into()),
                },
            ],
            snapshot: Some(PortfolioSnapshot {
                holdings: vec![SnapshotHolding {
                    symbol: "600519".into(),
                    name: Some("贵州茅台".into()),
                    shares: Some(100.0),
                    notional: 170000.0,
                    kind: "hold".into(),
                }],
                cash: 50000.0,
                total_notional: 170000.0,
                timestamp: "2026-06-18T00:00:00Z".into(),
            }),
            max_concurrent: 5,
            updated_at: "2026-06-18T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: CommitteeQueueState = serde_json::from_str(&json).unwrap();
        assert_eq!(back.items.len(), 2);
        assert_eq!(back.items[0].symbol, "600519");
        assert_eq!(back.items[0].status, QueueItemStatus::Done);
        assert_eq!(back.items[1].error.as_deref(), Some("boom"));
        assert_eq!(back.max_concurrent, 5);
        assert_eq!(back.snapshot.unwrap().holdings[0].kind, "hold");
    }

    #[test]
    fn status_serializes_snake_case() {
        let json = serde_json::to_string(&QueueItemStatus::Aborted).unwrap();
        assert_eq!(json, "\"aborted\"");
    }

    #[test]
    fn default_state_is_empty() {
        let s = CommitteeQueueState::default();
        assert!(s.items.is_empty());
        assert!(s.snapshot.is_none());
        assert_eq!(s.max_concurrent, 0);
    }
}
