//! Solar time watch face.
//!
//! Port of the C `solar_time_face.c`. Displays solar time information based on
//! the user's location. It is a pure state machine: it reacts to a single event
//! and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc::{self, DateTime};

/// Display modes for the solar time face.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SolarTimeMode {
    Lst = 0,  // Solar Time: SO HH:MM:SS
    Noon = 1, // Solar Noon (local): nO HH:MM
    Hra = 2,  // Hour Angle: Hr +/-DDD
}

const SOLAR_TIME_NUM_MODES: u8 = 3;

/// The backup register that stores the wearer's location (BKUP[1]).
const LOCATION_BACKUP_REG: u8 = 1;

/// The solar time face state.
pub struct SolarTimeFace {
    mode: SolarTimeMode,
    last_calc_d: u16,
    eot: f32, // Equation of Time [minutes]
    tc: f32,  // Time Correction Factor [minutes]
}

impl SolarTimeFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        SolarTimeFace {
            mode: SolarTimeMode::Lst,
            last_calc_d: 0,
            eot: 0.0,
            tc: 0.0,
        }
    }

    pub fn new() -> Self {
        SolarTimeFace::new_static()
    }

    /// Reads the wearer's location from the backup register.
    fn load_location() -> u32 {
        crate::watch::deepsleep::get_backup_data(LOCATION_BACKUP_REG)
    }

    /// Computes and caches EoT and TC. Call when d != last_calc_d.
    fn compute_daily(&mut self, d: u16) {
        // LSTM: movement_get_current_timezone_offset() returns seconds from UTC.
        let delta_t_utc = movement::get_current_timezone_offset() as f32 / 3600.0;
        let lstm = 15.0 * delta_t_utc;

        let loc = Self::load_location();
        let longitude = ((loc as i16) as f32) / 100.0;

        // B in radians for sinf/cosf.
        let b = (360.0 / 365.0) * (d as f32 - 81.0) * (core::f32::consts::PI / 180.0);

        self.eot = 9.87 * libm::sinf(2.0 * b) - 7.53 * libm::cosf(b) - 1.5 * libm::sinf(b);
        self.tc = 4.0 * (longitude - lstm) + self.eot;
        self.last_calc_d = d;
    }

    /// Recomputes if the day-of-year has rolled over. Returns the current d.
    fn maybe_recompute(&mut self, dt: DateTime) -> u16 {
        let d = crate::watch::utility::days_since_new_year(
            dt.year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR,
            dt.month,
            dt.day,
        );
        if d != self.last_calc_d && Self::load_location() != 0 {
            self.compute_daily(d);
        }
        d
    }

    /// LST as total seconds since midnight (0..86399).
    fn lst_seconds(dt: DateTime, tc: f32) -> i32 {
        let lt = dt.hour as i32 * 3600 + dt.minute as i32 * 60 + dt.second as i32;
        let tc = (tc * 60.0) as i32;
        ((lt + tc) % 86400 + 86400) % 86400
    }

    fn update_display(&mut self, dt: DateTime) {
        let mut bottom = [0u8; 9];

        if Self::load_location() == 0 {
            watch::slcd::display_string("SOL", 0);
            watch::slcd::display_string("  ", 3);
            watch::slcd::display_string("no Loc", 4);
            watch::slcd::clear_colon();
            return;
        }

        match self.mode {
            SolarTimeMode::Lst => {
                let s = Self::lst_seconds(dt, self.tc);
                watch::slcd::display_string("SOL", 0);
                watch::slcd::display_string("Ar", 3);
                bottom[0] = b'0' + ((s / 3600) / 10) as u8;
                bottom[1] = b'0' + ((s / 3600) % 10) as u8;
                bottom[2] = b'0' + (((s % 3600) / 60) / 10) as u8;
                bottom[3] = b'0' + (((s % 3600) / 60) % 10) as u8;
                bottom[4] = b'0' + ((s % 60) / 10) as u8;
                bottom[5] = b'0' + ((s % 60) % 10) as u8;
                watch::slcd::set_colon();
            }
            SolarTimeMode::Noon => {
                // Solar noon: moment when LST = 12:00 -> LT_noon = 12h - TC/60.
                let mut s = ((12.0 - self.tc / 60.0) * 3600.0) as i32;
                s = ((s % 86400) + 86400) % 86400;
                watch::slcd::display_string("NOO", 0);
                watch::slcd::display_string("n ", 3);
                bottom[0] = b'0' + ((s / 3600) / 10) as u8;
                bottom[1] = b'0' + ((s / 3600) % 10) as u8;
                bottom[2] = b'0' + (((s % 3600) / 60) / 10) as u8;
                bottom[3] = b'0' + (((s % 3600) / 60) % 10) as u8;
                watch::slcd::set_colon();
            }
            SolarTimeMode::Hra => {
                // HRA = 15 * (LST - 12); negative = morning, positive = afternoon.
                let s = Self::lst_seconds(dt, self.tc);
                let hra = libm::roundf(15.0 * (s as f32 / 3600.0 - 12.0)) as i16;
                watch::slcd::display_string("HrA", 0);
                watch::slcd::display_string("n ", 3);
                let mut i = 0;
                if hra < 0 {
                    bottom[i] = b'-';
                    i += 1;
                } else {
                    bottom[i] = b'+';
                    i += 1;
                }
                let abs = hra.unsigned_abs();
                bottom[i] = b'0' + ((abs / 100) % 10) as u8;
                bottom[i + 1] = b'0' + ((abs / 10) % 10) as u8;
                bottom[i + 2] = b'0' + (abs % 10) as u8;
                watch::slcd::clear_colon();
            }
        }

        watch::slcd::display_string(core::str::from_utf8(&bottom[..6]).unwrap_or(""), 4);
    }
}

impl WatchFace for SolarTimeFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        // Force recompute on activation: timezone or location may have changed.
        self.last_calc_d = 0;
        let dt = movement::get_local_date_time();
        self.maybe_recompute(dt);
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => {
                let dt = movement::get_local_date_time();
                self.maybe_recompute(dt);
                self.update_display(dt);
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.mode = match self.mode {
                    SolarTimeMode::Lst => SolarTimeMode::Noon,
                    SolarTimeMode::Noon => SolarTimeMode::Hra,
                    SolarTimeMode::Hra => SolarTimeMode::Lst,
                };
                let dt = movement::get_local_date_time();
                self.update_display(dt);
            }
            Event::BackgroundTask => {
                self.mode = SolarTimeMode::Lst;
                if Self::load_location() == 0 {
                    movement::move_to_face(0);
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.mode = SolarTimeMode::Lst;
        watch::slcd::clear_colon();
    }
}
