//! Time Left watch face.
//!
//! Port of the C `time_left_face.c`. Shows days left until a target date, days
//! from birth, and percentages. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd;
use crate::watch::utility;

const TIME_LEFT_FACE_STATES: u8 = 10;
const TIME_LEFT_FACE_SETTINGS_STATE: u8 = 4;

const STATE_TITLES: [&str; 10] = [
    "DL ", "DL ", "DA ", "DA ", "YRb", "MOb", "DAb", "YRd", "MOd", "DAd",
];

const PERCENTAGE_SEGDATA: [(u8, u8); 4] = [(1, 2), (2, 2), (2, 3), (1, 3)];
const ANIMATION_SEGDATA: [(u8, u8); 4] = [(2, 8), (1, 8), (2, 7), (2, 6)];

fn juliandaynum(year: u16, month: u16, day: u16) -> u32 {
    ((1461 * (year as i64 + 4800 + (month as i64 - 14) / 12)) / 4
        + (367 * (month as i64 - 2 - 12 * ((month as i64 - 14) / 12))) / 12
        - (3 * ((year as i64 + 4900 + (month as i64 - 14) / 12) / 100)) / 4
        + day as i64
        - 32075) as u32
}

/// A packed date (year, month, day).
#[derive(Clone, Copy)]
struct Date {
    year: u16,
    month: u8,
    day: u8,
}

/// The time left face state.
pub struct TimeLeftFace {
    current_page: u8,
    quick_ticks_running: bool,
    current_year: u16,
    birth_date: Date,
    target_date: Date,
}

impl TimeLeftFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        TimeLeftFace {
            current_page: 0,
            quick_ticks_running: false,
            current_year: 2020,
            birth_date: Date {
                year: 1959,
                month: 1,
                day: 1,
            },
            target_date: Date {
                year: 2030,
                month: 1,
                day: 1,
            },
        }
    }

    pub fn new() -> Self {
        TimeLeftFace::new_static()
    }

    fn display_integer(&self, buf: &[u8; 7]) {
        if buf[1] == b' ' {
            slcd::display_character(b' ', 8);
            slcd::display_character(b' ', 9);
            slcd::display_string(core::str::from_utf8(&buf[2..]).unwrap_or(""), 4);
        } else {
            slcd::display_string(core::str::from_utf8(buf).unwrap_or(""), 4);
        }
        slcd::clear_colon();
    }

    fn display_percentage(&self, mut percentage: f32, buf: &mut [u8; 7]) {
        if percentage < 0.0 {
            percentage *= -1.0;
            slcd::display_character(b'O', 1);
        }
        let integral = percentage as i32;
        if integral >= 100 {
            buf[0] = b' ';
            buf[1] = b'0' + (integral / 100) as u8;
            buf[2] = b'0' + ((integral / 10) % 10) as u8;
            buf[3] = b'0' + (integral % 10) as u8;
            buf[4] = b' ';
            buf[5] = b'o';
            slcd::clear_colon();
        } else {
            let fraction = ((percentage * 100.0) as i32) % 100;
            buf[0] = b'0' + (integral / 10) as u8;
            buf[1] = b'0' + (integral % 10) as u8;
            buf[2] = b'0' + (fraction / 10) as u8;
            buf[3] = b'0' + (fraction % 10) as u8;
            buf[4] = b' ';
            buf[5] = b'o';
            slcd::set_colon();
        }
        slcd::display_string(core::str::from_utf8(&buf[..6]).unwrap_or(""), 4);
        for &(c, s) in PERCENTAGE_SEGDATA.iter() {
            slcd::set_pixel(c, s);
        }
    }

    fn draw(&self, subsecond: u8) {
        let title = STATE_TITLES[self.current_page as usize].as_bytes();
        slcd::display_character(title[0], 0);
        slcd::display_character(title[1], 1);
        slcd::display_character(b' ', 2);
        slcd::display_character(title[2], 3);

        let mut buf = [0u8; 7];
        if self.current_page < TIME_LEFT_FACE_SETTINGS_STATE {
            let date_time = rtc::get_date_time();
            let julian_current = juliandaynum(
                date_time.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
                date_time.month as u16,
                date_time.day as u16,
            );
            let julian_target = juliandaynum(
                self.target_date.year,
                self.target_date.month as u16,
                self.target_date.day as u16,
            );
            let days_left = julian_target as i64 - julian_current as i64;
            if self.current_page == 0 {
                write_num(&mut buf, days_left, 6);
                self.display_integer(&buf);
            } else {
                let julian_start = juliandaynum(
                    self.birth_date.year,
                    self.birth_date.month as u16,
                    self.birth_date.day as u16,
                );
                if (self.current_page & 1) == 1 {
                    let percentage_left = if julian_start == julian_target {
                        0.0
                    } else {
                        days_left as f32 * 100.0
                            / (julian_target as i64 - julian_start as i64) as f32
                    };
                    self.display_percentage(
                        if self.current_page == 1 {
                            percentage_left
                        } else {
                            100.0 - percentage_left
                        },
                        &mut buf,
                    );
                } else {
                    write_num(&mut buf, julian_current as i64 - julian_start as i64, 6);
                    self.display_integer(&buf);
                }
            }
        } else {
            let mut val = [0u8; 5];
            match self.current_page {
                TIME_LEFT_FACE_SETTINGS_STATE => {
                    val[0] = b'0' + (self.birth_date.year / 1000) as u8;
                    val[1] = b'0' + ((self.birth_date.year / 100) % 10) as u8;
                    val[2] = b'0' + ((self.birth_date.year / 10) % 10) as u8;
                    val[3] = b'0' + (self.birth_date.year % 10) as u8;
                }
                5 => {
                    val[0] = b' ';
                    val[1] = b' ';
                    val[2] = b'0' + self.birth_date.month / 10;
                    val[3] = b'0' + self.birth_date.month % 10;
                }
                6 => {
                    val[0] = b' ';
                    val[1] = b' ';
                    val[2] = b'0' + self.birth_date.day / 10;
                    val[3] = b'0' + self.birth_date.day % 10;
                }
                7 => {
                    val[0] = b'0' + (self.target_date.year / 1000) as u8;
                    val[1] = b'0' + ((self.target_date.year / 100) % 10) as u8;
                    val[2] = b'0' + ((self.target_date.year / 10) % 10) as u8;
                    val[3] = b'0' + (self.target_date.year % 10) as u8;
                }
                8 => {
                    val[0] = b' ';
                    val[1] = b' ';
                    val[2] = b'0' + self.target_date.month / 10;
                    val[3] = b'0' + self.target_date.month % 10;
                }
                _ => {
                    val[0] = b' ';
                    val[1] = b' ';
                    val[2] = b'0' + self.target_date.day / 10;
                    val[3] = b'0' + self.target_date.day % 10;
                }
            }
            if subsecond & 1 == 1 {
                slcd::display_string("    ", 4);
            } else {
                slcd::display_string(core::str::from_utf8(&val[..4]).unwrap_or("    "), 4);
            }
        }
    }

    fn handle_alarm_button(&mut self) {
        match self.current_page {
            TIME_LEFT_FACE_SETTINGS_STATE => {
                self.birth_date.year += 1;
                if self.birth_date.year > self.current_year + 10 {
                    self.birth_date.year = 1959;
                }
            }
            5 => {
                self.birth_date.month = (self.birth_date.month % 12) + 1;
            }
            6 => self.birth_date.day += 1,
            7 => {
                self.target_date.year += 1;
                if self.target_date.year > 2083 {
                    self.target_date.year = self.current_year - 10;
                }
            }
            8 => {
                self.target_date.month = (self.target_date.month % 12) + 1;
            }
            9 => self.target_date.day += 1,
            _ => {}
        }
        if self.birth_date.day > utility::days_in_month(self.birth_date.month, self.birth_date.year)
        {
            self.birth_date.day = 1;
        }
        if self.target_date.day
            > utility::days_in_month(self.target_date.month, self.birth_date.year)
        {
            self.target_date.day = 1;
        }
    }

    fn initiate_setting(&mut self) {
        self.current_page = TIME_LEFT_FACE_SETTINGS_STATE;
        slcd::clear_colon();
    }

    fn resume_setting(&mut self) {
        self.current_page = 0;
    }

    fn abort_quick_ticks(&mut self) {
        self.quick_ticks_running = false;
    }
}

/// Writes a signed number right-aligned into a 6-digit buffer.
fn write_num(buf: &mut [u8; 7], value: i64, width: usize) {
    let mut v = value;
    let mut i = width - 1;
    loop {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if i == 0 || v == 0 {
            break;
        }
        i -= 1;
    }
}

impl WatchFace for TimeLeftFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        let date_time = rtc::get_date_time();
        self.current_year = date_time.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR;
        self.quick_ticks_running = false;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => self.draw(0),
            Event::Tick => {
                let mut subsecond = 0u8;
                if self.quick_ticks_running {
                    if watch::gpio::get_pin_level(watch::extint::BTN_ALARM) {
                        self.handle_alarm_button();
                        subsecond = 0;
                    } else {
                        self.abort_quick_ticks();
                    }
                }
                if self.current_page >= TIME_LEFT_FACE_SETTINGS_STATE {
                    self.draw(subsecond);
                } else {
                    let date_time = rtc::get_date_time();
                    if date_time.hour == 0 && date_time.minute == 0 && date_time.second == 0 {
                        self.draw(subsecond);
                    }
                    let animation_step = date_time.second % 4;
                    let (c, s) = ANIMATION_SEGDATA[animation_step as usize];
                    slcd::set_pixel(c, s);
                    let prev = if animation_step == 0 {
                        3
                    } else {
                        animation_step - 1
                    };
                    let (pc, ps) = ANIMATION_SEGDATA[prev as usize];
                    slcd::clear_pixel(pc, ps);
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.current_page < TIME_LEFT_FACE_SETTINGS_STATE {
                    movement::illuminate_led();
                } else {
                    self.current_page += 1;
                    if self.current_page >= TIME_LEFT_FACE_STATES {
                        self.resume_setting();
                        self.draw(0);
                    }
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.current_page >= TIME_LEFT_FACE_SETTINGS_STATE {
                    self.resume_setting();
                } else {
                    self.initiate_setting();
                }
                self.draw(0);
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.abort_quick_ticks();
                if self.current_page < TIME_LEFT_FACE_SETTINGS_STATE {
                    self.current_page = (self.current_page + 1) % TIME_LEFT_FACE_SETTINGS_STATE;
                } else {
                    self.handle_alarm_button();
                }
                self.draw(0);
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.current_page >= TIME_LEFT_FACE_SETTINGS_STATE {
                    self.quick_ticks_running = true;
                    self.handle_alarm_button();
                    self.draw(0);
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        if self.current_page >= TIME_LEFT_FACE_SETTINGS_STATE {
            self.resume_setting();
        }
    }
}
