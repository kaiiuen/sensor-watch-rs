//! Sunrise/sunset watch face.
//!
//! Port of the C `sunrise_sunset_face.c`. Shows the next sunrise or sunset time
//! for a configured location. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

/// A latitude/longitude preset.
struct LatLonPreset {
    name: &'static str,
    latitude: i16,
    longitude: i16,
}

const LONG_LAT_PRESETS: [LatLonPreset; 1] = [LatLonPreset {
    name: " ",
    latitude: 0,
    longitude: 0,
}];
const LOCATION_COUNT: u8 = 1;

/// The sunrise/sunset face state.
pub struct SunriseSunsetFace {
    page: u8,
    active_digit: u8,
    rise_index: u8,
    long_lat_to_use: u8,
    rise_set_expires: rtc::DateTime,
    location_changed: bool,
    working_latitude: i16,
    working_longitude: i16,
}

impl SunriseSunsetFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        SunriseSunsetFace {
            page: 0,
            active_digit: 0,
            rise_index: 0,
            long_lat_to_use: 0,
            rise_set_expires: rtc::DateTime {
                second: 0,
                minute: 0,
                hour: 0,
                day: 0,
                month: 0,
                year: 0,
            },
            location_changed: false,
            working_latitude: 0,
            working_longitude: 0,
        }
    }

    pub fn new() -> Self {
        SunriseSunsetFace::new_static()
    }

    fn set_expiration(&mut self, next_rise_set: rtc::DateTime) {
        let timestamp = utility::date_time_to_unix_time(next_rise_set, 0);
        self.rise_set_expires = utility::date_time_from_unix_time(timestamp + 60, 0);
    }

    fn sun_rise_set(&self, year: u16, month: u8, day: u8) -> (f64, f64, u8) {
        let n = utility::days_since_new_year(year, month, day);
        let decl = 23.44 * libm::sin(2.0 * core::f64::consts::PI * (284.0 + n as f64) / 365.0);
        let decl_rad = decl * core::f64::consts::PI / 180.0;
        let cos_h = -libm::tan(0.0) * libm::tan(decl_rad);
        let h = libm::acos(cos_h.clamp(-1.0, 1.0)) * 180.0 / core::f64::consts::PI;
        (12.0 - h / 15.0, 12.0 + h / 15.0, 0)
    }

    fn update(&mut self, settings: &Settings) {
        let mut buf = [0u8; 11];
        let date_time = rtc::get_date_time();
        let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60)
            as u32;
        let mut utc_now = utility::date_time_convert_zone(date_time, tz, 0);
        let mut scratch = utc_now;
        let hours_from_utc =
            movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as f64 / 60.0;

        let mut show_next_match = false;
        for _ in 0..2 {
            let (rise, set, result) = self.sun_rise_set(
                scratch.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
                scratch.month,
                scratch.day,
            );
            if result != 0 {
                watch::slcd::clear_colon();
                watch::slcd::clear_indicator(Indicator::Pm);
                watch::slcd::clear_indicator(Indicator::H24);
                let prefix = if result == 1 { "SE" } else { "rI" };
                let pb = prefix.as_bytes();
                buf[0] = pb[0];
                buf[1] = pb[1];
                buf[2] = b'0' + scratch.day / 10;
                buf[3] = b'0' + scratch.day % 10;
                buf[4] = b' ';
                buf[5] = b'n';
                buf[6] = b'o';
                buf[7] = b'n';
                buf[8] = b'e';
                buf[9] = b' ';
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
                return;
            }
            watch::slcd::set_colon();
            if settings.clock_mode_24h() && !settings.clock_24h_leading_zero() {
                watch::slcd::set_indicator(Indicator::H24);
            }

            let rise = rise + hours_from_utc;
            let set = set + hours_from_utc;

            let mut minutes = 60.0 * libm::fmod(rise, 1.0);
            let seconds = 60.0 * libm::fmod(minutes, 1.0);
            scratch.hour = libm::floor(rise) as u8;
            scratch.minute = if seconds < 30.0 {
                libm::floor(minutes) as u8
            } else {
                libm::ceil(minutes) as u8
            };
            if scratch.minute == 60 {
                scratch.minute = 0;
                scratch.hour = (scratch.hour + 1) % 24;
            }

            if date_time.to_reg() < scratch.to_reg() {
                self.set_expiration(scratch);
            }
            if date_time.to_reg() < scratch.to_reg() || show_next_match {
                if self.rise_index == 0 || show_next_match {
                    let mut set_leading_zero = false;
                    if !settings.clock_mode_24h() {
                        let pm = utility::convert_to_12_hour(&mut scratch);
                        if pm {
                            watch::slcd::set_indicator(Indicator::Pm);
                        } else {
                            watch::slcd::clear_indicator(Indicator::Pm);
                        }
                    } else if settings.clock_24h_leading_zero() && scratch.hour < 10 {
                        set_leading_zero = true;
                    }
                    buf[0] = b'r';
                    buf[1] = b'I';
                    buf[2] = b'0' + scratch.day / 10;
                    buf[3] = b'0' + scratch.day % 10;
                    buf[4] = b'0' + scratch.hour / 10;
                    buf[5] = b'0' + scratch.hour % 10;
                    buf[6] = b'0' + scratch.minute / 10;
                    buf[7] = b'0' + scratch.minute % 10;
                    let name = LONG_LAT_PRESETS[self.long_lat_to_use as usize]
                        .name
                        .as_bytes();
                    buf[8] = name[0];
                    buf[9] = name[1];
                    watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
                    if set_leading_zero {
                        watch::slcd::display_string("0", 4);
                    }
                    return;
                } else {
                    show_next_match = true;
                }
            }

            minutes = 60.0 * libm::fmod(set, 1.0);
            let seconds = 60.0 * libm::fmod(minutes, 1.0);
            scratch.hour = libm::floor(set) as u8;
            scratch.minute = if seconds < 30.0 {
                libm::floor(minutes) as u8
            } else {
                libm::ceil(minutes) as u8
            };
            if scratch.minute == 60 {
                scratch.minute = 0;
                scratch.hour = (scratch.hour + 1) % 24;
            }

            if date_time.to_reg() < scratch.to_reg() {
                self.set_expiration(scratch);
            }
            if date_time.to_reg() < scratch.to_reg() || show_next_match {
                if self.rise_index == 0 || show_next_match {
                    let mut set_leading_zero = false;
                    if !settings.clock_mode_24h() {
                        let pm = utility::convert_to_12_hour(&mut scratch);
                        if pm {
                            watch::slcd::set_indicator(Indicator::Pm);
                        } else {
                            watch::slcd::clear_indicator(Indicator::Pm);
                        }
                    } else if settings.clock_24h_leading_zero() && scratch.hour < 10 {
                        set_leading_zero = true;
                    }
                    buf[0] = b'S';
                    buf[1] = b'E';
                    buf[2] = b'0' + scratch.day / 10;
                    buf[3] = b'0' + scratch.day % 10;
                    buf[4] = b'0' + scratch.hour / 10;
                    buf[5] = b'0' + scratch.hour % 10;
                    buf[6] = b'0' + scratch.minute / 10;
                    buf[7] = b'0' + scratch.minute % 10;
                    let name = LONG_LAT_PRESETS[self.long_lat_to_use as usize]
                        .name
                        .as_bytes();
                    buf[8] = name[0];
                    buf[9] = name[1];
                    watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
                    if set_leading_zero {
                        watch::slcd::display_string("0", 4);
                    }
                    return;
                } else {
                    show_next_match = true;
                }
            }

            let timestamp = utility::date_time_to_unix_time(utc_now, 0);
            utc_now = utility::date_time_from_unix_time(timestamp + 86400, 0);
            scratch = utc_now;
        }
    }

    fn update_settings_display(&self) {
        let mut buf = [0u8; 11];
        match self.page {
            1 => {
                buf[0] = b'L';
                buf[1] = b'A';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = if self.working_latitude < 0 {
                    b'-'
                } else {
                    b'+'
                };
                let v = self.working_latitude.unsigned_abs();
                buf[5] = b'0' + ((v / 1000) % 10) as u8;
                buf[6] = b'0' + ((v / 100) % 10) as u8;
                buf[7] = b'0' + ((v / 10) % 10) as u8;
                buf[8] = b'0' + (v % 10) as u8;
            }
            2 => {
                buf[0] = b'L';
                buf[1] = b'O';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = if self.working_longitude < 0 {
                    b'-'
                } else {
                    b'+'
                };
                let v = self.working_longitude.unsigned_abs();
                buf[5] = b'0' + ((v / 10000) % 10) as u8;
                buf[6] = b'0' + ((v / 1000) % 10) as u8;
                buf[7] = b'0' + ((v / 100) % 10) as u8;
                buf[8] = b'0' + ((v / 10) % 10) as u8;
                buf[9] = b'0' + (v % 10) as u8;
            }
            _ => return,
        }
        if 0 % 2 == 1 {
            buf[self.active_digit as usize + 4] = b' ';
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn advance_digit(&mut self) {
        self.location_changed = true;
        let v = if self.page == 1 {
            &mut self.working_latitude
        } else {
            &mut self.working_longitude
        };
        let max = if self.page == 1 { 9000 } else { 18000 };
        let value = *v;
        let digit = match self.active_digit {
            0 => {
                return;
            }
            1 => 1000,
            2 => 100,
            3 => 10,
            _ => 1,
        };
        let mut abs = value.unsigned_abs();
        let place = abs / digit as u16 % 10;
        abs = abs - place * digit as u16 + ((place + 1) % 10) * digit as u16;
        if abs > max as u16 {
            abs = 0;
        }
        *v = if value < 0 { -(abs as i16) } else { abs as i16 };
    }
}

impl WatchFace for SunriseSunsetFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        if watch::slcd::tick_animation_is_running() {
            watch::slcd::stop_tick_animation();
        }
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => self.update(settings),
            Event::Tick => {
                if self.page == 0 {
                    let date_time = rtc::get_date_time();
                    if date_time.to_reg() >= self.rise_set_expires.to_reg() {
                        self.rise_index = 0;
                        self.update(settings);
                    }
                } else {
                    self.update_settings_display();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                if self.page != 0 {
                    self.active_digit += 1;
                    if self.page == 1 && self.active_digit == 1 {
                        self.active_digit += 1;
                    }
                    if self.active_digit > 5 {
                        self.active_digit = 0;
                        self.page = (self.page + 1) % 3;
                    }
                    self.update_settings_display();
                } else if LOCATION_COUNT <= 1 {
                    movement::illuminate_led();
                }
                if self.page == 0 {
                    self.update(settings);
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if LOCATION_COUNT <= 1 {
                } else if self.page == 0 {
                    movement::illuminate_led();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.page == 0 && LOCATION_COUNT > 1 {
                    self.long_lat_to_use = (self.long_lat_to_use + 1) % 2;
                    self.update(settings);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.page != 0 {
                    self.advance_digit();
                    self.update_settings_display();
                } else {
                    self.rise_index = (self.rise_index + 1) % 2;
                    self.update(settings);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.page == 0 {
                    if self.long_lat_to_use != 0 {
                        self.long_lat_to_use = 0;
                        self.update(settings);
                    } else {
                        self.page += 1;
                        self.active_digit = 0;
                        watch::slcd::clear_display();
                        self.update_settings_display();
                    }
                } else {
                    self.active_digit = 0;
                    self.page = 0;
                    self.update(settings);
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.page = 0;
        self.active_digit = 0;
        self.rise_index = 0;
    }
}
