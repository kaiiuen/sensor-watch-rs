//! Tide computation watch face.
//!
//! Port of the C `tide_face.c`. Computes the time of the next high and low
//! tides and gives an approximation of the current tide. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, ClockMode, Event, Settings, WatchFace};
use crate::watch::slcd;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const LUNAR_DAYS: f64 = 29.530_587_705_76;
const FIRST_MOON: f64 = 947_182_440.0; // Saturday, 6 January 2000 18:14:00 in unix epoch time
const SEMI_DIURNAL_TIDAL_PERIOD: u32 = (LUNAR_DAYS / (LUNAR_DAYS - 1.0) * 12.0 * 3600.0) as u32; // 12h25m in seconds

/// The tide amplitude.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TideAmplitude {
    Spring,
    Neap,
    Medium,
}

/// The tide screen mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TideMode {
    Empty,
    Current,
    Future,
    SettingHour,
    SettingMin,
}

/// The tide type.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TideType {
    High,
    Low,
}

/// The tide face state.
pub struct TideFace {
    mode: TideMode,
    start_setting: bool,
    next_high_tide: u32,
    last_current_update_time: u32,
    future_tide_time: u32,
    future_tide_type: TideType,
}

impl TideFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        TideFace {
            mode: TideMode::Empty,
            start_setting: false,
            next_high_tide: 0,
            last_current_update_time: 0,
            future_tide_time: 0,
            future_tide_type: TideType::High,
        }
    }

    pub fn new() -> Self {
        TideFace::new_static()
    }

    fn get_tide_amplitude(time: u32) -> TideAmplitude {
        // Moon age in days, looped over between new and full moon (so age is
        // 14.7 days at most).
        let moon_age = libm::fmod((time as f64 - FIRST_MOON) / 86400.0, LUNAR_DAYS / 2.0);
        if moon_age <= LUNAR_DAYS / 16.0 || moon_age >= LUNAR_DAYS * 7.0 / 16.0 {
            TideAmplitude::Spring
        } else if moon_age > LUNAR_DAYS * 3.0 / 16.0 && moon_age < LUNAR_DAYS * 5.0 / 16.0 {
            TideAmplitude::Neap
        } else {
            TideAmplitude::Medium
        }
    }

    fn get_current_unix_time() -> u32 {
        utility::date_time_to_unix_time(movement::get_utc_date_time(), 0)
    }

    fn move_next_high_tide(&mut self, now: u32) {
        while self.next_high_tide > now + SEMI_DIURNAL_TIDAL_PERIOD {
            self.next_high_tide -= SEMI_DIURNAL_TIDAL_PERIOD;
        }
        while self.next_high_tide < now {
            self.next_high_tide += SEMI_DIURNAL_TIDAL_PERIOD;
        }
    }

    fn draw_tide_amplitude(&self, time: u32) {
        // Classic LCD: position 9 (bottom-right character) horizontal bars.
        // 9A = top (2,4), 9G = mid (1,5), 9D = bottom (0,6).
        match Self::get_tide_amplitude(time) {
            TideAmplitude::Spring => {
                slcd::set_pixel(2, 4); // top horizontal bar
                slcd::set_pixel(1, 5); // mid horizontal bar
                slcd::set_pixel(0, 6); // bottom horizontal bar
            }
            TideAmplitude::Medium => {
                slcd::set_pixel(1, 5); // mid horizontal bar
                slcd::set_pixel(0, 6); // bottom horizontal bar
            }
            TideAmplitude::Neap => {
                slcd::set_pixel(0, 6); // bottom horizontal bar
            }
        }
    }

    fn draw_day_and_time(&self, time: u32, show_day: bool, show_hour: bool, show_minute: bool) {
        let mut date_time = utility::date_time_from_unix_time(
            time,
            (movement::get_current_timezone_offset() * 60) as u32,
        );
        let mut pm = false;
        if movement::clock_mode_24h() == ClockMode::H12 {
            pm = utility::convert_to_12_hour(&mut date_time);
        } else {
            slcd::set_indicator(Indicator::H24);
        }
        if pm {
            slcd::set_indicator(Indicator::Pm);
        }

        if show_hour {
            let mut buf = [b' '; 2];
            buf[0] = b'0' + date_time.hour / 10;
            buf[1] = b'0' + date_time.hour % 10;
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
        }
        if show_minute {
            let mut buf = [b'0'; 2];
            buf[0] = b'0' + date_time.minute / 10;
            buf[1] = b'0' + date_time.minute % 10;
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 6);
        }
        if show_day {
            let mut buf = [b' '; 2];
            buf[0] = b'0' + date_time.day / 10;
            buf[1] = b'0' + date_time.day % 10;
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 2);
        }

        slcd::set_colon();
    }

    fn draw(&mut self, now: u32, subsecond: u8) {
        slcd::clear_display();
        match self.mode {
            TideMode::Empty => {
                slcd::display_string("TI", 0);
                slcd::display_string("----", 4);
            }
            TideMode::Current => {
                let tide_age = self.next_high_tide as i64 - now as i64;
                self.draw_tide_amplitude(now);
                let tide_percent = (libm::cos(
                    tide_age as f64 / SEMI_DIURNAL_TIDAL_PERIOD as f64
                        * core::f64::consts::PI
                        * 2.0,
                ) + 1.0)
                    * 50.0;
                if tide_percent < 5.0 {
                    slcd::display_string("LO", 0);
                } else if tide_percent > 95.0 {
                    slcd::display_string("HI", 0);
                } else {
                    if self.next_high_tide - now < SEMI_DIURNAL_TIDAL_PERIOD / 2 {
                        slcd::display_string("FL", 0);
                    } else {
                        slcd::display_string("EB", 0);
                    }
                    let tide_upercent = tide_percent as u8;
                    let mut hour = [b' '; 2];
                    let mut minute = [b' '; 2];
                    hour[1] = b'0' + tide_upercent / 10;
                    minute[0] = b'0' + tide_upercent % 10;
                    // We use the second hour digit for our first digit, as it's
                    // more capable than the first hour or minute digits.
                    slcd::display_string(core::str::from_utf8(&hour[..]).unwrap_or(""), 4);
                    slcd::display_string(core::str::from_utf8(&minute[..]).unwrap_or(""), 6);
                }
            }
            TideMode::Future => {
                if self.future_tide_type == TideType::Low {
                    slcd::display_string("LO", 0);
                } else {
                    slcd::display_string("HI", 0);
                }
                self.draw_day_and_time(self.future_tide_time, true, true, true);
                self.draw_tide_amplitude(self.future_tide_time);
            }
            TideMode::SettingHour | TideMode::SettingMin => {
                slcd::display_string("HI", 0);
                self.draw_day_and_time(
                    self.next_high_tide,
                    !self.start_setting,
                    self.mode != TideMode::SettingHour || subsecond % 2 == 1,
                    self.mode != TideMode::SettingMin || subsecond % 2 == 1,
                );
            }
        }
    }

    fn offset_next_high_tide(&mut self, offset: i32) {
        self.next_high_tide = (self.next_high_tide as i64 + offset as i64) as u32;
        if !self.next_high_tide.is_multiple_of(60) {
            self.next_high_tide -= self.next_high_tide % 60;
        }
        self.start_setting = false;
    }
}

impl WatchFace for TideFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {
        self.mode = TideMode::Empty;
    }

    fn activate(&mut self, _settings: &Settings) {
        if self.mode != TideMode::Empty {
            self.mode = TideMode::Current;
        }
        let now = Self::get_current_unix_time();
        if (now as i64 - self.next_high_tide as i64).abs() > 60 * 86400 {
            // We revert to the empty mode if the next high tide is more than 2
            // months from now, to avoid accumulating too much errors.
            self.mode = TideMode::Empty;
            return;
        }
        self.move_next_high_tide(now);
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        let now = Self::get_current_unix_time();
        match event {
            Event::Activate => {
                self.draw(now, 0);
                if self.mode == TideMode::Current {
                    self.last_current_update_time = now;
                }
            }
            Event::Tick => match self.mode {
                TideMode::Current => {
                    if now - self.last_current_update_time >= 60 {
                        self.move_next_high_tide(now);
                        self.draw(now, 0);
                        self.last_current_update_time = now;
                    }
                }
                TideMode::SettingHour | TideMode::SettingMin => self.draw(now, 0),
                _ => {}
            },
            Event::Button(Button::Light, ButtonEvent::Down) => match self.mode {
                TideMode::SettingHour => {
                    self.mode = TideMode::SettingMin;
                    self.draw(now, 0);
                }
                TideMode::SettingMin => {
                    self.mode = TideMode::Current;
                    self.move_next_high_tide(Self::get_current_unix_time());
                    movement::request_tick_frequency(1);
                    self.draw(now, 0);
                }
                _ => movement::illuminate_led(),
            },
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.mode == TideMode::Future {
                    self.mode = TideMode::Current;
                    self.draw(now, 0);
                    self.last_current_update_time = now;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Down) => match self.mode {
                TideMode::SettingHour => self.offset_next_high_tide(3600),
                TideMode::SettingMin => self.offset_next_high_tide(60),
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::Up) => match self.mode {
                TideMode::Current => {
                    if self.next_high_tide - now > SEMI_DIURNAL_TIDAL_PERIOD / 2 {
                        self.future_tide_time = self.next_high_tide - SEMI_DIURNAL_TIDAL_PERIOD / 2;
                        self.future_tide_type = TideType::Low;
                    } else {
                        self.future_tide_time = self.next_high_tide;
                        self.future_tide_type = TideType::High;
                    }
                    self.mode = TideMode::Future;
                }
                TideMode::Future => {
                    self.future_tide_time += SEMI_DIURNAL_TIDAL_PERIOD / 2;
                    self.future_tide_type = if self.future_tide_type == TideType::Low {
                        TideType::High
                    } else {
                        TideType::Low
                    };
                }
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => match self.mode {
                TideMode::Empty => {
                    self.next_high_tide = Self::get_current_unix_time();
                    self.mode = TideMode::SettingHour;
                    self.start_setting = true;
                    movement::request_tick_frequency(4);
                }
                TideMode::Current | TideMode::Future => {
                    self.mode = TideMode::SettingHour;
                    self.start_setting = true;
                    movement::request_tick_frequency(4);
                }
                _ => {}
            },
            Event::Button(Button::Mode, ButtonEvent::Down) => match self.mode {
                TideMode::SettingHour => self.offset_next_high_tide(-3600),
                TideMode::SettingMin => self.offset_next_high_tide(-60),
                _ => movement::default_loop_handler(event, settings),
            },
            Event::Button(Button::Mode, ButtonEvent::Up)
            | Event::Button(Button::Mode, ButtonEvent::LongPress) => match self.mode {
                TideMode::SettingHour | TideMode::SettingMin => {}
                _ => movement::default_loop_handler(event, settings),
            },
            Event::BackgroundTask => {
                if self.mode == TideMode::SettingMin || self.mode == TideMode::SettingHour {
                    self.mode = TideMode::Current;
                    self.draw(now, 0);
                }
                movement::default_loop_handler(event, settings);
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        if self.mode == TideMode::SettingHour || self.mode == TideMode::SettingMin {
            self.move_next_high_tide(Self::get_current_unix_time());
        }
    }
}
