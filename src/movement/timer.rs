//! Timer watch face.
//!
//! Port of the C `timer_face.c`. A multi-slot countdown timer with pause,
//! repeat, and settings modes. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const TIMER_SLOTS: usize = 5;

/// Default timer values: 2 min, 5 min, 10 min, 20 min, 2 h 45 min.
const DEFAULT_TIMER_VALUES: [u32; 5] = [0x000200, 0x000500, 0x000A00, 0x001400, 0x002D02];

/// A single timer slot.
#[derive(Clone, Copy)]
struct TimerSlot {
    value: u32,
}

impl TimerSlot {
    fn hours(&self) -> u8 {
        ((self.value >> 16) & 0xFF) as u8
    }
    fn minutes(&self) -> u8 {
        ((self.value >> 8) & 0xFF) as u8
    }
    fn seconds(&self) -> u8 {
        (self.value & 0xFF) as u8
    }
    fn repeat(&self) -> bool {
        (self.value >> 24) & 0x1 != 0
    }
    fn set_hours(&mut self, v: u8) {
        self.value = (self.value & !(0xFF << 16)) | ((v as u32) << 16);
    }
    fn set_minutes(&mut self, v: u8) {
        self.value = (self.value & !(0xFF << 8)) | ((v as u32) << 8);
    }
    fn set_seconds(&mut self, v: u8) {
        self.value = (self.value & !0xFF) | (v as u32);
    }
    fn set_repeat(&mut self, v: bool) {
        self.value = (self.value & !(0x1 << 24)) | ((v as u32) << 24);
    }
    fn is_zero(&self) -> bool {
        (self.value & 0xFF_FFFF) == 0
    }
}

/// The timer mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Running,
    Pausing,
    Waiting,
    Setting,
}

/// The timer face state.
pub struct TimerFace {
    watch_face_index: usize,
    mode: Mode,
    timers: [TimerSlot; TIMER_SLOTS],
    current_timer: u8,
    settings_state: u8,
    erase_timer_flag: bool,
    quick_cycle: bool,
    now_ts: u32,
    target_ts: u32,
    paused_left: u32,
    pausing_seconds: u8,
}

impl TimerFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        TimerFace {
            watch_face_index: 0,
            mode: Mode::Waiting,
            timers: [
                TimerSlot {
                    value: DEFAULT_TIMER_VALUES[0],
                },
                TimerSlot {
                    value: DEFAULT_TIMER_VALUES[1],
                },
                TimerSlot {
                    value: DEFAULT_TIMER_VALUES[2],
                },
                TimerSlot {
                    value: DEFAULT_TIMER_VALUES[3],
                },
                TimerSlot {
                    value: DEFAULT_TIMER_VALUES[4],
                },
            ],
            current_timer: 0,
            settings_state: 0,
            erase_timer_flag: false,
            quick_cycle: false,
            now_ts: 0,
            target_ts: 0,
            paused_left: 0,
            pausing_seconds: 0,
        }
    }

    pub fn new() -> Self {
        TimerFace::new_static()
    }

    fn tz_offset(settings: &Settings) -> u32 {
        (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60) as u32
    }

    fn start(&mut self, settings: &Settings, with_beep: bool) {
        if self.timers[self.current_timer as usize].is_zero() {
            return;
        }
        let now = rtc::get_date_time();
        self.now_ts = utility::date_time_to_unix_time(now, Self::tz_offset(settings));
        if self.mode == Mode::Pausing {
            self.target_ts = self.now_ts + self.paused_left;
        } else {
            let t = self.timers[self.current_timer as usize];
            self.target_ts = utility::offset_timestamp(
                self.now_ts,
                t.hours() as i8,
                t.minutes() as i8,
                t.seconds() as i8,
            );
        }
        let target_dt =
            utility::date_time_from_unix_time(self.target_ts, Self::tz_offset(settings));
        self.mode = Mode::Running;
        movement::schedule_background_task_for_face(self.watch_face_index, target_dt);
        watch::slcd::set_indicator(Indicator::Bell);
        if with_beep {
            crate::movement::play_alarm_beeps(1, crate::watch::buzzer::Note::C8);
        }
    }

    fn reset(&mut self) {
        self.mode = Mode::Waiting;
        movement::cancel_background_task_for_face(self.watch_face_index);
        watch::slcd::clear_indicator(Indicator::Bell);
    }

    fn set_next_valid_timer(&mut self) {
        if self.timers[self.current_timer as usize].is_zero() {
            let mut i = self.current_timer;
            loop {
                i = (i + 1) % TIMER_SLOTS as u8;
                if !self.timers[i as usize].is_zero() || i == self.current_timer {
                    break;
                }
            }
            self.current_timer = i;
        }
    }

    fn resume_setting(&mut self) {
        self.settings_state = 0;
        self.mode = Mode::Waiting;
        self.set_next_valid_timer();
    }

    fn settings_increment(&mut self) {
        match self.settings_state {
            0 => self.current_timer = (self.current_timer + 1) % TIMER_SLOTS as u8,
            1 => self.erase_timer_flag = !self.erase_timer_flag,
            2 => {
                let t = &mut self.timers[self.current_timer as usize];
                t.set_hours((t.hours() + 1) % 24);
            }
            3 => {
                let t = &mut self.timers[self.current_timer as usize];
                t.set_minutes((t.minutes() + 1) % 60);
            }
            4 => {
                let t = &mut self.timers[self.current_timer as usize];
                t.set_seconds((t.seconds() + 1) % 60);
            }
            5 => {
                let t = &mut self.timers[self.current_timer as usize];
                t.set_repeat(!t.repeat());
            }
            _ => {}
        }
    }

    fn abort_quick_cycle(&mut self) {
        if self.quick_cycle {
            self.quick_cycle = false;
        }
    }

    fn draw(&mut self, subsecond: u8) {
        let mut buf = [0u8; 11];
        let mut h = 0u8;
        let mut min = 0u8;
        let mut sec = 0u8;

        match self.mode {
            Mode::Running | Mode::Pausing => {
                if self.mode == Mode::Pausing {
                    if self.pausing_seconds % 2 == 0 {
                        watch::slcd::set_indicator(Indicator::Bell);
                    } else {
                        watch::slcd::clear_indicator(Indicator::Bell);
                    }
                    if self.pausing_seconds != 1 {
                        return;
                    }
                }
                let delta = self.target_ts.saturating_sub(self.now_ts);
                sec = (delta % 60) as u8;
                let mins = delta / 60;
                h = (mins / 60) as u8;
                min = (mins % 60) as u8;
            }
            Mode::Setting => {
                if self.settings_state == 1 {
                    buf[6] = b'C';
                    buf[7] = b'L';
                    buf[8] = b'E';
                    buf[9] = b'A';
                    buf[10] = b'R';
                    watch::slcd::clear_colon();
                    buf[5] = if self.erase_timer_flag { b'y' } else { b'n' };
                } else if self.settings_state == 5 {
                    buf[6] = b'L';
                    buf[7] = b'O';
                    buf[8] = b'O';
                    buf[9] = b'P';
                    watch::slcd::clear_colon();
                    buf[5] = if self.timers[self.current_timer as usize].repeat() {
                        b'y'
                    } else {
                        b'n'
                    };
                } else {
                    let t = self.timers[self.current_timer as usize];
                    h = t.hours();
                    min = t.minutes();
                    sec = t.seconds();
                    watch::slcd::set_colon();
                }
            }
            Mode::Waiting => {
                let t = self.timers[self.current_timer as usize];
                h = t.hours();
                min = t.minutes();
                sec = t.seconds();
            }
        }

        buf[0] = b'1' + self.current_timer;
        buf[1] = b' ';
        buf[2] = b'0' + h / 10;
        buf[3] = b'0' + h % 10;
        buf[4] = b'0' + min / 10;
        buf[5] = b'0' + min % 10;
        buf[6] = b'0' + sec / 10;
        buf[7] = b'0' + sec % 10;

        if self.mode == Mode::Setting && subsecond % 2 == 1 {
            // Blink the current settings value.
            if self.settings_state == 0 {
                buf[0] = b' ';
            } else if self.settings_state == 1 || self.settings_state == 5 {
                buf[6] = b' ';
            } else {
                let idx = (self.settings_state - 1) as usize * 2;
                buf[idx + 1] = b' ';
                buf[idx + 2] = b' ';
            }
        }

        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 3);

        if self.timers[self.current_timer as usize].repeat() {
            watch::slcd::set_indicator(Indicator::Lap);
        } else {
            watch::slcd::clear_indicator(Indicator::Lap);
        }
    }
}

impl WatchFace for TimerFace {
    fn setup(&mut self, _settings: &Settings, watch_face_index: usize) {
        self.watch_face_index = watch_face_index;
    }

    fn activate(&mut self, _settings: &Settings) {
        watch::slcd::display_string("TR", 0);
        watch::slcd::set_colon();
        if self.mode == Mode::Running {
            let now = rtc::get_date_time();
            self.now_ts = utility::date_time_to_unix_time(now, Self::tz_offset(_settings));
            watch::slcd::set_indicator(Indicator::Bell);
        } else {
            self.pausing_seconds = 1;
        }
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        let mut subsecond = 0u8;
        match event {
            Event::Activate => self.draw(0),
            Event::Tick => {
                if self.mode == Mode::Running {
                    self.now_ts += 1;
                } else if self.mode == Mode::Pausing {
                    self.pausing_seconds = self.pausing_seconds.wrapping_add(1);
                } else if self.quick_cycle {
                    if watch::gpio::get_pin_level(watch::extint::BTN_ALARM) {
                        self.settings_increment();
                        subsecond = 0;
                    } else {
                        self.abort_quick_cycle();
                    }
                }
                self.draw(subsecond);
            }
            Event::Button(Button::Light, ButtonEvent::Down) => match self.mode {
                Mode::Pausing | Mode::Running => movement::illuminate_led(),
                Mode::Setting => {
                    if self.erase_timer_flag {
                        self.timers[self.current_timer as usize].value = 0;
                        self.erase_timer_flag = false;
                    }
                    self.settings_state = (self.settings_state + 1) % 6;
                    if self.settings_state == 1
                        && self.timers[self.current_timer as usize].is_zero()
                    {
                        self.settings_state = 2;
                    } else if self.settings_state == 5
                        && self.timers[self.current_timer as usize].is_zero()
                    {
                        self.settings_state = 0;
                    }
                }
                _ => {}
            },
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.mode == Mode::Waiting {
                    movement::illuminate_led();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.abort_quick_cycle();
                match self.mode {
                    Mode::Running => {
                        self.mode = Mode::Pausing;
                        self.pausing_seconds = 0;
                        self.paused_left = self.target_ts - self.now_ts;
                        movement::cancel_background_task_for_face(self.watch_face_index);
                    }
                    Mode::Pausing => self.start(settings, false),
                    Mode::Waiting => {
                        let last_timer = self.current_timer;
                        self.current_timer = (self.current_timer + 1) % TIMER_SLOTS as u8;
                        self.set_next_valid_timer();
                        if last_timer == self.current_timer {
                            self.start(settings, true);
                        }
                    }
                    Mode::Setting => {
                        self.settings_increment();
                        subsecond = 0;
                    }
                }
                self.draw(subsecond);
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.mode == Mode::Waiting {
                    self.mode = Mode::Setting;
                    self.settings_state = 0;
                    self.erase_timer_flag = false;
                } else if self.mode == Mode::Setting {
                    self.resume_setting();
                }
                self.draw(0);
            }
            Event::BackgroundTask => {
                crate::movement::play_alarm();
                self.reset();
                if self.timers[self.current_timer as usize].repeat() {
                    self.start(settings, false);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => match self.mode {
                Mode::Setting => match self.settings_state {
                    0 => self.current_timer = 0,
                    2 | 3 | 4 => self.quick_cycle = true,
                    _ => {}
                },
                Mode::Waiting => self.start(settings, true),
                Mode::Pausing | Mode::Running => {
                    self.reset();
                    if settings.button_should_sound() {
                        crate::movement::play_alarm_beeps(1, crate::watch::buzzer::Note::C7);
                    }
                }
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::LongUp) => self.abort_quick_cycle(),
            Event::Button(Button::Mode, ButtonEvent::LongPress) => {
                self.abort_quick_cycle();
                movement::move_to_face(0);
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        if self.mode == Mode::Setting {
            self.settings_state = 0;
            self.mode = Mode::Waiting;
        }
    }
}
