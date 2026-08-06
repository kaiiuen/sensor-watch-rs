//! Internal settings persistence.
//!
//! Saves the app's configuration to a JSON file next to the executable so the
//! user's settings, presets, and custom NTP servers survive restarts. The file
//! is written atomically (write-then-rename) to avoid corruption.

use std::path::PathBuf;

use super::settings::AppSettings;

/// The settings file name, stored next to the executable.
const SETTINGS_FILE: &str = "studio-settings.json";

/// Returns the path to the settings file (next to the executable).
pub fn settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(SETTINGS_FILE)
}

/// Loads settings from the file next to the executable, if present.
pub fn load() -> Option<AppSettings> {
    let path = settings_path();
    let json = std::fs::read_to_string(&path).ok()?;
    AppSettings::from_json(&json).ok()
}

/// Saves settings to the file next to the executable (atomically).
pub fn save(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path();
    let json = settings.to_json()?;
    // Write to a temp file, then rename over the target for atomicity.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}
