//! Toss-up (coins and dice) watch face.
//!
//! Port of the C `toss_up_face.c`. Tosses coins or rolls dice with a pixel
//! animation. It is a pure state machine: it reacts to a single event and
//! returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::slcd;

const HEADS: [u8; 5] = [b'8', b'H', b'B', b'C', b'D'];
const TAILS: [u8; 5] = [b'0', b'T', b'E', b'F', b'G'];
const DD: [u8; 13] = [2, 4, 6, 8, 10, 12, 20, 24, 30, 50, 60, 100, 120];

/// The toss-up face state.
pub struct TossUpFace {
    mode: u8,
    animate: bool,
    animation: u8,
    coin_num: u8,
    dice_num: u8,
    dice_sides: [u8; 3],
    coin_style: [u8; 2],
    coinface: u8,
    dd: u8,
    coins: [bool; 6],
    dice: [u8; 3],
}

impl TossUpFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        TossUpFace {
            mode: 0,
            animate: false,
            animation: 0,
            coin_num: 1,
            dice_num: 1,
            dice_sides: [6, 6, 6],
            coin_style: [b'8', b'0'],
            coinface: 0,
            dd: 0,
            coins: [false; 6],
            dice: [0; 3],
        }
    }

    pub fn new() -> Self {
        TossUpFace::new_static()
    }

    fn get_true_entropy(&self) -> u32 {
        // A simple xorshift PRNG seeded from the RTC time.
        let now = crate::watch::rtc::get_date_time();
        let mut x = now.to_reg();
        if x == 0 {
            x = 0x9E37_79B9;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x & 0x7FFF_FFFF
    }

    fn divine_bit(&self) -> u8 {
        let mut stalks;
        loop {
            stalks = self.get_true_entropy();
            if stalks <= 0x7FFF_FFFF && stalks > 0 {
                break;
            }
        }
        let mut pile1_xor = 0u8;
        let mut pile2_xor = 0u8;
        for i in 0..16 {
            let left_bit = ((stalks >> (31 - 2 * i)) & 1) as u8;
            let right_bit = ((stalks >> (30 - 2 * i)) & 1) as u8;
            if i % 2 == 0 {
                pile1_xor ^= left_bit;
                pile2_xor ^= right_bit;
            } else {
                pile1_xor ^= right_bit;
                pile2_xor ^= left_bit;
            }
        }
        pile1_xor ^ pile2_xor
    }

    fn roll_dice(&self, sides: u8) -> u8 {
        let mut bits_needed = 0u8;
        let mut temp_sides = sides - 1;
        while temp_sides > 0 {
            bits_needed += 1;
            temp_sides >>= 1;
        }
        let mut result;
        loop {
            result = 0;
            for _ in 0..bits_needed {
                result <<= 1;
                result |= self.divine_bit();
            }
            if result <= sides - 1 {
                break;
            }
        }
        result + 1
    }

    fn sort_coins(&self, token: &mut [u8; 7], num_bits: u8, bits: u8) {
        let mut num_ones = 0;
        let mut idx = 0;
        for i in 0..num_bits {
            if ((bits >> i) & 1) != 0 {
                token[idx] = self.coin_style[0];
                idx += 1;
                num_ones += 1;
            }
        }
        if num_bits < 6 {
            for _ in 0..(6 - num_bits) {
                token[idx] = b' ';
                idx += 1;
            }
        }
        for _ in 0..(num_bits - num_ones) {
            token[idx] = self.coin_style[1];
            idx += 1;
        }
    }

    fn display_coins(&self, token: &mut [u8; 7]) {
        let mut bits = 0u8;
        for i in 0..self.coin_num {
            if self.coins[i as usize] {
                bits |= 1 << (self.coin_num - 1 - i);
            }
        }
        self.sort_coins(token, self.coin_num, bits);
    }

    fn coin_animation(&mut self) {
        let mut heads = false;
        let mut tails = false;
        for i in 0..self.coin_num {
            if self.coins[i as usize] {
                heads = true;
            } else {
                tails = true;
            }
        }
        match self.animation {
            0 => {
                slcd::display_string("      ", 4);
                if heads {
                    slcd::set_pixel(0, 18);
                    slcd::set_pixel(2, 18);
                } else {
                    self.animation = 12;
                }
            }
            1 => {
                if heads {
                    slcd::set_pixel(1, 18);
                }
            }
            2 => {
                if heads {
                    slcd::set_pixel(0, 19);
                    slcd::set_pixel(2, 19);
                }
            }
            3 => {
                if heads {
                    slcd::clear_pixel(0, 18);
                    slcd::clear_pixel(2, 18);
                }
            }
            4 => {
                if heads {
                    slcd::clear_pixel(1, 18);
                }
            }
            5 => {
                if heads {
                    slcd::clear_pixel(0, 19);
                    slcd::clear_pixel(2, 19);
                    slcd::set_pixel(1, 17);
                    slcd::set_pixel(0, 20);
                }
            }
            6 => {
                if heads {
                    slcd::set_pixel(2, 20);
                    slcd::set_pixel(0, 21);
                }
            }
            7 => {
                if heads {
                    slcd::set_pixel(1, 21);
                    slcd::set_pixel(2, 21);
                }
            }
            8 => {
                if heads {
                    slcd::clear_pixel(1, 17);
                    slcd::clear_pixel(0, 20);
                }
            }
            9 => {
                if heads {
                    slcd::clear_pixel(2, 20);
                    slcd::clear_pixel(0, 21);
                }
            }
            10 => {
                if heads {
                    slcd::clear_pixel(1, 21);
                    slcd::clear_pixel(2, 21);
                    slcd::set_pixel(1, 22);
                    slcd::set_pixel(2, 22);
                }
            }
            11 => {
                if heads {
                    slcd::set_pixel(0, 22);
                }
            }
            12 => {
                if heads {
                    slcd::set_pixel(2, 23);
                    slcd::set_pixel(0, 23);
                }
                if tails {
                    slcd::set_pixel(0, 18);
                    slcd::set_pixel(2, 18);
                }
            }
            13 => {
                if heads {
                    slcd::clear_pixel(1, 22);
                    slcd::clear_pixel(2, 22);
                }
                if tails {
                    slcd::set_pixel(1, 18);
                }
            }
            14 => {
                if heads {
                    slcd::clear_pixel(0, 22);
                }
                if tails {
                    slcd::set_pixel(0, 19);
                    slcd::set_pixel(2, 19);
                }
            }
            15 => {
                if heads {
                    slcd::clear_pixel(2, 23);
                    slcd::clear_pixel(0, 23);
                    slcd::set_pixel(2, 0);
                    slcd::set_pixel(1, 0);
                }
                if tails {
                    slcd::clear_pixel(0, 18);
                    slcd::clear_pixel(2, 18);
                }
            }
            16 => {
                if heads {
                    slcd::set_pixel(2, 1);
                    slcd::set_pixel(0, 0);
                }
                if tails {
                    slcd::clear_pixel(1, 18);
                }
            }
            17 => {
                if heads {
                    slcd::set_pixel(2, 10);
                    slcd::set_pixel(0, 1);
                }
                if tails {
                    slcd::clear_pixel(0, 19);
                    slcd::clear_pixel(2, 19);
                    slcd::set_pixel(1, 17);
                    slcd::set_pixel(0, 20);
                }
            }
            18 => {
                if heads {
                    slcd::clear_pixel(2, 0);
                    slcd::clear_pixel(1, 0);
                }
                if tails {
                    slcd::set_pixel(2, 20);
                    slcd::set_pixel(0, 21);
                }
            }
            19 => {
                if heads {
                    slcd::clear_pixel(2, 1);
                    slcd::clear_pixel(0, 0);
                }
                if tails {
                    slcd::set_pixel(1, 21);
                    slcd::set_pixel(2, 21);
                }
            }
            20 => {
                if heads {
                    slcd::set_pixel(2, 1);
                    slcd::set_pixel(0, 0);
                }
                if tails {
                    slcd::clear_pixel(1, 17);
                    slcd::clear_pixel(0, 20);
                }
            }
            21 => {
                if heads {
                    slcd::set_pixel(2, 0);
                    slcd::set_pixel(1, 0);
                }
                if tails {
                    slcd::clear_pixel(2, 20);
                    slcd::clear_pixel(0, 21);
                }
            }
            22 => {
                if heads {
                    slcd::clear_pixel(2, 10);
                    slcd::clear_pixel(0, 1);
                }
                if tails {
                    slcd::clear_pixel(1, 21);
                    slcd::clear_pixel(2, 21);
                    slcd::set_pixel(1, 22);
                    slcd::set_pixel(2, 22);
                }
            }
            23 => {
                if heads {
                    slcd::clear_pixel(2, 1);
                    slcd::clear_pixel(0, 0);
                }
                if tails {
                    slcd::set_pixel(0, 22);
                }
            }
            24 => {
                if heads {
                    slcd::set_pixel(2, 23);
                    slcd::set_pixel(0, 23);
                    slcd::clear_pixel(2, 0);
                    slcd::clear_pixel(1, 0);
                }
                if tails {
                    slcd::set_pixel(2, 23);
                    slcd::set_pixel(0, 23);
                }
            }
            25 => {
                if heads {
                    slcd::set_pixel(0, 22);
                }
                if tails {
                    slcd::clear_pixel(1, 22);
                    slcd::clear_pixel(2, 22);
                }
            }
            26 => {
                if heads {
                    slcd::set_pixel(1, 22);
                    slcd::set_pixel(2, 22);
                }
                if tails {
                    slcd::clear_pixel(0, 22);
                }
            }
            27 => {
                if heads {
                    slcd::clear_pixel(2, 23);
                    slcd::clear_pixel(0, 23);
                }
                if tails {
                    slcd::clear_pixel(2, 23);
                    slcd::clear_pixel(0, 23);
                    slcd::set_pixel(2, 0);
                    slcd::set_pixel(1, 0);
                }
            }
            28 => {
                if heads {
                    slcd::clear_pixel(0, 22);
                }
                if tails {
                    slcd::set_pixel(2, 1);
                    slcd::set_pixel(0, 0);
                }
            }
            29 => {
                if heads {
                    slcd::set_pixel(1, 21);
                    slcd::set_pixel(2, 21);
                    slcd::clear_pixel(1, 22);
                    slcd::clear_pixel(2, 22);
                }
                if tails {
                    slcd::set_pixel(2, 10);
                    slcd::set_pixel(0, 1);
                }
            }
            30 => {
                if heads {
                    slcd::set_pixel(2, 20);
                    slcd::set_pixel(0, 21);
                }
                if tails {
                    slcd::clear_pixel(1, 0);
                    slcd::clear_pixel(2, 0);
                }
            }
            31 => {
                if heads {
                    slcd::set_pixel(1, 17);
                    slcd::set_pixel(0, 20);
                }
                if tails {
                    slcd::clear_pixel(2, 1);
                    slcd::clear_pixel(0, 0);
                }
            }
            32 => {
                if heads {
                    slcd::clear_pixel(1, 21);
                    slcd::clear_pixel(2, 21);
                }
                if tails {
                    slcd::clear_pixel(2, 10);
                    slcd::clear_pixel(0, 1);
                    slcd::set_pixel(0, 2);
                    slcd::set_pixel(1, 2);
                }
            }
            33 => {
                if heads {
                    slcd::clear_pixel(2, 20);
                    slcd::clear_pixel(0, 21);
                }
                if tails {
                    slcd::set_pixel(2, 2);
                    slcd::set_pixel(0, 3);
                }
            }
            34 => {
                if heads {
                    slcd::set_pixel(0, 19);
                    slcd::set_pixel(2, 19);
                    slcd::clear_pixel(1, 17);
                    slcd::clear_pixel(0, 20);
                }
                if tails {
                    slcd::set_pixel(2, 3);
                    slcd::set_pixel(0, 4);
                }
            }
            35 => {
                if heads {
                    slcd::set_pixel(1, 18);
                }
                if tails {
                    slcd::clear_pixel(1, 2);
                    slcd::clear_pixel(0, 2);
                }
            }
            36 => {
                if heads {
                    slcd::set_pixel(0, 18);
                    slcd::set_pixel(2, 18);
                }
                if tails {
                    slcd::clear_pixel(2, 2);
                    slcd::clear_pixel(0, 3);
                }
            }
            37 => {
                if heads {
                    slcd::clear_pixel(0, 19);
                    slcd::clear_pixel(2, 19);
                }
                if tails {
                    slcd::clear_pixel(2, 3);
                    slcd::clear_pixel(0, 4);
                    slcd::set_pixel(1, 4);
                    slcd::set_pixel(0, 5);
                }
            }
            38 => {
                if heads {
                    slcd::clear_pixel(1, 18);
                }
                if tails {
                    slcd::set_pixel(2, 4);
                    slcd::set_pixel(0, 6);
                }
            }
            _ => {
                if heads {
                    slcd::clear_pixel(0, 18);
                    slcd::clear_pixel(2, 18);
                }
                if tails {
                    slcd::set_pixel(1, 6);
                    slcd::set_pixel(2, 5);
                }
                self.animate = false;
                self.animation = 0;
            }
        }
    }

    fn dice_animation(&mut self) {
        slcd::display_string("      ", 4);
        for i in 0..self.dice_num {
            slcd::display_string("0", i * 2 + 5);
        }
        match self.animation {
            0 => {
                slcd::clear_pixel(1, 17);
                slcd::clear_pixel(0, 0);
                slcd::clear_pixel(1, 6);
            }
            1 => {
                slcd::clear_pixel(2, 20);
                slcd::clear_pixel(1, 0);
                slcd::clear_pixel(0, 6);
            }
            2 => {
                slcd::clear_pixel(2, 21);
                slcd::clear_pixel(2, 0);
                slcd::clear_pixel(0, 5);
            }
            3 => {
                slcd::clear_pixel(1, 21);
                slcd::clear_pixel(2, 1);
                slcd::clear_pixel(1, 4);
            }
            4 => {
                slcd::clear_pixel(0, 21);
                slcd::clear_pixel(2, 10);
                slcd::clear_pixel(2, 4);
            }
            5 => {
                slcd::clear_pixel(0, 20);
                slcd::clear_pixel(0, 1);
                slcd::clear_pixel(2, 5);
            }
            6 => {
                slcd::clear_pixel(1, 17);
                slcd::clear_pixel(0, 0);
                slcd::clear_pixel(1, 6);
            }
            7 => {
                slcd::clear_pixel(2, 20);
                slcd::clear_pixel(1, 0);
                slcd::clear_pixel(0, 6);
            }
            8 => {
                slcd::clear_pixel(2, 21);
                slcd::clear_pixel(2, 0);
                slcd::clear_pixel(0, 5);
            }
            9 => {
                slcd::clear_pixel(1, 21);
                slcd::clear_pixel(2, 1);
                slcd::clear_pixel(1, 4);
            }
            10 => {
                slcd::clear_pixel(0, 21);
                slcd::clear_pixel(2, 10);
                slcd::clear_pixel(2, 4);
            }
            _ => {
                slcd::clear_pixel(0, 20);
                slcd::clear_pixel(0, 1);
                slcd::clear_pixel(2, 5);
                self.animate = false;
                self.animation = 0;
            }
        }
    }

    fn display(&mut self) {
        let mut buf = [0u8; 11];
        match self.mode {
            0 => {
                buf[0] = b' ';
                buf[1] = b' ';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b'C';
                buf[5] = b'o';
                buf[6] = b'i';
                buf[7] = b'n';
                buf[8] = b's';
                buf[9] = b' ';
            }
            1 => {
                self.coin_animation();
                if !self.animate {
                    slcd::clear_display();
                    let mut token = [0u8; 7];
                    self.display_coins(&mut token);
                    buf[0] = b' ';
                    buf[1] = b' ';
                    buf[2] = b' ';
                    buf[3] = b' ';
                    buf[4] = token[0];
                    buf[5] = token[1];
                    buf[6] = token[2];
                    buf[7] = token[3];
                    buf[8] = token[4];
                    buf[9] = token[5];
                }
            }
            2 => {
                buf[0] = b' ';
                buf[1] = b' ';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b'D';
                buf[5] = b'i';
                buf[6] = b'c';
                buf[7] = b'e';
                buf[8] = b' ';
                buf[9] = b' ';
            }
            _ => {
                self.dice_animation();
                if !self.animate {
                    buf[0] = b' ';
                    buf[1] = b' ';
                    buf[2] = b' ';
                    buf[3] = b' ';
                    for i in 0..self.dice_num {
                        let dice_result = self.dice[i as usize];
                        let tens = dice_result / 10;
                        let ones = dice_result % 10;
                        buf[4 + (i as usize) * 2] = if tens == 0 { b' ' } else { b'0' + tens };
                        buf[5 + (i as usize) * 2] = b'0' + ones;
                    }
                }
            }
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

impl WatchFace for TossUpFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => slcd::display_string("    Coins ", 0),
            Event::Tick => {
                if self.animate {
                    self.animation = self.animation.wrapping_add(1);
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
                        self.animate = true;
                        for i in 0..self.coin_num {
                            self.coins[i as usize] = self.divine_bit() != 0;
                        }
                    }
                    1 => {
                        self.animate = true;
                        for i in 0..self.coin_num {
                            self.coins[i as usize] = self.divine_bit() != 0;
                        }
                    }
                    2 => {
                        self.mode += 1;
                        self.animate = true;
                        for i in 0..self.dice_num {
                            self.dice[i as usize] = self.roll_dice(self.dice_sides[i as usize]);
                        }
                    }
                    _ => {
                        self.animate = true;
                        for i in 0..self.dice_num {
                            self.dice[i as usize] = self.roll_dice(self.dice_sides[i as usize]);
                        }
                    }
                }
                self.display();
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.animate {
                    return;
                }
                self.animate = false;
                match self.mode {
                    0 => {
                        self.coin_style[0] = HEADS[0];
                        self.coin_style[1] = TAILS[0];
                        self.coinface = 0;
                    }
                    1 => {
                        self.coinface = (self.coinface + 1) % 5;
                        self.coin_style[0] = HEADS[self.coinface as usize];
                        self.coin_style[1] = TAILS[self.coinface as usize];
                    }
                    2 => {
                        self.dice_sides = [6, 6, 6];
                        self.dd = 0;
                    }
                    _ => {
                        self.dd = (self.dd + 1) % 13;
                        self.dice_sides[(self.dice_num - 1) as usize] = DD[self.dd as usize];
                        self.dice[(self.dice_num - 1) as usize] = DD[self.dd as usize];
                    }
                }
                self.display();
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.animate {
                    return;
                }
                self.animate = false;
                match self.mode {
                    0 => self.coin_num = 1,
                    1 => self.coin_num = (self.coin_num % 6) + 1,
                    2 => self.dice_num = 1,
                    _ => {
                        self.dice_num = (self.dice_num % 3) + 1;
                        self.dd = 0;
                    }
                }
                self.display();
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
