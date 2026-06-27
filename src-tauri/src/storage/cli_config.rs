use crate::storage::teams::claude_home_dir;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;

/// Serializes the load→mutate→write cycle on ~/.claude/settings.json so concurrent
/// callers (dual frontends, debounced saves) don't clobber each other's patches.
static CLI_CONFIG_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// Top-level keys in `~/.claude/settings.json` that contain secrets.
/// Must stay in sync with `session.rs:AUTH_KEYS` and `provider_claude_config.rs`.
pub const SENSITIVE_KEYS: &[&str] = &["apiKey", "primaryApiKey"];

/// Keys inside the `env` sub-object of `~/.claude/settings.json` that carry
/// credentials/endpoints. These must be stripped from the native base before a
/// provider session overlays its own auth — otherwise the user's native
/// `ANTHROPIC_API_KEY` would survive alongside the injected `ANTHROPIC_AUTH_TOKEN`,
/// handing a third-party endpoint two sets of credentials (H-sec-6).
pub const SENSITIVE_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "CLAUDE_CODE_API_KEY",
];

/// Path to the user-level CLI settings file: ~/.claude/settings.json
fn cli_config_path() -> PathBuf {
    claude_home_dir().join("settings.json")
}

/// Load user-level CLI config (~/.claude/settings.json).
/// Returns `{}` if the file doesn't exist or is invalid.
pub fn load_cli_config() -> Value {
    let path = cli_config_path();
    match std::fs::read_to_string(&path) {
        Ok(s) => match serde_json::from_str::<Value>(&s) {
            Ok(v) if v.is_object() => {
                log::debug!("[cli_config] loaded {} keys", v.as_object().unwrap().len());
                v
            }
            Ok(_) => {
                log::warn!("[cli_config] not an object, returning {{}}");
                Value::Object(serde_json::Map::new())
            }
            Err(e) => {
                log::warn!("[cli_config] parse error: {}", e);
                Value::Object(serde_json::Map::new())
            }
        },
        Err(e) => {
            log::debug!("[cli_config] read error (expected if first run): {}", e);
            Value::Object(serde_json::Map::new())
        }
    }
}

/// Load project-level CLI config ({cwd}/.claude/settings.json).
/// Read-only — used for override indicator display.
pub fn load_project_cli_config(cwd: &str) -> Value {
    let path = PathBuf::from(cwd).join(".claude").join("settings.json");
    match std::fs::read_to_string(&path) {
        Ok(s) => match serde_json::from_str::<Value>(&s) {
            Ok(v) if v.is_object() => {
                log::debug!(
                    "[cli_config] project config loaded {} keys from {}",
                    v.as_object().unwrap().len(),
                    path.display()
                );
                v
            }
            Ok(_) => Value::Object(serde_json::Map::new()),
            Err(e) => {
                log::warn!("[cli_config] project parse error {}: {}", path.display(), e);
                Value::Object(serde_json::Map::new())
            }
        },
        Err(e) => {
            log::debug!("[cli_config] project read: {}: {}", path.display(), e);
            Value::Object(serde_json::Map::new())
        }
    }
}

/// Apply a shallow merge patch to the user-level CLI config.
/// - Only top-level keys in `patch` are written.
/// - `null` values delete the key (restore CLI default).
/// - All other existing keys are preserved (hooks, env, enabledPlugins, etc.).
/// - File permissions are set to 0o600 on unix.
pub fn update_cli_config(patch: Value) -> Result<Value, String> {
    let patch_obj = patch
        .as_object()
        .ok_or_else(|| "patch must be a JSON object".to_string())?;

    // Hold the lock across load→mutate→write so a concurrent update can't lose this patch.
    let _guard = CLI_CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let mut config = load_cli_config();
    let map = config
        .as_object_mut()
        .expect("load_cli_config always returns object");

    for (key, value) in patch_obj {
        if value.is_null() {
            log::debug!("[cli_config] deleting key: {}", key);
            map.remove(key);
        } else {
            if SENSITIVE_KEYS.contains(&key.as_str()) {
                log::debug!("[cli_config] setting key: {} = ***", key);
            } else {
                log::debug!("[cli_config] setting key: {}", key);
            }
            map.insert(key.clone(), value.clone());
        }
    }

    // Write with pretty formatting
    let path = cli_config_path();

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let content =
        serde_json::to_string_pretty(&config).map_err(|e| format!("Failed to serialize: {}", e))?;

    // Atomic write: tmp + (perms BEFORE rename) + rename. settings.json is shared with the
    // Anthropic CLI; a truncate-write crash would corrupt it, and setting 0o600 only after
    // the final write left a world-readable window over apiKey/primaryApiKey.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &content).map_err(|e| format!("Failed to write temp: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)) {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("Failed to set perms: {}", e));
        }
    }
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("Failed to rename: {}", e)
    })?;

    log::debug!(
        "[cli_config] updated {} keys total",
        config.as_object().unwrap().len()
    );
    Ok(config)
}
