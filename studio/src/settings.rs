//! Settings save/export.
//!
//! Serializes the app's configuration (language, theme, presets, NTP server,
//! simulator scale) to a JSON file so the user can back up, restore, or export
//! their settings and data.

use serde::{Deserialize, Serialize};

use super::i18n::Language;
use super::presets::PresetManager;
use super::theme::Theme;
use super::watch_config::WatchConfig;

/// The serializable app configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSettings {
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
}

impl AppSettings {
    /// Captures the current app state into a serializable struct.
    pub fn capture(
        language: Language,
        theme: Theme,
        presets: &PresetManager,
        ntp_server: usize,
        ntp_servers: &[(String, String)],
        sim_scale: f32,
        watch_config: &WatchConfig,
    ) -> Self {
        AppSettings {
            language: language.name().to_string(),
            theme: theme.name().to_string(),
            presets: presets.clone(),
            ntp_server,
            ntp_servers: ntp_servers.to_vec(),
            sim_scale,
            watch_config: watch_config.clone(),
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
