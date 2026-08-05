//! Tomato (Pomodoro) timer watch face.
//!
//! Port of the C `tomato_face.c`. A Pomodoro timer alternating focus and break
//! periods. It is a pure state machine: it reacts to a single event and
//! returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const FOCUS_MIN: u8 = 25;
const BREAK_MIN: u8 = 5;

/// The tomato mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TomatoMode {
    Run,
    Ready,
}

/// The tomato kind.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TomatoKind {
    Focus,
    Break,
}

/// The tomato face state.
pub struct TomatoFace {
    mode: TomatoMode,
    kind: TomatoKind,
    done_count: u8,
    visible: bool,
    now_ts: u32,
    target_ts: u32,
}

impl TomatoFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        TomatoFace {
            mode: TomatoMode::Ready,
            kind: TomatoKind::Focus,
            done_count: 0,
            visible: true,
            now_ts: 0,
            target_ts: 0,
        }
    }

    pub fn new() -> Self {
        TomatoFace::new_static()
    }

    fn tz_offset(settings: &Settings) -> u32 {
        (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60) as u32
    }

    fn get_length(&self) -> u8 {
        match self.kind {
            TomatoKind::Focus => FOCUS_MIN,
            TomatoKind::Break => BREAK_MIN,
        }
    }

    fn start(&mut self, settings: &Settings) {
        let now = rtc::get_date_time();
        let length = self.get_length();
        self.mode = TomatoMode::Run;
        self.now_ts = utility::date_time_to_unix_time(now, Self::tz_offset(settings));
        self.target_ts = utility::offset_timestamp(self.now_ts, 0, length as i8, 0);
        let target_dt =
            utility::date_time_from_unix_time(self.target_ts, Self::tz_offset(settings));
        movement::schedule_background_task(target_dt);
        watch::slcd::set_indicator(Indicator::Bell);
    }

    fn draw(&self) {
        let mut buf = [0u8; 11];
        let mut min = 0u8;
        let mut sec = 0u8;
        let kind = match self.kind {
            TomatoKind::Break => b'b',
            TomatoKind::Focus => b'f',
        };
        match self.mode {
            TomatoMode::Run => {
                let delta = self.target_ts.saturating_sub(self.now_ts);
                min = (delta / 60) as u8;
                sec = (delta % 60) as u8;
            }
            TomatoMode::Ready => {
                min = self.get_length();
                sec = 0;
            }
        }
        if self.visible {
            buf[0] = b'T';
            buf[1] = b'O';
            buf[2] = b' ';
            buf[3] = kind;
            buf[4] = b'0' + min / 10;
            buf[5] = b'0' + min % 10;
            buf[6] = b'0' + sec / 10;
            buf[7] = b'0' + sec % 10;
            buf[8] = b' ';
            buf[9] = b'0' + self.done_count;
            watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        }
    }

    fn reset(&mut self) {
        self.mode = TomatoMode::Ready;
        movement::cancel_background_task();
        watch::slcd::clear_indicator(Indicator::Bell);
    }

    fn ring(&mut self) {
        movement::play_signal();
        self.reset();
        if self.kind == TomatoKind::Focus {
            self.kind = TomatoKind::Break;
            self.done_count += 1;
        } else {
            self.kind = TomatoKind::Focus;
        }
    }
}

impl WatchFace for TomatoFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, settings: &Settings) {
        if self.mode == TomatoMode::Run {
            let now = rtc::get_date_time();
            self.now_ts = utility::date_time_to_unix_time(now, Self::tz_offset(settings));
            watch::slcd::set_indicator(Indicator::Bell);
        }
        watch::slcd::set_colon();
        self.visible = true;
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => self.draw(),
            Event::Tick => {
                if self.mode == TomatoMode::Run {
                    self.now_ts += 1;
                }
                self.draw();
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                movement::illuminate_led();
                if self.mode == TomatoMode::Ready {
                    self.kind = match self.kind {
                        TomatoKind::Break => TomatoKind::Focus,
                        TomatoKind::Focus => TomatoKind::Break,
                    };
                }
                self.draw();
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                match self.mode {
                    TomatoMode::Run => self.reset(),
                    TomatoMode::Ready => self.start(settings),
                }
                self.draw();
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.done_count = 0;
            }
            Event::BackgroundTask => {
                self.ring();
                self.draw();
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.visible = false;
    }
}
