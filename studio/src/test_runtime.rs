//! Executable-scoped persistence for debug/test Studio binaries.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sha2::{Digest, Sha256};

const SETTINGS_FILE: &str = "studio-settings.json";
const RESTORE_FILE: &str = "studio-restore-points.json";
const FAILURE_IDENTITY: &str = "identity-unavailable";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfilePaths {
    pub root: PathBuf,
    pub settings: PathBuf,
    pub restore: PathBuf,
    pub warning: Option<String>,
    pub isolated_debug: bool,
}

static ACTIVE: OnceLock<ProfilePaths> = OnceLock::new();

/// The ordinary per-user Studio directory, without any executable namespace.
pub fn normal_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("FirmwareStudio");
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("firmware-studio");
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join(".config").join("firmware-studio");
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn paths(root: PathBuf, warning: Option<String>, isolated_debug: bool) -> ProfilePaths {
    ProfilePaths {
        settings: root.join(SETTINGS_FILE),
        restore: root.join(RESTORE_FILE),
        root,
        warning,
        isolated_debug,
    }
}

/// Resolves a profile using an injected executable identity. This is also the
/// test seam; production uses `current_executable_identity` below.
pub fn resolve(fresh: bool, executable_identity: Result<String, String>) -> ProfilePaths {
    resolve_from(fresh, executable_identity, normal_config_dir())
}

/// Resolves a profile beneath an explicitly selected, already validated root.
pub fn resolve_from(
    fresh: bool,
    executable_identity: Result<String, String>,
    normal: PathBuf,
) -> ProfilePaths {
    if !cfg!(debug_assertions) || !fresh {
        return paths(normal, None, false);
    }
    match executable_identity {
        Ok(identity) if !identity.is_empty() => paths(
            normal.join("debug").join(identity),
            Some("Using an executable-isolated debug Studio profile".into()),
            true,
        ),
        Ok(_) | Err(_) => paths(
            normal.join("debug").join(FAILURE_IDENTITY),
            Some("Studio could not identify this debug executable; using isolated fallback profile debug/identity-unavailable".into()),
            true,
        ),
    }
}

pub fn current_executable_identity() -> Result<String, String> {
    let executable =
        std::env::current_exe().map_err(|e| format!("cannot locate executable: {e}"))?;
    let bytes = std::fs::read(&executable)
        .map_err(|e| format!("cannot read executable {}: {e}", executable.display()))?;
    let digest = Sha256::digest(bytes);
    Ok(format!("{digest:x}"))
}

pub fn initialize(fresh: bool) -> ProfilePaths {
    initialize_from(fresh, normal_config_dir())
}

pub fn initialize_from(fresh: bool, normal: PathBuf) -> ProfilePaths {
    let profile = resolve_from(fresh, current_executable_identity(), normal);
    let _ = ACTIVE.set(profile.clone());
    profile
}

pub fn active() -> ProfilePaths {
    ACTIVE
        .get()
        .cloned()
        .unwrap_or_else(|| paths(normal_config_dir(), None, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_path_is_unchanged() {
        let p = resolve(true, Ok("abc".into()));
        if cfg!(debug_assertions) {
            assert!(p.root.ends_with(Path::new("debug").join("abc")));
        } else {
            assert_eq!(p.root, normal_config_dir());
        }
    }

    #[test]
    fn debug_namespaces_and_same_identity_reuses_profile() {
        let a = resolve(true, Ok("abc".into()));
        let b = resolve(true, Ok("abc".into()));
        assert_eq!(a, b);
        if cfg!(debug_assertions) {
            assert!(a.root.ends_with(Path::new("debug").join("abc")));
        } else {
            assert_eq!(a.root, normal_config_dir());
        }
    }

    #[test]
    fn different_identities_get_different_profiles() {
        let a = resolve(true, Ok("a".into())).root;
        let b = resolve(true, Ok("b".into())).root;
        if cfg!(debug_assertions) {
            assert_ne!(a, b);
        } else {
            assert_eq!(a, normal_config_dir());
            assert_eq!(b, normal_config_dir());
        }
    }

    #[test]
    fn settings_and_restore_paths_agree_on_root() {
        let p = resolve(true, Ok("abc".into()));
        assert_eq!(p.settings.parent(), p.restore.parent());
    }

    #[test]
    fn new_debug_identity_is_clean_until_explicit_migration() {
        let normal = std::env::temp_dir().join("studio-clean-debug-profile");
        let old = resolve_from(true, Ok("old-hash".into()), normal.clone());
        let new = resolve_from(true, Ok("new-hash".into()), normal);
        if cfg!(debug_assertions) {
            assert_ne!(old.root, new.root);
            assert!(!new.root.join(SETTINGS_FILE).exists());
        } else {
            assert_eq!(old.root, new.root);
        }
    }

    #[test]
    fn custom_root_keeps_debug_hash_namespacing() {
        let root = std::env::temp_dir().join("studio-runtime-custom-root");
        let p = resolve_from(true, Ok("abc".into()), root.clone());
        if cfg!(debug_assertions) {
            assert_eq!(p.root, root.join("debug").join("abc"));
        } else {
            assert_eq!(p.root, root);
        }
    }

    #[test]
    fn failure_isolated_and_toggle_off_is_normal() {
        let failed = resolve(true, Err("read failed".into()));
        if cfg!(debug_assertions) {
            assert!(failed
                .root
                .ends_with(Path::new("debug").join(FAILURE_IDENTITY)));
            assert!(!failed.root.ends_with("FirmwareStudio"));
        } else {
            assert_eq!(failed.root, normal_config_dir());
        }
        assert_eq!(resolve(false, Ok("abc".into())).root, normal_config_dir());
    }
}
