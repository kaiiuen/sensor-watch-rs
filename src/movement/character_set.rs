//! Character set watch face.
//!
//! Port of the C `character_set_face.c`. Cycles through the LCD character set.
//! It is a pure state machine: it reacts to a single event and returns; it
//! never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;

/// The character set face state.
pub struct CharacterSetFace {
    c: u8,
}

impl CharacterSetFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        CharacterSetFace { c: b'@' }
    }

    pub fn new() -> Self {
        CharacterSetFace::new_static()
    }
}

impl WatchFace for CharacterSetFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.c = b'@';
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.c += 1;
                if self.c & 0x80 != 0 {
                    self.c = b' ';
                }
                let mut buf = [0u8; 11];
                for i in 0..10 {
                    buf[i] = self.c;
                }
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            Event::Activate => {
                let mut buf = [0u8; 11];
                for i in 0..10 {
                    buf[i] = self.c;
                }
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
