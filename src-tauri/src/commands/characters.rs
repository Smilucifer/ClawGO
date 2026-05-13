use crate::models::{AiCharacter, AllSettings};
use crate::storage;

#[tauri::command]
pub fn list_characters() -> Result<Vec<AiCharacter>, String> {
    log::debug!("[characters] list_characters");
    let settings = storage::settings::get_user_settings();
    Ok(settings.ai_characters)
}

#[tauri::command]
pub fn create_character(
    label: String,
    role_type: String,
    role_instruction: Option<String>,
    default_provider: String,
    default_model: Option<String>,
    icon: Option<String>,
) -> Result<AiCharacter, String> {
    log::debug!("[characters] create_character: label={}", label);
    let trimmed_label = label.trim().to_string();
    if trimmed_label.is_empty() {
        return Err("Character label cannot be empty".to_string());
    }

    let now = crate::models::now_iso();
    let character = AiCharacter {
        id: uuid::Uuid::new_v4().to_string(),
        label: trimmed_label,
        role_type,
        role_instruction,
        default_provider,
        default_model,
        icon,
        avatar_path: None,
        personality: None,
        expertise: vec![],
        memory_config: None,
        created_at: now.clone(),
        updated_at: now,
    };

    let mut all = load_all()?;
    all.user.ai_characters.push(character.clone());
    all.user.updated_at = crate::models::now_iso();
    save_all(&all)?;
    Ok(character)
}

#[tauri::command]
pub fn update_character(
    id: String,
    label: Option<String>,
    role_type: Option<String>,
    role_instruction: Option<Option<String>>,
    default_provider: Option<String>,
    default_model: Option<Option<String>>,
    icon: Option<Option<String>>,
) -> Result<AiCharacter, String> {
    log::debug!("[characters] update_character: id={}", id);
    let mut all = load_all()?;
    let character = all
        .user
        .ai_characters
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("Character not found: {}", id))?;

    if let Some(v) = label {
        let trimmed = v.trim().to_string();
        if trimmed.is_empty() {
            return Err("Character label cannot be empty".to_string());
        }
        character.label = trimmed;
    }
    if let Some(v) = role_type {
        character.role_type = v;
    }
    if let Some(v) = role_instruction {
        character.role_instruction = v;
    }
    if let Some(v) = default_provider {
        character.default_provider = v;
    }
    if let Some(v) = default_model {
        character.default_model = v;
    }
    if let Some(v) = icon {
        character.icon = v;
    }
    character.updated_at = crate::models::now_iso();

    let updated = character.clone();
    all.user.updated_at = crate::models::now_iso();
    save_all(&all)?;
    Ok(updated)
}

#[tauri::command]
pub fn delete_character(id: String) -> Result<(), String> {
    log::debug!("[characters] delete_character: id={}", id);
    let mut all = load_all()?;
    let len_before = all.user.ai_characters.len();
    all.user.ai_characters.retain(|c| c.id != id);
    if all.user.ai_characters.len() == len_before {
        return Err(format!("Character not found: {}", id));
    }
    all.user.updated_at = crate::models::now_iso();
    save_all(&all)
}

fn load_all() -> Result<AllSettings, String> {
    // Use the same load path as get_user_settings / update_user_settings
    Ok(storage::settings::load())
}

fn save_all(all: &AllSettings) -> Result<(), String> {
    storage::settings::save(all)
}
