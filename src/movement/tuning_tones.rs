//! Tuning tones watch face.
//!
//! Port of the C `tuning_tones_face.c`. Plays a reference tone for tuning
//! musical instruments. It is a pure state machine: it reacts to a single
//! event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;

/// A note and its display name.
struct NoteEntry {
    note: Note,
    name: &'static str,
}

const NOTES: [NoteEntry; 12] = [
    NoteEntry {
        note: Note::C5,
        name: "C ",
    },
    NoteEntry {
        note: Note::C5SharpD5Flat,
        name: "Db",
    },
    NoteEntry {
        note: Note::D5,
        name: "D ",
    },
    NoteEntry {
        note: Note::D5SharpE5Flat,
        name: "Eb",
    },
    NoteEntry {
        note: Note::E5,
        name: "E ",
    },
    NoteEntry {
        note: Note::F5,
        name: "F ",
    },
    NoteEntry {
        note: Note::F5SharpG5Flat,
        name: "Gb",
    },
    NoteEntry {
        note: Note::G5,
        name: "G ",
    },
    NoteEntry {
        note: Note::G5SharpA5Flat,
        name: "Ab",
    },
    NoteEntry {
        note: Note::A5,
        name: "A ",
    },
    NoteEntry {
        note: Note::A5SharpB5Flat,
        name: "Bb",
    },
    NoteEntry {
        note: Note::B5,
        name: "B ",
    },
];

/// The tuning tones face state.
pub struct TuningTonesFace {
    note_ind: usize,
    playing: bool,
}

impl TuningTonesFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        TuningTonesFace {
            note_ind: 9,
            playing: false,
        }
    }

    pub fn new() -> Self {
        TuningTonesFace::new_static()
    }

    fn draw(&self) {
        watch::slcd::display_string(NOTES[self.note_ind].name, 8);
    }

    fn update_buzzer(&self) {
        if self.playing {
            watch::buzzer::set_buzzer_off();
            watch::buzzer::set_buzzer_period(
                crate::watch::buzzer::NOTE_PERIODS[NOTES[self.note_ind].note as usize] as u32,
            );
            watch::buzzer::set_buzzer_on();
        }
    }
}

impl WatchFace for TuningTonesFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.draw();
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => self.draw(),
            Event::Tick => {}
            Event::Button(Button::Light, ButtonEvent::Down) => {
                self.note_ind += 1;
                if self.note_ind == NOTES.len() {
                    self.note_ind = 0;
                }
                self.update_buzzer();
                self.draw();
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {}
            Event::Button(Button::Alarm, ButtonEvent::Down) => {
                self.playing = !self.playing;
                if !self.playing {
                    watch::buzzer::set_buzzer_off();
                } else {
                    self.update_buzzer();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {}
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        if self.playing {
            self.playing = false;
            watch::buzzer::set_buzzer_off();
        }
    }
}
