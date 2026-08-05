//! Butterfly game watch face.
//!
//! Port of the C `butterfly_game_face.c`. A two-player reaction game where
//! players spot a butterfly shape among decoys. It is a pure state machine: it
//! reacts to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd;

const BUTTERFLY_SHAPES: [&str; 13] = [
    "[]", "][", "25", "52", "9e", "e9", "6a", "a6", "3E", "E3", "00", "HH", "88",
];
const NUM_SHAPES: u8 = 13;

const POS_LEFT: u8 = 4;
const POS_CENTER: u8 = 6;
const POS_RIGHT: u8 = 8;

const TICK_FREQ: u32 = 8;
const TICKS_PER_SHAPE: u8 = 8;

const PLAYER_1: u8 = 0;
const PLAYER_2: u8 = 1;

/// The current screen.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Splash,
    SoundSelect,
    ContinueSelect,
    Reset,
    GoalSelect,
    RoundStart,
    FirstWrongShape,
    WrongShape,
    CorrectShape,
    RoundLose,
    RoundWin,
    GameWin,
}

/// The butterfly game face state.
pub struct ButterflyGameFace {
    screen: Screen,
    ctr: u8,
    correct_shape: u8,
    current_shape: u8,
    show_correct_shape_after: u8,
    round_winner: u8,
    score_p1: u8,
    score_p2: u8,
    goal_score: u8,
    sound: bool,
    cont: bool,
}

impl ButterflyGameFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        ButterflyGameFace {
            screen: Screen::Splash,
            ctr: 0,
            correct_shape: 0,
            current_shape: 0,
            show_correct_shape_after: 0,
            round_winner: 0,
            score_p1: 0,
            score_p2: 0,
            goal_score: 6,
            sound: true,
            cont: false,
        }
    }

    pub fn new() -> Self {
        ButterflyGameFace::new_static()
    }

    fn get_rand(&self, max: u8) -> u8 {
        let now = crate::watch::rtc::get_date_time();
        let mut x = now.to_reg();
        if x == 0 {
            x = 0xABCD_EF01;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        (x % max as u32) as u8
    }

    fn pick_wrong_shape(&self, skip_wrong_shape: bool) -> u8 {
        if !skip_wrong_shape {
            let mut r = self.get_rand(NUM_SHAPES - 1);
            if r >= self.correct_shape {
                r += 1;
            }
            r
        } else {
            let mut r = self.get_rand(NUM_SHAPES - 2);
            let (i1, i2) = if self.correct_shape < self.current_shape {
                (self.correct_shape, self.current_shape)
            } else {
                (self.current_shape, self.correct_shape)
            };
            if r >= i1 {
                r += 1;
            }
            if r >= i2 {
                r += 1;
            }
            r
        }
    }

    fn display_shape(&self, shape: u8, pos: u8) {
        slcd::display_string(BUTTERFLY_SHAPES[shape as usize], pos);
    }

    fn display_scores(&self) {
        let mut buf = [b' '; 1];
        buf[0] = b'0' + self.score_p1;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(" "), 0);
        buf[0] = b'0' + self.score_p2;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(" "), 3);
    }

    fn play_sound(&self, note: Note) {
        if self.sound {
            crate::movement::play_alarm_beeps(1, note);
        }
    }

    fn transition(&mut self, screen: Screen) {
        self.screen = screen;
        self.handle_activate();
    }

    fn handle_activate(&mut self) {
        match self.screen {
            Screen::Splash => {
                self.ctr = TICK_FREQ as u8;
                slcd::clear_display();
                slcd::display_string("Btrfly", 4);
            }
            Screen::SoundSelect => {
                slcd::clear_display();
            }
            Screen::ContinueSelect => {
                slcd::clear_display();
                if self.score_p1 == 0 && self.score_p2 == 0 {
                    self.transition(Screen::GoalSelect);
                    return;
                }
                self.cont = false;
            }
            Screen::Reset => {
                self.score_p1 = 0;
                self.score_p2 = 0;
                self.transition(Screen::GoalSelect);
                return;
            }
            Screen::GoalSelect => {
                slcd::clear_display();
                self.goal_score = 6;
            }
            Screen::RoundStart => {
                self.correct_shape = self.get_rand(NUM_SHAPES);
                self.show_correct_shape_after = self.get_rand(10) + 1;
                slcd::display_string("    -    -", 0);
                self.display_scores();
                self.display_shape(self.correct_shape, POS_CENTER);
            }
            Screen::FirstWrongShape => {
                self.ctr = TICKS_PER_SHAPE;
                self.current_shape = self.pick_wrong_shape(false);
                self.display_shape(self.current_shape, POS_CENTER);
                self.play_sound(Note::A7);
            }
            Screen::WrongShape => {
                self.ctr = TICKS_PER_SHAPE;
                self.current_shape = self.pick_wrong_shape(true);
                self.display_shape(self.current_shape, POS_CENTER);
                self.play_sound(Note::A7);
            }
            Screen::CorrectShape => {
                self.display_shape(self.correct_shape, POS_CENTER);
                self.play_sound(Note::A7);
            }
            Screen::RoundLose => {
                self.ctr = TICK_FREQ as u8;
                if self.round_winner == PLAYER_1 {
                    if self.score_p2 > 0 {
                        self.score_p2 -= 1;
                    }
                } else if self.score_p1 > 0 {
                    self.score_p1 -= 1;
                }
                self.display_shape(self.correct_shape, POS_CENTER);
                self.play_sound(Note::E6);
            }
            Screen::RoundWin => {
                self.ctr = TICK_FREQ as u8;
                if self.round_winner == PLAYER_1 {
                    self.score_p1 += 1;
                } else {
                    self.score_p2 += 1;
                }
                slcd::clear_display();
                self.display_scores();
                self.display_shape(
                    self.correct_shape,
                    if self.round_winner == PLAYER_1 {
                        POS_LEFT
                    } else {
                        POS_RIGHT
                    },
                );
                self.play_sound(Note::C6);
            }
            Screen::GameWin => {
                self.ctr = 4 * TICK_FREQ as u8;
                slcd::clear_display();
                if self.score_p1 >= self.goal_score {
                    slcd::display_string("pl1  wins", 0);
                } else {
                    slcd::display_string("pl2  wins", 0);
                }
                self.play_sound(Note::G6);
            }
        }
    }

    fn handle_tick(&mut self) {
        match self.screen {
            Screen::Splash => {
                self.ctr -= 1;
                if self.ctr == 0 {
                    self.transition(Screen::SoundSelect);
                }
            }
            Screen::FirstWrongShape => {
                self.ctr -= 1;
                if self.ctr == 0 {
                    self.transition(Screen::WrongShape);
                }
            }
            Screen::WrongShape => {
                self.ctr -= 1;
                if self.ctr == 0 {
                    self.show_correct_shape_after -= 1;
                    if self.show_correct_shape_after == 0 {
                        self.transition(Screen::CorrectShape);
                    } else {
                        self.transition(Screen::WrongShape);
                    }
                }
            }
            Screen::RoundLose => {
                self.ctr -= 1;
                if self.ctr == 0 {
                    self.transition(Screen::RoundStart);
                } else {
                    self.display_shape(
                        if self.ctr % 2 == 1 {
                            self.correct_shape
                        } else {
                            self.current_shape
                        },
                        POS_CENTER,
                    );
                }
            }
            Screen::RoundWin => {
                self.ctr -= 1;
                if self.ctr == 0 {
                    if self.score_p1 >= self.goal_score || self.score_p2 >= self.goal_score {
                        self.transition(Screen::GameWin);
                    } else {
                        self.transition(Screen::RoundStart);
                    }
                }
            }
            Screen::GameWin => {
                self.ctr -= 1;
                if self.ctr == 0 {
                    self.transition(Screen::Reset);
                }
            }
            _ => {}
        }
    }

    fn handle_light_down(&mut self) {
        match self.screen {
            Screen::Splash => self.transition(Screen::SoundSelect),
            Screen::SoundSelect => self.transition(Screen::ContinueSelect),
            Screen::ContinueSelect => {
                if self.cont {
                    self.transition(Screen::RoundStart);
                } else {
                    self.transition(Screen::Reset);
                }
            }
            Screen::GoalSelect => self.transition(Screen::RoundStart),
            Screen::RoundStart => {
                slcd::display_string("      ", 4);
                self.transition(Screen::FirstWrongShape);
            }
            Screen::FirstWrongShape | Screen::WrongShape => {
                self.round_winner = PLAYER_2;
                self.transition(Screen::RoundLose);
            }
            Screen::CorrectShape => {
                self.round_winner = PLAYER_1;
                self.transition(Screen::RoundWin);
            }
            _ => {}
        }
    }

    fn handle_alarm_down(&mut self) {
        match self.screen {
            Screen::Splash => self.transition(Screen::SoundSelect),
            Screen::SoundSelect => self.sound = !self.sound,
            Screen::ContinueSelect => self.cont = !self.cont,
            Screen::GoalSelect => {
                self.goal_score += 3;
                if self.goal_score > 9 {
                    self.goal_score = 3;
                }
            }
            Screen::RoundStart => {
                slcd::display_string("      ", 4);
                self.transition(Screen::FirstWrongShape);
            }
            Screen::FirstWrongShape | Screen::WrongShape => {
                self.round_winner = PLAYER_1;
                self.transition(Screen::RoundLose);
            }
            Screen::CorrectShape => {
                self.round_winner = PLAYER_2;
                self.transition(Screen::RoundWin);
            }
            _ => {}
        }
    }
}

impl WatchFace for ButterflyGameFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => {
                self.transition(Screen::Splash);
            }
            Event::Tick => self.handle_tick(),
            Event::Button(Button::Light, ButtonEvent::Down) => self.handle_light_down(),
            Event::Button(Button::Alarm, ButtonEvent::Down) => self.handle_alarm_down(),
            _ => movement::default_loop_handler(event, _settings),
        }
        // Draw the goal select screen value.
        if self.screen == Screen::GoalSelect {
            let mut buf = [0u8; 6];
            buf[0] = b'G';
            buf[1] = b'O';
            buf[2] = b'a';
            buf[3] = b'L';
            buf[4] = b' ';
            buf[5] = b'0' + self.goal_score;
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
        } else if self.screen == Screen::SoundSelect {
            if self.sound {
                slcd::display_string("snd y", 5);
            } else {
                slcd::display_string("snd n", 5);
            }
        } else if self.screen == Screen::ContinueSelect {
            if self.cont {
                slcd::display_string("Cont y", 4);
            } else {
                slcd::display_string("Cont n", 4);
            }
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
