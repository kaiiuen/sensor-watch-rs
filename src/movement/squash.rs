//! Squash scoring watch face.
//!
//! Port of the C `squash_face.c`. Keeps track of scores in a squash match. It
//! is a pure state machine: it reacts to a single event and returns; it never
//! keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::slcd;
use crate::watch::slcd::Indicator;

// Using "point-a-rally scoring (PARS)" to 11 below.
const POINTS_TO_WIN_GAME: u8 = 11;
// For example if both players have 10 points one of them has to get to 12, not 11, to win.
const MIN_POINT_DIFFERENCE: u8 = 2;
// First to 3 games won (max 5 games played).
const GAMES_TO_WIN_MATCH: u8 = 3;

/// The squash face state.
pub struct SquashFace {
    player1_score: u8,
    player2_score: u8,
    player1_games: u8,
    player2_games: u8,
    is_game_over: bool,
}

impl SquashFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        SquashFace {
            player1_score: 0,
            player2_score: 0,
            player1_games: 0,
            player2_games: 0,
            is_game_over: false,
        }
    }

    pub fn new() -> Self {
        SquashFace::new_static()
    }

    fn update_display(&self) {
        slcd::clear_display();

        // The colon makes it easier to distinguish each player's score.
        slcd::set_colon();

        // Show games won in small digits.
        let mut buf = [b' '; 2];
        buf[0] = b'0' + self.player1_games / 10;
        buf[1] = b'0' + self.player1_games % 10;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        buf[0] = b'0' + self.player2_games / 10;
        buf[1] = b'0' + self.player2_games % 10;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 2);

        // Show current score: P1-P2.
        buf[0] = b'0' + self.player1_score / 10;
        buf[1] = b'0' + self.player1_score % 10;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
        buf[0] = b'0' + self.player2_score / 10;
        buf[1] = b'0' + self.player2_score % 10;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 6);

        // If game over, show indicator.
        if self.is_game_over {
            slcd::set_indicator(Indicator::Lap);
        } else {
            slcd::clear_indicator(Indicator::Lap);
        }
    }

    fn check_game_status(&mut self) {
        // Check if a player has won the current game.
        if (self.player1_score >= POINTS_TO_WIN_GAME || self.player2_score >= POINTS_TO_WIN_GAME)
            && (self.player1_score as i16 - self.player2_score as i16).abs()
                >= MIN_POINT_DIFFERENCE as i16
        {
            // Award a game to the winner.
            if self.player1_score > self.player2_score {
                self.player1_games += 1;
                movement::play_signal();
            } else {
                self.player2_games += 1;
                movement::play_signal();
            }

            // Check if the match is over.
            if self.player1_games >= GAMES_TO_WIN_MATCH || self.player2_games >= GAMES_TO_WIN_MATCH
            {
                self.is_game_over = true;
                movement::play_signal();
            } else {
                // Reset for next game.
                self.player1_score = 0;
                self.player2_score = 0;
            }
        }
    }

    fn reset_match(&mut self) {
        self.player1_score = 0;
        self.player2_score = 0;
        self.player1_games = 0;
        self.player2_games = 0;
        self.is_game_over = false;

        movement::play_signal();
    }
}

impl WatchFace for SquashFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.update_display();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => self.update_display(),
            Event::Button(Button::Light, ButtonEvent::Down) => {
                // Suppress default LED behavior.
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if !self.is_game_over {
                    // Increment player 1's score.
                    self.player1_score += 1;
                    self.check_game_status();
                    self.update_display();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if !self.is_game_over {
                    // Increment player 2's score.
                    self.player2_score += 1;
                    self.check_game_status();
                    self.update_display();
                }
            }
            Event::Button(Button::Mode, ButtonEvent::LongPress) => {
                // Reset the match.
                self.reset_match();
                self.update_display();
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => self.update_display(),
            Event::BackgroundTask => {}
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
