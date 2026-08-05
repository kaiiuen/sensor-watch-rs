//! Tachymeter watch face.
//!
//! Port of the C `tachymeter_face.c`. Measures speed over a fixed distance
//! using a stopwatch and a configurable distance value. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::rtc;
use crate::watch::slcd;
use crate::watch::utility;

/// The distance, stored digit-wise.
#[derive(Clone, Copy)]
struct DistanceDigits {
    thousands: u8,
    hundreds: u8,
    tens: u8,
    ones: u8,
    tenths: u8,
    hundredths: u8,
}

impl DistanceDigits {
    fn value(&self) -> u32 {
        self.thousands as u32 * 100000
            + self.hundreds as u32 * 10000
            + self.tens as u32 * 1000
            + self.ones as u32 * 100
            + self.tenths as u32 * 10
            + self.hundredths as u32
    }
}

/// The tachymeter face state.
pub struct TachymeterFace {
    running: bool,
    editing: bool,
    active_digit: u8,
    animation_state: u8,
    distance: u32,
    dist_digits: DistanceDigits,
    start_seconds: rtc::DateTime,
    start_subsecond: u8,
    total_time: u32,
    total_speed: u32,
}

impl TachymeterFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        TachymeterFace {
            running: false,
            editing: false,
            active_digit: 0,
            animation_state: 0,
            distance: 100,
            dist_digits: DistanceDigits {
                thousands: 0,
                hundreds: 0,
                tens: 0,
                ones: 1,
                tenths: 0,
                hundredths: 0,
            },
            start_seconds: rtc::DateTime {
                second: 0,
                minute: 0,
                hour: 0,
                day: 0,
                month: 0,
                year: 0,
            },
            start_subsecond: 0,
            total_time: 0,
            total_speed: 0,
        }
    }

    pub fn new() -> Self {
        TachymeterFace::new_static()
    }

    fn distance_lcd(&mut self, subsecond: u8) {
        let mut buf = [0u8; 11];
        self.distance = self.dist_digits.value();
        buf[0] = b'T';
        buf[1] = b'C';
        buf[2] = b' ';
        buf[3] = if self.running { b' ' } else { b'd' };
        write_num(&mut buf, self.distance, 4, 6);
        if self.editing {
            if subsecond < 2 {
                buf[3] = b' ';
            }
            if subsecond % 2 == 1 {
                buf[self.active_digit as usize + 4] = b' ';
            }
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn totals_lcd(&self, show_time: bool) {
        let mut buf = [0u8; 11];
        buf[0] = b'T';
        buf[1] = b'C';
        buf[2] = b' ';
        buf[3] = if show_time { b't' } else { b'h' };
        if show_time {
            write_num(&mut buf, self.total_time, 4, 6);
        } else {
            write_num(&mut buf, self.total_speed, 4, 6);
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        if !show_time {
            slcd::set_pixel(0, 9);
            slcd::set_pixel(0, 10);
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

impl WatchFace for TachymeterFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        if self.total_time == 0 {
            self.distance_lcd(0);
        }
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => {
                if self.total_time == 0 {
                    self.distance_lcd(0);
                }
            }
            Event::Tick => {
                if self.editing {
                    self.distance_lcd(0);
                }
                if !self.running && self.total_time != 0 {
                    if 0 < 2 {
                        self.totals_lcd(true);
                    } else {
                        self.totals_lcd(false);
                    }
                } else if self.running {
                    slcd::display_string("  ", 2);
                    match self.animation_state {
                        0 => slcd::set_pixel(0, 7),
                        1 => slcd::set_pixel(1, 7),
                        2 => slcd::set_pixel(2, 7),
                        3 => slcd::set_pixel(2, 6),
                        4 => slcd::set_pixel(2, 8),
                        _ => slcd::set_pixel(0, 8),
                    }
                    self.animation_state = (self.animation_state + 1) % 6;
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.editing {
                    self.active_digit = (self.active_digit + 1) % 6;
                } else {
                    movement::illuminate_led();
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if !self.running && !self.editing {
                    if self.total_time != 0 {
                        self.total_time = 0;
                        self.total_speed = 0;
                    } else {
                        self.dist_digits = DistanceDigits {
                            thousands: 0,
                            hundreds: 0,
                            tens: 0,
                            ones: 1,
                            tenths: 0,
                            hundredths: 0,
                        };
                        self.distance = self.dist_digits.value();
                    }
                    self.distance_lcd(0);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if !self.running && self.total_time == 0 {
                    if settings.button_should_sound() && !self.editing {
                        crate::movement::play_alarm_beeps(1, crate::watch::buzzer::Note::C8);
                    }
                    if !self.editing {
                        self.running = true;
                        self.start_seconds = rtc::get_date_time();
                        self.start_subsecond = 0;
                        self.total_time = 0;
                    } else {
                        match self.active_digit {
                            0 => self.dist_digits.thousands = (self.dist_digits.thousands + 1) % 10,
                            1 => self.dist_digits.hundreds = (self.dist_digits.hundreds + 1) % 10,
                            2 => self.dist_digits.tens = (self.dist_digits.tens + 1) % 10,
                            3 => self.dist_digits.ones = (self.dist_digits.ones + 1) % 10,
                            4 => self.dist_digits.tenths = (self.dist_digits.tenths + 1) % 10,
                            _ => {
                                self.dist_digits.hundredths = (self.dist_digits.hundredths + 1) % 10
                            }
                        }
                    }
                } else if self.running {
                    if settings.button_should_sound() && !self.editing {
                        crate::movement::play_alarm_beeps(1, crate::watch::buzzer::Note::C8);
                    }
                    self.running = false;
                    let now = rtc::get_date_time();
                    let now_ts = utility::date_time_to_unix_time(now, 0);
                    let start_ts = utility::date_time_to_unix_time(self.start_seconds, 0);
                    self.total_time = (now_ts * 100) - (start_ts * 100);
                    self.total_speed = (3600 * 100 * self.distance) / self.total_time;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if !self.running && self.total_time == 0 {
                    if !self.editing {
                        self.editing = true;
                        self.active_digit = 0;
                    } else {
                        self.editing = false;
                        if self.dist_digits.value() == 0 {
                            self.dist_digits.ones = 1;
                        }
                        self.distance_lcd(0);
                    }
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
