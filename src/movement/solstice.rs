//! Solstice watch face.
//!
//! Port of the C `solstice_face.c`. Shows the dates of the solstices and
//! equinoxes for a given year. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const APPROX_TERMS: [[f64; 5]; 4] = [
    [2451623.80984, 365242.37404, 0.05169, -0.00411, -0.00057],
    [2451716.56767, 365241.62603, 0.00325, 0.00888, -0.00030],
    [2451810.21715, 365242.01767, -0.11575, 0.00337, 0.00078],
    [2451900.05952, 365242.74049, -0.06223, -0.00823, 0.00032],
];

const CORRECTION_TERMS: [[f64; 3]; 24] = [
    [485.0, 324.96, 1934.136],
    [203.0, 337.23, 32964.467],
    [199.0, 342.08, 20.186],
    [182.0, 27.85, 445267.112],
    [156.0, 73.14, 45036.886],
    [136.0, 171.52, 22518.443],
    [77.0, 222.54, 65928.934],
    [74.0, 296.72, 3034.906],
    [70.0, 243.58, 9037.513],
    [58.0, 119.81, 33718.147],
    [52.0, 297.17, 150.678],
    [50.0, 21.02, 2281.226],
    [45.0, 247.54, 29929.562],
    [44.0, 325.15, 31555.956],
    [29.0, 60.93, 4443.417],
    [18.0, 155.12, 67555.328],
    [17.0, 288.79, 4562.452],
    [16.0, 198.04, 62894.029],
    [14.0, 199.76, 31436.921],
    [12.0, 95.39, 14577.848],
    [12.0, 287.11, 31931.756],
    [12.0, 320.81, 34777.259],
    [9.0, 227.73, 1222.114],
    [8.0, 15.45, 16859.074],
];

/// The solstice face state.
pub struct SolsticeFace {
    year: u8,
    index: u8,
    datetimes: [rtc::DateTime; 4],
}

impl SolsticeFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        SolsticeFace {
            year: 0,
            index: 0,
            datetimes: [rtc::DateTime {
                second: 0,
                minute: 0,
                hour: 0,
                day: 0,
                month: 0,
                year: 0,
            }; 4],
        }
    }

    pub fn new() -> Self {
        SolsticeFace::new_static()
    }

    fn calculate_solstice_equinox(year: u16, k: u8) -> f64 {
        let y = (year as f64 - 2000.0) / 1000.0;
        let t0 = APPROX_TERMS[k as usize];
        let jde0 = t0[0] + y * (t0[1] + y * (t0[2] + y * (t0[3] + y * t0[4])));
        let t = (jde0 - 2451545.0) / 36525.0;
        let w = 35999.373 * t - 2.47;
        let dlambda = 1.0
            + (0.0334 * libm::cos(w * core::f64::consts::PI / 180.0))
            + (0.0007 * libm::cos(2.0 * w * core::f64::consts::PI / 180.0));
        let mut s = 0.0;
        for term in CORRECTION_TERMS.iter() {
            s += term[0] * libm::cos((term[1] + term[2] * t) * core::f64::consts::PI / 180.0);
        }
        jde0 + (0.00001 * s) / dlambda
    }

    fn jde_to_date_time(jde: f64) -> rtc::DateTime {
        let tmp = jde + 0.5;
        let z = libm::floor(tmp);
        let f = libm::fmod(tmp, 1.0);
        let a = if z < 2299161.0 {
            z
        } else {
            let alpha = libm::floor((z - 1867216.25) / 36524.25);
            z + 1.0 + alpha - libm::floor(alpha / 4.0)
        };
        let b = a + 1524.0;
        let c = libm::floor((b - 122.1) / 365.25);
        let d = libm::floor(365.25 * c);
        let e = libm::floor((b - d) / 30.6001);
        let day = b - d - libm::floor(30.6001 * e) + f;
        let month = if e < 14.0 { e - 1.0 } else { e - 13.0 };
        let year = if month > 2.0 { c - 4716.0 } else { c - 4715.0 };
        let hours = libm::fmod(day, 1.0) * 24.0;
        let minutes = libm::fmod(hours, 1.0) * 60.0;
        let seconds = libm::fmod(minutes, 1.0) * 60.0;
        rtc::DateTime {
            second: libm::floor(seconds) as u8,
            minute: libm::floor(minutes) as u8,
            hour: libm::floor(hours) as u8,
            day: libm::floor(day) as u8,
            month: libm::floor(month) as u8,
            year: libm::floor(year - 2020.0) as u8,
        }
    }

    fn calculate_datetimes(&mut self, settings: &Settings) {
        let tz = movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as f64
            / (60.0 * 24.0);
        for i in 0..4u8 {
            self.datetimes[i as usize] = Self::jde_to_date_time(
                Self::calculate_solstice_equinox(2020 + self.year as u16, i) + tz,
            );
        }
    }

    fn show_main_screen(&self) {
        let mut buf = [0u8; 11];
        let dt = self.datetimes[self.index as usize];
        buf[0] = b' ';
        buf[1] = b' ';
        buf[2] = b'0' + (dt.year + 20) / 10;
        buf[3] = b'0' + (dt.year + 20) % 10;
        buf[4] = b' ';
        buf[5] = b' ';
        buf[6] = b'0' + dt.month / 10;
        buf[7] = b'0' + dt.month % 10;
        buf[8] = b'0' + dt.day / 10;
        buf[9] = b'0' + dt.day % 10;
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn show_date_time(&self, settings: &Settings) {
        let mut buf = [0u8; 11];
        let mut dt = self.datetimes[self.index as usize];
        if !settings.clock_mode_24h() {
            if dt.hour < 12 {
                watch::slcd::clear_indicator(Indicator::Pm);
            } else {
                watch::slcd::set_indicator(Indicator::Pm);
            }
            dt.hour %= 12;
            if dt.hour == 0 {
                dt.hour = 12;
            }
        }
        let weekday = utility::get_weekday(dt);
        let wb = weekday.as_bytes();
        buf[0] = wb[0];
        buf[1] = wb[1];
        buf[2] = b'0' + dt.day / 10;
        buf[3] = b'0' + dt.day % 10;
        buf[4] = b'0' + dt.hour / 10;
        buf[5] = b'0' + dt.hour % 10;
        buf[6] = b'0' + dt.minute / 10;
        buf[7] = b'0' + dt.minute % 10;
        buf[8] = b'0' + dt.second / 10;
        buf[9] = b'0' + dt.second % 10;
        watch::slcd::set_colon();
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

impl WatchFace for SolsticeFace {
    fn setup(&mut self, settings: &Settings, _watch_face_index: usize) {
        let now = rtc::get_date_time();
        self.year = now.year;
        self.index = 0;
        self.calculate_datetimes(settings);
        let now_unix = utility::date_time_to_unix_time(now, 0);
        for i in 0..4 {
            if self.index == 0 && utility::date_time_to_unix_time(self.datetimes[i], 0) > now_unix {
                self.index = i as u8;
            }
        }
    }

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => self.show_date_time(settings),
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.index == 0 {
                    if self.year == 0 {
                        return;
                    }
                    self.year -= 1;
                    self.index = 3;
                    self.calculate_datetimes(settings);
                } else {
                    self.index -= 1;
                }
                self.show_main_screen();
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.index += 1;
                if self.index > 3 {
                    if self.year == 83 {
                        return;
                    }
                    self.year += 1;
                    self.index = 0;
                    self.calculate_datetimes(settings);
                }
                self.show_main_screen();
            }
            Event::Button(Button::Alarm, ButtonEvent::LongUp) => {
                watch::slcd::clear_colon();
                watch::slcd::clear_indicator(Indicator::Pm);
                self.show_main_screen();
            }
            Event::Activate => self.show_main_screen(),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
