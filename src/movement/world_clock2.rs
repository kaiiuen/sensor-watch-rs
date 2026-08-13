//! World clock 2 watch face.
//!
//! Port of the C `world_clock2_face.c`. Shows the time in a selectable time
//! zone with a settings mode to pick zones. It is a pure state machine: it
//! reacts to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const NUM_TIME_ZONES: u8 = 41;
const WORLD_CLOCK2_MODE_DISPLAY: u8 = 0;
const WORLD_CLOCK2_MODE_SETTINGS: u8 = 1;

const ZONE_NAMES: [&str; 41] = [
    "UTC", "CET", "SAST", "ARST", "IRST", "GET", "AFT", "PKT", "IST", "NPT", "KGT", "MYST", "THA",
    "CST", "ACWS", "JST", "ACST", "AEST", "LHST", "SBT", "NZST", "CHAS", "TOT", "CHAD", "LINT",
    "BIT", "NUT", "HST", "MART", "AKST", "PST", "MST", "CST", "EST", "VET", "AST", "NST", "BRT",
    "NDT", "FNT", "AZOT",
];

/// The world clock 2 face state.
pub struct WorldClock2Face {
    current_mode: u8,
    current_zone: u8,
    zones_selected: [bool; 41],
    previous_date_time: u32,
    refresh_face: bool,
}

impl WorldClock2Face {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        WorldClock2Face {
            current_mode: WORLD_CLOCK2_MODE_SETTINGS,
            current_zone: 0,
            zones_selected: [false; 41],
            previous_date_time: 0xFFFF_FFFF,
            refresh_face: true,
        }
    }

    pub fn new() -> Self {
        WorldClock2Face::new_static()
    }

    fn mod_i(a: i32, b: i32) -> i32 {
        let r = a % b;
        if r < 0 { r + b } else { r }
    }

    fn find_selected_zone(&self, direction: i32) -> u8 {
        let mut i = self.current_zone as i32;
        loop {
            i = Self::mod_i(i + direction, NUM_TIME_ZONES as i32);
            if i == self.current_zone as i32 {
                return 0;
            }
            if self.zones_selected[i as usize] {
                return i as u8;
            }
        }
    }

    fn mode_display(&mut self, event: Event, settings: &Settings) {
        let mut buf = [0u8; 11];
        match event {
            Event::Activate | Event::Tick => {
                if self.refresh_face {
                    watch::slcd::clear_indicator(Indicator::Signal);
                    watch::slcd::set_colon();
                    if settings.clock_mode_24h() && !settings.clock_24h_leading_zero() {
                        watch::slcd::set_indicator(Indicator::H24);
                    }
                    self.previous_date_time = 0xFFFF_FFFF;
                    self.refresh_face = false;
                }
                let date_time = rtc::get_date_time();
                let tz = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)]
                    as i32
                    * 60) as u32;
                let zone_off = (movement::TIMEZONE_OFFSETS[(self.current_zone as usize).min(40)]
                    as i32
                    * 60) as u32;
                let timestamp = utility::date_time_to_unix_time(date_time, tz);
                let dt = utility::date_time_from_unix_time(timestamp, zone_off);
                let previous = self.previous_date_time;
                self.previous_date_time = dt.to_reg();
                let mut set_leading_zero = false;
                let pos;
                if (dt.to_reg() >> 6) == (previous >> 6) {
                    pos = 8;
                    buf[0] = b'0' + dt.second / 10;
                    buf[1] = b'0' + dt.second % 10;
                } else if (dt.to_reg() >> 12) == (previous >> 12) {
                    pos = 6;
                    buf[0] = b'0' + dt.minute / 10;
                    buf[1] = b'0' + dt.minute % 10;
                    buf[2] = b'0' + dt.second / 10;
                    buf[3] = b'0' + dt.second % 10;
                } else {
                    let mut hour = dt.hour;
                    if !settings.clock_mode_24h() {
                        if hour < 12 {
                            watch::slcd::clear_indicator(Indicator::Pm);
                        } else {
                            watch::slcd::set_indicator(Indicator::Pm);
                        }
                        hour %= 12;
                        if hour == 0 {
                            hour = 12;
                        }
                    } else if settings.clock_24h_leading_zero() && hour < 10 {
                        set_leading_zero = true;
                    }
                    pos = 0;
                    let name = ZONE_NAMES[self.current_zone as usize].as_bytes();
                    buf[0] = name[0];
                    buf[1] = name[1];
                    buf[2] = b'0' + dt.day / 10;
                    buf[3] = b'0' + dt.day % 10;
                    buf[4] = b'0' + hour / 10;
                    buf[5] = b'0' + hour % 10;
                    buf[6] = b'0' + dt.minute / 10;
                    buf[7] = b'0' + dt.minute % 10;
                    buf[8] = b'0' + dt.second / 10;
                    buf[9] = b'0' + dt.second % 10;
                }
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), pos);
                if set_leading_zero {
                    watch::slcd::display_string("0", 4);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.current_zone = self.find_selected_zone(1);
                self.previous_date_time = 0xFFFF_FFFF;
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.current_zone = self.find_selected_zone(-1);
                self.previous_date_time = 0xFFFF_FFFF;
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => movement::illuminate_led(),
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.current_mode = WORLD_CLOCK2_MODE_SETTINGS;
                self.refresh_face = true;
                if settings.button_should_sound() {
                    crate::movement::play_alarm_beeps(1, Note::C8);
                }
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => movement::move_to_next_face(),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn mode_settings(&mut self, event: Event, settings: &Settings) {
        let mut buf = [0u8; 11];
        match event {
            Event::Activate | Event::Tick => {
                if self.refresh_face {
                    watch::slcd::clear_colon();
                    watch::slcd::clear_indicator(Indicator::H24);
                    watch::slcd::clear_indicator(Indicator::Pm);
                    self.refresh_face = false;
                }
                let offset = movement::TIMEZONE_OFFSETS[(self.current_zone as usize).min(40)];
                let hours = offset / 60;
                let minutes = offset % 60;
                let name = ZONE_NAMES[self.current_zone as usize].as_bytes();
                buf[0] = name[0];
                buf[1] = name[1];
                buf[2] = b'0' + (self.current_zone / 10) % 10;
                buf[3] = b'0' + self.current_zone % 10;
                buf[4] = b' ';
                buf[5] = if hours < 0 { b'-' } else { b'+' };
                buf[6] = b'0' + ((hours.unsigned_abs() % 24) / 10) as u8;
                buf[7] = b'0' + ((hours.unsigned_abs() % 24) % 10) as u8;
                buf[8] = b'0' + ((minutes.unsigned_abs() % 60) / 10) as u8;
                buf[9] = b'0' + ((minutes.unsigned_abs() % 60) % 10) as u8;
                if self.zones_selected[self.current_zone as usize] {
                    watch::slcd::set_indicator(Indicator::Signal);
                } else {
                    watch::slcd::clear_indicator(Indicator::Signal);
                }
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.current_zone =
                    Self::mod_i(self.current_zone as i32 + 1, NUM_TIME_ZONES as i32) as u8;
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.current_zone =
                    Self::mod_i(self.current_zone as i32 - 1, NUM_TIME_ZONES as i32) as u8;
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if !self.zones_selected[self.current_zone as usize] {
                    self.current_zone = self.find_selected_zone(1);
                }
                self.current_mode = WORLD_CLOCK2_MODE_DISPLAY;
                self.refresh_face = true;
                if settings.button_should_sound() {
                    crate::movement::play_alarm_beeps(1, Note::C8);
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                let zone = self.current_zone as usize;
                self.zones_selected[zone] = !self.zones_selected[zone];
                if settings.button_should_sound() {
                    crate::movement::play_alarm_beeps(
                        1,
                        if self.zones_selected[zone] {
                            Note::G7
                        } else {
                            Note::C8
                        },
                    );
                }
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => movement::move_to_next_face(),
            _ => movement::default_loop_handler(event, settings),
        }
    }
}

impl WatchFace for WorldClock2Face {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        if watch::slcd::tick_animation_is_running() {
            watch::slcd::stop_tick_animation();
        }
        self.refresh_face = true;
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        if self.current_mode == WORLD_CLOCK2_MODE_DISPLAY {
            self.mode_display(event, settings);
        } else {
            self.mode_settings(event, settings);
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
