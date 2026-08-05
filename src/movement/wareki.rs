//! Wareki (Japanese era) watch face.
//!
//! Port of the C `wareki_face.c`. Shows the current year in both the Western
//! calendar and the Japanese era (Heisei/Reiwa). It is a pure state machine:
//! it reacts to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;

const HEISEI_GANNEN: u16 = 1989;
const REIWA_GANNEN: u16 = 2019;
const REIWA_LIMIT: u16 = 2099;

/// The wareki face state.
pub struct WarekiFace {
    disp_year: u16,
    real_year: u16,
    start_year: u16,
    alarm_button_press: bool,
    light_button_press: bool,
}

impl WarekiFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        WarekiFace {
            disp_year: 2020,
            real_year: 2020,
            start_year: 2020,
            alarm_button_press: false,
            light_button_press: false,
        }
    }

    pub fn new() -> Self {
        WarekiFace::new_static()
    }

    fn draw_splash(&self) {
        watch::slcd::clear_colon();
        watch::slcd::display_string("wa  ------", 0);
    }

    fn draw_year_and_wareki(&self) {
        let mut buf = [0u8; 11];
        if self.disp_year < REIWA_GANNEN {
            buf[0] = b' ';
            buf[1] = b'h';
            buf[2] = b'0' + ((self.disp_year - HEISEI_GANNEN + 1) / 10) as u8;
            buf[3] = b'0' + ((self.disp_year - HEISEI_GANNEN + 1) % 10) as u8;
            buf[4] = b' ';
            buf[5] = b'0' + (self.disp_year / 1000) as u8;
            buf[6] = b'0' + ((self.disp_year / 100) % 10) as u8;
            buf[7] = b'0' + ((self.disp_year / 10) % 10) as u8;
            buf[8] = b'0' + (self.disp_year % 10) as u8;
            buf[9] = b' ';
        } else {
            buf[0] = b' ';
            buf[1] = b'r';
            buf[2] = b'0' + ((self.disp_year - REIWA_GANNEN + 1) / 10) as u8;
            buf[3] = b'0' + ((self.disp_year - REIWA_GANNEN + 1) % 10) as u8;
            buf[4] = b' ';
            buf[5] = b'0' + (self.disp_year / 1000) as u8;
            buf[6] = b'0' + ((self.disp_year / 100) % 10) as u8;
            buf[7] = b'0' + ((self.disp_year / 10) % 10) as u8;
            buf[8] = b'0' + (self.disp_year % 10) as u8;
            buf[9] = b' ';
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn add_year(&mut self, count: u16) {
        self.disp_year += count;
        if self.disp_year > REIWA_LIMIT {
            self.disp_year = REIWA_LIMIT;
        }
    }

    fn sub_year(&mut self, count: u16) {
        self.disp_year = self.disp_year.saturating_sub(count);
        if self.disp_year < 1989 {
            self.disp_year = 1989;
        }
    }
}

impl WatchFace for WarekiFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        if watch::slcd::tick_animation_is_running() {
            watch::slcd::stop_tick_animation();
        }
        self.real_year = rtc::get_date_time().year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR;
        self.start_year = self.real_year;
        self.disp_year = self.real_year;
        self.alarm_button_press = false;
        self.light_button_press = false;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        self.real_year = rtc::get_date_time().year as u16 + rtc::WATCH_RTC_REFERENCE_YEAR;
        if self.real_year != self.start_year {
            self.start_year = self.real_year;
            self.disp_year = self.real_year;
        }

        match event {
            Event::Activate => self.draw_splash(),
            Event::Button(Button::Mode, ButtonEvent::Up) => movement::move_to_next_face(),
            Event::Tick => {
                if self.alarm_button_press && !watch::gpio::get_pin_level(watch::extint::BTN_ALARM)
                {
                    self.alarm_button_press = false;
                }
                if self.light_button_press && !watch::gpio::get_pin_level(watch::extint::BTN_LIGHT)
                {
                    self.light_button_press = false;
                }
                if self.alarm_button_press {
                    self.add_year(1);
                }
                if self.light_button_press {
                    self.sub_year(1);
                }
                self.draw_year_and_wareki();
            }
            Event::Button(Button::Light, ButtonEvent::Down) => self.sub_year(1),
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.light_button_press = true;
            }
            Event::Button(Button::Light, ButtonEvent::LongUp) => {
                self.light_button_press = false;
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.light_button_press = false;
            }
            Event::Button(Button::Alarm, ButtonEvent::Down) => self.add_year(1),
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.alarm_button_press = true;
            }
            Event::Button(Button::Alarm, ButtonEvent::LongUp) => {
                self.alarm_button_press = false;
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.alarm_button_press = false;
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
