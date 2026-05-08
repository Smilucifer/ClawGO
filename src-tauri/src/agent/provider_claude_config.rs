use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::models::PlatformCredential;

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

pub fn platform_to_provider_id(platform_id: &str) -> Option<&'static str> {
    match platform_id {
        "deepseek" => Some("deepseek"),
        "zhipu" | "zhipu-intl" => Some("glm"),
        "bailian" => Some("qwen"),
        "kimi" => Some("kimi"),
        "mimo-pro" => Some("mimo-pro"),
        "packy-cx2cc" => Some("packy-cx2cc"),
        _ => None,
    }
}

pub fn write_provider_claude_config(
    provider_id: &str,
    platform_id: &str,
    cred: &PlatformCredential,
    run_id: &str,
) -> Result<ProviderClaudeConfigMaterialized, String> {
    // Build env vars dynamically from the latest credential (from settings page).
    let env = provider_env_from_credential(platform_id, cred)?;
    // Merge into the fixed JSON template.
    let json_value = provider_config_json_from_env(&env);
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

pub(crate) fn provider_env_from_credential(
    platform_id: &str,
    cred: &PlatformCredential,
) -> Result<HashMap<String, String>, String> {
    match platform_id {
        "deepseek" => build_deepseek_env(cred),
        "zhipu" | "zhipu-intl" | "bailian" | "kimi" | "mimo-pro" | "packy-cx2cc" => build_parameterized_env(platform_id, cred),
        _ => Err(format!("unsupported provider-backed Claude platform: {platform_id}")),
    }
}

/// Resolve model tier strings from a credential's models array.
/// Mirrors `resolve_model_tiers` expansion:
///   1 model  → all tiers use the same model
///   2 models → [0]=opus+sonnet+subagent, [1]=haiku
///   3+       → positional: [0]=opus, [1]=sonnet, [2]=haiku, [0]=subagent
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
        "mimo-pro" => Some("https://token-plan-cn.xiaomimimo.com/anthropic"),
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
            log::debug!("[provider_claude_config] extra_env override: {}={}", key, value);
            env.insert(key.clone(), value.clone());
        }
    }
}

fn build_deepseek_env(cred: &PlatformCredential) -> Result<HashMap<String, String>, String> {
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

    let models: Vec<String> = cred
        .models
        .clone()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| {
            vec![
                "deepseek-v4-pro".to_string(),
                "deepseek-v4-flash".to_string(),
            ]
        });
    let (opus, sonnet, haiku, subagent) = resolve_model_tiers(&models);

    let mut env = HashMap::from([
        ("ANTHROPIC_BASE_URL".to_string(), base_url),
        ("ANTHROPIC_AUTH_TOKEN".to_string(), api_key),
        ("ANTHROPIC_MODEL".to_string(), opus.to_string()),
        (
            "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(),
            opus.to_string(),
        ),
        (
            "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
            sonnet.to_string(),
        ),
        (
            "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
            haiku.to_string(),
        ),
        (
            "CLAUDE_CODE_SUBAGENT_MODEL".to_string(),
            subagent.to_string(),
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
    let api_key = cred
        .api_key
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{platform_id} API key is not configured"))?;
    let base_url = cred
        .base_url
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            default_base_url(platform_id)
                .unwrap_or("")
                .to_string()
        });
    if base_url.is_empty() {
        return Err(format!("{platform_id} base URL is not configured"));
    }
    let models: Vec<String> = cred
        .models
        .clone()
        .filter(|m| !m.is_empty())
        .ok_or_else(|| format!("{platform_id} model is not configured"))?;
    let (opus, sonnet, haiku, subagent) = resolve_model_tiers(&models);

    let mut env = HashMap::from([
        ("ANTHROPIC_BASE_URL".to_string(), base_url),
        ("ANTHROPIC_AUTH_TOKEN".to_string(), api_key),
        ("ANTHROPIC_MODEL".to_string(), opus.to_string()),
        (
            "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(),
            opus.to_string(),
        ),
        (
            "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
            sonnet.to_string(),
        ),
        (
            "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
            haiku.to_string(),
        ),
        (
            "CLAUDE_CODE_SUBAGENT_MODEL".to_string(),
            subagent.to_string(),
        ),
    ]);
    env.extend(stability_env_vars());
    merge_extra_env(&mut env, &cred.extra_env);
    Ok(env)
}

fn provider_config_json_from_env(env: &HashMap<String, String>) -> Value {
    let mut env_obj = Map::new();
    for (key, value) in env {
        env_obj.insert(key.clone(), Value::String(value.clone()));
    }

    json!({
        "env": env_obj,
        "permissions": {
            "defaultMode": "bypassPermissions"
        },
        "enabledPlugins": {
            "superpowers@claude-plugins-official": true
        },
        "includeCoAuthoredBy": false,
        "thinking": false,
        "skipDangerousModePermissionPrompt": true,
        "autoUpdatesChannel": "latest",
        "language": "简体中文"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PlatformCredential;

    fn cred(platform_id: &str, api_key: &str, base_url: Option<&str>, model: Option<&str>) -> PlatformCredential {
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
        assert_eq!(platform_to_provider_id("mimo-pro"), Some("mimo-pro"));
        assert_eq!(platform_to_provider_id("packy-cx2cc"), Some("packy-cx2cc"));
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
    fn builds_deepseek_env_with_defaults() {
        let env = build_deepseek_env(&cred("deepseek", "sk-deepseek", None, None)).unwrap();
        assert_eq!(env.get("ANTHROPIC_AUTH_TOKEN").map(String::as_str), Some("sk-deepseek"));
        assert_eq!(env.get("ANTHROPIC_BASE_URL").map(String::as_str), Some("https://api.deepseek.com/anthropic"));
        assert_eq!(env.get("ANTHROPIC_MODEL").map(String::as_str), Some("deepseek-v4-pro"));
        assert_eq!(
            env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").map(String::as_str),
            Some("deepseek-v4-flash")
        );
    }

    #[test]
    fn builds_deepseek_env_with_custom_model() {
        let env = build_deepseek_env(&cred("deepseek", "sk-ds", None, Some("custom-ds-model"))).unwrap();
        assert_eq!(env.get("ANTHROPIC_MODEL").map(String::as_str), Some("custom-ds-model"));
        assert_eq!(env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").map(String::as_str), Some("custom-ds-model"));
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
        assert_eq!(env.get("ANTHROPIC_MODEL").map(String::as_str), Some("ds-v4"));
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
        assert_eq!(env.get("ANTHROPIC_AUTH_TOKEN").map(String::as_str), Some("sk-qwen"));
        assert_eq!(env.get("ANTHROPIC_MODEL").map(String::as_str), Some("qwen3.5-plus"));
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://coding.dashscope.aliyuncs.com/apps/anthropic")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_SUBAGENT_MODEL").map(String::as_str),
            Some("qwen3.5-plus")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC").map(String::as_str),
            Some("1")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_EFFORT_LEVEL").map(String::as_str),
            Some("max")
        );
        assert_eq!(
            env.get("CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS").map(String::as_str),
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
        assert_eq!(env.get("ANTHROPIC_MODEL").map(String::as_str), Some("glm-5"));
    }

    #[test]
    fn builds_parameterized_env_with_tiered_models() {
        let c = PlatformCredential {
            platform_id: "kimi".to_string(),
            api_key: Some("sk-kimi".to_string()),
            base_url: Some("https://api.moonshot.cn/anthropic".to_string()),
            auth_env_var: Some("ANTHROPIC_AUTH_TOKEN".to_string()),
            name: Some("kimi".to_string()),
            models: Some(vec![
                "kimi-k2.5".to_string(),
                "kimi-k2".to_string(),
            ]),
            extra_env: None,
        };
        let env = build_parameterized_env("kimi", &c).unwrap();
        assert_eq!(env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").map(String::as_str), Some("kimi-k2.5"));
        assert_eq!(env.get("ANTHROPIC_DEFAULT_SONNET_MODEL").map(String::as_str), Some("kimi-k2.5"));
        assert_eq!(env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").map(String::as_str), Some("kimi-k2"));
        assert_eq!(env.get("CLAUDE_CODE_SUBAGENT_MODEL").map(String::as_str), Some("kimi-k2.5"));
    }

    #[test]
    fn writes_temp_config_with_run_id_in_path() {
        let _guard = crate::storage::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("OPENCOVIBE_DATA_DIR");
        std::env::set_var("OPENCOVIBE_DATA_DIR", tmp.path());

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
        )
        .unwrap();

        assert!(result
            .json_path
            .ends_with(std::path::Path::new("provider-claude-configs/session-test-run-001.json")));
        let content = fs::read_to_string(&result.json_path).unwrap();
        assert!(content.contains("ANTHROPIC_BASE_URL"));
        assert!(content.contains("qwen3.5-plus"));

        match previous {
            Some(value) => std::env::set_var("OPENCOVIBE_DATA_DIR", value),
            None => std::env::remove_var("OPENCOVIBE_DATA_DIR"),
        }
    }

    fn cred_with_extra_env(platform_id: &str, extra_env: Option<HashMap<String, String>>) -> PlatformCredential {
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
            ("ANTHROPIC_BASE_URL".to_string(), "https://base.example.com".to_string()),
        ]);
        let extra_env = HashMap::from([
            ("ANTHROPIC_MODEL".to_string(), "override-model".to_string()),
        ]);
        merge_extra_env(&mut env, &Some(extra_env));
        assert_eq!(env.get("ANTHROPIC_MODEL").map(String::as_str), Some("override-model"));
        assert_eq!(env.get("ANTHROPIC_BASE_URL").map(String::as_str), Some("https://base.example.com"));
    }

    #[test]
    fn merge_extra_env_non_whitelisted_keys_ignored() {
        let mut env = HashMap::from([
            ("ANTHROPIC_BASE_URL".to_string(), "https://base.example.com".to_string()),
        ]);
        let extra_env = HashMap::from([
            ("ANTHROPIC_BASE_URL".to_string(), "https://evil.example.com".to_string()),
            ("SOME_INTERNAL_KEY".to_string(), "bad".to_string()),
        ]);
        merge_extra_env(&mut env, &Some(extra_env));
        assert_eq!(env.get("ANTHROPIC_BASE_URL").map(String::as_str), Some("https://base.example.com"));
        assert!(!env.contains_key("SOME_INTERNAL_KEY"));
    }

    #[test]
    fn merge_extra_env_empty_values_filtered() {
        let mut env = HashMap::from([
            ("ANTHROPIC_MODEL".to_string(), "original".to_string()),
        ]);
        let extra_env = HashMap::from([
            ("ANTHROPIC_MODEL".to_string(), "  ".to_string()),
        ]);
        merge_extra_env(&mut env, &Some(extra_env));
        assert_eq!(env.get("ANTHROPIC_MODEL").map(String::as_str), Some("original"));
    }

    #[test]
    fn merge_extra_env_none_is_noop() {
        let mut env = HashMap::from([
            ("ANTHROPIC_MODEL".to_string(), "original".to_string()),
        ]);
        merge_extra_env(&mut env, &None);
        assert_eq!(env.get("ANTHROPIC_MODEL").map(String::as_str), Some("original"));
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
        assert_eq!(env.get("ANTHROPIC_MODEL").map(String::as_str), Some("custom-ds-model"));
        assert_eq!(env.get("CLAUDE_CODE_EFFORT_LEVEL").map(String::as_str), Some("low"));
        assert!(!env.contains_key("EVIL_KEY"));
    }

    #[test]
    fn build_parameterized_env_with_extra_env_overrides() {
        let c = cred_with_extra_env(
            "kimi",
            Some(HashMap::from([
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(), "custom-haiku".to_string()),
            ])),
        );
        let env = build_parameterized_env("kimi", &c).unwrap();
        assert_eq!(env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").map(String::as_str), Some("custom-haiku"));
    }
}
