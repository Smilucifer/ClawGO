use crate::models::WindowsMsvcEnvMode;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

const WINDOWS_PATH_SEPARATOR: char = ';';
const PATH_LIKE_WARNING_THRESHOLD: usize = 32_000;

const MSVC_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "INCLUDE",
    "LIB",
    "LIBPATH",
    "VCTOOLSINSTALLDIR",
    "VCINSTALLDIR",
    "VSINSTALLDIR",
    "WINDOWSSDKDIR",
    "WINDOWSSDKVERSION",
    "UNIVERSALCRTSDKDIR",
    "UCRTVERSION",
    "DEVENVDIR",
    "FRAMEWORKDIR",
    "FRAMEWORKDIR64",
    "FRAMEWORKVERSION",
    "FRAMEWORKVERSION64",
    "__VSCMD_PREINIT_PATH",
    "VSCMD_ARG_TGT_ARCH",
    "VSCMD_ARG_HOST_ARCH",
    "VSCMD_VER",
];

const NATIVE_PACKAGE_HINTS: &[&str] = &[
    "node-gyp",
    "@tauri-apps/cli",
    "tauri",
    "electron",
    "sharp",
    "better-sqlite3",
    "sqlite3",
    "canvas",
    "node-sass",
    "esbuild",
];

const RUST_NATIVE_DEP_HINTS: &[&str] =
    &["cc", "cmake", "bindgen", "windows", "windows-sys", "winapi"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsvcEnvWarning {
    pub code: String,
    pub message: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathMergeResult {
    pub value: String,
    pub warnings: Vec<MsvcEnvWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsvcEnvDecision {
    Skip {
        reason: MsvcEnvSkipReason,
    },
    Inject {
        env: HashMap<String, String>,
        source: MsvcEnvSource,
    },
    Warn {
        message: String,
        next_action: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsvcEnvSkipReason {
    NonWindows,
    RemoteSession,
    DisabledByUser,
    ProjectDoesNotNeedNativeToolchain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsvcEnvSource {
    pub installation_path: PathBuf,
    pub arch: String,
    pub host_arch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MsvcEnvCacheKey {
    pub installation_path: PathBuf,
    pub arch: String,
    pub host_arch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnEnvPlan {
    pub path_override: Option<String>,
    pub msvc_env: HashMap<String, String>,
    pub warnings: Vec<MsvcEnvWarning>,
    pub status: MsvcEnvStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsvcEnvStatus {
    Skipped(MsvcEnvSkipReason),
    Injected(MsvcEnvSource),
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnPathPolicy {
    InheritUnlessInjected,
    AlwaysUseAugmentedPath,
}

static MSVC_ENV_CACHE: Lazy<Mutex<HashMap<MsvcEnvCacheKey, HashMap<String, String>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub trait MsvcEnvResolverIo {
    fn vswhere_path(&self) -> Option<PathBuf>;
    fn run_vswhere(&self, vswhere_path: &Path) -> Result<String, String>;
    fn path_exists(&self, path: &Path) -> bool;
    fn run_vsdevcmd(
        &self,
        vsdevcmd_path: &Path,
        arch: &str,
        host_arch: &str,
    ) -> Result<String, String>;
}

pub struct DefaultMsvcEnvResolverIo;

impl MsvcEnvResolverIo for DefaultMsvcEnvResolverIo {
    fn vswhere_path(&self) -> Option<PathBuf> {
        default_vswhere_path()
    }

    fn run_vswhere(&self, vswhere_path: &Path) -> Result<String, String> {
        let output = Command::new(vswhere_path)
            .args([
                "-latest",
                "-products",
                "*",
                "-requires",
                "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                "-property",
                "installationPath",
            ])
            .output()
            .map_err(|e| format!("Failed to run vswhere: {e}"))?;

        if !output.status.success() {
            return Err(format!("vswhere exited with status {}", output.status));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn run_vsdevcmd(
        &self,
        vsdevcmd_path: &Path,
        arch: &str,
        host_arch: &str,
    ) -> Result<String, String> {
        let command = format!(
            "\"\"{}\" -arch={} -host_arch={} >nul && set\"",
            vsdevcmd_path.display(),
            arch,
            host_arch
        );
        let output = Command::new("cmd")
            .args(["/d", "/s", "/c", &command])
            .output()
            .map_err(|e| format!("Failed to run VsDevCmd.bat: {e}"))?;

        if !output.status.success() {
            return Err(format!("VsDevCmd.bat exited with status {}", output.status));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[cfg(windows)]
fn default_vswhere_path() -> Option<PathBuf> {
    let root = std::env::var_os("ProgramFiles(x86)")?;
    Some(
        PathBuf::from(root)
            .join("Microsoft Visual Studio")
            .join("Installer")
            .join("vswhere.exe"),
    )
    .filter(|path| path.exists())
}

#[cfg(not(windows))]
fn default_vswhere_path() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
pub fn resolve_default_windows_msvc_env(arch: &str, host_arch: &str) -> MsvcEnvDecision {
    resolve_windows_msvc_env(&DefaultMsvcEnvResolverIo, arch, host_arch)
}

#[cfg(not(windows))]
pub fn resolve_default_windows_msvc_env(_arch: &str, _host_arch: &str) -> MsvcEnvDecision {
    MsvcEnvDecision::Skip {
        reason: MsvcEnvSkipReason::NonWindows,
    }
}

pub fn resolve_windows_msvc_env<R: MsvcEnvResolverIo>(
    io: &R,
    arch: &str,
    host_arch: &str,
) -> MsvcEnvDecision {
    let Some(vswhere_path) = io.vswhere_path() else {
        return warn(
            "Visual Studio Installer vswhere.exe was not found.",
            "Install Visual Studio Build Tools or Visual Studio through the Visual Studio Installer.",
        );
    };

    let installation_output =
        match io.run_vswhere(&vswhere_path) {
            Ok(output) => output,
            Err(e) => return warn(
                &format!("Failed to query Visual Studio installation: {e}"),
                "Open Visual Studio Installer and verify Visual Studio Build Tools are installed.",
            ),
        };

    let Some(installation_path) = installation_output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
    else {
        return warn(
            "Visual Studio VC tools were not found.",
            "Install the Visual Studio Installer workload \"Desktop development with C++\".",
        );
    };

    let vsdevcmd_path = installation_path
        .join("Common7")
        .join("Tools")
        .join("VsDevCmd.bat");
    if !io.path_exists(&vsdevcmd_path) {
        return warn(
            "VsDevCmd.bat was not found in the selected Visual Studio installation.",
            "Repair Visual Studio Build Tools or install the \"Desktop development with C++\" workload.",
        );
    }

    let cache_key = MsvcEnvCacheKey {
        installation_path: installation_path.clone(),
        arch: arch.to_string(),
        host_arch: host_arch.to_string(),
    };
    let source = MsvcEnvSource {
        installation_path,
        arch: arch.to_string(),
        host_arch: host_arch.to_string(),
    };

    if let Some(env) = MSVC_ENV_CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(&cache_key).cloned())
    {
        return MsvcEnvDecision::Inject { env, source };
    }

    let raw_output = match io.run_vsdevcmd(&vsdevcmd_path, arch, host_arch) {
        Ok(output) => output,
        Err(e) => {
            return warn(
                &format!("Failed to derive Visual Studio developer environment: {e}"),
                "Open Visual Studio Installer and repair the C++ build tools installation.",
            )
        }
    };

    let env = filter_msvc_env(parse_set_output(&raw_output));
    if let Ok(mut cache) = MSVC_ENV_CACHE.lock() {
        cache.insert(cache_key, env.clone());
    }

    MsvcEnvDecision::Inject { env, source }
}

fn warn(message: &str, next_action: &str) -> MsvcEnvDecision {
    MsvcEnvDecision::Warn {
        message: message.to_string(),
        next_action: next_action.to_string(),
    }
}

#[cfg(test)]
pub fn clear_msvc_env_cache_for_tests() {
    MSVC_ENV_CACHE.lock().unwrap().clear();
}

pub fn parse_set_output(output: &str) -> HashMap<String, String> {
    output
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            if key.trim().is_empty() {
                return None;
            }
            Some((key.to_string(), value.to_string()))
        })
        .collect()
}

pub fn filter_msvc_env(raw: HashMap<String, String>) -> HashMap<String, String> {
    raw.into_iter()
        .filter(|(key, _)| {
            let normalized = key.to_ascii_uppercase();
            MSVC_ENV_ALLOWLIST.contains(&normalized.as_str())
        })
        .collect()
}

pub fn project_needs_msvc(cwd: &Path) -> bool {
    if cwd.join("src-tauri").is_dir() || cwd.join("binding.gyp").is_file() {
        return true;
    }

    if package_json_has_native_hint(&cwd.join("package.json")) {
        return true;
    }

    rust_project_has_native_marker(cwd)
}

pub fn merge_path_like(base: &str, injected: &str) -> PathMergeResult {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();

    for value in [injected, base] {
        for entry in value.split(WINDOWS_PATH_SEPARATOR) {
            let trimmed = entry.trim();
            if trimmed.is_empty() {
                continue;
            }
            let key = trimmed.to_ascii_lowercase();
            if seen.insert(key) {
                entries.push(trimmed.to_string());
            }
        }
    }

    let value = entries.join(&WINDOWS_PATH_SEPARATOR.to_string());
    let warnings = if value.len() > PATH_LIKE_WARNING_THRESHOLD {
        vec![MsvcEnvWarning {
            code: "path_like_env_too_long".to_string(),
            message: "Merged build environment variable is unusually long and may fail on older Windows toolchains.".to_string(),
            next_action: "Remove duplicate or obsolete PATH-like entries from provider and user environment settings.".to_string(),
        }]
    } else {
        Vec::new()
    };

    PathMergeResult { value, warnings }
}

pub fn build_spawn_env_plan(
    cwd: &Path,
    is_remote: bool,
    mode: WindowsMsvcEnvMode,
    path_policy: SpawnPathPolicy,
    base_path: Option<&str>,
    decision: MsvcEnvDecision,
) -> SpawnEnvPlan {
    if is_remote {
        return skipped_plan(
            MsvcEnvSkipReason::RemoteSession,
            SpawnPathPolicy::InheritUnlessInjected,
            base_path,
        );
    }

    if mode == WindowsMsvcEnvMode::Off {
        return skipped_plan(MsvcEnvSkipReason::DisabledByUser, path_policy, base_path);
    }

    if mode == WindowsMsvcEnvMode::Auto && !project_needs_msvc(cwd) {
        return skipped_plan(
            MsvcEnvSkipReason::ProjectDoesNotNeedNativeToolchain,
            path_policy,
            base_path,
        );
    }

    match decision {
        MsvcEnvDecision::Skip { reason } => skipped_plan(reason, path_policy, base_path),
        MsvcEnvDecision::Warn {
            message,
            next_action,
        } => SpawnEnvPlan {
            path_override: path_override_for_policy(path_policy, base_path),
            msvc_env: HashMap::new(),
            warnings: vec![MsvcEnvWarning {
                code: "msvc_env_unavailable".to_string(),
                message,
                next_action,
            }],
            status: MsvcEnvStatus::Warning,
        },
        MsvcEnvDecision::Inject { env, source } => {
            injected_plan(env, source, path_policy, base_path)
        }
    }
}

pub fn merge_extra_env_into_spawn_env_plan(
    plan: &mut SpawnEnvPlan,
    extra_env: &HashMap<String, String>,
) {
    for (key, value) in extra_env {
        if key.eq_ignore_ascii_case("PATH") {
            let planned_path = plan.path_override.take().unwrap_or_default();
            let result = merge_path_like(&planned_path, value);
            plan.path_override = Some(result.value);
            plan.warnings.extend(result.warnings);
        } else if is_protected_path_like_key(key) {
            let canonical = key.to_ascii_uppercase();
            let planned =
                remove_env_key_case_insensitive(&mut plan.msvc_env, key).unwrap_or_default();
            let result = merge_path_like(&planned, value);
            plan.msvc_env.insert(canonical, result.value);
            plan.warnings.extend(result.warnings);
        } else {
            plan.msvc_env.insert(key.clone(), value.clone());
        }
    }
}

pub fn resolve_spawn_env_plan(
    cwd: &Path,
    is_remote: bool,
    mode: WindowsMsvcEnvMode,
    path_policy: SpawnPathPolicy,
    base_path: Option<&str>,
) -> SpawnEnvPlan {
    let decision = if is_remote {
        MsvcEnvDecision::Skip {
            reason: MsvcEnvSkipReason::RemoteSession,
        }
    } else if mode == WindowsMsvcEnvMode::Off {
        MsvcEnvDecision::Skip {
            reason: MsvcEnvSkipReason::DisabledByUser,
        }
    } else if mode == WindowsMsvcEnvMode::Auto && !project_needs_msvc(cwd) {
        MsvcEnvDecision::Skip {
            reason: MsvcEnvSkipReason::ProjectDoesNotNeedNativeToolchain,
        }
    } else {
        match default_arch_pair() {
            Some((arch, host_arch)) => resolve_default_windows_msvc_env(arch, host_arch),
            None => MsvcEnvDecision::Warn {
                message: "Current Windows architecture is not supported by the MSVC environment resolver.".to_string(),
                next_action: "Use mode off or run OpenCovibe from a matching Developer PowerShell for this architecture.".to_string(),
            },
        }
    };

    build_spawn_env_plan(cwd, is_remote, mode, path_policy, base_path, decision)
}

fn injected_plan(
    mut env: HashMap<String, String>,
    source: MsvcEnvSource,
    path_policy: SpawnPathPolicy,
    base_path: Option<&str>,
) -> SpawnEnvPlan {
    let path_override = match remove_env_key_case_insensitive(&mut env, "PATH") {
        Some(msvc_path) => {
            let base = base_path.unwrap_or_default();
            let result = merge_path_like(base, &msvc_path);
            Some((result.value, result.warnings))
        }
        None => path_override_for_policy(path_policy, base_path).map(|path| (path, Vec::new())),
    };

    let (path_override, warnings) = match path_override {
        Some((path, warnings)) => (Some(path), warnings),
        None => (None, Vec::new()),
    };

    SpawnEnvPlan {
        path_override,
        msvc_env: env,
        warnings,
        status: MsvcEnvStatus::Injected(source),
    }
}

fn skipped_plan(
    reason: MsvcEnvSkipReason,
    path_policy: SpawnPathPolicy,
    base_path: Option<&str>,
) -> SpawnEnvPlan {
    SpawnEnvPlan {
        path_override: path_override_for_policy(path_policy, base_path),
        msvc_env: HashMap::new(),
        warnings: Vec::new(),
        status: MsvcEnvStatus::Skipped(reason),
    }
}

fn path_override_for_policy(policy: SpawnPathPolicy, base_path: Option<&str>) -> Option<String> {
    match policy {
        SpawnPathPolicy::AlwaysUseAugmentedPath => base_path.map(str::to_string),
        SpawnPathPolicy::InheritUnlessInjected => None,
    }
}

fn is_protected_path_like_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("INCLUDE")
        || key.eq_ignore_ascii_case("LIB")
        || key.eq_ignore_ascii_case("LIBPATH")
}

fn remove_env_key_case_insensitive(
    env: &mut HashMap<String, String>,
    target: &str,
) -> Option<String> {
    let key = env
        .keys()
        .find(|key| key.eq_ignore_ascii_case(target))
        .cloned()?;
    env.remove(&key)
}

fn default_arch_pair() -> Option<(&'static str, &'static str)> {
    match std::env::consts::ARCH {
        "x86_64" => Some(("x64", "x64")),
        "aarch64" => Some(("arm64", "arm64")),
        _ => None,
    }
}

fn package_json_has_native_hint(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };

    ["dependencies", "devDependencies", "optionalDependencies"]
        .into_iter()
        .filter_map(|section| root.get(section).and_then(|value| value.as_object()))
        .flat_map(|deps| deps.keys())
        .any(|name| NATIVE_PACKAGE_HINTS.contains(&name.as_str()))
}

fn rust_project_has_native_marker(cwd: &Path) -> bool {
    let cargo_toml = cwd.join("Cargo.toml");
    if !cargo_toml.is_file() {
        return false;
    }

    if cwd.join("build.rs").is_file() {
        return true;
    }

    if file_contains_any(
        &cwd.join(".cargo").join("config.toml"),
        &["msvc", "windows"],
    ) {
        return true;
    }

    if file_contains_any(&cwd.join("rust-toolchain"), &["windows", "msvc"])
        || file_contains_any(&cwd.join("rust-toolchain.toml"), &["windows", "msvc"])
    {
        return true;
    }

    let Ok(content) = fs::read_to_string(cargo_toml) else {
        return false;
    };
    RUST_NATIVE_DEP_HINTS
        .iter()
        .any(|name| cargo_toml_mentions_dependency(&content, name))
}

fn file_contains_any(path: &Path, needles: &[&str]) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let lower = content.to_ascii_lowercase();
    needles.iter().any(|needle| lower.contains(needle))
}

fn cargo_toml_mentions_dependency(content: &str, name: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with(&format!("{name} ="))
            || trimmed.starts_with(&format!("{name}="))
            || trimmed.starts_with(&format!("\"{name}\" ="))
            || trimmed.starts_with(&format!("\"{name}\"="))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;
    use std::path::{Path, PathBuf};

    struct FakeMsvcIo {
        vswhere_path: Option<PathBuf>,
        vswhere_output: Result<String, String>,
        vsdevcmd_exists: bool,
        vsdevcmd_output: Result<String, String>,
        vsdevcmd_calls: Cell<usize>,
    }

    impl FakeMsvcIo {
        fn success() -> Self {
            Self {
                vswhere_path: Some(PathBuf::from(
                    "C:\\Program Files (x86)\\Microsoft Visual Studio\\Installer\\vswhere.exe",
                )),
                vswhere_output: Ok("C:\\VS\\Community\r\n".to_string()),
                vsdevcmd_exists: true,
                vsdevcmd_output: Ok(
                    "PATH=C:\\VS\\VC\\bin\r\nINCLUDE=C:\\VS\\VC\\include\r\nSECRET=hidden\r\n"
                        .to_string(),
                ),
                vsdevcmd_calls: Cell::new(0),
            }
        }
    }

    impl MsvcEnvResolverIo for FakeMsvcIo {
        fn vswhere_path(&self) -> Option<PathBuf> {
            self.vswhere_path.clone()
        }

        fn run_vswhere(&self, _vswhere_path: &Path) -> Result<String, String> {
            self.vswhere_output.clone()
        }

        fn path_exists(&self, path: &Path) -> bool {
            path.ends_with("VsDevCmd.bat") && self.vsdevcmd_exists
        }

        fn run_vsdevcmd(
            &self,
            _vsdevcmd_path: &Path,
            _arch: &str,
            _host_arch: &str,
        ) -> Result<String, String> {
            self.vsdevcmd_calls.set(self.vsdevcmd_calls.get() + 1);
            self.vsdevcmd_output.clone()
        }
    }

    fn reset_cache() {
        clear_msvc_env_cache_for_tests();
    }

    #[test]
    fn parse_set_output_keeps_values_containing_equals() {
        let parsed = parse_set_output("PATH=C:\\VS=Tools\r\nINCLUDE=C:\\Inc\r\nNO_EQUALS\r\n");

        assert_eq!(parsed.get("PATH").map(String::as_str), Some("C:\\VS=Tools"));
        assert_eq!(parsed.get("INCLUDE").map(String::as_str), Some("C:\\Inc"));
        assert!(!parsed.contains_key("NO_EQUALS"));
    }

    #[test]
    fn filter_msvc_env_keeps_only_allowed_keys() {
        let raw = parse_set_output(
            "PATH=C:\\VS\\bin\r\nINCLUDE=C:\\VS\\include\r\nSECRET_TOKEN=redacted\r\nVSCMD_VER=17.0\r\n",
        );
        let filtered = filter_msvc_env(raw);

        assert_eq!(
            filtered.get("PATH").map(String::as_str),
            Some("C:\\VS\\bin")
        );
        assert_eq!(
            filtered.get("INCLUDE").map(String::as_str),
            Some("C:\\VS\\include")
        );
        assert_eq!(filtered.get("VSCMD_VER").map(String::as_str), Some("17.0"));
        assert!(!filtered.contains_key("SECRET_TOKEN"));
    }

    #[test]
    fn project_needs_msvc_detects_tauri_and_binding_projects() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!project_needs_msvc(tmp.path()));

        fs::create_dir(tmp.path().join("src-tauri")).unwrap();
        assert!(project_needs_msvc(tmp.path()));

        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("binding.gyp"), "{}").unwrap();
        assert!(project_needs_msvc(tmp.path()));
    }

    #[test]
    fn project_needs_msvc_detects_native_package_dependencies() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("package.json"),
            r#"{"dependencies":{"sharp":"latest"},"devDependencies":{"vite":"latest"}}"#,
        )
        .unwrap();

        assert!(project_needs_msvc(tmp.path()));
    }

    #[test]
    fn project_needs_msvc_rejects_plain_cargo_and_plain_package_projects() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Cargo.toml"), "[package]\nname='plain'\n").unwrap();
        assert!(!project_needs_msvc(tmp.path()));

        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("package.json"),
            r#"{"dependencies":{"svelte":"latest"}}"#,
        )
        .unwrap();
        assert!(!project_needs_msvc(tmp.path()));
    }

    #[test]
    fn project_needs_msvc_detects_rust_projects_with_stronger_native_markers() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname='native'\n[build-dependencies]\ncc='1'\n",
        )
        .unwrap();
        assert!(project_needs_msvc(tmp.path()));

        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Cargo.toml"), "[package]\nname='native'\n").unwrap();
        fs::write(tmp.path().join("build.rs"), "fn main() {}").unwrap();
        assert!(project_needs_msvc(tmp.path()));
    }

    #[test]
    fn merge_path_like_keeps_injected_entries_before_base_and_dedupes() {
        let result = merge_path_like("C:\\shared;C:\\User\\bin", "C:\\VS\\bin;C:\\Shared");

        assert_eq!(result.value, "C:\\VS\\bin;C:\\Shared;C:\\User\\bin");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn merge_path_like_warns_when_value_is_unusually_long() {
        let long_base = (0..1800)
            .map(|index| format!("C:\\VeryLongPathSegment\\{index}"))
            .collect::<Vec<_>>()
            .join(";");
        let result = merge_path_like(&long_base, "C:\\VS\\bin");

        assert!(result.value.starts_with("C:\\VS\\bin"));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.code == "path_like_env_too_long"));
    }

    #[test]
    fn resolve_windows_msvc_env_warns_when_vswhere_is_missing() {
        reset_cache();
        let io = FakeMsvcIo {
            vswhere_path: None,
            ..FakeMsvcIo::success()
        };

        let decision = resolve_windows_msvc_env(&io, "x64", "x64");

        assert!(matches!(decision, MsvcEnvDecision::Warn { .. }));
        if let MsvcEnvDecision::Warn { next_action, .. } = decision {
            assert!(next_action.contains("Visual Studio Installer"));
        }
    }

    #[test]
    fn resolve_windows_msvc_env_warns_when_vc_tools_workload_is_missing() {
        reset_cache();
        let io = FakeMsvcIo {
            vswhere_output: Ok("\r\n".to_string()),
            ..FakeMsvcIo::success()
        };

        let decision = resolve_windows_msvc_env(&io, "x64", "x64");

        assert!(matches!(decision, MsvcEnvDecision::Warn { .. }));
        if let MsvcEnvDecision::Warn {
            message,
            next_action,
        } = decision
        {
            assert!(message.contains("VC tools"));
            assert!(next_action.contains("Desktop development with C++"));
        }
    }

    #[test]
    fn resolve_windows_msvc_env_warns_when_vsdevcmd_is_missing() {
        reset_cache();
        let io = FakeMsvcIo {
            vsdevcmd_exists: false,
            ..FakeMsvcIo::success()
        };

        let decision = resolve_windows_msvc_env(&io, "x64", "x64");

        assert!(matches!(decision, MsvcEnvDecision::Warn { .. }));
        if let MsvcEnvDecision::Warn { message, .. } = decision {
            assert!(message.contains("VsDevCmd.bat"));
        }
    }

    #[test]
    fn resolve_windows_msvc_env_returns_sanitized_injected_env() {
        reset_cache();
        let io = FakeMsvcIo::success();

        let decision = resolve_windows_msvc_env(&io, "x64", "x64");

        let MsvcEnvDecision::Inject { env, source } = decision else {
            panic!("expected injected env");
        };
        assert_eq!(env.get("PATH").map(String::as_str), Some("C:\\VS\\VC\\bin"));
        assert_eq!(
            env.get("INCLUDE").map(String::as_str),
            Some("C:\\VS\\VC\\include")
        );
        assert!(!env.contains_key("SECRET"));
        assert_eq!(source.installation_path, PathBuf::from("C:\\VS\\Community"));
        assert_eq!(source.arch, "x64");
        assert_eq!(source.host_arch, "x64");
    }

    #[test]
    fn resolve_windows_msvc_env_caches_success_by_installation_and_arch() {
        reset_cache();
        let io = FakeMsvcIo::success();

        let first = resolve_windows_msvc_env(&io, "x64", "x64");
        let second = resolve_windows_msvc_env(&io, "x64", "x64");

        assert!(matches!(first, MsvcEnvDecision::Inject { .. }));
        assert!(matches!(second, MsvcEnvDecision::Inject { .. }));
        assert_eq!(io.vsdevcmd_calls.get(), 1);
    }

    #[test]
    fn build_spawn_env_plan_skips_remote_and_off_mode() {
        let decision = MsvcEnvDecision::Inject {
            env: HashMap::from([("PATH".to_string(), "C:\\VS\\bin".to_string())]),
            source: MsvcEnvSource {
                installation_path: PathBuf::from("C:\\VS"),
                arch: "x64".to_string(),
                host_arch: "x64".to_string(),
            },
        };

        let remote = build_spawn_env_plan(
            Path::new("C:\\native"),
            true,
            WindowsMsvcEnvMode::Always,
            SpawnPathPolicy::AlwaysUseAugmentedPath,
            Some("C:\\App\\bin"),
            decision.clone(),
        );
        assert!(remote.path_override.is_none());
        assert!(matches!(
            remote.status,
            MsvcEnvStatus::Skipped(MsvcEnvSkipReason::RemoteSession)
        ));

        let off = build_spawn_env_plan(
            Path::new("C:\\native"),
            false,
            WindowsMsvcEnvMode::Off,
            SpawnPathPolicy::AlwaysUseAugmentedPath,
            Some("C:\\App\\bin"),
            decision,
        );
        assert_eq!(off.path_override.as_deref(), Some("C:\\App\\bin"));
        assert!(matches!(
            off.status,
            MsvcEnvStatus::Skipped(MsvcEnvSkipReason::DisabledByUser)
        ));
    }

    #[test]
    fn build_spawn_env_plan_preserves_inherit_policy_when_skipped() {
        let plan = build_spawn_env_plan(
            Path::new("C:\\plain"),
            false,
            WindowsMsvcEnvMode::Auto,
            SpawnPathPolicy::InheritUnlessInjected,
            Some("C:\\App\\bin"),
            MsvcEnvDecision::Skip {
                reason: MsvcEnvSkipReason::ProjectDoesNotNeedNativeToolchain,
            },
        );

        assert!(plan.path_override.is_none());
    }

    #[test]
    fn build_spawn_env_plan_merges_msvc_path_before_base() {
        let plan = build_spawn_env_plan(
            Path::new("C:\\native"),
            false,
            WindowsMsvcEnvMode::Always,
            SpawnPathPolicy::AlwaysUseAugmentedPath,
            Some("C:\\App\\bin"),
            MsvcEnvDecision::Inject {
                env: HashMap::from([
                    ("PATH".to_string(), "C:\\VS\\bin;C:\\SDK\\bin".to_string()),
                    ("INCLUDE".to_string(), "C:\\VS\\include".to_string()),
                ]),
                source: MsvcEnvSource {
                    installation_path: PathBuf::from("C:\\VS"),
                    arch: "x64".to_string(),
                    host_arch: "x64".to_string(),
                },
            },
        );

        assert_eq!(
            plan.path_override.as_deref(),
            Some("C:\\VS\\bin;C:\\SDK\\bin;C:\\App\\bin")
        );
        assert_eq!(
            plan.msvc_env.get("INCLUDE").map(String::as_str),
            Some("C:\\VS\\include")
        );
        assert!(!plan.msvc_env.contains_key("PATH"));
        assert!(matches!(plan.status, MsvcEnvStatus::Injected(_)));
    }

    #[test]
    fn build_spawn_env_plan_keeps_warning_status() {
        let plan = build_spawn_env_plan(
            Path::new("C:\\native"),
            false,
            WindowsMsvcEnvMode::Always,
            SpawnPathPolicy::AlwaysUseAugmentedPath,
            Some("C:\\App\\bin"),
            MsvcEnvDecision::Warn {
                message: "Visual Studio VC tools were not found.".to_string(),
                next_action: "Install Desktop development with C++.".to_string(),
            },
        );

        assert_eq!(plan.path_override.as_deref(), Some("C:\\App\\bin"));
        assert!(matches!(plan.status, MsvcEnvStatus::Warning));
        assert_eq!(plan.warnings.len(), 1);
        assert_eq!(plan.warnings[0].code, "msvc_env_unavailable");
    }

    #[test]
    fn merge_extra_env_into_spawn_env_plan_preserves_protected_msvc_values() {
        let mut plan = SpawnEnvPlan {
            path_override: Some("C:\\VS\\bin;C:\\App\\bin".to_string()),
            msvc_env: HashMap::from([
                ("INCLUDE".to_string(), "C:\\VS\\include".to_string()),
                ("LIB".to_string(), "C:\\VS\\lib".to_string()),
            ]),
            warnings: vec![],
            status: MsvcEnvStatus::Injected(MsvcEnvSource {
                installation_path: PathBuf::from("C:\\VS"),
                arch: "x64".to_string(),
                host_arch: "x64".to_string(),
            }),
        };

        merge_extra_env_into_spawn_env_plan(
            &mut plan,
            &HashMap::from([
                ("PATH".to_string(), "C:\\User\\bin".to_string()),
                ("INCLUDE".to_string(), "C:\\User\\include".to_string()),
                ("API_TIMEOUT_MS".to_string(), "600000".to_string()),
            ]),
        );

        assert_eq!(
            plan.path_override.as_deref(),
            Some("C:\\User\\bin;C:\\VS\\bin;C:\\App\\bin")
        );
        assert_eq!(
            plan.msvc_env.get("INCLUDE").map(String::as_str),
            Some("C:\\User\\include;C:\\VS\\include")
        );
        assert_eq!(
            plan.msvc_env.get("API_TIMEOUT_MS").map(String::as_str),
            Some("600000")
        );
    }
}
