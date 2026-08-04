//! Sailing race timer watch face.
//!
//! Port of the C `sailing_face.c`. A sailing race countdown timer with
//! configurable warning signals and a lap counter. It is a pure state machine:
//! it reacts to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const SL_SELECTIONS: u8 = 6;
const DEFAULT_MINUTES: [u8; 6] = [5, 4, 1, 0, 0, 0];

/// Seconds before start that can trigger the buzzer.
const BEEP_SECONDS: [i32; 22] = [
    600, 540, 480, 420, 360, 300, 240, 180, 120, 60, 30, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
];

/// The sailing face mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Running,
    Waiting,
    Setting,
    Counting,
}

/// The sailing face state.
pub struct SailingFace {
    watch_face_index: usize,
    mode: Mode,
    minutes: [u8; 6],
    selection: u8,
    index: u8,
    now_ts: u32,
    target_ts: u32,
    nextbeep_ts: u32,
    beepflag: usize,
    ringflag: bool,
    alarmflag: u8,
    lap: u8,
}

impl SailingFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        SailingFace {
            watch_face_index: 0,
            mode: Mode::Waiting,
            minutes: DEFAULT_MINUTES,
            selection: 0,
            index: 0,
            now_ts: 0,
            target_ts: 0,
            nextbeep_ts: 0,
            beepflag: 0,
            ringflag: false,
            alarmflag: 3,
            lap: 0,
        }
    }

    pub fn new() -> Self {
        SailingFace::new_static()
    }

    fn tz_offset(settings: &Settings) -> u32 {
        (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60) as u32
    }

    fn reset(&mut self) {
        self.index = 0;
        self.mode = Mode::Waiting;
        movement::cancel_background_task_for_face(self.watch_face_index);
        watch::slcd::clear_indicator(Indicator::Lap);
        self.beepflag = 0;
        self.ringflag = false;
    }

    fn counting(&mut self) {
        self.mode = Mode::Counting;
        movement::cancel_background_task_for_face(self.watch_face_index);
        watch::slcd::set_indicator(Indicator::Lap);
    }

    fn update_alarm_indicators(&self) {
        match self.alarmflag {
            0 => {
                watch::slcd::clear_indicator(Indicator::Bell);
                watch::slcd::clear_indicator(Indicator::Signal);
            }
            1 => {
                watch::slcd::set_indicator(Indicator::Bell);
                watch::slcd::clear_indicator(Indicator::Signal);
            }
            2 => {
                watch::slcd::clear_indicator(Indicator::Bell);
                watch::slcd::set_indicator(Indicator::Signal);
            }
            _ => {
                watch::slcd::set_indicator(Indicator::Bell);
                watch::slcd::set_indicator(Indicator::Signal);
            }
        }
    }

    fn draw(&self, subsecond: u8) {
        let mut buf = [0u8; 11];
        match self.mode {
            Mode::Running => {
                let delta = if self.now_ts > self.target_ts {
                    0
                } else {
                    self.target_ts - self.now_ts
                };
                let min = delta / 60;
                let sec = delta % 60;
                buf[0] = b'S';
                buf[1] = b'A';
                buf[2] = b'1';
                buf[3] = b'L';
                buf[4] = b' ';
                buf[5] = b' ';
                if min > 0 {
                    buf[6] = b'0' + (min / 10) as u8;
                    buf[7] = b'0' + (min % 10) as u8;
                    buf[8] = b'0' + (sec / 10) as u8;
                    buf[9] = b'0' + (sec % 10) as u8;
                } else {
                    buf[6] = b'0' + (sec / 10) as u8;
                    buf[7] = b'0' + (sec % 10) as u8;
                    buf[8] = b' ';
                    buf[9] = b' ';
                }
            }
            Mode::Waiting => {
                buf[0] = b'S';
                buf[1] = b'A';
                buf[2] = b'1';
                buf[3] = b'L';
                buf[4] = b' ';
                buf[5] = b' ';
                buf[6] = b'0' + self.minutes[0] / 10;
                buf[7] = b'0' + self.minutes[0] % 10;
                buf[8] = b'0';
                buf[9] = b'0';
            }
            Mode::Setting => {
                buf[0] = b'S';
                buf[1] = b'A';
                buf[2] = b'1';
                buf[3] = b'L';
                for (i, &m) in self.minutes.iter().enumerate() {
                    buf[4 + i] = b'0' + m;
                }
                if subsecond % 2 == 1 {
                    buf[4 + self.selection as usize] = b' ';
                }
            }
            Mode::Counting => {
                let delta = self.now_ts.saturating_sub(self.target_ts);
                if self.now_ts <= self.target_ts {
                    buf[0] = b'S';
                    buf[1] = b'A';
                    buf[2] = b'1';
                    buf[3] = b'L';
                    buf[4] = b' ';
                    buf[5] = b' ';
                    buf[6] = b'0';
                    buf[7] = b'0';
                    buf[8] = b' ';
                    buf[9] = b' ';
                } else {
                    let hrs = delta / 3600;
                    let rem = delta % 3600;
                    let min = rem / 60;
                    let sec = rem % 60;
                    buf[0] = b'S';
                    buf[1] = b'L';
                    buf[2] = b'0' + self.lap / 10;
                    buf[3] = b'0' + self.lap % 10;
                    buf[4] = b'0' + (hrs / 10) as u8;
                    buf[5] = b'0' + (hrs % 10) as u8;
                    buf[6] = b'0' + (min / 10) as u8;
                    buf[7] = b'0' + (min % 10) as u8;
                    buf[8] = b'0' + (sec / 10) as u8;
                    buf[9] = b'0' + (sec % 10) as u8;
                }
            }
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn ring(&mut self, settings: &Settings) {
        movement::cancel_background_task_for_face(self.watch_face_index);
        if self.beepflag + 1 == BEEP_SECONDS.len() {
            if self.alarmflag != 0 {
                crate::movement::play_alarm_beeps(1, crate::watch::buzzer::Note::C8);
            }
            movement::cancel_background_task_for_face(self.watch_face_index);
            self.counting();
            return;
        }
        self.nextbeep_ts = self.target_ts - BEEP_SECONDS[self.beepflag + 1] as u32;
        let target_dt =
            utility::date_time_from_unix_time(self.nextbeep_ts, Self::tz_offset(settings));
        movement::schedule_background_task_for_face(self.watch_face_index, target_dt);
        for i in 0..5 {
            if BEEP_SECONDS[self.beepflag] == 60 * self.minutes[i] as i32 {
                if self.alarmflag > 1 {
                    crate::movement::play_alarm_beeps(1, crate::watch::buzzer::Note::C8);
                }
                self.ringflag = true;
            }
        }
        if !self.ringflag && self.alarmflag == 3 {
            crate::movement::play_alarm_beeps(1, crate::watch::buzzer::Note::C8);
        }
        self.ringflag = false;
        self.beepflag += 1;
    }

    fn start(&mut self, settings: &Settings) {
        while BEEP_SECONDS[self.beepflag] < self.minutes[self.index as usize] as i32 * 60 {
            self.index += 1;
        }
        while BEEP_SECONDS[self.beepflag] > self.minutes[self.index as usize] as i32 * 60 {
            self.beepflag += 1;
        }
        if self.index > 5 || self.minutes[self.index as usize] == 0 {
            let now = rtc::get_date_time();
            self.now_ts = utility::date_time_to_unix_time(now, Self::tz_offset(settings));
            self.target_ts = self.now_ts;
            if self.alarmflag != 0 {
                crate::movement::play_alarm_beeps(1, crate::watch::buzzer::Note::C8);
            }
            self.counting();
            return;
        }
        self.mode = Mode::Running;
        let now = rtc::get_date_time();
        self.now_ts = utility::date_time_to_unix_time(now, Self::tz_offset(settings));
        self.target_ts =
            utility::offset_timestamp(self.now_ts, 0, self.minutes[self.index as usize] as i8, 0);
        self.ring(settings);
    }

    fn settings_increment(&mut self) {
        self.minutes[self.selection as usize] += 1;
        let mut max = 11;
        if self.selection > 0 {
            max = self.minutes[(self.selection - 1) as usize];
        }
        if self.minutes[self.selection as usize] >= max {
            self.minutes[self.selection as usize] = 0;
        }
        if self.selection < 5 {
            for i in 0..5 {
                if self.minutes[i + 1] >= self.minutes[i] {
                    if self.minutes[i] > 0 {
                        self.minutes[i + 1] = self.minutes[i] - 1;
                    } else {
                        self.minutes[i + 1] = 0;
                    }
                }
            }
        }
    }
}

impl WatchFace for SailingFace {
    fn setup(&mut self, _settings: &Settings, watch_face_index: usize) {
        self.watch_face_index = watch_face_index;
    }

    fn activate(&mut self, settings: &Settings) {
        if self.mode == Mode::Running || self.mode == Mode::Counting {
            let now = rtc::get_date_time();
            self.now_ts = utility::date_time_to_unix_time(now, Self::tz_offset(settings));
        }
        if self.mode == Mode::Counting {
            watch::slcd::set_indicator(Indicator::Lap);
        }
        self.update_alarm_indicators();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => self.draw(0),
            Event::Tick => {
                if self.mode == Mode::Running || self.mode == Mode::Counting {
                    self.now_ts += 1;
                }
                self.draw(0);
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.mode == Mode::Running || self.mode == Mode::Counting {
                    self.reset();
                }
                if self.mode == Mode::Setting {
                    self.alarmflag = if self.alarmflag == 3 {
                        0
                    } else {
                        self.alarmflag + 1
                    };
                    self.update_alarm_indicators();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => match self.mode {
                Mode::Running | Mode::Counting => movement::illuminate_led(),
                Mode::Waiting => {
                    self.mode = Mode::Setting;
                }
                Mode::Setting => {
                    self.selection += 1;
                    if self.selection >= SL_SELECTIONS {
                        self.selection = 0;
                        self.mode = Mode::Waiting;
                    }
                }
            },
            Event::Button(Button::Alarm, ButtonEvent::Up) => match self.mode {
                Mode::Running | Mode::Waiting => self.start(settings),
                Mode::Setting => self.settings_increment(),
                Mode::Counting => {
                    if self.lap < 39 {
                        self.lap += 1;
                    }
                }
            },
            Event::BackgroundTask => self.ring(settings),
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.mode == Mode::Setting {
                    self.minutes = DEFAULT_MINUTES;
                    self.index = 0;
                    self.draw(0);
                }
                if self.mode == Mode::Counting {
                    self.lap = 0;
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        if self.mode == Mode::Setting {
            self.selection = 0;
            self.mode = Mode::Waiting;
        }
    }
}
