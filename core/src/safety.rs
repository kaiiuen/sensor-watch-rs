//! Pure input validation shared by firmware and host tooling.
//!
//! Limits in this module are register/board limits already established by the
//! firmware: SAM L22 PMUX has functions A-H, the RTC uses a 2020-based 6-bit
//! year, and the buzzer voltage is expressed in the existing 0..=90 tenths-of-a
//! volt range. These are validation limits, not electrical certification.

pub const BUZZER_VOLTAGE_MAX_TENTHS: u8 = 90;
/// Maximum safe unboosted drive, expressed in tenths of a volt.
/// 4.2 V is the battery charging maximum; boosted output is a separate
/// capability and is never implied by this limit.
pub const BUZZER_BATTERY_LEVEL_MAX_TENTHS: u8 = 42;

pub fn valid_buzzer_voltage_for_battery(voltage: u8) -> bool {
    voltage <= BUZZER_BATTERY_LEVEL_MAX_TENTHS
}
pub const TCC_PERIOD_MAX: u32 = 0x00ff_ffff;
pub const DISPLAY_CHAR_COUNT: u8 = 10;

pub fn valid_buzzer_voltage(voltage: u8) -> bool {
    voltage <= BUZZER_VOLTAGE_MAX_TENTHS
}

pub fn valid_buzzer_period(period: u32) -> bool {
    (1..=TCC_PERIOD_MAX).contains(&period)
}

pub fn clamp_channel(channel: u8) -> u8 {
    channel
}

pub fn valid_display_position(position: u8) -> bool {
    position < DISPLAY_CHAR_COUNT
}

pub fn valid_display_character(character: u8) -> bool {
    (0x20..=0x7e).contains(&character)
}

pub fn valid_pin(port: u8, pin: u8) -> bool {
    port < 2 && pin < 32
}

pub fn valid_pmux(function: u8) -> bool {
    function <= 7
}

/// Returns whether an address is usable for a normal 7-bit I2C target.
/// Reserved addresses outside 0x08..=0x77 are rejected.
pub fn valid_i2c_address(address: i16) -> bool {
    (0x08..=0x77).contains(&address)
}

pub fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

pub fn valid_datetime(year: u8, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> bool {
    let full_year = 2020u16 + year as u16;
    if month == 0 || month > 12 || hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    let days = match month {
        2 if is_leap_year(full_year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    day >= 1 && day <= days
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_hardware_boundary_overruns() {
        assert!(valid_buzzer_voltage(BUZZER_VOLTAGE_MAX_TENTHS));
        assert!(!valid_buzzer_voltage(BUZZER_VOLTAGE_MAX_TENTHS + 1));
        assert!(valid_buzzer_voltage_for_battery(
            BUZZER_BATTERY_LEVEL_MAX_TENTHS
        ));
        assert!(!valid_buzzer_voltage_for_battery(
            BUZZER_BATTERY_LEVEL_MAX_TENTHS + 1
        ));
        assert!(valid_buzzer_period(1));
        assert!(!valid_buzzer_period(0));
        assert!(!valid_buzzer_period(TCC_PERIOD_MAX + 1));
        assert!(valid_pin(1, 31));
        assert!(!valid_pin(2, 0));
        assert!(valid_pmux(7));
        assert!(!valid_pmux(8));
        assert!(!valid_i2c_address(0x00));
        assert!(!valid_i2c_address(0x07));
        assert!(valid_i2c_address(0x08));
        assert!(valid_i2c_address(0x77));
        assert!(!valid_i2c_address(0x78));
        assert!(!valid_i2c_address(0x7f));
    }

    #[test]
    fn validates_calendar_and_lcd_input() {
        assert!(valid_datetime(4, 2, 29, 23, 59, 59));
        assert!(!valid_datetime(3, 2, 29, 0, 0, 0));
        assert!(!valid_datetime(0, 4, 31, 0, 0, 0));
        assert!(valid_display_position(9));
        assert!(!valid_display_position(10));
        assert!(valid_display_character(b'~'));
        assert!(!valid_display_character(0x7f));
    }
}
