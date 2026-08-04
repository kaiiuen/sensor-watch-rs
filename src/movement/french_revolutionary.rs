//! French Revolutionary decimal time watch face.
//!
//! Port of the C `french_revolutionary_face.c`. Shows the time in decimal
//! (10-hour day) format, with optional date or normal-time display. It is a
//! pure state machine: it renders on wake and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;

/// A decimal-time value (10-hour day, 100 minutes per hour, 100 seconds per minute).
struct FrDecimalTime {
    hour: u8,
    minute: u8,
    second: u8,
}

fn get_decimal_time(date_time: &rtc::DateTime) -> FrDecimalTime {
    let current_24hr_secs =
        date_time.hour as u32 * 3600 + date_time.minute as u32 * 60 + date_time.second as u32;
    let mut current_decimal_seconds = current_24hr_secs * 1000 / 864;
    let hour = (current_decimal_seconds / 10000) as u8;
    current_decimal_seconds -= (hour as u32) * 10000;
    let minute = (current_decimal_seconds / 100) as u8;
    let second = (current_decimal_seconds - (minute as u32) * 100) as u8;
    FrDecimalTime {
        hour,
        minute,
        second,
    }
}

/// Fixes the second character of the day field for digits that the LCD can't show well.
fn fix_character_one(digit: u8) -> u8 {
    match digit {
        b'2' => b'|',
        b'4' => b'&',
        b'5' => b'F',
        b'6' => b'E',
        b'9' => b'N',
        other => other,
    }
}

/// The French Revolutionary face state.
pub struct FrenchRevolutionaryFace {
    use_am_pm: bool,
    show_seconds: bool,
    display_type: u8,
    colon_set_after_splash: bool,
}

impl FrenchRevolutionaryFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        FrenchRevolutionaryFace {
            use_am_pm: false,
            show_seconds: true,
            display_type: 0,
            colon_set_after_splash: false,
        }
    }

    pub fn new() -> Self {
        FrenchRevolutionaryFace::new_static()
    }

    fn set_display_buffer(&self, buf: &mut [u8; 11], dt: &rtc::DateTime, decimal: &FrDecimalTime) {
        match self.display_type {
            0 => {
                buf[0] = b'd';
                buf[1] = b'T';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b'0' + decimal.hour / 10;
                buf[5] = b'0' + decimal.hour % 10;
                buf[6] = b'0' + decimal.minute / 10;
                buf[7] = b'0' + decimal.minute % 10;
                buf[8] = b'0' + decimal.second / 10;
                buf[9] = b'0' + decimal.second % 10;
                watch::slcd::clear_indicator(Indicator::Pm);
                watch::slcd::clear_indicator(Indicator::H24);
            }
            1 => {
                buf[0] = b'd';
                buf[1] = b'T';
                buf[2] = b'0' + dt.day / 10;
                buf[3] = b'0' + dt.day % 10;
                buf[4] = b'0' + decimal.hour / 10;
                buf[5] = b'0' + decimal.hour % 10;
                buf[6] = b'0' + decimal.minute / 10;
                buf[7] = b'0' + decimal.minute % 10;
                buf[8] = b'0' + decimal.second / 10;
                buf[9] = b'0' + decimal.second % 10;
                watch::slcd::clear_indicator(Indicator::Pm);
                watch::slcd::clear_indicator(Indicator::H24);
            }
            _ => {
                let mut hour = dt.hour;
                if self.use_am_pm {
                    watch::slcd::clear_indicator(Indicator::H24);
                    if hour < 12 {
                        watch::slcd::clear_indicator(Indicator::Pm);
                    } else {
                        watch::slcd::set_indicator(Indicator::Pm);
                    }
                    hour %= 12;
                    if hour == 0 {
                        hour = 12;
                    }
                } else {
                    watch::slcd::clear_indicator(Indicator::Pm);
                    watch::slcd::set_indicator(Indicator::H24);
                }
                buf[0] = b'0' + dt.minute / 10;
                buf[1] = fix_character_one(b'0' + dt.minute % 10);
                buf[2] = b'0' + hour / 10;
                buf[3] = b'0' + hour % 10;
                buf[4] = b'0' + decimal.hour / 10;
                buf[5] = b'0' + decimal.hour % 10;
                buf[6] = b'0' + decimal.minute / 10;
                buf[7] = b'0' + decimal.minute % 10;
                buf[8] = b'0' + decimal.second / 10;
                buf[9] = b'0' + decimal.second % 10;
            }
        }
        if !self.show_seconds {
            buf[8] = b' ';
            buf[9] = b' ';
        }
    }
}

impl WatchFace for FrenchRevolutionaryFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.colon_set_after_splash = false;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => {
                watch::slcd::clear_display();
                watch::slcd::display_string("FR  dECimL", 0);
            }
            Event::Tick => {
                let date_time = rtc::get_date_time();
                let decimal = get_decimal_time(&date_time);
                let mut buf = [0u8; 11];
                self.set_display_buffer(&mut buf, &date_time, &decimal);
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
                if !self.colon_set_after_splash {
                    watch::slcd::set_colon();
                    self.colon_set_after_splash = true;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.display_type += 1;
                if self.display_type > 2 {
                    self.display_type = 0;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.show_seconds = !self.show_seconds;
                if !self.show_seconds {
                    watch::slcd::display_string("  ", 8);
                } else {
                    watch::slcd::display_string("--", 8);
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.use_am_pm = !self.use_am_pm;
                if self.use_am_pm {
                    watch::slcd::clear_indicator(Indicator::H24);
                    let date_time = rtc::get_date_time();
                    if date_time.hour < 12 {
                        watch::slcd::clear_indicator(Indicator::Pm);
                    } else {
                        watch::slcd::set_indicator(Indicator::Pm);
                    }
                } else {
                    watch::slcd::clear_indicator(Indicator::Pm);
                    watch::slcd::set_indicator(Indicator::H24);
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
