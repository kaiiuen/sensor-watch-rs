//! Ship's bell watch face.
//!
//! Port of the C `ships_bell_face.c`. Shows the time in ship's-bell format and
//! optionally rings the bell on the half hour. It is a pure state machine: it
//! reacts to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;

/// The ships bell face state.
pub struct ShipsBellFace {
    bell_enabled: bool,
    on_watch: u8,
}

impl ShipsBellFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        ShipsBellFace {
            bell_enabled: false,
            on_watch: 0,
        }
    }

    pub fn new() -> Self {
        ShipsBellFace::new_static()
    }

    fn draw(&self) {
        let mut buf = [0u8; 8];
        if self.on_watch != 0 {
            buf[0] = b'0' + self.on_watch;
        } else {
            buf[0] = b' ';
        }
        let date_time = rtc::get_date_time();
        let hour = date_time.hour % 4;
        buf[1] = b' ';
        buf[2] = b'0' + hour;
        buf[3] = b'0' + date_time.minute / 10;
        buf[4] = b'0' + date_time.minute % 10;
        buf[5] = b'0' + date_time.second / 10;
        buf[6] = b'0' + date_time.second % 10;
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 3);
    }

    fn ring(&self) {
        let date_time = rtc::get_date_time();
        let mut hour = date_time.hour % 4;
        if hour == 0 && date_time.minute < 30 {
            hour = 4;
        }
        for _ in 0..hour {
            crate::movement::play_alarm_beeps(1, Note::C8);
        }
        if date_time.minute >= 30 {
            crate::movement::play_alarm_beeps(1, Note::C8);
        }
    }
}

impl WatchFace for ShipsBellFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        if self.bell_enabled {
            watch::slcd::set_indicator(Indicator::Bell);
        } else {
            watch::slcd::clear_indicator(Indicator::Bell);
        }
        watch::slcd::display_string("SB", 0);
        watch::slcd::set_colon();
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => self.draw(),
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.bell_enabled = !self.bell_enabled;
                if self.bell_enabled {
                    watch::slcd::set_indicator(Indicator::Bell);
                } else {
                    watch::slcd::clear_indicator(Indicator::Bell);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.on_watch = (self.on_watch + 1) % 4;
                self.draw();
            }
            Event::BackgroundTask => {
                self.ring();
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        if !self.bell_enabled {
            return false;
        }
        let date_time = rtc::get_date_time();
        if !(date_time.minute == 0 || date_time.minute == 30) {
            return false;
        }
        let hour = date_time.hour % 12;
        match self.on_watch {
            1 => (4..8).contains(&hour) || (hour == 8 && date_time.minute == 0),
            2 => (8..12).contains(&hour) || (hour == 0 && date_time.minute == 0),
            3 => (0..4).contains(&hour) || (hour == 4 && date_time.minute == 0),
            _ => true,
        }
    }
}
