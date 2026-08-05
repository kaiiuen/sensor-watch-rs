//! Endless runner watch face.
//!
//! Port of the C `endless_runner_face.c`. A jump-over-obstacles game with
//! several difficulty modes. It is a pure state machine: it reacts to a single
//! event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;

const NUM_GRID: u32 = 12;
const FREQ: u32 = 8;
const FREQ_SLOW: u32 = 4;
const JUMP_FRAMES: u8 = 2;
const JUMP_FRAMES_EASY: u8 = 3;
const MIN_ZEROES: u8 = 4;
const MIN_ZEROES_HARD: u8 = 3;
const MAX_HI_SCORE: u16 = 9999;
const MAX_DISP_SCORE: u8 = 39;
const JUMP_FRAMES_FUEL: u8 = 30;
const JUMP_FRAMES_FUEL_RECHARGE: u8 = 3;
const MAX_DISP_SCORE_FUEL: u8 = 9;

const DIFF_BABY: u8 = 0;
const DIFF_EASY: u8 = 1;
const DIFF_NORM: u8 = 2;
const DIFF_HARD: u8 = 3;
const DIFF_FUEL: u8 = 4;
const DIFF_FUEL_1: u8 = 5;
const DIFF_COUNT: u8 = 6;

const JUMPING_FINAL_FRAME: u8 = 0;
const NOT_JUMPING: u8 = 1;
const JUMPING_START: u8 = 2;

const SCREEN_TITLE: u8 = 0;
const SCREEN_PLAYING: u8 = 1;
const SCREEN_LOSE: u8 = 2;
const SCREEN_TIME: u8 = 3;

const NUM_BITS: u32 = 32;

/// The endless runner face state.
pub struct EndlessRunnerFace {
    difficulty: u8,
    sound_on: bool,
    hi_score: u16,
    year_last_hi_score: u8,
    month_last_hi_score: u8,
    obst_pattern: u32,
    obst_indx: u8,
    jump_state: u8,
    sec_before_moves: u8,
    curr_score: u16,
    curr_screen: u8,
    loc_2_on: bool,
    loc_3_on: bool,
    success_jump: bool,
    fuel_mode: bool,
    fuel: u8,
}

impl EndlessRunnerFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        EndlessRunnerFace {
            difficulty: DIFF_NORM,
            sound_on: true,
            hi_score: 0,
            year_last_hi_score: 0,
            month_last_hi_score: 0,
            obst_pattern: 0,
            obst_indx: 0,
            jump_state: NOT_JUMPING,
            sec_before_moves: 1,
            curr_score: 0,
            curr_screen: SCREEN_TITLE,
            loc_2_on: false,
            loc_3_on: false,
            success_jump: false,
            fuel_mode: false,
            fuel: 0,
        }
    }

    pub fn new() -> Self {
        EndlessRunnerFace::new_static()
    }

    fn get_random(&self, max: u32) -> u32 {
        let now = rtc::get_date_time();
        let mut x = now.to_reg();
        if x == 0 {
            x = 0xDEAD_BEEF;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x % max
    }

    fn get_random_nonzero(&self, max: u32) -> u32 {
        loop {
            let r = self.get_random(max);
            if r != 0 {
                return r;
            }
        }
    }

    fn get_random_kinda_nonzero(&self, max: u32) -> u32 {
        if max == 0 {
            0
        } else if max == 1 {
            self.get_random(max)
        } else {
            self.get_random_nonzero(max)
        }
    }

    fn get_random_fuel(&self, prev_val: u32) -> u32 {
        let mut prev_rand_subset = 0u8;
        let mut rand_legal = 0u32;
        let prev_val = prev_val & !0xFFFF;
        for i in 0..2 {
            let mut subset = 0u8;
            let mut max_ones = 8u8;
            if prev_rand_subset > 4 {
                max_ones -= prev_rand_subset;
            }
            let mut rand = self.get_random_kinda_nonzero(max_ones as u32) as u8;
            if rand > 5 && prev_rand_subset != 0 {
                rand = 5;
            }
            for j in 0..rand {
                subset |= 1 << j;
            }
            if prev_rand_subset >= 7 {
                subset <<= 1;
            }
            subset &= 0xFF;
            rand_legal |= (subset as u32) << (8 * i);
            prev_rand_subset = rand;
        }
        prev_val | rand_legal
    }

    fn get_random_legal(&self, prev_val: u32, difficulty: u8) -> u32 {
        let min_zeros = if difficulty == DIFF_HARD {
            MIN_ZEROES_HARD
        } else {
            MIN_ZEROES
        };
        let max = (1 << (NUM_BITS - NUM_GRID)) - 1;
        let rand = self.get_random_nonzero(max);
        let mut rand_legal = 0u32;
        let prev_val = prev_val & !max;

        let mut i = NUM_GRID + 1;
        while i <= NUM_BITS {
            let mask = 1 << (NUM_BITS - i);
            let msb = (rand & mask) >> (NUM_BITS - i);
            if msb != 0 {
                rand_legal <<= min_zeros;
                i += min_zeros as u32;
            }
            rand_legal |= msb;
            rand_legal <<= 1;
            i += 1;
        }
        rand_legal &= max;
        for i in 0..=min_zeros {
            if prev_val & (1 << (i as u32 + NUM_BITS - NUM_GRID)) != 0 {
                rand_legal >>= (min_zeros - i);
                break;
            }
        }
        prev_val | rand_legal
    }

    fn display_ball(&self, jumping: bool) {
        if !jumping {
            watch::slcd::set_pixel(0, 21);
            watch::slcd::set_pixel(1, 21);
            watch::slcd::set_pixel(0, 20);
            watch::slcd::set_pixel(1, 20);
            watch::slcd::clear_pixel(1, 17);
            watch::slcd::clear_pixel(2, 20);
            watch::slcd::clear_pixel(2, 21);
        } else {
            watch::slcd::clear_pixel(0, 21);
            watch::slcd::clear_pixel(1, 21);
            watch::slcd::clear_pixel(0, 20);
            watch::slcd::set_pixel(1, 20);
            watch::slcd::set_pixel(1, 17);
            watch::slcd::set_pixel(2, 20);
            watch::slcd::set_pixel(2, 21);
        }
    }

    fn display_score(&self, score: u8) {
        let mut buf = [0u8; 3];
        if self.fuel_mode {
            let s = score % (MAX_DISP_SCORE_FUEL + 1);
            buf[0] = b'0' + s;
            watch::slcd::display_string(core::str::from_utf8(&buf[..1]).unwrap_or(" "), 0);
        } else {
            let s = score % (MAX_DISP_SCORE + 1);
            buf[0] = b'0' + s / 10;
            buf[1] = b'0' + s % 10;
            watch::slcd::display_string(core::str::from_utf8(&buf[..2]).unwrap_or("  "), 2);
        }
    }

    fn add_to_score(&mut self) {
        if self.curr_score <= MAX_HI_SCORE {
            self.curr_score += 1;
            if self.curr_score > self.hi_score {
                self.hi_score = self.curr_score;
            }
        }
        self.success_jump = true;
        self.display_score(self.curr_score as u8);
    }

    fn display_fuel(&self, subsecond: u8) {
        if self.difficulty == DIFF_FUEL_1 && self.fuel == 0 && subsecond % (FREQ as u8 / 2) == 0 {
            watch::slcd::display_string("  ", 2);
            return;
        }
        let mut buf = [0u8; 3];
        buf[0] = b'0' + self.fuel / 10;
        buf[1] = b'0' + self.fuel % 10;
        watch::slcd::display_string(core::str::from_utf8(&buf[..2]).unwrap_or("  "), 2);
    }

    fn check_and_reset_hi_score(&mut self) {
        let date_time = rtc::get_date_time();
        if self.year_last_hi_score != date_time.year || self.month_last_hi_score != date_time.month
        {
            self.hi_score = 0;
            self.year_last_hi_score = date_time.year;
            self.month_last_hi_score = date_time.month;
        }
    }

    fn display_difficulty(&mut self, difficulty: u8) {
        match difficulty {
            DIFF_BABY => watch::slcd::display_string(" b", 2),
            DIFF_EASY => watch::slcd::display_string(" E", 2),
            DIFF_HARD => watch::slcd::display_string(" H", 2),
            DIFF_FUEL => watch::slcd::display_string(" F", 2),
            DIFF_FUEL_1 => watch::slcd::display_string("1F", 2),
            _ => watch::slcd::display_string(" N", 2),
        }
        self.fuel_mode = difficulty >= DIFF_FUEL && difficulty <= DIFF_FUEL_1;
    }

    fn change_difficulty(&mut self) {
        self.difficulty = (self.difficulty + 1) % DIFF_COUNT;
        self.display_difficulty(self.difficulty);
        if self.sound_on {
            crate::movement::play_alarm_beeps(
                1,
                if self.difficulty == 0 {
                    Note::B4
                } else {
                    Note::C5
                },
            );
        }
    }

    fn toggle_sound(&mut self) {
        self.sound_on = !self.sound_on;
        if self.sound_on {
            crate::movement::play_alarm_beeps(1, Note::C5);
            watch::slcd::set_indicator(Indicator::Bell);
        } else {
            watch::slcd::clear_indicator(Indicator::Bell);
        }
    }

    fn display_title(&mut self) {
        let hi_score = self.hi_score;
        let difficulty = self.difficulty;
        let sound_on = self.sound_on;
        self.curr_screen = SCREEN_TITLE;
        self.obst_pattern = 0;
        self.obst_indx = 0;
        self.jump_state = NOT_JUMPING;
        self.sec_before_moves = 1;
        self.curr_score = 0;
        self.loc_2_on = false;
        self.loc_3_on = false;
        self.success_jump = false;
        self.fuel = 0;
        if sound_on {
            self.sec_before_moves -= 1;
        }
        watch::slcd::set_colon();
        let mut buf = [0u8; 11];
        buf[0] = b'E';
        buf[1] = b'R';
        buf[2] = b' ';
        buf[3] = b' ';
        if hi_score > MAX_HI_SCORE {
            buf[4] = b'H';
            buf[5] = b'S';
            buf[6] = b' ';
            buf[7] = b' ';
            buf[8] = b'-';
            buf[9] = b'-';
        } else {
            buf[4] = b'H';
            buf[5] = b'S';
            buf[6] = b' ';
            buf[7] = b'0' + ((hi_score / 1000) % 10) as u8;
            buf[8] = b'0' + ((hi_score / 100) % 10) as u8;
            buf[9] = b'0' + ((hi_score / 10) % 10) as u8;
            buf[10] = b'0' + (hi_score % 10) as u8;
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        self.display_difficulty(difficulty);
    }

    fn begin_playing(&mut self) {
        let difficulty = self.difficulty;
        self.curr_screen = SCREEN_PLAYING;
        watch::slcd::clear_colon();
        if self.fuel_mode {
            watch::slcd::display_string("           ", 0);
            self.obst_pattern = self.get_random_fuel(0);
            if (16 * JUMP_FRAMES_FUEL_RECHARGE) < JUMP_FRAMES_FUEL {
                self.fuel = JUMP_FRAMES_FUEL - (16 * JUMP_FRAMES_FUEL_RECHARGE);
            }
            if self.fuel < JUMP_FRAMES_FUEL_RECHARGE {
                self.fuel = JUMP_FRAMES_FUEL_RECHARGE;
            }
        } else {
            watch::slcd::display_string("         ", 2);
            self.obst_pattern = self.get_random_legal(0, difficulty);
        }
        self.jump_state = NOT_JUMPING;
        self.display_ball(self.jump_state != NOT_JUMPING);
        self.display_score(self.curr_score as u8);
        if self.sound_on {
            crate::movement::play_alarm_beeps(1, Note::C5);
            crate::movement::play_alarm_beeps(1, Note::E5);
            crate::movement::play_alarm_beeps(1, Note::G5);
        }
    }

    fn display_lose_screen(&mut self) {
        self.curr_screen = SCREEN_LOSE;
        self.curr_score = 0;
        watch::slcd::display_string("     LOSE ", 0);
        if self.sound_on {
            crate::movement::play_alarm_beeps(1, Note::A1);
        }
    }

    fn display_obstacle(&mut self, obstacle: bool, grid_loc: u32) {
        let mut prev_obst_pos_two = false;
        match grid_loc {
            2 => {
                self.loc_2_on = obstacle;
                if obstacle {
                    watch::slcd::set_pixel(0, 20);
                } else if self.jump_state != NOT_JUMPING {
                    watch::slcd::clear_pixel(0, 20);
                    if self.fuel_mode && prev_obst_pos_two {
                        self.add_to_score();
                    }
                }
                prev_obst_pos_two = obstacle;
            }
            3 => {
                self.loc_3_on = obstacle;
                if obstacle {
                    watch::slcd::set_pixel(1, 21);
                } else if self.jump_state != NOT_JUMPING {
                    watch::slcd::clear_pixel(1, 21);
                }
            }
            1 => {
                if !self.fuel_mode && obstacle {
                    self.add_to_score();
                }
                if obstacle {
                    watch::slcd::set_pixel(0, 18 + grid_loc as u8);
                } else {
                    watch::slcd::clear_pixel(0, 18 + grid_loc as u8);
                }
            }
            0 | 5 => {
                if obstacle {
                    watch::slcd::set_pixel(0, 18 + grid_loc as u8);
                } else {
                    watch::slcd::clear_pixel(0, 18 + grid_loc as u8);
                }
            }
            4 => {
                if obstacle {
                    watch::slcd::set_pixel(1, 22);
                } else {
                    watch::slcd::clear_pixel(1, 22);
                }
            }
            6 => {
                if obstacle {
                    watch::slcd::set_pixel(1, 0);
                } else {
                    watch::slcd::clear_pixel(1, 0);
                }
            }
            7 | 8 => {
                if obstacle {
                    watch::slcd::set_pixel(0, grid_loc as u8 - 6);
                } else {
                    watch::slcd::clear_pixel(0, grid_loc as u8 - 6);
                }
            }
            9 | 10 => {
                if obstacle {
                    watch::slcd::set_pixel(0, grid_loc as u8 - 5);
                } else {
                    watch::slcd::clear_pixel(0, grid_loc as u8 - 5);
                }
            }
            11 => {
                if obstacle {
                    watch::slcd::set_pixel(1, 6);
                } else {
                    watch::slcd::clear_pixel(1, 6);
                }
            }
            _ => {}
        }
    }

    fn stop_jumping(&mut self) {
        self.jump_state = NOT_JUMPING;
        self.display_ball(self.jump_state != NOT_JUMPING);
        if self.sound_on {
            crate::movement::play_alarm_beeps(
                1,
                if self.success_jump {
                    Note::C5
                } else {
                    Note::C3
                },
            );
        }
        self.success_jump = false;
    }

    fn display_obstacles(&mut self) {
        for i in 0..NUM_GRID {
            let mask = 1 << ((NUM_BITS - 1) - i);
            let obstacle = (self.obst_pattern & mask) >> ((NUM_BITS - 1) - i) != 0;
            self.display_obstacle(obstacle, i);
        }
        self.obst_pattern <<= 1;
        self.obst_indx += 1;
        if self.fuel_mode {
            if self.obst_indx >= (NUM_BITS / 2) as u8 {
                self.obst_indx = 0;
                self.obst_pattern = self.get_random_fuel(self.obst_pattern);
            }
        } else if self.obst_indx as u32 >= NUM_BITS - NUM_GRID {
            self.obst_indx = 0;
            self.obst_pattern = self.get_random_legal(self.obst_pattern, self.difficulty);
        }
    }

    fn update_game(&mut self, subsecond: u8) {
        if self.sec_before_moves != 0 {
            if subsecond == 0 {
                self.sec_before_moves -= 1;
            }
            return;
        }
        self.display_obstacles();
        match self.jump_state {
            NOT_JUMPING => {
                if self.fuel_mode {
                    for _ in 0..JUMP_FRAMES_FUEL_RECHARGE {
                        if self.fuel >= JUMP_FRAMES_FUEL
                            || (self.difficulty == DIFF_FUEL_1 && self.fuel == 0)
                        {
                            break;
                        }
                        self.fuel += 1;
                    }
                }
            }
            JUMPING_FINAL_FRAME => self.stop_jumping(),
            _ => {
                if self.fuel_mode {
                    if self.fuel == 0 {
                        self.jump_state = JUMPING_FINAL_FRAME;
                    } else {
                        self.fuel -= 1;
                    }
                    if !watch::gpio::get_pin_level(watch::extint::BTN_ALARM)
                        && !watch::gpio::get_pin_level(watch::extint::BTN_LIGHT)
                    {
                        self.stop_jumping();
                    }
                } else {
                    let curr_jump_frame = self.jump_state - NOT_JUMPING;
                    if curr_jump_frame >= JUMP_FRAMES_EASY
                        || (self.difficulty >= DIFF_NORM && curr_jump_frame >= JUMP_FRAMES)
                    {
                        self.jump_state = JUMPING_FINAL_FRAME;
                    } else {
                        self.jump_state += 1;
                    }
                }
            }
        }
        if self.jump_state == NOT_JUMPING && (self.loc_2_on || self.loc_3_on) {
            self.display_lose_screen();
        } else if self.fuel_mode {
            self.display_fuel(subsecond);
        }
    }
}

impl WatchFace for EndlessRunnerFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => {
                self.check_and_reset_hi_score();
                if self.sound_on {
                    watch::slcd::set_indicator(Indicator::Bell);
                }
                self.display_title();
            }
            Event::Tick => match self.curr_screen {
                SCREEN_TITLE | SCREEN_LOSE => {}
                _ => self.update_game(0),
            },
            Event::Button(Button::Light, ButtonEvent::Up)
            | Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.curr_screen == SCREEN_TITLE {
                    self.begin_playing();
                } else if self.curr_screen == SCREEN_LOSE {
                    self.display_title();
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.curr_screen == SCREEN_TITLE {
                    self.change_difficulty();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down)
            | Event::Button(Button::Alarm, ButtonEvent::Down) => {
                if self.curr_screen == SCREEN_PLAYING && self.jump_state == NOT_JUMPING {
                    if self.fuel_mode && self.fuel == 0 {
                        return;
                    }
                    self.jump_state = JUMPING_START;
                    self.display_ball(self.jump_state != NOT_JUMPING);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.curr_screen != SCREEN_PLAYING {
                    self.toggle_sound();
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
