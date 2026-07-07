use crate::storage::memos::{self, MemoItem, MemoScope};

fn clean_text(text: String) -> Result<String, String> {
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        Err("Memo text cannot be empty".to_string())
    } else {
        Ok(trimmed)
    }
}

#[tauri::command]
pub fn list_memos(scope: MemoScope) -> Result<Vec<MemoItem>, String> {
    Ok(memos::list_memos(scope))
}

#[tauri::command]
pub fn add_memo(scope: MemoScope, text: String) -> Result<MemoItem, String> {
    memos::add_memo(scope, clean_text(text)?)
}

#[tauri::command]
pub fn update_memo(scope: MemoScope, id: String, text: String) -> Result<MemoItem, String> {
    memos::update_memo(scope, id, clean_text(text)?)
}

#[tauri::command]
pub fn delete_memo(scope: MemoScope, id: String) -> Result<(), String> {
    memos::delete_memo(scope, id)
}

#[tauri::command]
pub fn clear_memos(scope: MemoScope) -> Result<(), String> {
    memos::clear_memos(scope)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_temp_data_dir<T>(f: impl FnOnce() -> T) -> T {
        let _guard = crate::storage::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("CLAW_GO_DATA_DIR");

        std::env::set_var("CLAW_GO_DATA_DIR", tmp.path());
        let result = f();

        match previous {
            Some(value) => std::env::set_var("CLAW_GO_DATA_DIR", value),
            None => std::env::remove_var("CLAW_GO_DATA_DIR"),
        }

        result
    }

    #[test]
    fn add_memo_rejects_empty_text() {
        with_temp_data_dir(|| {
            assert!(add_memo(MemoScope::Global, "   ".to_string()).is_err());
        });
    }

    #[test]
    fn add_update_list_and_clear_memos_through_commands() {
        with_temp_data_dir(|| {
            let added = add_memo(MemoScope::Global, "  first  ".to_string()).unwrap();
            assert_eq!(added.text, "first");
            assert_eq!(list_memos(MemoScope::Global).unwrap(), vec![added.clone()]);

            let updated = update_memo(
                MemoScope::Global,
                added.id.clone(),
                "  changed  ".to_string(),
            )
            .unwrap();
            assert_eq!(updated.text, "changed");

            delete_memo(MemoScope::Global, updated.id).unwrap();
            assert!(list_memos(MemoScope::Global).unwrap().is_empty());

            add_memo(MemoScope::Global, "again".to_string()).unwrap();
            clear_memos(MemoScope::Global).unwrap();
            assert!(list_memos(MemoScope::Global).unwrap().is_empty());
        });
    }
}
