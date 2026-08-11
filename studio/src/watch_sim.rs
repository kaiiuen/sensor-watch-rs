//! Casio F-91W display simulation.
//!
//! A clean, minimal model of the F-91W's LCD. It renders the time (with an
//! optional date/time override), can show a short text string on the digits
//! (used for the face-name display and the CASIO logo), and handles the 12/24
//! hour toggle. The design is intentionally simple: a single `override_text`
//! field takes priority over the normal time display, so overrides can never be
//! clobbered by the per-frame update.

use std::time::{SystemTime, UNIX_EPOCH};

/// The time mode (12 or 24 hour).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeMode {
    H24,
    H12,
}

/// The full display state, mirroring the JS `display` object.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Display {
    pub alarm_on_mark: bool,
    pub time_signal_on_mark: bool,
    pub time_mode_24: bool,
    pub time_mode_12: bool,
    pub lap: bool,
    pub dots: bool,
    pub light: bool,
    pub mode_2: char,
    pub mode_1: char,
    pub day_2: char,
    pub day_1: char,
    pub hour_2: char,
    pub hour_1: char,
    pub minute_2: char,
    pub minute_1: char,
    pub second_2: char,
    pub second_1: char,
}

/// The watch operating system.
pub struct CasioF91W {
    /// The current time (seconds since epoch) plus an offset.
    pub time_offset: i64,
    /// The time mode (12 or 24 hour).
    pub time_mode: TimeMode,
    /// The light state.
    pub light: bool,
    /// The current display.
    pub display: Display,
    /// Optional weekday override (0=Sun..6=Sat). When None, the weekday is
    /// derived from the date.
    pub weekday_override: Option<u32>,
    /// When set, `update_display` shows this text on the LCD instead of the
    /// normal time (used for the face-name display and the CASIO logo). This
    /// takes priority over the normal display so it can't be overwritten.
    pub override_text: Option<String>,
}

impl CasioF91W {
    /// Creates a new watch with default state.
    pub fn new() -> Self {
        CasioF91W {
            time_offset: 0,
            time_mode: TimeMode::H24,
            light: false,
            display: Display::default(),
            weekday_override: None,
            override_text: None,
        }
    }

    /// Returns the current wall-clock time in seconds since the epoch.
    fn now(&self) -> i64 {
        let base = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        base + self.time_offset
    }

    /// Returns the current simulated civil time (year, month, day, hour, minute,
    /// second, weekday).
    pub fn get_time(&self) -> (i32, u32, u32, u32, u32, u32, u32) {
        let t = self.now();
        let days = t.div_euclid(86400);
        let secs = t.rem_euclid(86400);
        let (year, month, day) = civil_from_days(days);
        let hour = (secs / 3600) as u32;
        let minute = ((secs % 3600) / 60) as u32;
        let second = (secs % 60) as u32;
        let dow = self
            .weekday_override
            .unwrap_or_else(|| ((days + 4).rem_euclid(7)) as u32);
        (year as i32, month, day, hour, minute, second, dow % 7)
    }

    /// Sets the watch's displayed date/time to the given civil date.
    pub fn set_datetime(&mut self, year: i32, month: u32, day: u32, hour: u32, minute: u32) {
        let days = days_from_civil(year, month, day);
        let target = days * 86400 + (hour as i64) * 3600 + (minute as i64) * 60;
        let base = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.time_offset = target - base;
    }

    /// Toggles the 12/24 hour mode.
    pub fn toggle_time_mode(&mut self) {
        self.time_mode = match self.time_mode {
            TimeMode::H24 => TimeMode::H12,
            TimeMode::H12 => TimeMode::H24,
        };
    }

    /// Shows the CASIO logo (or returns to the normal time display).
    /// Only clears the override if it is currently the CASIO logo, so it never
    /// clobbers a face-name override set by `show_text`.
    pub fn set_casio(&mut self, show: bool) {
        if show {
            self.override_text = Some("CASIO ".to_string());
        } else if self.override_text.as_deref() == Some("CASIO ") {
            self.override_text = None;
        }
    }

    /// Displays a short text string on the LCD (used to show the active watch
    /// face name when cycling).
    #[allow(dead_code)]
    pub fn show_text(&mut self, text: &str) {
        self.override_text = Some(text.chars().take(6).collect());
    }

    /// Updates the display based on the current state.
    #[allow(clippy::field_reassign_with_default)]
    pub fn update_display(&mut self) {
        let mut d = Display::default();
        d.light = self.light;

        // If an override is set, show it and skip the normal time display.
        if let Some(text) = &self.override_text {
            let chars: Vec<char> = text.chars().collect();
            let slot = |i: usize| -> char { chars.get(i).copied().unwrap_or(' ') };
            d.hour_2 = slot(0);
            d.hour_1 = slot(1);
            d.minute_2 = slot(2);
            d.minute_1 = slot(3);
            d.second_2 = slot(4);
            d.second_1 = slot(5);
            self.display = d;
            return;
        }

        // Normal time display.
        let t = self.now();
        let (weekday, day, hours, minutes, seconds) = self.date_time_parts(t);
        let hours = self.display_hour(hours);

        d.dots = true;
        d.time_mode_24 = self.time_mode == TimeMode::H24;
        d.time_mode_12 = self.time_mode == TimeMode::H12;
        d.mode_2 = weekday.chars().nth(0).unwrap_or(' ');
        d.mode_1 = weekday.chars().nth(1).unwrap_or(' ');
        d.day_2 = if day > 9 {
            (b'0' + (day / 10) as u8) as char
        } else {
            ' '
        };
        d.day_1 = (b'0' + (day % 10) as u8) as char;
        d.hour_2 = if hours > 9 {
            (b'0' + (hours / 10) as u8) as char
        } else {
            ' '
        };
        d.hour_1 = (b'0' + (hours % 10) as u8) as char;
        d.minute_2 = if minutes > 9 {
            (b'0' + (minutes / 10) as u8) as char
        } else {
            '0'
        };
        d.minute_1 = (b'0' + (minutes % 10) as u8) as char;
        d.second_2 = if seconds > 9 {
            (b'0' + (seconds / 10) as u8) as char
        } else {
            '0'
        };
        d.second_1 = (b'0' + (seconds % 10) as u8) as char;

        self.display = d;
    }

    /// Returns (weekday, day, hours, minutes, seconds) for a timestamp.
    fn date_time_parts(&self, t: i64) -> (String, u32, u32, u32, u32) {
        let days = t.div_euclid(86400);
        let secs = t.rem_euclid(86400);
        let hours = (secs / 3600) as u32;
        let minutes = ((secs % 3600) / 60) as u32;
        let seconds = (secs % 60) as u32;
        // Day of week: 1970-01-01 was a Thursday.
        let dow = self
            .weekday_override
            .unwrap_or_else(|| ((days + 4).rem_euclid(7)) as u32); // 0=Sun
        let weekday = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"][(dow % 7) as usize];
        let day = civil_day_of_month(days);
        (weekday.to_string(), day, hours, minutes, seconds)
    }

    /// Applies the 12/24 hour display conversion.
    fn display_hour(&self, hours: u32) -> u32 {
        if self.time_mode == TimeMode::H12 {
            let hour = hours % 12;
            if hour == 0 {
                12
            } else {
                hour
            }
        } else {
            hours
        }
    }
}

impl Default for CasioF91W {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{CasioF91W, TimeMode};

    #[test]
    fn twelve_hour_display_uses_twelve_for_midnight_and_noon() {
        let mut watch = CasioF91W::new();
        watch.time_mode = TimeMode::H12;
        assert_eq!(watch.display_hour(0), 12);
        assert_eq!(watch.display_hour(12), 12);
        assert_eq!(watch.display_hour(13), 1);
        assert_eq!(watch.display_hour(23), 11);
    }
}

/// Converts a count of days since the Unix epoch into a civil (year, month, day).
/// Uses Howard Hinnant's `civil_from_days` algorithm.
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Day of month (1-31) for a day count since the Unix epoch.
fn civil_day_of_month(days: i64) -> u32 {
    civil_from_days(days).2
}

/// Converts a civil (year, month, day) into a count of days since the Unix
/// epoch. Inverse of `civil_from_days` (Howard Hinnant's algorithm).
pub(crate) fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let mp = (m + 9) % 12; // [0, 11]
    let doy = (153 * mp as u64 + 2) / 5 + (d as u64 - 1); // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era as i64 * 146097 + doe as i64 - 719_468
}
