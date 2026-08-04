//! Blinky watch face.
//!
//! Port of the C `blinky_face.c`. A simple LED blinker for testing the
//! bi-color LED. It is a pure state machine: it reacts to a single event and
//! returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;

/// The state for the blinky face.
pub struct BlinkyFace {
    active: bool,
    fast: bool,
    color: u8,
    led_on: bool,
}

impl BlinkyFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        BlinkyFace {
            active: false,
            fast: false,
            color: 0,
            led_on: false,
        }
    }

    pub fn new() -> Self {
        BlinkyFace::new_static()
    }

    fn update_lcd(&self) {
        let mut buf = [0u8; 11];
        let color = match self.color {
            0 => " red  ",
            1 => " Green",
            _ => " Yello",
        };
        let c = color.as_bytes();
        buf[0] = b'B';
        buf[1] = b'L';
        buf[2] = b' ';
        buf[3] = if self.fast { b'F' } else { b'S' };
        buf[4] = c[0];
        buf[5] = c[1];
        buf[6] = c[2];
        buf[7] = c[3];
        buf[8] = c[4];
        buf[9] = c[5];
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn set_led(&self) {
        match self.color {
            0 => watch::led::set_led_red(),
            1 => watch::led::set_led_green(),
            _ => watch::led::set_led_yellow(),
        }
    }
}

impl WatchFace for BlinkyFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.active = false;
        self.led_on = false;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => self.update_lcd(),
            Event::Button(Button::Light, ButtonEvent::Down) => {
                if !self.active {
                    self.color = (self.color + 1) % 3;
                    self.update_lcd();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if !self.active {
                    self.active = true;
                    watch::slcd::clear_display();
                    self.led_on = false;
                } else {
                    self.active = false;
                    watch::led::set_led_off();
                    self.led_on = false;
                    self.update_lcd();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if !self.active {
                    self.fast = !self.fast;
                    self.update_lcd();
                }
            }
            Event::Tick => {
                if self.active {
                    // Toggle the LED on each wake (1 Hz blink).
                    self.led_on = !self.led_on;
                    if self.led_on {
                        self.set_led();
                    } else {
                        watch::led::set_led_off();
                    }
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        watch::led::set_led_off();
        self.led_on = false;
    }
}
