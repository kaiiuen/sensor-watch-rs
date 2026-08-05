//! Probability (dice) watch face.
//!
//! Port of the C `probability_face.c`. Rolls a die with a selectable number of
//! sides. It is a pure state machine: it reacts to a single event and returns;
//! it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::slcd;

const DEFAULT_DICE_SIDES: u16 = 2;
const PROBABILITY_ANIMATION_TICK_FREQUENCY: u32 = 8;
const NUM_DICE_TYPES: usize = 8;
const DICE_TYPES: [u16; 8] = [2, 4, 6, 8, 10, 12, 20, 100];

/// The probability face state.
pub struct ProbabilityFace {
    dice_sides: u16,
    rolled_value: u16,
    is_rolling: bool,
    animation_frame: u8,
}

impl ProbabilityFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        ProbabilityFace {
            dice_sides: DEFAULT_DICE_SIDES,
            rolled_value: 0,
            is_rolling: false,
            animation_frame: 0,
        }
    }

    pub fn new() -> Self {
        ProbabilityFace::new_static()
    }

    fn display_dice_roll(&self) {
        let mut buf = [0u8; 8];
        if self.rolled_value == 0 {
            if self.dice_sides == 100 {
                buf[0] = b' ';
                buf[1] = b'C';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b' ';
                buf[5] = b' ';
            } else {
                buf[0] = b'0' + (self.dice_sides / 10) as u8;
                buf[1] = b'0' + (self.dice_sides % 10) as u8;
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b' ';
                buf[5] = b' ';
            }
        } else if self.dice_sides == 2 {
            buf[0] = b'0' + (self.dice_sides / 10) as u8;
            buf[1] = b'0' + (self.dice_sides % 10) as u8;
            buf[2] = b' ';
            buf[3] = b' ';
            buf[4] = b' ';
            buf[5] = if self.rolled_value == 1 { b'H' } else { b'T' };
        } else if self.dice_sides == 100 {
            buf[0] = b' ';
            buf[1] = b'C';
            buf[2] = b' ';
            buf[3] = b'0' + (self.rolled_value / 100) as u8;
            buf[4] = b'0' + ((self.rolled_value / 10) % 10) as u8;
            buf[5] = b'0' + (self.rolled_value % 10) as u8;
        } else {
            buf[0] = b'0' + (self.dice_sides / 10) as u8;
            buf[1] = b'0' + (self.dice_sides % 10) as u8;
            buf[2] = b' ';
            buf[3] = b'0' + (self.rolled_value / 100) as u8;
            buf[4] = b'0' + ((self.rolled_value / 10) % 10) as u8;
            buf[5] = b'0' + (self.rolled_value % 10) as u8;
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("      "), 4);
    }

    fn generate_random_number(&mut self) {
        // A simple xorshift PRNG seeded from the RTC time.
        let now = crate::watch::rtc::get_date_time();
        let mut x = now.to_reg();
        if x == 0 {
            x = 0x1234_5678;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rolled_value = (x % self.dice_sides as u32) as u16 + 1;
    }

    fn display_dice_roll_animation(&mut self) {
        if self.is_rolling {
            if self.animation_frame == 0 {
                slcd::display_string("   ", 7);
                slcd::set_pixel(1, 4);
                slcd::set_pixel(1, 6);
                self.animation_frame = 1;
            } else if self.animation_frame == 1 {
                slcd::clear_pixel(1, 4);
                slcd::clear_pixel(1, 6);
                slcd::set_pixel(2, 4);
                slcd::set_pixel(0, 6);
                self.animation_frame = 2;
            } else if self.animation_frame == 2 {
                slcd::clear_pixel(2, 4);
                slcd::clear_pixel(0, 6);
                slcd::set_pixel(2, 5);
                slcd::set_pixel(0, 5);
                self.animation_frame = 3;
            } else {
                self.animation_frame = 0;
                self.is_rolling = false;
                self.display_dice_roll();
            }
        }
    }
}

impl WatchFace for ProbabilityFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.dice_sides = DEFAULT_DICE_SIDES;
        self.rolled_value = 0;
        slcd::display_string("PR", 0);
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        if self.is_rolling && event != Event::Tick {
            return;
        }
        match event {
            Event::Activate => self.display_dice_roll(),
            Event::Tick => self.display_dice_roll_animation(),
            Event::Button(Button::Light, ButtonEvent::Down) => {
                for (i, &d) in DICE_TYPES.iter().enumerate() {
                    if d == self.dice_sides {
                        self.dice_sides = DICE_TYPES[(i + 1) % NUM_DICE_TYPES];
                        break;
                    }
                }
                self.rolled_value = 0;
                self.display_dice_roll();
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.generate_random_number();
                self.is_rolling = true;
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
