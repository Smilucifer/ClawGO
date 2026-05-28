pub mod artifacts;
pub mod changelog;
pub mod characters;
pub mod claude_usage;
pub mod cli_config;
pub mod cli_sessions;
pub mod community_skills;
pub mod events;
pub mod favorites;
pub mod invest;
pub mod managed_apps;
pub mod mcp_registry;
pub mod memos;
pub mod memory_store;
pub mod plugins;
pub mod prompt_index;
pub mod group_chats;
pub mod run_index;
pub mod runs;
pub mod settings;
pub mod teams;

use std::path::PathBuf;

#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn data_dir() -> PathBuf {
    if let Ok(path) = std::env::var("CLAW_GO_DATA_DIR") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    let home = dirs_next().expect("Could not determine home directory");
    home.join(".claw-go")
}

pub fn runs_dir() -> PathBuf {
    data_dir().join("runs")
}

pub fn run_dir(run_id: &str) -> PathBuf {
    runs_dir().join(run_id)
}

/// Resolve the user's home directory reliably.
/// Primary: `getpwuid()` system call (works even when `$HOME` is unset,
/// e.g. GUI apps launched from Finder/Dock on macOS 26+).
/// Fallback: `$HOME` (Unix) or `$USERPROFILE` (Windows).
pub fn home_dir() -> Option<String> {
    #[cfg(unix)]
    {
        let pwd_home = unsafe {
            let uid = libc::getuid();
            let pw = libc::getpwuid(uid);
            if !pw.is_null() {
                let dir = (*pw).pw_dir;
                if !dir.is_null() {
                    Some(std::ffi::CStr::from_ptr(dir).to_string_lossy().into_owned())
                } else {
                    None
                }
            } else {
                None
            }
        };
        if pwd_home.is_some() {
            return pwd_home;
        }
        std::env::var("HOME").ok()
    }
    #[cfg(not(unix))]
    {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .or_else(|_| {
                let drive = std::env::var("HOMEDRIVE").unwrap_or_default();
                let path = std::env::var("HOMEPATH").unwrap_or_default();
                if !drive.is_empty() && !path.is_empty() {
                    Ok(format!("{}{}", drive, path))
                } else {
                    Err(std::env::VarError::NotPresent)
                }
            })
            .ok()
    }
}

pub(crate) fn dirs_next() -> Option<PathBuf> {
    home_dir().map(PathBuf::from)
}

pub fn ensure_dir(path: &std::path::Path) -> std::io::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }

    // Restrict directory permissions — data dir may contain sensitive data
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_uses_claw_go_data_dir_override() {
        let _guard = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("CLAW_GO_DATA_DIR");

        std::env::set_var("CLAW_GO_DATA_DIR", tmp.path());
        assert_eq!(data_dir(), tmp.path());

        match previous {
            Some(value) => std::env::set_var("CLAW_GO_DATA_DIR", value),
            None => std::env::remove_var("CLAW_GO_DATA_DIR"),
        }
    }

    #[test]
    fn data_dir_ignores_empty_claw_go_data_dir_override() {
        let _guard = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var_os("CLAW_GO_DATA_DIR");

        std::env::set_var("CLAW_GO_DATA_DIR", "");
        assert_eq!(
            data_dir(),
            dirs_next()
                .expect("Could not determine home directory")
                .join(".claw-go")
        );

        match previous {
            Some(value) => std::env::set_var("CLAW_GO_DATA_DIR", value),
            None => std::env::remove_var("CLAW_GO_DATA_DIR"),
        }
    }
}
