//! Set time watch face.
//!
//! Port of the C `set_time_face.c`. Lets the user set the time, date, and time
//! zone. It is a pure state machine: it reacts to a single event and returns;
//! it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const SET_TIME_FACE_NUM_SETTINGS: u8 = 7;
const TITLES: [&str; 7] = ["HR", "M1", "SE", "YR", "MO", "DA", "ZO"];
const RTC_YEAR_COUNT: u8 = 64;

/// The set time face state.
pub struct SetTimeFace {
    current_page: u8,
    quick_ticks_running: bool,
    rtc_write_failed: bool,
}

impl SetTimeFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        SetTimeFace {
            current_page: 0,
            quick_ticks_running: false,
            rtc_write_failed: false,
        }
    }

    pub fn new() -> Self {
        SetTimeFace::new_static()
    }

    fn handle_alarm_button(&mut self, settings: &mut Settings, mut date_time: rtc::DateTime) {
        match self.current_page {
            0 => date_time.hour = (date_time.hour + 1) % 24,
            1 => date_time.minute = (date_time.minute + 1) % 60,
            2 => date_time.second = 0,
            3 => date_time.year = (date_time.year + 1) % RTC_YEAR_COUNT,
            4 => date_time.month = (date_time.month % 12) + 1,
            5 => date_time.day += 1,
            6 => {
                settings.set_time_zone(settings.time_zone() + 1);
                if settings.time_zone() > 40 {
                    settings.set_time_zone(0);
                }
            }
            _ => {}
        }
        if date_time.day
            > utility::days_in_month(
                date_time.month,
                date_time.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            )
        {
            date_time.day = 1;
        }
        self.rtc_write_failed = rtc::set_date_time(date_time).is_err();
    }

    fn abort_quick_ticks(&mut self) {
        self.quick_ticks_running = false;
    }
}

impl WatchFace for SetTimeFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.current_page = 0;
        self.quick_ticks_running = false;
        self.rtc_write_failed = false;
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        let mut date_time = rtc::get_date_time();
        match event {
            Event::Tick => {
                if self.quick_ticks_running {
                    if watch::gpio::get_pin_level(watch::extint::BTN_ALARM) {
                        self.handle_alarm_button(settings, date_time);
                    } else {
                        self.abort_quick_ticks();
                    }
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.current_page != 2 {
                    self.quick_ticks_running = true;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongUp) => self.abort_quick_ticks(),
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                self.abort_quick_ticks();
                movement::move_to_next_face();
                return;
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                self.current_page = (self.current_page + 1) % SET_TIME_FACE_NUM_SETTINGS;
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.abort_quick_ticks();
                self.handle_alarm_button(settings, date_time);
            }
            _ => movement::default_loop_handler(event, settings),
        }

        date_time = rtc::get_date_time();
        let mut buf = [0u8; 11];
        let mut set_leading_zero = false;
        let title = TITLES[self.current_page as usize].as_bytes();
        buf[0] = title[0];
        buf[1] = title[1];
        buf[2] = b' ';
        buf[3] = b' ';
        if self.current_page < 3 {
            watch::slcd::set_colon();
            let hour = if settings.clock_mode_24h() {
                if !settings.clock_24h_leading_zero() {
                    watch::slcd::set_indicator(Indicator::H24);
                } else if date_time.hour < 10 {
                    set_leading_zero = true;
                }
                date_time.hour
            } else {
                let h = if date_time.hour % 12 != 0 {
                    date_time.hour % 12
                } else {
                    12
                };
                if date_time.hour < 12 {
                    watch::slcd::clear_indicator(Indicator::Pm);
                } else {
                    watch::slcd::set_indicator(Indicator::Pm);
                }
                h
            };
            buf[4] = b'0' + hour / 10;
            buf[5] = b'0' + hour % 10;
            buf[6] = b'0' + date_time.minute / 10;
            buf[7] = b'0' + date_time.minute % 10;
            buf[8] = b'0' + date_time.second / 10;
            buf[9] = b'0' + date_time.second % 10;
        } else if self.current_page < 6 {
            watch::slcd::clear_colon();
            watch::slcd::clear_indicator(Indicator::H24);
            watch::slcd::clear_indicator(Indicator::Pm);
            buf[4] = b'0' + (date_time.year + 20) / 10;
            buf[5] = b'0' + (date_time.year + 20) % 10;
            buf[6] = b'0' + date_time.month / 10;
            buf[7] = b'0' + date_time.month % 10;
            buf[8] = b'0' + date_time.day / 10;
            buf[9] = b'0' + date_time.day % 10;
        } else {
            watch::slcd::set_colon();
            let offset = movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)];
            let hours = offset / 60;
            let mins = offset % 60;
            buf[4] = if hours < 0 { b'-' } else { b' ' };
            buf[5] = b'0' + (hours.unsigned_abs() / 10) as u8;
            buf[6] = b'0' + (hours.unsigned_abs() % 10) as u8;
            buf[7] = b'0' + (mins.unsigned_abs() / 10) as u8;
            buf[8] = b'0' + (mins.unsigned_abs() % 10) as u8;
            buf[9] = b' ';
        }

        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        if set_leading_zero {
            watch::slcd::display_string("0", 4);
        }
        if self.rtc_write_failed {
            watch::slcd::display_string("Err", 0);
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        watch::led::set_led_off();
        crate::movement::save_settings();
    }
}
