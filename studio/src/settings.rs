//! Settings save/export.
//!
//! Serializes the app's configuration (language, theme, presets, NTP server,
//! simulator scale) to a JSON file so the user can back up, restore, or export
//! their settings and data.

use serde::{Deserialize, Serialize};

use super::components::BuildProfile;
use super::i18n::Language;
use super::modules::ModuleManager;
use super::ntp;
use super::presets::PresetManager;
use super::theme::Theme;
use super::watch_config::WatchConfig;

const MAX_SETTINGS_JSON_BYTES: usize = 256 * 1024;
const MAX_NTP_SERVERS: usize = 64;
const MAX_SETTINGS_TEXT_BYTES: usize = 256;

/// Versioned, persistence-safe Studio representation of RTC calibration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RtcCalibrationSettings {
    pub version: u8,
    pub base_ppm: f32,
    pub temperature_coefficient_ppm_per_c: f32,
    pub reference_temperature_c: f32,
}

impl Default for RtcCalibrationSettings {
    fn default() -> Self {
        Self {
            version: 0,
            base_ppm: 0.0,
            temperature_coefficient_ppm_per_c: 0.0,
            reference_temperature_c: 25.0,
        }
    }
}

impl RtcCalibrationSettings {
    pub fn enabled(&self) -> bool {
        self.version == sensor_watch_core::rtc_calibration::CALIBRATION_VERSION
    }
    pub fn clamp_values(&mut self) {
        let c = sensor_watch_core::rtc_calibration::RtcCalibration::new(
            self.base_ppm,
            self.temperature_coefficient_ppm_per_c,
            self.reference_temperature_c,
        );
        self.base_ppm = c.base_ppm;
        self.temperature_coefficient_ppm_per_c = c.temperature_coefficient_ppm_per_c;
        self.reference_temperature_c = c.reference_temperature_c;
        if self.enabled() {
            self.version = sensor_watch_core::rtc_calibration::CALIBRATION_VERSION;
        }
    }
}

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
    /// Optional temperature-compensated RTC calibration. Version 0 is disabled.
    #[serde(default)]
    pub rtc_calibration: RtcCalibrationSettings,
    /// The maximum number of lines kept in each output/terminal/debug log.
    /// Oldest lines are dropped past this so the logs never grow without bound.
    #[serde(default = "default_line_limit")]
    pub line_limit: usize,
    /// Where high-frequency tick/process events are displayed.
    #[serde(default = "default_tick_verbosity")]
    pub tick_verbosity: String,
    /// Named hardware component/build profiles used for configuration review.
    #[serde(default)]
    pub component_profiles: Vec<BuildProfile>,
    /// The active named component profile index.
    #[serde(default)]
    pub active_component_profile: usize,
    /// The selected target board revision.
    #[serde(default = "default_board")]
    pub board: String,
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
        rtc_calibration: &RtcCalibrationSettings,
        line_limit: usize,
        tick_verbosity: String,
        component_profiles: &[BuildProfile],
        active_component_profile: usize,
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
            rtc_calibration: rtc_calibration.clone(),
            line_limit,
            tick_verbosity,
            component_profiles: component_profiles.to_vec(),
            active_component_profile,
            board: default_board(),
        }
    }

    pub fn with_board(mut self, board: impl Into<String>) -> Self {
        self.board = board.into();
        self
    }

    /// Serializes the settings to a JSON string.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    /// Validates values loaded from disk before they can affect the app/build.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version == 0 || self.schema_version > 1 {
            return Err("unsupported settings schema version".into());
        }
        for (field, value) in [
            ("language", &self.language),
            ("theme", &self.theme),
            ("output directory", &self.output_dir),
            ("board", &self.board),
        ] {
            if value.len() > MAX_SETTINGS_TEXT_BYTES || value.chars().any(|c| c.is_control()) {
                return Err(format!("{field} contains invalid or excessive text"));
            }
        }
        if self.output_dir.trim().is_empty() {
            return Err("output directory must not be empty".into());
        }
        if self.ntp_servers.len() > MAX_NTP_SERVERS {
            return Err("too many custom NTP servers".into());
        }
        for (name, host) in &self.ntp_servers {
            if name.len() > MAX_SETTINGS_TEXT_BYTES
                || host.len() > MAX_SETTINGS_TEXT_BYTES
                || name.chars().any(|c| c.is_control())
                || host.chars().any(|c| c.is_control())
                || host.trim().is_empty()
            {
                return Err("custom NTP server contains invalid or excessive text".into());
            }
        }
        if !self.sim_scale.is_finite() || !(0.5..=2.0).contains(&self.sim_scale) {
            return Err("simulator scale must be finite and between 0.5 and 2.0".into());
        }
        if !self.catalog_width.is_finite()
            || !(0.0..=10_000.0).contains(&self.catalog_width)
            || !self.preset_height.is_finite()
            || !(0.0..=10_000.0).contains(&self.preset_height)
        {
            return Err("panel dimensions must be finite and between 0 and 10000".into());
        }
        if self.text_size > 2 {
            return Err("text size must be 0, 1, or 2".into());
        }
        if self.output_dir.trim().is_empty() || self.output_dir.len() > 4096 {
            return Err("output directory must be non-empty and at most 4096 bytes".into());
        }
        if self.output_dir.chars().any(|c| c.is_control()) {
            return Err("output directory cannot contain control characters".into());
        }
        for (name, host) in &self.ntp_servers {
            if name.is_empty()
                || host.is_empty()
                || name.len() > 256
                || host.len() > 256
                || name.chars().any(|c| c.is_control())
                || host.chars().any(|c| c.is_control())
            {
                return Err(
                    "custom NTP server names and hosts must be non-empty, bounded, and printable"
                        .into(),
                );
            }
        }
        if self.ntp_server >= sensor_watch_studio_ntp_server_count(&self.ntp_servers) {
            return Err("selected NTP server is out of range".into());
        }
        if !self.drift_ppm.is_finite() || self.drift_ppm.abs() > 1000.0 {
            return Err("drift correction must be finite and within +/-1000 ppm".into());
        }
        let mut calibration = self.rtc_calibration.clone();
        calibration.clamp_values();
        if calibration.enabled()
            != (self.rtc_calibration.version
                == sensor_watch_core::rtc_calibration::CALIBRATION_VERSION)
            || calibration.base_ppm != self.rtc_calibration.base_ppm
            || calibration.temperature_coefficient_ppm_per_c
                != self.rtc_calibration.temperature_coefficient_ppm_per_c
            || calibration.reference_temperature_c != self.rtc_calibration.reference_temperature_c
        {
            return Err("RTC calibration contains out-of-range values".into());
        }
        if self.text_size > 2 {
            return Err("text size is out of range".into());
        }
        if self.ntp_server >= ntp::SERVERS.len() + self.ntp_servers.len() {
            return Err("NTP server index is out of range".into());
        }
        if self.line_limit == 0 || self.line_limit > 10_000 {
            return Err("line limit must be between 1 and 10000".into());
        }
        if !matches!(self.tick_verbosity.as_str(), "hide" | "dedicated" | "main") {
            return Err("tick verbosity is invalid".into());
        }
        if !matches!(self.board.as_str(), "Green" | "Red / Lite" | "Blue" | "Pro") {
            return Err("board is not a supported revision".into());
        }
        if self.active_component_profile >= self.component_profiles.len()
            && !self.component_profiles.is_empty()
        {
            return Err("active component profile is out of range".into());
        }
        for profile in &self.component_profiles {
            profile.validate()?;
        }
        Ok(())
    }

    /// Deserializes and validates settings from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, String> {
        if json.len() > MAX_SETTINGS_JSON_BYTES {
            return Err("settings JSON is too large".into());
        }
        let settings: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        settings.validate()?;
        Ok(settings)
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
            rtc_calibration: RtcCalibrationSettings::default(),
            line_limit: default_line_limit(),
            tick_verbosity: default_tick_verbosity(),
            component_profiles: Vec::new(),
            active_component_profile: 0,
            board: default_board(),
        }
    }
}

/// The default maximum number of lines kept in each output log.
fn sensor_watch_studio_ntp_server_count(custom: &[(String, String)]) -> usize {
    super::ntp::SERVERS.len() + custom.len()
}

pub fn default_tick_verbosity() -> String {
    "hide".to_string()
}

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
pub fn default_board() -> String {
    "Green".to_string()
}

pub fn default_output_dir() -> String {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|home| format!("{home}/Documents/FirmwareStudio"))
        .unwrap_or_else(|_| "FirmwareStudio".to_string())
}
