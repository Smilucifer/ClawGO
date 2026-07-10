//! East Money board membership and sector strength via Python akshare bridge.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardMembership {
    pub ts_code: String,
    pub board_name: String,
    pub board_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardStrength {
    pub board_name: String,
    pub board_type: String,
    #[serde(default)]
    pub change_pct: Option<f64>,
    #[serde(default)]
    pub net_amount: Option<f64>,
}

/// Fetch East Money board constituents.
/// `board_type`: None = industry+concept all; Some("industry") / Some("concept") partial.
pub async fn fetch_board_membership(
    board_type: Option<&str>,
) -> Result<Vec<BoardMembership>, String> {
    let runtime = crate::python::require()?;
    let params = match board_type {
        Some(t) => serde_json::json!({ "board_type": t }),
        None => serde_json::json!({ "board_type": null }),
    };
    let value = runtime
        .call("akshare_sector.board_cons_em", params)
        .await?;
    serde_json::from_value::<Vec<BoardMembership>>(value)
        .map_err(|e| format!("parse akshare_sector.board_cons_em: {e}"))
}

/// Fetch East Money sector strength (industry+concept merged).
pub async fn fetch_sector_strength_em() -> Result<Vec<BoardStrength>, String> {
    let runtime = crate::python::require()?;
    let value = runtime
        .call("akshare_sector.sector_strength_em", serde_json::json!({}))
        .await?;
    serde_json::from_value::<Vec<BoardStrength>>(value)
        .map_err(|e| format!("parse akshare_sector.sector_strength_em: {e}"))
}
