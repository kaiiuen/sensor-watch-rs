//! Utility functions.
//!
//! Port of the C `watch_utility.c`. Provides date/time helpers used by watch
//! faces: weekday, week number, leap year, UNIX time conversion, durations,
//! 12-hour conversion, and thermistor temperature.

use crate::watch::rtc::{DateTime, WATCH_RTC_REFERENCE_YEAR};

/// A duration broken into days, hours, minutes, and seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Duration {
    pub seconds: u8,
    pub minutes: u8,
    pub hours: u8,
    pub days: u32,
}

/// Returns a two-letter weekday abbreviation for the given date/time.
pub fn get_weekday(date_time: DateTime) -> &'static str {
    const WEEKDAYS: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];
    let year = date_time.year as u16 + WATCH_RTC_REFERENCE_YEAR;
    let n = get_iso8601_weekday_number(year, date_time.month, date_time.day);
    WEEKDAYS[(n - 1) as usize]
}

/// Returns a number 1-7 representing the weekday per ISO8601 (Monday=1, Sunday=7).
pub fn get_iso8601_weekday_number(year: u16, month: u8, day: u8) -> u8 {
    let mut year = year as i32 - WATCH_RTC_REFERENCE_YEAR as i32;
    year += 20;
    let mut month = month as i32;
    if month <= 2 {
        month += 12;
        year -= 1;
    }
    ((day as i32 + (13 * (month + 1) / 5) + year + (year / 4) + 5) % 7) as u8 + 1
}

/// Returns a number 1-53 representing the week number (from the musl library).
pub fn get_weeknumber(year: u16, month: u8, day: u8) -> u8 {
    let weekday = get_iso8601_weekday_number(year, month, day) % 7;
    let days = days_since_new_year(year, month, day);

    let mut val = (days as i32 + 7 - (weekday as i32 + 6) % 7) / 7;
    // If 1 Jan is just 1-3 days past Monday, the previous week is also in this year.
    if (weekday as i32 + 371 - days as i32 - 2) % 7 <= 2 {
        val += 1;
    }
    if val == 0 {
        val = 52;
        // If 31 December of prev year a Thursday, or Friday of a leap year,
        // then the prev year has 53 weeks.
        let dec31 = (weekday as i32 + 7 - days as i32 - 1) % 7;
        if dec31 == 4 || (dec31 == 5 && is_leap(year.wrapping_sub(1))) {
            val += 1;
        }
    } else if val == 53 {
        // If 1 January is not a Thursday, and not a Wednesday of a leap year,
        // then this year has only 52 weeks.
        let jan1 = (weekday as i32 + 371 - days as i32) % 7;
        if jan1 != 4 && (jan1 != 3 || !is_leap(year)) {
            val = 1;
        }
    }
    val as u8
}

/// Returns 1 if the year is a leap year, 0 otherwise.
pub fn is_leap(y: u16) -> bool {
    let y = y as i32 + 1900;
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

/// Returns the number of days elapsed since January 1st of the same year.
pub fn days_since_new_year(year: u16, month: u8, day: u8) -> u16 {
    const DAYS_SO_FAR: [u16; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    (if is_leap(year) && month > 2 { 1 } else { 0 })
        + DAYS_SO_FAR[(month - 1) as usize]
        + day as u16
}

/// Converts a year to seconds since the epoch (from musl `__year_to_secs`).
fn year_to_secs(year: u32) -> (u32, bool) {
    let mut is_leap = false;
    if year - 2 <= 136 {
        let y = year as i32;
        let mut leaps = (y - 68) >> 2;
        if (y - 68) & 3 == 0 {
            leaps -= 1;
            is_leap = true;
        }
        return (31536000 * (y - 70) as u32 + 86400 * leaps as u32, is_leap);
    }

    let mut cycles = (year as i32 - 100) / 400;
    let mut rem = (year as i32 - 100) % 400;
    if rem < 0 {
        cycles -= 1;
        rem += 400;
    }
    let (centuries, leaps, leap);
    if rem == 0 {
        is_leap = true;
        centuries = 0;
        leaps = 0;
    } else {
        let (cent, r) = if rem >= 200 {
            if rem >= 300 {
                (3, rem - 300)
            } else {
                (2, rem - 200)
            }
        } else if rem >= 100 {
            (1, rem - 100)
        } else {
            (0, rem)
        };
        centuries = cent;
        if r == 0 {
            is_leap = false;
            leaps = 0;
        } else {
            leaps = r / 4;
            is_leap = r % 4 == 0;
        }
        rem = r;
    }
    leap = is_leap;
    let leaps = leaps + 97 * cycles + 24 * centuries - if leap { 1 } else { 0 };
    (
        (year as i32 - 100) as u32 * 31536000 + leaps as u32 * 86400 + 946684800 + 86400,
        is_leap,
    )
}

/// Converts a month to seconds (from musl `__month_to_secs`).
fn month_to_secs(month: i32, is_leap: bool) -> i32 {
    const SECS_THROUGH_MONTH: [i32; 12] = [
        0,
        31 * 86400,
        59 * 86400,
        90 * 86400,
        120 * 86400,
        151 * 86400,
        181 * 86400,
        212 * 86400,
        243 * 86400,
        273 * 86400,
        304 * 86400,
        334 * 86400,
    ];
    let mut t = SECS_THROUGH_MONTH[month as usize];
    if is_leap && month >= 2 {
        t += 86400;
    }
    t
}

/// Returns the UNIX time (seconds since 1970) for a given date/time in UTC.
pub fn convert_to_unix_time(
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    utc_offset: u32,
) -> u32 {
    // POSIX tm struct starts year at 1900 and month at 0.
    let (secs, is_leap) = year_to_secs(year as u32 - 1900);
    let mut timestamp = secs as i64;
    timestamp += month_to_secs(month as i32 - 1, is_leap) as i64;

    timestamp += (day as i64 - 1) * 86400;
    timestamp += hour as i64 * 3600;
    timestamp += minute as i64 * 60;
    timestamp += second as i64;
    timestamp -= utc_offset as i64;

    timestamp as u32
}

/// Returns the UNIX time for a given `DateTime`.
pub fn date_time_to_unix_time(date_time: DateTime, utc_offset: u32) -> u32 {
    convert_to_unix_time(
        date_time.year as u16 + WATCH_RTC_REFERENCE_YEAR,
        date_time.month,
        date_time.day,
        date_time.hour,
        date_time.minute,
        date_time.second,
        utc_offset,
    )
}

const LEAPOCH: i64 = 946684800 + 86400 * (31 + 29);
const DAYS_PER_400Y: i64 = 365 * 400 + 97;
const DAYS_PER_100Y: i64 = 365 * 100 + 24;
const DAYS_PER_4Y: i64 = 365 * 4 + 1;

/// Returns a `DateTime` for a given UNIX time and UTC offset (from musl).
pub fn date_time_from_unix_time(timestamp: u32, utc_offset: u32) -> DateTime {
    let mut retval = DateTime {
        second: 0,
        minute: 0,
        hour: 0,
        day: 0,
        month: 0,
        year: 0,
    };
    const DAYS_IN_MONTH: [i64; 12] = [31, 30, 31, 30, 31, 31, 30, 31, 30, 31, 31, 29];

    let timestamp = timestamp as i64 + utc_offset as i64;

    let secs = timestamp - LEAPOCH;
    let mut days = secs / 86400;
    let mut remsecs = secs % 86400;
    if remsecs < 0 {
        remsecs += 86400;
        days -= 1;
    }

    let mut qc_cycles = days / DAYS_PER_400Y;
    let mut remdays = days % DAYS_PER_400Y;
    if remdays < 0 {
        remdays += DAYS_PER_400Y;
        qc_cycles -= 1;
    }

    let mut c_cycles = remdays / DAYS_PER_100Y;
    if c_cycles == 4 {
        c_cycles -= 1;
    }
    remdays -= c_cycles * DAYS_PER_100Y;

    let mut q_cycles = remdays / DAYS_PER_4Y;
    if q_cycles == 25 {
        q_cycles -= 1;
    }
    remdays -= q_cycles * DAYS_PER_4Y;

    let mut remyears = remdays / 365;
    if remyears == 4 {
        remyears -= 1;
    }
    remdays -= remyears * 365;

    let leap = remyears == 0 && (q_cycles != 0 || c_cycles == 0);
    let mut yday = remdays + 31 + 28 + if leap { 1 } else { 0 };
    if yday >= 365 + if leap { 1 } else { 0 } {
        yday -= 365 + if leap { 1 } else { 0 };
    }

    let years = remyears + 4 * q_cycles + 100 * c_cycles + 400 * qc_cycles;

    let mut months = 0;
    let mut remdays = remdays;
    while DAYS_IN_MONTH[months as usize] <= remdays {
        remdays -= DAYS_IN_MONTH[months as usize];
        months += 1;
    }

    let mut years = years + 2000;

    months += 2;
    if months >= 12 {
        months -= 12;
        years += 1;
    }

    if years < 2020 || years > 2083 {
        return retval;
    }
    retval.year = (years - WATCH_RTC_REFERENCE_YEAR as i64) as u8;
    retval.month = (months + 1) as u8;
    retval.day = (remdays + 1) as u8;

    retval.hour = (remsecs / 3600) as u8;
    retval.minute = ((remsecs / 60) % 60) as u8;
    retval.second = (remsecs % 60) as u8;

    retval
}

/// Converts a `DateTime` from one time zone to another.
pub fn date_time_convert_zone(
    date_time: DateTime,
    origin_utc_offset: u32,
    destination_utc_offset: u32,
) -> DateTime {
    let timestamp = date_time_to_unix_time(date_time, origin_utc_offset);
    date_time_from_unix_time(timestamp, destination_utc_offset)
}

/// Converts a duration in seconds to a `Duration` struct.
pub fn seconds_to_duration(seconds: u32) -> Duration {
    Duration {
        seconds: (seconds % 60) as u8,
        minutes: ((seconds % 3600) / 60) as u8,
        hours: ((seconds % 86400) / 3600) as u8,
        days: seconds / 86400,
    }
}

/// Converts a `DateTime` for 12-hour display.
///
/// Returns true if the value is in the afternoon (PM).
pub fn convert_to_12_hour(date_time: &mut DateTime) -> bool {
    let is_pm = date_time.hour > 11;
    date_time.hour %= 12;
    if date_time.hour == 0 {
        date_time.hour = 12;
    }
    is_pm
}

/// Returns a temperature in degrees Celsius for a thermistor voltage divider.
pub fn thermistor_temperature(
    value: u16,
    highside: bool,
    b_coefficient: f32,
    nominal_temperature: f32,
    nominal_resistance: f32,
    series_resistance: f32,
) -> f32 {
    let mut reading = value as f32;

    if highside {
        reading = (1023.0 * series_resistance) / (reading / 64.0);
        reading -= series_resistance;
    } else {
        reading = series_resistance / (65535.0 / value as f32 - 1.0);
    }

    reading = reading / nominal_resistance;
    reading = libm::logf(reading);
    reading /= b_coefficient;
    reading += 1.0 / (nominal_temperature + 273.15);
    reading = 1.0 / reading;
    reading -= 273.15;

    reading
}

/// Offsets a timestamp by a given amount.
pub fn offset_timestamp(now: u32, hours: i8, minutes: i8, seconds: i8) -> u32 {
    let mut new = now as i64;
    new += hours as i64 * 60 * 60;
    new += minutes as i64 * 60;
    new += seconds as i64;
    new as u32
}

/// Returns the number of days in a month, handling leap years for February.
pub fn days_in_month(month: u8, year: u16) -> u8 {
    const DAYS_IN_MONTH: [u8; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut days = DAYS_IN_MONTH[(month - 1) as usize];
    if month == 2 && is_leap(year) {
        days += 1;
    }
    days
}
