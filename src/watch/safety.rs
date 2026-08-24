//! Firmware-local copies of pure hardware input guards.

pub const BUZZER_VOLTAGE_MAX_TENTHS: u8 = 90;
pub const BUZZER_BATTERY_LEVEL_MAX_TENTHS: u8 = 42;
pub const TCC_PERIOD_MAX: u32 = 0x00ff_ffff;

pub const fn valid_buzzer_voltage_for_battery(value: u8) -> bool {
    value <= BUZZER_BATTERY_LEVEL_MAX_TENTHS
}

pub const fn valid_buzzer_voltage(value: u8) -> bool {
    value <= BUZZER_VOLTAGE_MAX_TENTHS
}
pub const fn valid_buzzer_period(value: u32) -> bool {
    value >= 1 && value <= TCC_PERIOD_MAX
}
pub const fn valid_display_position(value: u8) -> bool {
    value < 10
}
pub const fn valid_display_character(value: u8) -> bool {
    value >= 0x20 && value <= 0x7e
}
pub const fn valid_pin(port: u8, pin: u8) -> bool {
    port < 2 && pin < 32
}
pub const fn valid_pmux(value: u8) -> bool {
    value <= 7
}
/// Returns whether an address is usable for a normal 7-bit I2C target.
/// The ranges reserved by the I2C specification are rejected.
pub const fn valid_i2c_address(value: i16) -> bool {
    value >= 0x08 && value <= 0x77
}

pub const fn is_leap_year(year: u16) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

pub const fn valid_datetime(
    year: u8,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
) -> bool {
    if month == 0 || month > 12 || hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    let max = match month {
        2 if is_leap_year(2020 + year as u16) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    day >= 1 && day <= max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buzzer_voltage_is_limited_by_drive_mode() {
        assert!(valid_buzzer_voltage(90));
        assert!(!valid_buzzer_voltage_for_battery(90));
        assert!(valid_buzzer_voltage_for_battery(42));
    }

    #[test]
    fn reject_reserved_i2c_addresses() {
        assert!(!valid_i2c_address(0x00));
        assert!(!valid_i2c_address(0x07));
        assert!(valid_i2c_address(0x08));
        assert!(valid_i2c_address(0x77));
        assert!(!valid_i2c_address(0x78));
        assert!(!valid_i2c_address(0x7f));
    }

    #[test]
    fn validate_display_and_calendar_boundaries() {
        assert!(valid_display_position(9));
        assert!(!valid_display_position(10));
        assert!(valid_datetime(4, 2, 29, 23, 59, 59));
        assert!(!valid_datetime(3, 2, 29, 0, 0, 0));
        assert!(!valid_datetime(0, 4, 31, 0, 0, 0));
    }
}
