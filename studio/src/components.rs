//! Conservative hardware component and build-profile configuration.
//!
//! These profiles describe the intended hardware for review and validation. They
//! deliberately do not alter firmware build flags or pin mappings.

use egui::Ui;
use serde::{Deserialize, Serialize};

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
            Self::Standard => "Sensor Watch LCD",
            Self::OsoAccessory => "OSO accessory LCD",
            Self::Custom => "Custom LCD",
        }
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

impl BuildProfile {
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
                thermistor: true,
                buzzer: true,
                led: true,
                ..Default::default()
            },
        ),
        BuildProfile::new(
            "Pro",
            ComponentsConfig {
                thermistor: true,
                accelerometer: true,
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
        + config.buzzer as u32 * 1
        + config.led as u32
        + config.uart_shell as u32 * 8
        + config.gpio as u32 * 1
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

/// Renders the profile editor and returns whether the current draft changed.
pub fn show_configurator(
    ui: &mut Ui,
    profiles: &mut Vec<BuildProfile>,
    selected: &mut usize,
    draft: &mut ComponentsConfig,
) -> bool {
    let mut changed = false;
    ui.strong("Components / Build Profile");
    ui.label("Describe a custom Sensor Watch board or OSO accessory LCD/sensor board.");
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
                        .selectable_value(selected, index, &profile.name)
                        .clicked()
                    {
                        *draft = profile.config.clone();
                        changed = true;
                    }
                }
            });
        if ui
            .button("Save")
            .on_hover_text("Save the current component choices to this named profile")
            .clicked()
        {
            if let Some(profile) = profiles.get_mut(*selected) {
                profile.config = draft.clone();
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
        ui.label("LCD:");
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
        for (label, value) in [
            ("Accelerometer", &mut draft.accelerometer),
            ("Light sensor", &mut draft.light_sensor),
            ("Thermistor", &mut draft.thermistor),
            ("Buzzer", &mut draft.buzzer),
            ("LED", &mut draft.led),
            ("UART shell", &mut draft.uart_shell),
            ("Optional GPIO", &mut draft.gpio),
            ("Optional SPI", &mut draft.spi),
            ("Optional I2C", &mut draft.i2c),
        ] {
            if ui.checkbox(value, label).changed() {
                changed = true;
            }
        }
    });
    let (flash, ram) = estimate(draft);
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
    ui.weak("This profile guides configuration and validation only. It does not magically change hardware pin mappings unless a registered module exists. Firmware build flags are unchanged.");
    changed
}
