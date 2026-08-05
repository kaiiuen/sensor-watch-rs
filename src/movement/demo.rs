//! Demo watch face.
//!
//! Port of the C `demo_face.c`. Shows sample screens for various watch faces.
//! It is a pure state machine: it reacts to a single event and returns; it
//! never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::slcd::Indicator;

const DEMO_FACE_NUM_FACES: u8 = 12;

/// The demo face state.
pub struct DemoFace {
    screen: u8,
}

impl DemoFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        DemoFace { screen: 0 }
    }

    pub fn new() -> Self {
        DemoFace::new_static()
    }

    fn draw(&self) {
        match self.screen {
            0 => {
                watch::slcd::display_string("TH10101036", 0);
                watch::slcd::set_colon();
            }
            1 => {
                watch::slcd::display_string("UT10 21036", 0);
                watch::slcd::set_indicator(Indicator::Pm);
            }
            2 => {
                watch::slcd::display_string("bt   64125", 0);
                watch::slcd::clear_indicator(Indicator::Pm);
                watch::slcd::clear_colon();
            }
            3 => watch::slcd::display_string("2F29808494", 0),
            4 => watch::slcd::display_string("TE  72.1#F", 0),
            5 => watch::slcd::display_string("TE  22.3#C", 0),
            6 => watch::slcd::display_string("TL  43.6#F", 0),
            7 => {
                watch::slcd::display_string("AT 6100000", 0);
                watch::slcd::set_colon();
            }
            8 => {
                watch::slcd::clear_colon();
                watch::slcd::display_string("DA   12879", 0);
            }
            9 => {
                watch::slcd::display_string("ST 01042  ", 0);
                watch::slcd::set_colon();
            }
            10 => {
                watch::slcd::display_string("    68 bpn", 0);
                watch::slcd::clear_colon();
            }
            11 => watch::slcd::display_string("BA  2.97 V", 0),
            _ => {}
        }
    }
}

impl WatchFace for DemoFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.screen = (self.screen + 1) % DEMO_FACE_NUM_FACES;
                self.draw();
            }
            Event::Activate => self.draw(),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
