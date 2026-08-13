//! Set time (hackwatch) watch face.
//!
//! Port of the C `set_time_hackwatch_face.c`. Sets the time with precise
//! second alignment. It is a pure state machine: it reacts to a single event
//! and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const NUM_SETTINGS: u8 = 7;
const TITLES: [&str; 7] = ["HR", "M1", "SE", "YR", "MO", "DA", "ZO"];

/// The set time hackwatch face state.
pub struct SetTimeHackwatchFace {
    current_page: u8,
    date_time_settings: rtc::DateTime,
    seconds_reset_sequence: i8,
}

impl SetTimeHackwatchFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        SetTimeHackwatchFace {
            current_page: 3,
            date_time_settings: rtc::DateTime {
                second: 0,
                minute: 0,
                hour: 0,
                day: 0,
                month: 0,
                year: 0,
            },
            seconds_reset_sequence: 0,
        }
    }

    pub fn new() -> Self {
        SetTimeHackwatchFace::new_static()
    }
}

impl WatchFace for SetTimeHackwatchFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.current_page = 3;
        self.date_time_settings = rtc::get_date_time();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        if 0 == 15 {
            self.date_time_settings = rtc::get_date_time();
        }
        match event {
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                movement::move_to_next_face();
                return;
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.current_page = (self.current_page + NUM_SETTINGS - 1) % NUM_SETTINGS;
                if self.current_page == 2 {
                    self.seconds_reset_sequence = 0;
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.current_page = (self.current_page + 1) % NUM_SETTINGS;
                if self.current_page == 2 {
                    self.seconds_reset_sequence = 0;
                }
            }
            Event::Tick => {
                if self.current_page == 2 && self.seconds_reset_sequence == 1 && 0 == 15 {
                    self.seconds_reset_sequence = 2;
                    if self.date_time_settings.second > 30 {
                        self.date_time_settings.minute = (self.date_time_settings.minute + 1) % 60;
                        if self.date_time_settings.minute == 0 {
                            self.date_time_settings.hour = (self.date_time_settings.hour + 1) % 24;
                            if self.date_time_settings.hour == 0 {
                                self.date_time_settings.day += 1;
                            }
                        }
                    }
                    self.date_time_settings.second = 0;
                    let _ = rtc::set_date_time(self.date_time_settings);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Down) => {
                if self.current_page == 2 {
                    self.seconds_reset_sequence = 1;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                match self.current_page {
                    0 => {
                        self.date_time_settings.hour = (self.date_time_settings.hour + 24 - 1) % 24;
                    }
                    1 => {
                        self.date_time_settings.minute =
                            (self.date_time_settings.minute + 60 - 1) % 60;
                    }
                    3 => {
                        self.date_time_settings.year = (self.date_time_settings.year + 50 - 1) % 50;
                    }
                    4 => {
                        self.date_time_settings.month =
                            (self.date_time_settings.month + 12 - 2) % 12 + 1;
                    }
                    5 => {
                        self.date_time_settings.day = self.date_time_settings.day - 2;
                        if self.date_time_settings.day == 0 {
                            self.date_time_settings.day = utility::days_in_month(
                                self.date_time_settings.month,
                                self.date_time_settings.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
                            );
                        } else {
                            self.date_time_settings.day += 1;
                        }
                    }
                    6 => {
                        if settings.time_zone() > 0 {
                            settings.set_time_zone(settings.time_zone() - 1);
                        } else {
                            settings.set_time_zone(40);
                        }
                    }
                    _ => {}
                }
                if self.current_page != 2 {
                    let _ = rtc::set_date_time(self.date_time_settings);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongUp) => {
                if self.current_page == 2 {
                    self.seconds_reset_sequence = 0;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                match self.current_page {
                    0 => self.date_time_settings.hour = (self.date_time_settings.hour + 1) % 24,
                    1 => self.date_time_settings.minute = (self.date_time_settings.minute + 1) % 60,
                    2 => {
                        self.seconds_reset_sequence = 0;
                    }
                    3 => self.date_time_settings.year = (self.date_time_settings.year % 50) + 1,
                    4 => self.date_time_settings.month = (self.date_time_settings.month % 12) + 1,
                    5 => self.date_time_settings.day += 1,
                    6 => {
                        settings.set_time_zone(settings.time_zone() + 1);
                        if settings.time_zone() > 40 {
                            settings.set_time_zone(0);
                        }
                    }
                    _ => {}
                }
                if self.date_time_settings.day
                    > utility::days_in_month(
                        self.date_time_settings.month,
                        self.date_time_settings.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
                    )
                {
                    self.date_time_settings.day = 1;
                }
                if self.current_page != 2 {
                    let _ = rtc::set_date_time(self.date_time_settings);
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            _ => movement::default_loop_handler(event, settings),
        }

        let mut buf = [0u8; 11];
        let mut set_leading_zero = false;
        let title = TITLES[self.current_page as usize].as_bytes();
        buf[0] = title[0];
        buf[1] = title[1];
        buf[2] = b' ';
        buf[3] = b' ';
        let dt = self.date_time_settings;
        if self.current_page < 3 {
            watch::slcd::set_colon();
            let hour = if settings.clock_mode_24h() {
                if !settings.clock_24h_leading_zero() {
                    watch::slcd::set_indicator(Indicator::H24);
                } else if dt.hour < 10 {
                    set_leading_zero = true;
                }
                dt.hour
            } else {
                let h = if dt.hour % 12 != 0 { dt.hour % 12 } else { 12 };
                if dt.hour < 12 {
                    watch::slcd::clear_indicator(Indicator::Pm);
                } else {
                    watch::slcd::set_indicator(Indicator::Pm);
                }
                h
            };
            buf[4] = b'0' + hour / 10;
            buf[5] = b'0' + hour % 10;
            buf[6] = b'0' + dt.minute / 10;
            buf[7] = b'0' + dt.minute % 10;
            buf[8] = b'0' + dt.second / 10;
            buf[9] = b'0' + dt.second % 10;
        } else if self.current_page < 6 {
            watch::slcd::clear_colon();
            watch::slcd::clear_indicator(Indicator::H24);
            watch::slcd::clear_indicator(Indicator::Pm);
            buf[4] = b'0' + (dt.year + 20) / 10;
            buf[5] = b'0' + (dt.year + 20) % 10;
            buf[6] = b'0' + dt.month / 10;
            buf[7] = b'0' + dt.month % 10;
            buf[8] = b'0' + dt.day / 10;
            buf[9] = b'0' + dt.day % 10;
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
    }

    fn resign(&mut self, _settings: &mut Settings) {
        watch::led::set_led_off();
        crate::movement::save_settings();
    }
}
