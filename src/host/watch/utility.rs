//! Host utility shim: re-export the shared, pure date/time helpers from `core`.
//!
//! The real `src/watch/utility.rs` is pure logic with no hardware dependency, but
//! it lives in the ARM HAL tree. On host we reuse the proven `sensor_watch_core`
//! copy (identical, already unit-tested) so faces like `simple_clock` can call
//! `crate::watch::utility::get_weekday` unchanged.

pub use sensor_watch_core::utility::{
    convert_to_12_hour, convert_to_unix_time, date_time_convert_zone, date_time_from_unix_time,
    date_time_to_unix_time, days_in_month, days_since_new_year, get_iso8601_weekday_number,
    get_weekday, get_weeknumber, seconds_to_duration,
};

/// Offsets a timestamp by a given amount. Same logic as the real
/// `src/watch/utility.rs`; defined here (not in `core`) so the host shim mirrors
/// the ARM HAL surface without touching the shared core crate.
pub fn thermistor_temperature(
    value: u16,
    highside: bool,
    b: f32,
    nominal_temp: f32,
    nominal_resistance: f32,
    series_resistance: f32,
) -> f32 {
    if value == 0 || value == u16::MAX {
        return f32::NAN;
    }
    let reading = if highside {
        (65535.0 * series_resistance) / value as f32 - series_resistance
    } else {
        series_resistance / (65535.0 / value as f32 - 1.0)
    };
    let inv_t = 1.0 / (nominal_temp + 273.15) + libm::logf(reading / nominal_resistance) / b;
    1.0 / inv_t - 273.15
}

pub fn offset_timestamp(now: u32, hours: i8, minutes: i8, seconds: i8) -> u32 {
    let mut new = now as i64;
    new += hours as i64 * 60 * 60;
    new += minutes as i64 * 60;
    new += seconds as i64;
    new as u32
}
