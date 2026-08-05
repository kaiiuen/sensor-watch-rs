//! Kè (Decimal Time) watch face.
//!
//! Port of the C `ke_decimal_time_face.c`. Displays the weekday and day at the
//! top, and the percentage of the day that has passed (midnight = 0%, 11:59 PM
//! = 99.9%) on the main line. It is a pure state machine: it renders on wake
//! and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc::DateTime;
use crate::watch::slcd::Indicator;

/// The Kè decimal time face state.
pub struct KeDecimalTimeFace {
    previous_day: u8,
    previous_time: u32,
}

impl KeDecimalTimeFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        KeDecimalTimeFace {
            previous_day: 0xFF,
            previous_time: 0xFFFF_FFFF,
        }
    }

    pub fn new() -> Self {
        KeDecimalTimeFace::new_static()
    }

    fn display_date(&mut self, date_time: DateTime) {
        let mut buf = [0u8; 3];
        let weekday = crate::watch::utility::get_weekday(date_time);
        watch::slcd::display_string(weekday, 0);
        buf[0] = b'0' + date_time.day / 10;
        buf[1] = b'0' + date_time.day % 10;
        watch::slcd::display_string(core::str::from_utf8(&buf[..2]).unwrap_or("  "), 2);
    }

    fn display_time(&mut self, date_time: DateTime, low_energy: bool) {
        let mut buf = [0u8; 8];
        let mut value =
            date_time.hour as u32 * 3600 + date_time.minute as u32 * 60 + date_time.second as u32;

        if value == self.previous_time {
            return;
        }

        value = value * 100;
        value = value / 864;
        buf[0] = b'0' + ((value / 1000) % 10) as u8;
        buf[1] = b'0' + ((value / 100) % 10) as u8;
        buf[2] = b'0' + ((value / 10) % 10) as u8;
        buf[3] = b'0' + (value % 10) as u8;
        buf[4] = b'#';
        buf[5] = b'o';

        // If under 10%, display 0.00 instead of 00.00.
        if value < 1000 {
            buf[0] = b' ';
        }

        // If low energy, truncate at the tens place.
        if low_energy {
            buf[3] = b' ';
        }

        watch::slcd::display_string(core::str::from_utf8(&buf[..6]).unwrap_or(""), 4);

        self.previous_time = value;
    }
}

impl WatchFace for KeDecimalTimeFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, settings: &Settings) {
        // Force re-display of date and time in EVENT_ACTIVATE.
        self.previous_day = 0xFF;
        self.previous_time = 0xFFFF_FFFF;
        if settings.alarm_enabled() {
            watch::slcd::set_indicator(Indicator::Signal);
        }
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        let date_time = movement::get_local_date_time();

        match event {
            Event::Activate => {
                if settings.alarm_enabled() {
                    watch::slcd::set_indicator(Indicator::Signal);
                }
                self.display_time(date_time, false);
                if self.previous_day != date_time.day {
                    self.display_date(date_time);
                    self.previous_day = date_time.day;
                }
            }
            Event::Tick => {
                self.display_time(date_time, false);
                if self.previous_day != date_time.day {
                    self.display_date(date_time);
                    self.previous_day = date_time.day;
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {}
            Event::Button(Button::Alarm, ButtonEvent::Up) => {}
            Event::BackgroundTask => {}
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
