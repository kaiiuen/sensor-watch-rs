//! Firmware-local copies of pure hardware input guards.

pub const BUZZER_VOLTAGE_MAX_TENTHS: u8 = 90;
pub const TCC_PERIOD_MAX: u32 = 0x00ff_ffff;

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
pub const fn valid_i2c_address(value: i16) -> bool {
    value >= 0 && value <= 0x7f
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
