//! Settings save/export.
//!
//! Serializes the app's configuration (language, theme, presets, NTP server,
//! simulator scale) to a JSON file so the user can back up, restore, or export
//! their settings and data.

use serde::{Deserialize, Serialize};

use super::i18n::Language;
use super::modules::ModuleManager;
use super::presets::PresetManager;
use super::theme::Theme;
use super::watch_config::WatchConfig;

/// The serializable app configuration.
///
/// Every field carries a `#[serde(default)]` so that a JSON file from an older
/// or newer version, or one missing a field, still loads by filling in defaults
/// instead of failing the whole load.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Schema version, bumped on backward-incompatible changes so future
    /// migrations are possible. Old files without it default to 1.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// The selected language.
    pub language: String,
    /// The selected theme.
    pub theme: String,
    /// The watch-face presets.
    pub presets: PresetManager,
    /// The selected NTP server index.
    pub ntp_server: usize,
    /// Custom NTP servers added by the user (name, host).
    pub ntp_servers: Vec<(String, String)>,
    /// The simulator display scale.
    pub sim_scale: f32,
    /// The watch configuration.
    pub watch_config: WatchConfig,
    /// The UI text size (0=small, 1=normal, 2=big).
    pub text_size: u8,
    /// Persisted Watch Faces panel widths.
    pub catalog_width: f32,
    pub preset_height: f32,
    /// Custom hardware modules.
    pub modules: ModuleManager,
    /// The output directory for built artifacts (e.g. the .uf2 file).
    /// Defaults to a writable user folder when running as a standalone exe.
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
    /// Whether the first-run welcome overlay has been dismissed.
    #[serde(default)]
    pub first_run: bool,
    /// The last measured crystal drift (parts-per-million), persisted between
    /// sessions so the user can recall the calibration without re-measuring.
    #[serde(default)]
    pub drift_ppm: f64,
    /// The maximum number of lines kept in each output/terminal/debug log.
    /// Oldest lines are dropped past this so the logs never grow without bound.
    #[serde(default = "default_line_limit")]
    pub line_limit: usize,
}

impl AppSettings {
    /// Captures the current app state into a serializable struct.
    #[allow(clippy::too_many_arguments)]
    pub fn capture(
        language: Language,
        theme: Theme,
        presets: &PresetManager,
        ntp_server: usize,
        ntp_servers: &[(String, String)],
        sim_scale: f32,
        watch_config: &WatchConfig,
        text_size: u8,
        catalog_width: f32,
        preset_height: f32,
        modules: &ModuleManager,
        output_dir: String,
        first_run: bool,
        drift_ppm: f64,
        line_limit: usize,
    ) -> Self {
        AppSettings {
            schema_version: 1,
            language: language.name().to_string(),
            theme: theme.name().to_string(),
            presets: presets.clone(),
            ntp_server,
            ntp_servers: ntp_servers.to_vec(),
            sim_scale,
            watch_config: watch_config.clone(),
            text_size,
            catalog_width,
            preset_height,
            modules: modules.clone(),
            output_dir,
            first_run,
            drift_ppm,
            line_limit,
        }
    }

    /// Serializes the settings to a JSON string.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    /// Deserializes settings from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
}

/// Defaults used when a field is missing from a JSON file (or the file is
/// absent entirely). Kept simple: empty strings and zero-values, since the app
/// populates real defaults from the running UI state before serializing.
impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            schema_version: 1,
            language: String::new(),
            theme: String::new(),
            presets: PresetManager::new(),
            ntp_server: 0,
            ntp_servers: Vec::new(),
            sim_scale: 1.0,
            watch_config: WatchConfig::default(),
            text_size: 1,
            catalog_width: 0.0,
            preset_height: 0.0,
            modules: ModuleManager::default(),
            output_dir: default_output_dir(),
            first_run: false,
            drift_ppm: 0.0,
            line_limit: default_line_limit(),
        }
    }
}

/// The default maximum number of lines kept in each output log.
pub fn default_line_limit() -> usize {
    500
}

/// The default schema version for settings that predate schema tracking.
pub fn default_schema_version() -> u32 {
    1
}

/// The default output directory for built artifacts: `<User Documents>/FirmwareStudio`.
/// This is writable even when the app runs as a standalone exe from a read-only
/// location.
pub fn default_output_dir() -> String {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|home| format!("{home}/Documents/FirmwareStudio"))
        .unwrap_or_else(|_| "FirmwareStudio".to_string())
}
