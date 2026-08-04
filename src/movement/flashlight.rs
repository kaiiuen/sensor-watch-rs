//! Flashlight watch face.
//!
//! Port of the C `flashlight_face.c`. Uses pin A2 as a digital output to drive
//! an external LED (or other load) on the 9-pin connector. The Light button
//! toggles it on/off.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::gpio::{self, Direction, Pin};

/// Pin A2 (PB02) used as the flashlight output.
const A2: Pin = Pin(1, 2);

/// The flashlight face state.
pub struct FlashlightFace;

impl FlashlightFace {
    pub const fn new_static() -> Self {
        FlashlightFace
    }
}

impl WatchFace for FlashlightFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        // Configure A2 as an output, initially off.
        gpio::set_pin_direction(A2, Direction::Out);
        gpio::set_pin_level(A2, false);
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => {
                watch::slcd::display_string("FL", 0);
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                // Toggle the flashlight output.
                let on = gpio::get_pin_level(A2);
                gpio::set_pin_level(A2, !on);
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        // Turn off and release the pin.
        gpio::set_pin_level(A2, false);
        gpio::set_pin_direction(A2, Direction::Off);
    }
}
