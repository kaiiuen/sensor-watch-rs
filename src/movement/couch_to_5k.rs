//! Couch to 5K watch face.
//!
//! Port of the C `couch_to_5k_face.c`. A guided run/walk interval trainer. It
//! is a pure state machine: it reacts to a single event and returns; it never
//! keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd;

const C25K_WEEK_1: [u16; 18] = [
    300, 60, 90, 60, 90, 60, 90, 60, 90, 60, 90, 60, 90, 60, 90, 60, 90, 0,
];
const C25K_WEEK_2: [u16; 14] = [300, 90, 120, 90, 120, 90, 120, 90, 120, 90, 120, 90, 120, 0];
const C25K_WEEK_3: [u16; 10] = [300, 90, 90, 180, 180, 90, 90, 180, 180, 0];
const C25K_WEEK_4: [u16; 9] = [300, 180, 90, 300, 150, 180, 90, 300, 0];
const C25K_WEEK_5_1: [u16; 7] = [300, 300, 180, 300, 180, 300, 0];
const C25K_WEEK_5_2: [u16; 5] = [300, 480, 300, 480, 0];
const C25K_WEEK_5_3: [u16; 3] = [300, 1200, 0];
const C25K_WEEK_6_1: [u16; 7] = [300, 300, 180, 480, 180, 300, 0];
const C25K_WEEK_6_2: [u16; 5] = [300, 600, 180, 600, 0];
const C25K_WEEK_6_3: [u16; 3] = [300, 1500, 0];
const C25K_WEEK_7: [u16; 3] = [300, 1500, 0];
const C25K_WEEK_8: [u16; 3] = [300, 1680, 0];
const C25K_WEEK_9: [u16; 3] = [300, 1800, 0];

const C25K_SESSIONS_LENGTH: usize = 27;

/// The session table (index -> week array).
const C25K_SESSIONS: [&[u16]; C25K_SESSIONS_LENGTH] = [
    &C25K_WEEK_1,
    &C25K_WEEK_1,
    &C25K_WEEK_1,
    &C25K_WEEK_2,
    &C25K_WEEK_2,
    &C25K_WEEK_2,
    &C25K_WEEK_3,
    &C25K_WEEK_3,
    &C25K_WEEK_3,
    &C25K_WEEK_4,
    &C25K_WEEK_4,
    &C25K_WEEK_4,
    &C25K_WEEK_5_1,
    &C25K_WEEK_5_2,
    &C25K_WEEK_5_3,
    &C25K_WEEK_6_1,
    &C25K_WEEK_6_2,
    &C25K_WEEK_6_3,
    &C25K_WEEK_7,
    &C25K_WEEK_7,
    &C25K_WEEK_7,
    &C25K_WEEK_8,
    &C25K_WEEK_8,
    &C25K_WEEK_8,
    &C25K_WEEK_9,
    &C25K_WEEK_9,
    &C25K_WEEK_9,
];

const C25K_WARMUP: u8 = 0;
const C25K_RUN: u8 = 1;
const C25K_WALK: u8 = 2;
const C25K_FINISHED: u8 = 3;

/// The couch to 5k face state.
pub struct CouchTo5kFace {
    session: usize,
    exercise: usize,
    timer: u16,
    exercise_type: u8,
    paused: bool,
}

impl CouchTo5kFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        CouchTo5kFace {
            session: 0,
            exercise: 0,
            timer: 300,
            exercise_type: C25K_WARMUP,
            paused: true,
        }
    }

    pub fn new() -> Self {
        CouchTo5kFace::new_static()
    }

    fn finished(&self) -> bool {
        self.exercise_type == C25K_FINISHED
    }

    fn cleared(&self) -> bool {
        self.timer == C25K_SESSIONS[self.session][0] && self.exercise == 0
    }

    fn next_session(&mut self) {
        self.session += 1;
        if self.session >= C25K_SESSIONS_LENGTH {
            self.session = 0;
        }
    }

    fn assign_exercise_type(&mut self) {
        if self.exercise == 0 {
            self.exercise_type = C25K_WARMUP;
        } else if self.exercise % 2 == 1 {
            self.exercise_type = C25K_RUN;
        } else {
            self.exercise_type = C25K_WALK;
        }
    }

    fn next_exercise(&mut self) {
        self.exercise += 1;
        self.timer = C25K_SESSIONS[self.session][self.exercise];
        if self.timer == 0 {
            crate::movement::play_alarm_beeps(7, Note::C8);
            self.exercise_type = C25K_FINISHED;
            return;
        }
        crate::movement::play_alarm_beeps(4, Note::A7);
        self.assign_exercise_type();
    }

    fn init_session(&mut self) {
        self.exercise = 0;
        self.timer = C25K_SESSIONS[self.session][self.exercise];
        self.assign_exercise_type();
    }

    fn exercise_type_to_str(&self) -> &'static str {
        match self.exercise_type {
            C25K_WARMUP => "WU",
            C25K_RUN => "RU",
            C25K_WALK => "WA",
            C25K_FINISHED => "--",
            _ => "  ",
        }
    }

    fn display(&self) {
        let mut buf = [0u8; 11];
        let t = self.exercise_type_to_str().as_bytes();
        buf[0] = t[0];
        buf[1] = t[1];
        let seconds = self.timer % 60;
        buf[2] = b'0' + (((self.session + 1) % 100) / 10) as u8;
        buf[3] = b'0' + (((self.session + 1) % 100) % 10) as u8;
        let mins = ((self.timer - seconds) / 60) % 100;
        buf[4] = b'0' + (mins / 10) as u8;
        buf[5] = b'0' + (mins % 10) as u8;
        buf[6] = b'0' + (seconds / 10) as u8;
        buf[7] = b'0' + (seconds % 10) as u8;
        buf[8] = b'0' + (((self.exercise + 1) % 100) / 10) as u8;
        buf[9] = b'0' + (((self.exercise + 1) % 100) % 10) as u8;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

impl WatchFace for CouchTo5kFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        slcd::set_colon();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => {
                self.init_session();
                self.paused = true;
                self.display();
            }
            Event::Tick => {
                if !self.paused && !self.finished() {
                    if self.timer == 0 {
                        self.next_exercise();
                    } else {
                        self.timer -= 1;
                    }
                }
                self.display();
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.finished() {
                    self.next_session();
                    self.init_session();
                    self.paused = true;
                } else if self.paused {
                    if self.cleared() {
                        self.next_session();
                    }
                    self.init_session();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if settings.button_should_sound() {
                    crate::movement::play_alarm_beeps(1, Note::C8);
                }
                self.paused = !self.paused;
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
