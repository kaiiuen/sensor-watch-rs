//! Simon watch face.
//!
//! Port of the C `simon_face.c`. A memory game where the player repeats a
//! growing sequence of tones. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd;
use crate::watch::slcd::Indicator;

const SIMON_FACE_FREQUENCY: u32 = 8;
const DELAY_FOR_TONE_MS: u16 = 200;
const TIMER_MAX: u16 = 10;

const SIMON_NOT_PLAYING: u8 = 0;
const SIMON_TEACHING: u8 = 1;
const SIMON_LISTENING_BACK: u8 = 2;
const SIMON_READY_FOR_NEXT_NOTE: u8 = 3;

const SIMON_MODE_EASY: u8 = 0;
const SIMON_MODE_HARD: u8 = 1;
const SIMON_MODE_TOTAL: u8 = 2;

const SIMON_LED_NOTE: u8 = 0;
const SIMON_ALARM_NOTE: u8 = 1;
const SIMON_MODE_NOTE: u8 = 2;
const SIMON_WRONG_NOTE: u8 = 3;

/// The simon face state.
pub struct SimonFace {
    playing_state: u8,
    listen_index: u8,
    sequence_length: u8,
    teaching_index: u8,
    sequence: [u8; 64],
    best_score: u8,
    sound_off: bool,
    light_off: bool,
    mode: u8,
    timer: u16,
    delay_beep: u16,
    sec_sub: u32,
    timeout: u32,
}

impl SimonFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        SimonFace {
            playing_state: SIMON_NOT_PLAYING,
            listen_index: 0,
            sequence_length: 0,
            teaching_index: 0,
            sequence: [0; 64],
            best_score: 0,
            sound_off: false,
            light_off: false,
            mode: SIMON_MODE_EASY,
            timer: 0,
            delay_beep: DELAY_FOR_TONE_MS,
            sec_sub: SIMON_FACE_FREQUENCY,
            timeout: TIMER_MAX as u32 * SIMON_FACE_FREQUENCY,
        }
    }

    pub fn new() -> Self {
        SimonFace::new_static()
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

    fn clear_display(&self) {
        if self.playing_state == SIMON_NOT_PLAYING {
            slcd::display_string("          ", 0);
        } else {
            let mut buf = [0u8; 11];
            buf[0] = b' ';
            buf[1] = b' ';
            buf[2] = b'0' + self.sequence_length / 10;
            buf[3] = b'0' + self.sequence_length % 10;
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        }
    }

    fn not_playing_display(&self) {
        self.clear_display();
        let mut buf = [0u8; 11];
        buf[0] = b'S';
        buf[1] = b'I';
        buf[2] = b' ';
        buf[3] = b' ';
        buf[4] = b'0' + self.best_score / 10;
        buf[5] = b'0' + self.best_score % 10;
        if !self.sound_off {
            slcd::set_indicator(Indicator::Bell);
        } else {
            slcd::clear_indicator(Indicator::Bell);
        }
        if !self.light_off {
            slcd::set_indicator(Indicator::Signal);
        } else {
            slcd::clear_indicator(Indicator::Signal);
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        if self.mode == SIMON_MODE_EASY {
            slcd::display_string("E", 9);
        } else {
            slcd::display_string("H", 9);
        }
    }

    fn reset(&mut self) {
        self.playing_state = SIMON_NOT_PLAYING;
        self.listen_index = 0;
        self.sequence_length = 0;
        self.not_playing_display();
    }

    fn display_note(&self, note: u8) {
        let mut buf = [0u8; 11];
        match note {
            SIMON_LED_NOTE => {
                buf[0] = b'L';
                buf[1] = b'I';
                buf[2] = b'0' + self.sequence_length / 10;
                buf[3] = b'0' + self.sequence_length % 10;
            }
            SIMON_ALARM_NOTE => {
                buf[2] = b'0' + self.sequence_length / 10;
                buf[3] = b'0' + self.sequence_length % 10;
                buf[8] = b'A';
                buf[9] = b'L';
            }
            SIMON_MODE_NOTE => {
                buf[2] = b'0' + self.sequence_length / 10;
                buf[3] = b'0' + self.sequence_length % 10;
                buf[4] = b'D';
                buf[5] = b'E';
            }
            _ => {
                buf[0] = b'O';
                buf[1] = b'H';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b'N';
                buf[5] = b'O';
                buf[6] = b'O';
                buf[7] = b'O';
                buf[8] = b'O';
                buf[9] = b'O';
            }
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn play_note(&self, note: u8, skip_rest: bool) {
        self.display_note(note);
        match note {
            SIMON_LED_NOTE => {
                if !self.light_off {
                    watch::led::set_led_yellow();
                }
                if !self.sound_off {
                    crate::movement::play_alarm_beeps(1, Note::D3);
                }
            }
            SIMON_MODE_NOTE => {
                if !self.light_off {
                    watch::led::set_led_red();
                }
                if !self.sound_off {
                    crate::movement::play_alarm_beeps(1, Note::E4);
                }
            }
            SIMON_ALARM_NOTE => {
                if !self.light_off {
                    watch::led::set_led_green();
                }
                if !self.sound_off {
                    crate::movement::play_alarm_beeps(1, Note::C3);
                }
            }
            _ => {
                if !self.sound_off {
                    crate::movement::play_alarm_beeps(1, Note::A1);
                }
            }
        }
        watch::led::set_led_off();
        if note != SIMON_WRONG_NOTE {
            self.clear_display();
            if !skip_rest {
                crate::movement::play_alarm_beeps(1, Note::Rest);
            }
        }
    }

    fn setup_next_note(&mut self) {
        if self.sequence_length > self.best_score {
            self.best_score = self.sequence_length;
        }
        self.clear_display();
        self.playing_state = SIMON_TEACHING;
        self.sequence[self.sequence_length as usize] = self.get_rand_num(3) + 1;
        self.sequence_length += 1;
        self.teaching_index = 0;
        self.listen_index = 0;
    }

    fn listen(&mut self, note: u8) {
        if self.sequence[self.listen_index as usize] == note {
            self.play_note(note, true);
            self.listen_index += 1;
            self.timer = 0;
            if self.listen_index == self.sequence_length {
                self.playing_state = SIMON_READY_FOR_NEXT_NOTE;
            }
        } else {
            self.play_note(SIMON_WRONG_NOTE, true);
            self.reset();
        }
    }

    fn begin_listening(&mut self) {
        self.playing_state = SIMON_LISTENING_BACK;
        self.listen_index = 0;
    }

    fn change_speed(&mut self) {
        if self.mode == SIMON_MODE_HARD {
            self.delay_beep = DELAY_FOR_TONE_MS / 2;
            self.sec_sub = SIMON_FACE_FREQUENCY / 2;
            self.timeout = (TIMER_MAX as u32 * SIMON_FACE_FREQUENCY) / 2;
        } else {
            self.delay_beep = DELAY_FOR_TONE_MS;
            self.sec_sub = SIMON_FACE_FREQUENCY;
            self.timeout = TIMER_MAX as u32 * SIMON_FACE_FREQUENCY;
        }
    }
}

impl WatchFace for SimonFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.change_speed();
        self.timer = 0;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => self.reset(),
            Event::Tick => {
                if self.playing_state == SIMON_LISTENING_BACK && self.mode != SIMON_MODE_EASY {
                    self.timer += 1;
                    if self.timer as u32 >= self.timeout {
                        self.timer = 0;
                        self.play_note(SIMON_WRONG_NOTE, true);
                        self.reset();
                    }
                } else if self.playing_state == SIMON_TEACHING {
                    let note = self.sequence[self.teaching_index as usize];
                    self.play_note(note, self.teaching_index == (self.sequence_length - 1));
                    self.teaching_index += 1;
                    if self.teaching_index == self.sequence_length {
                        self.begin_listening();
                    }
                } else if self.playing_state == SIMON_READY_FOR_NEXT_NOTE {
                    self.timer = 0;
                    self.setup_next_note();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.playing_state == SIMON_NOT_PLAYING {
                    self.light_off = !self.light_off;
                    self.not_playing_display();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.playing_state == SIMON_NOT_PLAYING {
                    self.sound_off = !self.sound_off;
                    self.not_playing_display();
                    if !self.sound_off {
                        crate::movement::play_alarm_beeps(1, Note::D3);
                    }
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.playing_state == SIMON_NOT_PLAYING {
                    self.sequence_length = 0;
                    slcd::clear_indicator(Indicator::Bell);
                    slcd::clear_indicator(Indicator::Signal);
                    self.setup_next_note();
                } else if self.playing_state == SIMON_LISTENING_BACK {
                    self.listen(SIMON_LED_NOTE);
                }
            }
            Event::Button(Button::Mode, ButtonEvent::LongPress) => {
                if self.playing_state == SIMON_NOT_PLAYING {
                    movement::move_to_face(0);
                } else {
                    self.playing_state = SIMON_NOT_PLAYING;
                    self.reset();
                }
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                if self.playing_state == SIMON_NOT_PLAYING {
                    movement::move_to_next_face();
                } else if self.playing_state == SIMON_LISTENING_BACK {
                    self.listen(SIMON_MODE_NOTE);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.playing_state == SIMON_LISTENING_BACK {
                    self.listen(SIMON_ALARM_NOTE);
                } else if self.playing_state == SIMON_NOT_PLAYING {
                    self.mode = (self.mode + 1) % SIMON_MODE_TOTAL;
                    self.change_speed();
                    self.not_playing_display();
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        watch::led::set_led_off();
        watch::buzzer::set_buzzer_off();
    }
}
