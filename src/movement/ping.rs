//! PING watch face.
//!
//! Port of the C `ping_face.c`. A pong-style game played on the LCD. On the
//! title screen you can select a difficulty by long-pressing LIGHT or toggle
//! sound by long-pressing ALARM. ALARM moves the paddle; holding it longer
//! makes the paddle travel further. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd;

const FREQ_BABY: u8 = 2;
const FREQ_EASY: u8 = 4;
const FREQ_NORM: u8 = 8;

const BALL_POS_MAX: u8 = 11;
const BALL_OFF_SCREEN: u8 = 100;
const MAX_HI_SCORE: u16 = 9999; // Max hi score to store and display on the title screen.
const MAX_DISP_SCORE: u16 = 39; // The top-right digits can't properly display above 39.

#[derive(Clone, Copy, PartialEq)]
enum PingPaddleState {
    Retracted = 0,
    Extending,
    Extended,
    Retracting,
}

#[derive(Clone, Copy, PartialEq)]
enum PingCurrScreen {
    Title = 0,
    Score,
    Playing,
    Lose,
    Count,
}

#[derive(Clone, Copy, PartialEq)]
enum PingDifficulty {
    Baby = 0,
    Easy,
    Norm,
    Hard,
    Count,
}

#[derive(Clone, Copy, PartialEq)]
enum PingResult {
    Lose = -1,
    None = 0,
    Hit = 1,
    FirstHit = 2,
}

/// The start tune: (note, duration) pairs ending in 0.
static START_TUNE: [i8; 7] = [
    Note::C5 as i8,
    15,
    Note::E5 as i8,
    15,
    Note::G5 as i8,
    15,
    0,
];

/// The lose tune: (note, duration) pairs ending in 0.
static LOSE_TUNE: [i8; 7] = [
    Note::D3 as i8,
    10,
    Note::C3SharpD3Flat as i8,
    10,
    Note::C3 as i8,
    10,
    0,
];

/// The transient game state (the C `game_state_t`).
struct GameState {
    ball_pos: u8, // 0 to 11; 0 is the bottom-right and 11 is the top right.
    paddle_pos: PingPaddleState,
    ball_char_pos: u8, // Derived from ball_pos
    ball_is_clockwise: bool,
    ball_is_moving: bool,
    curr_score: u16,
    curr_screen: PingCurrScreen,
    paddle_hit: bool,
    paddle_released: bool,
    curr_freq: u8,
    moving_from_tap: bool,
}

impl GameState {
    const fn new_static() -> Self {
        GameState {
            ball_pos: 0,
            paddle_pos: PingPaddleState::Retracted,
            ball_char_pos: 0,
            ball_is_clockwise: false,
            ball_is_moving: false,
            curr_score: 0,
            curr_screen: PingCurrScreen::Title,
            paddle_hit: false,
            paddle_released: false,
            curr_freq: 0,
            moving_from_tap: false,
        }
    }
}

/// The PING face state.
pub struct PingFace {
    /// The persistent state (the C `ping_state_t`).
    hi_score: u16,
    difficulty: u8,
    month_last_hi_score: u8,
    year_last_hi_score: u8,
    sound_on: bool,
    tap_control_on: bool,
    /// The transient game state.
    game: GameState,
    /// Ticks remaining before the title screen switches to the score screen.
    ticks_show_title: i8,
    /// Whether the LCD is a custom one (unused in this port; always classic).
    is_custom_lcd: bool,
}

impl PingFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        PingFace {
            hi_score: 0,
            difficulty: PingDifficulty::Norm as u8,
            month_last_hi_score: 0,
            year_last_hi_score: 0,
            sound_on: false,
            tap_control_on: false,
            game: GameState::new_static(),
            ticks_show_title: 0,
            is_custom_lcd: false,
        }
    }

    pub fn new() -> Self {
        PingFace::new_static()
    }

    fn ball_pos_to_char_pos(ball_pos: u8) -> u8 {
        match ball_pos {
            5 | 6 => 4,
            4 | 7 => 5,
            3 | 8 => 6,
            2 | 9 => 7,
            1 | 10 => 8,
            0 | 11 => 9,
            _ => BALL_OFF_SCREEN,
        }
    }

    fn paddle_and_ball_on_same_segment(&self) -> bool {
        match self.game.paddle_pos {
            PingPaddleState::Extended => {
                if self.game.ball_pos == 9 || self.game.ball_pos == 2 {
                    return true;
                }
            }
            PingPaddleState::Extending | PingPaddleState::Retracting => {
                if self.game.ball_pos == 10 || self.game.ball_pos == 1 {
                    return true;
                }
            }
            PingPaddleState::Retracted => {
                if self.game.ball_pos == 11 || self.game.ball_pos == 0 {
                    return true;
                }
            }
        }
        false
    }

    fn paddle_hit_ball(&self) -> bool {
        match self.game.paddle_pos {
            PingPaddleState::Extended => {
                if self.game.ball_pos >= 9 && self.game.ball_is_clockwise {
                    return true;
                }
                if self.game.ball_pos <= 2 && !self.game.ball_is_clockwise {
                    return true;
                }
            }
            PingPaddleState::Extending => {
                if self.game.ball_pos >= 10 && self.game.ball_is_clockwise {
                    return true;
                }
                if self.game.ball_pos <= 1 && !self.game.ball_is_clockwise {
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn get_next_ball_pos(&mut self, ball_hit: bool, difficulty: u8) -> u8 {
        let offset_next: i8;
        if ball_hit {
            let ball_on_top = self.game.ball_pos > 5;
            self.game.ball_is_clockwise = !ball_on_top;
            // ball is at the same frame as the paddle
            match self.game.paddle_pos {
                PingPaddleState::Extended => return if ball_on_top { 9 } else { 2 },
                PingPaddleState::Extending => return if ball_on_top { 10 } else { 1 },
                _ => {}
            }
        }
        if self.game.ball_is_clockwise {
            offset_next = 1;
        } else {
            offset_next = -1;
        }
        let mut next_pos = self.game.ball_pos as i8 + offset_next;
        if next_pos > BALL_POS_MAX as i8 || next_pos < 0 {
            return BALL_OFF_SCREEN;
        }
        if difficulty == PingDifficulty::Hard as u8 {
            if next_pos == 4 {
                next_pos = 8;
            } else if next_pos == 7 {
                next_pos = 3;
            }
        }
        next_pos as u8
    }

    fn display_ball(&self) {
        let char_pos = Self::ball_pos_to_char_pos(self.game.ball_pos);
        let char_display: u8;
        let overlap = self.paddle_and_ball_on_same_segment();
        if self.game.ball_pos > 5 {
            if overlap {
                char_display = b'q';
            } else {
                char_display = b'#';
            }
        } else {
            if !self.is_custom_lcd && (char_pos == 4 || char_pos == 6) {
                char_display = b'n'; // No need to check for overlap on these segments
            } else {
                if overlap {
                    char_display = b'd';
                } else {
                    char_display = b'o';
                }
            }
        }
        slcd::display_character(char_display, char_pos);
    }

    fn update_ball(&mut self, difficulty: u8) -> PingResult {
        let ball_hit = self.paddle_hit_ball();
        let mut first_hit = false;
        if !self.game.ball_is_moving {
            if ball_hit {
                self.game.ball_is_moving = true;
                first_hit = true;
            } else {
                return PingResult::None;
            }
        }
        self.game.ball_pos = self.get_next_ball_pos(ball_hit, difficulty);
        if self.game.ball_pos == BALL_OFF_SCREEN {
            return PingResult::Lose;
        }
        self.display_ball();
        if ball_hit {
            if first_hit {
                PingResult::FirstHit
            } else {
                PingResult::Hit
            }
        } else {
            PingResult::None
        }
    }

    fn display_paddle(&self) {
        match self.game.paddle_pos {
            PingPaddleState::Extending | PingPaddleState::Retracting => {
                slcd::display_character(b'-', 9);
                slcd::display_character(b'1', 8);
            }
            PingPaddleState::Extended => {
                slcd::display_character(b'-', 9);
                slcd::display_character(b'-', 8);
                slcd::display_character(b'1', 7);
            }
            PingPaddleState::Retracted => {
                slcd::display_character(b'1', 9);
            }
        }
    }

    fn update_paddle(&mut self) {
        match self.game.paddle_pos {
            PingPaddleState::Retracted => {
                if self.game.paddle_hit {
                    self.game.paddle_pos = PingPaddleState::Extending;
                }
            }
            PingPaddleState::Extending => {
                if !self.game.moving_from_tap
                    && !watch::gpio::get_pin_level(watch::extint::BTN_ALARM)
                {
                    self.game.paddle_pos = PingPaddleState::Retracted;
                    slcd::display_character(b' ', 8);
                    self.game.moving_from_tap = false;
                } else {
                    self.game.paddle_pos = PingPaddleState::Extended;
                }
            }
            PingPaddleState::Extended => {
                self.game.paddle_pos = PingPaddleState::Retracting;
                slcd::display_character(b' ', 7);
            }
            PingPaddleState::Retracting => {
                self.game.paddle_pos = PingPaddleState::Retracted;
                slcd::display_character(b' ', 8);
                self.game.moving_from_tap = false;
            }
        }
        self.game.paddle_hit = false;
        self.display_paddle();
    }

    fn display_score(&self, score: u16) {
        let score = score % (MAX_DISP_SCORE + 1);
        let mut buf = [0u8; 3];
        buf[0] = b' ';
        buf[1] = b'0' + (score / 10) as u8;
        buf[2] = b'0' + (score % 10) as u8;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("  "), 2);
    }

    fn add_to_score(&mut self) {
        if self.game.curr_score <= MAX_HI_SCORE {
            self.game.curr_score += 1;
            if self.game.curr_score > self.hi_score {
                self.hi_score = self.game.curr_score;
            }
        }
        self.display_score(self.game.curr_score);
    }

    fn check_and_reset_hi_score(&mut self) {
        // Resets the hi score at the beginning of each month.
        let date_time = movement::get_local_date_time();
        if self.year_last_hi_score != date_time.year || self.month_last_hi_score != date_time.month
        {
            // The high score resets itself every new month.
            self.hi_score = 0;
            self.year_last_hi_score = date_time.year;
            self.month_last_hi_score = date_time.month;
        }
    }

    fn display_difficulty(&self, difficulty: u8) {
        let label = match difficulty {
            d if d == PingDifficulty::Baby as u8 => " b",
            d if d == PingDifficulty::Easy as u8 => " E",
            d if d == PingDifficulty::Norm as u8 => " N",
            _ => " H",
        };
        slcd::display_string(label, 2);
    }

    fn change_difficulty(&mut self) {
        self.difficulty = (self.difficulty + 1) % PingDifficulty::Count as u8;
        self.display_difficulty(self.difficulty);
        if self.sound_on {
            if self.difficulty == 0 {
                movement::play_note(Note::B4, 0);
            } else {
                movement::play_note(Note::C5, 0);
            }
        }
    }

    fn display_sound_indicator(&self, sound_on: bool) {
        if sound_on {
            slcd::set_indicator(slcd::Indicator::Bell);
        } else {
            slcd::clear_indicator(slcd::Indicator::Bell);
        }
    }

    fn toggle_sound(&mut self) {
        self.sound_on = !self.sound_on;
        self.display_sound_indicator(self.sound_on);
        if self.sound_on {
            movement::play_note(Note::C5, 0);
        }
    }

    fn enable_tap_control(&mut self) {
        // Tap detection is not ported to Rust; the paddle is controlled with
        // the ALARM button only.
        self.tap_control_on = false;
    }

    fn disable_tap_control(&mut self) {
        self.tap_control_on = false;
    }

    fn display_title(&mut self) {
        movement::request_tick_frequency(1);
        self.game.curr_screen = PingCurrScreen::Title;
        slcd::clear_colon();
        slcd::display_string("Ping", 0);
        slcd::display_string(" Ping ", 4);
        self.display_sound_indicator(self.sound_on);
        self.ticks_show_title = 1;
    }

    fn display_score_screen(&mut self) {
        let hi_score = self.hi_score;
        let difficulty = self.difficulty;
        movement::request_tick_frequency(1);
        let sound_on = self.sound_on;
        self.game = GameState::new_static();
        self.game.curr_screen = PingCurrScreen::Score;
        slcd::set_colon();
        slcd::display_string("PI  ", 0);
        if hi_score > MAX_HI_SCORE {
            slcd::display_string("HS  --", 4);
        } else {
            // "HS" followed by the 4-digit high score.
            let mut buf = [0u8; 6];
            buf[0] = b'H';
            buf[1] = b'S';
            buf[2] = b'0' + (hi_score / 1000 % 10) as u8;
            buf[3] = b'0' + (hi_score / 100 % 10) as u8;
            buf[4] = b'0' + (hi_score / 10 % 10) as u8;
            buf[5] = b'0' + (hi_score % 10) as u8;
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("      "), 4);
        }
        self.display_difficulty(difficulty);
        self.display_sound_indicator(sound_on);
    }

    fn begin_playing(&mut self) {
        self.game.curr_screen = PingCurrScreen::Playing;
        slcd::clear_colon();
        self.display_sound_indicator(self.sound_on);
        self.game.curr_freq = match self.difficulty {
            d if d == PingDifficulty::Baby as u8 => FREQ_BABY,
            d if d == PingDifficulty::Easy as u8 => FREQ_EASY,
            _ => FREQ_NORM,
        };
        movement::request_tick_frequency(self.game.curr_freq);
        slcd::display_string("  ", 2);
        slcd::display_string("      ", 4);
        self.game.paddle_pos = PingPaddleState::Retracted;
        self.game.ball_pos = 1;
        self.game.paddle_hit = false;
        self.game.ball_is_moving = false;
        self.game.ball_is_clockwise = false;
        self.game.curr_score = 0;
        self.display_paddle();
        self.display_ball();
        self.display_score(self.game.curr_score);
    }

    fn display_lose_screen(&mut self) {
        self.game.curr_screen = PingCurrScreen::Lose;
        self.game.curr_score = 0;
        slcd::clear_display();
        slcd::display_string(" LOSE ", 4);
        if self.sound_on {
            movement::play_sequence(LOSE_TUNE.as_ptr(), None);
        }
    }

    fn update_game(&mut self) {
        if self.game.ball_is_moving {
            slcd::display_character(b' ', Self::ball_pos_to_char_pos(self.game.ball_pos));
        }
        self.update_paddle();
        let game_result = self.update_ball(self.difficulty);
        if game_result == PingResult::Lose {
            self.display_lose_screen();
        } else if game_result == PingResult::Hit {
            self.add_to_score();
            if self.sound_on {
                movement::play_note(Note::C5, 0);
            }
        } else if game_result == PingResult::FirstHit && self.sound_on {
            movement::play_sequence(START_TUNE.as_ptr(), None);
        }
    }
}

impl WatchFace for PingFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {
        self.difficulty = PingDifficulty::Norm as u8;
        self.tap_control_on = false;
    }

    fn activate(&mut self, _settings: &Settings) {
        self.is_custom_lcd = false;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => {
                self.disable_tap_control();
                self.check_and_reset_hi_score();
                self.display_title();
            }
            Event::Tick => match self.game.curr_screen {
                PingCurrScreen::Title => {
                    if self.ticks_show_title > 0 {
                        self.ticks_show_title -= 1;
                    } else {
                        slcd::clear_display();
                        self.display_score_screen();
                    }
                }
                PingCurrScreen::Score | PingCurrScreen::Lose => {}
                PingCurrScreen::Playing => self.update_game(),
                PingCurrScreen::Count => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::Up)
            | Event::Button(Button::Light, ButtonEvent::Up) => match self.game.curr_screen {
                PingCurrScreen::Score => {
                    self.enable_tap_control();
                    self.begin_playing();
                }
                PingCurrScreen::Title => {
                    self.enable_tap_control();
                    slcd::clear_display();
                    self.display_score_screen();
                }
                PingCurrScreen::Lose => {
                    slcd::clear_display();
                    self.display_score_screen();
                }
                PingCurrScreen::Playing | PingCurrScreen::Count => {}
            },
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.game.curr_screen == PingCurrScreen::Score {
                    self.change_difficulty();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Down) => {
                if self.game.curr_screen == PingCurrScreen::Playing {
                    self.game.moving_from_tap = false;
                    self.game.paddle_hit = true;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.game.curr_screen == PingCurrScreen::Title
                    || self.game.curr_screen == PingCurrScreen::Score
                {
                    self.toggle_sound();
                }
            }
            Event::BackgroundTask => {
                self.disable_tap_control();
                if self.game.curr_screen != PingCurrScreen::Score {
                    self.display_score_screen();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.disable_tap_control();
    }
}
