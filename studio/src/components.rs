//! Conservative hardware component and build-profile configuration.
//!
//! These profiles describe the intended hardware for review and validation. They
//! deliberately do not alter firmware build flags or pin mappings.

use egui::Ui;

use crate::theme::semantic;
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

/// The only supported buzzer drive modes. Boosted output is deliberately tied
/// to a validated board revision rather than inferred from a product label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuzzerDrive {
    BatteryLevel,
    Boosted9V,
}

impl BuzzerDrive {
    pub fn label(self) -> &'static str {
        match self {
            Self::BatteryLevel => "battery-level/unboosted",
            Self::Boosted9V => "9 V boosted/amplified",
        }
    }
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
    pub buzzer_drive: BuzzerDrive,
    pub i2c: CapabilityStatus,
    pub spi: CapabilityStatus,
    pub uart: CapabilityStatus,
    pub usb_uf2: CapabilityStatus,
    pub sensor_connector: CapabilityStatus,
}

/// Authoritative Studio capability table. Unknown is intentional: it prevents
/// a planning profile from turning an unverified component into a build claim.
pub const CAPABILITY_TABLE: [(BoardKind, BoardCapabilities); 4] = [
    (
        BoardKind::Green,
        BoardCapabilities {
            buzzer_drive: BuzzerDrive::BatteryLevel,
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
            usb_uf2: CapabilityStatus::Unknown,
            sensor_connector: CapabilityStatus::Unknown,
        },
    ),
    (
        BoardKind::RedLite,
        BoardCapabilities {
            buzzer_drive: BuzzerDrive::BatteryLevel,
            lcd: CapabilityStatus::Verified,
            red_led: CapabilityStatus::Verified,
            green_led: CapabilityStatus::Verified,
            blue_led: CapabilityStatus::Unsupported,
            rgb_led: CapabilityStatus::Unsupported,
            thermistor: CapabilityStatus::Documented,
            accelerometer: CapabilityStatus::Unsupported,
            light_sensor: CapabilityStatus::Unsupported,
            buzzer: CapabilityStatus::Documented,
            i2c: CapabilityStatus::Unsupported,
            spi: CapabilityStatus::Unsupported,
            uart: CapabilityStatus::Documented,
            usb_uf2: CapabilityStatus::Documented,
            sensor_connector: CapabilityStatus::Unsupported,
        },
    ),
    (
        BoardKind::Blue,
        BoardCapabilities {
            buzzer_drive: BuzzerDrive::BatteryLevel,
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
            usb_uf2: CapabilityStatus::Unknown,
            sensor_connector: CapabilityStatus::Unknown,
        },
    ),
    (
        BoardKind::Pro,
        BoardCapabilities {
            buzzer_drive: BuzzerDrive::Boosted9V,
            lcd: CapabilityStatus::Documented,
            red_led: CapabilityStatus::Documented,
            green_led: CapabilityStatus::Documented,
            blue_led: CapabilityStatus::Documented,
            rgb_led: CapabilityStatus::Documented,
            thermistor: CapabilityStatus::Documented,
            accelerometer: CapabilityStatus::Documented,
            light_sensor: CapabilityStatus::Documented,
            buzzer: CapabilityStatus::Documented,
            i2c: CapabilityStatus::Documented,
            spi: CapabilityStatus::Documented,
            uart: CapabilityStatus::Documented,
            usb_uf2: CapabilityStatus::Documented,
            sensor_connector: CapabilityStatus::Documented,
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
        capabilities, capability_chart_cells, capability_chart_rows, default_profiles,
        effective_config, is_unedited_stock_selection, resolve_conflict, stock_profile_config,
        stock_profile_index, validate_compatibility, BoardKind, BuildProfile, BuzzerDrive,
        CapabilityStatus, CompatibilitySeverity, ComponentsConfig, ConflictResolution, LcdVariant,
        CAPABILITY_BOARD_WIDTH, CAPABILITY_CHART, CAPABILITY_CHART_MIN_WIDTH,
        CAPABILITY_NAME_WIDTH, CAPABILITY_VALUE_WIDTH,
    };

    #[test]
    fn buzzer_drive_capabilities_are_conservative_by_board() {
        assert_eq!(
            capabilities(BoardKind::Green).buzzer_drive,
            BuzzerDrive::BatteryLevel
        );
        assert_eq!(
            capabilities(BoardKind::RedLite).buzzer_drive,
            BuzzerDrive::BatteryLevel
        );
        assert_eq!(
            capabilities(BoardKind::Blue).buzzer_drive,
            BuzzerDrive::BatteryLevel
        );
        assert_eq!(
            capabilities(BoardKind::Pro).buzzer_drive,
            BuzzerDrive::Boosted9V
        );
    }

    #[test]
    fn stock_profiles_match_product_thermistor_evidence() {
        let profiles = default_profiles();
        assert!(!profiles[0].config.thermistor);
        assert!(profiles[1].config.thermistor);
        assert!(!profiles[2].config.thermistor);
        assert!(profiles[3].config.thermistor);
        assert_eq!(
            profiles[1].config.thermistor,
            capabilities(BoardKind::RedLite).thermistor != CapabilityStatus::Unknown
        );
        assert_eq!(
            profiles[3].config.thermistor,
            capabilities(BoardKind::Pro).thermistor != CapabilityStatus::Unknown
        );
        assert!(effective_config(BoardKind::RedLite, &profiles[1], &profiles[1].config).thermistor);
        assert!(effective_config(BoardKind::Pro, &profiles[3], &profiles[3].config).thermistor);
    }

    #[test]
    fn edited_stock_profile_is_not_legacy_default() {
        let mut profile = default_profiles()[3].clone();
        profile.config.buzzer = false;
        assert!(!super::is_legacy_default_profile(3, &profile));
    }

    #[test]
    fn every_board_selects_its_matching_stock_profile() {
        let profiles = default_profiles();
        for board in BoardKind::ALL {
            let index = stock_profile_index(board);
            assert_eq!(index, board as usize);
            assert_eq!(stock_profile_config(board), profiles[index].config);
            assert!(is_unedited_stock_selection(
                board,
                index,
                &profiles[index],
                &profiles[index].config,
            ));
        }
    }

    #[test]
    fn edited_and_custom_profiles_are_not_auto_selected() {
        let profiles = default_profiles();
        let mut edited = profiles[0].clone();
        edited.config.buzzer = false;
        assert!(!is_unedited_stock_selection(
            BoardKind::Green,
            0,
            &edited,
            &edited.config,
        ));
        assert!(!is_unedited_stock_selection(
            BoardKind::Green,
            4,
            &profiles[4],
            &profiles[4].config,
        ));
    }

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
        assert!(!pro.iter().any(|item| item.component == "RGB LED"));
        assert_eq!(
            capabilities(BoardKind::Pro).rgb_led,
            CapabilityStatus::Documented
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
    #[allow(clippy::assertions_on_constants)]
    fn capability_chart_layout_is_compact_and_deterministic() {
        assert_eq!(capability_chart_cells(&CAPABILITY_CHART[0]).len(), 14);
        assert_eq!(CAPABILITY_BOARD_WIDTH, 96.0);
        assert_eq!(CAPABILITY_NAME_WIDTH, 128.0);
        assert_eq!(CAPABILITY_VALUE_WIDTH, 400.0);
        assert_eq!(
            CAPABILITY_CHART_MIN_WIDTH,
            CAPABILITY_BOARD_WIDTH + CAPABILITY_NAME_WIDTH + CAPABILITY_VALUE_WIDTH + 16.0
        );
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
        assert_eq!(
            red_lite.light_sensor,
            "Unsupported: no claimed onboard sensor"
        );
        assert_eq!(red_lite.led, "Red + green PWM");
        assert_eq!(red_lite.thermistor, "Documented onboard temperature sensor");
        assert!(red_lite.buses.contains("A1/A4"));
        assert!(red_lite
            .source
            .contains("https://www.crowdsupply.com/oddly-specific-objects/sensor-watch"));
        assert!(red_lite.connector.contains("No nine-pin"));
        assert!(red_lite.verification.contains("Firmware verification"));
        let blue = rows.iter().find(|row| row.board == "Blue").unwrap();
        assert_eq!(blue.led, "Red + blue");
        assert_eq!(blue.thermistor, "Unknown: board/revision-dependent");
        let pro = rows.iter().find(|row| row.board == "Pro").unwrap();
        assert_eq!(pro.light_sensor, "Documented infrared phototransistor");
        assert_eq!(pro.led, "Red + green + blue PWM");
        assert!(pro.lcd.contains("72-segment") && pro.lcd.contains("92-segment"));
        assert!(pro.buses.contains("nine-pin") || pro.buses.contains("Nine-pin"));
        assert!(pro.connector.contains("Nine-pin"));
        assert!(pro
            .source
            .contains("https://www.crowdsupply.com/oddly-specific-objects/sensor-watch-pro"));
        assert!(rows.iter().all(|row| !row.lcd.is_empty()
            && !row.light_sensor.is_empty()
            && !row.verification.is_empty()
            && !row.programming.is_empty()
            && !row.connector.is_empty()));
    }

    #[test]
    fn lite_and_pro_thermistors_are_documented_without_compensation_claims() {
        assert_eq!(
            capabilities(BoardKind::RedLite).thermistor,
            CapabilityStatus::Documented
        );
        assert_eq!(
            capabilities(BoardKind::Pro).thermistor,
            CapabilityStatus::Documented
        );
        assert!(CAPABILITY_CHART.iter().all(|row| {
            !row.thermistor.contains("automatic")
                && !row.thermistor.contains("compensation")
                && row.verification.contains("Firmware verification")
        }));
    }

    #[test]
    fn lite_rejects_requested_accelerometer() {
        let result = issues(
            BoardKind::RedLite,
            ComponentsConfig {
                accelerometer: true,
                ..Default::default()
            },
        );
        assert!(result.iter().any(|item| {
            item.component == "accelerometer" && item.severity == CompatibilitySeverity::Error
        }));
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
            CapabilityStatus::Documented
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
    pub programming: &'static str,
    pub connector: &'static str,
}

pub const CAPABILITY_CHART: [CapabilityChartRow; 4] = [
    CapabilityChartRow {
        board: "Green",
        lcd: "Classic LCD",
        led: "RGB (red/green/blue)",
        light_sensor: "Onboard IR light sensor",
        thermistor: "Unknown: board/revision-dependent",
        accelerometer: "Optional accessory: not onboard",
        buzzer: "Piezo",
        buses: "I2C/SPI available: pin mapping revision-dependent",
        uart: "Multiplexed: not dedicated",
        confidence: "High for RGB/IR: medium for buses",
        source: "Official maintained pin evidence: community IR evidence",
        revision_notes: "Do not generalize across commercial revisions",
        verification: "Firmware verification: not established here. Hardware validation: fitted sensors remain revision-sensitive",
        programming: "USB Micro-B; UF2 bootloader",
        connector: "Unknown: board/revision-dependent",
    },
    CapabilityChartRow {
        board: "Red / Lite",
        lcd: "Classic 72-segment F-91W/A158W LCD",
        led: "Red + green PWM",
        light_sensor: "Unsupported: no claimed onboard sensor",
        thermistor: "Documented onboard temperature sensor",
        accelerometer: "Unsupported: no onboard accelerometer claimed",
        buzzer: "Piezo pad",
        buses: "A1/A4 pads: analog/digital/PWM; UART",
        uart: "A1/A4 test pads",
        confidence: "Product-page claim; firmware/hardware validation separate",
        source: "Product-page claim: https://www.crowdsupply.com/oddly-specific-objects/sensor-watch",
        revision_notes: "No nine-pin connector; A4 can provide wake input",
        verification: "Firmware verification: no temperature-compensation proof. Hardware validation: confirm fitted sensor and A1/A4 mapping",
        programming: "USB Micro-B; UF2 bootloader",
        connector: "No nine-pin connector claimed",
    },
    CapabilityChartRow {
        board: "Blue",
        lcd: "Classic LCD",
        led: "Red + blue",
        light_sensor: "Unknown: board/revision-dependent",
        thermistor: "Unknown: board/revision-dependent",
        accelerometer: "Optional accessory: not onboard",
        buzzer: "Piezo",
        buses: "I2C/SPI available: pin mapping revision-dependent",
        uart: "Multiplexed: not dedicated",
        confidence: "High for red/blue: low for fitted sensors",
        source: "Official maintained pin evidence: community LED evidence",
        revision_notes: "Thermistor and accessory population may vary",
        verification: "Firmware verification: not established here. Hardware validation: sensor fit remains unknown",
        programming: "Unknown: board/revision-dependent",
        connector: "Unknown: board/revision-dependent",
    },
    CapabilityChartRow {
        board: "Pro",
        lcd: "Classic 72-segment LCD or custom 92-segment LCD",
        led: "Red + green + blue PWM",
        light_sensor: "Documented infrared phototransistor",
        thermistor: "Documented onboard temperature sensor",
        accelerometer: "Optional LIS2DW add-on",
        buzzer: "Amplified piezo",
        buses: "Nine-pin connector: I2C/SPI/GPIO/UART/analog/wake",
        uart: "UART test points and nine-pin connector",
        confidence: "Product-page claim; firmware/hardware validation separate",
        source: "Product-page claim: https://www.crowdsupply.com/oddly-specific-objects/sensor-watch-pro",
        revision_notes: "Custom LCD and LIS2DW are optional Pro add-ons",
        verification: "Firmware verification: no temperature-compensation proof. Hardware validation: confirm connector population and sensor/add-on fit",
        programming: "USB Micro-B; UF2 bootloader",
        connector: "Nine-pin sensor-board connector",
    },
];

pub fn capability_chart_rows() -> &'static [CapabilityChartRow; 4] {
    &CAPABILITY_CHART
}

const CAPABILITY_BOARD_WIDTH: f32 = 96.0;
const CAPABILITY_NAME_WIDTH: f32 = 128.0;
const CAPABILITY_VALUE_WIDTH: f32 = 400.0;
const CAPABILITY_CHART_MIN_WIDTH: f32 =
    CAPABILITY_BOARD_WIDTH + CAPABILITY_NAME_WIDTH + CAPABILITY_VALUE_WIDTH + 16.0;

fn capability_chart_cells(row: &CapabilityChartRow) -> [(&'static str, &'static str); 14] {
    [
        ("LCD", row.lcd),
        ("LED channels/type", row.led),
        ("Light sensor", row.light_sensor),
        ("Thermistor", row.thermistor),
        ("Accelerometer", row.accelerometer),
        ("Buzzer", row.buzzer),
        ("Buses", row.buses),
        ("UART", row.uart),
        ("Confidence", row.confidence),
        ("Source", row.source),
        ("Revision notes", row.revision_notes),
        ("Verification", row.verification),
        ("Programming", row.programming),
        ("Connector", row.connector),
    ]
}

/// Renders the capability facts as a compact vertical table. The inner
/// horizontal scroll is intentional: it keeps the evidence column readable
/// without allowing the chart to widen the Build & Flash panel itself.
pub fn show_capability_chart(ui: &mut Ui) {
    ui.label("Capability status is authoritative for Studio review. Uncertain hardware is never assumed buildable.");
    egui::ScrollArea::horizontal()
        .id_source("board_capability_chart_scroll")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.set_min_width(CAPABILITY_CHART_MIN_WIDTH);
            egui::Grid::new("board_capability_chart")
                .striped(true)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.add_sized(
                        [CAPABILITY_BOARD_WIDTH, 0.0],
                        egui::Label::new(egui::RichText::new("Board").strong()),
                    );
                    ui.add_sized(
                        [CAPABILITY_NAME_WIDTH, 0.0],
                        egui::Label::new(egui::RichText::new("Capability").strong()),
                    );
                    ui.add_sized(
                        [CAPABILITY_VALUE_WIDTH, 0.0],
                        egui::Label::new(egui::RichText::new("Evidence / status").strong()),
                    );
                    ui.end_row();

                    for row in capability_chart_rows() {
                        for (name, value) in capability_chart_cells(row) {
                            ui.add_sized(
                                [CAPABILITY_BOARD_WIDTH, 0.0],
                                egui::Label::new(egui::RichText::new(row.board).strong()),
                            );
                            ui.add_sized(
                                [CAPABILITY_NAME_WIDTH, 0.0],
                                egui::Label::new(name).wrap(true),
                            );
                            ui.add_sized(
                                [CAPABILITY_VALUE_WIDTH, 0.0],
                                egui::Label::new(value).wrap(true),
                            );
                            ui.end_row();
                        }
                    }
                });
        });
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
    if config.uart_shell
        && board == BoardKind::Pro
        && matches!(caps.uart, CapabilityStatus::Documented)
    {
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
    if config.accelerometer {
        match caps.accelerometer {
            CapabilityStatus::Unsupported => issues.push(issue(
                "accelerometer",
                CompatibilitySeverity::Error,
                "accelerometer is not claimed for this board",
                "Disable the accelerometer or select Pro with the optional LIS2DW add-on",
            )),
            CapabilityStatus::Unknown => issues.push(issue(
                "accelerometer",
                CompatibilitySeverity::Warning,
                "accelerometer capability is unknown for this board",
                "Confirm the accelerometer, bus, address, and power mapping before building",
            )),
            _ => {}
        }
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
                "Do not build RGB. Confirm a blue channel implementation or select a verified bi-color configuration",
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
                thermistor: true,
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
                thermistor: true,
                accelerometer: false,
                buzzer: true,
                led: true,
                ..Default::default()
            },
        ),
        BuildProfile::new("Custom", ComponentsConfig::default()),
    ]
}

/// Returns whether a persisted profile is byte-for-byte equivalent to a stock
/// profile from settings schema 1. This deliberately excludes edited profiles
/// from the stock-default migration.
pub fn stock_profile_index(board: BoardKind) -> usize {
    board as usize
}

pub fn stock_profile_config(board: BoardKind) -> ComponentsConfig {
    default_profiles()[stock_profile_index(board)]
        .config
        .clone()
}

/// Returns a compact description of the requested component defaults.
pub fn stock_profile_defaults_summary(config: &ComponentsConfig) -> String {
    let mut defaults = vec![format!(
        "LCD {}",
        match config.lcd {
            LcdVariant::Standard => "Standard",
            LcdVariant::OsoAccessory => "OSO accessory",
            LcdVariant::Custom => "Custom",
        }
    )];
    for (name, enabled) in [
        ("accelerometer", config.accelerometer),
        ("light sensor", config.light_sensor),
        ("thermistor", config.thermistor),
        ("buzzer", config.buzzer),
        ("LED", config.led),
        ("RGB LED", config.rgb_led),
        ("UART shell", config.uart_shell),
        ("GPIO", config.gpio),
        ("SPI", config.spi),
        ("I2C", config.i2c),
    ] {
        if enabled {
            defaults.push(format!("{name} on"));
        }
    }
    defaults.join(", ")
}

/// Returns whether the selected profile and requested draft are both the
/// untouched stock profile for the currently selected board.
pub fn is_unedited_stock_selection(
    board: BoardKind,
    index: usize,
    profile: &BuildProfile,
    draft: &ComponentsConfig,
) -> bool {
    index == stock_profile_index(board)
        && *profile == default_profiles()[index]
        && draft == &profile.config
}

pub fn is_legacy_default_profile(index: usize, profile: &BuildProfile) -> bool {
    let legacy = match index {
        0 => BuildProfile::new(
            "Green",
            ComponentsConfig {
                buzzer: true,
                led: true,
                ..Default::default()
            },
        ),
        1 => BuildProfile::new(
            "Red / Lite",
            ComponentsConfig {
                light_sensor: false,
                buzzer: true,
                led: true,
                ..Default::default()
            },
        ),
        2 => BuildProfile::new(
            "Blue",
            ComponentsConfig {
                thermistor: false,
                buzzer: true,
                led: true,
                ..Default::default()
            },
        ),
        3 => BuildProfile::new(
            "Pro",
            ComponentsConfig {
                thermistor: false,
                accelerometer: false,
                buzzer: true,
                led: true,
                ..Default::default()
            },
        ),
        4 => BuildProfile::new("Custom", ComponentsConfig::default()),
        _ => return false,
    };
    *profile == legacy
}

/// Product-page-backed thermistor defaults used by stock profiles and migration.
pub fn default_thermistor_for_profile(name: &str) -> bool {
    matches!(name, "Red / Lite" | "Red" | "Lite" | "Pro")
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
    let colors = semantic(ui);
    ui.strong("Components / Build Profile");
    ui.label("Describe a custom Sensor Watch board or OSO accessory LCD/sensor board.");
    ui.colored_label(
        colors.warning,
        "UF2 build disabled: this profile is planning data only. It is not a firmware build input.",
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
                    ui.colored_label(colors.error, format!("Error: {error}"));
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
                ui.horizontal(|ui| {
                    ui.scope(|ui| {
                        let mut style = ui.style().as_ref().clone();
                        style.visuals.widgets.noninteractive.fg_stroke.color = colors.disabled_text;
                        ui.set_style(style);
                        ui.add_enabled(false, egui::Checkbox::new(value, label));
                    });
                    ui.colored_label(colors.disabled_text, format!("Unavailable: {reason}"));
                });
                ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover())
            } else {
                ui.checkbox(value, label)
            };
            if response.changed() {
                changed = true;
            }
        }
    });
    let effective = effective_config(board, &BuildProfile::new("draft", draft.clone()), draft);
    ui.label("Requested selections are preserved. Effective state is fail-closed:");
    ui.monospace(format!("Effective: LCD={}, accelerometer={}, light sensor={}, thermistor={}, buzzer={}, LED={}, RGB LED={}", effective.lcd.label(), effective.accelerometer, effective.light_sensor, effective.thermistor, effective.buzzer, effective.led, effective.rgb_led));
    let (flash, ram) = estimate(&effective);
    let draft_profile = BuildProfile::new("draft", draft.clone());
    if let Err(error) = draft_profile.validate() {
        ui.colored_label(colors.error, format!("Error: {error}"));
    }
    for finding in validate_compatibility(board, &draft_profile, draft) {
        let color = match finding.severity {
            CompatibilitySeverity::Error => colors.error,
            CompatibilitySeverity::Warning => colors.warning,
        };
        ui.colored_label(
            color,
            format!(
                "{:?}: {}: {}",
                finding.severity, finding.reason, finding.suggested_action
            ),
        );
    }
    ui.monospace(format!(
        "Estimated component impact: +{flash} KiB flash, +{ram} KiB RAM (planning estimate)"
    ));
    let mut warnings = Vec::new();
    if draft.lcd == LcdVariant::OsoAccessory && !(draft.spi || draft.i2c) {
        warnings.push("OSO accessory LCD usually needs SPI or I2C enabled. Verify the registered module interface.");
    }
    if draft.light_sensor && draft.lcd == LcdVariant::OsoAccessory {
        warnings
            .push("Confirm the OSO accessory light-sensor address and power budget before use.");
    }
    if draft.thermistor && draft.i2c {
        warnings.push("Thermistor is commonly analog. Enabling I2C does not provide an automatic analog pin mapping.");
    }
    for warning in warnings {
        ui.colored_label(colors.warning, format!("Warning: {warning}"));
    }
    ui.colored_label(colors.secondary_text, "Profile edits remain available for planning and review. Studio does not infer pins, buses, addresses, power sequencing, or firmware modules from these choices. The build preflight therefore refuses to generate a configured UF2.");
    (changed, profile_selection)
}
