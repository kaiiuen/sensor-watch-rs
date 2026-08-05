//! Higher/Lower game watch face.
//!
//! Port of the C `higher_lower_game_face.c`. A card guessing game where you
//! predict whether the next card is higher or lower. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::slcd;

const TITLE_TEXT: &str = "Hi-Lo";
const GAME_BOARD_SIZE: usize = 6;
const MAX_BOARDS: u8 = 40;
const GUESSES_PER_SCREEN: u8 = 5;
const WIN_SCORE: u8 = MAX_BOARDS * GUESSES_PER_SCREEN;
const STATUS_DISPLAY_START: u8 = 0;
const BOARD_SCORE_DISPLAY_START: u8 = 2;
const BOARD_DISPLAY_START: u8 = 4;
const BOARD_DISPLAY_END: u8 = 9;
const MIN_CARD_VALUE: u8 = 2;
const MAX_CARD_VALUE: u8 = 14;
const CARD_RANK_COUNT: u8 = MAX_CARD_VALUE - MIN_CARD_VALUE + 1;
const CARD_SUIT_COUNT: u8 = 4;
const DECK_SIZE: u8 = CARD_SUIT_COUNT * CARD_RANK_COUNT;

/// A card.
#[derive(Clone, Copy)]
struct Card {
    value: u8,
    revealed: bool,
}

/// The game state.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GameState {
    TitleScreen,
    Guessing,
    Win,
    Lose,
    ShowScore,
}

/// The higher/lower face state.
pub struct HigherLowerGameFace {
    game_state: GameState,
    game_board: [Card; GAME_BOARD_SIZE],
    guess_position: u8,
    score: u8,
    completed_board_count: u8,
    deck: [u8; DECK_SIZE as usize],
    current_card: u8,
}

impl HigherLowerGameFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        HigherLowerGameFace {
            game_state: GameState::TitleScreen,
            game_board: [Card {
                value: 0,
                revealed: false,
            }; GAME_BOARD_SIZE],
            guess_position: 0,
            score: 0,
            completed_board_count: 0,
            deck: [0; DECK_SIZE as usize],
            current_card: 0,
        }
    }

    pub fn new() -> Self {
        HigherLowerGameFace::new_static()
    }

    fn generate_random_number(&self, num_values: u8) -> u8 {
        let now = crate::watch::rtc::get_date_time();
        let mut x = now.to_reg();
        if x == 0 {
            x = 0x0BAD_F00D;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        (x % num_values as u32) as u8
    }

    fn stack_deck(&mut self) {
        for i in 0..CARD_RANK_COUNT {
            for j in 0..CARD_SUIT_COUNT {
                self.deck[(i * CARD_SUIT_COUNT + j) as usize] = MIN_CARD_VALUE + i;
            }
        }
    }

    fn shuffle_deck(&mut self) {
        let mut i = DECK_SIZE - 1;
        while i > 0 {
            let j = self.generate_random_number(0xFF) % (i + 1);
            let tmp = self.deck[j as usize];
            self.deck[j as usize] = self.deck[i as usize];
            self.deck[i as usize] = tmp;
            i -= 1;
        }
    }

    fn reset_deck(&mut self) {
        self.current_card = 0;
        self.stack_deck();
        self.shuffle_deck();
    }

    fn get_next_card(&mut self) -> u8 {
        if self.current_card >= DECK_SIZE {
            self.reset_deck();
        }
        let card = self.deck[self.current_card as usize];
        self.current_card += 1;
        card
    }

    fn reset_board(&mut self, first_round: bool) {
        let first_card_value = if first_round {
            self.get_next_card()
        } else {
            self.game_board[GAME_BOARD_SIZE - 1].value
        };
        self.game_board[0].value = first_card_value;
        self.game_board[0].revealed = true;
        for i in 1..GAME_BOARD_SIZE {
            self.game_board[i].value = self.get_next_card();
            self.game_board[i].revealed = false;
        }
    }

    fn init_game(&mut self) {
        slcd::clear_display();
        slcd::display_string(TITLE_TEXT, BOARD_DISPLAY_START);
        slcd::display_string("GA", STATUS_DISPLAY_START);
        self.reset_deck();
        self.reset_board(true);
        self.score = 0;
        self.completed_board_count = 0;
        self.guess_position = 1;
    }

    fn render_board_position(&self, board_position: usize) {
        let display_position = BOARD_DISPLAY_END - board_position as u8;
        let card = self.game_board[board_position];
        if !card.revealed {
            slcd::display_character(b' ', display_position);
            return;
        }
        match card.value {
            14 => {
                slcd::display_character(b' ', display_position);
                slcd::display_character(b'A', display_position);
            }
            13 => {
                slcd::display_character(b' ', display_position);
                slcd::display_character(b'K', display_position);
            }
            12 => {
                slcd::display_character(b'Q', display_position);
            }
            _ => {
                slcd::display_character((card.value - MIN_CARD_VALUE) + b'0', display_position);
            }
        }
    }

    fn render_board(&self) {
        for i in 0..GAME_BOARD_SIZE {
            self.render_board_position(i);
        }
    }

    fn render_board_count(&self) {
        let mut buf = [0u8; 3];
        buf[0] = b'0' + self.completed_board_count / 10;
        buf[1] = b'0' + self.completed_board_count % 10;
        slcd::display_string(
            core::str::from_utf8(&buf[..2]).unwrap_or("  "),
            BOARD_SCORE_DISPLAY_START,
        );
    }

    fn render_final_score(&self) {
        slcd::display_string("SC", STATUS_DISPLAY_START);
        let complete_boards = self.score / GUESSES_PER_SCREEN;
        let mut buf = [0u8; 7];
        buf[0] = b'0' + complete_boards / 10;
        buf[1] = b'0' + complete_boards % 10;
        buf[2] = b' ';
        buf[3] = b'0' + (self.score / 100) % 10;
        buf[4] = b'0' + (self.score / 10) % 10;
        buf[5] = b'0' + self.score % 10;
        slcd::set_colon();
        slcd::display_string(
            core::str::from_utf8(&buf[..6]).unwrap_or(""),
            BOARD_DISPLAY_START,
        );
    }

    fn get_answer(&mut self) -> u8 {
        if self.guess_position < 1 || self.guess_position > GAME_BOARD_SIZE as u8 {
            return 0;
        }
        self.game_board[self.guess_position as usize].revealed = true;
        let previous = self.game_board[self.guess_position as usize - 1].value;
        let current = self.game_board[self.guess_position as usize].value;
        if current > previous {
            1
        } else if current < previous {
            2
        } else {
            0
        }
    }

    fn do_game_loop(&mut self, user_guess: u8) {
        match self.game_state {
            GameState::TitleScreen => {
                self.init_game();
                self.render_board();
                self.render_board_count();
                self.game_state = GameState::Guessing;
            }
            GameState::Guessing => {
                let answer = self.get_answer();
                match answer {
                    0 => slcd::display_string("==", STATUS_DISPLAY_START),
                    1 => slcd::display_string("HI", STATUS_DISPLAY_START),
                    _ => slcd::display_string("LO", STATUS_DISPLAY_START),
                }
                if answer == user_guess {
                    self.score += 1;
                } else if answer != 0 {
                    slcd::display_string("GO", STATUS_DISPLAY_START);
                    self.game_board[self.guess_position as usize].revealed = true;
                    self.render_board_position(self.guess_position as usize);
                    self.game_state = GameState::Lose;
                    return;
                }
                if self.score >= WIN_SCORE {
                    slcd::display_string("WI", STATUS_DISPLAY_START);
                    slcd::display_string("  ", BOARD_SCORE_DISPLAY_START);
                    slcd::display_string("------", BOARD_DISPLAY_START);
                    self.game_state = GameState::Win;
                    return;
                }
                let final_board_guess = self.guess_position == GAME_BOARD_SIZE as u8 - 1;
                if final_board_guess {
                    self.completed_board_count += 1;
                    self.render_board_count();
                    self.guess_position = 1;
                    self.reset_board(false);
                    self.render_board();
                } else {
                    self.guess_position += 1;
                    self.render_board_position(self.guess_position as usize - 1);
                    self.render_board_position(self.guess_position as usize);
                }
            }
            GameState::Win | GameState::Lose => {
                slcd::clear_display();
                self.render_final_score();
                self.game_state = GameState::ShowScore;
            }
            GameState::ShowScore => {
                slcd::clear_display();
                slcd::display_string(TITLE_TEXT, BOARD_DISPLAY_START);
                slcd::display_string("GA", STATUS_DISPLAY_START);
                self.game_state = GameState::TitleScreen;
            }
        }
    }
}

impl WatchFace for HigherLowerGameFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.game_state = GameState::TitleScreen;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => {
                slcd::display_string(TITLE_TEXT, BOARD_DISPLAY_START);
                slcd::display_string("GA", STATUS_DISPLAY_START);
            }
            Event::Tick => {}
            Event::Button(Button::Light, ButtonEvent::Up) => self.do_game_loop(1),
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            Event::Button(Button::Alarm, ButtonEvent::Up) => self.do_game_loop(2),
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
