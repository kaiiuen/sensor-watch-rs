//! Blackjack watch face.
//!
//! Port of the C `blackjack_face.c`. A simple blackjack game. It is a pure
//! state machine: it reacts to a single event and returns; it never keeps the
//! CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::rtc;
use crate::watch::slcd;
use crate::watch::slcd::Indicator;

const ACE: u8 = 14;
const KING: u8 = 13;
const QUEEN: u8 = 12;
const JACK: u8 = 11;

const MIN_CARD_VALUE: u8 = 2;
const MAX_CARD_VALUE: u8 = ACE;
const CARD_RANK_COUNT: u8 = MAX_CARD_VALUE - MIN_CARD_VALUE + 1;
const CARD_SUIT_COUNT: u8 = 4;
const DECK_SIZE: u8 = CARD_SUIT_COUNT * CARD_RANK_COUNT;

const BLACKJACK_MAX_HAND_SIZE: usize = 11; // 4*1 + 4*2 + 3*3 = 21; 11 cards total
const MAX_PLAYER_CARDS_DISPLAY: u8 = 4;
const BOARD_DISPLAY_START: u8 = 4;

/// The game state.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GameState {
    TitleScreen,
    Playing,
    DealerPlaying,
    Bust,
    Result,
    WinRatio,
}

/// A hand of cards.
#[derive(Clone, Copy)]
struct HandInfo {
    score: u8,
    idx_hand: u8,
    high_aces_in_hand: i8,
    hand: [u8; BLACKJACK_MAX_HAND_SIZE],
}

impl HandInfo {
    const fn new() -> Self {
        HandInfo {
            score: 0,
            idx_hand: 0,
            high_aces_in_hand: 0,
            hand: [0; BLACKJACK_MAX_HAND_SIZE],
        }
    }
}

/// The blackjack face state.
pub struct BlackjackFace {
    tap_control_on: bool,
    games_played: u16,
    games_won: u16,
    game_state: GameState,
    deck: [u8; DECK_SIZE as usize],
    current_card: u8,
    player: HandInfo,
    dealer: HandInfo,
}

impl BlackjackFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        BlackjackFace {
            tap_control_on: false,
            games_played: 0,
            games_won: 0,
            game_state: GameState::TitleScreen,
            deck: [0; DECK_SIZE as usize],
            current_card: 0,
            player: HandInfo::new(),
            dealer: HandInfo::new(),
        }
    }

    pub fn new() -> Self {
        BlackjackFace::new_static()
    }

    fn generate_random_number(&self, num_values: u8) -> u8 {
        let now = rtc::get_date_time();
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
        // Randomize shuffle with Fisher Yates.
        let mut i = DECK_SIZE - 1;
        while i > 0 {
            let j = self.generate_random_number(0xFF) % (i + 1);
            self.deck.swap(j as usize, i as usize);
            i -= 1;
        }
    }

    fn reset_deck(&mut self) {
        self.current_card = 0;
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

    fn get_card_value(card: u8) -> u8 {
        match card {
            ACE => 11,
            KING | QUEEN | JACK => 10,
            _ => card,
        }
    }

    fn modify_score_from_aces(hand_info: &mut HandInfo) {
        while hand_info.score > 21 && hand_info.high_aces_in_hand > 0 {
            hand_info.score -= 10;
            hand_info.high_aces_in_hand -= 1;
        }
    }

    fn reset_hands(&mut self) {
        self.player = HandInfo::new();
        self.dealer = HandInfo::new();
        self.reset_deck();
    }

    fn give_card(&mut self, hand_info: &mut HandInfo) {
        let card = self.get_next_card();
        if card == ACE {
            hand_info.high_aces_in_hand += 1;
        }
        hand_info.hand[hand_info.idx_hand as usize] = card;
        hand_info.idx_hand += 1;
        let card_value = Self::get_card_value(card);
        hand_info.score += card_value;
        Self::modify_score_from_aces(hand_info);
    }

    fn set_segment_at_position(&self, segment: u8, position: u8) {
        // Classic LCD segment mapping for the given position, indexed by
        // segment A=0, B=1, C=2, D=3, E=4, F=5, G=6.
        const CLASSIC_MAPPING: [[(u8, u8); 7]; 10] = [
            [
                (0, 13),
                (1, 13),
                (2, 13),
                (2, 15),
                (2, 14),
                (0, 14),
                (1, 15),
            ],
            [
                (0, 11),
                (1, 11),
                (1, 11),
                (2, 11),
                (1, 12),
                (1, 12),
                (2, 12),
            ],
            [(1, 9), (0, 9), (2, 9), (1, 9), (0, 10), (0, 0), (1, 9)],
            [(0, 7), (1, 7), (2, 7), (2, 6), (2, 8), (0, 8), (1, 8)],
            [
                (1, 18),
                (2, 19),
                (0, 19),
                (1, 18),
                (0, 18),
                (2, 18),
                (1, 19),
            ],
            [
                (2, 20),
                (2, 21),
                (1, 21),
                (0, 21),
                (0, 20),
                (1, 17),
                (1, 20),
            ],
            [
                (0, 22),
                (2, 23),
                (0, 23),
                (0, 22),
                (1, 22),
                (2, 22),
                (1, 23),
            ],
            [(2, 1), (2, 10), (0, 1), (0, 0), (1, 0), (2, 0), (1, 1)],
            [(2, 2), (2, 3), (0, 4), (0, 3), (0, 2), (1, 2), (1, 3)],
            [(2, 4), (2, 5), (1, 6), (0, 6), (0, 5), (1, 4), (1, 5)],
        ];
        let (com, seg) = CLASSIC_MAPPING[position as usize][segment as usize];
        slcd::set_pixel(com, seg);
    }

    fn display_card_at_position(&self, card: u8, display_position: u8) {
        match card {
            KING => {
                slcd::display_character(b' ', display_position);
                self.set_segment_at_position(0, display_position); // A
                self.set_segment_at_position(3, display_position); // D
                self.set_segment_at_position(6, display_position); // G
            }
            QUEEN => {
                slcd::display_character(b' ', display_position);
                self.set_segment_at_position(0, display_position); // A
                self.set_segment_at_position(3, display_position); // D
            }
            JACK => {
                slcd::display_character(b'-', display_position);
            }
            ACE => {
                slcd::display_character(b'a', display_position);
            }
            10 => {
                slcd::display_character(b'0', display_position);
            }
            _ => {
                slcd::display_character(card + b'0', display_position);
            }
        }
    }

    fn display_player_hand(&self) {
        if self.player.idx_hand <= MAX_PLAYER_CARDS_DISPLAY {
            let card = self.player.hand[(self.player.idx_hand - 1) as usize];
            self.display_card_at_position(card, BOARD_DISPLAY_START + self.player.idx_hand - 1);
        } else {
            for i in 0..MAX_PLAYER_CARDS_DISPLAY {
                let card = self.player.hand
                    [(self.player.idx_hand - MAX_PLAYER_CARDS_DISPLAY + i) as usize];
                self.display_card_at_position(card, BOARD_DISPLAY_START + i);
            }
        }
    }

    fn display_dealer_hand(&self) {
        let card = self.dealer.hand[(self.dealer.idx_hand - 1) as usize];
        self.display_card_at_position(card, 0);
    }

    fn display_score(&self, score: u8, position: u8) {
        let mut buf = [b' '; 2];
        buf[0] = b'0' + score / 10;
        buf[1] = b'0' + score % 10;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), position);
    }

    fn add_to_game_scores(&mut self, add_to_wins: bool) {
        self.games_played += 1;
        if self.games_played == 0 {
            // Overflow.
            self.games_played = 1;
            self.games_won = if add_to_wins { 1 } else { 0 };
            return;
        }
        if add_to_wins {
            self.games_won += 1;
            if self.games_won == 0 {
                // Overflow.
                self.games_played = 1;
                self.games_won = 1;
            }
        }
    }

    fn display_win(&mut self) {
        self.game_state = GameState::Result;
        self.add_to_game_scores(true);
        slcd::display_string(" WIN", 4);
        self.display_score(self.player.score, 8);
        self.display_score(self.dealer.score, 2);
    }

    fn display_lose(&mut self) {
        self.game_state = GameState::Result;
        self.add_to_game_scores(false);
        slcd::display_string("lOSE", 4);
        self.display_score(self.player.score, 8);
        self.display_score(self.dealer.score, 2);
    }

    fn display_tie(&mut self) {
        self.game_state = GameState::Result;
        // Don't record ties to the win ratio.
        slcd::display_string(" TIE", 4);
        self.display_score(self.player.score, 8);
    }

    fn display_bust(&mut self) {
        self.game_state = GameState::Result;
        self.add_to_game_scores(false);
        slcd::display_string("BUST", 4);
    }

    fn display_title(&mut self) {
        self.game_state = GameState::TitleScreen;
        slcd::display_string("  ", 2);
        slcd::display_string("21", 0);
        slcd::display_string("BLaKJK", 4);
    }

    fn display_win_ratio(&mut self) {
        self.game_state = GameState::WinRatio;
        let mut win_ratio = 0u8;
        if self.games_played > 0 {
            // Avoid dividing by zero.
            win_ratio = ((100 * self.games_won).checked_div(self.games_played)).unwrap_or(0) as u8;
        }
        slcd::display_string("  ", 2);
        slcd::display_string("WR", 0);
        let mut buf = [b' '; 6];
        buf[0] = b'0' + (win_ratio / 100) % 10;
        buf[1] = b'0' + (win_ratio / 10) % 10;
        buf[2] = b'0' + win_ratio % 10;
        buf[3] = b'P';
        buf[4] = b'c';
        buf[5] = b't';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
    }

    fn begin_playing(&mut self, tap_control_on: bool) {
        slcd::clear_display();
        if tap_control_on {
            slcd::set_indicator(Indicator::Signal);
        }
        self.game_state = GameState::Playing;
        self.reset_hands();
        // Give player their first 2 cards.
        let mut player = self.player;
        self.give_card(&mut player);
        self.player = player;
        self.display_player_hand();
        let mut player = self.player;
        self.give_card(&mut player);
        self.player = player;
        self.display_player_hand();
        self.display_score(self.player.score, 8);
        let mut dealer = self.dealer;
        self.give_card(&mut dealer);
        self.dealer = dealer;
        self.display_dealer_hand();
        self.display_score(self.dealer.score, 2);
    }

    fn perform_stand(&mut self) {
        self.game_state = GameState::DealerPlaying;
        slcd::display_string("Stnd", 4);
        self.display_score(self.player.score, 8);
    }

    fn perform_hit(&mut self) {
        if self.player.score == 21 {
            self.perform_stand();
            return; // Assume hitting on 21 is a mistake and stand.
        }
        let mut player = self.player;
        self.give_card(&mut player);
        self.player = player;
        if self.player.score > 21 {
            self.game_state = GameState::Bust;
        }
        self.display_player_hand();
        self.display_score(self.player.score, 8);
    }

    fn dealer_performs_hits(&mut self) {
        let mut dealer = self.dealer;
        self.give_card(&mut dealer);
        self.dealer = dealer;
        self.display_dealer_hand();
        if self.dealer.score > 21 {
            self.display_win();
        } else if self.dealer.score > self.player.score {
            self.display_lose();
        } else {
            self.display_dealer_hand();
            self.display_score(self.dealer.score, 2);
        }
    }

    fn see_if_dealer_hits(&mut self) {
        if self.dealer.score > 16 {
            if self.dealer.score > self.player.score {
                self.display_lose();
            } else if self.dealer.score < self.player.score {
                self.display_win();
            } else {
                self.display_tie();
            }
        } else {
            self.dealer_performs_hits();
        }
    }

    fn handle_button_presses(&mut self, tap_control_on: bool, hit: bool) {
        match self.game_state {
            GameState::TitleScreen => {
                self.begin_playing(tap_control_on);
            }
            GameState::Playing => {
                if hit {
                    self.perform_hit();
                } else {
                    self.perform_stand();
                }
            }
            GameState::DealerPlaying => self.see_if_dealer_hits(),
            GameState::Bust => self.display_bust(),
            GameState::Result | GameState::WinRatio => self.display_title(),
        }
    }
}

impl WatchFace for BlackjackFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {
        self.tap_control_on = false;
    }

    fn activate(&mut self, _settings: &Settings) {
        self.display_title();
        self.stack_deck();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => {
                if self.tap_control_on {
                    slcd::set_indicator(Indicator::Signal);
                }
            }
            Event::Tick => {
                if self.game_state == GameState::DealerPlaying {
                    self.see_if_dealer_hits();
                } else if self.game_state == GameState::Bust {
                    self.display_bust();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.handle_button_presses(self.tap_control_on, false);
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.handle_button_presses(self.tap_control_on, true);
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.game_state == GameState::TitleScreen {
                    self.display_win_ratio();
                } else {
                    movement::illuminate_led();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.game_state == GameState::TitleScreen {
                    // Toggle tap control.
                    self.tap_control_on = movement::enable_tap_detection_if_available();
                    if self.tap_control_on {
                        slcd::set_indicator(Indicator::Signal);
                    }
                } else if self.game_state == GameState::WinRatio {
                    // Reset the win-lose ratio.
                    self.games_won = 0;
                    self.games_played = 0;
                    slcd::display_string("  0Pct", 4);
                }
            }
            // A tap acts as a hit when tap control is on.
            Event::SingleTap | Event::DoubleTap => {
                if self.tap_control_on {
                    self.handle_button_presses(self.tap_control_on, true);
                }
            }
            Event::BackgroundTask => {}
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        if self.tap_control_on {
            movement::disable_tap_detection_if_available();
        }
        self.tap_control_on = false;
    }
}
