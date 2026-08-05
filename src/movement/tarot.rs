//! Tarot watch face.
//!
//! Port of the C `tarot_face.c`. Draws tarot cards (major arcana only or full
//! deck) with a shuffle animation. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::slcd::Indicator;

const TAROT_ANIMATION_TICK_FREQUENCY: u32 = 8;
const FLIPPED_MASK: u8 = 1 << 7;

const MAJOR_ARCANA: [&str; 22] = [
    " FOOL ", "Mgcian", "HPrsts", "En&prs", "En&por", "Hiroph", "Lovers", "Chriot", "Strgth",
    "Hrn&it", " Frtun", "Justce", "Hangn&", " Death", " tmprn", " Devil", " Tower", "  Star",
    "n&OON ", "  Sun ", "Jdgmnt", " World",
];
const SUITS: [&str; 4] = [" wands", "  cups", "swords", " coins"];
const NUM_MAJOR_ARCANA: u8 = 22;
const NUM_CARDS_PER_SUIT: u8 = 14;
const NUM_TAROT_CARDS: u8 = 78;

/// The tarot face state.
pub struct TarotFace {
    drawn_cards: [u8; 10],
    current_card: u8,
    num_cards_to_draw: u8,
    major_arcana_only: bool,
    is_picking: bool,
    animation_frame: u8,
}

impl TarotFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        TarotFace {
            drawn_cards: [0xFF; 10],
            current_card: 0,
            num_cards_to_draw: 3,
            major_arcana_only: true,
            is_picking: false,
            animation_frame: 0,
        }
    }

    pub fn new() -> Self {
        TarotFace::new_static()
    }

    fn init_deck(&mut self) {
        self.drawn_cards = [0xFF; 10];
        self.current_card = 0;
    }

    fn get_rand_num(&self, num_values: u8) -> u8 {
        let now = crate::watch::rtc::get_date_time();
        let mut x = now.to_reg();
        if x == 0 {
            x = 0x1357_9BDF;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        (x % num_values as u32) as u8
    }

    fn draw_one_card(&self) -> u8 {
        if self.major_arcana_only {
            self.get_rand_num(NUM_MAJOR_ARCANA)
        } else {
            self.get_rand_num(NUM_TAROT_CARDS)
        }
    }

    fn already_drawn(&self, drawn_card: u8) -> bool {
        for i in 0..self.num_cards_to_draw as usize {
            if self.drawn_cards[i] == 0xFF {
                break;
            }
            if (self.drawn_cards[i] & !FLIPPED_MASK) == drawn_card {
                return true;
            }
        }
        false
    }

    fn pick_cards(&mut self) {
        for i in 0..self.num_cards_to_draw as usize {
            let mut card = self.draw_one_card();
            while self.already_drawn(card) {
                card = self.draw_one_card();
            }
            card |= self.get_rand_num(2) << 7;
            self.drawn_cards[i] = card;
        }
    }

    fn display(&self) {
        let mut buf = [0u8; 11];
        if self.drawn_cards[0] == 0xFF {
            watch::slcd::clear_indicator(Indicator::Signal);
            buf[0] = b'T';
            buf[1] = b'A';
            buf[2] = b'0' + self.num_cards_to_draw / 10;
            buf[3] = b'0' + self.num_cards_to_draw % 10;
            if self.major_arcana_only {
                buf[4] = b'n';
                buf[5] = b'&';
                buf[6] = b'a';
                buf[7] = b'j';
                buf[8] = b'o';
                buf[9] = b'r';
            } else {
                buf[4] = b' ';
                buf[5] = b' ';
                buf[6] = b' ';
                buf[7] = b'A';
                buf[8] = b'l';
                buf[9] = b'l';
            }
            watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            return;
        }

        let start_end = if self.current_card == 0 {
            "St"
        } else if self.current_card == self.num_cards_to_draw - 1 {
            "En"
        } else {
            "  "
        };
        let se = start_end.as_bytes();
        buf[0] = se[0];
        buf[1] = se[1];

        let mut card = self.drawn_cards[self.current_card as usize];
        let flipped = card & FLIPPED_MASK != 0;
        card &= !FLIPPED_MASK;
        if card < NUM_MAJOR_ARCANA {
            let name = MAJOR_ARCANA[card as usize].as_bytes();
            buf[2] = b' ';
            buf[3] = b' ';
            for (i, &c) in name.iter().take(6).enumerate() {
                buf[4 + i] = c;
            }
        } else {
            let suit = (card - NUM_MAJOR_ARCANA) / NUM_CARDS_PER_SUIT;
            let rank = ((card - NUM_MAJOR_ARCANA) % NUM_CARDS_PER_SUIT) + 1;
            buf[2] = b'0' + rank / 10;
            buf[3] = b'0' + rank % 10;
            let s = SUITS[suit as usize].as_bytes();
            for (i, &c) in s.iter().take(6).enumerate() {
                buf[4 + i] = c;
            }
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);

        if flipped {
            watch::slcd::set_indicator(Indicator::Signal);
        } else {
            watch::slcd::clear_indicator(Indicator::Signal);
        }
    }

    fn display_animation(&mut self) {
        if self.animation_frame == 0 {
            watch::slcd::display_string("   ", 7);
            watch::slcd::set_pixel(1, 4);
            watch::slcd::set_pixel(1, 6);
            self.animation_frame = 1;
        } else if self.animation_frame == 1 {
            watch::slcd::clear_pixel(1, 4);
            watch::slcd::clear_pixel(1, 6);
            watch::slcd::set_pixel(2, 4);
            watch::slcd::set_pixel(0, 6);
            self.animation_frame = 2;
        } else if self.animation_frame == 2 {
            watch::slcd::clear_pixel(2, 4);
            watch::slcd::clear_pixel(0, 6);
            watch::slcd::set_pixel(2, 5);
            watch::slcd::set_pixel(0, 5);
            self.animation_frame = 3;
        } else {
            self.animation_frame = 0;
            self.is_picking = false;
            self.display();
        }
    }
}

impl WatchFace for TarotFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        watch::slcd::display_string("TA", 0);
        self.init_deck();
        self.num_cards_to_draw = 3;
        self.major_arcana_only = true;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        if self.is_picking && event != Event::Tick {
            return;
        }
        match event {
            Event::Activate => self.display(),
            Event::Tick => {
                if self.is_picking {
                    self.display_animation();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.drawn_cards[0] == 0xFF {
                    self.num_cards_to_draw += 1;
                    if self.num_cards_to_draw > 10 {
                        self.num_cards_to_draw = 3;
                    }
                } else {
                    self.current_card = (self.current_card + 1) % self.num_cards_to_draw;
                }
                self.display();
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.drawn_cards[0] == 0xFF {
                    self.major_arcana_only = !self.major_arcana_only;
                } else {
                    self.init_deck();
                }
                self.display();
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                watch::slcd::display_string("      ", 4);
                watch::slcd::clear_indicator(Indicator::Signal);
                self.init_deck();
                self.pick_cards();
                self.is_picking = true;
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
