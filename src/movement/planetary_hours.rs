//! Planetary hours watch face.
//!
//! Port of the C `planetary_hours_face.c`. Shows the current planetary hour and
//! its ruling planet. It is a pure state machine: it reacts to a single event
//! and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const PLANETS: [&str; 7] = ["Sa", "Ju", "Ma", "So", "Ve", "Me", "Lu"];
const PLANETES: [&str; 7] = ["Ch", "Ze", "Ar", "He", "Af", "Hr", "Se"];
const PLINDEX: [u8; 7] = [3, 6, 2, 5, 1, 4, 0];

/// The planetary hours face state.
pub struct PlanetaryHoursFace {
    no_location: bool,
    start_at_night: bool,
    ruler: u8,
    hour: u8,
    skip_to_current: bool,
    phase_start: u32,
    phase_end: u32,
    phase_next: u32,
    planetary_hours: [u32; 24],
    utc_offset: f64,
}

impl PlanetaryHoursFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        PlanetaryHoursFace {
            no_location: false,
            start_at_night: false,
            ruler: 0,
            hour: 0,
            skip_to_current: true,
            phase_start: 0,
            phase_end: 0,
            phase_next: 0,
            planetary_hours: [0; 24],
            utc_offset: 0.0,
        }
    }

    pub fn new() -> Self {
        PlanetaryHoursFace::new_static()
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

    fn sun_rise_set(&self, year: u16, month: u8, day: u8) -> (f64, f64) {
        let n = utility::days_since_new_year(year, month, day);
        let decl = 23.44 * libm::sin(2.0 * core::f64::consts::PI * (284.0 + n as f64) / 365.0);
        let decl_rad = decl * core::f64::consts::PI / 180.0;
        let cos_h = -libm::tan(0.0) * libm::tan(decl_rad);
        let h = libm::acos(cos_h.clamp(-1.0, 1.0)) * 180.0 / core::f64::consts::PI;
        (12.0 - h / 15.0, 12.0 + h / 15.0)
    }

    fn planetary_solar_phases(&mut self, settings: &Settings) {
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

        let midnight_epoch_today = utility::date_time_to_unix_time(midnight, 0);
        let (sunrise, sunset) = self.sun_rise_set(
            utc_now.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            utc_now.month,
            utc_now.day,
        );
        let sunrise_epoch_today = midnight_epoch_today + (sunrise * 3600.0) as u32;
        let sunset_epoch_today = midnight_epoch_today + (sunset * 3600.0) as u32;

        let midnight_epoch_yesterday = midnight_epoch_today - 86400;
        let scratch = utility::date_time_from_unix_time(midnight_epoch_yesterday, 0);
        let (_, sunset_y) = self.sun_rise_set(
            scratch.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            scratch.month,
            scratch.day,
        );
        let sunset_epoch_yesterday = midnight_epoch_yesterday + (sunset_y * 3600.0) as u32;

        let midnight_epoch_tomorrow = midnight_epoch_today + 86400;
        let scratch = utility::date_time_from_unix_time(midnight_epoch_tomorrow, 0);
        let (sunrise_t, _) = self.sun_rise_set(
            scratch.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            scratch.month,
            scratch.day,
        );
        let sunrise_epoch_tomorrow = midnight_epoch_tomorrow + (sunrise_t * 3600.0) as u32;
        let sunset_epoch_tomorrow = midnight_epoch_tomorrow + (sunset * 3600.0) as u32;

        let now_epoch = utility::date_time_to_unix_time(utc_now, 0);

        phase = 1;
        self.phase_start = sunrise_epoch_today;
        self.phase_end = sunset_epoch_today;
        self.phase_next = sunrise_epoch_tomorrow;
        self.start_at_night = false;

        if now_epoch < sunrise_epoch_today && now_epoch < sunset_epoch_today {
            phase = 0;
        }
        if now_epoch > sunrise_epoch_today && now_epoch >= sunset_epoch_today {
            phase = 2;
        }
        if phase == 0 {
            self.phase_start = sunset_epoch_yesterday;
            self.phase_end = sunrise_epoch_today;
            self.phase_next = sunset_epoch_today;
            self.start_at_night = true;
        }
        if phase == 2 {
            self.phase_start = sunset_epoch_today;
            self.phase_end = sunrise_epoch_tomorrow;
            self.phase_next = sunset_epoch_tomorrow;
            self.start_at_night = true;
        }

        let hour_duration = (self.phase_end - self.phase_start) as f64 / 12.0;
        let next_hour_duration = (self.phase_next - self.phase_end) as f64 / 12.0;
        for h in 0..24 {
            if h < 12 {
                self.planetary_hours[h] = self.phase_start + (h as f64 * hour_duration) as u32;
            } else {
                self.planetary_hours[h] =
                    self.phase_end + ((h - 12) as f64 * next_hour_duration) as u32;
            }
        }
        self.hour = 0;
        self.ruler = 0;
        self.skip_to_current = true;
    }

    fn planetary_hours(&mut self, settings: &Settings) {
        if self.no_location {
            watch::slcd::display_string("    no Loc", 0);
            return;
        }
        let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60)
            as u32;
        let utc_now = utility::date_time_convert_zone(rtc::get_date_time(), tz, 0);
        let current_hour_epoch = utility::date_time_to_unix_time(utc_now, 0);

        if self.skip_to_current {
            self.hour = ((current_hour_epoch - self.phase_start) as f64
                / ((self.phase_end - self.phase_start) as f64 / 12.0))
                as u8;
            self.skip_to_current = false;
        }

        if utility::date_time_to_unix_time(utc_now, 0) >= self.phase_end {
            self.planetary_solar_phases(settings);
            return;
        }

        if settings.clock_mode_24h() && !settings.clock_24h_leading_zero() {
            watch::slcd::set_indicator(Indicator::H24);
        }

        if self.hour > 23 {
            self.hour = 0;
        }

        watch::slcd::clear_indicator(Indicator::Bell);
        watch::slcd::clear_indicator(Indicator::Lap);

        if self.hour < 24
            && current_hour_epoch >= self.planetary_hours[self.hour as usize]
            && current_hour_epoch < self.planetary_hours[self.hour as usize + 1]
        {
            watch::slcd::set_indicator(Indicator::Bell);
        }
        if self.start_at_night && self.hour > 11 {
            watch::slcd::set_indicator(Indicator::Lap);
        }

        let mut scratch = utility::date_time_from_unix_time(self.phase_start, 0);
        let mut weekday = utility::get_iso8601_weekday_number(
            scratch.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            scratch.month,
            scratch.day,
        ) - 1;

        let mut planetary_hour = self.hour % 12;
        if self.hour < 12 {
            if self.start_at_night {
                planetary_hour += 12;
            }
        } else if self.start_at_night {
            weekday = (weekday + 1) % 7;
        } else {
            planetary_hour += 12;
        }

        scratch = utility::date_time_from_unix_time(self.planetary_hours[self.hour as usize], 0);
        if scratch.second < 30 && scratch.minute > 0 {
            scratch.minute -= 1;
        } else if scratch.minute < 59 {
            scratch.minute += 1;
        }

        let mut set_leading_zero = false;
        if !settings.clock_mode_24h() {
            if scratch.hour < 12 {
                watch::slcd::clear_indicator(Indicator::Pm);
            } else {
                watch::slcd::set_indicator(Indicator::Pm);
            }
            scratch.hour %= 12;
            if scratch.hour == 0 {
                scratch.hour = 12;
            }
        } else if settings.clock_24h_leading_zero() && scratch.hour < 10 {
            set_leading_zero = true;
        }

        let planet = (PLINDEX[weekday as usize] + planetary_hour) % 7;
        let ruler = match self.ruler {
            0 => PLANETS[planet as usize],
            1 => PLANETES[planet as usize],
            _ => "  ",
        };
        let rb = ruler.as_bytes();

        let mut buf = [0u8; 11];
        buf[0] = rb[0];
        buf[1] = rb[1];
        buf[2] = b'0' + ((planetary_hour % 24) + 1) / 10;
        buf[3] = b'0' + ((planetary_hour % 24) + 1) % 10;
        buf[4] = b'0' + scratch.hour / 10;
        buf[5] = b'0' + scratch.hour % 10;
        buf[6] = b'0' + scratch.minute / 10;
        buf[7] = b'0' + scratch.minute % 10;
        buf[8] = b' ';
        buf[9] = b' ';
        watch::slcd::set_colon();
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        if set_leading_zero {
            watch::slcd::display_string("0", 4);
        }
        if self.ruler == 2 {
            self.planetary_icon(planet);
        }
    }
}

impl WatchFace for PlanetaryHoursFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, settings: &Settings) {
        if watch::slcd::tick_animation_is_running() {
            watch::slcd::stop_tick_animation();
        }
        self.planetary_solar_phases(settings);
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => {
                watch::slcd::clear_indicator(Indicator::Pm);
                watch::slcd::clear_indicator(Indicator::H24);
                self.planetary_hours(settings);
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.ruler = (self.ruler + 1) % 3;
                self.planetary_hours(settings);
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.skip_to_current = true;
                self.planetary_hours(settings);
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.hour += 1;
                self.planetary_hours(settings);
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.hour -= 1;
                self.planetary_hours(settings);
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
