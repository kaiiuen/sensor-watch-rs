//! Internal settings persistence.
//!
//! Saves the app's configuration to a JSON file in the user's per-user config
//! directory so the user's settings, presets, and custom NTP servers survive
//! restarts even when the app is installed in a read-only location (Program
//! Files, /usr/bin, etc.). The file is written atomically
//! (write-temp-fsync-then-rename) to avoid corruption on crash.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::settings::AppSettings;
use super::test_runtime;

const MAX_SETTINGS_BYTES: u64 = 512 * 1024;
const SETTINGS_FILE: &str = "studio-settings.json";
const RUNTIME_FILE: &str = "studio-runtime.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RuntimePreferences {
    pub fresh_test_executable_profile: bool,
    pub persist_user_changes: bool,
}

impl Default for RuntimePreferences {
    fn default() -> Self {
        Self {
            fresh_test_executable_profile: true,
            persist_user_changes: true,
        }
    }
}

pub fn runtime_path() -> PathBuf {
    test_runtime::normal_config_dir().join(RUNTIME_FILE)
}

/// Loads launch preferences before the executable-scoped profile is selected.
/// Older Studio versions stored these in the normal settings file, so migrate
/// those values once when the bootstrap file does not exist yet.
pub fn load_runtime_preferences() -> RuntimePreferences {
    let path = runtime_path();
    if let Ok(json) = read_bounded(&path, 16 * 1024) {
        if let Ok(preferences) = serde_json::from_str::<RuntimePreferences>(&json) {
            return preferences;
        }
    }
    let migrated = load_at(&test_runtime::normal_config_dir().join(SETTINGS_FILE))
        .map(|settings| RuntimePreferences {
            fresh_test_executable_profile: settings.fresh_test_executable_profile,
            persist_user_changes: settings.persist_user_changes,
        })
        .unwrap_or_default();
    let _ = save_runtime_preferences(&migrated);
    migrated
}

/// Writes bootstrap preferences independently of the active profile.
pub fn save_runtime_preferences(preferences: &RuntimePreferences) -> Result<(), String> {
    let json = serde_json::to_string_pretty(preferences).map_err(|e| e.to_string())?;
    save_json_at(&json, &runtime_path())
}

fn save_json_at(json: &str, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    ensure_regular_or_absent(path)?;
    ensure_regular_or_absent(&tmp)?;
    if tmp.exists() {
        std::fs::remove_file(&tmp).map_err(|e| e.to_string())?;
    }
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    replace_existing(&tmp, path)
}

/// Returns the active settings file path. Settings and restore points use the
/// same profile selected by `test_runtime`.
pub fn settings_path() -> PathBuf {
    test_runtime::active().settings
}

/// Loads settings from the active profile, if present.
pub fn load() -> Option<AppSettings> {
    load_at(&settings_path())
}

/// Loads settings from an explicitly selected path during profile bootstrap.
pub fn load_at(path: &Path) -> Option<AppSettings> {
    let json = read_bounded(path, MAX_SETTINGS_BYTES).ok()?;
    AppSettings::from_json(&json).ok()
}

/// Saves settings to the per-user config directory (atomically).
///
/// Writes to a temp file in the same directory, syncs it to disk, then renames
/// it over the target so a crash can never leave a half-written settings file.
/// fsync and directory sync are best-effort: if they fail the rename still
/// proceeds.
pub fn save(settings: &AppSettings) -> Result<(), String> {
    save_at(settings, &settings_path())
}

/// Saves a settings document to an explicit profile path.
pub fn save_at(settings: &AppSettings, path: &Path) -> Result<(), String> {
    let json = settings.to_json()?;

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    // Write to a temp file, then rename over the target for atomicity.
    let tmp = path.with_extension("json.tmp");
    ensure_regular_or_absent(&path)?;
    ensure_regular_or_absent(&tmp)?;
    // Clean up any stale temp file left by a previously crashed/cancelled write.
    if tmp.exists() {
        std::fs::remove_file(&tmp).map_err(|e| e.to_string())?;
    }

    std::fs::write(&tmp, &json).map_err(|e| e.to_string())?;

    // Best-effort fsync of the temp file before rename so the contents hit the
    // disk before the rename makes them visible. If it fails, proceed anyway.
    if let Ok(file) = std::fs::File::open(&tmp) {
        let _ = file.sync_all();
    }

    replace_existing(&tmp, &path)?;

    // Best-effort fsync of the parent directory so the rename itself is durable.
    // On Windows this is generally not needed/supported, so failures are ignored.
    if let Some(dir) = path.parent() {
        if let Ok(dir_file) = std::fs::File::open(dir) {
            let _ = dir_file.sync_all();
        }
    }

    Ok(())
}

/// Saves both bootstrap preferences independently of the active profile.
pub fn save_toggle_preferences(
    fresh_test_executable_profile: bool,
    persist_user_changes: bool,
) -> Result<(), String> {
    save_runtime_preferences(&RuntimePreferences {
        fresh_test_executable_profile,
        persist_user_changes,
    })
}

/// Replaces a file on platforms where rename cannot overwrite an existing file.
/// Keep the old target until the new file is installed, restoring it on failure.
fn replace_existing(tmp: &std::path::Path, target: &std::path::Path) -> Result<(), String> {
    ensure_regular_or_absent(tmp)?;
    ensure_regular_or_absent(target)?;
    let backup = target.with_extension("json.previous");
    ensure_regular_or_absent(&backup)?;
    let had_old = target.is_file();
    if had_old {
        if backup.exists() {
            std::fs::remove_file(&backup)
                .map_err(|e| format!("cannot remove old settings backup: {e}"))?;
        }
        if let Err(error) = std::fs::rename(target, &backup) {
            return Err(format!("cannot stage existing settings file: {error}"));
        }
    }
    if let Err(error) = std::fs::rename(tmp, target) {
        if had_old {
            let _ = std::fs::rename(&backup, target);
        }
        let _ = std::fs::remove_file(tmp);
        return Err(format!("cannot install settings file: {error}"));
    }
    if had_old {
        std::fs::remove_file(&backup)
            .map_err(|e| format!("settings saved, but old backup could not be removed: {e}"))?;
    }
    Ok(())
}

fn ensure_regular_or_absent(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "refusing symlinked settings path: {}",
            path.display()
        )),
        Ok(metadata) if !metadata.is_file() => Err(format!(
            "settings path is not a regular file: {}",
            path.display()
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect settings path: {error}")),
    }
}

fn read_bounded(path: &Path, max_bytes: u64) -> std::io::Result<String> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(max_bytes + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "settings file is too large",
        ));
    }
    String::from_utf8(bytes)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "settings is not UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::RuntimePreferences;

    #[test]
    fn bootstrap_preferences_have_explicit_true_defaults() {
        let defaults = RuntimePreferences::default();
        assert!(defaults.fresh_test_executable_profile);
        assert!(defaults.persist_user_changes);
    }

    #[test]
    fn bootstrap_preferences_are_backward_compatible_with_missing_fields() {
        let loaded: RuntimePreferences = serde_json::from_str("{}").unwrap();
        assert!(loaded.fresh_test_executable_profile);
        assert!(loaded.persist_user_changes);

        let loaded: RuntimePreferences =
            serde_json::from_str(r#"{"fresh_test_executable_profile":false}"#).unwrap();
        assert!(!loaded.fresh_test_executable_profile);
        assert!(loaded.persist_user_changes);
    }
}
