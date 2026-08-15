//! Geomancy / I Ching watch face.
//!
//! Port of the C `geomancy_face.c`. Casts I Ching hexagrams and geomantic
//! figures from random bits. It is a pure state machine: it reacts to a single
//! event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::slcd;

/// The Bagua trigrams encoded as 3-bit tribbles.
const BAGUA: u32 = 0b00000101001110010111011100000000;

/// The King Wen Sequence of hexagrams.
const WEN_ORDER: [u8; 64] = [
    1, 22, 7, 19, 15, 34, 44, 11, 14, 51, 38, 52, 61, 55, 30, 32, 6, 3, 28, 58, 39, 63, 46, 5, 45,
    17, 47, 56, 31, 49, 27, 43, 23, 26, 2, 41, 50, 20, 16, 24, 35, 21, 62, 36, 54, 29, 48, 12, 18,
    40, 59, 60, 53, 37, 57, 9, 10, 25, 4, 8, 33, 13, 42, 0,
];

/// The geomantic figures encoded as 4-bit nibbles.
const GEOMANTIC: u64 = 0x4ABF39D25E76C180;

const FIGURES: [&str; 16] = [
    "VI", "Hd", "PA", "GF", "PR", "AQ", "CA", "TR", "Td", "CO", "AM", "AL", "LF", "RU", "LA", "PO",
];

const THROW_ANIMATION_FREQUENCY: u8 = 16;

/// The geomancy face state.
pub struct GeomancyFace {
    mode: u8,
    animate: bool,
    animation: u8,
    caption: bool,
    i_ching_hexagram: u8,
    geomantic_figure: u8,
}

impl GeomancyFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        GeomancyFace {
            mode: 0,
            animate: false,
            animation: 0,
            caption: false,
            i_ching_hexagram: 0,
            geomantic_figure: 0,
        }
    }

    pub fn new() -> Self {
        GeomancyFace::new_static()
    }

    fn divine_bit(&self) -> u8 {
        let now = crate::watch::rtc::get_date_time();
        let mut x = now.to_reg();
        if x == 0 {
            x = 0x1234_5678;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        (x & 1) as u8
    }

    fn iching_pick_trigram(&self) -> u8 {
        let index = (self.divine_bit() << 2) | (self.divine_bit() << 1) | self.divine_bit();
        ((BAGUA >> (3 * index)) & 0b111) as u8
    }

    fn iching_form_hexagram(&self) -> u8 {
        let inner = self.iching_pick_trigram();
        let outer = self.iching_pick_trigram();
        (inner << 3) | outer
    }

    fn geomancy_pick_figure(&self) -> u8 {
        let index = (self.divine_bit() << 3)
            | (self.divine_bit() << 2)
            | (self.divine_bit() << 1)
            | self.divine_bit();
        ((GEOMANTIC >> (4 * (15 - index))) & 0xF) as u8
    }

    fn display_hexagram(&self, hexagram: u8) {
        let mut buf = [0u8; 7];
        for i in 0..6 {
            if hexagram & (1 << (5 - i)) != 0 {
                buf[i] = b'1';
            } else {
                buf[i] = b'=';
            }
        }
        slcd::display_string(core::str::from_utf8(&buf[..6]).unwrap_or(""), 4);
        for i in 0..6 {
            if hexagram & (1 << (5 - i)) == 0 {
                if i == 1 {
                    slcd::set_pixel(2, 20);
                }
                if i == 3 {
                    slcd::set_pixel(2, 1);
                }
                if i == 4 {
                    slcd::set_pixel(2, 2);
                }
                if i == 5 {
                    slcd::set_pixel(2, 4);
                }
            }
        }
    }

    fn geomancy_display(&self, code: u8) {
        let row1 = (code >> 3) & 1 != 0;
        let row2 = (code >> 2) & 1 != 0;
        let row3 = (code >> 1) & 1 != 0;
        let row4 = code & 1 != 0;
        if row1 {
            slcd::set_pixel(1, 18);
        } else {
            slcd::set_pixel(1, 19);
        }
        if row2 {
            slcd::set_pixel(2, 20);
            slcd::set_pixel(0, 21);
        } else {
            slcd::set_pixel(1, 20);
        }
        if row3 {
            slcd::set_pixel(0, 22);
        } else {
            slcd::set_pixel(1, 23);
        }
        if row4 {
            slcd::set_pixel(2, 1);
            slcd::set_pixel(0, 0);
        } else {
            slcd::set_pixel(1, 1);
        }
    }

    fn set_throw_animation(&mut self, animate: bool) {
        self.animate = animate;
        self.animation = 0;
    }

    fn start_throw_animation(&mut self) {
        self.set_throw_animation(true);
        movement::request_tick_frequency(THROW_ANIMATION_FREQUENCY);
    }

    fn finish_throw_animation(&mut self) {
        self.set_throw_animation(false);
        movement::request_tick_frequency(1);
    }

    fn throw_animation(&mut self) {
        match self.animation {
            0 => slcd::set_pixel(0, 22),
            1 => {
                slcd::set_pixel(2, 22);
                slcd::set_pixel(2, 23);
                slcd::clear_pixel(0, 22);
            }
            2 => {
                slcd::set_pixel(1, 22);
                slcd::set_pixel(0, 23);
            }
            3 => {
                slcd::set_pixel(2, 0);
                slcd::set_pixel(1, 0);
                slcd::set_pixel(2, 21);
                slcd::set_pixel(1, 21);
                slcd::clear_pixel(2, 22);
                slcd::clear_pixel(1, 22);
                slcd::clear_pixel(2, 23);
                slcd::clear_pixel(0, 23);
                slcd::clear_pixel(1, 23);
            }
            4 => {
                slcd::set_pixel(1, 17);
                slcd::set_pixel(0, 20);
                slcd::set_pixel(2, 10);
                slcd::set_pixel(0, 1);
            }
            5 => {
                slcd::clear_pixel(2, 21);
                slcd::clear_pixel(1, 21);
                slcd::clear_pixel(2, 0);
                slcd::clear_pixel(1, 0);
                slcd::clear_pixel(1, 20);
                slcd::clear_pixel(2, 20);
                slcd::clear_pixel(0, 21);
                slcd::clear_pixel(1, 1);
                slcd::clear_pixel(0, 0);
                slcd::clear_pixel(2, 1);
                slcd::set_pixel(2, 19);
                slcd::set_pixel(0, 19);
                slcd::set_pixel(1, 2);
                slcd::set_pixel(0, 2);
            }
            6 => {
                slcd::clear_pixel(1, 17);
                slcd::clear_pixel(0, 20);
                slcd::clear_pixel(2, 10);
                slcd::clear_pixel(0, 1);
                slcd::set_pixel(2, 18);
                slcd::set_pixel(0, 18);
                slcd::set_pixel(2, 3);
                slcd::set_pixel(0, 4);
            }
            7 => {
                slcd::clear_pixel(2, 19);
                slcd::clear_pixel(0, 19);
                slcd::clear_pixel(1, 18);
                slcd::clear_pixel(1, 19);
                slcd::clear_pixel(1, 2);
                slcd::clear_pixel(0, 2);
                slcd::clear_pixel(1, 3);
                slcd::clear_pixel(0, 3);
                slcd::clear_pixel(2, 2);
                slcd::set_pixel(1, 4);
                slcd::set_pixel(0, 5);
            }
            8 => {
                slcd::clear_pixel(2, 18);
                slcd::clear_pixel(0, 18);
                slcd::clear_pixel(2, 3);
                slcd::clear_pixel(0, 4);
                slcd::set_pixel(2, 5);
                slcd::set_pixel(1, 6);
            }
            9 => {
                slcd::clear_pixel(1, 4);
                slcd::clear_pixel(0, 5);
                slcd::clear_pixel(1, 5);
                slcd::clear_pixel(2, 4);
                slcd::clear_pixel(0, 6);
            }
            10 => {
                slcd::clear_pixel(2, 5);
                slcd::clear_pixel(1, 6);
            }
            _ => self.finish_throw_animation(),
        }
    }

    fn display(&mut self) {
        match self.mode {
            0 => slcd::display_string("    IChing", 0),
            1 => {
                self.throw_animation();
                if !self.animate {
                    self.display_hexagram(self.i_ching_hexagram);
                    if self.caption {
                        let mut buf = [0u8; 3];
                        let n = WEN_ORDER[self.i_ching_hexagram as usize] + 1;
                        buf[0] = b'0' + n / 10;
                        buf[1] = b'0' + n % 10;
                        slcd::display_string(core::str::from_utf8(&buf[..2]).unwrap_or("  "), 2);
                    }
                }
            }
            2 => slcd::display_string("    GeomCy", 0),
            _ => {
                self.throw_animation();
                if !self.animate {
                    if self.caption {
                        let f = FIGURES[self.geomantic_figure as usize].as_bytes();
                        let mut buf = [0u8; 3];
                        buf[0] = f[0];
                        buf[1] = f[1];
                        slcd::display_string(core::str::from_utf8(&buf[..2]).unwrap_or("  "), 0);
                    }
                    self.geomancy_display(self.geomantic_figure);
                }
            }
        }
    }
}

impl WatchFace for GeomancyFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.finish_throw_animation();
        slcd::display_string("    IChing", 0);
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => {
                self.finish_throw_animation();
                slcd::display_string("    IChing", 0);
            }
            Event::Tick => {
                if self.animate {
                    self.animation = (self.animation + 1) % 39;
                    self.display();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.animate {
                    return;
                }
                if self.mode <= 1 {
                    self.mode = 2;
                } else if self.mode >= 2 {
                    self.mode = 0;
                }
                self.display();
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.animate {
                    return;
                }
                match self.mode {
                    0 => {
                        self.mode += 1;
                        self.start_throw_animation();
                        self.i_ching_hexagram = self.iching_form_hexagram();
                    }
                    1 => {
                        self.start_throw_animation();
                        self.i_ching_hexagram = self.iching_form_hexagram();
                    }
                    2 => {
                        self.mode += 1;
                        self.start_throw_animation();
                        self.geomantic_figure = self.geomancy_pick_figure();
                    }
                    _ => {
                        self.start_throw_animation();
                        self.geomantic_figure = self.geomancy_pick_figure();
                    }
                }
                self.display();
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.animate {
                    return;
                }
                self.caption = !self.caption;
                slcd::display_string("    ", 0);
                self.display();
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.finish_throw_animation();
    }
}

#[cfg(test)]
mod tests {
    use super::GeomancyFace;

    #[test]
    fn throw_animation_state_resets_at_completion() {
        let mut face = GeomancyFace::new_static();

        // The host movement seam intentionally exposes only the 1 Hz versus
        // non-1 Hz distinction and cannot observe the requested 16 Hz value.
        face.set_throw_animation(true);
        assert!(face.animate);
        assert_eq!(face.animation, 0);

        face.animation = 11;
        face.set_throw_animation(false);
        assert!(!face.animate);
        assert_eq!(face.animation, 0);
    }
}
