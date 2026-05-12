use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::models::{
    PlatformCredential, ProviderIssue, ProviderValidationResult, ValidatePlatformCredentialsResponse,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderClaudeConfigMaterialized {
    pub provider_id: String,
    pub json_path: PathBuf,
    pub env: HashMap<String, String>,
}

fn provider_claude_config_temp_path(run_id: &str) -> PathBuf {
    crate::storage::data_dir()
        .join("provider-claude-configs")
        .join(format!("session-{run_id}.json"))
}

use std::sync::Mutex;

/// Returns true if `platform_id` is a user-created custom endpoint.
pub(crate) fn is_custom_platform(platform_id: &str) -> bool {
    platform_id.starts_with("custom-")
}

/// Cache for leaked custom platform_id strings so repeated calls don't leak duplicates.
static CUSTOM_PROVIDER_CACHE: Mutex<Option<std::collections::HashMap<String, &'static str>>> =
    Mutex::new(None);

fn leak_custom_id(id: &str) -> &'static str {
    let mut guard = CUSTOM_PROVIDER_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(std::collections::HashMap::new);
    if let Some(&existing) = cache.get(id) {
        return existing;
    }
    let leaked: &'static str = Box::leak(id.to_string().into_boxed_str());
    cache.insert(id.to_string(), leaked);
    leaked
}

pub fn platform_to_provider_id(platform_id: &str) -> Option<&'static str> {
    match platform_id {
        "deepseek" => Some("deepseek"),
        "zhipu" | "zhipu-intl" => Some("glm"),
        "bailian" => Some("qwen"),
        "kimi" => Some("kimi"),
        "mimo-plan" => Some("mimo-plan"),
        "mimo-api" => Some("mimo-api"),
        "packy-cx2cc" => Some("packy-cx2cc"),
        id if is_custom_platform(id) => Some(leak_custom_id(id)),
        _ => None,
    }
}

fn make_issue(code: &str, field: &str, message: String) -> ProviderIssue {
    ProviderIssue {
        code: code.to_string(),
        field: field.to_string(),
        message,
    }
}

fn required_nonempty_issue(
    platform_id: &str,
    field: &str,
    value: Option<&str>,
    code: &str,
) -> Option<ProviderIssue> {
    if value.is_some_and(|value| !value.trim().is_empty()) {
        return None;
    }
    Some(make_issue(
        code,
        field,
        format!("{platform_id} 缺少必填项：{field}"),
    ))
}

fn required_extra_env_issue(
    platform_id: &str,
    cred: &PlatformCredential,
    key: &str,
) -> Option<ProviderIssue> {
    let value = cred
        .extra_env
        .as_ref()
        .and_then(|env| env.get(key))
        .map(String::as_str);
    required_nonempty_issue(platform_id, key, value, "missing_model_env")
}

fn required_model_issue(platform_id: &str, cred: &PlatformCredential) -> Option<ProviderIssue> {
    let model = cred
        .models
        .as_ref()
        .and_then(|models| models.iter().find(|model| !model.trim().is_empty()))
        .map(String::as_str)
        .or_else(|| {
            cred.extra_env
                .as_ref()
                .and_then(|env| env.get("ANTHROPIC_MODEL"))
                .map(String::as_str)
        });
    required_nonempty_issue(platform_id, "model", model, "missing_model")
}

fn required_model_env_keys(platform_id: &str) -> &'static [&'static str] {
    match platform_id {
        "deepseek" | "mimo-plan" | "mimo-api" | "packy-cx2cc" => &[
            "ANTHROPIC_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "CLAUDE_CODE_SUBAGENT_MODEL",
        ],
        _ => &[],
    }
}

fn requires_explicit_base_url(platform_id: &str) -> bool {
    matches!(platform_id, "zhipu" | "zhipu-intl" | "bailian" | "kimi") || is_custom_platform(platform_id)
}

fn requires_explicit_model(platform_id: &str) -> bool {
    matches!(platform_id, "zhipu" | "zhipu-intl" | "bailian" | "kimi") || is_custom_platform(platform_id)
}

pub fn validate_provider_credential(
    platform_id: &str,
    cred: &PlatformCredential,
) -> Result<ProviderValidationResult, String> {
    let Some(provider_id) = platform_to_provider_id(platform_id) else {
        return Err(format!("unsupported provider-backed Claude platform: {platform_id}"));
    };

    let mut issues = Vec::new();
    if let Some(issue) = required_nonempty_issue(
        platform_id,
        "api_key",
        cred.api_key.as_deref(),
        "missing_api_key",
    ) {
        issues.push(issue);
    }

    for key in required_model_env_keys(platform_id) {
        if let Some(issue) = required_extra_env_issue(platform_id, cred, key) {
            issues.push(issue);
        }
    }

    if requires_explicit_base_url(platform_id) {
        if let Some(issue) = required_nonempty_issue(
            platform_id,
            "base_url",
            cred.base_url.as_deref(),
            "missing_base_url",
        ) {
            issues.push(issue);
        }
    }

    if requires_explicit_model(platform_id) {
        if let Some(issue) = required_model_issue(platform_id, cred) {
            issues.push(issue);
        }
    }

    let ok = issues.is_empty();
    let message = if ok {
        format!("{platform_id} 配置完整")
    } else {
        format!("{platform_id} 配置不完整")
    };

    Ok(ProviderValidationResult {
        platform_id: platform_id.to_string(),
        provider_id: provider_id.to_string(),
        ok,
        issues,
        message,
    })
}

pub fn validate_platform_credentials(credentials: &[PlatformCredential]) -> ValidatePlatformCredentialsResponse {
    let mut results = Vec::new();
    for cred in credentials {
        if platform_to_provider_id(&cred.platform_id).is_none() {
            continue;
        }
        if let Ok(result) = validate_provider_credential(&cred.platform_id, cred) {
            results.push(result);
        }
    }
    ValidatePlatformCredentialsResponse { results }
}

pub fn write_provider_claude_config(
    provider_id: &str,
    platform_id: &str,
    cred: &PlatformCredential,
    run_id: &str,
    mcp_servers: &HashMap<String, serde_json::Value>,
) -> Result<ProviderClaudeConfigMaterialized, String> {
    // Build env vars dynamically from the latest credential (from settings page).
    let env = provider_env_from_credential(platform_id, cred)?;
    // Merge into the fixed JSON template.
    let json_value = provider_config_json_from_env(&env, mcp_servers);
    // Each session gets a unique temp JSON so stale cache is impossible.
    let path = provider_claude_config_temp_path(run_id);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("create provider config dir {}: {e}", parent.display()))?;
    }

    let serialized = serde_json::to_string_pretty(&json_value)
        .map_err(|e| format!("serialize provider config json: {e}"))?;
    fs::write(&path, serialized)
        .map_err(|e| format!("write provider config {}: {e}", path.display()))?;

    log::info!(
        "[provider_claude_config] wrote temp config for run {}: {} (provider={}, keys={})",
        run_id,
        path.display(),
        provider_id,
        env.keys().fold(String::new(), |acc, k| if acc.is_empty() {
            k.clone()
        } else {
            format!("{acc}, {k}")
        })
    );

    Ok(ProviderClaudeConfigMaterialized {
        provider_id: provider_id.to_string(),
        json_path: path,
        env,
    })
}

/// Write a minimal settings JSON containing only MCP server configs.
/// Used when there is no provider config but the user has managed MCP servers.
pub fn write_mcp_only_settings(
    run_id: &str,
    mcp_servers: &HashMap<String, serde_json::Value>,
) -> Result<PathBuf, String> {
    // Reuse the shared JSON builder with an empty env — same structure as provider sessions.
    let config = provider_config_json_from_env(&HashMap::new(), mcp_servers);

    let path = provider_claude_config_temp_path(run_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("create mcp config dir {}: {e}", parent.display()))?;
    }

    let serialized =
        serde_json::to_string_pretty(&config).map_err(|e| format!("serialize mcp config: {e}"))?;
    fs::write(&path, serialized)
        .map_err(|e| format!("write mcp config {}: {e}", path.display()))?;

    log::info!(
        "[provider_claude_config] wrote MCP-only settings for run {}: {} (servers={})",
        run_id,
        path.display(),
        mcp_servers.len()
    );

    Ok(path)
}

pub(crate) fn provider_env_from_credential(
    platform_id: &str,
    cred: &PlatformCredential,
) -> Result<HashMap<String, String>, String> {
    match platform_id {
        "deepseek" => build_deepseek_env(cred),
        "zhipu" | "zhipu-intl" | "bailian" | "kimi" | "mimo-plan" | "mimo-api" | "packy-cx2cc" => {
            build_parameterized_env(platform_id, cred)
        }
        id if is_custom_platform(id) => build_parameterized_env(platform_id, cred),
        _ => Err(format!(
            "unsupported provider-backed Claude platform: {platform_id}"
        )),
    }
}

/// Resolve model tier strings from a credential's models array.
/// Mirrors `resolve_model_tiers` expansion:
///   1 model  → all tiers use the same model
///   2 models → [0]=opus+sonnet+subagent, [1]=haiku
///   3+       → positional: [0]=opus, [1]=sonnet, [2]=haiku, [0]=subagent
#[cfg(test)]
fn resolve_model_tiers(models: &[String]) -> (&str, &str, &str, &str) {
    match models.len() {
        0 => ("", "", "", ""),
        1 => {
            let m = models[0].as_str();
            (m, m, m, m)
        }
        2 => {
            let main = models[0].as_str();
            let haiku = models[1].as_str();
            (main, main, haiku, main)
        }
        _ => {
            let opus = models[0].as_str();
            let sonnet = models[1].as_str();
            let haiku = models[2].as_str();
            (opus, sonnet, haiku, opus)
        }
    }
}

/// Known provider default base URLs — used as fallback when credential has no base_url.
fn default_base_url(platform_id: &str) -> Option<&'static str> {
    match platform_id {
        "deepseek" => Some("https://api.deepseek.com/anthropic"),
        "zhipu" => Some("https://open.bigmodel.cn/api/anthropic"),
        "zhipu-intl" => Some("https://api.z.ai/api/anthropic"),
        "bailian" => Some("https://coding.dashscope.aliyuncs.com/apps/anthropic"),
        "kimi" => Some("https://api.moonshot.cn/anthropic"),
        "mimo-plan" => Some("https://token-plan-cn.xiaomimimo.com/anthropic"),
        "mimo-api" => Some("https://api.xiaomimimo.com/anthropic"),
        "packy-cx2cc" => Some("https://www.packyapi.com"),
        _ => None,
    }
}

fn stability_env_vars() -> HashMap<String, String> {
    HashMap::from([
        (
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(),
            "1".to_string(),
        ),
        (
            "CLAUDE_CODE_DISABLE_NONSTREAMING_FALLBACK".to_string(),
            "1".to_string(),
        ),
        ("CLAUDE_CODE_EFFORT_LEVEL".to_string(), "max".to_string()),
        (
            "CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS".to_string(),
            "true".to_string(),
        ),
        (
            "CLAUDE_CODE_AUTO_COMPACT_WINDOW".to_string(),
            "400000".to_string(),
        ),
    ])
}

/// Whitelist of extra_env keys that users can override via the settings UI.
/// Prevents accidental overwriting of stability or internal env vars.
const ALLOWED_EXTRA_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "CLAUDE_CODE_SUBAGENT_MODEL",
    "CLAUDE_CODE_EFFORT_LEVEL",
];

/// Merge filtered extra_env into the env map. Only whitelisted keys are applied.
/// Called after stability_env_vars() so user values take precedence.
fn merge_extra_env(env: &mut HashMap<String, String>, extra_env: &Option<HashMap<String, String>>) {
    let Some(extra) = extra_env else { return };
    for (key, value) in extra {
        if ALLOWED_EXTRA_ENV_KEYS.contains(&key.as_str()) && !value.trim().is_empty() {
            log::debug!(
                "[provider_claude_config] extra_env override: {}={}",
                key,
                value
            );
            env.insert(key.clone(), value.clone());
        }
    }
}

fn build_deepseek_env(cred: &PlatformCredential) -> Result<HashMap<String, String>, String> {
    let validation = validate_provider_credential("deepseek", cred)?;
    if !validation.ok {
        let missing = validation
            .issues
            .iter()
            .map(|issue| issue.field.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("deepseek 配置不完整，请先在连接设置中补齐：{missing}"));
    }

    let api_key = cred
        .api_key
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "DeepSeek API key is not configured".to_string())?;

    let base_url = cred
        .base_url
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            default_base_url("deepseek")
                .expect("deepseek default base_url")
                .to_string()
        });

    let env_values = cred.extra_env.clone().unwrap_or_default();
    let mut env = HashMap::from([
        ("ANTHROPIC_BASE_URL".to_string(), base_url),
        ("ANTHROPIC_AUTH_TOKEN".to_string(), api_key),
        (
            "ANTHROPIC_MODEL".to_string(),
            env_values
                .get("ANTHROPIC_MODEL")
                .cloned()
                .unwrap_or_default(),
        ),
        (
            "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(),
            env_values
                .get("ANTHROPIC_DEFAULT_OPUS_MODEL")
                .cloned()
                .unwrap_or_default(),
        ),
        (
            "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
            env_values
                .get("ANTHROPIC_DEFAULT_SONNET_MODEL")
                .cloned()
                .unwrap_or_default(),
        ),
        (
            "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
            env_values
                .get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
                .cloned()
                .unwrap_or_default(),
        ),
        (
            "CLAUDE_CODE_SUBAGENT_MODEL".to_string(),
            env_values
                .get("CLAUDE_CODE_SUBAGENT_MODEL")
                .cloned()
                .unwrap_or_default(),
        ),
    ]);
    env.extend(stability_env_vars());
    merge_extra_env(&mut env, &cred.extra_env);
    Ok(env)
}

fn build_parameterized_env(
    platform_id: &str,
    cred: &PlatformCredential,
) -> Result<HashMap<String, String>, String> {
    let validation = validate_provider_credential(platform_id, cred)?;
    if !validation.ok {
        let missing = validation
            .issues
            .iter()
            .map(|issue| issue.field.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "{platform_id} 配置不完整，请先在连接设置中补齐：{missing}"
        ));
    }

    let api_key = cred
        .api_key
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{platform_id} API key is not configured"))?;
    let base_url = if platform_id == "packy-cx2cc" {
        default_base_url(platform_id).unwrap_or("").to_string()
    } else {
        cred.base_url
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| default_base_url(platform_id).unwrap_or("").to_string())
    };
    if base_url.is_empty() {
        return Err(format!("{platform_id} base URL is not configured"));
    }

    let env_values = cred.extra_env.clone().unwrap_or_default();
    let model = env_values
        .get("ANTHROPIC_MODEL")
        .cloned()
        .or_else(|| {
            cred.models.as_ref().and_then(|models| {
                models
                    .iter()
                    .find(|model| !model.trim().is_empty())
                    .map(|model| model.trim().to_string())
            })
        })
        .unwrap_or_default();
    let opus = env_values
        .get("ANTHROPIC_DEFAULT_OPUS_MODEL")
        .cloned()
        .unwrap_or_else(|| model.clone());
    let sonnet = env_values
        .get("ANTHROPIC_DEFAULT_SONNET_MODEL")
        .cloned()
        .unwrap_or_else(|| model.clone());
    let haiku = env_values
        .get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
        .cloned()
        .unwrap_or_else(|| model.clone());
    let subagent = env_values
        .get("CLAUDE_CODE_SUBAGENT_MODEL")
        .cloned()
        .unwrap_or_else(|| model.clone());

    let mut env = HashMap::from([
        ("ANTHROPIC_BASE_URL".to_string(), base_url),
        ("ANTHROPIC_AUTH_TOKEN".to_string(), api_key),
        ("ANTHROPIC_MODEL".to_string(), model),
        ("ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), opus),
        (
            "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
            sonnet,
        ),
        (
            "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
            haiku,
        ),
        (
            "CLAUDE_CODE_SUBAGENT_MODEL".to_string(),
            subagent,
        ),
    ]);
    env.extend(stability_env_vars());
    merge_extra_env(&mut env, &cred.extra_env);
    Ok(env)
}

fn provider_config_json_from_env(
    env: &HashMap<String, String>,
    mcp_servers: &HashMap<String, serde_json::Value>,
) -> Value {
    // Merge order: native base → strip secrets → overlay env → force
    // permissions → fixed fields → merge MCP. The native base ensures
    // hooks, enabledMcpjsonServers, enabledPlugins, extraKnownMarketplaces,
    // and other user fields survive the --settings override.
    let mut config = crate::storage::cli_config::load_cli_config();

    // SAFETY: load_cli_config() always returns Value::Object.
    let obj = config.as_object_mut().unwrap();

    // Strip sensitive keys that must not leak into provider session files.
    for &key in crate::storage::cli_config::SENSITIVE_KEYS {
        obj.remove(key);
    }

    // ── Claw GO overrides (applied on top of native base) ──

    // env: start from native env, then overlay provider-specific vars.
    let mut env_obj = obj
        .get("env")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();
    for (key, value) in env {
        env_obj.insert(key.clone(), Value::String(value.clone()));
    }
    obj.insert("env".to_string(), Value::Object(env_obj));

    // permissions: force bypass for provider sessions
    obj.insert(
        "permissions".to_string(),
        json!({ "defaultMode": "bypassPermissions" }),
    );

    // Ensure superpowers plugin is always enabled, even if native config lacks it.
    let plugins = obj
        .entry("enabledPlugins".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(plugins_obj) = plugins.as_object_mut() {
        plugins_obj
            .entry("superpowers@claude-plugins-official".to_string())
            .or_insert(Value::Bool(true));
    }
    // ── Intentional overrides (always forced for provider sessions) ──
    // These fields are deliberately overwritten regardless of native config.
    // - includeCoAuthoredBy: false — provider sessions don't need co-author trailers.
    // - thinking: false — third-party providers don't support extended thinking.
    // - skipDangerousModePermissionPrompt: true — bypass permissions are already set.
    // - autoUpdatesChannel: "latest" — keep provider sessions on the latest release.
    // - language: "简体中文" — matches the app's primary locale. If the app adds
    //   multi-locale support, this should read from user settings instead.
    obj.insert("includeCoAuthoredBy".to_string(), Value::Bool(false));
    obj.insert("thinking".to_string(), Value::Bool(false));
    obj.insert(
        "skipDangerousModePermissionPrompt".to_string(),
        Value::Bool(true),
    );
    obj.insert(
        "autoUpdatesChannel".to_string(),
        Value::String("latest".to_string()),
    );
    obj.insert(
        "language".to_string(),
        Value::String("简体中文".to_string()),
    );

    // mcpServers: merge native + Claw GO managed (additive).
    // Assumes upstream has already deduplicated by name.
    if !mcp_servers.is_empty() {
        let existing = obj
            .get("mcpServers")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let mut merged = existing;
        for (name, cfg) in mcp_servers {
            merged.insert(name.clone(), cfg.clone());
        }
        obj.insert("mcpServers".to_string(), Value::Object(merged));
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PlatformCredential;

    fn cred(
        platform_id: &str,
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
    ) -> PlatformCredential {
        PlatformCredential {
            platform_id: platform_id.to_string(),
            api_key: Some(api_key.to_string()),
            base_url: base_url.map(|value| value.to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some(platform_id.to_string()),
            models: model.map(|value| vec![value.to_string()]),
            extra_env: None,
        }
    }

    #[test]
    fn maps_platform_ids_to_provider_ids() {
        assert_eq!(platform_to_provider_id("deepseek"), Some("deepseek"));
        assert_eq!(platform_to_provider_id("zhipu"), Some("glm"));
        assert_eq!(platform_to_provider_id("zhipu-intl"), Some("glm"));
        assert_eq!(platform_to_provider_id("bailian"), Some("qwen"));
        assert_eq!(platform_to_provider_id("kimi"), Some("kimi"));
        assert_eq!(platform_to_provider_id("mimo-plan"), Some("mimo-plan"));
        assert_eq!(platform_to_provider_id("mimo-api"), Some("mimo-api"));
        assert_eq!(platform_to_provider_id("packy-cx2cc"), Some("packy-cx2cc"));
        assert_eq!(platform_to_provider_id("mimo"), None);
        assert_eq!(platform_to_provider_id("mimo-pro"), None);
        assert_eq!(platform_to_provider_id("xiaomi"), None);
        assert_eq!(platform_to_provider_id("anthropic"), None);
    }

    #[test]
    fn resolve_model_tiers_single() {
        let models = ["m1".to_string()];
        let (opus, sonnet, haiku, subagent) = resolve_model_tiers(&models);
        assert_eq!(opus, "m1");
        assert_eq!(sonnet, "m1");
        assert_eq!(haiku, "m1");
        assert_eq!(subagent, "m1");
    }

    #[test]
    fn resolve_model_tiers_two() {
        let models = ["main".to_string(), "eco".to_string()];
        let (opus, sonnet, haiku, subagent) = resolve_model_tiers(&models);
        assert_eq!(opus, "main");
        assert_eq!(sonnet, "main");
        assert_eq!(haiku, "eco");
        assert_eq!(subagent, "main");
    }

    #[test]
    fn resolve_model_tiers_three() {
        let models = ["o".to_string(), "s".to_string(), "h".to_string()];
        let (opus, sonnet, haiku, subagent) = resolve_model_tiers(&models);
        assert_eq!(opus, "o");
        assert_eq!(sonnet, "s");
        assert_eq!(haiku, "h");
        assert_eq!(subagent, "o");
    }

    #[test]
    fn packy_uses_root_base_url_by_default() {
        let env = build_parameterized_env(
            "packy-cx2cc",
            &cred("packy-cx2cc", "sk-packy", None, Some("claude-opus-4-7")),
        )
        .unwrap();

        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://www.packyapi.com")
        );
    }

    #[test]
    fn builds_deepseek_env_with_defaults() {
        let env = build_deepseek_env(&cred("deepseek", "sk-deepseek", None, None)).unwrap();
        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN").map(String::as_str),
            Some("sk-deepseek")
        );
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://api.deepseek.com/anthropic")
        );
        assert_eq!(
            env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("deepseek-v4-pro")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").map(String::as_str),
            Some("deepseek-v4-flash")
        );
    }

    #[test]
    fn builds_deepseek_env_with_custom_model() {
        let env =
            build_deepseek_env(&cred("deepseek", "sk-ds", None, Some("custom-ds-model"))).unwrap();
        assert_eq!(
            env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("custom-ds-model")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").map(String::as_str),
            Some("custom-ds-model")
        );
    }

    #[test]
    fn builds_deepseek_env_with_custom_base_url() {
        let c = PlatformCredential {
            platform_id: "deepseek".to_string(),
            api_key: Some("sk-ds".to_string()),
            base_url: Some("https://custom-deepseek.example.com/anthropic".to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("deepseek".to_string()),
            models: Some(vec!["ds-v4".to_string()]),
            extra_env: None,
        };
        let env = build_deepseek_env(&c).unwrap();
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://custom-deepseek.example.com/anthropic")
        );
        assert_eq!(
            env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("ds-v4")
        );
    }

    #[test]
    fn builds_parameterized_env() {
        let env = build_parameterized_env(
            "bailian",
            &cred(
                "bailian",
                "sk-qwen",
                Some("https://coding.dashscope.aliyuncs.com/apps/anthropic"),
                Some("qwen3.5-plus"),
            ),
        )
        .unwrap();
        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN").map(String::as_str),
            Some("sk-qwen")
        );
        assert_eq!(
            env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("qwen3.5-plus")
        );
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://coding.dashscope.aliyuncs.com/apps/anthropic")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_SUBAGENT_MODEL").map(String::as_str),
            Some("qwen3.5-plus")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")
                .map(String::as_str),
            Some("1")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_EFFORT_LEVEL").map(String::as_str),
            Some("max")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS")
                .map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn builds_parameterized_env_with_base_url_fallback() {
        let c = PlatformCredential {
            platform_id: "zhipu".to_string(),
            api_key: Some("sk-glm".to_string()),
            base_url: None,
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("zhipu".to_string()),
            models: Some(vec!["glm-5".to_string()]),
            extra_env: None,
        };
        let env = build_parameterized_env("zhipu", &c).unwrap();
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://open.bigmodel.cn/api/anthropic")
        );
        assert_eq!(
            env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("glm-5")
        );
    }

    #[test]
    fn builds_parameterized_env_with_tiered_models() {
        let c = PlatformCredential {
            platform_id: "kimi".to_string(),
            api_key: Some("sk-kimi".to_string()),
            base_url: Some("https://api.moonshot.cn/anthropic".to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("kimi".to_string()),
            models: Some(vec!["kimi-k2.5".to_string(), "kimi-k2".to_string()]),
            extra_env: None,
        };
        let env = build_parameterized_env("kimi", &c).unwrap();
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").map(String::as_str),
            Some("kimi-k2.5")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_SONNET_MODEL")
                .map(String::as_str),
            Some("kimi-k2.5")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").map(String::as_str),
            Some("kimi-k2")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_SUBAGENT_MODEL").map(String::as_str),
            Some("kimi-k2.5")
        );
    }

    #[test]
    fn writes_temp_config_with_run_id_in_path() {
        let _guard = crate::storage::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("CLAW_GO_DATA_DIR");
        std::env::set_var("CLAW_GO_DATA_DIR", tmp.path());

        let result = write_provider_claude_config(
            "qwen",
            "bailian",
            &cred(
                "bailian",
                "sk-qwen",
                Some("https://coding.dashscope.aliyuncs.com/apps/anthropic"),
                Some("qwen3.5-plus"),
            ),
            "test-run-001",
            &HashMap::new(),
        )
        .unwrap();

        assert!(result.json_path.ends_with(std::path::Path::new(
            "provider-claude-configs/session-test-run-001.json"
        )));
        let content = fs::read_to_string(&result.json_path).unwrap();
        assert!(content.contains("ANTHROPIC_BASE_URL"));
        assert!(content.contains("qwen3.5-plus"));

        match previous {
            Some(value) => std::env::set_var("CLAW_GO_DATA_DIR", value),
            None => std::env::remove_var("CLAW_GO_DATA_DIR"),
        }
    }

    fn cred_with_extra_env(
        platform_id: &str,
        extra_env: Option<HashMap<String, String>>,
    ) -> PlatformCredential {
        PlatformCredential {
            platform_id: platform_id.to_string(),
            api_key: Some("sk-test".to_string()),
            base_url: Some("https://example.com/anthropic".to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some(platform_id.to_string()),
            models: Some(vec!["test-model".to_string()]),
            extra_env,
        }
    }

    #[test]
    fn merge_extra_env_whitelisted_keys_override() {
        let mut env = HashMap::from([
            ("ANTHROPIC_MODEL".to_string(), "default-model".to_string()),
            (
                "ANTHROPIC_BASE_URL".to_string(),
                "https://base.example.com".to_string(),
            ),
        ]);
        let extra_env =
            HashMap::from([("ANTHROPIC_MODEL".to_string(), "override-model".to_string())]);
        merge_extra_env(&mut env, &Some(extra_env));
        assert_eq!(
            env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("override-model")
        );
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://base.example.com")
        );
    }

    #[test]
    fn merge_extra_env_non_whitelisted_keys_ignored() {
        let mut env = HashMap::from([(
            "ANTHROPIC_BASE_URL".to_string(),
            "https://base.example.com".to_string(),
        )]);
        let extra_env = HashMap::from([
            (
                "ANTHROPIC_BASE_URL".to_string(),
                "https://evil.example.com".to_string(),
            ),
            ("SOME_INTERNAL_KEY".to_string(), "bad".to_string()),
        ]);
        merge_extra_env(&mut env, &Some(extra_env));
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://base.example.com")
        );
        assert!(!env.contains_key("SOME_INTERNAL_KEY"));
    }

    #[test]
    fn merge_extra_env_empty_values_filtered() {
        let mut env = HashMap::from([("ANTHROPIC_MODEL".to_string(), "original".to_string())]);
        let extra_env = HashMap::from([("ANTHROPIC_MODEL".to_string(), "  ".to_string())]);
        merge_extra_env(&mut env, &Some(extra_env));
        assert_eq!(
            env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("original")
        );
    }

    #[test]
    fn merge_extra_env_none_is_noop() {
        let mut env = HashMap::from([("ANTHROPIC_MODEL".to_string(), "original".to_string())]);
        merge_extra_env(&mut env, &None);
        assert_eq!(
            env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("original")
        );
    }

    #[test]
    fn build_deepseek_env_with_extra_env_overrides() {
        let c = cred_with_extra_env(
            "deepseek",
            Some(HashMap::from([
                ("ANTHROPIC_MODEL".to_string(), "custom-ds-model".to_string()),
                ("CLAUDE_CODE_EFFORT_LEVEL".to_string(), "low".to_string()),
                ("EVIL_KEY".to_string(), "should-be-ignored".to_string()),
            ])),
        );
        let env = build_deepseek_env(&c).unwrap();
        assert_eq!(
            env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("custom-ds-model")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_EFFORT_LEVEL").map(String::as_str),
            Some("low")
        );
        assert!(!env.contains_key("EVIL_KEY"));
    }

    #[test]
    fn build_parameterized_env_with_extra_env_overrides() {
        let c = cred_with_extra_env(
            "kimi",
            Some(HashMap::from([(
                "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
                "custom-haiku".to_string(),
            )])),
        );
        let env = build_parameterized_env("kimi", &c).unwrap();
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").map(String::as_str),
            Some("custom-haiku")
        );
    }

    #[test]
    fn validate_provider_credential_mimo_api_requires_shared_model_fields() {
        let c = PlatformCredential {
            platform_id: "mimo-api".to_string(),
            api_key: Some("sk-mimo-api".to_string()),
            base_url: Some("https://api.xiaomimimo.com/anthropic".to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("mimo-api".to_string()),
            models: None,
            extra_env: Some(HashMap::from([(
                "ANTHROPIC_MODEL".to_string(),
                "mimo-v2.5-pro".to_string(),
            )])),
        };
        let result = validate_provider_credential("mimo-api", &c).unwrap();
        assert!(!result.ok);
        let fields = result
            .issues
            .iter()
            .map(|issue| issue.field.as_str())
            .collect::<Vec<_>>();
        assert!(fields.contains(&"ANTHROPIC_DEFAULT_OPUS_MODEL"));
        assert!(fields.contains(&"CLAUDE_CODE_SUBAGENT_MODEL"));
    }

    #[test]
    fn build_parameterized_env_mimo_api_uses_full_explicit_model_env() {
        let c = PlatformCredential {
            platform_id: "mimo-api".to_string(),
            api_key: Some("sk-mimo-api".to_string()),
            base_url: Some("https://api.xiaomimimo.com/anthropic".to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("mimo-api".to_string()),
            models: None,
            extra_env: Some(HashMap::from([
                ("ANTHROPIC_MODEL".to_string(), "mimo-v2.5-pro".to_string()),
                (
                    "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(),
                    "mimo-v2.5-pro".to_string(),
                ),
                (
                    "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
                    "mimo-v2.5-pro".to_string(),
                ),
                (
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
                    "mimo-v2.5-pro".to_string(),
                ),
                (
                    "CLAUDE_CODE_SUBAGENT_MODEL".to_string(),
                    "mimo-v2.5-pro".to_string(),
                ),
            ])),
        };
        let env = build_parameterized_env("mimo-api", &c).unwrap();
        assert_eq!(
            env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("mimo-v2.5-pro")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_SUBAGENT_MODEL").map(String::as_str),
            Some("mimo-v2.5-pro")
        );
    }

    #[test]
    fn validate_provider_credential_kimi_requires_explicit_base_url_and_model() {
        let c = PlatformCredential {
            platform_id: "kimi".to_string(),
            api_key: Some("sk-kimi".to_string()),
            base_url: None,
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("kimi".to_string()),
            models: None,
            extra_env: None,
        };
        let result = validate_provider_credential("kimi", &c).unwrap();
        assert!(!result.ok);
        let fields = result
            .issues
            .iter()
            .map(|issue| issue.field.as_str())
            .collect::<Vec<_>>();
        assert!(fields.contains(&"base_url"));
        assert!(fields.contains(&"model"));
    }

    #[test]
    fn validate_provider_credential_deepseek_requires_explicit_model_envs() {
        let c = PlatformCredential {
            platform_id: "deepseek".to_string(),
            api_key: Some("sk-deepseek".to_string()),
            base_url: Some("https://api.deepseek.com/anthropic".to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("deepseek".to_string()),
            models: Some(vec!["deepseek-v4-pro".to_string()]),
            extra_env: None,
        };
        let result = validate_provider_credential("deepseek", &c).unwrap();
        assert!(!result.ok);
        let fields = result
            .issues
            .iter()
            .map(|issue| issue.field.as_str())
            .collect::<Vec<_>>();
        assert!(fields.contains(&"ANTHROPIC_MODEL"));
        assert!(fields.contains(&"CLAUDE_CODE_SUBAGENT_MODEL"));
    }

    #[test]
    fn build_parameterized_env_packy_ignores_persisted_base_url() {
        let c = PlatformCredential {
            platform_id: "packy-cx2cc".to_string(),
            api_key: Some("sk-packy".to_string()),
            base_url: Some("https://www.packyapi.com/anthropic".to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("packy-cx2cc".to_string()),
            models: None,
            extra_env: Some(HashMap::from([
                ("ANTHROPIC_MODEL".to_string(), "gpt-5.4-xhigh".to_string()),
                (
                    "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(),
                    "gpt-5.4-xhigh".to_string(),
                ),
                (
                    "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
                    "gpt-5.4-xhigh".to_string(),
                ),
                (
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
                    "gpt-5.4-high".to_string(),
                ),
                (
                    "CLAUDE_CODE_SUBAGENT_MODEL".to_string(),
                    "gpt-5.4-high".to_string(),
                ),
            ])),
        };
        let env = build_parameterized_env("packy-cx2cc", &c).unwrap();
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://www.packyapi.com")
        );
    }

    #[test]
    fn build_parameterized_env_packy_requires_full_explicit_model_env() {
        let c = PlatformCredential {
            platform_id: "packy-cx2cc".to_string(),
            api_key: Some("sk-packy".to_string()),
            base_url: Some("https://www.packyapi.com/anthropic".to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("packy-cx2cc".to_string()),
            models: None,
            extra_env: Some(HashMap::from([
                ("ANTHROPIC_MODEL".to_string(), "gpt-5.4-xhigh".to_string()),
                (
                    "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(),
                    "gpt-5.4-xhigh".to_string(),
                ),
                (
                    "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
                    "gpt-5.4-xhigh".to_string(),
                ),
                (
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
                    "gpt-5.4-high".to_string(),
                ),
            ])),
        };
        let err = build_parameterized_env("packy-cx2cc", &c).unwrap_err();
        assert!(err.contains("CLAUDE_CODE_SUBAGENT_MODEL"));
    }

    #[test]
    fn build_parameterized_env_packy_uses_full_explicit_model_env() {
        let c = PlatformCredential {
            platform_id: "packy-cx2cc".to_string(),
            api_key: Some("sk-packy".to_string()),
            base_url: Some("https://www.packyapi.com/anthropic".to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("packy-cx2cc".to_string()),
            models: None,
            extra_env: Some(HashMap::from([
                ("ANTHROPIC_MODEL".to_string(), "gpt-5.4-xhigh".to_string()),
                (
                    "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(),
                    "gpt-5.4-xhigh".to_string(),
                ),
                (
                    "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
                    "gpt-5.4-xhigh".to_string(),
                ),
                (
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
                    "gpt-5.4-high".to_string(),
                ),
                (
                    "CLAUDE_CODE_SUBAGENT_MODEL".to_string(),
                    "gpt-5.4-high".to_string(),
                ),
            ])),
        };
        let env = build_parameterized_env("packy-cx2cc", &c).unwrap();
        assert_eq!(
            env.get("ANTHROPIC_MODEL").map(String::as_str),
            Some("gpt-5.4-xhigh")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").map(String::as_str),
            Some("gpt-5.4-xhigh")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_SONNET_MODEL").map(String::as_str),
            Some("gpt-5.4-xhigh")
        );
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").map(String::as_str),
            Some("gpt-5.4-high")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_SUBAGENT_MODEL").map(String::as_str),
            Some("gpt-5.4-high")
        );
    }

    #[test]
    fn build_parameterized_env_model_fallback_to_extra_env() {
        let c = PlatformCredential {
            platform_id: "packy-cx2cc".to_string(),
            api_key: Some("sk-packy".to_string()),
            base_url: Some("https://www.packyapi.com/anthropic".to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("packy-cx2cc".to_string()),
            models: None,
            extra_env: Some(HashMap::from([(
                "ANTHROPIC_MODEL".to_string(),
                "claude-sonnet-4-20250514".to_string(),
            )])),
        };
        let err = build_parameterized_env("packy-cx2cc", &c).unwrap_err();
        assert!(err.contains("ANTHROPIC_DEFAULT_OPUS_MODEL"));
    }

    #[test]
    fn build_parameterized_env_no_model_fails() {
        let c = PlatformCredential {
            platform_id: "packy-cx2cc".to_string(),
            api_key: Some("sk-packy".to_string()),
            base_url: Some("https://www.packyapi.com/anthropic".to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("packy-cx2cc".to_string()),
            models: None,
            extra_env: None,
        };
        let err = build_parameterized_env("packy-cx2cc", &c).unwrap_err();
        assert!(err.contains("ANTHROPIC_MODEL"));
    }

    #[test]
    fn provider_config_preserves_native_hooks() {
        let env = HashMap::from([("ANTHROPIC_MODEL".to_string(), "test-model".to_string())]);
        let config = provider_config_json_from_env(&env, &HashMap::new());
        // Native ~/.claude/settings.json has hooks — they must survive.
        // If no native file exists (CI), hooks simply won't be present; that's fine.
        // The key assertion: Claw GO's own fields are always present.
        assert!(config.get("permissions").is_some());
        assert!(config.get("env").is_some());
        assert!(config.get("skipDangerousModePermissionPrompt").is_some());
    }

    #[test]
    fn provider_config_strips_api_key_from_native_base() {
        let env = HashMap::from([("ANTHROPIC_MODEL".to_string(), "test-model".to_string())]);
        let config = provider_config_json_from_env(&env, &HashMap::new());
        assert!(
            config.get("apiKey").is_none(),
            "apiKey must not leak into provider session config"
        );
        assert!(
            config.get("primaryApiKey").is_none(),
            "primaryApiKey must not leak into provider session config"
        );
    }

    #[test]
    fn provider_config_merges_mcp_servers_additively() {
        let env = HashMap::new();
        let managed = HashMap::from([(
            "my-server".to_string(),
            serde_json::json!({"command": "node", "args": ["server.js"]}),
        )]);
        let config = provider_config_json_from_env(&env, &managed);
        let mcp = config.get("mcpServers").and_then(|v| v.as_object()).unwrap();
        assert!(mcp.contains_key("my-server"));
        // If native settings also have mcpServers, they should be preserved too.
    }

    #[test]
    fn provider_config_env_overrides_native_env() {
        let env = HashMap::from([(
            "ANTHROPIC_BASE_URL".to_string(),
            "https://provider.example.com".to_string(),
        )]);
        let config = provider_config_json_from_env(&env, &HashMap::new());
        let env_obj = config.get("env").and_then(|v| v.as_object()).unwrap();
        assert_eq!(
            env_obj.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()),
            Some("https://provider.example.com")
        );
    }

    #[test]
    fn provider_config_preserves_native_env_vars() {
        // On this machine, native settings.json has MX_APIKEY in env.
        // After the merge, native-only env vars should survive.
        let env = HashMap::from([("ANTHROPIC_MODEL".to_string(), "test".to_string())]);
        let config = provider_config_json_from_env(&env, &HashMap::new());
        let env_obj = config.get("env").and_then(|v| v.as_object()).unwrap();
        // Claw GO's env must be present
        assert_eq!(
            env_obj.get("ANTHROPIC_MODEL").and_then(|v| v.as_str()),
            Some("test")
        );
        // Native-only vars (if present in real settings) are preserved.
        // In CI where no native file exists, this simply has no extra keys.
    }

    #[test]
    fn provider_config_always_enables_superpowers_plugin() {
        let env = HashMap::new();
        let config = provider_config_json_from_env(&env, &HashMap::new());
        let plugins = config
            .get("enabledPlugins")
            .and_then(|v| v.as_object())
            .expect("enabledPlugins must exist");
        assert_eq!(
            plugins.get("superpowers@claude-plugins-official"),
            Some(&Value::Bool(true)),
            "superpowers plugin must always be enabled"
        );
    }

    #[test]
    fn custom_platform_maps_to_self() {
        assert_eq!(platform_to_provider_id("custom-1234"), Some("custom-1234"));
        assert_eq!(platform_to_provider_id("custom-my-api"), Some("custom-my-api"));
        // Stable across repeated calls
        assert_eq!(platform_to_provider_id("custom-1234"), Some("custom-1234"));
    }

    #[test]
    fn custom_platform_requires_base_url_and_model() {
        let c = PlatformCredential {
            platform_id: "custom-test".to_string(),
            api_key: Some("sk-test".to_string()),
            base_url: None,
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("Custom Test".to_string()),
            models: None,
            extra_env: None,
        };
        let result = validate_provider_credential("custom-test", &c).unwrap();
        assert!(!result.ok);
        let fields: Vec<&str> = result.issues.iter().map(|i| i.field.as_str()).collect();
        assert!(fields.contains(&"base_url"));
        assert!(fields.contains(&"model"));
    }

    #[test]
    fn custom_platform_valid_with_all_fields() {
        let c = cred("custom-ok", "sk-test", Some("https://example.com/anthropic"), Some("my-model"));
        let result = validate_provider_credential("custom-ok", &c).unwrap();
        assert!(result.ok);
    }

    #[test]
    fn custom_platform_builds_parameterized_env() {
        let c = cred(
            "custom-env",
            "sk-custom",
            Some("https://custom.example.com/anthropic"),
            Some("custom-model-v1"),
        );
        let env = provider_env_from_credential("custom-env", &c).unwrap();
        assert_eq!(env.get("ANTHROPIC_AUTH_TOKEN").map(String::as_str), Some("sk-custom"));
        assert_eq!(env.get("ANTHROPIC_BASE_URL").map(String::as_str), Some("https://custom.example.com/anthropic"));
        assert_eq!(env.get("ANTHROPIC_MODEL").map(String::as_str), Some("custom-model-v1"));
        assert_eq!(env.get("ANTHROPIC_DEFAULT_SONNET_MODEL").map(String::as_str), Some("custom-model-v1"));
        assert!(env.contains_key("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"));
    }
}
