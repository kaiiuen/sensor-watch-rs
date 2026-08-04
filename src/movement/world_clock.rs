//! World clock watch face.
//!
//! Port of the C `world_clock_face.c`, adapted to the event-driven model.
//! Shows the time in a selected time zone, with configurable label characters.

use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::movement::{self, TIMEZONE_OFFSETS};
use crate::watch;
use crate::watch::rtc::{self, DateTime};
use crate::watch::slcd::Indicator;
use crate::watch::utility;

/// Valid characters for position 0 of the label.
const VALID_POS_0: &str = " AaBbCcDdEeFGgHhIiJKLMNnOoPQrSTtUuWXYZ-='+\\/0123456789";
/// Valid characters for position 1 of the label.
const VALID_POS_1: &str = " ABCDEFHlJLNORTtUX-='01378";

/// The world clock face state.
pub struct WorldClockFace {
    char_0: u8,
    char_1: u8,
    timezone_index: u8,
    current_screen: u8,
    previous_date_time: u32,
}

impl WorldClockFace {
    pub const fn new_static() -> Self {
        WorldClockFace {
            char_0: 0,
            char_1: 0,
            timezone_index: 0,
            current_screen: 0,
            previous_date_time: 0xFFFF_FFFF,
        }
    }

    fn tz_offset(index: u8) -> u32 {
        (TIMEZONE_OFFSETS[(index as usize).min(40)] as i32 * 60) as u32
    }

    fn do_display_mode(&mut self, event: Event, settings: &Settings) {
        let mut buf = [0u8; 11];
        let pos: u8;

        match event {
            Event::Activate => {
                if settings.clock_mode_24h() && !settings.clock_24h_leading_zero() {
                    watch::slcd::set_indicator(Indicator::H24);
                }
                watch::slcd::set_colon();
                self.previous_date_time = 0xFFFF_FFFF;
                self.render_full(&mut buf, settings);
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            Event::Tick => {
                let date_time = rtc::get_date_time();
                let timestamp = utility::date_time_to_unix_time(
                    date_time,
                    Self::tz_offset(settings.time_zone()),
                );
                let dt = utility::date_time_from_unix_time(
                    timestamp,
                    Self::tz_offset(self.timezone_index),
                );
                let previous = self.previous_date_time;
                self.previous_date_time = dt.to_reg();

                if (dt.to_reg() >> 6) == (previous >> 6) {
                    // Only seconds changed.
                    pos = 8;
                    buf[0] = b'0' + dt.second / 10;
                    buf[1] = b'0' + dt.second % 10;
                } else if (dt.to_reg() >> 12) == (previous >> 12) {
                    // Minutes and seconds changed.
                    pos = 6;
                    buf[0] = b'0' + dt.minute / 10;
                    buf[1] = b'0' + dt.minute % 10;
                    buf[2] = b'0' + dt.second / 10;
                    buf[3] = b'0' + dt.second % 10;
                } else {
                    pos = 0;
                    self.render_full(&mut buf, settings);
                }
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), pos);
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.current_screen = 1;
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn render_full(&self, buf: &mut [u8; 11], settings: &Settings) {
        let date_time = rtc::get_date_time();
        let timestamp =
            utility::date_time_to_unix_time(date_time, Self::tz_offset(settings.time_zone()));
        let dt = utility::date_time_from_unix_time(timestamp, Self::tz_offset(self.timezone_index));

        let mut hour = dt.hour;
        if !settings.clock_mode_24h() {
            if hour < 12 {
                watch::slcd::clear_indicator(Indicator::Pm);
            } else {
                watch::slcd::set_indicator(Indicator::Pm);
            }
            hour %= 12;
            if hour == 0 {
                hour = 12;
            }
        }

        let c0 = VALID_POS_0.as_bytes()[(self.char_0 as usize).min(VALID_POS_0.len() - 1)];
        let c1 = VALID_POS_1.as_bytes()[(self.char_1 as usize).min(VALID_POS_1.len() - 1)];
        buf[0] = c0;
        buf[1] = c1;
        buf[2] = b'0' + dt.day / 10;
        buf[3] = b'0' + dt.day % 10;
        buf[4] = b'0' + hour / 10;
        buf[5] = b'0' + hour % 10;
        buf[6] = b'0' + dt.minute / 10;
        buf[7] = b'0' + dt.minute % 10;
        buf[8] = b'0' + dt.second / 10;
        buf[9] = b'0' + dt.second % 10;
    }

    fn do_settings_mode(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                movement::move_to_next_face();
                return;
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                self.current_screen += 1;
                if self.current_screen > 3 {
                    self.current_screen = 0;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Down) => match self.current_screen {
                1 => {
                    self.char_0 = (self.char_0 + 1) % VALID_POS_0.len() as u8;
                }
                2 => {
                    self.char_1 = (self.char_1 + 1) % VALID_POS_1.len() as u8;
                }
                3 => {
                    self.timezone_index = (self.timezone_index + 1) % 41;
                }
                _ => {}
            },
            _ => {}
        }

        let mut buf = [0u8; 11];
        let c0 = VALID_POS_0.as_bytes()[(self.char_0 as usize).min(VALID_POS_0.len() - 1)];
        let c1 = VALID_POS_1.as_bytes()[(self.char_1 as usize).min(VALID_POS_1.len() - 1)];
        buf[0] = c0;
        buf[1] = c1;
        buf[2] = b' ';
        let offset = TIMEZONE_OFFSETS[(self.timezone_index as usize).min(40)];
        let hours = (offset / 60) as i8;
        let mins = (offset % 60).unsigned_abs();
        buf[3] = if hours < 0 { b'-' } else { b' ' };
        buf[4] = b'0' + hours.unsigned_abs() / 10;
        buf[5] = b'0' + hours.unsigned_abs() % 10;
        buf[6] = b'0' + (mins / 10) as u8;
        buf[7] = b'0' + (mins % 10) as u8;
        watch::slcd::set_colon();
        watch::slcd::clear_indicator(Indicator::Pm);
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

impl WatchFace for WorldClockFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.current_screen = 0;
        if watch::slcd::tick_animation_is_running() {
            watch::slcd::stop_tick_animation();
        }
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        if self.current_screen == 0 {
            self.do_display_mode(event, settings);
        } else {
            self.do_settings_mode(event, settings);
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
