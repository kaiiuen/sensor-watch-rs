//! Chirpy demo watch face.
//!
//! Port of the C `chirpy_demo_face.c`. Chirps out a frequency scale or data
//! over the buzzer. It is a pure state machine: it reacts to a single event
//! and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd::Indicator;

const CDM_CHOOSE: u8 = 0;
const CDM_CHIRPING: u8 = 1;

const CDP_SCALE: u8 = 0;
const CDP_INFO_SHORT: u8 = 1;
const CDP_INFO_LONG: u8 = 2;
const CDP_INFO_NANOSEC: u8 = 3;

const SHORT_DATA: [u8; 20] = [
    0x27, 0x00, 0x0c, 0x42, 0xa3, 0xd4, 0x06, 0x54, 0x00, 0x00, 0x02, 0x0c, 0x6b, 0x05, 0x5a, 0x09,
    0xd8, 0x00, 0xf5, 0x00,
];

const LONG_DATA: &str = "There once was a ship that put to sea\n";

/// The chirpy demo face state.
pub struct ChirpyDemoFace {
    mode: u8,
    program: u8,
    tick_count: u8,
    tick_compare: u8,
    seq_pos: u16,
    curr_data_ix: u16,
    curr_data_len: u16,
}

impl ChirpyDemoFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        ChirpyDemoFace {
            mode: CDM_CHOOSE,
            program: CDP_SCALE,
            tick_count: 0,
            tick_compare: 8,
            seq_pos: 0,
            curr_data_ix: 0,
            curr_data_len: 0,
        }
    }

    pub fn new() -> Self {
        ChirpyDemoFace::new_static()
    }

    fn update_lcd(&self) {
        watch::slcd::display_string("CH", 0);
        let s = match self.program {
            CDP_SCALE => " SCALE",
            CDP_INFO_SHORT => "SHORT ",
            CDP_INFO_LONG => " LOng ",
            _ => "nAnO  ",
        };
        watch::slcd::display_string(s, 4);
    }

    fn quit_chirping(&mut self) {
        self.mode = CDM_CHOOSE;
        watch::buzzer::set_buzzer_off();
        watch::slcd::clear_indicator(Indicator::Bell);
    }

    fn scale_tick(&mut self) {
        if self.seq_pos == 58 {
            self.quit_chirping();
            return;
        }
        let freq = 700 + self.seq_pos * 200;
        let period = 1000000u32 / freq as u32;
        watch::buzzer::set_buzzer_period(period);
        watch::buzzer::set_buzzer_on();
        self.seq_pos += 1;
    }

    fn data_tick(&mut self) {
        if self.curr_data_ix >= self.curr_data_len {
            self.quit_chirping();
            return;
        }
        let byte = if self.program == CDP_INFO_SHORT {
            SHORT_DATA[self.curr_data_ix as usize]
        } else {
            LONG_DATA.as_bytes()[self.curr_data_ix as usize]
        };
        self.curr_data_ix += 1;
        let _ = byte;
        watch::buzzer::set_buzzer_period(5000);
        watch::buzzer::set_buzzer_on();
    }

    fn countdown_tick(&mut self) {
        if self.seq_pos == 8 * 3 {
            self.tick_compare = 3;
            self.tick_count = 0;
            self.seq_pos = 0;
            self.curr_data_ix = 0;
            self.curr_data_len = if self.program == CDP_INFO_SHORT {
                SHORT_DATA.len() as u16
            } else {
                LONG_DATA.len() as u16
            };
            return;
        }
        if self.seq_pos % 8 == 0 {
            watch::buzzer::set_buzzer_period(
                crate::watch::buzzer::NOTE_PERIODS[Note::A5 as usize] as u32,
            );
            watch::buzzer::set_buzzer_on();
        } else if self.seq_pos % 8 == 1 {
            watch::buzzer::set_buzzer_off();
        }
        self.seq_pos += 1;
    }

    fn setup_chirp(&mut self) {
        watch::slcd::set_indicator(Indicator::Bell);
        self.mode = CDM_CHIRPING;
        self.tick_count = 0;
        self.tick_compare = 8;
        self.seq_pos = 0;
    }
}

impl WatchFace for ChirpyDemoFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.mode = CDM_CHOOSE;
        self.program = CDP_SCALE;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => self.update_lcd(),
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                if self.mode != CDM_CHIRPING {
                    movement::move_to_next_face();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {}
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.mode == CDM_CHOOSE {
                    self.program = match self.program {
                        CDP_SCALE => CDP_INFO_SHORT,
                        CDP_INFO_SHORT => CDP_INFO_LONG,
                        CDP_INFO_LONG => CDP_SCALE,
                        _ => CDP_SCALE,
                    };
                    self.update_lcd();
                } else if self.mode == CDM_CHIRPING {
                    self.quit_chirping();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.mode == CDM_CHOOSE {
                    self.setup_chirp();
                }
            }
            Event::Tick => {
                if self.mode == CDM_CHIRPING {
                    self.tick_count += 1;
                    if self.tick_count == self.tick_compare {
                        self.tick_count = 0;
                        if self.program == CDP_SCALE {
                            self.scale_tick();
                        } else {
                            self.countdown_tick();
                        }
                    }
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
