//! Internal settings persistence.
//!
//! Saves the app's configuration to a JSON file in the user's per-user config
//! directory so the user's settings, presets, and custom NTP servers survive
//! restarts even when the app is installed in a read-only location (Program
//! Files, /usr/bin, etc.). The file is written atomically
//! (write-temp-fsync-then-rename) to avoid corruption on crash.

use std::io::Read;
use std::path::{Path, PathBuf};

use super::settings::AppSettings;

/// The settings file name, stored in the per-user config directory.
const SETTINGS_FILE: &str = "studio-settings.json";
const MAX_SETTINGS_BYTES: u64 = 512 * 1024;

/// Returns the per-user config directory, preferring platform conventions.
///
/// - Unix: `$XDG_CONFIG_HOME` if set, else `$HOME/.config`
/// - Windows: `%APPDATA%`
/// - Falls back to the executable's directory if none of the above exist.
fn config_dir() -> PathBuf {
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
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Returns the path to the settings file (per-user config directory).
pub fn settings_path() -> PathBuf {
    config_dir().join(SETTINGS_FILE)
}

/// Loads settings from the per-user config directory, if present.
pub fn load() -> Option<AppSettings> {
    let path = settings_path();
    let json = read_bounded(&path, MAX_SETTINGS_BYTES).ok()?;
    AppSettings::from_json(&json).ok()
}

/// Saves settings to the per-user config directory (atomically).
///
/// Writes to a temp file in the same directory, syncs it to disk, then renames
/// it over the target so a crash can never leave a half-written settings file.
/// fsync and directory sync are best-effort: if they fail the rename still
/// proceeds.
pub fn save(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path();
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
