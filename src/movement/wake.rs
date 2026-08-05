//! Wake alarm watch face.
//!
//! Port of the C `wake_face.c`. A simple daily wake alarm. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;

/// The wake face state.
pub struct WakeFace {
    hour: u8,
    minute: u8,
    mode: u8,
}

impl WakeFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        WakeFace {
            hour: 5,
            minute: 0,
            mode: 0,
        }
    }

    pub fn new() -> Self {
        WakeFace::new_static()
    }

    fn update_display(&self, settings: &Settings) {
        let mut hour = self.hour;
        watch::slcd::clear_display();
        let mut set_leading_zero = false;
        if !settings.clock_mode_24h() {
            if hour >= 12 {
                watch::slcd::set_indicator(Indicator::Pm);
            }
            hour = if hour % 12 != 0 { hour % 12 } else { 12 };
        } else if !settings.clock_24h_leading_zero() {
            watch::slcd::set_indicator(Indicator::H24);
        } else if hour < 10 {
            set_leading_zero = true;
        }

        if self.mode != 0 {
            watch::slcd::set_indicator(Indicator::Bell);
        }

        let mut buf = [0u8; 11];
        buf[0] = b'W';
        buf[1] = b'A';
        buf[2] = b' ';
        buf[3] = b' ';
        buf[4] = b'0' + hour / 10;
        buf[5] = b'0' + hour % 10;
        buf[6] = b'0' + self.minute / 10;
        buf[7] = b'0' + self.minute % 10;
        buf[8] = b' ';
        buf[9] = b' ';

        watch::slcd::set_colon();
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        if set_leading_zero {
            watch::slcd::display_string("0", 4);
        }
    }
}

impl WatchFace for WakeFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => self.update_display(settings),
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.hour = (self.hour + 1) % 24;
                self.update_display(settings);
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.hour = (self.hour + 6) % 24;
                self.update_display(settings);
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.minute = (self.minute + 10) % 60;
                self.update_display(settings);
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.mode ^= 1;
                self.update_display(settings);
            }
            Event::BackgroundTask => {
                movement::play_alarm();
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        if self.mode == 0 {
            return false;
        }
        let now = rtc::get_date_time();
        self.hour == now.hour && self.minute == now.minute
    }
}
