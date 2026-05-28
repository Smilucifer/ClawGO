use crate::models::{AiCharacter, AllSettings, MemoryNode};
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
    avatar_path: Option<Option<String>>,
    personality: Option<Option<String>>,
    expertise: Option<Vec<String>>,
    memory_config: Option<Option<crate::models::MemoryConfig>>,
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
    if let Some(v) = avatar_path {
        character.avatar_path = v;
    }
    if let Some(v) = personality {
        character.personality = v;
    }
    if let Some(v) = expertise {
        character.expertise = v;
    }
    if let Some(v) = memory_config {
        character.memory_config = v;
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

// --- Memory CRUD (user-centric, via SQLite memory_store) ---

#[tauri::command]
pub async fn list_character_memories(
    _character_id: String,
) -> Result<Vec<MemoryNode>, String> {
    storage::memory_store::list_memories(None, None, 500, 0)
}

#[tauri::command]
pub async fn get_character_memory(
    _character_id: String,
    memory_id: String,
) -> Result<Option<MemoryNode>, String> {
    storage::memory_store::get_memory(&memory_id)
}

const ALLOWED_MEMORY_TYPES: &[&str] = &["fact", "preference", "skill", "feedback", "experience", "relationship"];
const ALLOWED_STATUSES: &[&str] = &["pending", "approved", "rejected", "archived"];

fn validate_memory_type(t: &str) -> Result<(), String> {
    if ALLOWED_MEMORY_TYPES.contains(&t) {
        Ok(())
    } else {
        Err(format!(
            "Invalid memory_type '{}'. Allowed: {}",
            t,
            ALLOWED_MEMORY_TYPES.join(", ")
        ))
    }
}

fn validate_memory_status(s: &str) -> Result<(), String> {
    if ALLOWED_STATUSES.contains(&s) {
        Ok(())
    } else {
        Err(format!(
            "Invalid status '{}'. Allowed: {}",
            s,
            ALLOWED_STATUSES.join(", ")
        ))
    }
}

#[tauri::command]
pub async fn create_character_memory(
    _character_id: String,
    content: String,
    memory_type: String,
    confidence: f64,
    tags: Vec<String>,
) -> Result<MemoryNode, String> {
    validate_memory_type(&memory_type)?;
    let now = chrono::Utc::now().to_rfc3339();
    let node = MemoryNode {
        id: uuid::Uuid::new_v4().to_string(),
        character_id: String::new(),
        content,
        memory_type,
        confidence,
        source: crate::models::MemorySource {
            kind: "manual".to_string(),
            run_id: None,
            group_chat_id: None,
        },
        tags,
        created_at: now.clone(),
        updated_at: now,
        status: "approved".to_string(),
        scope: "global".to_string(),
        project_id: None,
    };
    storage::memory_store::insert_memory(&node)?;
    Ok(node)
}

#[tauri::command]
pub async fn update_character_memory(
    _character_id: String,
    memory_id: String,
    content: Option<String>,
    memory_type: Option<String>,
    confidence: Option<f64>,
    tags: Option<Vec<String>>,
) -> Result<MemoryNode, String> {
    let mut node = storage::memory_store::get_memory(&memory_id)?
        .ok_or_else(|| format!("Memory not found: {}", memory_id))?;

    if let Some(c) = content {
        node.content = c;
    }
    if let Some(t) = &memory_type {
        validate_memory_type(t)?;
        node.memory_type = t.clone();
    }
    if let Some(c) = confidence {
        node.confidence = c;
    }
    if let Some(t) = tags {
        node.tags = t;
    }
    node.updated_at = chrono::Utc::now().to_rfc3339();

    storage::memory_store::update_memory(&node)?;
    Ok(node)
}

#[tauri::command]
pub async fn delete_character_memory(
    _character_id: String,
    memory_id: String,
) -> Result<(), String> {
    storage::memory_store::delete_memory(&memory_id)
}

#[tauri::command]
pub async fn search_character_memories(
    _character_id: String,
    query: String,
    top_k: Option<usize>,
    _threshold: Option<f64>,
    _graph_hops: Option<usize>,
) -> Result<Vec<MemoryNode>, String> {
    storage::memory_store::search_fts(&query, top_k.unwrap_or(5), "approved")
}

#[tauri::command]
pub async fn list_pending_memories(
    _character_id: String,
) -> Result<Vec<MemoryNode>, String> {
    storage::memory_store::list_memories(Some("pending"), None, 100, 0)
}

#[tauri::command]
pub async fn approve_memory(
    _character_id: String,
    memory_id: String,
) -> Result<MemoryNode, String> {
    let mut node = storage::memory_store::get_memory(&memory_id)?
        .ok_or_else(|| format!("memory {} not found", memory_id))?;
    node.status = "approved".to_string();
    storage::memory_store::update_memory(&node)?;
    Ok(node)
}

#[tauri::command]
pub async fn reject_memory(
    _character_id: String,
    memory_id: String,
) -> Result<MemoryNode, String> {
    let mut node = storage::memory_store::get_memory(&memory_id)?
        .ok_or_else(|| format!("memory {} not found", memory_id))?;
    node.status = "rejected".to_string();
    storage::memory_store::update_memory(&node)?;
    Ok(node)
}

fn load_all() -> Result<AllSettings, String> {
    Ok(storage::settings::load())
}

fn save_all(all: &AllSettings) -> Result<(), String> {
    storage::settings::save(all)
}
