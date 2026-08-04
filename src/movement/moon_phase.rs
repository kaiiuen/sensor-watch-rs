//! Moon phase watch face.
//!
//! Port of the C `moon_phase_face.c`. Shows the current moon phase as a small
//! pixel-art moon plus a phase label. It is a pure state machine: it renders on
//! wake and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd;
use crate::watch::utility;

const LUNAR_DAYS: f64 = 29.530_587_705_76;
const LUNAR_SECONDS: f64 = LUNAR_DAYS * (24.0 * 60.0 * 60.0);
const FIRST_MOON: f64 = 947_182_440.0;
const NUM_PHASES: usize = 8;

const PHASE_CHANGES: [f64; 10] = [
    0.0,
    1.0,
    6.382_646_926_44,
    8.382_646_926_44,
    13.765_293_852_88,
    15.765_293_852_88,
    21.147_940_779_32,
    23.147_940_779_32,
    28.530_587_705_76,
    29.530_587_705_76,
];

/// The moon phase face state.
pub struct MoonPhaseFace {
    offset: u32,
}

impl MoonPhaseFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        MoonPhaseFace { offset: 0 }
    }

    pub fn new() -> Self {
        MoonPhaseFace::new_static()
    }

    fn update(&self, settings: &Settings, offset: u32) {
        let mut buf = [0u8; 11];
        let date_time = rtc::get_date_time();
        let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60)
            as u32;
        let now = utility::date_time_to_unix_time(date_time, tz) + offset;
        let dt = utility::date_time_from_unix_time(now, tz);
        let currentfrac = libm::fmod(now as f64 - FIRST_MOON, LUNAR_SECONDS) / LUNAR_SECONDS;
        let currentday = currentfrac * LUNAR_DAYS;
        let mut phase_index = 0;
        for i in 0..=NUM_PHASES {
            if currentday > PHASE_CHANGES[i] && currentday <= PHASE_CHANGES[i + 1] {
                phase_index = i;
                break;
            }
        }

        slcd::display_string(" ", 0);
        match phase_index {
            0 | 8 => {
                buf[0] = b'0' + dt.day / 10;
                buf[1] = b'0' + dt.day % 10;
                buf[2] = b' ';
                buf[3] = b'N';
                buf[4] = b'e';
                buf[5] = b'u';
                buf[6] = b' ';
                buf[7] = b' ';
            }
            1 => {
                buf[0] = b'0' + dt.day / 10;
                buf[1] = b'0' + dt.day % 10;
                buf[2] = b'C';
                buf[3] = b'r';
                buf[4] = b'e';
                buf[5] = b's';
                buf[6] = b'n';
                buf[7] = b't';
                slcd::set_pixel(2, 13);
                slcd::set_pixel(2, 15);
                if currentfrac > 0.125 {
                    slcd::set_pixel(1, 13);
                }
            }
            2 => {
                buf[0] = b'0' + dt.day / 10;
                buf[1] = b'0' + dt.day % 10;
                buf[2] = b' ';
                buf[3] = b'1';
                buf[4] = b's';
                buf[5] = b't';
                buf[6] = b' ';
                buf[7] = b'q';
                slcd::set_pixel(2, 13);
                slcd::set_pixel(2, 15);
                slcd::set_pixel(1, 13);
                slcd::set_pixel(1, 14);
            }
            3 => {
                buf[0] = b'0' + dt.day / 10;
                buf[1] = b'0' + dt.day % 10;
                buf[2] = b' ';
                buf[3] = b'G';
                buf[4] = b'i';
                buf[5] = b'b';
                buf[6] = b'b';
                buf[7] = b' ';
                slcd::set_pixel(2, 13);
                slcd::set_pixel(2, 15);
                slcd::set_pixel(1, 14);
                slcd::set_pixel(1, 13);
                slcd::set_pixel(1, 15);
            }
            4 => {
                buf[0] = b'0' + dt.day / 10;
                buf[1] = b'0' + dt.day % 10;
                buf[2] = b' ';
                buf[3] = b'F';
                buf[4] = b'U';
                buf[5] = b'L';
                buf[6] = b'L';
                buf[7] = b' ';
                slcd::set_pixel(2, 13);
                slcd::set_pixel(2, 15);
                slcd::set_pixel(1, 14);
                slcd::set_pixel(2, 14);
                slcd::set_pixel(1, 15);
                slcd::set_pixel(0, 14);
                slcd::set_pixel(0, 13);
                slcd::set_pixel(1, 13);
            }
            5 => {
                buf[0] = b'0' + dt.day / 10;
                buf[1] = b'0' + dt.day % 10;
                buf[2] = b' ';
                buf[3] = b'G';
                buf[4] = b'i';
                buf[5] = b'b';
                buf[6] = b'b';
                buf[7] = b' ';
                slcd::set_pixel(1, 14);
                slcd::set_pixel(2, 14);
                slcd::set_pixel(1, 15);
                slcd::set_pixel(0, 14);
                slcd::set_pixel(0, 13);
            }
            6 => {
                buf[0] = b'0' + dt.day / 10;
                buf[1] = b'0' + dt.day % 10;
                buf[2] = b' ';
                buf[3] = b'3';
                buf[4] = b'r';
                buf[5] = b'd';
                buf[6] = b' ';
                buf[7] = b'q';
                slcd::set_pixel(1, 14);
                slcd::set_pixel(2, 14);
                slcd::set_pixel(0, 14);
                slcd::set_pixel(0, 13);
            }
            _ => {
                buf[0] = b'0' + dt.day / 10;
                buf[1] = b'0' + dt.day % 10;
                buf[2] = b'C';
                buf[3] = b'r';
                buf[4] = b'e';
                buf[5] = b's';
                buf[6] = b'n';
                buf[7] = b't';
                slcd::set_pixel(0, 14);
                slcd::set_pixel(0, 13);
                if currentfrac < 0.875 {
                    slcd::set_pixel(2, 14);
                }
            }
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 2);
    }
}

impl WatchFace for MoonPhaseFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => self.update(settings, self.offset),
            Event::Tick => {
                let date_time = rtc::get_date_time();
                if date_time.minute == 0 && date_time.second == 0 {
                    self.update(settings, self.offset);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.offset += 86400;
                self.update(settings, self.offset);
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.offset = 0;
                self.update(settings, self.offset);
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.offset = 0;
    }
}
