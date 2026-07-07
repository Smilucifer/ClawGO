//! 行业敏感度:全局 regime × 行业 → positive/negative/neutral。
//! 单次运行进程内存缓存(§8.2-K):key=行业|signal;signal 变更清空,不持久化。

use std::collections::HashMap;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub const SENSITIVITY_PROMPT: &str = r#"你是宏观敏感度分析助手。给定当前 A 股全局信号与某行业,判断该行业在此环境下的敏感度。

全局信号: {{signal}}
所属行业: {{industry}}

只输出两行,中文,不解释多余内容:
敏感度: positive | negative | neutral
敏感度原因: <一句话≤20字>"#;

/// 缓存键:行业|signal(纯函数,可测)。
pub fn cache_key(industry: &str, signal: &str) -> String {
    format!("{industry}|{signal}")
}

/// 进程内存缓存:(当前 signal, {key → (sensitivity, reason)})。
/// signal 变更(regime 切换)时整表清空。
#[allow(clippy::type_complexity)]
static CACHE: Mutex<Option<(String, HashMap<String, (Option<String>, Option<String>)>)>> =
    Mutex::new(None);

/// 分析行业敏感度。命中缓存直接返回;signal 变更(regime 切换)清空整表。
pub async fn analyze(
    industry: &str,
    signal: &str,
    settings_path: Option<&std::path::Path>,
    cancel: Option<&CancellationToken>,
) -> (Option<String>, Option<String>) {
    if industry.is_empty() || industry == "N/A" {
        return (None, None);
    }
    let key = cache_key(industry, signal);
    {
        let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        match &*guard {
            Some((sig, map)) if sig == signal => {
                if let Some(hit) = map.get(&key) {
                    return hit.clone();
                }
            }
            _ => *guard = Some((signal.to_string(), HashMap::new())),
        }
    }
    let cli = match super::cli_executor::CliCommitteeExecutor::global() {
        Some(c) => c,
        None => return (None, None),
    };
    let sys = SENSITIVITY_PROMPT
        .replace("{{signal}}", signal)
        .replace("{{industry}}", industry);
    let raw = match cli
        .run_role(&sys, "请判断该行业敏感度。", 60, settings_path, cancel)
        .await
    {
        Ok(t) => t,
        Err(_) => return (None, None),
    };
    let parsed =
        super::parser::parse_role_output(super::roles::CommitteeRole::Macro, &raw, false);
    let result = (parsed.sensitivity.clone(), parsed.sensitivity_reason.clone());
    if let Some((sig, map)) = &mut *CACHE.lock().unwrap_or_else(|e| e.into_inner()) {
        if sig == signal {
            map.insert(key, result.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_cache_key() {
        assert_eq!(cache_key("白酒", "risk_on"), "白酒|risk_on");
        assert_ne!(cache_key("白酒", "risk_on"), cache_key("白酒", "risk_off"));
    }
}
