//! Simple coin flip watch face.
//!
//! Port of the C `simple_coin_flip_face.c`. Flips a coin with a short
//! animation. It is a pure state machine: it reacts to a single event and
//! returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::slcd;

/// The coin flip face state.
pub struct SimpleCoinFlipFace {
    animation_frame: u8,
}

impl SimpleCoinFlipFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        SimpleCoinFlipFace { animation_frame: 0 }
    }

    pub fn new() -> Self {
        SimpleCoinFlipFace::new_static()
    }

    fn get_random(max: u32) -> u32 {
        let now = crate::watch::rtc::get_date_time();
        let mut x = now.to_reg();
        if x == 0 {
            x = 0xDEAD_BEEF;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x % max
    }

    fn animation_0() {
        slcd::display_string("  ", 8);
        slcd::set_pixel(0, 3);
        slcd::set_pixel(0, 6);
    }

    fn animation_1() {
        slcd::display_string("  ", 8);
        slcd::set_pixel(1, 3);
        slcd::set_pixel(1, 5);
    }

    fn animation_2() {
        slcd::display_string("  ", 8);
        slcd::set_pixel(2, 2);
        slcd::set_pixel(2, 4);
    }
}

impl WatchFace for SimpleCoinFlipFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        slcd::display_string("flip", 5);
        self.animation_frame = 0;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => {
                slcd::display_string("flip", 5);
                self.animation_frame = 0;
            }
            Event::Tick => {
                match self.animation_frame {
                    0 | 7 => return,
                    1 => {
                        slcd::display_string("      ", 4);
                        Self::animation_0();
                    }
                    2 | 4 => Self::animation_1(),
                    3 => Self::animation_2(),
                    5 => Self::animation_0(),
                    6 => {
                        if Self::get_random(2) != 0 {
                            slcd::display_string("Heads ", 4);
                        } else {
                            slcd::display_string(" Tails", 4);
                        }
                    }
                    _ => {}
                }
                self.animation_frame += 1;
            }
            Event::Button(Button::Light, ButtonEvent::Up)
            | Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.animation_frame == 0 {
                    self.animation_frame = 1;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress)
            | Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.animation_frame = 1;
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
