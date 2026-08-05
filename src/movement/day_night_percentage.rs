//! Day/night percentage watch face.
//!
//! Port of the C `day_night_percentage_face.c`. Shows what percentage of the
//! day or night has elapsed. It is a pure state machine: it reacts to a single
//! event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

/// The day/night percentage face state.
pub struct DayNightPercentageFace {
    result: i8,
    daylen: f64,
    rise: f64,
    set: f64,
}

impl DayNightPercentageFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        DayNightPercentageFace {
            result: 0,
            daylen: 12.0,
            rise: 6.0,
            set: 18.0,
        }
    }

    pub fn new() -> Self {
        DayNightPercentageFace::new_static()
    }

    fn better_fmod(x: f64, y: f64) -> f64 {
        libm::fmod(libm::fmod(x, y) + y, y)
    }

    fn sun_rise_set(&self, year: u16, month: u8, day: u8) -> (f64, f64, u8) {
        let n = utility::days_since_new_year(year, month, day);
        let decl = 23.44 * libm::sin(2.0 * core::f64::consts::PI * (284.0 + n as f64) / 365.0);
        let decl_rad = decl * core::f64::consts::PI / 180.0;
        let cos_h = -libm::tan(0.0) * libm::tan(decl_rad);
        let h = libm::acos(cos_h.clamp(-1.0, 1.0)) * 180.0 / core::f64::consts::PI;
        (12.0 - h / 15.0, 12.0 + h / 15.0, 0)
    }

    fn recalculate(&mut self, utc_now: rtc::DateTime) {
        let (rise, set, result) = self.sun_rise_set(
            utc_now.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            utc_now.month,
            utc_now.day,
        );
        self.result = result as i8;
        self.rise = rise;
        self.set = set;
        self.daylen = set - rise;
    }
}

impl WatchFace for DayNightPercentageFace {
    fn setup(&mut self, settings: &Settings, _watch_face_index: usize) {
        let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60)
            as u32;
        let utc_now = utility::date_time_convert_zone(rtc::get_date_time(), tz, 0);
        self.recalculate(utc_now);
    }

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => {
                let date_time = rtc::get_date_time();
                let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)]
                    as i32
                    * 60) as u32;
                let utc_now = utility::date_time_convert_zone(date_time, tz, 0);
                if (utc_now.hour == 0 && utc_now.minute == 0 && utc_now.second == 0)
                    || self.result == -2
                {
                    self.recalculate(utc_now);
                }
                if self.result == -2 {
                    watch::slcd::display_string("    no Loc", 0);
                    return;
                }
                let mut buf = [0u8; 11];
                if self.result != 0 {
                    if self.result == 1 {
                        watch::slcd::clear_indicator(Indicator::Pm);
                    } else {
                        watch::slcd::set_indicator(Indicator::Pm);
                    }
                    let weekday = utility::get_weekday(date_time);
                    let wb = weekday.as_bytes();
                    buf[0] = wb[0];
                    buf[1] = wb[1];
                    buf[2] = b'0' + date_time.day / 10;
                    buf[3] = b'0' + date_time.day % 10;
                    buf[4] = b'E';
                    buf[5] = b't';
                    buf[6] = b'r';
                    buf[7] = b'n';
                    buf[8] = b'a';
                    buf[9] = b'l';
                    watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
                } else {
                    let day_hours_decimal = utc_now.hour as f64
                        + (utc_now.minute as f64 + utc_now.second as f64 / 60.0) / 60.0;
                    let day_percentage = (24.0
                        - Self::better_fmod(self.rise - day_hours_decimal, 24.0))
                        / self.daylen;
                    let night_percentage = (24.0
                        - Self::better_fmod(self.set - day_hours_decimal, 24.0))
                        / (24.0 - self.daylen);
                    let percentage;
                    if day_percentage > 0.0 && day_percentage < 1.0 {
                        percentage = (day_percentage * 10000.0) as u16;
                        watch::slcd::clear_indicator(Indicator::Pm);
                    } else {
                        percentage = (night_percentage * 10000.0) as u16;
                        watch::slcd::set_indicator(Indicator::Pm);
                    }
                    let weekday = utility::get_weekday(date_time);
                    let wb = weekday.as_bytes();
                    buf[0] = wb[0];
                    buf[1] = wb[1];
                    buf[2] = b'0' + date_time.day / 10;
                    buf[3] = b'0' + date_time.day % 10;
                    buf[4] = b' ';
                    buf[5] = b' ';
                    buf[6] = b'0' + ((percentage / 1000) % 10) as u8;
                    buf[7] = b'0' + ((percentage / 100) % 10) as u8;
                    buf[8] = b'0' + ((percentage / 10) % 10) as u8;
                    buf[9] = b'0' + (percentage % 10) as u8;
                    watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
