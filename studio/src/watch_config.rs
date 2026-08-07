//! Watch configuration.
//!
//! Mirrors the firmware `Settings` struct (a single packed `u32` register) so
//! the user can configure the watch from the app. The values map 1:1 to the
//! bit fields in `src/movement/types.rs`, so a config can be serialized and
//! flashed to the watch.

use serde::{Deserialize, Serialize};

/// Time zone offsets in minutes from UTC (mirrors the firmware table).
/// The index is the `time_zone` setting value.
pub const TIMEZONE_OFFSETS: [i16; 41] = [
    0, 60, 120, 180, 210, 240, 270, 300, 330, 345, 360, 390, 420, 480, 525, 540, 570, 600, 630,
    660, 720, 765, 780, 825, 840, -720, -660, -600, -570, -540, -480, -420, -360, -300, -270, -240,
    -210, -180, -150, -120, -60,
];

/// The watch configuration, mirroring the firmware `Settings` register.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WatchConfig {
    /// Whether pressing a button emits a sound.
    pub button_should_sound: bool,
    /// The inactivity interval (0-3) before the active face resigns.
    pub to_interval: u8,
    /// Whether to always time out to face 0.
    pub to_always: bool,
    /// The low-energy interval (0-7); 0 disables LE mode.
    pub le_interval: u8,
    /// How many seconds to shine the LED (x2); 0 = only while pressed, 7 = off.
    pub led_duration: u8,
    /// Red LED value (0-15) for general illumination.
    pub led_red_color: u8,
    /// Green LED value (0-15) for general illumination.
    pub led_green_color: u8,
    /// Index into the time zone table.
    pub time_zone: u8,
    /// Whether the clock uses 12 or 24 hour mode.
    pub clock_mode_24h: bool,
    /// Whether to show a leading zero in 24h mode.
    pub clock_24h_leading_zero: bool,
    /// Whether to use imperial units.
    pub use_imperial_units: bool,
    /// Whether at least one alarm is enabled.
    pub alarm_enabled: bool,
    /// Whether the clock shows seconds (false = power-saving, wake once/min).
    pub show_seconds: bool,
    /// Button-press volume: false = soft, true = loud.
    pub button_volume: bool,
    /// Signal volume: false = soft, true = loud.
    pub signal_volume: bool,
    /// Alarm volume: false = soft, true = loud.
    pub alarm_volume: bool,

    // ---- Advanced / app-level settings (used by the compiler app) ----
    /// Piezo buzzer drive voltage in volts (0.0 - 9.0).
    pub piezo_voltage: f32,
    /// Whether the LED uses a color gradient when lit.
    pub led_gradient: bool,
    /// The LED gradient color as a hex string (e.g. "#00FF88").
    pub led_gradient_hex: String,
    /// The LED color as a hex string (e.g. "#00FF00").
    pub led_color_hex: String,
    /// Whether raise-to-wake is enabled.
    pub raise_to_wake: bool,
    /// Whether raise-to-wake lights the LED.
    pub raise_to_wake_light: bool,
    /// Whether the light uses red at night instead of the day color.
    pub night_light_red: bool,
}

impl Default for WatchConfig {
    fn default() -> Self {
        WatchConfig {
            button_should_sound: false,
            to_interval: 0,
            to_always: false,
            le_interval: 0,
            led_duration: 0,
            led_red_color: 0,
            led_green_color: 0,
            time_zone: 0,
            clock_mode_24h: true,
            clock_24h_leading_zero: false,
            use_imperial_units: false,
            alarm_enabled: false,
            show_seconds: true,
            button_volume: false,
            signal_volume: false,
            alarm_volume: false,
            piezo_voltage: 9.0,
            led_gradient: false,
            led_gradient_hex: "#00FF88".to_string(),
            led_color_hex: "#00FF00".to_string(),
            raise_to_wake: false,
            raise_to_wake_light: false,
            night_light_red: false,
        }
    }
}

impl WatchConfig {
    /// Packs the config into the firmware's single `u32` settings register.
    pub fn to_reg(&self) -> u32 {
        let mut reg = 0u32;
        reg |= (self.button_should_sound as u32) & 0x1;
        reg |= (self.to_interval as u32 & 0x3) << 1;
        reg |= (self.to_always as u32) << 3;
        reg |= (self.le_interval as u32 & 0x7) << 4;
        reg |= (self.led_duration as u32 & 0x7) << 7;
        reg |= (self.led_red_color as u32 & 0xF) << 10;
        reg |= (self.led_green_color as u32 & 0xF) << 14;
        reg |= (self.time_zone as u32 & 0x3F) << 18;
        reg |= (self.clock_mode_24h as u32) << 24;
        reg |= (self.clock_24h_leading_zero as u32) << 25;
        reg |= (self.use_imperial_units as u32) << 26;
        reg |= (self.alarm_enabled as u32) << 27;
        reg |= (self.show_seconds as u32) << 28;
        reg |= (self.button_volume as u32) << 29;
        reg |= (self.signal_volume as u32) << 30;
        reg |= (self.alarm_volume as u32) << 31;
        reg
    }

    /// Unpacks the firmware's settings register into a config.
    pub fn from_reg(reg: u32) -> Self {
        WatchConfig {
            button_should_sound: reg & 0x1 != 0,
            to_interval: ((reg >> 1) & 0x3) as u8,
            to_always: (reg >> 3) & 0x1 != 0,
            le_interval: ((reg >> 4) & 0x7) as u8,
            led_duration: ((reg >> 7) & 0x7) as u8,
            led_red_color: ((reg >> 10) & 0xF) as u8,
            led_green_color: ((reg >> 14) & 0xF) as u8,
            time_zone: ((reg >> 18) & 0x3F) as u8,
            clock_mode_24h: (reg >> 24) & 0x1 != 0,
            clock_24h_leading_zero: (reg >> 25) & 0x1 != 0,
            use_imperial_units: (reg >> 26) & 0x1 != 0,
            alarm_enabled: (reg >> 27) & 0x1 != 0,
            show_seconds: (reg >> 28) & 0x1 != 0,
            button_volume: (reg >> 29) & 0x1 != 0,
            signal_volume: (reg >> 30) & 0x1 != 0,
            alarm_volume: (reg >> 31) & 0x1 != 0,
            piezo_voltage: 9.0,
            led_gradient: false,
            led_gradient_hex: "#00FF88".to_string(),
            led_color_hex: "#00FF00".to_string(),
            raise_to_wake: false,
            raise_to_wake_light: false,
            night_light_red: false,
        }
    }
}
