//! Casio F-91W display simulation.
//!
//! A clean, minimal model of the F-91W's LCD. It renders the time (with an
//! optional date/time override), can show a short text string on the digits
//! (used for the face-name display and the CASIO logo), and handles the 12/24
//! hour toggle. The design is intentionally simple: a single `override_text`
//! field takes priority over the normal time display, so overrides can never be
//! clobbered by the per-frame update.

use chrono::{Datelike, Local, TimeZone, Timelike};
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
    /// The physical PM glyph. AM leaves this off, even in 12-hour mode.
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

impl Display {
    /// Replaces the normal digits with an overlay while preserving a physically
    /// held backlight and hiding every other LCD indicator.
    pub fn apply_text_override(&mut self, text: &str) {
        self.alarm_on_mark = false;
        self.time_signal_on_mark = false;
        self.time_mode_24 = false;
        self.time_mode_12 = false;
        self.lap = false;
        self.dots = false;

        self.mode_2 = ' ';
        self.mode_1 = ' ';
        self.day_2 = ' ';
        self.day_1 = ' ';
        let chars: Vec<char> = text.chars().collect();
        let slot = |i: usize| chars.get(i).copied().unwrap_or(' ');
        self.hour_2 = slot(0);
        self.hour_1 = slot(1);
        self.minute_2 = slot(2);
        self.minute_1 = slot(3);
        self.second_2 = slot(4);
        self.second_1 = slot(5);
    }
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
        let mut watch = CasioF91W {
            time_offset: 0,
            time_mode: TimeMode::H24,
            light: false,
            display: Display::default(),
            weekday_override: None,
            override_text: None,
        };
        // Simulator policy: "now" means the host PC's local civil time, not UTC.
        watch.reset_to_now();
        watch
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

    /// Sets the watch's displayed date/time to explicit civil fields.
    ///
    /// This intentionally does not consult the host timezone: Apply date/time
    /// is deterministic and treats the fields as simulator civil time.
    pub fn set_datetime(&mut self, year: i32, month: u32, day: u32, hour: u32, minute: u32) {
        self.set_civil_datetime(year, month, day, hour, minute, 0);
    }

    /// Resets the simulator to the host PC's local civil time.
    pub fn reset_to_now(&mut self) {
        let (year, month, day, hour, minute, second, _) = local_civil_time_now();
        self.set_civil_datetime(year, month, day, hour, minute, second);
        self.weekday_override = None;
    }

    fn set_civil_datetime(
        &mut self,
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
    ) {
        let days = days_from_civil(year, month, day);
        let target = days * 86400 + (hour as i64) * 3600 + (minute as i64) * 60 + second as i64;
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
            d.apply_text_override(text);
            self.display = d;
            return;
        }

        // Normal time display.
        let t = self.now();
        let (weekday, day, hours, minutes, seconds) = self.date_time_parts(t);
        let is_pm = hours >= 12;
        let hours = self.display_hour(hours);

        d.dots = true;
        d.time_mode_24 = self.time_mode == TimeMode::H24;
        // The F-91W's physical 12-hour marker is the PM indicator. It must
        // remain off during AM even though the watch is in 12-hour mode.
        d.time_mode_12 = self.time_mode == TimeMode::H12 && is_pm;
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

/// Returns the host-local civil fields for a Unix timestamp.
///
/// Keeping this conversion in chrono's platform-backed local timezone support
/// avoids hand-written offset/DST arithmetic in the simulator.
pub(crate) fn local_civil_time_at(timestamp: i64) -> (i32, u32, u32, u32, u32, u32, u32) {
    let local = Local
        .timestamp_opt(timestamp, 0)
        .single()
        .expect("Unix timestamp must map to one local civil time");
    (
        local.year(),
        local.month(),
        local.day(),
        local.hour(),
        local.minute(),
        local.second(),
        local.weekday().num_days_from_sunday(),
    )
}

fn local_civil_time_now() -> (i32, u32, u32, u32, u32, u32, u32) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    local_civil_time_at(timestamp)
}

impl Default for CasioF91W {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{local_civil_time_at, CasioF91W, Display, TimeMode};
    use chrono::{Datelike, Local, TimeZone, Timelike};

    #[test]
    fn fixed_timestamp_uses_platform_local_civil_conversion() {
        let timestamp = 1_735_732_800; // 2025-01-01 00:00:00 UTC
        let actual = local_civil_time_at(timestamp);
        let expected = Local.timestamp_opt(timestamp, 0).single().unwrap();
        assert_eq!(
            actual,
            (
                expected.year(),
                expected.month(),
                expected.day(),
                expected.hour(),
                expected.minute(),
                expected.second(),
                expected.weekday().num_days_from_sunday(),
            )
        );
    }

    #[test]
    fn reset_to_now_uses_host_local_civil_time() {
        let mut watch = CasioF91W::new();
        watch.reset_to_now();
        let (year, month, day, hour, minute, second, _) = watch.get_time();
        let now = Local::now();
        assert_eq!(
            (year, month, day, hour, minute),
            (now.year(), now.month(), now.day(), now.hour(), now.minute())
        );
        assert!((second as i32 - now.second() as i32).abs() <= 1);
    }

    #[test]
    fn explicit_simulated_time_is_timezone_independent_and_stable() {
        let mut watch = CasioF91W::new();
        watch.set_datetime(2025, 7, 4, 23, 59);
        assert_eq!(
            (
                watch.get_time().0,
                watch.get_time().1,
                watch.get_time().2,
                watch.get_time().3,
                watch.get_time().4
            ),
            (2025, 7, 4, 23, 59)
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(
            (
                watch.get_time().0,
                watch.get_time().1,
                watch.get_time().2,
                watch.get_time().3,
                watch.get_time().4
            ),
            (2025, 7, 4, 23, 59)
        );
    }

    #[test]
    fn changing_time_mode_only_changes_display_representation() {
        let mut watch = CasioF91W::new();
        watch.set_datetime(2025, 7, 4, 23, 0);
        let civil_before = watch.get_time();
        watch.time_mode = TimeMode::H12;
        watch.update_display();
        assert_eq!(watch.get_time(), civil_before);
        assert!(watch.display.time_mode_12);
        watch.time_mode = TimeMode::H24;
        watch.update_display();
        assert_eq!(watch.get_time(), civil_before);
        assert!(watch.display.time_mode_24);
    }

    #[test]
    fn twelve_hour_display_uses_twelve_for_midnight_and_noon() {
        let mut watch = CasioF91W::new();
        watch.time_mode = TimeMode::H12;
        assert_eq!(watch.display_hour(0), 12);
        assert_eq!(watch.display_hour(12), 12);
        assert_eq!(watch.display_hour(13), 1);
        assert_eq!(watch.display_hour(23), 11);
    }

    #[test]
    fn casio_overlay_clears_indicators_but_preserves_held_light_and_restores() {
        let normal = Display {
            alarm_on_mark: true,
            time_signal_on_mark: true,
            time_mode_24: true,
            time_mode_12: true,
            lap: true,
            dots: true,
            light: true,
            mode_2: 'M',
            mode_1: 'O',
            day_2: '2',
            day_1: '4',
            hour_2: '1',
            hour_1: '2',
            minute_2: '3',
            minute_1: '4',
            second_2: '5',
            second_1: '6',
        };
        let mut overlay = normal;
        overlay.apply_text_override("CASIO ");
        assert_eq!(overlay.hour_2, 'C');
        assert_eq!(overlay.hour_1, 'A');
        assert_eq!(overlay.minute_2, 'S');
        assert_eq!(overlay.minute_1, 'I');
        assert_eq!(overlay.second_2, 'O');
        assert_eq!(overlay.second_1, ' ');
        assert!(!overlay.alarm_on_mark);
        assert!(!overlay.time_signal_on_mark);
        assert!(!overlay.time_mode_24);
        assert!(!overlay.time_mode_12);
        assert!(!overlay.lap);
        assert!(!overlay.dots);
        assert!(overlay.light);

        // A released CASIO override is rebuilt from the normal watch state,
        // restoring the snapshot rather than retaining the cleared flags.
        let mut watch = CasioF91W::new();
        watch.time_mode = TimeMode::H24;
        watch.light = true;
        watch.update_display();
        let restored = watch.display;
        watch.set_casio(true);
        watch.update_display();
        assert!(!watch.display.dots);
        assert!(!watch.display.time_mode_24);
        watch.set_casio(false);
        watch.update_display();
        assert_eq!(watch.display, restored);
        assert_eq!(normal, Display { ..normal });
    }

    #[test]
    fn twelve_hour_simulator_only_shows_pm_after_noon() {
        let mut watch = CasioF91W::new();
        watch.time_mode = TimeMode::H12;

        watch.set_datetime(2025, 1, 1, 11, 30);
        watch.update_display();
        assert_eq!(watch.display.hour_2, '1');
        assert_eq!(watch.display.hour_1, '1');
        assert!(!watch.display.time_mode_12);

        watch.set_datetime(2025, 1, 1, 12, 0);
        watch.update_display();
        assert_eq!(watch.display.hour_2, '1');
        assert_eq!(watch.display.hour_1, '2');
        assert!(watch.display.time_mode_12);

        watch.set_datetime(2025, 1, 1, 0, 0);
        watch.update_display();
        assert_eq!(watch.display.hour_2, '1');
        assert_eq!(watch.display.hour_1, '2');
        assert!(!watch.display.time_mode_12);

        watch.set_datetime(2025, 1, 1, 23, 0);
        watch.update_display();
        assert_eq!(watch.display.hour_2, '1');
        assert_eq!(watch.display.hour_1, '1');
        assert!(watch.display.time_mode_12);
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
