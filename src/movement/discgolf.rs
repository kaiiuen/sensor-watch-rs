//! Disc golf scorecard watch face.
//!
//! Port of the C `discgolf_face.c`. Keeps score for disc golf rounds across
//! several courses. It is a pure state machine: it reacts to a single event
//! and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd::Indicator;

const COURSES: usize = 11;

const PARS: [[u8; 18]; COURSES] = [
    [3, 3, 4, 3, 3, 3, 5, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3],
    [3, 4, 3, 3, 4, 3, 3, 3, 3, 4, 3, 3, 3, 3, 3, 3, 3, 3],
    [3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0],
    [3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0],
    [3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0],
    [3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0],
    [3, 3, 3, 3, 3, 3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [3, 3, 3, 4, 3, 3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [3, 3, 3, 3, 3, 3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [3, 3, 3, 3, 3, 3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [3, 3, 3, 3, 3, 3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0],
];

const HOLES: [u8; COURSES] = [18, 18, 10, 10, 10, 10, 9, 9, 9, 9, 9];

const LABELS: [(u8, u8); COURSES] = [
    (b'G', b'H'),
    (b'G', b'N'),
    (b'V', b'I'),
    (b'D', b'V'),
    (b'L', b'A'),
    (b'G', b'L'),
    (b'V', b'T'),
    (b'F', b'V'),
    (b'K', b'T'),
    (b'S', b'E'),
    (b'F', b'H'),
];

/// The disc golf mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DgMode {
    Setting,
    Idle,
    Scoring,
}

/// The disc golf face state.
pub struct DiscgolfFace {
    mode: DgMode,
    course: usize,
    hole: u8,
    playing: u8,
    scores: [u8; 18],
    best: [i8; COURSES],
}

impl DiscgolfFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        DiscgolfFace {
            mode: DgMode::Setting,
            course: 0,
            hole: 1,
            playing: 19,
            scores: [0; 18],
            best: [99; COURSES],
        }
    }

    pub fn new() -> Self {
        DiscgolfFace::new_static()
    }

    fn beep(&self, settings: &Settings) {
        if settings.button_should_sound() {
            crate::movement::play_alarm_beeps(1, Note::C7);
        }
    }

    fn reset(&mut self) {
        for i in 0..HOLES[self.course] as usize {
            self.scores[i] = 0;
        }
        self.hole = 1;
        watch::slcd::clear_indicator(Indicator::Lap);
    }

    fn score_sum(&self) -> u8 {
        let mut sum = 0;
        for i in 0..HOLES[self.course] as usize {
            sum += self.scores[i];
        }
        sum
    }

    fn count_played(&self) -> u8 {
        let mut played = 0;
        for i in 0..HOLES[self.course] as usize {
            if self.scores[i] > 0 {
                played += 1;
            }
        }
        played
    }

    fn calculate_score(&self) -> i8 {
        let mut par_sum = 0u8;
        let mut score_sum = 0u8;
        for i in 0..HOLES[self.course] as usize {
            if self.scores[i] > 0 {
                par_sum += PARS[self.course][i];
                score_sum += self.scores[i];
            }
        }
        score_sum as i8 - par_sum as i8
    }

    fn store_best(&mut self) {
        let played = self.count_played();
        if played == HOLES[self.course] {
            let high_score = self.calculate_score();
            if high_score < self.best[self.course] {
                self.best[self.course] = high_score;
            }
        }
    }
}

impl WatchFace for DiscgolfFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        watch::slcd::clear_colon();
        if self.playing <= HOLES[0] {
            self.hole = self.playing;
        }
        if self.count_played() == HOLES[self.course] {
            watch::slcd::set_indicator(Indicator::Lap);
        }
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                if self.mode != DgMode::Scoring {
                    movement::move_to_next_face();
                }
            }
            Event::Button(Button::Mode, ButtonEvent::LongPress) => {
                if self.mode != DgMode::Scoring {
                    movement::move_to_face(0);
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                match self.mode {
                    DgMode::Idle => {
                        if self.score_sum() == 0 {
                            self.playing = self.hole;
                        }
                        self.mode = DgMode::Scoring;
                    }
                    DgMode::Scoring => {
                        if self.count_played() == HOLES[self.course] {
                            watch::slcd::set_indicator(Indicator::Lap);
                        }
                        if self.hole == self.playing {
                            if self.hole < HOLES[self.course] {
                                self.hole += 1;
                            } else {
                                self.hole = 1;
                            }
                            if self.playing < HOLES[self.course] {
                                self.playing += 1;
                            } else {
                                self.playing = 1;
                            }
                        }
                        self.mode = DgMode::Idle;
                    }
                    DgMode::Setting => {
                        self.playing = HOLES[self.course] + 1;
                        self.mode = DgMode::Idle;
                    }
                }
                self.beep(settings);
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => match self.mode {
                DgMode::Setting => {
                    self.course = (self.course + 1) % COURSES;
                }
                DgMode::Scoring => {
                    self.scores[(self.hole - 1) as usize] =
                        (self.scores[(self.hole - 1) as usize] + 1) % 16;
                }
                DgMode::Idle => {
                    if self.hole < HOLES[self.course] {
                        self.hole += 1;
                    } else {
                        self.hole = 1;
                    }
                }
            },
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.mode == DgMode::Idle {
                    self.mode = DgMode::Setting;
                    self.store_best();
                    self.reset();
                    self.beep(settings);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.mode == DgMode::Idle
                    && self.hole != self.playing
                    && self.playing <= HOLES[self.course]
                {
                    self.hole = self.playing;
                    self.beep(settings);
                }
            }
            _ => {}
        }

        let mut buf = [0u8; 11];
        let (l0, l1) = LABELS[self.course];
        match self.mode {
            DgMode::Setting => {
                let best = self.best[self.course];
                buf[0] = l0;
                buf[1] = l1;
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b' ';
                buf[5] = if best < 0 { b'-' } else { b' ' };
                buf[6] = b'0' + (best.unsigned_abs() / 10) as u8;
                buf[7] = b'0' + (best.unsigned_abs() % 10) as u8;
                buf[8] = b' ';
                buf[9] = b' ';
            }
            DgMode::Idle => {
                buf[0] = l0;
                buf[1] = l1;
                buf[2] = b'0' + self.hole / 10;
                buf[3] = b'0' + self.hole % 10;
                buf[4] = b' ';
                if self.hole == self.playing {
                    let diff = self.calculate_score();
                    buf[5] = if diff < 0 { b'-' } else { b' ' };
                    buf[6] = b'0' + (diff.unsigned_abs() / 10) as u8;
                    buf[7] = b'0' + (diff.unsigned_abs() % 10) as u8;
                } else {
                    buf[5] = b' ';
                    buf[6] = b'0' + self.scores[(self.hole - 1) as usize] / 10;
                    buf[7] = b'0' + self.scores[(self.hole - 1) as usize] % 10;
                }
                buf[8] = b' ';
                buf[9] = b' ';
            }
            DgMode::Scoring => {
                buf[0] = l0;
                buf[1] = l1;
                buf[2] = b'0' + self.hole / 10;
                buf[3] = b'0' + self.hole % 10;
                buf[4] = b' ';
                buf[5] = b' ';
                buf[6] = b'0' + self.scores[(self.hole - 1) as usize] / 10;
                buf[7] = b'0' + self.scores[(self.hole - 1) as usize] % 10;
                buf[8] = b' ';
                buf[9] = b' ';
            }
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn resign(&mut self, _settings: &mut Settings) {
        watch::slcd::clear_indicator(Indicator::Lap);
    }
}
