//! Global settings bit-packing.
//!
//! The settings are stored in a single 32-bit register (RTC backup register 0).
//! This module provides getters and setters for each field.

/// Global settings covering watch behavior, stored in RTC backup register 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub reg: u32,
}

impl Settings {
    /// The inactivity interval for asking the active face to resign.
    pub fn to_interval(self) -> u8 {
        ((self.reg >> 1) & 0x3) as u8
    }
    /// If true, always time out from the active face to face 0.
    pub fn to_always(self) -> bool {
        (self.reg >> 3) & 0x1 != 0
    }
    /// 0 to disable low energy mode, or an inactivity interval for LE mode.
    pub fn le_interval(self) -> u8 {
        ((self.reg >> 4) & 0x7) as u8
    }
    /// How many seconds to shine the LED for (x2); 0 = only while pressed.
    pub fn led_duration(self) -> u8 {
        ((self.reg >> 7) & 0x7) as u8
    }
    /// Red LED value (0-15) for general illumination.
    pub fn led_red_color(self) -> u8 {
        ((self.reg >> 10) & 0xF) as u8
    }
    /// Green LED value (0-15) for general illumination.
    pub fn led_green_color(self) -> u8 {
        ((self.reg >> 14) & 0xF) as u8
    }
    /// An index into the time zone table.
    pub fn time_zone(self) -> u8 {
        ((self.reg >> 18) & 0x3F) as u8
    }
    /// Whether the clock should use 12 or 24 hour mode.
    pub fn clock_mode_24h(self) -> bool {
        (self.reg >> 24) & 0x1 != 0
    }
    /// Whether the clock should show a leading zero in 24h mode.
    pub fn clock_24h_leading_zero(self) -> bool {
        (self.reg >> 25) & 0x1 != 0
    }
    /// Whether to use imperial units.
    pub fn use_imperial_units(self) -> bool {
        (self.reg >> 26) & 0x1 != 0
    }
    /// Whether there is at least one alarm enabled.
    pub fn alarm_enabled(self) -> bool {
        (self.reg >> 27) & 0x1 != 0
    }
    /// Whether pressing a button emits a sound.
    pub fn button_should_sound(self) -> bool {
        self.reg & 0x1 != 0
    }

    pub fn set_button_should_sound(&mut self, v: bool) {
        self.reg = (self.reg & !0x1) | (v as u32);
    }
    pub fn set_to_interval(&mut self, v: u8) {
        self.reg = (self.reg & !(0x3 << 1)) | ((v as u32 & 0x3) << 1);
    }
    pub fn set_to_always(&mut self, v: bool) {
        self.reg = (self.reg & !(0x1 << 3)) | ((v as u32) << 3);
    }
    pub fn set_le_interval(&mut self, v: u8) {
        self.reg = (self.reg & !(0x7 << 4)) | ((v as u32 & 0x7) << 4);
    }
    pub fn set_led_duration(&mut self, v: u8) {
        self.reg = (self.reg & !(0x7 << 7)) | ((v as u32 & 0x7) << 7);
    }
    pub fn set_led_red_color(&mut self, v: u8) {
        self.reg = (self.reg & !(0xF << 10)) | ((v as u32 & 0xF) << 10);
    }
    pub fn set_led_green_color(&mut self, v: u8) {
        self.reg = (self.reg & !(0xF << 14)) | ((v as u32 & 0xF) << 14);
    }
    pub fn set_time_zone(&mut self, v: u8) {
        self.reg = (self.reg & !(0x3F << 18)) | ((v as u32 & 0x3F) << 18);
    }
    pub fn set_clock_mode_24h(&mut self, v: bool) {
        self.reg = (self.reg & !(0x1 << 24)) | ((v as u32) << 24);
    }
    pub fn set_clock_24h_leading_zero(&mut self, v: bool) {
        self.reg = (self.reg & !(0x1 << 25)) | ((v as u32) << 25);
    }
    pub fn set_use_imperial_units(&mut self, v: bool) {
        self.reg = (self.reg & !(0x1 << 26)) | ((v as u32) << 26);
    }
    pub fn set_alarm_enabled(&mut self, v: bool) {
        self.reg = (self.reg & !(0x1 << 27)) | ((v as u32) << 27);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_fields_round_trip() {
        let mut s = Settings::default();
        s.set_button_should_sound(true);
        s.set_to_interval(3);
        s.set_to_always(true);
        s.set_le_interval(7);
        s.set_led_duration(5);
        s.set_led_red_color(0xA);
        s.set_led_green_color(0x5);
        s.set_time_zone(40);
        s.set_clock_mode_24h(true);
        s.set_clock_24h_leading_zero(true);
        s.set_use_imperial_units(true);
        s.set_alarm_enabled(true);

        assert!(s.button_should_sound());
        assert_eq!(s.to_interval(), 3);
        assert!(s.to_always());
        assert_eq!(s.le_interval(), 7);
        assert_eq!(s.led_duration(), 5);
        assert_eq!(s.led_red_color(), 0xA);
        assert_eq!(s.led_green_color(), 0x5);
        assert_eq!(s.time_zone(), 40);
        assert!(s.clock_mode_24h());
        assert!(s.clock_24h_leading_zero());
        assert!(s.use_imperial_units());
        assert!(s.alarm_enabled());
    }

    #[test]
    fn default_is_all_zero() {
        let s = Settings::default();
        assert_eq!(s.reg, 0);
        assert!(!s.button_should_sound());
        assert_eq!(s.to_interval(), 0);
        assert!(!s.clock_mode_24h());
    }

    #[test]
    fn fields_are_independent() {
        let mut s = Settings::default();
        s.set_led_red_color(0xF);
        s.set_led_green_color(0x0);
        // Setting red must not affect green.
        assert_eq!(s.led_red_color(), 0xF);
        assert_eq!(s.led_green_color(), 0x0);
    }
}
