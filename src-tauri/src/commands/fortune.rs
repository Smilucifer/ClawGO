//! 每日盈记 Tauri 命令边界。薄封装：委托 storage + aggregate。
use crate::invest::fortune::aggregate::{self, Analysis, Overview, DataSummary};
use crate::storage::invest::fortune as store;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchEntry { pub date: String, pub return_pct: f64, pub note: String }

#[tauri::command]
pub fn fortune_upsert_return(date: String, return_pct: f64, note: String) -> Result<(), String> {
    store::upsert_return(&date, return_pct, &note)
}

#[tauri::command]
pub fn fortune_batch_upsert(entries: Vec<BatchEntry>) -> Result<(), String> {
    for e in &entries {
        store::upsert_return(&e.date, e.return_pct, &e.note)?;
    }
    Ok(())
}

#[tauri::command]
pub fn fortune_delete_return(date: String) -> Result<(), String> {
    store::delete_return(&date)
}

#[tauri::command]
pub fn fortune_get_analysis() -> Result<Analysis, String> { aggregate::compute_analysis() }

#[tauri::command]
pub fn fortune_get_overview() -> Result<Overview, String> { aggregate::compute_overview() }

#[tauri::command]
pub fn fortune_get_data_summary() -> Result<DataSummary, String> { aggregate::compute_data_summary() }

#[tauri::command]
pub async fn fortune_generate_reading(date: String) -> Result<String, String> {
    crate::invest::fortune::reading::generate_reading(&date).await
}

#[tauri::command]
pub fn fortune_get_reading(date: String) -> Result<Option<String>, String> {
    store::get_latest_reading(&date)
}
