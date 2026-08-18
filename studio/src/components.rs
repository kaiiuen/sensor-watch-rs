//! Conservative hardware component and build-profile configuration.
//!
//! These profiles describe the intended hardware for review and validation. They
//! deliberately do not alter firmware build flags or pin mappings.

use egui::Ui;
use serde::{Deserialize, Serialize};

/// Studio's supported board identities. This is a model identity only; it does
/// not select firmware flags or pin mappings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(usize)]
pub enum BoardKind {
    Green,
    RedLite,
    Blue,
    Pro,
}

impl BoardKind {
    pub const ALL: [Self; 4] = [Self::Green, Self::RedLite, Self::Blue, Self::Pro];

    pub fn label(self) -> &'static str {
        match self {
            Self::Green => "Green",
            Self::RedLite => "Red / Lite",
            Self::Blue => "Blue",
            Self::Pro => "Pro",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityStatus {
    Verified,
    Documented,
    Unknown,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardCapabilities {
    pub lcd: CapabilityStatus,
    pub red_led: CapabilityStatus,
    pub green_led: CapabilityStatus,
    pub blue_led: CapabilityStatus,
    pub rgb_led: CapabilityStatus,
    pub thermistor: CapabilityStatus,
    pub accelerometer: CapabilityStatus,
    pub light_sensor: CapabilityStatus,
    pub buzzer: CapabilityStatus,
    pub i2c: CapabilityStatus,
    pub spi: CapabilityStatus,
    pub uart: CapabilityStatus,
}

/// Authoritative Studio capability table. Unknown is intentional: it prevents
/// a planning profile from turning an unverified component into a build claim.
pub const CAPABILITY_TABLE: [(BoardKind, BoardCapabilities); 4] = [
    (
        BoardKind::Green,
        BoardCapabilities {
            lcd: CapabilityStatus::Verified,
            red_led: CapabilityStatus::Verified,
            green_led: CapabilityStatus::Verified,
            blue_led: CapabilityStatus::Unsupported,
            rgb_led: CapabilityStatus::Verified,
            thermistor: CapabilityStatus::Unknown,
            accelerometer: CapabilityStatus::Unknown,
            light_sensor: CapabilityStatus::Verified,
            buzzer: CapabilityStatus::Verified,
            i2c: CapabilityStatus::Documented,
            spi: CapabilityStatus::Documented,
            uart: CapabilityStatus::Documented,
        },
    ),
    (
        BoardKind::RedLite,
        BoardCapabilities {
            lcd: CapabilityStatus::Verified,
            red_led: CapabilityStatus::Verified,
            green_led: CapabilityStatus::Verified,
            blue_led: CapabilityStatus::Unsupported,
            rgb_led: CapabilityStatus::Unsupported,
            thermistor: CapabilityStatus::Unknown,
            accelerometer: CapabilityStatus::Unknown,
            light_sensor: CapabilityStatus::Unsupported,
            buzzer: CapabilityStatus::Verified,
            i2c: CapabilityStatus::Unsupported,
            spi: CapabilityStatus::Unsupported,
            uart: CapabilityStatus::Verified,
        },
    ),
    (
        BoardKind::Blue,
        BoardCapabilities {
            lcd: CapabilityStatus::Verified,
            red_led: CapabilityStatus::Verified,
            green_led: CapabilityStatus::Unsupported,
            blue_led: CapabilityStatus::Verified,
            rgb_led: CapabilityStatus::Unsupported,
            thermistor: CapabilityStatus::Unknown,
            accelerometer: CapabilityStatus::Unknown,
            light_sensor: CapabilityStatus::Unknown,
            buzzer: CapabilityStatus::Verified,
            i2c: CapabilityStatus::Documented,
            spi: CapabilityStatus::Documented,
            uart: CapabilityStatus::Documented,
        },
    ),
    (
        BoardKind::Pro,
        BoardCapabilities {
            lcd: CapabilityStatus::Verified,
            red_led: CapabilityStatus::Verified,
            green_led: CapabilityStatus::Verified,
            blue_led: CapabilityStatus::Unsupported,
            rgb_led: CapabilityStatus::Unsupported,
            thermistor: CapabilityStatus::Unknown,
            accelerometer: CapabilityStatus::Unknown,
            light_sensor: CapabilityStatus::Unknown,
            buzzer: CapabilityStatus::Documented,
            i2c: CapabilityStatus::Documented,
            spi: CapabilityStatus::Documented,
            uart: CapabilityStatus::Documented,
        },
    ),
];

pub fn capabilities(board: BoardKind) -> BoardCapabilities {
    CAPABILITY_TABLE[board as usize].1
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LcdVariant {
    #[default]
    Standard,
    OsoAccessory,
    Custom,
}

impl LcdVariant {
    pub const ALL: [Self; 3] = [Self::Standard, Self::OsoAccessory, Self::Custom];

    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Original F-91W / Sensor Watch LCD",
            Self::OsoAccessory => "OSO BU9796 custom LCD",
            Self::Custom => "Custom LCD",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        capabilities, capability_chart_rows, effective_config, resolve_conflict,
        validate_compatibility, BoardKind, BuildProfile, CapabilityStatus, CompatibilitySeverity,
        ComponentsConfig, ConflictResolution, LcdVariant,
    };

    #[test]
    fn lcd_variant_labels_are_stable_and_non_empty() {
        let expected = [
            (LcdVariant::Standard, "Original F-91W / Sensor Watch LCD"),
            (LcdVariant::OsoAccessory, "OSO BU9796 custom LCD"),
            (LcdVariant::Custom, "Custom LCD"),
        ];

        for (variant, label) in expected {
            assert_eq!(variant.label(), label);
            assert!(!label.is_empty());
        }

        let standard_label = LcdVariant::Standard.label();
        assert!(standard_label.contains("F-91W"));
        assert!(standard_label.contains("Sensor Watch"));
    }

    fn issues(board: BoardKind, config: ComponentsConfig) -> super::CompatibilityResult {
        let profile = BuildProfile::new("test", config.clone());
        validate_compatibility(board, &profile, &config)
    }

    #[test]
    fn red_lite_rejects_requested_light_sensor() {
        let config = ComponentsConfig {
            light_sensor: true,
            ..Default::default()
        };
        let result = issues(BoardKind::RedLite, config);
        let light_sensor_issue = result
            .iter()
            .find(|item| item.component == "light sensor")
            .unwrap();
        assert_eq!(light_sensor_issue.severity, CompatibilitySeverity::Error);
        assert!(light_sensor_issue
            .reason
            .contains("documents no light sensor"));
        assert!(light_sensor_issue
            .suggested_action
            .contains("Disable the light sensor"));
    }

    #[test]
    fn rgb_matches_board_led_facts() {
        let config = ComponentsConfig {
            rgb_led: true,
            ..Default::default()
        };
        let blue = issues(BoardKind::Blue, config.clone());
        let blue_rgb_issue = blue
            .iter()
            .find(|item| item.component == "RGB LED")
            .unwrap();
        assert_eq!(blue_rgb_issue.severity, CompatibilitySeverity::Error);
        assert!(blue_rgb_issue.reason.contains("unsupported"));
        assert!(blue_rgb_issue.suggested_action.contains("Disable RGB"));
        assert_eq!(
            capabilities(BoardKind::Blue).rgb_led,
            CapabilityStatus::Unsupported
        );

        let pro = issues(BoardKind::Pro, config.clone());
        let rgb_issue = pro.iter().find(|item| item.component == "RGB LED").unwrap();
        assert_eq!(rgb_issue.severity, CompatibilitySeverity::Error);
        assert!(rgb_issue.reason.contains("unsupported"));
        assert_eq!(
            capabilities(BoardKind::Pro).rgb_led,
            CapabilityStatus::Unsupported
        );

        assert!(issues(BoardKind::Green, config)
            .iter()
            .all(|item| item.component != "RGB LED"));
    }

    #[test]
    fn custom_lcd_requires_a_declared_bus() {
        let config = ComponentsConfig {
            lcd: LcdVariant::Custom,
            ..Default::default()
        };
        let result = issues(BoardKind::Green, config);
        assert!(result.iter().any(|item| {
            item.component == "LCD" && item.severity == CompatibilitySeverity::Error
        }));
    }

    #[test]
    fn standard_profile_is_compatible_with_green() {
        let config = ComponentsConfig::default();
        assert!(issues(BoardKind::Green, config).is_empty());
    }

    #[test]
    fn lite_rejects_unexposed_buses() {
        let config = ComponentsConfig {
            i2c: true,
            spi: true,
            ..Default::default()
        };
        let result = issues(BoardKind::RedLite, config);
        assert!(result
            .iter()
            .any(|item| item.component == "I2C" && item.severity == CompatibilitySeverity::Error));
        assert!(result
            .iter()
            .any(|item| item.component == "SPI" && item.severity == CompatibilitySeverity::Error));
    }

    #[test]
    fn oso_lcd_requires_pro_i2c_while_classic_lcd_does_not() {
        let config = ComponentsConfig {
            lcd: LcdVariant::OsoAccessory,
            ..Default::default()
        };
        let red_lite = issues(BoardKind::RedLite, config.clone());
        assert!(red_lite
            .iter()
            .any(|item| item.component == "LCD"
                && item.reason.contains("only documented for the Pro")));
        assert!(red_lite
            .iter()
            .any(|item| item.component == "LCD" && item.reason.contains("requires I2C")));
        assert!(issues(BoardKind::Pro, config)
            .iter()
            .any(|item| item.reason.contains("requires I2C")));
    }

    #[test]
    fn uart_conflict_is_warning_only_when_multiplexed() {
        let config = ComponentsConfig {
            uart_shell: true,
            ..Default::default()
        };
        let pro = issues(BoardKind::Pro, config.clone());
        assert!(pro.iter().any(|item| {
            item.component == "UART" && item.severity == CompatibilitySeverity::Warning
        }));
        assert!(!issues(BoardKind::RedLite, config)
            .iter()
            .any(|item| item.component == "UART"));
    }

    #[test]
    fn capability_chart_lists_evidence_status_without_overclaiming() {
        let rows = capability_chart_rows();
        assert_eq!(rows.len(), 4);
        for board in ["Green", "Red / Lite", "Blue", "Pro"] {
            assert!(rows.iter().any(|row| row.board == board));
        }
        let green = rows.iter().find(|row| row.board == "Green").unwrap();
        assert_eq!(green.light_sensor, "Onboard IR light sensor");
        assert_eq!(green.led, "RGB (red/green/blue)");
        let red_lite = rows.iter().find(|row| row.board == "Red / Lite").unwrap();
        assert_eq!(red_lite.light_sensor, "No onboard sensor");
        assert_eq!(red_lite.led, "Red + green");
        assert!(red_lite.buses.contains("I2C/SPI not exposed"));
        let blue = rows.iter().find(|row| row.board == "Blue").unwrap();
        assert_eq!(blue.led, "Red + blue");
        let pro = rows.iter().find(|row| row.board == "Pro").unwrap();
        assert_eq!(pro.light_sensor, "Unknown / revision-dependent");
        assert_eq!(pro.led, "Red + green");
        assert!(pro.lcd.contains("Classic LCD") && pro.lcd.contains("OSO BU9796"));
        assert!(!pro.led.contains("RGB, 3"));
        assert!(rows.iter().all(|row| !row.lcd.is_empty()
            && !row.light_sensor.is_empty()
            && !row.verification.is_empty()));
    }

    #[test]
    fn board_change_conflict_is_deterministic() {
        let requested = ComponentsConfig {
            light_sensor: true,
            ..Default::default()
        };
        let profile = BuildProfile::new("Green", requested.clone());
        let issues = validate_compatibility(BoardKind::RedLite, &profile, &requested);
        assert!(issues.iter().any(|issue| issue.component == "light sensor"));
    }

    #[test]
    fn profile_on_wrong_board_reports_conflict() {
        let requested = ComponentsConfig {
            rgb_led: true,
            ..Default::default()
        };
        let profile = BuildProfile::new("Pro", requested.clone());
        let issues = validate_compatibility(BoardKind::Blue, &profile, &requested);
        assert!(issues.iter().any(|issue| issue.component == "RGB LED"));
    }

    #[test]
    fn conflict_cancel_keeps_no_effect_and_keep_preserves_requested_state() {
        let requested = ComponentsConfig {
            light_sensor: true,
            ..Default::default()
        };
        let profile = BuildProfile::new("Red / Lite", requested.clone());
        assert_eq!(
            resolve_conflict(
                ConflictResolution::Cancel,
                BoardKind::RedLite,
                &profile,
                &requested
            ),
            None
        );
        assert_eq!(
            resolve_conflict(
                ConflictResolution::KeepRequested,
                BoardKind::RedLite,
                &profile,
                &requested
            ),
            Some(requested)
        );
    }

    #[test]
    fn effective_state_disables_only_errors_and_preserves_requested_state() {
        let requested = ComponentsConfig {
            light_sensor: true,
            rgb_led: true,
            ..Default::default()
        };
        let profile = BuildProfile::new("test", requested.clone());
        let effective = effective_config(BoardKind::RedLite, &profile, &requested);
        assert!(!effective.light_sensor);
        assert!(!effective.rgb_led);
        assert!(requested.light_sensor);
        assert!(requested.rgb_led);
    }

    #[test]
    fn unknown_capabilities_warn_without_changing_requested_state() {
        assert_eq!(
            capabilities(BoardKind::Green).light_sensor,
            CapabilityStatus::Verified
        );
        assert_eq!(
            capabilities(BoardKind::Blue).light_sensor,
            CapabilityStatus::Unknown
        );
        assert_eq!(
            capabilities(BoardKind::Pro).light_sensor,
            CapabilityStatus::Unknown
        );

        let config = ComponentsConfig {
            accelerometer: true,
            thermistor: true,
            light_sensor: true,
            ..Default::default()
        };
        let result = issues(BoardKind::Blue, config.clone());
        assert!(result.iter().any(|item| {
            item.component == "accelerometer" && item.severity == CompatibilitySeverity::Warning
        }));
        assert!(result.iter().any(|item| {
            item.component == "light sensor" && item.severity == CompatibilitySeverity::Warning
        }));
        assert!(config.light_sensor);
        assert!(config.thermistor);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentsConfig {
    pub lcd: LcdVariant,
    pub accelerometer: bool,
    pub light_sensor: bool,
    pub thermistor: bool,
    pub buzzer: bool,
    pub led: bool,
    #[serde(default)]
    pub rgb_led: bool,
    pub uart_shell: bool,
    pub gpio: bool,
    pub spi: bool,
    pub i2c: bool,
}

impl Default for ComponentsConfig {
    fn default() -> Self {
        Self {
            lcd: LcdVariant::Standard,
            accelerometer: false,
            light_sensor: false,
            thermistor: false,
            buzzer: true,
            led: true,
            rgb_led: false,
            uart_shell: false,
            gpio: false,
            spi: false,
            i2c: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildProfile {
    pub name: String,
    pub config: ComponentsConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompatibilitySeverity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompatibilityIssue {
    pub component: String,
    pub severity: CompatibilitySeverity,
    pub reason: String,
    pub suggested_action: String,
}

pub type CompatibilityResult = Vec<CompatibilityIssue>;

/// A stable, UI-ready row for the board capability chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityChartRow {
    pub board: &'static str,
    pub lcd: &'static str,
    pub led: &'static str,
    pub light_sensor: &'static str,
    pub thermistor: &'static str,
    pub accelerometer: &'static str,
    pub buzzer: &'static str,
    pub buses: &'static str,
    pub uart: &'static str,
    pub confidence: &'static str,
    pub source: &'static str,
    pub revision_notes: &'static str,
    pub verification: &'static str,
}

pub const CAPABILITY_CHART: [CapabilityChartRow; 4] = [
    CapabilityChartRow {
        board: "Green",
        lcd: "Classic LCD",
        led: "RGB (red/green/blue)",
        light_sensor: "Onboard IR light sensor",
        thermistor: "Board/revision-dependent",
        accelerometer: "Accessory/optional; not built in",
        buzzer: "Piezo",
        buses: "I2C/SPI available; pin mapping revision-dependent",
        uart: "Multiplexed; not dedicated",
        confidence: "High for RGB/IR; medium for buses",
        source: "Official maintained pin evidence; community IR evidence",
        revision_notes: "Do not generalize across commercial revisions",
        verification: "Core LED/LCD evidence maintained; fitted sensors remain revision-sensitive",
    },
    CapabilityChartRow {
        board: "Red / Lite",
        lcd: "Classic LCD",
        led: "Red + green",
        light_sensor: "No onboard sensor",
        thermistor: "Board/revision-dependent",
        accelerometer: "Accessory/optional; not built in",
        buzzer: "Piezo",
        buses: "I2C/SPI not exposed on current Lite pins",
        uart: "Dedicated UART",
        confidence: "High for current Lite pinout; medium for revisions",
        source: "Official maintained pin evidence; community board evidence",
        revision_notes: "Statement is limited to the current Lite pinout",
        verification: "No inferred buses or sensors beyond the documented current pins",
    },
    CapabilityChartRow {
        board: "Blue",
        lcd: "Classic LCD",
        led: "Red + blue",
        light_sensor: "Unknown / revision-dependent",
        thermistor: "Board/revision-dependent",
        accelerometer: "Accessory/optional; not built in",
        buzzer: "Piezo",
        buses: "I2C/SPI available; pin mapping revision-dependent",
        uart: "Multiplexed; not dedicated",
        confidence: "High for red/blue; low for fitted sensors",
        source: "Official maintained pin evidence; community LED evidence",
        revision_notes: "Thermistor and accessory population may vary",
        verification: "LED colors corrected; sensor fit remains unknown",
    },
    CapabilityChartRow {
        board: "Pro",
        lcd: "Classic LCD or OSO BU9796 custom LCD",
        led: "Red + green",
        light_sensor: "Unknown / revision-dependent",
        thermistor: "Board/revision-dependent",
        accelerometer: "Accessory/optional; not built in",
        buzzer: "Timed driver",
        buses: "I2C/SPI available; OSO BU9796 requires I2C",
        uart: "Multiplexed; not dedicated",
        confidence: "High for red/green and OSO requirement; medium overall",
        source: "Official maintained pin evidence; community OSO/BU9796 evidence",
        revision_notes: "Classic/OSO fit and sensors are not identical on every revision",
        verification: "OSO is compatible only with a board exposing the required I2C",
    },
];

pub fn capability_chart_rows() -> &'static [CapabilityChartRow; 4] {
    &CAPABILITY_CHART
}

fn issue(
    component: &str,
    severity: CompatibilitySeverity,
    reason: &str,
    suggested_action: &str,
) -> CompatibilityIssue {
    CompatibilityIssue {
        component: component.into(),
        severity,
        reason: reason.into(),
        suggested_action: suggested_action.into(),
    }
}

/// Pure compatibility validation for a board, profile, and requested draft.
/// Requested state is never changed; callers decide how to present results.
pub fn validate_compatibility(
    board: BoardKind,
    profile: &BuildProfile,
    config: &ComponentsConfig,
) -> CompatibilityResult {
    let mut issues = Vec::new();
    if let Err(reason) = profile.validate() {
        issues.push(issue(
            "profile",
            CompatibilitySeverity::Error,
            &reason,
            "Correct the profile before saving or building",
        ));
    }

    let caps = capabilities(board);
    if config.lcd == LcdVariant::OsoAccessory && board != BoardKind::Pro {
        issues.push(issue(
            "LCD",
            CompatibilitySeverity::Error,
            "the OSO BU9796 custom LCD is only documented for the Pro board",
            "Select Pro or use the classic LCD",
        ));
    }
    if config.lcd == LcdVariant::OsoAccessory && !config.i2c {
        issues.push(issue(
            "LCD",
            CompatibilitySeverity::Error,
            "the OSO BU9796 custom LCD requires I2C",
            "Enable I2C and confirm the OSO pin/address mapping",
        ));
    }
    if config.lcd == LcdVariant::Custom && !(config.spi || config.i2c) {
        issues.push(issue(
            "LCD",
            CompatibilitySeverity::Error,
            "custom LCD has no declared SPI or I2C bus requirement",
            "Enable the bus required by the custom LCD",
        ));
    }
    if config.i2c && matches!(caps.i2c, CapabilityStatus::Unsupported) {
        issues.push(issue(
            "I2C",
            CompatibilitySeverity::Error,
            "I2C is not exposed on the current Lite pins",
            "Disable I2C or select a board/revision with exposed I2C",
        ));
    }
    if config.spi && matches!(caps.spi, CapabilityStatus::Unsupported) {
        issues.push(issue(
            "SPI",
            CompatibilitySeverity::Error,
            "SPI is not exposed on the current Lite pins",
            "Disable SPI or select a board/revision with exposed SPI",
        ));
    }
    if config.uart_shell && matches!(caps.uart, CapabilityStatus::Documented) {
        issues.push(issue(
            "UART",
            CompatibilitySeverity::Warning,
            "UART is multiplexed on this board and may conflict with other functions",
            "Confirm the revision-specific UART pin ownership before building",
        ));
    }
    if config.light_sensor && matches!(caps.light_sensor, CapabilityStatus::Unsupported) {
        issues.push(issue(
            "light sensor",
            CompatibilitySeverity::Error,
            "this board documents no light sensor",
            "Disable the light sensor or select a board with documented hardware",
        ));
    } else if config.light_sensor && matches!(caps.light_sensor, CapabilityStatus::Unknown) {
        issues.push(issue(
            "light sensor",
            CompatibilitySeverity::Warning,
            "light-sensor capability is not established for this board",
            "Confirm the sensor, address, power, and pin mapping before building",
        ));
    }
    if config.thermistor {
        match caps.thermistor {
            CapabilityStatus::Unsupported => issues.push(issue(
                "thermistor",
                CompatibilitySeverity::Error,
                "thermistor is unsupported on this board",
                "Disable the thermistor or select a compatible board",
            )),
            CapabilityStatus::Unknown => issues.push(issue(
                "thermistor",
                CompatibilitySeverity::Warning,
                "thermistor capability is unknown for this board",
                "Confirm the sensor and analog pin mapping before building",
            )),
            _ => {}
        }
    }
    if config.accelerometer && matches!(caps.accelerometer, CapabilityStatus::Unknown) {
        issues.push(issue(
            "accelerometer",
            CompatibilitySeverity::Warning,
            "accelerometer capability is unknown for this board",
            "Confirm the accelerometer, bus, address, and power mapping before building",
        ));
    }
    if config.rgb_led {
        match caps.rgb_led {
            CapabilityStatus::Unsupported => issues.push(issue(
                "RGB LED",
                CompatibilitySeverity::Error,
                "RGB LED channels are unsupported on this board",
                "Disable RGB or select the Pro board",
            )),
            CapabilityStatus::Unknown => issues.push(issue(
                "RGB LED",
                CompatibilitySeverity::Warning,
                "RGB LED is not buildable from current evidence: firmware proves only bi-color red/green and ignores blue",
                "Do not build RGB; confirm a blue channel implementation or select a verified bi-color configuration",
            )),
            _ => {}
        }
    }
    if config.led
        && matches!(
            caps.red_led,
            CapabilityStatus::Unknown | CapabilityStatus::Unsupported
        )
    {
        issues.push(issue(
            "LED",
            CompatibilitySeverity::Warning,
            "requested LED channel capability is not fully established",
            "Confirm the available LED channel and polarity before building",
        ));
    }
    issues
}

/// Resolve a requested draft into the fail-closed effective state. Requested
/// values are never modified; only explicitly incompatible options are disabled.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictResolution {
    Cancel,
    KeepRequested,
    DisableIncompatible,
}

pub fn resolve_conflict(
    choice: ConflictResolution,
    board: BoardKind,
    profile: &BuildProfile,
    requested: &ComponentsConfig,
) -> Option<ComponentsConfig> {
    match choice {
        ConflictResolution::Cancel => None,
        ConflictResolution::KeepRequested => Some(requested.clone()),
        ConflictResolution::DisableIncompatible => {
            Some(effective_config(board, profile, requested))
        }
    }
}

pub fn effective_config(
    board: BoardKind,
    profile: &BuildProfile,
    requested: &ComponentsConfig,
) -> ComponentsConfig {
    let mut effective = requested.clone();
    for finding in validate_compatibility(board, profile, requested) {
        if finding.severity != CompatibilitySeverity::Error {
            continue;
        }
        match finding.component.as_str() {
            "LCD" => effective.lcd = LcdVariant::Standard,
            "light sensor" => effective.light_sensor = false,
            "thermistor" => effective.thermistor = false,
            "accelerometer" => effective.accelerometer = false,
            "RGB LED" => effective.rgb_led = false,
            "I2C" => effective.i2c = false,
            "SPI" => effective.spi = false,
            _ => {}
        }
    }
    effective
}

impl BuildProfile {
    pub fn validate(&self) -> Result<(), String> {
        let name = self.name.trim();
        if name.is_empty() || name.len() > 64 {
            return Err("profile name must be 1-64 characters".to_string());
        }
        if self.config.lcd == LcdVariant::Custom && !(self.config.spi || self.config.i2c) {
            return Err("custom LCD requires SPI or I2C to be enabled".to_string());
        }
        Ok(())
    }

    pub fn new(name: impl Into<String>, config: ComponentsConfig) -> Self {
        Self {
            name: name.into(),
            config,
        }
    }
}

pub fn default_profiles() -> Vec<BuildProfile> {
    vec![
        BuildProfile::new(
            "Green",
            ComponentsConfig {
                buzzer: true,
                led: true,
                ..Default::default()
            },
        ),
        BuildProfile::new(
            "Red / Lite",
            ComponentsConfig {
                light_sensor: false,
                buzzer: true,
                led: true,
                ..Default::default()
            },
        ),
        BuildProfile::new(
            "Blue",
            ComponentsConfig {
                thermistor: false,
                buzzer: true,
                led: true,
                ..Default::default()
            },
        ),
        BuildProfile::new(
            "Pro",
            ComponentsConfig {
                thermistor: false,
                accelerometer: false,
                buzzer: true,
                led: true,
                ..Default::default()
            },
        ),
        BuildProfile::new("Custom", ComponentsConfig::default()),
    ]
}

pub fn selected_config(profiles: &[BuildProfile], selected: usize) -> ComponentsConfig {
    profiles
        .get(selected)
        .map(|p| p.config.clone())
        .unwrap_or_default()
}

pub fn estimate(config: &ComponentsConfig) -> (u32, u32) {
    // Planning estimates only: they are not linker measurements.
    let flash = 2
        + config.accelerometer as u32 * 5
        + config.light_sensor as u32 * 3
        + config.thermistor as u32 * 2
        + config.buzzer as u32
        + config.led as u32
        + config.uart_shell as u32 * 8
        + config.gpio as u32
        + config.spi as u32 * 2
        + config.i2c as u32 * 2;
    let ram = 8
        + config.accelerometer as u32 * 4
        + config.light_sensor as u32 * 2
        + config.thermistor as u32 * 2
        + config.uart_shell as u32 * 6
        + config.gpio as u32
        + config.spi as u32 * 2
        + config.i2c as u32 * 2;
    (flash, ram)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileSelection {
    pub index: usize,
    pub config: ComponentsConfig,
    pub issues: CompatibilityResult,
}

/// Renders the profile editor and returns whether the current draft changed,
/// plus a profile selection request that the owner must confirm if needed.
pub fn show_configurator(
    ui: &mut Ui,
    board: BoardKind,
    profiles: &mut Vec<BuildProfile>,
    selected: &mut usize,
    draft: &mut ComponentsConfig,
) -> (bool, Option<ProfileSelection>) {
    let mut changed = false;
    let mut profile_selection = None;
    ui.strong("Components / Build Profile");
    ui.label("Describe a custom Sensor Watch board or OSO accessory LCD/sensor board.");
    ui.colored_label(
        egui::Color32::from_rgb(220, 160, 80),
        "UF2 build disabled: this profile is planning data only; it is not a firmware build input.",
    );
    ui.collapsing("Missing Studio-to-firmware input contract", |ui| {
        ui.label("A configured build cannot be published until all of these are supplied:");
        for input in crate::build::missing_configuration_inputs() {
            ui.label(format!("• {input}"));
        }
    });
    ui.horizontal(|ui| {
        ui.label("Profile:");
        let current = profiles
            .get(*selected)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Unsaved".to_string());
        egui::ComboBox::from_id_source("component_profile_select")
            .selected_text(&current)
            .show_ui(ui, |ui| {
                for (index, profile) in profiles.iter().enumerate() {
                    if ui
                        .selectable_label(*selected == index, &profile.name)
                        .clicked()
                    {
                        let config = profile.config.clone();
                        let draft_profile = BuildProfile::new(profile.name.clone(), config.clone());
                        let issues = validate_compatibility(board, &draft_profile, &config);
                        profile_selection = Some(ProfileSelection {
                            index,
                            config,
                            issues,
                        });
                    }
                }
            });
        if ui
            .button("Save")
            .on_hover_text("Save the current component choices to this named profile")
            .clicked()
        {
            if let Some(profile) = profiles.get_mut(*selected) {
                let candidate = BuildProfile::new(profile.name.clone(), draft.clone());
                if let Err(error) = candidate.validate() {
                    ui.colored_label(egui::Color32::RED, format!("Error: {error}"));
                } else {
                    profile.config = draft.clone();
                }
            }
        }
        if ui.button("Duplicate").clicked() {
            let mut copy = BuildProfile::new(format!("{} copy", current), draft.clone());
            if profiles.iter().any(|p| p.name == copy.name) {
                copy.name.push_str(" 2");
            }
            profiles.push(copy);
            *selected = profiles.len() - 1;
        }
        if profiles.len() > 1 && ui.button("Delete").clicked() {
            profiles.remove(*selected);
            *selected = (*selected).min(profiles.len() - 1);
            *draft = profiles[*selected].config.clone();
        }
    });
    ui.horizontal(|ui| {
        ui.label("LCD (requested):");
        egui::ComboBox::from_id_source("component_lcd_variant")
            .selected_text(draft.lcd.label())
            .show_ui(ui, |ui| {
                for lcd in LcdVariant::ALL {
                    if ui
                        .selectable_value(&mut draft.lcd, lcd, lcd.label())
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
    });
    ui.horizontal_wrapped(|ui| {
        let profile = BuildProfile::new("draft", draft.clone());
        let findings = validate_compatibility(board, &profile, draft);
        for (label, component, value) in [
            ("Accelerometer", "accelerometer", &mut draft.accelerometer),
            ("Light sensor", "light sensor", &mut draft.light_sensor),
            ("Thermistor", "thermistor", &mut draft.thermistor),
            ("Buzzer", "buzzer", &mut draft.buzzer),
            ("LED", "LED", &mut draft.led),
            ("RGB LED", "RGB LED", &mut draft.rgb_led),
            ("UART shell", "UART shell", &mut draft.uart_shell),
            ("Optional GPIO", "GPIO", &mut draft.gpio),
            ("Optional SPI", "SPI", &mut draft.spi),
            ("Optional I2C", "I2C", &mut draft.i2c),
        ] {
            let reason = findings
                .iter()
                .find(|finding| {
                    finding.component == component
                        && finding.severity == CompatibilitySeverity::Error
                })
                .map(|finding| finding.reason.as_str());
            let response = if let Some(reason) = reason {
                ui.add_enabled(
                    false,
                    egui::Checkbox::new(value, format!("{label} — unavailable: {reason}")),
                )
            } else {
                ui.checkbox(value, label)
            };
            if response.changed() {
                changed = true;
            }
        }
    });
    let effective = effective_config(board, &BuildProfile::new("draft", draft.clone()), draft);
    ui.label("Requested selections are preserved; effective state is fail-closed:");
    ui.monospace(format!("Effective: LCD={}, accelerometer={}, light sensor={}, thermistor={}, buzzer={}, LED={}, RGB LED={}", effective.lcd.label(), effective.accelerometer, effective.light_sensor, effective.thermistor, effective.buzzer, effective.led, effective.rgb_led));
    let (flash, ram) = estimate(&effective);
    let draft_profile = BuildProfile::new("draft", draft.clone());
    if let Err(error) = draft_profile.validate() {
        ui.colored_label(egui::Color32::RED, format!("Error: {error}"));
    }
    for finding in validate_compatibility(board, &draft_profile, draft) {
        let color = match finding.severity {
            CompatibilitySeverity::Error => egui::Color32::RED,
            CompatibilitySeverity::Warning => egui::Color32::from_rgb(230, 170, 70),
        };
        ui.colored_label(
            color,
            format!(
                "{:?}: {} — {}",
                finding.severity, finding.reason, finding.suggested_action
            ),
        );
    }
    ui.monospace(format!(
        "Estimated component impact: +{flash} KiB flash, +{ram} KiB RAM (planning estimate)"
    ));
    let mut warnings = Vec::new();
    if draft.lcd == LcdVariant::OsoAccessory && !(draft.spi || draft.i2c) {
        warnings.push("OSO accessory LCD usually needs SPI or I2C enabled; verify the registered module interface.");
    }
    if draft.light_sensor && draft.lcd == LcdVariant::OsoAccessory {
        warnings
            .push("Confirm the OSO accessory light-sensor address and power budget before use.");
    }
    if draft.thermistor && draft.i2c {
        warnings.push("Thermistor is commonly analog; enabling I2C does not provide an automatic analog pin mapping.");
    }
    for warning in warnings {
        ui.colored_label(
            egui::Color32::from_rgb(230, 170, 70),
            format!("Warning: {warning}"),
        );
    }
    ui.weak("Profile edits remain available for planning and review. Studio does not infer pins, buses, addresses, power sequencing, or firmware modules from these choices; the build preflight therefore refuses to generate a configured UF2.");
    (changed, profile_selection)
}
