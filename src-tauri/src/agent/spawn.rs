use crate::agent::adapter::{self, AdapterSettings};

fn native_command(default_command: &str, settings: &AdapterSettings) -> String {
    settings
        .command_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(default_command)
        .to_string()
}

fn build_codex_base_args(settings: &AdapterSettings) -> Vec<String> {
    let plan_mode = settings.permission_mode.as_deref() == Some("plan");
    let mut args: Vec<String> = vec!["exec".to_string()];
    args.push("--json".to_string());
    args.push("--skip-git-repo-check".to_string());
    if !plan_mode {
        args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
    }
    if let Some(ref m) = settings.model {
        if !m.is_empty() {
            args.push("--model".to_string());
            args.push(m.to_string());
        }
    }
    for dir in &settings.add_dirs {
        args.push("--add-dir".to_string());
        args.push(dir.to_string());
    }
    if settings.no_session_persistence {
        args.push("--ephemeral".to_string());
    }
    append_extra_args_without_controlled_flags(
        &mut args,
        &settings.extra_args,
        &[
            "exec",
            "--json",
            "--skip-git-repo-check",
            "--dangerously-bypass-approvals-and-sandbox",
            "--no-alt-screen",
            "--yolo",
        ],
        &[],
    );
    args
}

fn append_extra_args_without_controlled_flags(
    args: &mut Vec<String>,
    extra_args: &[String],
    singleton_flags: &[&str],
    value_flags: &[&str],
) {
    let mut skip_next = false;
    for arg in extra_args {
        if skip_next {
            skip_next = false;
            continue;
        }
        let trimmed = arg.trim();
        if singleton_flags.contains(&trimmed) {
            continue;
        }
        if value_flags.contains(&trimmed) {
            skip_next = true;
            continue;
        }
        if value_flags.iter().any(|flag| {
            trimmed
                .strip_prefix(*flag)
                .is_some_and(|rest| rest.starts_with('='))
        }) {
            continue;
        }
        args.push(arg.clone());
    }
}

/// Build the command + args for a given agent (pipe-exec mode, not stream session)
pub fn build_agent_command(
    agent: &str,
    prompt: &str,
    settings: &AdapterSettings,
    print: bool,
) -> Result<(String, Vec<String>), String> {
    log::debug!(
        "[spawn] build_agent_command: agent={}, print={}, model={:?}, perm={:?}, allowed={}, disallowed={}",
        agent, print, settings.model, settings.permission_mode, settings.allowed_tools.len(), settings.disallowed_tools.len()
    );
    match agent {
        "claude" => {
            let mut args: Vec<String> = vec![];
            if print {
                args.push("--print".to_string());
            }

            // Use shared helper for all settings flags
            args.extend(adapter::build_settings_args(settings, print));

            if !prompt.is_empty() {
                args.push(prompt.to_string());
            }
            log::debug!("[spawn] claude command: claude {}", args.join(" "));
            Ok(("claude".to_string(), args))
        }
        "codex" => {
            let mut args = build_codex_base_args(settings);
            if !prompt.is_empty() {
                args.push(prompt.to_string());
            }
            log::debug!("[spawn] codex command: codex {}", args.join(" "));
            Ok((native_command("codex", settings), args))
        }
        _ => Err(format!(
            "Unsupported agent: {}. Supported: claude, codex",
            agent
        )),
    }
}

pub fn build_agent_resume_command(
    agent: &str,
    prompt: &str,
    settings: &AdapterSettings,
    thread_id: &str,
) -> Result<(String, Vec<String>), String> {
    match agent {
        "codex" => {
            let mut args = build_codex_base_args(settings);
            let exec_pos = args.iter().position(|a| a == "exec").unwrap_or(0);
            args.insert(exec_pos + 1, "resume".to_string());
            args.insert(exec_pos + 2, thread_id.to_string());
            if !prompt.is_empty() {
                args.push(prompt.to_string());
            }
            Ok((native_command("codex", settings), args))
        }
        _ => Err(format!("Resume latest is unsupported for agent: {}", agent)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(model: Option<&str>) -> AdapterSettings {
        AdapterSettings {
            model: model.map(|m| m.to_string()),
            allowed_tools: vec![],
            disallowed_tools: vec![],
            permission_mode: None,
            append_system_prompt: None,
            max_budget_usd: None,
            fallback_model: None,
            system_prompt: None,
            tool_set: None,
            add_dirs: vec![],
            json_schema: None,
            include_partial_messages: true,
            cli_debug: None,
            no_session_persistence: false,
            max_turns: None,
            effort: None,
            betas: vec![],
            agents_json: None,
            command_path: None,
            extra_args: vec![],
            yolo_mode: None,
        }
    }

    #[test]
    fn builds_codex_native_bypass_and_add_dir_args() {
        let mut s = settings(Some("gpt-5.5"));
        s.add_dirs = vec!["D:/shared".to_string()];
        s.yolo_mode = Some(false);

        let (command, args) =
            build_agent_command("codex", "Fix it", &s, true).expect("codex command");

        assert_eq!(command, "codex");
        assert!(args.contains(&"exec".to_string()));
        assert!(args.contains(&"--json".to_string()));
        assert!(args.contains(&"--skip-git-repo-check".to_string()));
        assert!(args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
        assert!(!args.contains(&"--no-alt-screen".to_string()));
        assert!(args.windows(2).any(|w| w == ["--add-dir", "D:/shared"]));
        assert!(args.windows(2).any(|w| w == ["--model", "gpt-5.5"]));
        assert_eq!(args.last().map(String::as_str), Some("Fix it"));
    }

    #[test]
    fn codex_native_bypass_flag_is_not_duplicated_from_extra_args() {
        let mut s = settings(None);
        s.extra_args = vec![
            "--dangerously-bypass-approvals-and-sandbox".to_string(),
            "--no-alt-screen".to_string(),
            "--yolo".to_string(),
            "--json".to_string(),
            "--skip-git-repo-check".to_string(),
            "--search".to_string(),
        ];

        let (_command, args) =
            build_agent_command("codex", "Fix it", &s, true).expect("codex command");

        assert_eq!(
            args.iter()
                .filter(|arg| arg.as_str() == "--dangerously-bypass-approvals-and-sandbox")
                .count(),
            1
        );
        assert!(!args.contains(&"--no-alt-screen".to_string()));
        assert!(!args.contains(&"--yolo".to_string()));
        assert_eq!(args.iter().filter(|a| a.as_str() == "--json").count(), 1);
        assert_eq!(
            args.iter()
                .filter(|a| a.as_str() == "--skip-git-repo-check")
                .count(),
            1
        );
        assert!(args.contains(&"--search".to_string()));
    }

    #[test]
    fn native_agents_force_elevated_permission_policy() {
        let mut s = settings(None);
        s.permission_mode = Some("auto_read".to_string());
        s.yolo_mode = Some(false);

        let (_codex_command, codex_args) =
            build_agent_command("codex", "Fix it", &s, true).expect("codex command");
        assert!(codex_args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
    }

    #[test]
    fn builds_codex_resume_with_thread_id() {
        let (command, args) = build_agent_resume_command(
            "codex",
            "Continue work",
            &settings(None),
            "019e4113-8979-7000-aaaa-bbbbbbbbbbbb",
        )
        .expect("codex resume command");

        assert_eq!(command, "codex");
        assert!(args.contains(&"exec".to_string()));
        assert!(args.contains(&"resume".to_string()));
        assert!(args.contains(&"--json".to_string()));
        assert!(args.contains(&"019e4113-8979-7000-aaaa-bbbbbbbbbbbb".to_string()));
        assert!(!args.contains(&"--last".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("Continue work"));
    }

    #[test]
    fn codex_plan_mode_omits_bypass_flag() {
        let mut s = settings(Some("gpt-5.5"));
        s.permission_mode = Some("plan".to_string());

        let (_command, args) =
            build_agent_command("codex", "Analyze this", &s, true).expect("codex command");

        assert!(!args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
        assert!(args.contains(&"--json".to_string()));
        assert!(args.windows(2).any(|w| w == ["--model", "gpt-5.5"]));
    }
}
