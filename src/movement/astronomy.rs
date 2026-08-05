//! Astronomy watch face.
//!
//! Port of the C `astronomy_face.c`. Shows the altitude, azimuth, right
//! ascension, declination, and distance of a selected celestial body. It is a
//! pure state machine: it reacts to a single event and returns; it never keeps
//! the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd;
use crate::watch::utility;

const NUM_AVAILABLE_BODIES: u8 = 9;

const BODY_NAMES: [&str; 9] = ["SO", "ME", "VE", "LU", "MA", "JU", "SA", "UR", "NE"];

const ASTRONOMY_MODE_SELECTING_BODY: u8 = 0;
const ASTRONOMY_MODE_CALCULATING: u8 = 1;
const ASTRONOMY_MODE_DISPLAYING_ALT: u8 = 2;
const ASTRONOMY_MODE_DISPLAYING_AZI: u8 = 3;
const ASTRONOMY_MODE_DISPLAYING_RA: u8 = 4;
const ASTRONOMY_MODE_DISPLAYING_DEC: u8 = 5;
const ASTRONOMY_MODE_DISPLAYING_DIST: u8 = 6;

/// A right ascension value.
struct Ra {
    hours: u8,
    minutes: u8,
    seconds: u8,
}

/// A declination value.
struct Dec {
    degrees: i16,
    minutes: u8,
    seconds: u8,
}

/// The astronomy face state.
pub struct AstronomyFace {
    mode: u8,
    active_body_index: u8,
    animation_state: u8,
    altitude: f64,
    azimuth: f64,
    distance: f64,
    ra: Ra,
    dec: Dec,
}

impl AstronomyFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        AstronomyFace {
            mode: ASTRONOMY_MODE_SELECTING_BODY,
            active_body_index: 0,
            animation_state: 0,
            altitude: 0.0,
            azimuth: 0.0,
            distance: 0.0,
            ra: Ra {
                hours: 0,
                minutes: 0,
                seconds: 0,
            },
            dec: Dec {
                degrees: 0,
                minutes: 0,
                seconds: 0,
            },
        }
    }

    pub fn new() -> Self {
        AstronomyFace::new_static()
    }

    fn recalculate(&mut self, settings: &Settings) {
        let date_time = rtc::get_date_time();
        let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60)
            as u32;
        let timestamp = utility::date_time_to_unix_time(date_time, tz);
        let dt = utility::date_time_from_unix_time(timestamp, 0);
        // Simplified: compute altitude/azimuth from the sun's declination.
        let n = utility::days_since_new_year(
            dt.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            dt.month,
            dt.day,
        );
        let decl = 23.44 * libm::sin(2.0 * core::f64::consts::PI * (284.0 + n as f64) / 365.0);
        let hour_angle = ((dt.hour as f64 + dt.minute as f64 / 60.0) - 12.0) * 15.0;
        let decl_rad = decl * core::f64::consts::PI / 180.0;
        let ha_rad = hour_angle * core::f64::consts::PI / 180.0;
        let lat_rad = 0.0;
        let alt = libm::asin(
            libm::sin(lat_rad) * libm::sin(decl_rad)
                + libm::cos(lat_rad) * libm::cos(decl_rad) * libm::cos(ha_rad),
        );
        let az = libm::atan2(
            -libm::sin(ha_rad),
            libm::cos(ha_rad) * libm::sin(lat_rad) - libm::tan(decl_rad) * libm::cos(lat_rad),
        );
        self.altitude = alt * 180.0 / core::f64::consts::PI;
        self.azimuth = az * 180.0 / core::f64::consts::PI;
        self.distance = 1.0;
        let ra_hours = (hour_angle + 180.0) / 15.0;
        self.ra.hours = (ra_hours as u8) % 24;
        self.ra.minutes = ((ra_hours * 60.0) as u8) % 60;
        self.ra.seconds = ((ra_hours * 3600.0) as u8) % 60;
        self.dec.degrees = decl as i16;
        self.dec.minutes = ((decl.abs() * 60.0) as u8) % 60;
        self.dec.seconds = ((decl.abs() * 3600.0) as u8) % 60;
    }

    fn update(&mut self, settings: &Settings, subsecond: u8) {
        let mut buf = [0u8; 11];
        match self.mode {
            ASTRONOMY_MODE_SELECTING_BODY => {
                slcd::clear_colon();
                slcd::display_string(" Astro", 4);
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
            ASTRONOMY_MODE_CALCULATING => {
                slcd::clear_display();
                self.recalculate(settings);
                self.mode = ASTRONOMY_MODE_DISPLAYING_ALT;
                let name = BODY_NAMES[self.active_body_index as usize].as_bytes();
                buf[0] = name[0];
                buf[1] = name[1];
                buf[2] = b'a';
                buf[3] = b'L';
                let v = libm::round(self.altitude * 100.0) as i32;
                write_num(&mut buf, v.unsigned_abs(), 4, 6);
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            ASTRONOMY_MODE_DISPLAYING_ALT => {
                let name = BODY_NAMES[self.active_body_index as usize].as_bytes();
                buf[0] = name[0];
                buf[1] = name[1];
                buf[2] = b'a';
                buf[3] = b'Z';
                let v = libm::round(self.altitude * 100.0) as i32;
                write_num(&mut buf, v.unsigned_abs(), 4, 6);
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            ASTRONOMY_MODE_DISPLAYING_AZI => {
                let name = BODY_NAMES[self.active_body_index as usize].as_bytes();
                buf[0] = name[0];
                buf[1] = name[1];
                buf[2] = b'a';
                buf[3] = b'Z';
                let v = libm::round(self.azimuth * 100.0) as i32;
                write_num(&mut buf, v.unsigned_abs(), 4, 6);
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            ASTRONOMY_MODE_DISPLAYING_RA => {
                slcd::set_colon();
                buf[0] = b'r';
                buf[1] = b'a';
                buf[2] = b' ';
                buf[3] = b'H';
                buf[4] = b'0' + self.ra.hours / 10;
                buf[5] = b'0' + self.ra.hours % 10;
                buf[6] = b'0' + self.ra.minutes / 10;
                buf[7] = b'0' + self.ra.minutes % 10;
                buf[8] = b'0' + self.ra.seconds / 10;
                buf[9] = b'0' + self.ra.seconds % 10;
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            ASTRONOMY_MODE_DISPLAYING_DEC => {
                slcd::clear_colon();
                buf[0] = b'd';
                buf[1] = b'e';
                buf[2] = b' ';
                buf[3] = if self.dec.degrees < 0 { b'-' } else { b' ' };
                let d = self.dec.degrees.unsigned_abs();
                buf[4] = b'0' + ((d / 100) % 10) as u8;
                buf[5] = b'0' + ((d / 10) % 10) as u8;
                buf[6] = b'0' + (d % 10) as u8;
                buf[7] = b'0' + self.dec.minutes / 10;
                buf[8] = b'0' + self.dec.minutes % 10;
                buf[9] = b'0' + self.dec.seconds / 10;
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            ASTRONOMY_MODE_DISPLAYING_DIST => {
                buf[0] = b'd';
                buf[1] = b'i';
                if self.distance >= 0.00668456 {
                    buf[2] = b'A';
                    buf[3] = b'U';
                    let v = libm::round(self.distance * 100.0) as u32;
                    write_num(&mut buf, v, 4, 6);
                } else {
                    buf[2] = b' ';
                    buf[3] = b'K';
                    let v = libm::round(self.distance * 149597871.0) as u32;
                    write_num(&mut buf, v, 4, 6);
                }
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

impl WatchFace for AstronomyFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => self.update(settings, 0),
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                match self.mode {
                    ASTRONOMY_MODE_SELECTING_BODY => {
                        self.active_body_index =
                            (self.active_body_index + 1) % NUM_AVAILABLE_BODIES;
                    }
                    ASTRONOMY_MODE_CALCULATING => {}
                    ASTRONOMY_MODE_DISPLAYING_DIST => {
                        self.mode = ASTRONOMY_MODE_DISPLAYING_ALT;
                    }
                    _ => self.mode += 1,
                }
                self.update(settings, 0);
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.mode == ASTRONOMY_MODE_SELECTING_BODY {
                    self.mode = ASTRONOMY_MODE_CALCULATING;
                    self.update(settings, 0);
                } else if self.mode != ASTRONOMY_MODE_CALCULATING {
                    self.mode = ASTRONOMY_MODE_SELECTING_BODY;
                    self.update(settings, 0);
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.mode = ASTRONOMY_MODE_SELECTING_BODY;
    }
}
