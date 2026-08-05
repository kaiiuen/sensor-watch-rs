//! Days Since watch face.
//!
//! Port of the C `days_since_face.c`. Displays the number of days since (or
//! until) a configurable date. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd;
use crate::watch::storage;
use crate::watch::utility;

const PAGE_DISPLAY: u8 = 0;
const PAGE_YEAR: u8 = 1;
const PAGE_MONTH: u8 = 2;
const PAGE_DAY: u8 = 3;
const PAGE_DATE: u8 = 4;

/// The storage row/offset used to persist the configured date.
const STORAGE_ROW: u32 = 8;
const STORAGE_OFFSET: u32 = 0;

/// Computes the Julian day number for a given date (from Wikipedia).
fn juliandaynum(year: u16, month: u16, day: u16) -> u32 {
    ((1461 * (year as i64 + 4800 + (month as i64 - 14) / 12)) / 4
        + (367 * (month as i64 - 2 - 12 * ((month as i64 - 14) / 12))) / 12
        - (3 * ((year as i64 + 4900 + (month as i64 - 14) / 12) / 100)) / 4
        + day as i64
        - 32075) as u32
}

/// Packs a date into the same bit layout as the C `days_since_date_t`.
fn pack_date(year: u16, month: u8, day: u8) -> u32 {
    (year as u32 & 0xFFF) | ((month as u32 & 0xF) << 12) | ((day as u32 & 0x1F) << 16)
}

/// Writes a number right-aligned into a buffer (padded with spaces).
fn write_num(buf: &mut [u8], value: u32) {
    let mut v = value;
    let mut i = buf.len() - 1;
    loop {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if i == 0 || v == 0 {
            break;
        }
        i -= 1;
    }
}

/// The days since face state.
pub struct DaysSinceFace {
    current_page: u8,
    face_index: u8,
    working_year: u16,
    working_month: u8,
    working_day: u8,
    birthday_changed: bool,
    quick_cycle: bool,
    ticks: u8,
}

impl DaysSinceFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        DaysSinceFace {
            current_page: PAGE_DISPLAY,
            face_index: 0,
            working_year: 1959,
            working_month: 1,
            working_day: 1,
            birthday_changed: false,
            quick_cycle: false,
            ticks: 0,
        }
    }

    pub fn new() -> Self {
        DaysSinceFace::new_static()
    }

    fn persist_date(&self) {
        let reg = pack_date(self.working_year, self.working_month, self.working_day);
        let buf = reg.to_le_bytes();
        let mut old = [0u8; 4];
        if storage::read(STORAGE_ROW, STORAGE_OFFSET, &mut old) && old == buf {
            return;
        }
        storage::erase(STORAGE_ROW);
        storage::write(STORAGE_ROW, STORAGE_OFFSET, &buf);
    }

    fn update(&self) {
        let mut buf = [b' '; 6];
        let date_time = movement::get_local_date_time();
        let julian_now = juliandaynum(
            date_time.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            date_time.month as u16,
            date_time.day as u16,
        );
        let julian_since = juliandaynum(
            self.working_year,
            self.working_month as u16,
            self.working_day as u16,
        );
        slcd::display_string("DA", 0);
        slcd::display_string("  ", 2);
        let diff = julian_now.abs_diff(julian_since);
        write_num(&mut buf, diff);
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
    }

    fn abort_quick_cycle(&mut self) {
        if self.quick_cycle {
            self.quick_cycle = false;
            movement::request_tick_frequency(4);
        }
    }

    fn increment(&mut self) {
        self.birthday_changed = true;
        match self.current_page {
            PAGE_YEAR => {
                self.working_year += 1;
                if self.working_year > 2080 {
                    self.working_year = 1900;
                }
            }
            PAGE_MONTH => self.working_month = (self.working_month % 12) + 1,
            PAGE_DAY => {
                self.working_day = (self.working_day
                    % utility::days_in_month(self.working_month, self.working_year))
                    + 1
            }
            _ => {}
        }
    }
}

impl WatchFace for DaysSinceFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {
        // Load the date from storage if it exists; otherwise use a sensible default.
        let mut buf = [0u8; 4];
        let mut reg = 0xFFFF_FFFF;
        if storage::read(STORAGE_ROW, STORAGE_OFFSET, &mut buf) {
            reg = u32::from_le_bytes(buf);
        }
        if reg == 0xFFFF_FFFF {
            self.working_year = 1959;
            self.working_month = 1;
            self.working_day = 1;
        } else {
            self.working_year = (reg & 0xFFF) as u16;
            self.working_month = ((reg >> 12) & 0xF) as u8;
            self.working_day = ((reg >> 16) & 0x1F) as u8;
        }
    }

    fn activate(&mut self, _settings: &Settings) {
        self.current_page = PAGE_DISPLAY;
        self.quick_cycle = false;
        self.ticks = 0;
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
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
                        slcd::display_string("YR", 0);
                        let mut buf = [b' '; 6];
                        buf[0] = b'0' + (self.working_year / 1000) as u8;
                        buf[1] = b'0' + ((self.working_year / 100) % 10) as u8;
                        buf[2] = b'0' + ((self.working_year / 10) % 10) as u8;
                        buf[3] = b'0' + (self.working_year % 10) as u8;
                        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
                    }
                    PAGE_MONTH => {
                        slcd::display_string("MO", 0);
                        let mut buf = [b' '; 6];
                        buf[0] = b'0' + self.working_month / 10;
                        buf[1] = b'0' + self.working_month % 10;
                        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
                    }
                    PAGE_DAY => {
                        slcd::display_string("DA", 0);
                        let mut buf = [b' '; 6];
                        buf[2] = b'0' + self.working_day / 10;
                        buf[3] = b'0' + self.working_day % 10;
                        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
                    }
                    PAGE_DISPLAY => {
                        let date_time = movement::get_local_date_time();
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
                        movement::request_tick_frequency(1);
                        self.persist_date();
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
                    slcd::display_string("DA", 0);
                    self.current_page = PAGE_DATE;
                    let mut buf = [0u8; 6];
                    let yy = self.working_year % 100;
                    buf[0] = b'0' + (yy / 10) as u8;
                    buf[1] = b'0' + (yy % 10) as u8;
                    buf[2] = b'0' + self.working_month / 10;
                    buf[3] = b'0' + self.working_month % 10;
                    buf[4] = b'0' + self.working_day / 10;
                    buf[5] = b'0' + self.working_day % 10;
                    slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
                    self.ticks = 2;
                }
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => match self.current_page {
                PAGE_DISPLAY => {
                    self.current_page += 1;
                    movement::request_tick_frequency(4);
                }
                PAGE_YEAR | PAGE_MONTH | PAGE_DAY => {
                    self.quick_cycle = true;
                    movement::request_tick_frequency(8);
                }
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::LongUp) => self.abort_quick_cycle(),
            Event::BackgroundTask => {
                self.abort_quick_cycle();
                if self.current_page != PAGE_DISPLAY {
                    movement::move_to_face(0);
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        if self.birthday_changed {
            self.persist_date();
            self.birthday_changed = false;
        }
    }
}
