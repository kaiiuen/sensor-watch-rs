//! Habit tracker watch face.
//!
//! Port of the C `habit_face.c`. Tracks a daily habit with a lookback history
//! and a total count. It is a pure state machine: it reacts to a single event
//! and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

/// The habit face state.
pub struct HabitFace {
    total_count: u16,
    lookback: u8,
    last_update: u32,
    display_total: bool,
}

impl HabitFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        HabitFace {
            total_count: 0,
            lookback: 0,
            last_update: 0,
            display_total: false,
        }
    }

    pub fn new() -> Self {
        HabitFace::new_static()
    }

    fn today_unix(&self, settings: &Settings) -> u32 {
        let dt = rtc::get_date_time();
        let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60)
            as u32;
        utility::convert_to_unix_time(
            dt.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            dt.month,
            dt.day,
            0,
            0,
            0,
            tz,
        )
    }

    fn days_since_unix(since: u32, until: u32) -> u8 {
        ((until - since) / (60 * 60 * 24)) as u8
    }

    fn display_state(&self) {
        let can_do = (self.lookback & 1) == 0;
        if can_do {
            watch::slcd::clear_indicator(Indicator::Lap);
        } else {
            watch::slcd::set_indicator(Indicator::Lap);
        }

        let mut buf = [0u8; 11];
        if self.display_total {
            buf[0] = b'H';
            buf[1] = b'A';
            buf[2] = b' ';
            buf[3] = b' ';
            buf[4] = b'0' + (self.total_count / 100) as u8;
            buf[5] = b'0' + ((self.total_count / 10) % 10) as u8;
            buf[6] = b'0' + (self.total_count % 10) as u8;
            buf[7] = b't';
            buf[8] = b'o';
            buf[9] = b't';
        } else {
            buf[0] = b'H';
            buf[1] = b'A';
            buf[2] = b' ';
            buf[3] = b' ';
            buf[4] = b' ';
            buf[5] = b' ';
            buf[6] = b' ';
            buf[7] = b'd';
            buf[8] = if can_do { b'o' } else { b'n' };
            let mut copy = self.lookback;
            let mut c = 0;
            while copy != 0 {
                match copy & 3 {
                    1 => buf[4 + c] = b'I',
                    2 => buf[4 + c] = b'1',
                    3 => buf[4 + c] = b'|',
                    _ => {}
                }
                copy >>= 2;
                c += 1;
            }
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

impl WatchFace for HabitFace {
    fn setup(&mut self, settings: &Settings, _watch_face_index: usize) {
        self.lookback = 0;
        let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60)
            as u32;
        self.last_update = utility::offset_timestamp(self.today_unix(settings), -24, 0, 0);
        let _ = tz;
    }

    fn activate(&mut self, _settings: &Settings) {
        self.display_state();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        let today_now_unix = self.today_unix(settings);
        let can_do = (self.lookback & 1) == 0;
        match event {
            Event::Activate | Event::Tick => {
                self.display_state();
                if today_now_unix > self.last_update {
                    let mut num_shifts = Self::days_since_unix(self.last_update, today_now_unix);
                    if num_shifts > 7 {
                        num_shifts = 7;
                    }
                    self.lookback <<= num_shifts;
                    self.last_update = today_now_unix;
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.display_total = !self.display_total;
                self.display_state();
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if can_do {
                    self.lookback |= 1;
                    self.total_count += 1;
                    self.last_update = today_now_unix;
                    self.display_state();
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
