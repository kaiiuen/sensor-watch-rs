//! Deadline countdown watch face.
//!
//! Port of the C `deadline_face.c`. Tracks up to four deadlines and shows the
//! time remaining, with an optional background alarm. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const DEADLINE_FACE_DATES: usize = 4;
const SETTINGS_NUM: u8 = 5;
const SETTINGS_TITLES: [&str; 5] = ["YR", "MO", "DA", "HR", "M1"];

/// The deadline mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DeadlineMode {
    Running,
    Setting,
}

/// The deadline face state.
pub struct DeadlineFace {
    face_idx: usize,
    mode: DeadlineMode,
    current_index: usize,
    current_page: u8,
    alarm_enabled: bool,
    deadlines: [u32; DEADLINE_FACE_DATES],
}

impl DeadlineFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        DeadlineFace {
            face_idx: 0,
            mode: DeadlineMode::Running,
            current_index: 0,
            current_page: 0,
            alarm_enabled: false,
            deadlines: [0; DEADLINE_FACE_DATES],
        }
    }

    pub fn new() -> Self {
        DeadlineFace::new_static()
    }

    fn tz_offset(settings: &Settings) -> u32 {
        (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60) as u32
    }

    fn is_leap(y: i16) -> bool {
        let y = y + 1900;
        y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
    }

    fn days_in_month(month: i16, year: i16) -> i32 {
        let days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let month = ((month - 1) % 12 + 12) % 12;
        if month == 1 && Self::is_leap(year) {
            days[month as usize] + 1
        } else {
            days[month as usize]
        }
    }

    fn closest_deadline(&self, settings: &Settings) -> usize {
        let now = rtc::get_date_time();
        let now_ts = utility::date_time_to_unix_time(now, Self::tz_offset(settings));
        let mut min_ts = u32::MAX;
        let mut min_index = 0;
        for i in 0..DEADLINE_FACE_DATES {
            if self.deadlines[i] < now_ts || self.deadlines[i] > min_ts {
                continue;
            }
            min_ts = self.deadlines[i];
            min_index = i;
        }
        min_index
    }

    fn reset_deadline(&mut self, settings: &Settings) {
        let mut date_time = rtc::get_date_time();
        date_time.second = 0;
        date_time.minute = 0;
        date_time.hour = 0;
        let mut ts = utility::date_time_to_unix_time(date_time, Self::tz_offset(settings));
        ts += 24 * 60 * 60;
        self.deadlines[self.current_index] = ts;
    }

    fn increment_date(&mut self, settings: &Settings, mut date_time: rtc::DateTime) {
        let days_in_month = [31, 28, 31, 30, 31, 30, 30, 31, 30, 31, 30, 31];
        match self.current_page {
            0 => {
                date_time.year = (date_time.year % 10) + 1;
            }
            1 => {
                date_time.month = (date_time.month % 12) + 1;
            }
            2 => {
                date_time.day = date_time.day + 1;
                let mut days = days_in_month[(date_time.month - 1) as usize];
                if date_time.month == 2 && Self::is_leap(date_time.year as i16) {
                    days += 1;
                }
                if date_time.day > days {
                    date_time.day = 1;
                }
            }
            3 => {
                date_time.hour = (date_time.hour + 1) % 24;
            }
            _ => {
                date_time.minute = (date_time.minute + 1) % 60;
            }
        }
        let ts = utility::date_time_to_unix_time(date_time, Self::tz_offset(settings));
        self.deadlines[self.current_index] = ts;
    }

    fn running_display(&self, settings: &Settings) {
        let mut buf = [0u8; 11];
        if self.alarm_enabled {
            watch::slcd::set_indicator(Indicator::Bell);
        } else {
            watch::slcd::clear_indicator(Indicator::Bell);
        }
        let now = rtc::get_date_time();
        let now_ts = utility::date_time_to_unix_time(now, Self::tz_offset(settings));
        let deadline_ts = self.deadlines[self.current_index];

        if deadline_ts < now_ts {
            let over = if deadline_ts + 24 * 60 * 60 > now_ts {
                "OVER  "
            } else {
                "----  "
            };
            buf[0] = b'D';
            buf[1] = b'L';
            buf[2] = b'0' + ((self.current_index + 1) / 10) as u8;
            buf[3] = b'0' + ((self.current_index + 1) % 10) as u8;
            let ob = over.as_bytes();
            for (i, &c) in ob.iter().enumerate() {
                buf[4 + i] = c;
            }
            watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            return;
        }

        let deadline = utility::date_time_from_unix_time(deadline_ts, Self::tz_offset(settings));
        let mut unit = [0i16; 6];
        unit[0] = deadline.second as i16 - now.second as i16;
        unit[1] = deadline.minute as i16 - now.minute as i16;
        unit[2] = deadline.hour as i16 - now.hour as i16;
        unit[3] = deadline.day as i16 - now.day as i16;
        unit[4] = deadline.month as i16 - now.month as i16;
        unit[5] = deadline.year as i16 - now.year as i16;
        let range = [60, 60, 24, 30, 12, 0];
        for i in 0..6 {
            if unit[i] < 0 {
                if i == 3 {
                    unit[i] +=
                        Self::days_in_month(deadline.month as i16 - 1, deadline.year as i16) as i16;
                } else {
                    unit[i] += range[i];
                }
                if i < 5 && unit[i + 1] != 0 {
                    unit[i + 1] -= 1;
                }
            }
        }

        let i = self.current_index + 1;
        buf[0] = b'D';
        buf[1] = b'L';
        buf[2] = b'0' + (i / 10) as u8;
        buf[3] = b'0' + (i % 10) as u8;
        if unit[5] > 0 {
            buf[4] = b'0' + ((unit[5] % 100) / 10) as u8;
            buf[5] = b'0' + (unit[5] % 10) as u8;
            buf[6] = b'0' + ((unit[4] % 12) / 10) as u8;
            buf[7] = b'0' + (unit[4] % 12) as u8;
            buf[8] = b'Y';
            buf[9] = b'R';
        } else if unit[4] > 0 {
            buf[4] = b'0' + (((unit[5] * 12 + unit[4]) % 100) / 10) as u8;
            buf[5] = b'0' + ((unit[5] * 12 + unit[4]) % 10) as u8;
            buf[6] = b'0' + ((unit[3] % 32) / 10) as u8;
            buf[7] = b'0' + (unit[3] % 32) as u8;
            buf[8] = b'M';
            buf[9] = b'O';
        } else if unit[3] > 0 {
            buf[4] = b'0' + ((unit[3] % 32) / 10) as u8;
            buf[5] = b'0' + (unit[3] % 32) as u8;
            buf[6] = b'0' + ((unit[2] % 24) / 10) as u8;
            buf[7] = b'0' + (unit[2] % 24) as u8;
            buf[8] = b'd';
            buf[9] = b'Y';
        } else {
            buf[4] = b'0' + ((unit[2] % 24) / 10) as u8;
            buf[5] = b'0' + (unit[2] % 24) as u8;
            buf[6] = b'0' + ((unit[1] % 60) / 10) as u8;
            buf[7] = b'0' + (unit[1] % 60) as u8;
            buf[8] = b'0' + ((unit[0] % 60) / 10) as u8;
            buf[9] = b'0' + (unit[0] % 60) as u8;
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn setting_display(&self, settings: &Settings, subsecond: u8) {
        let mut buf = [0u8; 11];
        let date_time = utility::date_time_from_unix_time(
            self.deadlines[self.current_index],
            Self::tz_offset(settings),
        );
        let i = self.current_index + 1;
        let title = SETTINGS_TITLES[self.current_page as usize].as_bytes();
        if self.current_page > 2 {
            watch::slcd::set_colon();
            if settings.clock_mode_24h() {
                watch::slcd::set_indicator(Indicator::H24);
                buf[0] = title[0];
                buf[1] = title[1];
                buf[2] = b'0' + (i / 10) as u8;
                buf[3] = b'0' + (i % 10) as u8;
                buf[4] = b'0' + date_time.hour / 10;
                buf[5] = b'0' + date_time.hour % 10;
                buf[6] = b'0' + date_time.minute / 10;
                buf[7] = b'0' + date_time.minute % 10;
            } else {
                let mut hour = date_time.hour % 12;
                if hour == 0 {
                    hour = 12;
                }
                if date_time.hour < 12 {
                    watch::slcd::clear_indicator(Indicator::Pm);
                } else {
                    watch::slcd::set_indicator(Indicator::Pm);
                }
                buf[0] = title[0];
                buf[1] = title[1];
                buf[2] = b'0' + (i / 10) as u8;
                buf[3] = b'0' + (i % 10) as u8;
                buf[4] = b'0' + hour / 10;
                buf[5] = b'0' + hour % 10;
                buf[6] = b'0' + date_time.minute / 10;
                buf[7] = b'0' + date_time.minute % 10;
            }
        } else {
            watch::slcd::clear_colon();
            watch::slcd::clear_indicator(Indicator::H24);
            watch::slcd::clear_indicator(Indicator::Pm);
            buf[0] = title[0];
            buf[1] = title[1];
            buf[2] = b'0' + (i / 10) as u8;
            buf[3] = b'0' + (i % 10) as u8;
            buf[4] = b'0' + (date_time.year + 20) / 10;
            buf[5] = b'0' + (date_time.year + 20) % 10;
            buf[6] = b'0' + date_time.month / 10;
            buf[7] = b'0' + date_time.month % 10;
            buf[8] = b'0' + date_time.day / 10;
            buf[9] = b'0' + date_time.day % 10;
        }
        if subsecond % 2 == 1 {
            match self.current_page {
                0 | 3 => {
                    buf[4] = b' ';
                    buf[5] = b' ';
                }
                1 | 4 => {
                    buf[6] = b' ';
                    buf[7] = b' ';
                }
                _ => {
                    buf[8] = b' ';
                    buf[9] = b' ';
                }
            }
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

impl WatchFace for DeadlineFace {
    fn setup(&mut self, _settings: &Settings, watch_face_index: usize) {
        self.face_idx = watch_face_index;
    }

    fn activate(&mut self, settings: &Settings) {
        watch::slcd::clear_indicator(Indicator::H24);
        watch::slcd::clear_indicator(Indicator::Pm);
        watch::slcd::set_colon();
        self.mode = DeadlineMode::Running;
        self.current_index = self.closest_deadline(settings);
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match self.mode {
            DeadlineMode::Running => {
                if event != Event::BackgroundTask {
                    self.running_display(settings);
                }
                match event {
                    Event::Button(Button::Alarm, ButtonEvent::Up) => {
                        self.current_index = (self.current_index + 1) % DEADLINE_FACE_DATES;
                        self.running_display(settings);
                    }
                    Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                        self.current_page = 0;
                        if self.deadlines[self.current_index] == 0 {
                            self.reset_deadline(settings);
                        }
                        self.mode = DeadlineMode::Setting;
                    }
                    Event::Button(Button::Mode, ButtonEvent::Up) => movement::move_to_next_face(),
                    Event::Button(Button::Light, ButtonEvent::Down) => {}
                    Event::Button(Button::Light, ButtonEvent::LongPress) => {
                        self.alarm_enabled = !self.alarm_enabled;
                        if self.alarm_enabled {
                            self.running_display(settings);
                        } else {
                            movement::cancel_background_task_for_face(self.face_idx);
                        }
                        self.running_display(settings);
                    }
                    Event::BackgroundTask => {
                        if self.alarm_enabled {
                            movement::play_alarm();
                            movement::move_to_face(self.face_idx);
                        }
                    }
                    _ => movement::default_loop_handler(event, settings),
                }
            }
            DeadlineMode::Setting => {
                let date_time = utility::date_time_from_unix_time(
                    self.deadlines[self.current_index],
                    Self::tz_offset(settings),
                );
                if event != Event::BackgroundTask {
                    self.setting_display(settings, 0);
                }
                match event {
                    Event::Button(Button::Light, ButtonEvent::LongPress) => {
                        self.reset_deadline(settings);
                    }
                    Event::Button(Button::Light, ButtonEvent::Down) => {}
                    Event::Button(Button::Light, ButtonEvent::Up) => {
                        self.current_page = (self.current_page + 1) % SETTINGS_NUM;
                        self.setting_display(settings, 0);
                    }
                    Event::Button(Button::Alarm, ButtonEvent::Up) => {
                        self.increment_date(settings, date_time);
                        self.setting_display(settings, 0);
                    }
                    Event::Button(Button::Mode, ButtonEvent::Up) => {
                        watch::slcd::clear_indicator(Indicator::H24);
                        watch::slcd::clear_indicator(Indicator::Pm);
                        watch::slcd::set_colon();
                        self.mode = DeadlineMode::Running;
                        self.running_display(settings);
                    }
                    Event::BackgroundTask => {
                        if self.alarm_enabled {
                            movement::play_alarm();
                            movement::move_to_face(self.face_idx);
                        }
                    }
                    _ => movement::default_loop_handler(event, settings),
                }
            }
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
