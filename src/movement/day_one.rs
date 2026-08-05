//! Day One watch face.
//!
//! Port of the C `day_one_face.c`. Counts the number of days since (or until)
//! a configurable birth date. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::utility;

const PAGE_DISPLAY: u8 = 0;
const PAGE_DATE: u8 = 1;
const PAGE_YEAR: u8 = 2;
const PAGE_MONTH: u8 = 3;
const PAGE_DAY: u8 = 4;

fn juliandaynum(year: u16, month: u16, day: u16) -> u32 {
    ((1461 * (year as i64 + 4800 + (month as i64 - 14) / 12)) / 4
        + (367 * (month as i64 - 2 - 12 * ((month as i64 - 14) / 12))) / 12
        - (3 * ((year as i64 + 4900 + (month as i64 - 14) / 12) / 100)) / 4
        + day as i64
        - 32075) as u32
}

/// The day one face state.
pub struct DayOneFace {
    current_page: u8,
    quick_cycle: bool,
    ticks: u8,
    birthday_changed: bool,
    birth_year: u16,
    birth_month: u8,
    birth_day: u8,
}

impl DayOneFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        DayOneFace {
            current_page: PAGE_DISPLAY,
            quick_cycle: false,
            ticks: 0,
            birthday_changed: false,
            birth_year: 1959,
            birth_month: 1,
            birth_day: 1,
        }
    }

    pub fn new() -> Self {
        DayOneFace::new_static()
    }

    fn update(&self) {
        let mut buf = [0u8; 11];
        let date_time = rtc::get_date_time();
        let julian_date = juliandaynum(
            date_time.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            date_time.month as u16,
            date_time.day as u16,
        );
        let julian_birthdate = juliandaynum(
            self.birth_year,
            self.birth_month as u16,
            self.birth_day as u16,
        );
        let diff = if julian_date < julian_birthdate {
            julian_birthdate - julian_date
        } else {
            julian_date - julian_birthdate
        };
        buf[0] = b'D';
        buf[1] = b'A';
        buf[2] = b' ';
        buf[3] = b' ';
        write_num(&mut buf, diff, 4, 6);
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn abort_quick_cycle(&mut self) {
        self.quick_cycle = false;
    }

    fn increment(&mut self) {
        self.birthday_changed = true;
        match self.current_page {
            PAGE_YEAR => {
                self.birth_year += 1;
                if self.birth_year > 2080 {
                    self.birth_year = 1900;
                }
            }
            PAGE_MONTH => self.birth_month = (self.birth_month % 12) + 1,
            PAGE_DAY => self.birth_day += 1,
            _ => {}
        }
        if self.birth_day == 0
            || self.birth_day > utility::days_in_month(self.birth_month, self.birth_year)
        {
            self.birth_day = 1;
        }
    }
}

/// Writes a number right-aligned into the buffer at the given offset.
fn write_num(buf: &mut [u8; 11], value: u32, offset: usize, width: usize) {
    let mut v = value;
    let mut i = offset + width - 1;
    loop {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if i == offset || v == 0 {
            break;
        }
        i -= 1;
    }
}

impl WatchFace for DayOneFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.current_page = PAGE_DISPLAY;
        self.quick_cycle = false;
        self.ticks = 0;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => self.update(),
            Event::Tick => {
                if self.quick_cycle {
                    if watch::gpio::get_pin_level(watch::extint::BTN_ALARM) {
                        self.increment();
                    } else {
                        self.abort_quick_cycle();
                    }
                }
                match self.current_page {
                    PAGE_YEAR => {
                        watch::slcd::display_string("YR        ", 0);
                        let mut buf = [0u8; 5];
                        buf[0] = b'0' + (self.birth_year / 1000) as u8;
                        buf[1] = b'0' + ((self.birth_year / 100) % 10) as u8;
                        buf[2] = b'0' + ((self.birth_year / 10) % 10) as u8;
                        buf[3] = b'0' + (self.birth_year % 10) as u8;
                        watch::slcd::display_string(
                            core::str::from_utf8(&buf[..4]).unwrap_or(""),
                            4,
                        );
                    }
                    PAGE_MONTH => {
                        watch::slcd::display_string("MO        ", 0);
                        let mut buf = [0u8; 3];
                        buf[0] = b'0' + self.birth_month / 10;
                        buf[1] = b'0' + self.birth_month % 10;
                        watch::slcd::display_string(
                            core::str::from_utf8(&buf[..2]).unwrap_or(""),
                            4,
                        );
                    }
                    PAGE_DAY => {
                        watch::slcd::display_string("DA        ", 0);
                        let mut buf = [0u8; 3];
                        buf[0] = b'0' + self.birth_day / 10;
                        buf[1] = b'0' + self.birth_day % 10;
                        watch::slcd::display_string(
                            core::str::from_utf8(&buf[..2]).unwrap_or(""),
                            6,
                        );
                    }
                    PAGE_DISPLAY => {
                        let date_time = rtc::get_date_time();
                        if date_time.hour == 0 && date_time.minute == 0 && date_time.second == 0 {
                            self.update();
                        }
                    }
                    PAGE_DATE => {
                        if self.ticks > 0 {
                            self.ticks -= 1;
                        } else {
                            self.current_page = PAGE_DISPLAY;
                            self.update();
                        }
                    }
                    _ => {}
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                if self.current_page == PAGE_DISPLAY || self.current_page == PAGE_DATE {
                    movement::illuminate_led();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => match self.current_page {
                PAGE_YEAR | PAGE_MONTH | PAGE_DAY => {
                    self.current_page = (self.current_page + 1) % 4;
                    if self.current_page == PAGE_DISPLAY {
                        self.update();
                    }
                }
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::Up) => match self.current_page {
                PAGE_YEAR | PAGE_MONTH | PAGE_DAY => {
                    self.abort_quick_cycle();
                    self.increment();
                }
                PAGE_DISPLAY => {
                    self.current_page = PAGE_DATE;
                    let mut buf = [0u8; 9];
                    buf[0] = b'0' + (self.birth_year / 1000) as u8;
                    buf[1] = b'0' + ((self.birth_year / 100) % 10) as u8;
                    buf[2] = b'0' + ((self.birth_year / 10) % 10) as u8;
                    buf[3] = b'0' + (self.birth_year % 10) as u8;
                    buf[4] = b'0' + self.birth_month / 10;
                    buf[5] = b'0' + self.birth_month % 10;
                    buf[6] = b'0' + self.birth_day / 10;
                    buf[7] = b'0' + self.birth_day % 10;
                    watch::slcd::display_string(core::str::from_utf8(&buf[..8]).unwrap_or(""), 2);
                    self.ticks = 2;
                }
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => match self.current_page {
                PAGE_DISPLAY => {
                    self.current_page += 1;
                }
                PAGE_YEAR | PAGE_MONTH | PAGE_DAY => {
                    self.quick_cycle = true;
                }
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::LongUp) => self.abort_quick_cycle(),
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        if self.birthday_changed {
            self.birthday_changed = false;
        }
    }
}
