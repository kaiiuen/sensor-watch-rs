//! Breathing exercise watch face.
//!
//! Port of the C `breathing_face.c`. Guides a box-breathing exercise (in,
//! hold, out, hold) with optional beeps. It is a pure state machine: it reacts
//! to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd::Indicator;

/// The breathing face state.
pub struct BreathingFace {
    current_stage: u8,
    sound_on: bool,
}

impl BreathingFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        BreathingFace {
            current_stage: 0,
            sound_on: true,
        }
    }

    pub fn new() -> Self {
        BreathingFace::new_static()
    }

    fn beep_in(&self) {
        crate::movement::play_alarm_beeps(1, Note::C4);
        crate::movement::play_alarm_beeps(1, Note::D4);
        crate::movement::play_alarm_beeps(1, Note::E4);
    }

    fn beep_in_hold(&self) {
        crate::movement::play_alarm_beeps(1, Note::E4);
        crate::movement::play_alarm_beeps(1, Note::E4);
    }

    fn beep_out(&self) {
        crate::movement::play_alarm_beeps(1, Note::E4);
        crate::movement::play_alarm_beeps(1, Note::D4);
        crate::movement::play_alarm_beeps(1, Note::C4);
    }

    fn beep_out_hold(&self) {
        crate::movement::play_alarm_beeps(1, Note::C4);
        crate::movement::play_alarm_beeps(1, Note::C4);
    }
}

impl WatchFace for BreathingFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.current_stage = 0;
        self.sound_on = true;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => {
                if self.sound_on {
                    watch::slcd::set_indicator(Indicator::Bell);
                } else {
                    watch::slcd::clear_indicator(Indicator::Bell);
                }
                match self.current_stage {
                    0 => {
                        watch::slcd::display_string("Breath", 4);
                        if self.sound_on {
                            self.beep_in();
                        }
                    }
                    1 => watch::slcd::display_string("In   3", 4),
                    2 => watch::slcd::display_string("In   2", 4),
                    3 => watch::slcd::display_string("In   1", 4),
                    4 => {
                        watch::slcd::display_string("Hold 4", 4);
                        if self.sound_on {
                            self.beep_in_hold();
                        }
                    }
                    5 => watch::slcd::display_string("Hold 3", 4),
                    6 => watch::slcd::display_string("Hold 2", 4),
                    7 => watch::slcd::display_string("Hold 1", 4),
                    8 => {
                        watch::slcd::display_string("Ou t 4", 4);
                        if self.sound_on {
                            self.beep_out();
                        }
                    }
                    9 => watch::slcd::display_string("Ou t 3", 4),
                    10 => watch::slcd::display_string("Ou t 2", 4),
                    11 => watch::slcd::display_string("Ou t 1", 4),
                    12 => {
                        watch::slcd::display_string("Hold 4", 4);
                        if self.sound_on {
                            self.beep_out_hold();
                        }
                    }
                    13 => watch::slcd::display_string("Hold 3", 4),
                    14 => watch::slcd::display_string("Hold 2", 4),
                    _ => watch::slcd::display_string("Hold 1", 4),
                }
                self.current_stage = (self.current_stage + 1) % 16;
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.sound_on = !self.sound_on;
                if self.sound_on {
                    watch::slcd::set_indicator(Indicator::Bell);
                } else {
                    watch::slcd::clear_indicator(Indicator::Bell);
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
