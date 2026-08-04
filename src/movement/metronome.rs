//! Metronome watch face.
//!
//! Port of the C `metronome_face.c`. A configurable metronome that plays beats
//! at a chosen BPM. It is a pure state machine: it reacts to a single event and
//! returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::slcd::Indicator;

/// The metronome mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MetMode {
    Wait,
    Run,
    SetMenu,
}

/// The setting currently being edited.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SetCur {
    Hundred,
    Ten,
    One,
    Count,
    Alarm,
}

/// The metronome face state.
pub struct MetronomeFace {
    mode: MetMode,
    bpm: u16,
    count: u8,
    sound_on: bool,
    set_cur: SetCur,
    tick: i32,
    cur_tick: i32,
    half_beat: i32,
    correction: f64,
    cur_correction: f64,
    cur_beat: u8,
}

impl MetronomeFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        MetronomeFace {
            mode: MetMode::Wait,
            bpm: 120,
            count: 4,
            sound_on: true,
            set_cur: SetCur::Hundred,
            tick: 0,
            cur_tick: 0,
            half_beat: 0,
            correction: 0.0,
            cur_correction: 0.0,
            cur_beat: 1,
        }
    }

    pub fn new() -> Self {
        MetronomeFace::new_static()
    }

    fn update_lcd(&self) {
        if self.sound_on {
            watch::slcd::set_indicator(Indicator::Bell);
        } else {
            watch::slcd::clear_indicator(Indicator::Bell);
        }
        let mut buf = [0u8; 11];
        buf[0] = b'M';
        buf[1] = b'N';
        buf[2] = b' ';
        buf[3] = b'0' + self.count;
        buf[4] = b' ';
        buf[5] = b'0' + ((self.bpm / 100) % 10) as u8;
        buf[6] = b'0' + ((self.bpm / 10) % 10) as u8;
        buf[7] = b'0' + (self.bpm % 10) as u8;
        buf[8] = b'b';
        buf[9] = b'p';
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn start_stop(&mut self) {
        if self.mode != MetMode::Run {
            self.mode = MetMode::Run;
            watch::slcd::clear_display();
            let ticks = 3840.0 / self.bpm as f64;
            self.tick = ticks as i32;
            self.cur_tick = ticks as i32;
            self.half_beat = self.tick / 2;
            self.cur_correction = ticks - self.tick as f64;
            self.correction = ticks - self.tick as f64;
            self.cur_beat = 1;
        } else {
            self.mode = MetMode::Wait;
            self.update_lcd();
        }
    }

    fn tick_beat(&self) {
        if self.sound_on {
            crate::movement::play_alarm_beeps(1, crate::watch::buzzer::Note::C6);
        }
        let mut buf = [0u8; 11];
        buf[0] = b'M';
        buf[1] = b'N';
        buf[2] = b' ';
        buf[3] = b'0' + self.count;
        buf[4] = b' ';
        buf[5] = b'0' + ((self.bpm / 100) % 10) as u8;
        buf[6] = b'0' + ((self.bpm / 10) % 10) as u8;
        buf[7] = b'0' + (self.bpm % 10) as u8;
        buf[8] = b'b';
        buf[9] = b'p';
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn event_tick(&mut self) {
        if self.cur_correction >= 1.0 {
            self.cur_correction -= 1.0;
            self.cur_tick -= 1;
        }
        let diff = self.cur_tick - self.tick;
        if diff == 0 {
            self.tick_beat();
            self.cur_tick = 0;
            self.cur_correction += self.correction;
            if self.cur_beat < self.count {
                self.cur_beat += 1;
            } else {
                self.cur_beat = 1;
            }
        } else {
            if self.cur_tick == self.half_beat {
                watch::slcd::clear_display();
            }
            self.cur_tick += 1;
        }
    }

    fn setting_tick(&self, subsecond: u8) {
        let mut buf = [0u8; 11];
        buf[0] = b'M';
        buf[1] = b'N';
        buf[2] = b' ';
        buf[3] = b'0' + self.count;
        buf[4] = b' ';
        buf[5] = b'0' + ((self.bpm / 100) % 10) as u8;
        buf[6] = b'0' + ((self.bpm / 10) % 10) as u8;
        buf[7] = b'0' + (self.bpm % 10) as u8;
        buf[8] = b'b';
        buf[9] = b'p';
        if subsecond % 2 == 0 {
            match self.set_cur {
                SetCur::Hundred => buf[5] = b' ',
                SetCur::Ten => buf[6] = b' ',
                SetCur::One => buf[7] = b' ',
                SetCur::Count => buf[3] = b' ',
                SetCur::Alarm => {}
            }
        }
        if self.set_cur == SetCur::Alarm {
            buf[3] = b' ';
            buf[4] = b' ';
            buf[5] = b'8';
            buf[6] = b'e';
            buf[7] = b'e';
            buf[8] = b'p';
            buf[9] = if self.sound_on { b'O' } else { b'-' };
        }
        if self.sound_on {
            watch::slcd::set_indicator(Indicator::Bell);
        } else {
            watch::slcd::clear_indicator(Indicator::Bell);
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn update_setting(&mut self) {
        match self.set_cur {
            SetCur::Hundred => {
                if self.bpm < 100 {
                    self.bpm += 100;
                } else {
                    self.bpm -= 100;
                }
            }
            SetCur::Ten => {
                if (self.bpm / 10) % 10 < 9 {
                    self.bpm += 10;
                } else {
                    self.bpm -= 90;
                }
            }
            SetCur::One => {
                if self.bpm % 10 < 9 {
                    self.bpm += 1;
                } else {
                    self.bpm -= 9;
                }
            }
            SetCur::Count => {
                if self.count < 9 {
                    self.count += 1;
                } else {
                    self.count = 2;
                }
            }
            SetCur::Alarm => self.sound_on = !self.sound_on,
        }
        if self.sound_on {
            watch::slcd::set_indicator(Indicator::Bell);
        } else {
            watch::slcd::clear_indicator(Indicator::Bell);
        }
    }
}

impl WatchFace for MetronomeFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        if self.bpm == 0 {
            self.count = 4;
            self.bpm = 120;
            self.sound_on = true;
        }
        self.mode = MetMode::Wait;
        self.correction = 0.0;
        self.set_cur = SetCur::Hundred;
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => self.update_lcd(),
            Event::Tick => {
                if self.mode == MetMode::Run {
                    self.event_tick();
                } else if self.mode == MetMode::SetMenu {
                    self.setting_tick(0);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.mode == MetMode::SetMenu {
                    self.update_setting();
                } else {
                    self.start_stop();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                if self.mode == MetMode::SetMenu {
                    self.set_cur = match self.set_cur {
                        SetCur::Alarm => SetCur::Hundred,
                        other => match other {
                            SetCur::Hundred => SetCur::Ten,
                            SetCur::Ten => SetCur::One,
                            SetCur::One => SetCur::Count,
                            SetCur::Count => SetCur::Alarm,
                            SetCur::Alarm => SetCur::Hundred,
                        },
                    };
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.mode != MetMode::Run && self.mode != MetMode::SetMenu {
                    self.mode = MetMode::SetMenu;
                    self.update_lcd();
                } else if self.mode == MetMode::SetMenu {
                    self.mode = MetMode::Wait;
                    self.update_lcd();
                }
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => movement::move_to_next_face(),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
