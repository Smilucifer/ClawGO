use crate::invest::premarket::sector_em;
use crate::storage::invest::stock_board_map;
use std::collections::HashSet;

/// Refresh East Money board mapping (industry + concept batch replace).
/// Returns coverage: distinct ts_code count.
pub async fn refresh_stock_board_map() -> Result<usize, String> {
    let mut all_codes: HashSet<String> = HashSet::new();
    for board_type in ["industry", "concept"] {
        let memberships = sector_em::fetch_board_membership(Some(board_type)).await?;
        let mut rows: Vec<(String, String)> = Vec::with_capacity(memberships.len());
        for m in &memberships {
            if m.ts_code.is_empty() || m.board_name.is_empty() {
                continue;
            }
            all_codes.insert(m.ts_code.clone());
            rows.push((m.ts_code.clone(), m.board_name.clone()));
        }
        stock_board_map::replace_board_type(board_type, &rows)?;
        log::info!("stock_board_map refreshed: board_type={}, written={}", board_type, rows.len());
    }
    let total = all_codes.len();
    log::info!("stock_board_map coverage: {} distinct ts_codes", total);
    Ok(total)
}
