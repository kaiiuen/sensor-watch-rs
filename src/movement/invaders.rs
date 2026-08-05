//! Invaders watch face.
//!
//! Port of the C `invaders_face.c`. A Space-Invaders-style reaction game where
//! you shoot descending invader digits. It is a pure state machine: it reacts
//! to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd;

const INVADERS_FACE_WAVES_PER_STAGE: u8 = 9;
const INVADERS_FACE_WAVE_INVADERS: u8 = 16;

const DEFENSE_LINES_SEGDATA: [(u8, u8); 3] = [(2, 12), (2, 11), (0, 11)];
const BONUS_POINTS_SEGDATA: [(u8, u8); 4] = [(2, 7), (2, 8), (2, 9), (0, 10)];
const BONUS_POINTS_HELPER: [u8; 9] = [1, 5, 9, 11, 15, 19, 21, 25, 29];

/// The current game state.
#[derive(Clone, Copy, PartialEq, Eq)]
enum InvadersState {
    Activated,
    PreGame,
    Playing,
    InWaveBreak,
    PreNextWave,
    NextWave,
    GameOver,
}

/// The invaders face state.
pub struct InvadersFace {
    state: InvadersState,
    sound_on: bool,
    highscore: u16,
    invaders: [i8; 6],
    wave_invaders: [u8; INVADERS_FACE_WAVE_INVADERS as usize],
    defense_lines: u8,
    aim: u8,
    invader_idx: u8,
    wave_position: u8,
    wave_tick_freq: u8,
    ticks: u8,
    bonus_countdown: u8,
    waves: u8,
    shots_in_wave: u8,
    invaders_shot: u8,
    invaders_shot_sum: u8,
    ufo_next: bool,
    inv_checking: bool,
    suspend_buttons: bool,
    score: u16,
}

impl InvadersFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        InvadersFace {
            state: InvadersState::Activated,
            sound_on: true,
            highscore: 0,
            invaders: [-1; 6],
            wave_invaders: [0; INVADERS_FACE_WAVE_INVADERS as usize],
            defense_lines: 0,
            aim: 0,
            invader_idx: 0,
            wave_position: 0,
            wave_tick_freq: 0,
            ticks: 0,
            bonus_countdown: 0,
            waves: 0,
            shots_in_wave: 0,
            invaders_shot: 0,
            invaders_shot_sum: 0,
            ufo_next: false,
            inv_checking: false,
            suspend_buttons: false,
            score: 0,
        }
    }

    pub fn new() -> Self {
        InvadersFace::new_static()
    }

    fn get_rand_num(&self, num_values: u8) -> u8 {
        let now = crate::watch::rtc::get_date_time();
        let mut x = now.to_reg();
        if x == 0 {
            x = 0x0F1E_2D3C;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        (x % num_values as u32) as u8
    }

    fn play_sequence(&self, note: Note) {
        if self.sound_on {
            crate::movement::play_alarm_beeps(1, note);
        }
    }

    fn display_defense_lines(&self) {
        slcd::display_character(b' ', 1);
        for i in 0..(3 - self.defense_lines) {
            let (c, s) = DEFENSE_LINES_SEGDATA[i as usize];
            slcd::set_pixel(c, s);
        }
    }

    fn display_score(&self, label: &str, score: u16) {
        let lb = label.as_bytes();
        slcd::display_character(lb[0], 0);
        slcd::display_character(lb[1], 1);
        let mut buf = [0u8; 10];
        let v = score as u32 * 10;
        buf[0] = b' ';
        buf[1] = b' ';
        buf[2] = b'0' + ((v / 100000) % 10) as u8;
        buf[3] = b'0' + ((v / 10000) % 10) as u8;
        buf[4] = b'0' + ((v / 1000) % 10) as u8;
        buf[5] = b'0' + ((v / 100) % 10) as u8;
        buf[6] = b'0' + ((v / 10) % 10) as u8;
        buf[7] = b'0' + (v % 10) as u8;
        slcd::display_string(core::str::from_utf8(&buf[..8]).unwrap_or(""), 2);
    }

    fn display_invader(&self, invader: i8, position: u8) {
        match invader {
            10 => slcd::display_character(b'n', position),
            -1 => slcd::display_character(b' ', position),
            _ => slcd::display_character(invader as u8 + 48, position),
        }
    }

    fn game_over(&mut self) {
        self.display_score("GO", self.score);
        self.state = InvadersState::GameOver;
        self.suspend_buttons = true;
        self.play_sequence(Note::A6);
        if self.score > self.highscore {
            self.highscore = self.score;
        }
    }

    fn init_wave(&mut self) {
        if self.state == InvadersState::InWaveBreak {
            self.invader_idx = self.invaders_shot;
        } else {
            self.invader_idx = 0;
            self.invaders_shot = 0;
            self.invaders_shot_sum = 0;
            self.defense_lines = 0;
            self.shots_in_wave = 0;
        }
        for i in self.invader_idx..INVADERS_FACE_WAVE_INVADERS {
            self.wave_invaders[i as usize] = self.get_rand_num(10);
        }
        for i in 1..6 {
            self.invaders[i] = -1;
        }
        self.invaders[0] = self.wave_invaders[self.invader_idx as usize] as i8;
        self.wave_position = 0;
        self.aim = 0;
        self.bonus_countdown = 0;
        self.ufo_next = false;
        self.inv_checking = false;
        self.suspend_buttons = false;
        self.state = InvadersState::Playing;
        self.wave_tick_freq = 6 - ((self.waves % INVADERS_FACE_WAVES_PER_STAGE) + 1) / 2;
        if self.waves >= INVADERS_FACE_WAVES_PER_STAGE {
            self.wave_tick_freq -= 1;
        }
        slcd::display_string("        ", 2);
        slcd::display_character(b'0', 0);
        self.display_defense_lines();
        slcd::display_character(self.wave_invaders[self.invader_idx as usize] + 48, 9);
    }

    fn move_invaders(&mut self) -> bool {
        if self.wave_position == 5 {
            return true;
        }
        self.inv_checking = true;
        if self.invaders[self.wave_position as usize] >= 0 {
            self.wave_position += 1;
        }
        for i in (1..=self.wave_position as usize).rev() {
            self.invaders[i] = self.invaders[i - 1];
        }
        if self.invader_idx < INVADERS_FACE_WAVE_INVADERS - 1 {
            self.invader_idx += 1;
            if self.ufo_next {
                self.invaders[0] = 10;
                self.ufo_next = false;
            } else {
                self.invaders[0] = self.wave_invaders[self.invader_idx as usize] as i8;
            }
        } else {
            self.invaders[0] = -1;
        }
        for i in 0..=self.wave_position as usize {
            self.display_invader(self.invaders[i], 9 - i as u8);
        }
        self.inv_checking = false;
        false
    }
}

impl WatchFace for InvadersFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.state = InvadersState::Activated;
        self.suspend_buttons = false;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => {
                self.display_score("GA", self.highscore);
            }
            Event::Tick => {
                self.ticks += 1;
                match self.state {
                    InvadersState::InWaveBreak
                    | InvadersState::PreGame
                    | InvadersState::NextWave => {
                        if self.ticks >= 2 {
                            self.ticks = 0;
                            self.init_wave();
                        }
                    }
                    InvadersState::Playing => {
                        if self.ticks >= self.wave_tick_freq {
                            self.ticks = 0;
                            if self.move_invaders() {
                                if self.defense_lines < 2 {
                                    self.defense_lines += 1;
                                    self.display_defense_lines();
                                    self.display_score("GA", self.score);
                                    self.state = InvadersState::InWaveBreak;
                                    self.play_sequence(Note::A6);
                                } else {
                                    self.game_over();
                                }
                            }
                        }
                        if self.bonus_countdown > 0 {
                            self.bonus_countdown -= 1;
                            if self.bonus_countdown == 0 {
                                slcd::display_character(b' ', 2);
                                slcd::display_character(b' ', 3);
                            }
                        }
                    }
                    InvadersState::PreNextWave => {
                        if self.ticks >= 3 {
                            self.ticks = 0;
                            self.display_score("GA", self.score);
                            slcd::set_pixel(1, 9);
                            slcd::display_character(
                                (self.waves % INVADERS_FACE_WAVES_PER_STAGE) + 49,
                                3,
                            );
                            self.state = InvadersState::NextWave;
                            self.waves += 1;
                            if self.waves == INVADERS_FACE_WAVES_PER_STAGE * 2 {
                                self.waves = 0;
                            }
                            self.play_sequence(Note::A6);
                        }
                    }
                    _ => {}
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                if !self.suspend_buttons {
                    if self.state == InvadersState::Playing {
                        self.aim = (self.aim + 1) % 11;
                        self.display_invader(self.aim as i8, 0);
                    } else if self.state == InvadersState::Activated
                        || self.state == InvadersState::GameOver
                    {
                        movement::illuminate_led();
                    }
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if (self.state == InvadersState::Activated || self.state == InvadersState::GameOver)
                    && !self.suspend_buttons
                {
                    self.sound_on = !self.sound_on;
                    crate::movement::play_alarm_beeps(1, Note::A7);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Down) => {
                if !self.suspend_buttons {
                    match self.state {
                        InvadersState::GameOver | InvadersState::Activated => {
                            self.waves = 0;
                            self.score = 0;
                            self.ticks = 0;
                            self.state = InvadersState::PreGame;
                            self.play_sequence(Note::A6);
                        }
                        InvadersState::Playing => {
                            self.shots_in_wave += 1;
                            if self.shots_in_wave == 30 {
                                self.game_over();
                            } else {
                                let mut skip = false;
                                let mut i = self.wave_position as i8;
                                while i >= 0 && !skip {
                                    if self.invaders[i as usize] == self.aim as i8 {
                                        skip = true;
                                        self.invaders_shot += 1;
                                        self.play_sequence(if self.aim == 10 {
                                            Note::A6
                                        } else {
                                            Note::A6
                                        });
                                        if self.invaders_shot == INVADERS_FACE_WAVE_INVADERS {
                                            slcd::display_character(b' ', 9 - self.wave_position);
                                            self.ticks = 0;
                                            self.state = InvadersState::PreNextWave;
                                        } else {
                                            if self.aim != 0 && self.aim < 10 {
                                                self.invaders_shot_sum =
                                                    (self.invaders_shot_sum + self.aim) % 10;
                                                if self.invaders_shot_sum == 0 {
                                                    self.ufo_next = true;
                                                }
                                            }
                                            if self.wave_position == 0 || i == 5 {
                                                self.invaders[i as usize] = -1;
                                            } else {
                                                for j in i as usize..self.wave_position as usize {
                                                    self.invaders[j] = self.invaders[j + 1];
                                                    self.display_invader(
                                                        self.invaders[j],
                                                        9 - j as u8,
                                                    );
                                                }
                                            }
                                            slcd::display_character(b' ', 9 - self.wave_position);
                                            if self.wave_position > 0 {
                                                self.wave_position -= 1;
                                            }
                                            if self.aim == 10 {
                                                let mut bonus_points = 0u8;
                                                for j in 0..BONUS_POINTS_HELPER.len() {
                                                    if self.shots_in_wave == BONUS_POINTS_HELPER[j]
                                                    {
                                                        bonus_points = 30;
                                                    } else if self.shots_in_wave - 1
                                                        == BONUS_POINTS_HELPER[j]
                                                    {
                                                        bonus_points = 20;
                                                    }
                                                }
                                                if bonus_points == 0 {
                                                    bonus_points = 10;
                                                }
                                                bonus_points += (6 - i as u8);
                                                if self.waves >= INVADERS_FACE_WAVES_PER_STAGE
                                                    && i != 0
                                                {
                                                    bonus_points += (6 - i as u8);
                                                }
                                                self.score += bonus_points as u16;
                                                for j in 0..(bonus_points / 10) as usize {
                                                    let (c, s) = BONUS_POINTS_SEGDATA[j];
                                                    slcd::set_pixel(c, s);
                                                }
                                                self.bonus_countdown = 9;
                                            } else {
                                                self.score += (6 - self.wave_position) as u16
                                                    * if self.waves >= INVADERS_FACE_WAVES_PER_STAGE
                                                    {
                                                        2
                                                    } else {
                                                        1
                                                    };
                                            }
                                        }
                                    }
                                    i -= 1;
                                }
                                if !skip {
                                    self.play_sequence(Note::A7);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.state = InvadersState::GameOver;
    }
}
