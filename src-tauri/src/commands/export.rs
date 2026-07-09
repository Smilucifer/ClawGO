use crate::storage;
use std::path::Path;

#[tauri::command]
pub fn export_conversation(run_id: String) -> Result<String, String> {
    log::debug!("[export] export_conversation: run_id={}", run_id);
    storage::runs::get_run(&run_id).ok_or_else(|| format!("Run {} not found", run_id))?;
    let events = storage::events::list_events(&run_id, 0);
    let mut md = String::new();
    md.push_str(&format!("# Conversation — {}\n\n", run_id));

    for event in events {
        let type_str = format!("{}", event.event_type);
        if type_str != "user" && type_str != "assistant" {
            continue;
        }
        let text = event
            .payload
            .get("text")
            .or_else(|| event.payload.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if text.is_empty() {
            continue;
        }
        let role = if type_str == "user" {
            "User"
        } else {
            "Assistant"
        };
        md.push_str(&format!("## {}\n\n{}\n\n---\n\n", role, text));
    }

    Ok(md)
}

#[tauri::command]
pub async fn write_html_export(path: String, content: String) -> Result<(), String> {
    log::debug!(
        "[export] write_html_export: path={}, content_len={}",
        path,
        content.len()
    );

    if !export_ext_allowed(&path, &["html", "htm"]) {
        log::error!(
            "[export] write_html_export rejected non-html path: {}",
            path
        );
        return Err("write_html_export: only .html/.htm paths allowed".into());
    }

    tokio::fs::write(&path, content).await.map_err(|e| {
        log::error!("[export] write_html_export failed: {}", e);
        e.to_string()
    })
}

/// 导出写盘扩展名白名单校验(大小写不敏感)。allowed 传小写扩展名列表。
fn export_ext_allowed(path: &str, allowed: &[&str]) -> bool {
    Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .is_some_and(|e| allowed.contains(&e.as_str()))
}

/// 把 base64 编码的二进制(PNG/PDF)解码后写入用户选定路径。
/// 仿 `write_html_export`,但白名单为 .png/.pdf,内容走 base64 解码。
#[tauri::command]
pub async fn write_binary_export(path: String, base64: String) -> Result<(), String> {
    use base64::Engine;
    log::debug!(
        "[export] write_binary_export: path={}, b64_len={}",
        path,
        base64.len()
    );
    if !export_ext_allowed(&path, &["png", "pdf"]) {
        log::error!("[export] write_binary_export rejected path: {}", path);
        return Err("write_binary_export: only .png/.pdf paths allowed".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64.as_bytes())
        .map_err(|e| {
            log::error!("[export] write_binary_export base64 decode failed: {}", e);
            e.to_string()
        })?;
    tokio::fs::write(&path, bytes).await.map_err(|e| {
        log::error!("[export] write_binary_export write failed: {}", e);
        e.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::export_ext_allowed;

    #[test]
    fn accepts_png_and_pdf_case_insensitive() {
        assert!(export_ext_allowed("C:/tmp/a.png", &["png", "pdf"]));
        assert!(export_ext_allowed("/tmp/a.PDF", &["png", "pdf"]));
        assert!(export_ext_allowed("report.Png", &["png", "pdf"]));
    }

    #[test]
    fn rejects_other_extensions() {
        assert!(!export_ext_allowed("a.exe", &["png", "pdf"]));
        assert!(!export_ext_allowed("a.html", &["png", "pdf"]));
        assert!(!export_ext_allowed("noext", &["png", "pdf"]));
    }
}
