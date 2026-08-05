//! Planetary time watch face.
//!
//! Port of the C `planetary_time_face.c`. Shows the time in planetary hours
//! (twelve hours per solar phase) with the ruling planet. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const PLANETS: [&str; 7] = ["Sa", "Ju", "Ma", "So", "Ve", "Me", "Lu"];
const PLANETES: [&str; 7] = ["Ch", "Ze", "Ar", "He", "Af", "Hr", "Se"];
const PLINDEX: [u8; 7] = [3, 6, 2, 5, 1, 4, 0];

/// The planetary time face state.
pub struct PlanetaryTimeFace {
    no_location: bool,
    night: bool,
    day_ruler: bool,
    ruler: u8,
    phase_start: u32,
    phase_end: u32,
    freq: f64,
    utc_offset: f64,
}

impl PlanetaryTimeFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        PlanetaryTimeFace {
            no_location: false,
            night: false,
            day_ruler: false,
            ruler: 0,
            phase_start: 0,
            phase_end: 0,
            freq: 1.0,
            utc_offset: 0.0,
        }
    }

    pub fn new() -> Self {
        PlanetaryTimeFace::new_static()
    }

    fn planetary_icon(&self, planet: u8) {
        for &(c, s) in [
            (0, 13),
            (0, 14),
            (1, 13),
            (1, 14),
            (1, 15),
            (2, 13),
            (2, 14),
            (2, 15),
        ]
        .iter()
        {
            watch::slcd::clear_pixel(c, s);
        }
        match planet {
            0 => {
                watch::slcd::set_pixel(0, 14);
                watch::slcd::set_pixel(2, 14);
                watch::slcd::set_pixel(1, 15);
                watch::slcd::set_pixel(2, 13);
            }
            1 => {
                watch::slcd::set_pixel(0, 14);
                watch::slcd::set_pixel(1, 15);
                watch::slcd::set_pixel(1, 14);
            }
            2 => {
                watch::slcd::set_pixel(2, 14);
                watch::slcd::set_pixel(2, 15);
                watch::slcd::set_pixel(1, 15);
                watch::slcd::set_pixel(2, 13);
                watch::slcd::set_pixel(1, 13);
            }
            3 => {
                watch::slcd::set_pixel(0, 14);
                watch::slcd::set_pixel(2, 14);
                watch::slcd::set_pixel(1, 13);
                watch::slcd::set_pixel(2, 13);
                watch::slcd::set_pixel(0, 13);
                watch::slcd::set_pixel(2, 15);
            }
            4 => {
                watch::slcd::set_pixel(0, 14);
                watch::slcd::set_pixel(0, 13);
                watch::slcd::set_pixel(1, 13);
                watch::slcd::set_pixel(1, 15);
                watch::slcd::set_pixel(1, 14);
            }
            5 => {
                watch::slcd::set_pixel(0, 14);
                watch::slcd::set_pixel(1, 13);
                watch::slcd::set_pixel(1, 14);
                watch::slcd::set_pixel(1, 15);
                watch::slcd::set_pixel(2, 15);
            }
            _ => {
                watch::slcd::set_pixel(2, 14);
                watch::slcd::set_pixel(2, 15);
                watch::slcd::set_pixel(2, 13);
            }
        }
    }

    fn sun_rise_set(&self, year: u16, month: u8, day: u8, lon: f64, lat: f64) -> (f64, f64) {
        // Simplified sunrise/sunset estimate (decimal hours after midnight).
        let n = utility::days_since_new_year(year, month, day);
        let decl = 23.44 * libm::sin(2.0 * core::f64::consts::PI * (284.0 + n as f64) / 365.0);
        let lat_rad = lat * core::f64::consts::PI / 180.0;
        let decl_rad = decl * core::f64::consts::PI / 180.0;
        let cos_h = -libm::tan(lat_rad) * libm::tan(decl_rad);
        let h = libm::acos(cos_h.clamp(-1.0, 1.0)) * 180.0 / core::f64::consts::PI;
        let sunrise = 12.0 - h / 15.0 - lon / 15.0;
        let sunset = 12.0 + h / 15.0 - lon / 15.0;
        (sunrise, sunset)
    }

    fn planetary_solar_phase(&mut self, settings: &Settings) {
        let mut phase;
        let date_time = rtc::get_date_time();
        let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60)
            as u32;
        let utc_now = utility::date_time_convert_zone(date_time, tz, 0);
        let mut midnight = utc_now;
        midnight.hour = 0;
        midnight.minute = 0;
        midnight.second = 0;

        self.utc_offset =
            movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as f64 / 60.0;

        let now_epoch = utility::date_time_to_unix_time(utc_now, 0);
        let mut midnight_epoch = utility::date_time_to_unix_time(midnight, 0);

        let (sunrise, sunset) = self.sun_rise_set(
            utc_now.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            utc_now.month,
            utc_now.day,
            0.0,
            0.0,
        );
        let mut sunrise_epoch = midnight_epoch + (sunrise * 3600.0) as u32;
        let mut sunset_epoch = midnight_epoch + (sunset * 3600.0) as u32;

        phase = 1;
        self.night = false;
        self.phase_start = sunrise_epoch;
        self.phase_end = sunset_epoch;

        if now_epoch < sunrise_epoch && now_epoch < sunset_epoch {
            phase = 0;
        }
        if now_epoch > sunrise_epoch && now_epoch >= sunset_epoch {
            phase = 2;
        }
        if phase == 0 {
            midnight_epoch -= 86400;
            let scratch = utility::date_time_from_unix_time(midnight_epoch, 0);
            let (_, sunset) = self.sun_rise_set(
                scratch.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
                scratch.month,
                scratch.day,
                0.0,
                0.0,
            );
            sunset_epoch = midnight_epoch + (sunset * 3600.0) as u32;
            self.night = true;
            self.phase_start = sunset_epoch;
            self.phase_end = sunrise_epoch;
        }
        if phase == 2 {
            midnight_epoch += 86400;
            let scratch = utility::date_time_from_unix_time(midnight_epoch, 0);
            let (sunrise, _) = self.sun_rise_set(
                scratch.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
                scratch.month,
                scratch.day,
                0.0,
                0.0,
            );
            sunrise_epoch = midnight_epoch + (sunrise * 3600.0) as u32;
            self.night = true;
            self.phase_start = sunset_epoch;
            self.phase_end = sunrise_epoch;
        }
        self.freq = 1.0 / ((self.phase_end - self.phase_start) as f64 / 43200.0);
    }

    fn planetary_time(&mut self, settings: &Settings) {
        watch::slcd::set_colon();
        let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60)
            as u32;
        let mut scratch = utility::date_time_convert_zone(rtc::get_date_time(), tz, 0);

        if utility::date_time_to_unix_time(scratch, 0) >= self.phase_end {
            self.planetary_solar_phase(settings);
            return;
        }

        if settings.clock_mode_24h() && !settings.clock_24h_leading_zero() {
            watch::slcd::set_indicator(Indicator::H24);
        }

        let mut night_hour_count = 0.0f64;
        if self.night {
            if settings.clock_mode_24h() {
                night_hour_count = 12.0;
            } else {
                watch::slcd::set_indicator(Indicator::Pm);
            }
        }

        let hour_duration = (self.phase_end - self.phase_start) as f64 / 12.0;
        let now_ts = utility::date_time_to_unix_time(scratch, 0) as f64;
        let mut current_hour = (now_ts - self.phase_start as f64) / hour_duration;
        let planetary_hour = libm::floor(current_hour) as u8 + if self.night { 12 } else { 0 };
        current_hour += night_hour_count;
        let (frac_hour, int_hour) = libm::modf(current_hour);
        current_hour = int_hour;
        let current_minute = frac_hour * 60.0;
        let (frac_min, int_min) = libm::modf(current_minute);
        let current_second = frac_min * 60.0;
        let current_minute = int_min;

        scratch = utility::date_time_from_unix_time(self.phase_start, 0);
        scratch.hour = libm::floor(current_hour) as u8;
        scratch.minute = libm::floor(current_minute) as u8;
        scratch.second = (libm::floor(current_second) as u8) % 60;

        let mut set_leading_zero = false;
        if settings.clock_mode_24h() && settings.clock_24h_leading_zero() && scratch.hour < 10 {
            set_leading_zero = true;
        }

        let weekday = utility::get_iso8601_weekday_number(
            scratch.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            scratch.month,
            scratch.day,
        ) - 1;

        let planet = if self.day_ruler {
            PLINDEX[weekday as usize]
        } else {
            (PLINDEX[weekday as usize] + planetary_hour) % 7
        };

        let ruler = match self.ruler {
            0 => PLANETS[planet as usize],
            1 => PLANETES[planet as usize],
            _ => "  ",
        };
        let rb = ruler.as_bytes();

        let mut buf = [0u8; 11];
        buf[0] = rb[0];
        buf[1] = rb[1];
        buf[2] = if self.day_ruler { b'd' } else { b'h' };
        buf[3] = b'0' + scratch.hour / 10;
        buf[4] = b'0' + scratch.hour % 10;
        buf[5] = b'0' + scratch.minute / 10;
        buf[6] = b'0' + scratch.minute % 10;
        buf[7] = b'0' + scratch.second / 10;
        buf[8] = b'0' + scratch.second % 10;
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        if set_leading_zero {
            watch::slcd::display_string("0", 4);
        }
        if self.ruler == 2 {
            self.planetary_icon(planet);
        }
    }
}

impl WatchFace for PlanetaryTimeFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, settings: &Settings) {
        if watch::slcd::tick_animation_is_running() {
            watch::slcd::stop_tick_animation();
        }
        self.planetary_solar_phase(settings);
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => {
                self.planetary_time(settings);
            }
            Event::Tick => self.planetary_time(settings),
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.ruler = (self.ruler + 1) % 3;
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.day_ruler = !self.day_ruler;
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
