//! Mars time watch face.
//!
//! Port of the C `mars_time_face.c`. Shows the current time at a Mars landing
//! site (or Mars Coordinated Time), or the mission sol. It is a pure state
//! machine: it renders on wake and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const MARS_TIME_NUM_SITES: usize = 5;

/// Lander longitudes (from Mars24's marslandmarks.xml).
const SITE_LONGITUDES: [f64; MARS_TIME_NUM_SITES] = [
    0.0,
    360.0 - 109.9,
    360.0 - 77.450_885_72,
    360.0 - 135.623_447,
    360.0 - 137.441_635,
];

const SITE_NAMES: [&str; MARS_TIME_NUM_SITES] = ["MC", "ZH", "PE", "IN", "CU"];

const LANDING_SOLS: [u16; MARS_TIME_NUM_SITES] = [0, 52387, 52304, 51511, 49269];

/// A Mars clock time.
struct MarsClockHms {
    hour: u8,
    minute: u8,
    second: u8,
}

fn h_to_hms(h: f64) -> MarsClockHms {
    let seconds = (h * 3600.0) as u32;
    let hour = seconds / 3600;
    let seconds = seconds % 3600;
    let minute = (seconds / 60) as u8;
    let second = (seconds % 60) as u8;
    MarsClockHms {
        hour: hour as u8,
        minute,
        second,
    }
}

/// The Mars time face state.
pub struct MarsTimeFace {
    current_site: usize,
    displaying_sol: bool,
}

impl MarsTimeFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        MarsTimeFace {
            current_site: 0,
            displaying_sol: false,
        }
    }

    pub fn new() -> Self {
        MarsTimeFace::new_static()
    }

    fn update(&self, settings: &Settings) {
        let mut buf = [0u8; 11];
        let date_time = rtc::get_date_time();
        let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60)
            as u32;
        let now = utility::date_time_to_unix_time(date_time, tz);
        let jdut = 2_440_587.5 + (now as f64 / 86400.0);
        let jdtt = jdut + ((37.0 + 32.184) / 86400.0);
        let jd2k = jdtt - 2_451_545.0;
        let msd = ((jd2k - 4.5) / 1.027_491_251_7) + 44796.0 - 0.000_962_6;
        let mtc = libm::fmod(24.0 * msd, 24.0);
        let lmt = if self.current_site == 0 {
            mtc
        } else {
            let longitude = SITE_LONGITUDES[self.current_site];
            let lmst = mtc - ((longitude * 24.0) / 360.0);
            libm::fmod(lmst + 24.0, 24.0)
        };

        let name = SITE_NAMES[self.current_site].as_bytes();
        if self.displaying_sol {
            let sol = libm::floor(msd) as i64 - LANDING_SOLS[self.current_site] as i64;
            buf[0] = name[0];
            buf[1] = name[1];
            buf[2] = b' ';
            buf[3] = b' ';
            if sol < 1000 {
                buf[4] = b'S';
                buf[5] = b'o';
                buf[6] = b'l';
                buf[7] = b'0' + ((sol / 100) % 10) as u8;
                buf[8] = b'0' + ((sol / 10) % 10) as u8;
                buf[9] = b'0' + (sol % 10) as u8;
            } else {
                buf[4] = b'$';
                buf[5] = b' ';
                write_sol(&mut buf, sol, 6);
            }
            watch::slcd::clear_colon();
            watch::slcd::clear_indicator(Indicator::H24);
        } else {
            let mars_time = h_to_hms(lmt);
            buf[0] = name[0];
            buf[1] = name[1];
            buf[2] = b' ';
            buf[3] = b' ';
            buf[4] = b'0' + mars_time.hour / 10;
            buf[5] = b'0' + mars_time.hour % 10;
            buf[6] = b'0' + mars_time.minute / 10;
            buf[7] = b'0' + mars_time.minute % 10;
            buf[8] = b'0' + mars_time.second / 10;
            buf[9] = b'0' + mars_time.second % 10;
            watch::slcd::set_colon();
            watch::slcd::set_indicator(Indicator::H24);
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

/// Writes a sol value (>= 1000) right-aligned into the buffer at the given offset.
fn write_sol(buf: &mut [u8; 11], sol: i64, offset: usize) {
    let mut v = sol;
    let mut i = 9;
    loop {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if i == offset || v == 0 {
            break;
        }
        i -= 1;
    }
}

impl WatchFace for MarsTimeFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => self.update(settings),
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.displaying_sol = !self.displaying_sol;
                self.update(settings);
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => movement::illuminate_led(),
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.current_site = (self.current_site + 1) % MARS_TIME_NUM_SITES;
                self.update(settings);
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
