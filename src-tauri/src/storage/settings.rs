use crate::models::{
    AgentSettings, AllSettings, BalanceHelperSettings, ConnectionProfile, UserSettings,
    WindowsMsvcEnvMode,
};
use std::fs;
use std::path::PathBuf;

fn settings_path() -> PathBuf {
    super::data_dir().join("settings.json")
}

pub fn load() -> AllSettings {
    let path = settings_path();
    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(mut settings) => {
                    log::debug!("[storage/settings] loaded settings from {}", path.display());
                    // Run one-time migrations on platform credentials
                    if migrate_platform_credentials(&mut settings) {
                        log::info!("[storage/settings] migrated platform credentials, saving");
                        let _ = save(&settings);
                    }
                    return settings;
                }
                Err(e) => {
                    log::warn!("[storage/settings] failed to parse settings: {}", e);
                }
            },
            Err(e) => {
                log::warn!("[storage/settings] failed to read settings: {}", e);
            }
        }
    }
    log::debug!("[storage/settings] using default settings");
    let defaults = AllSettings::default();
    let _ = save(&defaults);
    defaults
}

/// Known provider defaults for migration.
/// Must match the values in platform-presets.ts.
struct ProviderDefaults {
    base_url: Option<&'static str>,
    models: Option<Vec<String>>,
    extra_env: Option<std::collections::HashMap<String, String>>,
    key_optional: bool,
    auth_env_var: Option<&'static str>,
}

/// Known provider defaults exposed for auth resolution fallback.
pub(crate) struct ProviderInfo {
    pub base_url: Option<String>,
    pub models: Option<Vec<String>>,
    pub extra_env: Option<std::collections::HashMap<String, String>>,
    pub key_optional: bool,
    pub auth_env_var: Option<String>,
}

pub(crate) fn is_key_optional_platform(pid: &str) -> bool {
    known_provider_defaults(pid).is_some_and(|d| d.key_optional)
}

pub(crate) fn get_provider_info(pid: &str) -> Option<ProviderInfo> {
    known_provider_defaults(pid).map(|d| ProviderInfo {
        base_url: d.base_url.map(|s| s.to_string()),
        models: d.models,
        extra_env: d.extra_env,
        key_optional: d.key_optional,
        auth_env_var: d.auth_env_var.map(|s| s.to_string()),
    })
}

fn known_provider_defaults(pid: &str) -> Option<ProviderDefaults> {
    use std::collections::HashMap;
    match pid {
        "deepseek" => Some(ProviderDefaults {
            base_url: Some("https://api.deepseek.com/anthropic"),
            models: Some(vec![
                "deepseek-v4-pro".to_string(),
                "deepseek-v4-flash".to_string(),
            ]),
            extra_env: Some(HashMap::from([(
                "API_TIMEOUT_MS".to_string(),
                "600000".to_string(),
            )])),
            key_optional: false,
            auth_env_var: None,
        }),
        "kimi" => Some(ProviderDefaults {
            base_url: Some("https://api.moonshot.cn/anthropic"),
            models: Some(vec!["kimi-k2.5".to_string(), "kimi-k2".to_string()]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "kimi-coding" => Some(ProviderDefaults {
            base_url: Some("https://api.kimi.com/coding/"),
            models: None,
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "zhipu" => Some(ProviderDefaults {
            base_url: Some("https://open.bigmodel.cn/api/anthropic"),
            models: Some(vec![
                "glm-5".to_string(),
                "glm-4.7".to_string(),
                "glm-4.5-air".to_string(),
                "glm-4.5-flash".to_string(),
            ]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "zhipu-intl" => Some(ProviderDefaults {
            base_url: Some("https://api.z.ai/api/anthropic"),
            models: Some(vec![
                "glm-5".to_string(),
                "glm-4.7".to_string(),
                "glm-4.5-air".to_string(),
                "glm-4.5-flash".to_string(),
            ]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "bailian" => Some(ProviderDefaults {
            base_url: Some("https://coding.dashscope.aliyuncs.com/apps/anthropic"),
            models: Some(vec![
                "qwen3-max".to_string(),
                "qwen3.5-plus".to_string(),
                "qwen-plus".to_string(),
                "qwen-flash".to_string(),
            ]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "doubao" => Some(ProviderDefaults {
            base_url: Some("https://ark.cn-beijing.volces.com/api/coding"),
            models: Some(vec!["doubao-seed-code-preview-latest".to_string()]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "minimax" => Some(ProviderDefaults {
            base_url: Some("https://api.minimax.io/anthropic"),
            models: Some(vec![
                "MiniMax-M2.5".to_string(),
                "MiniMax-M2.5-highspeed".to_string(),
            ]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "minimax-cn" => Some(ProviderDefaults {
            base_url: Some("https://api.minimaxi.com/anthropic"),
            models: Some(vec![
                "MiniMax-M2.5".to_string(),
                "MiniMax-M2.5-highspeed".to_string(),
            ]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "mimo" => Some(ProviderDefaults {
            base_url: Some("https://api.xiaomimimo.com/anthropic"),
            models: Some(vec!["mimo-v2-flash".to_string()]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "mimo-pro" => Some(ProviderDefaults {
            base_url: Some("https://token-plan-cn.xiaomimimo.com/anthropic"),
            models: Some(vec!["mimo-v2.5-pro".to_string()]),
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "packy-cx2cc" => Some(ProviderDefaults {
            base_url: Some("https://www.packyapi.com"),
            models: None,
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "openrouter" => Some(ProviderDefaults {
            base_url: Some("https://openrouter.ai/api"),
            models: None,
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "aihubmix" => Some(ProviderDefaults {
            base_url: Some("https://aihubmix.com"),
            models: None,
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "zenmux" => Some(ProviderDefaults {
            base_url: Some("https://zenmux.ai/api/anthropic"),
            models: None,
            extra_env: Some(HashMap::from([(
                "API_TIMEOUT_MS".to_string(),
                "30000000".to_string(),
            )])),
            key_optional: false,
            auth_env_var: None,
        }),
        "vercel" => Some(ProviderDefaults {
            base_url: Some("https://ai-gateway.vercel.sh"),
            models: None,
            extra_env: None,
            key_optional: false,
            auth_env_var: None,
        }),
        "ccswitch" => Some(ProviderDefaults {
            base_url: Some("http://127.0.0.1:15721"),
            models: None,
            extra_env: None,
            key_optional: true,
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN"),
        }),
        "ccr" => Some(ProviderDefaults {
            base_url: Some("http://127.0.0.1:3456"),
            models: Some(vec!["claude-sonnet-4-6".to_string()]),
            extra_env: None,
            key_optional: true,
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN"),
        }),
        "ollama" => Some(ProviderDefaults {
            base_url: Some("http://localhost:11434"),
            models: None,
            extra_env: None,
            key_optional: true,
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN"),
        }),
        _ => None,
    }
}

/// Migrate stale platform credential data. Returns true if any changes were made.
///
/// Fixes:
/// - Incorrect auth_env_var for providers that need ANTHROPIC_API_KEY (x-api-key header)
/// - Old "minimax" credentials using minimaxi.com → rename to "minimax-cn" preset
/// - Missing models/extra_env on existing credentials (needed for ANTHROPIC_MODEL injection)
fn migrate_platform_credentials(settings: &mut AllSettings) -> bool {
    let auth_fixes: &[(&str, &str)] = &[
        ("deepseek", "ANTHROPIC_AUTH_TOKEN"),
        ("zhipu", "ANTHROPIC_AUTH_TOKEN"),
        ("zhipu-intl", "ANTHROPIC_AUTH_TOKEN"),
        ("doubao", "ANTHROPIC_AUTH_TOKEN"),
        ("minimax", "ANTHROPIC_AUTH_TOKEN"),
        ("minimax-cn", "ANTHROPIC_AUTH_TOKEN"),
        ("mimo", "ANTHROPIC_AUTH_TOKEN"),
        ("mimo-pro", "ANTHROPIC_AUTH_TOKEN"),
        ("bailian", "ANTHROPIC_AUTH_TOKEN"),
        ("kimi-coding", "ANTHROPIC_AUTH_TOKEN"),
        ("kimi", "ANTHROPIC_AUTH_TOKEN"),
        ("packy-cx2cc", "ANTHROPIC_AUTH_TOKEN"),
        ("aihubmix", "ANTHROPIC_AUTH_TOKEN"),
    ];
    let mut changed = false;

    for cred in &mut settings.user.platform_credentials {
        // Fix auth_env_var
        for &(pid, correct) in auth_fixes {
            if cred.platform_id == pid && cred.auth_env_var.as_deref() != Some(correct) {
                log::info!(
                    "[storage/settings] migrating auth_env_var for '{}': {:?} → {}",
                    pid,
                    cred.auth_env_var,
                    correct
                );
                cred.auth_env_var = Some(correct.to_string());
                changed = true;
            }
        }

        // Migrate old "minimax" credentials that used minimaxi.com → "minimax-cn"
        if cred.platform_id == "minimax" {
            if let Some(ref url) = cred.base_url {
                if url.contains("api.minimaxi.com") {
                    log::info!(
                        "[storage/settings] migrating minimax credential with minimaxi.com to minimax-cn"
                    );
                    cred.platform_id = "minimax-cn".to_string();
                    changed = true;
                }
            }
        }

        if cred.platform_id == "deepseek"
            && cred
                .models
                .as_ref()
                .is_some_and(|models| models.iter().any(|model| model == "deepseek-chat"))
        {
            log::info!(
                "[storage/settings] migrating deepseek model from deepseek-chat to deepseek-v4-pro"
            );
            cred.models = Some(vec![
                "deepseek-v4-pro".to_string(),
                "deepseek-v4-flash".to_string(),
            ]);
            changed = true;
        }

        // Populate base_url, models, and extra_env from known provider defaults if missing.
        // base_url is CRITICAL — without it, ANTHROPIC_BASE_URL is not set and
        // requests go to Anthropic's default endpoint instead of the third-party provider.
        if let Some(defaults) = known_provider_defaults(&cred.platform_id) {
            if cred.base_url.as_ref().is_none_or(|s| s.is_empty()) {
                if let Some(url) = defaults.base_url {
                    log::info!(
                        "[storage/settings] migrating base_url for '{}': {}",
                        cred.platform_id,
                        url
                    );
                    cred.base_url = Some(url.to_string());
                    changed = true;
                }
            }
            if cred.models.is_none() {
                if let Some(models) = defaults.models {
                    log::info!(
                        "[storage/settings] migrating models for '{}': {:?}",
                        cred.platform_id,
                        models
                    );
                    cred.models = Some(models);
                    changed = true;
                }
            }
            if cred.extra_env.is_none() {
                if let Some(extra) = defaults.extra_env {
                    log::info!(
                        "[storage/settings] migrating extra_env for '{}': {:?}",
                        cred.platform_id,
                        extra
                    );
                    cred.extra_env = Some(extra);
                    changed = true;
                }
            }
        }
    }

    // If active_platform_id was "minimax" but was migrated to "minimax-cn", update it
    if settings.user.active_platform_id.as_deref() == Some("minimax") {
        let has_minimax_cn = settings
            .user
            .platform_credentials
            .iter()
            .any(|c| c.platform_id == "minimax-cn");
        let has_minimax = settings
            .user
            .platform_credentials
            .iter()
            .any(|c| c.platform_id == "minimax");
        if has_minimax_cn && !has_minimax {
            log::info!(
                "[storage/settings] migrating active_platform_id from minimax to minimax-cn"
            );
            settings.user.active_platform_id = Some("minimax-cn".to_string());
            changed = true;
        }
    }

    // Also fix the global auth_env_var if it was set by one of these providers
    // (only if active_platform_id matches a provider that needs fixing)
    if let Some(ref pid) = settings.user.active_platform_id {
        for &(fix_pid, correct) in auth_fixes {
            if pid == fix_pid && settings.user.auth_env_var.as_deref() != Some(correct) {
                log::info!(
                    "[storage/settings] migrating global auth_env_var for active platform '{}': {:?} → {}",
                    pid,
                    settings.user.auth_env_var,
                    correct
                );
                settings.user.auth_env_var = Some(correct.to_string());
                changed = true;
            }
        }
    }

    changed
}

pub fn save(settings: &AllSettings) -> Result<(), String> {
    log::debug!("[storage/settings] saving settings");
    let path = settings_path();
    super::ensure_dir(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, &json).map_err(|e| e.to_string())?;

    // Restrict file permissions — settings may contain API keys
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(&path, fs::Permissions::from_mode(0o600)) {
            log::warn!(
                "[storage/settings] failed to set permissions on settings.json: {}",
                e
            );
        }
    }

    Ok(())
}

pub fn get_user_settings() -> UserSettings {
    load().user
}

/// Save web server config fields. Called by restart_with_config on success.
pub fn save_web_server_config(
    enabled: bool,
    port: u16,
    bind: &str,
    allowed_origins: &Option<Vec<String>>,
    tunnel_url: &Option<String>,
) -> Result<(), String> {
    let mut all = load();
    all.user.web_server_enabled = Some(enabled);
    all.user.web_server_port = Some(port);
    all.user.web_server_bind = Some(bind.to_string());
    all.user.web_server_allowed_origins = allowed_origins.clone();
    all.user.web_server_tunnel_url = tunnel_url.clone();
    all.user.updated_at = crate::models::now_iso();
    save(&all)?;
    log::debug!(
        "[storage/settings] web_server config saved: enabled={}, port={}, bind={}, tunnel={:?}",
        enabled,
        port,
        bind,
        tunnel_url,
    );
    Ok(())
}

/// Set only web_server_enabled, preserving all other web server fields.
pub fn set_web_server_enabled(enabled: bool) -> Result<(), String> {
    let mut all = load();
    all.user.web_server_enabled = Some(enabled);
    all.user.updated_at = crate::models::now_iso();
    save(&all)?;
    log::debug!("[storage/settings] web_server_enabled set to {}", enabled);
    Ok(())
}

/// Partial disable: only set enabled=false, never touch other web server fields.
/// Used by the disable path to ensure disable always succeeds regardless of form state.
pub fn save_web_server_partial_disable() -> Result<(), String> {
    let mut all = load();
    all.user.web_server_enabled = Some(false);
    all.user.updated_at = crate::models::now_iso();
    save(&all)?;
    log::debug!("[storage/settings] web_server partial disable saved");
    Ok(())
}

fn validate_ui_zoom(v: &serde_json::Value) -> Result<Option<f64>, String> {
    if v.is_null() {
        return Ok(None);
    }
    let f = v
        .as_f64()
        .ok_or_else(|| "ui_zoom must be a number".to_string())?;
    if !(0.75..=1.5).contains(&f) {
        return Err(format!("ui_zoom must be between 0.75 and 1.5, got {}", f));
    }
    Ok(Some(f))
}

fn validate_balance_helper(
    settings: BalanceHelperSettings,
) -> Result<BalanceHelperSettings, String> {
    if !(60..=180).contains(&settings.auto_refresh_secs) {
        return Err(format!(
            "balance_helper.auto_refresh_secs must be between 60 and 180, got {}",
            settings.auto_refresh_secs
        ));
    }
    Ok(settings)
}

pub fn update_user_settings(patch: serde_json::Value) -> Result<UserSettings, String> {
    let mut all = load();
    if let Some(agent) = patch.get("default_agent").and_then(|v| v.as_str()) {
        all.user.default_agent = agent.to_string();
    }
    if let Some(model) = patch.get("default_model") {
        all.user.default_model = model.as_str().map(|s| s.to_string());
    }
    if let Some(tools) = patch.get("allowed_tools").and_then(|v| v.as_array()) {
        all.user.allowed_tools = tools
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }
    if let Some(wd) = patch.get("working_directory") {
        all.user.working_directory = wd.as_str().map(|s| s.to_string());
    }
    if let Some(mode) = patch.get("provider_mode").and_then(|v| v.as_str()) {
        all.user.provider_mode = mode.to_string();
    }
    if let Some(mode) = patch.get("auth_mode").and_then(|v| v.as_str()) {
        all.user.auth_mode = mode.to_string();
    }
    if let Some(key) = patch.get("anthropic_api_key") {
        all.user.anthropic_api_key = key
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
    }
    if let Some(url) = patch.get("anthropic_base_url") {
        all.user.anthropic_base_url = url
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
    }
    if let Some(v) = patch.get("auth_env_var") {
        all.user.auth_env_var = v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());
    }
    if let Some(mode) = patch.get("permission_mode").and_then(|v| v.as_str()) {
        all.user.permission_mode = mode.to_string();
    }
    if let Some(v) = patch.get("max_budget_usd") {
        all.user.max_budget_usd = if v.is_null() { None } else { v.as_f64() };
    }
    if let Some(v) = patch.get("fallback_model") {
        all.user.fallback_model = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("keybinding_overrides") {
        if v.is_null() {
            all.user.keybinding_overrides = vec![];
        } else {
            all.user.keybinding_overrides = serde_json::from_value(v.clone())
                .map_err(|e| format!("Invalid keybinding_overrides: {}", e))?;
        }
    }
    if let Some(v) = patch.get("remote_hosts") {
        if v.is_null() {
            all.user.remote_hosts = vec![];
        } else {
            all.user.remote_hosts = serde_json::from_value(v.clone())
                .map_err(|e| format!("Invalid remote_hosts: {}", e))?;
        }
    }
    if let Some(v) = patch.get("platform_credentials") {
        if v.is_null() {
            all.user.platform_credentials = vec![];
        } else {
            all.user.platform_credentials = serde_json::from_value(v.clone())
                .map_err(|e| format!("Invalid platform_credentials: {}", e))?;
        }
    }
    if let Some(v) = patch.get("cc_agent_profiles") {
        if v.is_null() {
            all.user.cc_agent_profiles = vec![];
        } else {
            all.user.cc_agent_profiles = serde_json::from_value(v.clone())
                .map_err(|e| format!("Invalid cc_agent_profiles: {}", e))?;
        }
    }
    if let Some(v) = patch.get("connection_profiles") {
        if v.is_null() {
            all.user.connection_profiles = vec![];
        } else {
            all.user.connection_profiles = serde_json::from_value(v.clone())
                .map_err(|e| format!("Invalid connection_profiles: {}", e))?;
        }
    }
    if let Some(v) = patch.get("balance_helper") {
        if v.is_null() {
            all.user.balance_helper = BalanceHelperSettings::default();
        } else {
            let mut next = all.user.balance_helper.clone();
            if let Some(session) = v.get("packy_session") {
                next.packy_session = session
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
            }
            if let Some(itoken) = v.get("packy_tdc_itoken") {
                next.packy_tdc_itoken = itoken
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
            }
            if let Some(user_id) = v.get("packy_user_id") {
                next.packy_user_id = user_id
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
            }
            if let Some(token) = v.get("mimo_service_token") {
                next.mimo_service_token = token
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
            }
            if let Some(uid) = v.get("mimo_user_id") {
                next.mimo_user_id = uid
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
            }
            if let Some(slh) = v.get("mimo_slh") {
                next.mimo_slh = slh
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
            }
            if let Some(ph) = v.get("mimo_ph") {
                next.mimo_ph = ph
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
            }
            if let Some(secs) = v.get("auto_refresh_secs") {
                next.auto_refresh_secs = secs.as_u64().ok_or_else(|| {
                    "balance_helper.auto_refresh_secs must be a number".to_string()
                })?;
            }
            if let Some(cache) = v.get("cache") {
                next.cache = if cache.is_null() {
                    Default::default()
                } else {
                    serde_json::from_value(cache.clone())
                        .map_err(|e| format!("Invalid balance_helper.cache: {}", e))?
                };
            }
            all.user.balance_helper = validate_balance_helper(next)?;
        }
    }
    if let Some(v) = patch.get("active_platform_id") {
        all.user.active_platform_id = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("ui_zoom") {
        all.user.ui_zoom = validate_ui_zoom(v)?;
        log::debug!("[storage/settings] ui_zoom patched: {:?}", all.user.ui_zoom);
    }
    if let Some(v) = patch.get("onboarding_completed") {
        all.user.onboarding_completed = v.as_bool().unwrap_or(false);
    }
    if let Some(v) = patch.get("windows_msvc_env_mode") {
        all.user.windows_msvc_env_mode = serde_json::from_value::<WindowsMsvcEnvMode>(v.clone())
            .map_err(|e| format!("Invalid windows_msvc_env_mode: {}", e))?;
    }
    if let Some(v) = patch.get("github_proxy_enabled") {
        all.user.github_proxy_enabled = v.as_bool().unwrap_or(false);
    }
    if let Some(v) = patch.get("github_proxy_port") {
        all.user.github_proxy_port = v.as_u64().filter(|&p| p >= 1 && p <= 65535).unwrap_or(7890) as u16;
    }
    all.user.updated_at = crate::models::now_iso();
    save(&all)?;
    Ok(all.user)
}

pub fn get_agent_settings(agent: &str) -> AgentSettings {
    log::debug!("[storage/settings] get_agent_settings: agent={}", agent);
    let all = load();
    all.agents
        .get(agent)
        .cloned()
        .unwrap_or_else(|| AgentSettings::default_for(agent))
}

pub fn find_connection_profile(
    user: &UserSettings,
    agent: &str,
    profile_id: Option<&str>,
) -> Result<Option<ConnectionProfile>, String> {
    let Some(id) = profile_id.filter(|id| !id.trim().is_empty()) else {
        return Ok(None);
    };
    let profile = user
        .connection_profiles
        .iter()
        .find(|profile| profile.id == id)
        .ok_or_else(|| format!("Connection profile '{}' not found", id))?;
    if !profile.enabled {
        return Err(format!("Connection profile '{}' is disabled", id));
    }
    if profile.agent != agent {
        return Err(format!(
            "Connection profile '{}' is for agent '{}', not '{}'",
            id, profile.agent, agent
        ));
    }
    Ok(Some(profile.clone()))
}

/// Apply a JSON patch to AgentSettings (pure function, no I/O).
fn apply_agent_patch(settings: &mut AgentSettings, patch: &serde_json::Value) {
    if let Some(model) = patch.get("model") {
        settings.model = model.as_str().map(|s| s.to_string());
    }
    if let Some(tools) = patch.get("allowed_tools").and_then(|v| v.as_array()) {
        settings.allowed_tools = tools
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }
    if let Some(wd) = patch.get("working_directory") {
        settings.working_directory = wd.as_str().map(|s| s.to_string());
    }
    if let Some(v) = patch.get("plan_mode") {
        settings.plan_mode = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("disallowed_tools") {
        settings.disallowed_tools = if v.is_null() {
            None
        } else {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
        };
    }
    if let Some(v) = patch.get("append_system_prompt") {
        settings.append_system_prompt = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("max_budget_usd") {
        settings.max_budget_usd = if v.is_null() { None } else { v.as_f64() };
    }
    if let Some(v) = patch.get("fallback_model") {
        settings.fallback_model = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("system_prompt") {
        settings.system_prompt = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("tool_set") {
        settings.tool_set = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("add_dirs") {
        settings.add_dirs = if v.is_null() {
            None
        } else {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
        };
    }
    if let Some(v) = patch.get("json_schema") {
        settings.json_schema = if v.is_null() { None } else { Some(v.clone()) };
    }
    if let Some(v) = patch.get("include_partial_messages") {
        settings.include_partial_messages = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("cli_debug") {
        settings.cli_debug = if v.is_null() {
            None
        } else {
            // Allow empty string (means "--debug" with no filter)
            v.as_str().map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("no_session_persistence") {
        settings.no_session_persistence = if v.is_null() { None } else { v.as_bool() };
    }
    if let Some(v) = patch.get("effort") {
        settings.effort = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("command_path") {
        settings.command_path = if v.is_null() {
            None
        } else {
            v.as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
        };
    }
    if let Some(v) = patch.get("extra_args") {
        settings.extra_args = if v.is_null() {
            None
        } else {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
        };
    }
    if let Some(v) = patch.get("yolo_mode") {
        settings.yolo_mode = if v.is_null() { None } else { v.as_bool() };
    }
}

pub fn update_agent_settings(
    agent: &str,
    patch: serde_json::Value,
) -> Result<AgentSettings, String> {
    let mut all = load();
    let mut settings = all
        .agents
        .get(agent)
        .cloned()
        .unwrap_or_else(|| AgentSettings::default_for(agent));
    apply_agent_patch(&mut settings, &patch);
    settings.updated_at = crate::models::now_iso();
    all.agents.insert(agent.to_string(), settings.clone());
    save(&all)?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CcAgentProfile;
    use crate::models::{AllSettings, PlatformCredential};

    fn make_settings_with_cred(cred: PlatformCredential) -> AllSettings {
        let mut s = AllSettings::default();
        s.user.platform_credentials.push(cred);
        s
    }

    fn settings_json_with_user_patch(patch: serde_json::Value) -> String {
        let mut value = serde_json::to_value(AllSettings::default()).unwrap();
        let user = value.get_mut("user").unwrap().as_object_mut().unwrap();
        if let Some(patch) = patch.as_object() {
            for (key, value) in patch {
                user.insert(key.clone(), value.clone());
            }
        }
        serde_json::to_string(&value).unwrap()
    }

    #[test]
    fn user_settings_defaults_windows_msvc_env_mode_to_auto() {
        let json = settings_json_with_user_patch(serde_json::json!({}));
        let settings: AllSettings = serde_json::from_str(&json).unwrap();

        assert_eq!(
            settings.user.windows_msvc_env_mode,
            WindowsMsvcEnvMode::Auto
        );
    }

    #[test]
    fn user_settings_defaults_balance_helper_to_empty_state() {
        let json = settings_json_with_user_patch(serde_json::json!({}));
        let settings: AllSettings = serde_json::from_str(&json).unwrap();

        assert_eq!(settings.user.balance_helper.packy_session, None);
        assert_eq!(settings.user.balance_helper.packy_tdc_itoken, None);
        assert_eq!(settings.user.balance_helper.packy_user_id, None);
        assert_eq!(settings.user.balance_helper.auto_refresh_secs, 120);
        assert!(settings.user.balance_helper.cache.is_empty());
    }

    #[test]
    fn update_user_settings_persists_balance_helper_without_touching_platform_credentials() {
        let _guard = crate::storage::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("OPENCOVIBE_DATA_DIR");
        std::env::set_var("OPENCOVIBE_DATA_DIR", tmp.path());

        let initial = update_user_settings(serde_json::json!({
            "platform_credentials": [
                {
                    "platform_id": "deepseek",
                    "api_key": "sk-deepseek"
                }
            ]
        }))
        .unwrap();
        assert_eq!(initial.platform_credentials.len(), 1);

        let updated = update_user_settings(serde_json::json!({
            "balance_helper": {
                "packy_session": "session-secret",
                "packy_tdc_itoken": "595383047:1776349439",
                "packy_user_id": "98264",
                "auto_refresh_secs": 180
            }
        }))
        .unwrap();

        match previous {
            Some(value) => std::env::set_var("OPENCOVIBE_DATA_DIR", value),
            None => std::env::remove_var("OPENCOVIBE_DATA_DIR"),
        }

        assert_eq!(updated.platform_credentials.len(), 1);
        assert_eq!(
            updated.platform_credentials[0].platform_id.as_str(),
            "deepseek"
        );
        assert_eq!(updated.balance_helper.packy_session.as_deref(), Some("session-secret"));
        assert_eq!(
            updated.balance_helper.packy_tdc_itoken.as_deref(),
            Some("595383047:1776349439")
        );
        assert_eq!(updated.balance_helper.packy_user_id.as_deref(), Some("98264"));
        assert_eq!(updated.balance_helper.auto_refresh_secs, 180);
    }

    #[test]
    fn update_user_settings_rejects_balance_helper_refresh_outside_phase7_bounds() {
        let _guard = crate::storage::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("OPENCOVIBE_DATA_DIR");
        std::env::set_var("OPENCOVIBE_DATA_DIR", tmp.path());

        let result = update_user_settings(serde_json::json!({
            "balance_helper": {
                "auto_refresh_secs": 30
            }
        }));

        match previous {
            Some(value) => std::env::set_var("OPENCOVIBE_DATA_DIR", value),
            None => std::env::remove_var("OPENCOVIBE_DATA_DIR"),
        }

        assert!(result
            .unwrap_err()
            .contains("balance_helper.auto_refresh_secs"));
    }

    #[test]
    fn user_settings_deserializes_cc_agent_profiles_from_json() {
        let json = settings_json_with_user_patch(serde_json::json!({
            "cc_agent_profiles": [
                {
                    "id": "deepseek-via-ccr",
                    "label": "DeepSeek via CCR",
                    "agent": "claude",
                    "platform_id": "ccr",
                    "model": "deepseek-v4-pro",
                    "prompt": "You are the DeepSeek seat.",
                    "role": "researcher"
                }
            ]
        }));

        let settings: AllSettings = serde_json::from_str(&json).unwrap();

        assert_eq!(settings.user.cc_agent_profiles.len(), 1);
        let profile = &settings.user.cc_agent_profiles[0];
        assert_eq!(profile.id, "deepseek-via-ccr");
        assert_eq!(profile.label, "DeepSeek via CCR");
        assert_eq!(profile.agent, "claude");
        assert_eq!(profile.platform_id.as_deref(), Some("ccr"));
        assert_eq!(profile.model.as_deref(), Some("deepseek-v4-pro"));
        assert!(profile.enabled);
    }

    #[test]
    fn update_user_settings_replaces_cc_agent_profiles() {
        let _guard = crate::storage::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("OPENCOVIBE_DATA_DIR");
        std::env::set_var("OPENCOVIBE_DATA_DIR", tmp.path());

        let updated = update_user_settings(serde_json::json!({
            "cc_agent_profiles": [
                {
                    "id": "codex-via-cc",
                    "label": "Codex via CC",
                    "agent": "codex",
                    "platform_id": "ccswitch",
                    "model": "gpt-5.5"
                }
            ]
        }))
        .unwrap();

        match previous {
            Some(value) => std::env::set_var("OPENCOVIBE_DATA_DIR", value),
            None => std::env::remove_var("OPENCOVIBE_DATA_DIR"),
        }

        assert_eq!(
            updated.cc_agent_profiles,
            vec![CcAgentProfile {
                id: "codex-via-cc".to_string(),
                label: "Codex via CC".to_string(),
                agent: "codex".to_string(),
                platform_id: Some("ccswitch".to_string()),
                model: Some("gpt-5.5".to_string()),
                prompt: None,
                cwd: None,
                role: None,
                enabled: true,
            }]
        );
    }

    #[test]
    fn user_settings_deserializes_connection_profiles_from_json() {
        let json = settings_json_with_user_patch(serde_json::json!({
            "connection_profiles": [
                {
                    "id": "codex-official",
                    "label": "Codex Official",
                    "agent": "codex",
                    "auth_mode": "api",
                    "api_key": "sk-test",
                    "auth_env_var": "OPENAI_API_KEY",
                    "env": { "OPENAI_BASE_URL": "https://api.openai.com/v1" },
                    "extra_args": ["--search"]
                }
            ]
        }));

        let settings: AllSettings = serde_json::from_str(&json).unwrap();

        assert_eq!(settings.user.connection_profiles.len(), 1);
        let profile = &settings.user.connection_profiles[0];
        assert_eq!(profile.id, "codex-official");
        assert_eq!(profile.agent, "codex");
        assert_eq!(profile.auth_mode, "api");
        assert_eq!(profile.api_key.as_deref(), Some("sk-test"));
        assert_eq!(profile.auth_env_var.as_deref(), Some("OPENAI_API_KEY"));
        assert_eq!(
            profile
                .env
                .as_ref()
                .and_then(|env| env.get("OPENAI_BASE_URL"))
                .map(String::as_str),
            Some("https://api.openai.com/v1")
        );
    }

    #[test]
    fn user_settings_deserializes_explicit_windows_msvc_env_modes() {
        let off_json =
            settings_json_with_user_patch(serde_json::json!({ "windows_msvc_env_mode": "off" }));
        let always_json =
            settings_json_with_user_patch(serde_json::json!({ "windows_msvc_env_mode": "always" }));

        let off: AllSettings = serde_json::from_str(&off_json).unwrap();
        let always: AllSettings = serde_json::from_str(&always_json).unwrap();

        assert_eq!(off.user.windows_msvc_env_mode, WindowsMsvcEnvMode::Off);
        assert_eq!(
            always.user.windows_msvc_env_mode,
            WindowsMsvcEnvMode::Always
        );
    }

    #[test]
    fn migrate_empty_base_url_fills_from_defaults() {
        // Credential has base_url = "" (empty string), known defaults have a base_url.
        // Migration should populate the empty base_url from defaults.
        let cred = PlatformCredential {
            platform_id: "ollama".to_string(),
            api_key: None,
            base_url: Some(String::new()), // empty string
            auth_env_var: None,
            name: None,
            models: None,
            extra_env: None,
        };
        let mut settings = make_settings_with_cred(cred);
        let changed = migrate_platform_credentials(&mut settings);

        assert!(changed, "migration should have made changes");
        assert_eq!(
            settings.user.platform_credentials[0].base_url.as_deref(),
            Some("http://localhost:11434"),
            "empty base_url should be filled from defaults"
        );
    }

    #[test]
    fn migrate_legacy_deepseek_chat_model_to_v4_pro() {
        let cred = PlatformCredential {
            platform_id: "deepseek".to_string(),
            api_key: Some("sk-test".to_string()),
            base_url: Some("https://api.deepseek.com/anthropic".to_string()),
            auth_env_var: None,
            name: None,
            models: Some(vec!["deepseek-chat".to_string()]),
            extra_env: None,
        };
        let mut settings = make_settings_with_cred(cred);
        let changed = migrate_platform_credentials(&mut settings);

        assert!(changed, "migration should rewrite legacy DeepSeek models");
        assert_eq!(
            settings.user.platform_credentials[0].models.as_ref(),
            Some(&vec![
                "deepseek-v4-pro".to_string(),
                "deepseek-v4-flash".to_string()
            ])
        );
    }

    #[test]
    fn provider_info_ccswitch() {
        let info = get_provider_info("ccswitch").expect("ccswitch should have provider info");
        assert!(info.key_optional);
        assert_eq!(info.base_url.as_deref(), Some("http://127.0.0.1:15721"));
        assert_eq!(info.auth_env_var.as_deref(), Some("ANTHROPIC_AUTH_TOKEN"));
    }

    #[test]
    fn provider_info_ccr() {
        let info = get_provider_info("ccr").expect("ccr should have provider info");
        assert!(info.key_optional);
        assert_eq!(info.base_url.as_deref(), Some("http://127.0.0.1:3456"));
        assert_eq!(
            info.models
                .as_ref()
                .and_then(|m| m.first())
                .map(|s| s.as_str()),
            Some("claude-sonnet-4-6")
        );
    }

    #[test]
    fn apply_agent_patch_effort_set_and_clear() {
        let mut s = AgentSettings::default_for("claude");
        assert_eq!(s.effort, None);

        // Set effort to "high"
        apply_agent_patch(&mut s, &serde_json::json!({ "effort": "high" }));
        assert_eq!(s.effort, Some("high".to_string()));

        // Clear with empty string
        apply_agent_patch(&mut s, &serde_json::json!({ "effort": "" }));
        assert_eq!(s.effort, None);

        // Set then clear with null
        apply_agent_patch(&mut s, &serde_json::json!({ "effort": "low" }));
        assert_eq!(s.effort, Some("low".to_string()));
        apply_agent_patch(&mut s, &serde_json::json!({ "effort": null }));
        assert_eq!(s.effort, None);

        // Absent key doesn't touch existing value
        apply_agent_patch(&mut s, &serde_json::json!({ "effort": "medium" }));
        apply_agent_patch(&mut s, &serde_json::json!({ "model": "opus" }));
        assert_eq!(s.effort, Some("medium".to_string()));
    }

    #[test]
    fn validate_ui_zoom_rejects_invalid() {
        assert!(validate_ui_zoom(&serde_json::json!(0.1)).is_err());
        assert!(validate_ui_zoom(&serde_json::json!(5.0)).is_err());
        assert!(validate_ui_zoom(&serde_json::json!("abc")).is_err());
    }

    #[test]
    fn validate_ui_zoom_accepts_valid() {
        assert_eq!(
            validate_ui_zoom(&serde_json::json!(1.0)).unwrap(),
            Some(1.0)
        );
        assert_eq!(
            validate_ui_zoom(&serde_json::json!(0.75)).unwrap(),
            Some(0.75)
        );
        assert_eq!(
            validate_ui_zoom(&serde_json::json!(1.5)).unwrap(),
            Some(1.5)
        );
        assert_eq!(validate_ui_zoom(&serde_json::json!(null)).unwrap(), None);
    }

    #[test]
    fn is_key_optional_known_platforms() {
        assert!(is_key_optional_platform("ccswitch"));
        assert!(is_key_optional_platform("ccr"));
        assert!(is_key_optional_platform("ollama"));
        assert!(!is_key_optional_platform("deepseek"));
        assert!(!is_key_optional_platform("unknown-platform"));
    }
}
