//! Counter watch face.
//!
//! Port of the C `counter_face.c`. A simple tally counter (0-99) with an
//! optional beep on each count.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::slcd::Indicator;

/// The counter face state.
pub struct CounterFace {
    counter_idx: u8,
    beep_on: bool,
}

impl CounterFace {
    pub const fn new_static() -> Self {
        CounterFace {
            counter_idx: 0,
            beep_on: true,
        }
    }

    fn print_counter(&self) {
        let mut buf = [0u8; 11];
        buf[0] = b'C';
        buf[1] = b'O';
        buf[2] = b' ';
        buf[3] = b' ';
        buf[4] = b' ';
        buf[5] = b' ';
        buf[6] = b'0' + self.counter_idx / 10;
        buf[7] = b'0' + self.counter_idx % 10;
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn beep_counter(&self) {
        // Beep the counter value: low beeps for groups of 5, high beeps for the rest.
        let low_count = self.counter_idx / 5;
        let high_count = self.counter_idx % 5;
        for _ in 0..low_count {
            crate::movement::play_alarm_beeps(1, crate::watch::buzzer::Note::A6);
        }
        for _ in 0..high_count {
            crate::movement::play_alarm_beeps(1, crate::watch::buzzer::Note::B6);
        }
    }
}

impl WatchFace for CounterFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        if self.beep_on {
            watch::slcd::set_indicator(Indicator::Signal);
        }
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.counter_idx = (self.counter_idx + 1) % 100;
                self.print_counter();
                if self.beep_on {
                    self.beep_counter();
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.beep_on = !self.beep_on;
                if self.beep_on {
                    watch::slcd::set_indicator(Indicator::Signal);
                } else {
                    watch::slcd::clear_indicator(Indicator::Signal);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.counter_idx = 0;
                self.print_counter();
            }
            Event::Activate => self.print_counter(),
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
