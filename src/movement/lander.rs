//! Lander watch face.
//!
//! Port of the C `lander_face.c`. A lunar-lander style game played on the LCD.
//! Land the "Cringeworthy" safely with limited fuel, survive monsters, and
//! progress toward finding Earth. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd;
use crate::watch::storage;

const LANDER_TICK_FREQUENCY: u8 = 8;
const MONSTER_DISPLAY_TICKS: u16 = 9;
const ENGINE_THRUST: i16 = 11;

const MODE_WAITING_TO_START: u8 = 0;
const MODE_DISPLAY_SKILL_LEVEL: u8 = 1;
const MODE_PLAYING: u8 = 2;
const MODE_TOUCHDOWN_BLANK: u8 = 3;
const MODE_DISPLAY_FINAL_STATUS: u8 = 4;
const MODE_MONSTER: u8 = 5;
const MODE_FIND_EARTH_MESSAGE: u8 = 6;

const CREWS_COMPLIMENT: u8 = 13;
// Granularity is divisions per foot - height display.
const GRANUL: i32 = 40;

// Next lines are for repeat heroes only.
const PROMOTION_INTERVAL: u8 = 3;
const LEVEL_ACE: u8 = 8;
const LEVEL_STARBUCK: u8 = 11;
const HARD_EARTH_INCREMENTS: i16 = 11;
const MAX_HARD_EARTH_CHANCE: i16 = 6;

// The gory final result calculations.
const SPEED_FATALITY_ALL: i16 = 41;
const SPEED_FATALITY_NONE: i16 = 26;
const SPEED_NO_DAMAGE: i16 = 21;
const SPEED_LEVEL_INCREMENTS: i16 = 2;
const SPEED_MAJOR_CRASH: i16 = 73;
const MAJOR_CRASH_INCREMENTS: i16 = 65;
const SPEED_INJURY_NONE: i16 = 20;
const SPEED_INJURY_FULCRUM: i16 = 32;
const INJURY_FULCRUM_PROB: i16 = 65;
const FUEL_SCORE_GOOD: u16 = 145;
const FUEL_SCORE_GREAT: u16 = 131;
const FUEL_SCORE_FANTASTIC: u16 = 125;

// Joey Castillo to oversee storage allocation row.
const LANDER_STORAGE_ROW: u32 = 2;
const STORAGE_KEY_NUMBER: u8 = 110;

const DIFFICULTY_LEVELS: u8 = 3;
const DIFFICULTY_NAMES: [&str; 3] = ["NOrMAL", "HArd  ", "HArdEr"];
const MONSTER_TYPES: u8 = 4;
const MONSTER_NAMES: [&str; 4] = ["mOnStr", "6Erbil", "HAmStr", "Rabbit"];
const MONSTER_ACTIONS: u8 = 8;
const MONSTER_ACTION_STRINGS: [&str; 8] = [
    "HUn6ry", "  EAtS", "6Reedy", "annoYd", "nASty ", "SAVOry", "HO66SH", " pI66Y",
];

/// The lander face state.
pub struct LanderFace {
    height: i32,
    speed: i16, // Positive is up
    tick_counter: u16,
    fuel_start: u16,
    fuel_remaining: u16,
    fuel_tpl: u16,   // Fuel required for theoretical perfect landing
    fuel_score: u16, // 100 is perfect; higher is less perfect
    gravity: i8,     // negative downwards value
    led_enabled: bool,
    led_active: bool,
    mode: u8,
    skill_level: u8,
    ships_health: i8, // 0 thru 8. -1 = major crash
    hero_counter: u8,
    legend_counter: u8,
    difficulty_level: u8,
    reset_counter: u8,
    monster_type: u8,
    uninjured: u8,
    injured: u8,
    /// Xorshift PRNG state, seeded from the RTC at setup.
    rng_state: u32,
}

impl LanderFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        LanderFace {
            height: 0,
            speed: 0,
            tick_counter: 0,
            fuel_start: 0,
            fuel_remaining: 0,
            fuel_tpl: 0,
            fuel_score: 0,
            gravity: 0,
            led_enabled: false,
            led_active: false,
            mode: MODE_WAITING_TO_START,
            skill_level: 0,
            ships_health: 0,
            hero_counter: 0,
            legend_counter: 0,
            difficulty_level: 0,
            reset_counter: 0,
            monster_type: 0,
            uninjured: 0,
            injured: 0,
            rng_state: 0,
        }
    }

    pub fn new() -> Self {
        LanderFace::new_static()
    }

    /// Writes right-aligned decimal digits (width 1-4) into `buf[0..width]`.
    fn write_num(buf: &mut [u8], width: usize, value: u32) {
        let mut v = value;
        for i in (0..width).rev() {
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
    }

    /// A xorshift32 PRNG.
    fn next_random(&mut self) -> u32 {
        let mut x = self.rng_state;
        if x == 0 {
            x = 0xDEAD_BEEF;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng_state = x;
        x
    }

    fn gen_random_int(&mut self, lower: i16, upper: i16) -> i16 {
        let mut range = upper - lower + 1;
        if range < 2 {
            range = 2;
        }
        let ret = self.next_random() % range as u32;
        (ret as i16) + lower
    }

    fn assign_prob(
        lower_prob: u8,
        upper_prob: u8,
        lower_speed: i16,
        upper_speed: i16,
        act_speed: i16,
    ) -> u8 {
        let speed_range = (upper_speed - lower_speed) as f32;
        let speed_range = if speed_range < 1.0 { 1.0 } else { speed_range };
        let prob_range = (upper_prob - lower_prob) as f32;
        let ratio = (act_speed as f32 - lower_speed as f32) / speed_range;
        let prob_float = lower_prob as f32 + ratio * prob_range;
        let mut prob_int = (prob_float + 0.5) as i32;
        if prob_int > upper_prob as i32 {
            prob_int = upper_prob as i32;
        }
        if prob_int < lower_prob as i32 {
            prob_int = lower_prob as i32;
        }
        prob_int as u8
    }

    fn write_to_lander_eeprom(&self) {
        let output_array: [u8; 3] = [STORAGE_KEY_NUMBER, self.hero_counter, self.legend_counter];
        storage::erase(LANDER_STORAGE_ROW);
        storage::sync();
        storage::write(LANDER_STORAGE_ROW, 0, &output_array);
    }

    fn display_bottom(&self, bytes: &[u8]) {
        slcd::display_string(core::str::from_utf8(bytes).unwrap_or("      "), 4);
    }
}

impl WatchFace for LanderFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {
        // Zero the state (all fields already defaulted) and seed the RNG from
        // the RTC so each game differs.
        self.led_enabled = false;
        let now = rtc::get_date_time();
        self.rng_state = now.to_reg();
    }

    fn activate(&mut self, _settings: &Settings) {
        let mut buf = [0u8; 7];
        self.mode = MODE_WAITING_TO_START;
        self.led_active = false;
        self.reset_counter = 0;
        slcd::clear_all_indicators();

        // See if the hero_counter was ever written to EEPROM storage.
        let mut stored_data = [0u8; 3];
        storage::read(LANDER_STORAGE_ROW, 0, &mut stored_data);
        if stored_data[0] == STORAGE_KEY_NUMBER {
            self.hero_counter = stored_data[1]; // There's real data in there.
            self.legend_counter = stored_data[2];
        } else {
            self.hero_counter = 0; // Nope. Nothing there.
            self.legend_counter = 0;
            self.write_to_lander_eeprom(); // Initial EEPROM tracking data.
        }

        self.difficulty_level = self.hero_counter / PROMOTION_INTERVAL;
        if self.difficulty_level >= DIFFICULTY_LEVELS {
            self.difficulty_level = DIFFICULTY_LEVELS - 1;
        }

        // Fancy intro.
        if self.legend_counter == 0 {
            slcd::display_string("LA", 0);
        } else {
            slcd::display_string("LE", 0);
        }
        if self.hero_counter == 0 || self.hero_counter >= 40 {
            slcd::display_string("  ", 2);
        } else {
            buf[0] = b'0' + (self.hero_counter / 10) % 10;
            buf[1] = b'0' + self.hero_counter % 10;
            slcd::display_string(core::str::from_utf8(&buf[..2]).unwrap_or("  "), 2);
        }

        // Bottom line.
        if self.hero_counter >= 100 {
            buf[0] = b'S';
            buf[1] = b't';
            buf[2] = b'r';
            Self::write_num(&mut buf[3..6], 3, self.hero_counter as u32);
        } else if self.hero_counter >= 40 {
            buf[0] = b'S';
            buf[1] = b't';
            buf[2] = b'r';
            buf[3] = b'b';
            Self::write_num(&mut buf[4..6], 2, self.hero_counter as u32);
        } else if self.hero_counter >= LEVEL_STARBUCK {
            buf[..6].copy_from_slice(b"StrbUC");
        } else if self.hero_counter >= LEVEL_ACE {
            buf[..6].copy_from_slice(b" ACE  "); // This human is good
        } else if self.difficulty_level == 0 {
            buf[..6].copy_from_slice(b"      ");
        } else {
            buf[..6].copy_from_slice(DIFFICULTY_NAMES[self.difficulty_level as usize].as_bytes());
        }
        self.display_bottom(&buf[..6]);

        if self.led_enabled {
            slcd::set_indicator(slcd::Indicator::Signal);
        } else {
            slcd::clear_indicator(slcd::Indicator::Signal);
        }
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        let mut buf = [0u8; 7];
        match event {
            Event::Tick => {
                self.tick_counter += 1;
                if self.mode == MODE_PLAYING {
                    let mut accel = self.gravity as i16;
                    let gas_pedal_on = watch::gpio::get_pin_level(watch::extint::BTN_ALARM)
                        || watch::gpio::get_pin_level(watch::extint::BTN_LIGHT);
                    if gas_pedal_on && self.fuel_remaining > 0 {
                        accel = ENGINE_THRUST + self.gravity as i16; // Gravity is negative
                        self.fuel_remaining -= 1; // Used 1 fuel unit
                        slcd::set_indicator(slcd::Indicator::Lap);
                        // Low fuel warning indicators.
                        if self.fuel_remaining == (3 * LANDER_TICK_FREQUENCY) as u16 {
                            // 3 seconds of fuel left
                            slcd::set_indicator(slcd::Indicator::Signal);
                            slcd::set_indicator(slcd::Indicator::Bell);
                            slcd::set_indicator(slcd::Indicator::Pm);
                            slcd::set_indicator(slcd::Indicator::H24);
                        } else if self.fuel_remaining == 0 {
                            // 0 seconds of fuel left, empty!
                            slcd::clear_all_indicators();
                        }
                    } else {
                        slcd::clear_indicator(slcd::Indicator::Lap);
                    }
                    self.speed += accel;
                    self.height += self.speed as i32;
                    if self.height > 971 * 80 {
                        // Escape height
                        slcd::clear_all_indicators();
                        slcd::display_string("ESCAPE", 4);
                        self.tick_counter = 0;
                        self.mode = MODE_WAITING_TO_START;
                    } else if self.height <= 0 {
                        // Touchdown
                        self.tick_counter = 0;
                        self.mode = MODE_TOUCHDOWN_BLANK;
                    } else {
                        // Update height display
                        Self::write_num(&mut buf[0..4], 4, (self.height / GRANUL) as u32);
                        self.display_bottom(&buf[..4]);
                    }
                } else if self.mode == MODE_TOUCHDOWN_BLANK {
                    // Blank display on touchdown.
                    if self.tick_counter == 1 {
                        slcd::clear_all_indicators();
                        slcd::display_string("      ", 4);

                        // Also calc fuel score now.
                        let fuel_used = self.fuel_start - self.fuel_remaining;
                        let fuel_score_float = fuel_used as f32 / self.fuel_tpl as f32;
                        self.fuel_score = (fuel_score_float * 100.0 + 0.5) as u16;
                        if self.legend_counter == 0 {
                            self.fuel_score = self.fuel_score.wrapping_sub(8);
                        }
                        // First Earth is easier
                        // Monitor reset_counter.
                        if fuel_used >= 1 {
                            self.reset_counter = 0;
                        } else {
                            self.reset_counter += 1;
                        }
                        if self.reset_counter >= 3 {
                            self.hero_counter = 0;
                            self.difficulty_level = 0;
                            if self.reset_counter >= 6 {
                                self.legend_counter = 0;
                            }
                            slcd::display_string("rESET ", 4);
                            self.write_to_lander_eeprom();
                        }
                    }
                    // Wait until time for next display.
                    if self.tick_counter >= LANDER_TICK_FREQUENCY as u16 {
                        self.tick_counter = 0;
                        self.mode = MODE_DISPLAY_FINAL_STATUS;
                    }
                } else if self.mode == MODE_DISPLAY_FINAL_STATUS {
                    let last_pass = self.tick_counter >= LANDER_TICK_FREQUENCY as u16;

                    // Show final status.
                    if self.tick_counter == 1 {
                        let mut all_done = false;
                        let mut ships_health: i8 = 0;
                        // Easiest implementation for difficulty_level is to
                        // increase touchdown speed above actual.
                        let mut final_speed = self.speed.abs() + self.difficulty_level as i16 * 4;
                        // First Earth is a bit easier than all the others.
                        if self.legend_counter == 0 {
                            final_speed -= 2;
                        }

                        // 1) Major crash: bug, crater, vaporized (gone).
                        if final_speed >= SPEED_MAJOR_CRASH {
                            all_done = true;
                            ships_health = -1;
                            if final_speed >= (SPEED_MAJOR_CRASH + 2 * MAJOR_CRASH_INCREMENTS) {
                                buf[..6].copy_from_slice(b"GOnE  ");
                            } else if final_speed >= (SPEED_MAJOR_CRASH + MAJOR_CRASH_INCREMENTS) {
                                buf[..6].copy_from_slice(b" CrAtr");
                            } else {
                                buf[..6].copy_from_slice(b"   bU6");
                            }
                        }
                        // 2) Rank ship's health 0 to 8.
                        if !all_done {
                            let boosted_speed = final_speed + SPEED_LEVEL_INCREMENTS - 1;
                            let levels_damage =
                                (boosted_speed - SPEED_NO_DAMAGE) / SPEED_LEVEL_INCREMENTS;
                            ships_health = 8 - levels_damage as i8;
                            if ships_health > 8 {
                                ships_health = 8;
                            }
                            if ships_health < 0 {
                                ships_health = 0;
                            }
                        }
                        self.ships_health = ships_health; // Remember ships health
                        // 3) Crew fatalities and injuries.
                        if !all_done {
                            let prob_fatal = Self::assign_prob(
                                0,
                                92,
                                SPEED_FATALITY_NONE,
                                SPEED_FATALITY_ALL,
                                final_speed,
                            );
                            let prob_injury = if final_speed <= SPEED_INJURY_FULCRUM {
                                Self::assign_prob(
                                    0,
                                    INJURY_FULCRUM_PROB as u8,
                                    SPEED_INJURY_NONE,
                                    SPEED_INJURY_FULCRUM,
                                    final_speed,
                                )
                            } else {
                                Self::assign_prob(
                                    INJURY_FULCRUM_PROB as u8,
                                    96,
                                    SPEED_INJURY_FULCRUM,
                                    SPEED_FATALITY_ALL,
                                    final_speed,
                                )
                            };
                            let mut fatalities: u8 = 0;
                            self.injured = 0;
                            for _ in 0..CREWS_COMPLIMENT {
                                let my_rand = self.gen_random_int(1, 100);
                                if my_rand <= prob_fatal as i16 {
                                    fatalities += 1;
                                } else if my_rand <= prob_injury as i16 {
                                    self.injured += 1;
                                }
                            }
                            self.uninjured = CREWS_COMPLIMENT - fatalities - self.injured;
                        }
                        // 4) Special conditions: hero.
                        if !all_done {
                            if ships_health >= 8 && self.fuel_score <= FUEL_SCORE_FANTASTIC {
                                self.hero_counter += 1;
                                if self.hero_counter == 1 {
                                    buf[..6].copy_from_slice(b"HErO  ");
                                } else if self.hero_counter == LEVEL_ACE {
                                    buf[..6].copy_from_slice(b" ACE  ");
                                } else if self.hero_counter == LEVEL_STARBUCK {
                                    buf[..6].copy_from_slice(b"STrbUC");
                                } else if self.hero_counter > 99 {
                                    buf[..3].copy_from_slice(b"HEr");
                                    Self::write_num(&mut buf[3..6], 3, self.hero_counter as u32);
                                } else {
                                    buf[..4].copy_from_slice(b"HErO");
                                    Self::write_num(&mut buf[4..6], 2, self.hero_counter as u32);
                                }
                                all_done = true;
                                // Two rule sets for finding Earth. Alternate
                                // between easy and hard.
                                let mut my_odds: i16;
                                if self.legend_counter % 2 == 0 {
                                    my_odds = self.hero_counter as i16 - LEVEL_STARBUCK as i16;
                                } else {
                                    let temp = (self.hero_counter as i16 - LEVEL_STARBUCK as i16)
                                        + HARD_EARTH_INCREMENTS
                                        - 1;
                                    my_odds = temp / HARD_EARTH_INCREMENTS;
                                    if my_odds > MAX_HARD_EARTH_CHANCE {
                                        my_odds = MAX_HARD_EARTH_CHANCE;
                                    }
                                }
                                // Display odds in weekday region if positive value.
                                if my_odds > 0 {
                                    let mut odds_buf = [0u8; 2];
                                    Self::write_num(&mut odds_buf, 2, my_odds as u32);
                                    slcd::display_string(
                                        core::str::from_utf8(&odds_buf[..]).unwrap_or("  "),
                                        2,
                                    );
                                } else {
                                    slcd::display_string("  ", 2);
                                }
                                if my_odds >= self.gen_random_int(1, 200) {
                                    // EARTH!!!! The final objective.
                                    buf[..6].copy_from_slice(b"EArTH ");
                                    self.hero_counter = 0;
                                    self.legend_counter += 1;
                                }
                                // Recalculate difficulty level based on new hero_counter.
                                self.difficulty_level = self.hero_counter / PROMOTION_INTERVAL;
                                if self.difficulty_level >= DIFFICULTY_LEVELS {
                                    self.difficulty_level = DIFFICULTY_LEVELS - 1;
                                }
                                // Write to EEPROM.
                                self.write_to_lander_eeprom();
                            }
                        }
                        // 5) Set fuel conservation indicators as appropriate.
                        if ships_health >= 1 && self.fuel_score <= FUEL_SCORE_FANTASTIC {
                            slcd::set_indicator(slcd::Indicator::Lap);
                        }
                        if ships_health >= 1 && self.fuel_score <= FUEL_SCORE_GREAT {
                            slcd::set_indicator(slcd::Indicator::H24);
                        }
                        if ships_health >= 1 && self.fuel_score <= FUEL_SCORE_GOOD {
                            slcd::set_indicator(slcd::Indicator::Pm);
                        }
                        // 6) Set coffee maker OK indicator as appropriate.
                        if ships_health >= 5
                            || (ships_health >= 0 && self.gen_random_int(0, 3) != 1)
                        {
                            slcd::set_indicator(slcd::Indicator::Signal);
                        }
                        // 7) Green light if ship intact.
                        if ships_health >= 8 && self.led_enabled {
                            watch::led::set_led_green();
                            self.led_active = true;
                        }
                        // 8) Set standard display if not preempted.
                        if !all_done {
                            if self.injured > 0 || self.uninjured == 0 {
                                buf[0] = b'0' + ships_health as u8;
                                buf[1] = b' ';
                                buf[2] = b'0' + (self.uninjured / 10) % 10;
                                buf[3] = b'0' + self.uninjured % 10;
                                buf[4] = b'0' + (self.injured / 10) % 10;
                                buf[5] = b'0' + self.injured % 10;
                            } else {
                                buf[0] = b'0' + ships_health as u8;
                                buf[1] = b' ';
                                buf[2] = b'0' + (self.uninjured / 10) % 10;
                                buf[3] = b'0' + self.uninjured % 10;
                                buf[4] = b' ';
                                buf[5] = b' ';
                            }
                        }
                        // Display final status.
                        self.display_bottom(&buf[..6]);
                    } // End if tick_counter == 1

                    // Major crash - ship burning with red LED.
                    if self.ships_health < 0 && self.led_enabled {
                        if self.gen_random_int(0, 1) != 1 && !last_pass {
                            // Turn on red LED.
                            watch::led::set_led_red();
                            self.led_active = true;
                        } else {
                            watch::led::set_led_off();
                        }
                    }
                    // Wait long enough, then allow waiting for next game.
                    if last_pass {
                        watch::led::set_led_off();
                        // No change to display text, allow new game to start.
                        self.mode = MODE_WAITING_TO_START;
                        // Unless it's time for monsters.
                        let survivors = self.injured + self.uninjured;
                        if self.ships_health >= 0
                            && survivors > 0
                            && self.gen_random_int(-1, 3) >= self.ships_health as i16
                        {
                            self.mode = MODE_MONSTER;
                            self.tick_counter = 0;
                            self.monster_type =
                                self.gen_random_int(0, MONSTER_TYPES as i16 - 1) as u8;
                        }
                    }
                }
                // End if MODE_DISPLAY_FINAL_STATUS
                else if self.mode == MODE_DISPLAY_SKILL_LEVEL {
                    // Display skill level.
                    if self.tick_counter == 1 {
                        buf[0] = b' ';
                        buf[1] = b'0' + self.skill_level;
                        slcd::display_string(core::str::from_utf8(&buf[..2]).unwrap_or("  "), 2);
                        buf[0] = b' ';
                        buf[1] = b' ';
                        buf[2] = b'0' + self.skill_level;
                        buf[3] = b' ';
                        buf[4] = b' ';
                        buf[5] = b' ';
                        self.display_bottom(&buf[..6]);
                    }
                    // Wait long enough, then start game.
                    if self.tick_counter >= (2 * LANDER_TICK_FREQUENCY) as u16 {
                        self.tick_counter = 0;
                        // Houston, WE ARE LAUNCHING NOW....
                        self.mode = MODE_PLAYING;
                    }
                } else if self.mode == MODE_FIND_EARTH_MESSAGE {
                    // Display "Find" then "Earth".
                    if self.tick_counter == 1 {
                        buf[..6].copy_from_slice(b" FInd ");
                        slcd::display_string("  ", 2);
                        self.display_bottom(&buf[..6]);
                    }
                    if self.tick_counter == (1.5 * LANDER_TICK_FREQUENCY as f32 + 1.0) as u16 {
                        buf[..6].copy_from_slice(b"EArTH ");
                        slcd::display_string("  ", 2);
                        self.display_bottom(&buf[..6]);
                    }
                    // Wait long enough, then display skill level.
                    if self.tick_counter >= (3 * LANDER_TICK_FREQUENCY) as u16 {
                        self.tick_counter = 0;
                        self.mode = MODE_DISPLAY_SKILL_LEVEL;
                    }
                } else if self.mode == MODE_MONSTER {
                    if self.tick_counter == 1 {
                        slcd::display_string(MONSTER_NAMES[self.monster_type as usize], 4);
                    } else if self.tick_counter == MONSTER_DISPLAY_TICKS + 1 {
                        let my_rand = self.gen_random_int(0, MONSTER_ACTIONS as i16 - 1) as u8;
                        slcd::display_string(MONSTER_ACTION_STRINGS[my_rand as usize], 4);
                    } else if self.tick_counter == MONSTER_DISPLAY_TICKS * 2 {
                        // Display 1st monster character.
                        let name = MONSTER_NAMES[self.monster_type as usize].as_bytes();
                        slcd::display_character(name[0], 4);
                    } else if self.tick_counter == MONSTER_DISPLAY_TICKS * 2 + 1 {
                        // Display current population, close mouth.
                        buf[0] = b' ';
                        buf[1] = b'c';
                        buf[2] = b'0' + (self.uninjured / 10) % 10;
                        buf[3] = b'0' + self.uninjured % 10;
                        buf[4] = b'0' + (self.injured / 10) % 10;
                        buf[5] = b'0' + self.injured % 10;
                        self.display_bottom(&buf[..6]);
                    } else if self.tick_counter == MONSTER_DISPLAY_TICKS * 2 + 3 {
                        slcd::display_character(b'C', 5); // Open mouth
                    } else if self.tick_counter == MONSTER_DISPLAY_TICKS * 2 + 5 {
                        // Decision to: continue loop, end loop or eat astronaut.
                        let survivors = self.injured + self.uninjured;
                        let my_rand = self.gen_random_int(0, 16);
                        if survivors == 0 {
                            self.mode = MODE_WAITING_TO_START;
                        } else if my_rand <= 1 {
                            // Leave loop with survivors.
                            buf[0] = b'0' + self.ships_health as u8;
                            buf[1] = b' ';
                            buf[2] = b'0' + (self.uninjured / 10) % 10;
                            buf[3] = b'0' + self.uninjured % 10;
                            buf[4] = b'0' + (self.injured / 10) % 10;
                            buf[5] = b'0' + self.injured % 10;
                            self.display_bottom(&buf[..6]);
                            self.mode = MODE_WAITING_TO_START;
                        } else if my_rand <= 11 {
                            // Do nothing, loop continues.
                            self.tick_counter = MONSTER_DISPLAY_TICKS * 2;
                        } else {
                            // Eat an astronaut - welcome to the space program!
                            if self.injured > 0 && self.uninjured > 0 {
                                if self.gen_random_int(0, 1) == 0 {
                                    self.injured -= 1;
                                } else {
                                    self.uninjured -= 1;
                                }
                            } else if self.injured > 0 {
                                self.injured -= 1;
                            } else {
                                self.uninjured -= 1;
                            }
                            self.tick_counter = MONSTER_DISPLAY_TICKS * 2; // Re-display
                        }
                    } else if self.tick_counter >= MONSTER_DISPLAY_TICKS * 4 {
                        self.mode = MODE_WAITING_TO_START; // Safety
                    }
                } // End if MODE_MONSTER
            }
            Event::Button(Button::Alarm, ButtonEvent::Down) => {
                if self.mode == MODE_WAITING_TO_START {
                    // That was the go signal - start a new game!!
                    movement::request_tick_frequency(LANDER_TICK_FREQUENCY);
                    watch::led::set_led_off(); // Safety
                    slcd::clear_all_indicators();
                    // Randomize starting parameters.
                    self.height = self.gen_random_int(131, 181) as i32 * 80;
                    // Per line below; see Mars Orbiter September 23, 1999.
                    if self.gen_random_int(0, 8) == 5 {
                        self.height = self.gen_random_int(240, 800) as i32 * 80;
                    }
                    self.speed = self.gen_random_int(-120, 35); // Positive is up
                    self.gravity = (self.gen_random_int(-3, -2) * 2) as i8; // negative downwards value
                    let skill_level = self.gen_random_int(1, 4) as u8;
                    // Theoretical Perfect Landing (TPL) calculations start here.
                    let my_time = self.speed as f32 / self.gravity as f32;
                    let dist_to_top = libm::fabsf(0.5 * self.gravity as f32 * my_time * my_time);
                    let tpl_top = (self.height as f32 + dist_to_top + 0.5) as i32;
                    // Time squared = (2 * grav * height) / (t*t + g*t), where t
                    // is net acceleration with thrust on.
                    let gravity_abs = self.gravity.abs() as i16;
                    let thrust = ENGINE_THRUST + self.gravity as i16;
                    let numerator = 2.0 * gravity_abs as f32 * tpl_top as f32;
                    let denominator = (thrust * thrust + thrust * gravity_abs) as f32;
                    let time_squared = numerator / denominator;
                    self.fuel_tpl = (libm::sqrtf(time_squared) + 0.5) as u16;
                    let fuel_mult: f32 = if skill_level == 1 {
                        4.0 // TPL + 300%
                    } else if skill_level == 2 {
                        2.5 // TPL + 150%
                    } else if skill_level == 3 {
                        1.6 // TPL + 60%
                    } else {
                        1.3 // TPL + 30%
                    };
                    self.fuel_start = (self.fuel_tpl as f32 * fuel_mult) as u16;
                    self.fuel_remaining = self.fuel_start;
                    self.skill_level = skill_level;
                    self.tick_counter = 0;
                    if self.gen_random_int(1, 109) != 37 {
                        // Houston, approaching launch....
                        self.mode = MODE_DISPLAY_SKILL_LEVEL;
                    } else {
                        self.mode = MODE_FIND_EARTH_MESSAGE;
                    }
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                if self.mode == MODE_WAITING_TO_START {
                    // Display difficulty level.
                    slcd::display_string(DIFFICULTY_NAMES[self.difficulty_level as usize], 4);
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.mode != MODE_WAITING_TO_START {
                    return;
                }
                self.led_enabled = !self.led_enabled;
                if self.led_enabled {
                    slcd::set_indicator(slcd::Indicator::Signal);
                } else {
                    slcd::clear_indicator(slcd::Indicator::Signal);
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongUp) => {
                if self.mode == MODE_WAITING_TO_START && self.legend_counter > 0 {
                    if self.legend_counter > 9 {
                        buf[..4].copy_from_slice(b"EArt");
                        Self::write_num(&mut buf[4..6], 2, self.legend_counter as u32);
                    } else {
                        buf[..5].copy_from_slice(b"EArth");
                        buf[5] = b'0' + self.legend_counter;
                    }
                    // Display legend counter.
                    self.display_bottom(&buf[..6]);
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        watch::led::set_led_off();
    }
}
