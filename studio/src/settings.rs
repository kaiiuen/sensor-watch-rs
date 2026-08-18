//! Settings save/export.
//!
//! Serializes the app's configuration (language, theme, presets, NTP server,
//! simulator scale) to a JSON file so the user can back up, restore, or export
//! their settings and data.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::components::BuildProfile;
use super::i18n::Language;
use super::modules::ModuleManager;
use super::ntp;
use super::presets::PresetManager;
use super::theme::Theme;
use super::watch_config::{WatchConfig, TIMEZONE_OFFSETS};

/// Preferred number of rows for the top-level tab bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabLayoutMode {
    Auto,
    OneRow,
    TwoRows,
    ThreeRows,
}

impl Default for TabLayoutMode {
    fn default() -> Self {
        Self::Auto
    }
}

/// How the tab bar handles tabs that do not fit on one line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabOverflowBehavior {
    Wrap,
    HorizontalScroll,
}

impl Default for TabOverflowBehavior {
    fn default() -> Self {
        Self::Wrap
    }
}

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
pub struct PanelUxOverrides {
    /// Allow the guided tutorial to install its input barrier.
    #[serde(default = "default_true")]
    pub tutorial_input_barrier: bool,
    /// Draw the tutorial spotlight around its current target.
    #[serde(default = "default_true")]
    pub tutorial_spotlight: bool,
    /// Show advanced-only tabs when Developer Mode is enabled.
    #[serde(default = "default_true")]
    pub advanced_tab_visibility: bool,
    /// Show simulated or host-only diagnostic details.
    #[serde(default = "default_true")]
    pub simulated_diagnostics_visibility: bool,
    /// Show developer tool affordances in this panel.
    #[serde(default = "default_true")]
    pub developer_tool_visibility: bool,
    /// Use the full confirmation explanation for actions in this panel.
    #[serde(default = "default_true")]
    pub confirmation_verbosity: bool,
}

impl Default for PanelUxOverrides {
    fn default() -> Self {
        Self {
            tutorial_input_barrier: true,
            tutorial_spotlight: true,
            advanced_tab_visibility: true,
            simulated_diagnostics_visibility: true,
            developer_tool_visibility: true,
            confirmation_verbosity: true,
        }
    }
}

impl PanelUxOverrides {
    /// UX overrides cannot change any hard safety boundary.
    pub const fn affects_hard_safeguards(&self) -> bool {
        false
    }
}

/// Names of boundaries that are intentionally outside persisted UX settings.
pub const HARD_SAFEGUARD_BOUNDARIES: &[&str] = &[
    "cryptographic and signature checks",
    "UF2, hash, and sidecar validation",
    "configured-build fail-closed contract",
    "drive identity, count, and revalidation",
    "UART bounds, allowlist, and timeouts",
    "shell mutation authorization",
    "editor path and symlink safety",
    "file-browser read-only and path safety",
    "update rollback",
    "explicit physical-action consent",
];

impl AppSettings {
    /// Return the persisted UX policy for a panel, using safe defaults.
    pub fn panel_ux(&self, panel: &str) -> PanelUxOverrides {
        self.panel_ux_overrides
            .get(panel)
            .cloned()
            .unwrap_or_default()
    }

    /// Set a panel UX policy without changing any hard safeguard.
    pub fn set_panel_ux(&mut self, panel: impl Into<String>, overrides: PanelUxOverrides) {
        self.panel_ux_overrides.insert(panel.into(), overrides);
    }
}

/// The serializable app configuration.
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
    /// The configured Studio data root. It is applied only on next launch.
    #[serde(default = "default_data_folder")]
    pub data_folder: String,
    /// The output directory for built artifacts (e.g. the .uf2 file).
    /// Defaults to a writable user folder when running as a standalone exe.
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
    /// Whether the first-run welcome overlay has been dismissed.
    #[serde(default = "default_first_run")]
    pub first_run: bool,
    /// Stable IDs of panel tours explicitly completed or skipped.
    /// Missing in legacy settings, which means no panel has been claimed.
    #[serde(default)]
    pub tour_claims: Vec<String>,
    /// Whether ordinary Studio changes and close automatically save settings.
    #[serde(default = "default_true")]
    pub persist_user_changes: bool,
    /// Whether a valid build starts with a fresh transient test session.
    #[serde(default = "default_true")]
    pub reset_test_session_on_compile: bool,
    /// Whether debug/test executables use an isolated profile per executable.
    #[serde(default = "default_true")]
    pub fresh_test_executable_profile: bool,
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
    /// Whether the opt-in Developer Mode presentation is enabled.
    ///
    /// This only exposes development UX. Hard safety boundaries remain enforced
    /// by their operation handlers.
    #[serde(default)]
    pub developer_mode: bool,
    /// Legacy name retained for one-way migration from older settings files.
    #[serde(default, rename = "advanced_mode", skip_serializing)]
    legacy_advanced_mode: bool,
    /// Per-panel presentation and guidance preferences.
    #[serde(default)]
    pub panel_ux_overrides: BTreeMap<String, PanelUxOverrides>,
    /// Preferred top-level tab-bar row count.
    #[serde(default)]
    pub tab_layout: TabLayoutMode,
    /// Tab-bar overflow handling.
    #[serde(default)]
    pub tab_overflow: TabOverflowBehavior,
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
        tour_claims: Vec<String>,
        drift_ppm: f64,
        rtc_calibration: &RtcCalibrationSettings,
        line_limit: usize,
        tick_verbosity: String,
        component_profiles: &[BuildProfile],
        active_component_profile: usize,
        tab_layout: TabLayoutMode,
        tab_overflow: TabOverflowBehavior,
        persist_user_changes: bool,
        reset_test_session_on_compile: bool,
        fresh_test_executable_profile: bool,
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
            data_folder: default_data_folder(),
            output_dir,
            first_run,
            tour_claims,
            persist_user_changes,
            reset_test_session_on_compile,
            fresh_test_executable_profile,
            drift_ppm,
            rtc_calibration: rtc_calibration.clone(),
            line_limit,
            tick_verbosity,
            component_profiles: component_profiles.to_vec(),
            active_component_profile,
            board: default_board(),
            developer_mode: false,
            legacy_advanced_mode: false,
            panel_ux_overrides: BTreeMap::new(),
            tab_layout,
            tab_overflow,
        }
    }

    pub fn with_board(mut self, board: impl Into<String>) -> Self {
        self.board = board.into();
        self
    }

    pub fn with_developer_mode(mut self, developer_mode: bool) -> Self {
        self.developer_mode = developer_mode;
        self
    }

    pub fn with_panel_ux_overrides(
        mut self,
        panel_ux_overrides: BTreeMap<String, PanelUxOverrides>,
    ) -> Self {
        self.panel_ux_overrides = panel_ux_overrides;
        self
    }

    pub fn with_data_folder(mut self, data_folder: impl Into<String>) -> Self {
        self.data_folder = data_folder.into();
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
                || name.chars().any(|c| c.is_control())
                || !valid_ntp_host(host)
            {
                return Err("custom NTP server contains invalid or excessive text".into());
            }
        }
        if !self.sim_scale.is_finite() || !(0.5..=2.0).contains(&self.sim_scale) {
            return Err("simulator scale must be finite and between 0.5 and 2.0".into());
        }
        self.watch_config.validate()?;
        if self.watch_config.time_zone as usize >= TIMEZONE_OFFSETS.len() {
            return Err("time zone is out of range".into());
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
        if self.ntp_server >= sensor_watch_studio_ntp_server_count(&self.ntp_servers) {
            return Err("selected NTP server is out of range".into());
        }
        if !self.drift_ppm.is_finite() || self.drift_ppm.abs() > 1000.0 {
            return Err("drift correction must be finite and within +/-1000 ppm".into());
        }
        let current_calibration_version = sensor_watch_core::rtc_calibration::CALIBRATION_VERSION;
        if self.rtc_calibration.version != 0
            && self.rtc_calibration.version != current_calibration_version
        {
            return Err("unsupported RTC calibration version".into());
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
        let mut settings: Self = serde_json::from_str(json).map_err(|e| e.to_string())?;
        // Migrate the old persisted name only when the new field is absent.
        let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        if value.get("developer_mode").is_none() {
            settings.developer_mode = settings.legacy_advanced_mode;
        }
        // Compatibility migration for settings written before face identity
        // became case-insensitive. It is order-preserving and idempotent.
        settings.presets.migrate_face_duplicates();
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
            data_folder: default_data_folder(),
            output_dir: default_output_dir(),
            first_run: true,
            tour_claims: Vec::new(),
            persist_user_changes: true,
            reset_test_session_on_compile: true,
            fresh_test_executable_profile: true,
            drift_ppm: 0.0,
            rtc_calibration: RtcCalibrationSettings::default(),
            line_limit: default_line_limit(),
            tick_verbosity: default_tick_verbosity(),
            component_profiles: Vec::new(),
            active_component_profile: 0,
            board: default_board(),
            developer_mode: false,
            legacy_advanced_mode: false,
            panel_ux_overrides: BTreeMap::new(),
            tab_layout: TabLayoutMode::default(),
            tab_overflow: TabOverflowBehavior::default(),
        }
    }
}

/// The default maximum number of lines kept in each output log.
fn sensor_watch_studio_ntp_server_count(custom: &[(String, String)]) -> usize {
    super::ntp::SERVERS.len() + custom.len()
}

fn valid_ntp_host(host: &str) -> bool {
    let trimmed = host.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_SETTINGS_TEXT_BYTES
        || trimmed != host
        || trimmed.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return false;
    }
    // `query_ntp` appends the NTP port. Permit bracketed IPv6 literals, but
    // reject embedded ports/colon syntax that would create an invalid address.
    if trimmed.contains(':') {
        trimmed.starts_with('[') && trimmed.ends_with(']')
    } else {
        true
    }
}

pub fn default_true() -> bool {
    true
}

fn default_first_run() -> bool {
    true
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

pub fn default_data_folder() -> String {
    super::data_dir::default_path().display().to_string()
}

pub fn default_output_dir() -> String {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(|home| format!("{home}/Documents/FirmwareStudio"))
        .unwrap_or_else(|_| "FirmwareStudio".to_string())
}

#[cfg(test)]
mod tests {
    use super::{AppSettings, RtcCalibrationSettings};

    #[test]
    fn developer_mode_defaults_off_and_migrates_advanced_mode() {
        let defaults = AppSettings::default();
        assert!(!defaults.developer_mode);

        let mut value: serde_json::Value =
            serde_json::from_str(&defaults.to_json().unwrap()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("developer_mode");
        object.insert("advanced_mode".into(), serde_json::Value::Bool(true));
        let migrated = AppSettings::from_json(&value.to_string()).unwrap();
        assert!(migrated.developer_mode);
        assert!(!migrated.to_json().unwrap().contains("advanced_mode"));
    }

    #[test]
    fn panel_overrides_persist_and_default_safely() {
        let mut settings = AppSettings::default();
        let mut panel = settings.panel_ux("simulator");
        assert!(panel.tutorial_input_barrier);
        panel.tutorial_input_barrier = false;
        panel.tutorial_spotlight = false;
        settings.set_panel_ux("simulator", panel.clone());
        let loaded = AppSettings::from_json(&settings.to_json().unwrap()).unwrap();
        assert!(!loaded.panel_ux("simulator").tutorial_input_barrier);
        assert!(!loaded.panel_ux("simulator").tutorial_spotlight);
        assert!(loaded.panel_ux("editor").tutorial_input_barrier);
    }

    #[test]
    fn ux_overrides_cannot_affect_hard_safeguards() {
        assert!(!super::PanelUxOverrides::default().affects_hard_safeguards());
        assert_eq!(super::HARD_SAFEGUARD_BOUNDARIES.len(), 10);
    }

    #[test]
    fn rejects_unknown_rtc_calibration_versions() {
        let mut settings = AppSettings::default();
        settings.rtc_calibration.version = 255;

        let error = settings.validate().unwrap_err();
        assert!(error.contains("unsupported RTC calibration version"));
    }

    #[test]
    fn legacy_json_uses_true_defaults_for_studio_preferences() {
        let json = serde_json::to_string(&AppSettings::default()).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("persist_user_changes");
        value
            .as_object_mut()
            .unwrap()
            .remove("reset_test_session_on_compile");
        value
            .as_object_mut()
            .unwrap()
            .remove("fresh_test_executable_profile");
        let loaded = AppSettings::from_json(&value.to_string()).unwrap();
        assert!(loaded.persist_user_changes);
        assert!(loaded.reset_test_session_on_compile);
        assert!(loaded.fresh_test_executable_profile);
        assert!(loaded.tour_claims.is_empty());
    }

    #[test]
    fn studio_preferences_round_trip_false_and_true() {
        let mut settings = AppSettings::default();
        settings.persist_user_changes = false;
        settings.reset_test_session_on_compile = false;
        let loaded = AppSettings::from_json(&settings.to_json().unwrap()).unwrap();
        assert!(!loaded.persist_user_changes);
        assert!(!loaded.reset_test_session_on_compile);

        settings.persist_user_changes = true;
        settings.reset_test_session_on_compile = true;
        let loaded = AppSettings::from_json(&settings.to_json().unwrap()).unwrap();
        assert!(loaded.persist_user_changes);
        assert!(loaded.reset_test_session_on_compile);
    }

    #[test]
    fn capture_includes_studio_preferences() {
        let settings = AppSettings::capture(
            super::Language::English,
            super::Theme::Dark,
            &super::PresetManager::new(),
            0,
            &[],
            1.0,
            &super::WatchConfig::default(),
            1,
            0.0,
            0.0,
            &super::ModuleManager::default(),
            super::default_output_dir(),
            false,
            Vec::new(),
            0.0,
            &RtcCalibrationSettings::default(),
            500,
            "hide".to_string(),
            &[],
            0,
            super::TabLayoutMode::default(),
            super::TabOverflowBehavior::default(),
            false,
            true,
            false,
        );
        assert!(!settings.persist_user_changes);
        assert!(settings.reset_test_session_on_compile);
        assert!(!settings.fresh_test_executable_profile);
    }

    #[test]
    fn accepts_disabled_rtc_calibration_version_zero() {
        let settings = AppSettings {
            rtc_calibration: RtcCalibrationSettings::default(),
            ..AppSettings::default()
        };

        assert!(settings.validate().is_ok());
    }

    #[test]
    fn rejects_ntp_hosts_with_ports_or_whitespace() {
        let mut settings = AppSettings::default();
        settings.ntp_servers = vec![("bad".into(), "pool.ntp.org:9999".into())];
        assert!(settings.validate().is_err());

        settings.ntp_servers = vec![("bad".into(), " pool.ntp.org".into())];
        assert!(settings.validate().is_err());

        settings.ntp_servers = vec![("ipv6".into(), "[::1]".into())];
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn rejects_non_finite_calibration_values() {
        let mut settings = AppSettings::default();
        settings.rtc_calibration.version = sensor_watch_core::rtc_calibration::CALIBRATION_VERSION;
        settings.rtc_calibration.base_ppm = f32::NAN;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn rejects_invalid_timezone_index_from_json() {
        let mut settings = AppSettings::default();
        settings.watch_config.time_zone = 41;

        assert!(settings.validate().is_err());
        let json = settings.to_json().unwrap();
        assert!(AppSettings::from_json(&json).is_err());
    }

    #[test]
    fn accepts_last_valid_timezone_index_from_json() {
        let mut settings = AppSettings::default();
        settings.watch_config.time_zone = 40;

        let json = settings.to_json().unwrap();
        let imported = AppSettings::from_json(&json).unwrap();
        assert_eq!(imported.watch_config.time_zone, 40);
    }

    #[test]
    fn rejects_out_of_range_packed_watch_config_fields_from_json() {
        let fields = [
            ("to_interval", 4),
            ("le_interval", 8),
            ("led_duration", 8),
            ("led_red_color", 16),
            ("led_green_color", 16),
            ("buzzer_type", 4),
        ];

        for (field, value) in fields {
            let mut settings = AppSettings::default();
            match field {
                "to_interval" => settings.watch_config.to_interval = value,
                "le_interval" => settings.watch_config.le_interval = value,
                "led_duration" => settings.watch_config.led_duration = value,
                "led_red_color" => settings.watch_config.led_red_color = value,
                "led_green_color" => settings.watch_config.led_green_color = value,
                "buzzer_type" => settings.watch_config.buzzer_type = value,
                _ => unreachable!(),
            }

            let json = settings.to_json().unwrap();
            let error = AppSettings::from_json(&json).unwrap_err();
            assert!(error.contains(field), "{field}={value}: {error}");
        }
    }

    #[test]
    fn accepts_packed_watch_config_boundaries_from_json() {
        let mut settings = AppSettings::default();
        settings.watch_config.to_interval = 3;
        settings.watch_config.le_interval = 7;
        settings.watch_config.led_duration = 7;
        settings.watch_config.led_red_color = 15;
        settings.watch_config.led_green_color = 15;
        settings.watch_config.buzzer_type = 3;

        let json = settings.to_json().unwrap();
        let imported = AppSettings::from_json(&json).unwrap();
        assert_eq!(imported.watch_config.to_interval, 3);
        assert_eq!(imported.watch_config.le_interval, 7);
        assert_eq!(imported.watch_config.led_duration, 7);
        assert_eq!(imported.watch_config.led_red_color, 15);
        assert_eq!(imported.watch_config.led_green_color, 15);
        assert_eq!(imported.watch_config.buzzer_type, 3);
    }
}
