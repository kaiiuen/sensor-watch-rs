//! Orrery watch face.
//!
//! Port of the C `orrery_face.c`. Shows the heliocentric coordinates (X, Y, Z)
//! of a selected planet. It is a pure state machine: it reacts to a single
//! event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::rtc;
use crate::watch::slcd;
use crate::watch::utility;

const NUM_AVAILABLE_BODIES: u8 = 9;

const BODY_NAMES: [&str; 9] = ["ME", "VE", "EA", "LU", "MA", "JU", "SA", "UR", "NE"];

const ORRERY_MODE_SELECTING_BODY: u8 = 0;
const ORRERY_MODE_CALCULATING: u8 = 1;
const ORRERY_MODE_DISPLAYING_X: u8 = 2;
const ORRERY_MODE_DISPLAYING_Y: u8 = 3;
const ORRERY_MODE_DISPLAYING_Z: u8 = 4;

/// Approximate orbital radii (AU) for each body.
const ORBITAL_RADII: [f64; 9] = [0.39, 0.72, 1.0, 0.00257, 1.52, 5.2, 9.58, 19.2, 30.05];

/// The orrery face state.
pub struct OrreryFace {
    mode: u8,
    active_body_index: u8,
    animation_state: u8,
    coords: [f64; 3],
}

impl OrreryFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        OrreryFace {
            mode: ORRERY_MODE_SELECTING_BODY,
            active_body_index: 0,
            animation_state: 0,
            coords: [0.0; 3],
        }
    }

    pub fn new() -> Self {
        OrreryFace::new_static()
    }

    fn recalculate(&mut self, settings: &Settings) {
        let date_time = rtc::get_date_time();
        let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60)
            as u32;
        let timestamp = utility::date_time_to_unix_time(date_time, tz);
        let dt = utility::date_time_from_unix_time(timestamp, 0);
        let n = utility::days_since_new_year(
            dt.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            dt.month,
            dt.day,
        );
        let radius = ORBITAL_RADII[self.active_body_index as usize];
        let angle = 2.0 * core::f64::consts::PI * n as f64 / 365.25;
        self.coords[0] = radius * libm::cos(angle);
        self.coords[1] = radius * libm::sin(angle);
        self.coords[2] = 0.0;
    }

    fn update(&mut self, settings: &Settings, subsecond: u8) {
        let mut buf = [0u8; 11];
        match self.mode {
            ORRERY_MODE_SELECTING_BODY => {
                slcd::display_string("Orrery", 4);
                if subsecond % 2 == 1 {
                    slcd::display_string(BODY_NAMES[self.active_body_index as usize], 0);
                } else {
                    slcd::display_string("  ", 0);
                }
                if subsecond == 0 {
                    slcd::display_string("  ", 2);
                    match self.animation_state {
                        0 => {
                            slcd::set_pixel(0, 7);
                            slcd::set_pixel(2, 6);
                        }
                        1 => {
                            slcd::set_pixel(1, 7);
                            slcd::set_pixel(2, 9);
                        }
                        _ => {
                            slcd::set_pixel(2, 7);
                            slcd::set_pixel(0, 9);
                        }
                    }
                    self.animation_state = (self.animation_state + 1) % 3;
                }
            }
            ORRERY_MODE_CALCULATING => {
                slcd::clear_display();
                self.recalculate(settings);
                self.mode = ORRERY_MODE_DISPLAYING_X;
                let name = BODY_NAMES[self.active_body_index as usize].as_bytes();
                buf[0] = name[0];
                buf[1] = name[1];
                buf[2] = b' ';
                buf[3] = b'X';
                let v = libm::round(self.coords[0] * 100.0) as i32;
                write_num(&mut buf, v.unsigned_abs(), 4, 6);
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            ORRERY_MODE_DISPLAYING_X => {
                let name = BODY_NAMES[self.active_body_index as usize].as_bytes();
                buf[0] = name[0];
                buf[1] = name[1];
                buf[2] = b' ';
                buf[3] = b'X';
                let v = libm::round(self.coords[0] * 100.0) as i32;
                write_num(&mut buf, v.unsigned_abs(), 4, 6);
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            ORRERY_MODE_DISPLAYING_Y => {
                let name = BODY_NAMES[self.active_body_index as usize].as_bytes();
                buf[0] = name[0];
                buf[1] = name[1];
                buf[2] = b' ';
                buf[3] = b'Y';
                let v = libm::round(self.coords[1] * 100.0) as i32;
                write_num(&mut buf, v.unsigned_abs(), 4, 6);
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            ORRERY_MODE_DISPLAYING_Z => {
                let name = BODY_NAMES[self.active_body_index as usize].as_bytes();
                buf[0] = name[0];
                buf[1] = name[1];
                buf[2] = b' ';
                buf[3] = b'Z';
                let v = libm::round(self.coords[2] * 100.0) as i32;
                write_num(&mut buf, v.unsigned_abs(), 4, 6);
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            _ => {}
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

impl WatchFace for OrreryFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => self.update(settings, 0),
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                match self.mode {
                    ORRERY_MODE_SELECTING_BODY => {
                        self.active_body_index =
                            (self.active_body_index + 1) % NUM_AVAILABLE_BODIES;
                    }
                    ORRERY_MODE_CALCULATING => {}
                    ORRERY_MODE_DISPLAYING_Z => {
                        self.mode = ORRERY_MODE_DISPLAYING_X;
                    }
                    _ => self.mode += 1,
                }
                self.update(settings, 0);
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.mode == ORRERY_MODE_SELECTING_BODY {
                    self.mode = ORRERY_MODE_CALCULATING;
                    self.update(settings, 0);
                } else if self.mode != ORRERY_MODE_CALCULATING {
                    self.mode = ORRERY_MODE_SELECTING_BODY;
                    self.update(settings, 0);
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.mode = ORRERY_MODE_SELECTING_BODY;
    }
}
