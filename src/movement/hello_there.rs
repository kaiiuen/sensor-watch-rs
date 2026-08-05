//! Hello there watch face.
//!
//! Port of the C `hello_there_face.c`. A simple demo that animates "Hello
//! there". It is a pure state machine: it reacts to a single event and
//! returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;

/// The hello there face state.
pub struct HelloThereFace {
    current_word: u8,
    animating: bool,
}

impl HelloThereFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        HelloThereFace {
            current_word: 0,
            animating: true,
        }
    }

    pub fn new() -> Self {
        HelloThereFace::new_static()
    }
}

impl WatchFace for HelloThereFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.current_word = 0;
        self.animating = true;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => {
                if self.animating {
                    if self.current_word == 0 {
                        watch::slcd::display_string("Hello ", 4);
                    } else {
                        watch::slcd::display_string(" there", 4);
                    }
                    self.current_word = (self.current_word + 1) % 2;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.animating = !self.animating;
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
