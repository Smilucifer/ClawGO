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
    let mut args: Vec<String> = vec![
        "--dangerously-bypass-approvals-and-sandbox".to_string(),
        "--no-alt-screen".to_string(),
    ];
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
            "--dangerously-bypass-approvals-and-sandbox",
            "--no-alt-screen",
            "--yolo",
        ],
        &[],
    );
    args
}

fn build_gemini_base_args(settings: &AdapterSettings) -> Vec<String> {
    let mut args: Vec<String> = vec![];
    if let Some(ref m) = settings.model {
        if !m.is_empty() {
            args.push("--model".to_string());
            args.push(m.to_string());
        }
    }
    args.push("--approval-mode".to_string());
    args.push("yolo".to_string());
    args.push("--skip-trust".to_string());
    for dir in &settings.add_dirs {
        args.push("--include-directories".to_string());
        args.push(dir.to_string());
    }
    append_extra_args_without_controlled_flags(
        &mut args,
        &settings.extra_args,
        &["--skip-trust", "--yolo"],
        &["--approval-mode"],
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
        if singleton_flags.iter().any(|flag| trimmed == *flag) {
            continue;
        }
        if value_flags.iter().any(|flag| trimmed == *flag) {
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
        "gemini" => {
            let mut args = build_gemini_base_args(settings);
            if !prompt.is_empty() {
                args.push("--prompt-interactive".to_string());
                args.push(prompt.to_string());
            }
            log::debug!("[spawn] gemini command: gemini {}", args.join(" "));
            Ok((native_command("gemini", settings), args))
        }
        _ => Err(format!(
            "Unsupported agent: {}. Supported: claude, codex, gemini",
            agent
        )),
    }
}

pub fn build_agent_resume_command(
    agent: &str,
    prompt: &str,
    settings: &AdapterSettings,
) -> Result<(String, Vec<String>), String> {
    match agent {
        "codex" => {
            let mut args = build_codex_base_args(settings);
            args.push("resume".to_string());
            args.push("--last".to_string());
            if !prompt.is_empty() {
                args.push(prompt.to_string());
            }
            Ok((native_command("codex", settings), args))
        }
        "gemini" => {
            let mut args = build_gemini_base_args(settings);
            args.push("--resume".to_string());
            args.push("latest".to_string());
            if !prompt.is_empty() {
                args.push("--prompt-interactive".to_string());
                args.push(prompt.to_string());
            }
            Ok((native_command("gemini", settings), args))
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
    fn builds_gemini_native_interactive_command() {
        let (command, args) = build_agent_command(
            "gemini",
            "Explain this repo",
            &settings(Some("gemini-2.5-pro")),
            true,
        )
        .unwrap();

        assert_eq!(command, "gemini");
        assert_eq!(
            args,
            vec![
                "--model",
                "gemini-2.5-pro",
                "--approval-mode",
                "yolo",
                "--skip-trust",
                "--prompt-interactive",
                "Explain this repo"
            ]
        );
    }

    #[test]
    fn builds_codex_native_bypass_and_add_dir_args() {
        let mut s = settings(Some("gpt-5.5"));
        s.add_dirs = vec!["D:/shared".to_string()];
        s.yolo_mode = Some(false);

        let (command, args) =
            build_agent_command("codex", "Fix it", &s, true).expect("codex command");

        assert_eq!(command, "codex");
        assert!(!args.contains(&"exec".to_string()));
        assert!(args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
        assert!(!args.contains(&"--json".to_string()));
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
        assert_eq!(
            args.iter()
                .filter(|arg| arg.as_str() == "--no-alt-screen")
                .count(),
            1
        );
        assert!(!args.contains(&"--yolo".to_string()));
        assert!(args.contains(&"--search".to_string()));
    }

    #[test]
    fn builds_gemini_yolo_and_include_directories_args() {
        let mut s = settings(Some("gemini-2.5-pro"));
        s.add_dirs = vec!["D:/shared".to_string()];
        s.yolo_mode = Some(false);

        let (_command, args) =
            build_agent_command("gemini", "Explain this repo", &s, true).expect("gemini command");

        assert!(args.windows(2).any(|w| w == ["--approval-mode", "yolo"]));
        assert!(args.contains(&"--skip-trust".to_string()));
        assert!(args
            .windows(2)
            .any(|w| w == ["--include-directories", "D:/shared"]));
        assert_eq!(args.last().map(String::as_str), Some("Explain this repo"));
    }

    #[test]
    fn native_agents_force_elevated_permission_policy() {
        let mut s = settings(None);
        s.permission_mode = Some("auto_read".to_string());
        s.yolo_mode = Some(false);

        let (_codex_command, codex_args) =
            build_agent_command("codex", "Fix it", &s, true).expect("codex command");
        assert!(codex_args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));

        let (_gemini_command, gemini_args) =
            build_agent_command("gemini", "Fix it", &s, true).expect("gemini command");
        assert!(gemini_args
            .windows(2)
            .any(|w| w == ["--approval-mode", "yolo"]));
        assert!(gemini_args.contains(&"--skip-trust".to_string()));
    }

    #[test]
    fn builds_codex_resume_latest_without_exec() {
        let (command, args) = build_agent_resume_command("codex", "Continue work", &settings(None))
            .expect("codex resume command");

        assert_eq!(command, "codex");
        assert!(!args.contains(&"exec".to_string()));
        assert!(args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
        assert!(args.windows(2).any(|w| w == ["resume", "--last"]));
        assert_eq!(args.last().map(String::as_str), Some("Continue work"));
    }

    #[test]
    fn builds_gemini_resume_latest_with_prompt_interactive() {
        let (command, args) =
            build_agent_resume_command("gemini", "Continue work", &settings(None))
                .expect("gemini resume command");

        assert_eq!(command, "gemini");
        assert!(args.windows(2).any(|w| w == ["--approval-mode", "yolo"]));
        assert!(args.contains(&"--skip-trust".to_string()));
        assert!(args.windows(2).any(|w| w == ["--resume", "latest"]));
        assert!(args
            .windows(2)
            .any(|w| w == ["--prompt-interactive", "Continue work"]));
    }
}
