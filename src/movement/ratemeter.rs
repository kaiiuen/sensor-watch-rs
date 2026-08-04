//! Ratemeter watch face.
//!
//! Port of the C `ratemeter_face.c`. Measures a rate (events per minute) by
//! timing button presses. It is a pure state machine: it reacts to a single
//! event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;

const RATEMETER_FACE_FREQUENCY_FACTOR: u32 = 4;
const RATEMETER_FACE_FREQUENCY: u32 = 1 << RATEMETER_FACE_FREQUENCY_FACTOR;

/// The ratemeter face state.
pub struct RatemeterFace {
    ticks: u32,
    rate: i16,
}

impl RatemeterFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        RatemeterFace { ticks: 0, rate: 0 }
    }

    pub fn new() -> Self {
        RatemeterFace::new_static()
    }
}

impl WatchFace for RatemeterFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.ticks = 0;
        self.rate = 0;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => watch::slcd::display_string("ra          ", 0),
            Event::Button(Button::Alarm, ButtonEvent::Down) => {
                if self.ticks != 0 {
                    self.rate =
                        (60.0 / (self.ticks as f32 / RATEMETER_FACE_FREQUENCY as f32)) as i16;
                }
                self.ticks = 0;
            }
            Event::Tick => {
                if self.rate == 0 {
                    watch::slcd::display_string("ra          ", 0);
                } else if self.rate > 500 {
                    watch::slcd::display_string("ra      Hi", 0);
                } else if self.rate < 1 {
                    watch::slcd::display_string("ra      Lo", 0);
                } else {
                    let mut buf = [0u8; 11];
                    buf[0] = b'r';
                    buf[1] = b'a';
                    buf[2] = b' ';
                    buf[3] = b' ';
                    let v = self.rate;
                    buf[4] = b'0' + (v / 100) as u8;
                    buf[5] = b'0' + ((v / 10) % 10) as u8;
                    buf[6] = b'0' + (v % 10) as u8;
                    buf[7] = b' ';
                    buf[8] = b'p';
                    buf[9] = b'n';
                    watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
                }
                self.ticks += 1;
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
