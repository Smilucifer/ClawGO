use std::io::Read;
use std::path::Path;

fn validate_image_file(path: &Path) -> Result<(), String> {
    let mut buf = [0u8; 8];
    std::fs::File::open(path)
        .map_err(|e| format!("Cannot open avatar source: {e}"))?
        .read_exact(&mut buf)
        .map_err(|e| format!("Cannot read avatar source: {e}"))?;
    let is_png = buf == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    let is_jpg = buf[0] == 0xFF && buf[1] == 0xD8 && buf[2] == 0xFF;
    if !is_png && !is_jpg {
        return Err("File is not a valid jpg, jpeg, or png image".into());
    }
    Ok(())
}

#[tauri::command]
pub fn upload_character_avatar(
    character_id: String,
    file_path: String,
) -> Result<String, String> {
    log::debug!("[avatar] upload_character_avatar: character_id={}, file_path={}", character_id, file_path);

    crate::storage::characters::validate_character_id(&character_id)?;

    let src = Path::new(&file_path);
    validate_image_file(src)?;

    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");
    let filename = format!("avatar.{}", ext);
    let dst = crate::storage::characters::char_dir(&character_id).join(&filename);

    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
    Ok(dst.to_string_lossy().to_string())
}
