//! Casio F-91W operating system simulation.
//!
//! A faithful Rust port of `CasioF91WOperatingSystem.js` from the
//! alexisphilip/Casio-F-91W simulator. It models the watch's menus, buttons,
//! and display state so the app can simulate the watch before flashing.

use std::time::{SystemTime, UNIX_EPOCH};

/// The active menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    DateTime,
    DailyAlarm,
    Stopwatch,
    SetDateTime,
}

/// The active action within a menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Default,
    Casio,
    EditHours,
    EditMinutes,
    EditMonth,
    EditDayNumber,
}

/// The time mode (12 or 24 hour).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeMode {
    H24,
    H12,
}

/// The full display state, mirroring the JS `display` object.
#[derive(Clone, Copy, Debug, Default)]
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
    /// The active menu.
    pub active_menu: Menu,
    /// The active action.
    pub active_action: Action,
    /// The current time (seconds since epoch) plus an offset.
    pub time_offset: i64,
    /// The daily alarm time (seconds since midnight).
    pub alarm_seconds: u32,
    pub alarm_on_mark: bool,
    pub time_signal_on_mark: bool,
    pub time_mode: TimeMode,
    /// Stopwatch state in centiseconds.
    pub stopwatch_cs: u64,
    pub stopwatch_running: bool,
    pub stopwatch_split: Option<u64>,
    pub lap: bool,
    /// The light state.
    pub light: bool,
    /// The current display.
    pub display: Display,
    /// Whether a button is held (for repeat).
    pub button_a_held: bool,
    pub button_a_repeat_timer: u32,
    /// Optional weekday override (0=Sun..6=Sat). When None, the weekday is
    /// derived from the date.
    pub weekday_override: Option<u32>,
}

impl CasioF91W {
    /// Creates a new watch with default state.
    pub fn new() -> Self {
        CasioF91W {
            active_menu: Menu::DateTime,
            active_action: Action::Default,
            time_offset: 0,
            alarm_seconds: 7 * 3600, // 07:00
            alarm_on_mark: false,
            time_signal_on_mark: false,
            time_mode: TimeMode::H24,
            stopwatch_cs: 0,
            stopwatch_running: false,
            stopwatch_split: None,
            lap: false,
            light: false,
            display: Display::default(),
            button_a_held: false,
            button_a_repeat_timer: 0,
            weekday_override: None,
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

    /// Sets the watch's displayed date/time to the given civil date.
    /// The weekday is derived from the date; `time_offset` is adjusted so the
    /// simulated clock shows exactly this date/time.
    pub fn set_datetime(&mut self, year: i32, month: u32, day: u32, hour: u32, minute: u32) {
        let days = days_from_civil(year, month, day);
        let target = days * 86400 + (hour as i64) * 3600 + (minute as i64) * 60;
        let base = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.time_offset = target - base;
    }

    /// Returns the current milliseconds within the second.
    fn now_ms(&self) -> u32 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_millis())
            .unwrap_or(0)
    }

    /// The blinking state: elements hide for 250ms, show for 250ms.
    fn blinking(&self) -> bool {
        let ms = self.now_ms();
        (250..500).contains(&ms) || (750..1000).contains(&ms)
    }

    /// Advances the stopwatch by the given centiseconds.
    pub fn tick_stopwatch(&mut self, cs: u64) {
        if self.stopwatch_running {
            self.stopwatch_cs += cs;
        }
    }

    /// Updates the display based on the current state.
    pub fn update_display(&mut self) {
        // Snapshot immutable state to avoid borrow conflicts.
        let light = self.light;
        let alarm_on = self.alarm_on_mark;
        let signal_on = self.time_signal_on_mark;
        let menu = self.active_menu;
        let action = self.active_action;
        let time_mode = self.time_mode;
        let lap = self.lap;
        let blinking = self.blinking();
        let t = self.now();
        let (weekday, day, hours, minutes, seconds) = self.date_time_parts(t);
        let month = self.month(t);
        let alarm_hours = self.alarm_seconds / 3600;
        let alarm_minutes = (self.alarm_seconds % 3600) / 60;
        let cs = self.stopwatch_split.unwrap_or(self.stopwatch_cs);
        let total_seconds = cs / 100;
        let sw_minutes = (total_seconds / 60) % 100;
        let sw_seconds = total_seconds % 60;
        let centis = (cs % 100) as u32;

        let mut d = Display::default();
        d.light = light;
        d.alarm_on_mark = alarm_on;
        d.time_signal_on_mark = signal_on;

        match menu {
            Menu::DateTime => {
                d.lap = false;
                d.dots = true;
                if action == Action::Default {
                    let hours = self.display_hour(hours);
                    d.time_mode_24 = time_mode == TimeMode::H24;
                    d.time_mode_12 = time_mode == TimeMode::H12;
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
                } else if action == Action::Casio {
                    d.alarm_on_mark = false;
                    d.time_signal_on_mark = false;
                    d.time_mode_24 = false;
                    d.time_mode_12 = false;
                    d.lap = false;
                    d.dots = false;
                    d.mode_2 = ' ';
                    d.mode_1 = ' ';
                    d.day_2 = ' ';
                    d.day_1 = ' ';
                    d.hour_2 = 'C';
                    d.hour_1 = 'A';
                    d.minute_2 = 'S';
                    d.minute_1 = 'I';
                    d.second_2 = 'O';
                    d.second_1 = ' ';
                }
            }
            Menu::DailyAlarm => {
                d.time_mode_24 = false;
                d.time_mode_12 = false;
                d.lap = false;
                d.dots = true;
                d.mode_2 = 'A';
                d.mode_1 = 'L';
                d.day_2 = ' ';
                d.day_1 = ' ';
                d.hour_2 = if alarm_hours > 9 {
                    (b'0' + (alarm_hours / 10) as u8) as char
                } else {
                    ' '
                };
                d.hour_1 = (b'0' + (alarm_hours % 10) as u8) as char;
                d.minute_2 = if alarm_minutes > 9 {
                    (b'0' + (alarm_minutes / 10) as u8) as char
                } else {
                    '0'
                };
                d.minute_1 = (b'0' + (alarm_minutes % 10) as u8) as char;
                d.second_2 = ' ';
                d.second_1 = ' ';
                if action == Action::EditHours && blinking {
                    d.hour_2 = ' ';
                    d.hour_1 = ' ';
                } else if action == Action::EditMinutes && blinking {
                    d.minute_2 = ' ';
                    d.minute_1 = ' ';
                }
            }
            Menu::Stopwatch => {
                d.time_mode_24 = false;
                d.time_mode_12 = false;
                d.lap = lap;
                d.dots = true;
                d.mode_2 = 'S';
                d.mode_1 = 'T';
                d.day_2 = ' ';
                d.day_1 = ' ';
                d.hour_2 = if sw_minutes > 9 {
                    (b'0' + (sw_minutes / 10) as u8) as char
                } else {
                    ' '
                };
                d.hour_1 = (b'0' + (sw_minutes % 10) as u8) as char;
                d.minute_2 = if sw_seconds > 9 {
                    (b'0' + (sw_seconds / 10) as u8) as char
                } else {
                    '0'
                };
                d.minute_1 = (b'0' + (sw_seconds % 10) as u8) as char;
                d.second_2 = (b'0' + (centis / 10) as u8) as char;
                d.second_1 = (b'0' + (centis % 10) as u8) as char;
            }
            Menu::SetDateTime => {
                let hours = self.display_hour(hours);
                d.time_mode_24 = time_mode == TimeMode::H24;
                d.time_mode_12 = time_mode == TimeMode::H12;
                d.lap = false;
                d.mode_2 = weekday.chars().nth(0).unwrap_or(' ');
                d.mode_1 = weekday.chars().nth(1).unwrap_or(' ');
                d.day_2 = if day > 9 {
                    (b'0' + (day / 10) as u8) as char
                } else {
                    ' '
                };
                d.day_1 = (b'0' + (day % 10) as u8) as char;

                if matches!(action, Action::EditMonth | Action::EditDayNumber) {
                    d.dots = false;
                    d.hour_2 = if month > 9 {
                        (b'0' + (month / 10) as u8) as char
                    } else {
                        ' '
                    };
                    d.hour_1 = (b'0' + (month % 10) as u8) as char;
                    d.minute_2 = ' ';
                    d.minute_1 = ' ';
                    d.second_2 = ' ';
                    d.second_1 = ' ';
                } else {
                    d.dots = true;
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
                }

                if blinking {
                    match action {
                        Action::Default => {
                            d.second_2 = ' ';
                            d.second_1 = ' ';
                        }
                        Action::EditMinutes => {
                            d.minute_2 = ' ';
                            d.minute_1 = ' ';
                        }
                        Action::EditHours => {
                            d.hour_2 = ' ';
                            d.hour_1 = ' ';
                        }
                        Action::EditMonth => {
                            d.hour_2 = ' ';
                            d.hour_1 = ' ';
                        }
                        Action::EditDayNumber => {
                            d.day_2 = ' ';
                            d.day_1 = ' ';
                        }
                        _ => {}
                    }
                }
            }
        }
        self.display = d;
    }

    /// Returns (weekday, day, hours, minutes, seconds) for a timestamp.
    /// Day is the true day-of-month computed from the civil calendar.
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

    /// Returns the month (1-12) for a timestamp, from the civil calendar.
    fn month(&self, t: i64) -> u32 {
        let days = t.div_euclid(86400);
        civil_month(days)
    }

    /// Applies the 12/24 hour display conversion.
    fn display_hour(&self, hours: u32) -> u32 {
        if self.time_mode == TimeMode::H12 && hours > 12 {
            hours - 12
        } else {
            hours
        }
    }

    /// Button L (left): light + menu-specific actions.
    pub fn button_l(&mut self, is_down: bool) {
        match self.active_menu {
            Menu::DateTime => {}
            Menu::DailyAlarm => {
                if is_down {
                    match self.active_action {
                        Action::Default => {
                            self.alarm_on_mark = true;
                            self.active_action = Action::EditHours;
                        }
                        Action::EditHours => self.active_action = Action::EditMinutes,
                        Action::EditMinutes => self.active_action = Action::Default,
                        _ => {}
                    }
                }
            }
            Menu::Stopwatch => {
                if is_down {
                    if self.stopwatch_running {
                        if self.stopwatch_split.is_some() {
                            self.stopwatch_split = None;
                            self.lap = false;
                        } else {
                            self.stopwatch_split = Some(self.stopwatch_cs);
                            self.lap = true;
                        }
                    } else if self.stopwatch_split.is_some() {
                        self.stopwatch_split = None;
                        self.lap = false;
                    } else {
                        self.stopwatch_cs = 0;
                    }
                }
            }
            Menu::SetDateTime => {
                if is_down {
                    match self.active_action {
                        Action::Default => self.active_action = Action::EditMinutes,
                        Action::EditMinutes => self.active_action = Action::EditHours,
                        Action::EditHours => self.active_action = Action::EditMonth,
                        Action::EditMonth => self.active_action = Action::EditDayNumber,
                        Action::EditDayNumber => self.active_action = Action::Default,
                        _ => {}
                    }
                }
            }
        }
        self.light = is_down;
    }

    /// Button C (center): cycle menus.
    pub fn button_c(&mut self, is_down: bool) {
        if is_down {
            self.active_menu = match self.active_menu {
                Menu::DateTime => Menu::DailyAlarm,
                Menu::DailyAlarm => Menu::Stopwatch,
                Menu::Stopwatch => Menu::SetDateTime,
                Menu::SetDateTime => Menu::DateTime,
            };
            self.active_action = Action::Default;
        }
    }

    /// Button A (right): menu-specific actions.
    pub fn button_a(&mut self, is_down: bool) {
        match self.active_menu {
            Menu::DateTime => {
                // Track the hold state for the 3-second CASIO display; the
                // actual 12/24 toggle is handled by `toggle_time_mode()` on a
                // clean click (see main.rs) so it fires exactly once.
                self.button_a_held = is_down;
                if is_down {
                    self.button_a_repeat_timer = 0;
                }
            }
            Menu::DailyAlarm => {
                if self.active_action == Action::Default {
                    if is_down {
                        match (self.alarm_on_mark, self.time_signal_on_mark) {
                            (true, true) => {
                                self.alarm_on_mark = false;
                                self.time_signal_on_mark = false;
                            }
                            (true, false) => {
                                self.alarm_on_mark = false;
                                self.time_signal_on_mark = true;
                            }
                            (false, true) => {
                                self.alarm_on_mark = true;
                                self.time_signal_on_mark = true;
                            }
                            (false, false) => {
                                self.alarm_on_mark = true;
                                self.time_signal_on_mark = false;
                            }
                        }
                    }
                } else if matches!(self.active_action, Action::EditHours | Action::EditMinutes) {
                    if is_down {
                        if self.active_action == Action::EditHours {
                            self.alarm_seconds = (self.alarm_seconds + 3600) % 86400;
                        } else {
                            self.alarm_seconds = (self.alarm_seconds + 60) % 86400;
                        }
                    }
                }
            }
            Menu::Stopwatch => {
                if is_down {
                    if self.stopwatch_running {
                        self.stopwatch_running = false;
                    } else {
                        self.stopwatch_running = true;
                    }
                }
            }
            Menu::SetDateTime => {
                if is_down {
                    // Increment the selected field by adjusting the time offset.
                    let delta = match self.active_action {
                        Action::Default => 1,
                        Action::EditMinutes => 60,
                        Action::EditHours => 3600,
                        Action::EditMonth => 30 * 86400,
                        Action::EditDayNumber => 86400,
                        _ => 0,
                    };
                    self.time_offset += delta;
                }
            }
        }
    }

    /// Advances the button-A hold timer (for the 3-second CASIO display).
    pub fn tick_button_a(&mut self) {
        if self.button_a_held && self.active_menu == Menu::DateTime {
            self.button_a_repeat_timer += 1;
            if self.button_a_repeat_timer >= 150 {
                // ~3 seconds at 20ms ticks: show CASIO.
                self.active_action = Action::Casio;
            }
        }
    }

    /// Toggles the 12/24 hour mode. Called exactly once per clean click so it
    /// never flickers or double-fires.
    pub fn toggle_time_mode(&mut self) {
        if self.active_menu == Menu::DateTime {
            self.time_mode = match self.time_mode {
                TimeMode::H24 => TimeMode::H12,
                TimeMode::H12 => TimeMode::H24,
            };
            self.active_action = Action::Default;
        }
    }

    /// Returns the current instructions for each button, mirroring the online
    /// simulator's dynamic instructions.
    pub fn instructions(&self) -> (String, String, String, String) {
        let (menu, l, c, a) = match self.active_menu {
            Menu::DateTime => (
                "Regular time keeping",
                "Backlight",
                "Switch to daily alarm",
                if self.time_mode == TimeMode::H24 {
                    "Switch to 12-hour\nHold for a surprise..."
                } else {
                    "Switch to 24-hour\nHold for a surprise..."
                },
            ),
            Menu::DailyAlarm => {
                let l = match self.active_action {
                    Action::Default => "Set alarm hour",
                    Action::EditHours => "Set alarm minutes\nBacklight",
                    Action::EditMinutes => "Exit alarm setting\nBacklight",
                    _ => "Backlight",
                };
                let a = match self.active_action {
                    Action::Default => match (self.alarm_on_mark, self.time_signal_on_mark) {
                        (true, true) => "Turn OFF alarm + signal",
                        (true, false) => "Turn OFF alarm, ON signal",
                        (false, true) => "Turn ON alarm",
                        (false, false) => "Turn ON signal",
                    },
                    Action::EditHours => "Increment hours\nHold to speed up",
                    Action::EditMinutes => "Increment minutes\nHold to speed up",
                    _ => "",
                };
                ("Daily alarm", l, "Switch to stopwatch", a)
            }
            Menu::Stopwatch => {
                let l = if self.stopwatch_running {
                    if self.stopwatch_split.is_some() {
                        "Reset split\nBacklight"
                    } else {
                        "Record split\nBacklight"
                    }
                } else if self.stopwatch_split.is_some() {
                    "Reset split\nBacklight"
                } else {
                    "Reset stopwatch\nBacklight"
                };
                let a = if self.stopwatch_running {
                    "Stop stopwatch"
                } else {
                    "Start stopwatch"
                };
                ("Stopwatch", l, "Switch to time/calendar setting", a)
            }
            Menu::SetDateTime => {
                let l = match self.active_action {
                    Action::Default => "Set minutes\nBacklight",
                    Action::EditMinutes => "Set hours\nBacklight",
                    Action::EditHours => "Set month\nBacklight",
                    Action::EditMonth => "Set date\nBacklight",
                    Action::EditDayNumber => "Set seconds\nBacklight",
                    _ => "Backlight",
                };
                let a = match self.active_action {
                    Action::Default => "Increment seconds\nHold to speed up",
                    Action::EditMinutes => "Increment minutes\nHold to speed up",
                    Action::EditHours => "Increment hours\nHold to speed up",
                    Action::EditMonth => "Increment month\nHold to speed up",
                    Action::EditDayNumber => "Increment date\nHold to speed up",
                    _ => "",
                };
                (
                    "Time/calendar setting",
                    l,
                    "Switch to regular time keeping",
                    a,
                )
            }
        };
        (
            menu.to_string(),
            l.to_string(),
            c.to_string(),
            a.to_string(),
        )
    }
}

impl Default for CasioF91W {
    fn default() -> Self {
        Self::new()
    }
}

/// Converts a count of days since the Unix epoch into a civil (year, month, day).
/// Uses Howard Hinnant's `civil_from_days` algorithm.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
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

/// Month (1-12) for a day count since the Unix epoch.
fn civil_month(days: i64) -> u32 {
    civil_from_days(days).1
}

/// Converts a civil (year, month, day) into a count of days since the Unix
/// epoch. Inverse of `civil_from_days` (Howard Hinnant's algorithm).
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64; // [0, 399]
    let mp = (m + 9) % 12; // [0, 11]
    let doy = (153 * mp as u64 + 2) / 5 + (d as u64 - 1); // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    (era as i64 * 146097 + doe as i64 - 719_468) as i64
}
