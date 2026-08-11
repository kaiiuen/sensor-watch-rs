//! Countdown timer watch face.
//!
//! Port of the C `countdown_face.c`, adapted to the event-driven model. The
//! countdown is scheduled via the RTC alarm (a background task), so the CPU
//! stays asleep between updates - it never polls.

use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::movement::{self, TIMEZONE_OFFSETS};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

/// The number of selectable fields (hours, minutes, seconds).
const CD_SELECTIONS: u8 = 3;
/// The default countdown length in minutes.
const DEFAULT_MINUTES: u8 = 3;

/// The countdown mode.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Running,
    Reset,
    Paused,
    Setting,
}

/// The state for the countdown face.
pub struct CountdownFace {
    watch_face_index: usize,
    mode: Mode,
    selection: u8,
    hours: u8,
    minutes: u8,
    seconds: u8,
    set_hours: u8,
    set_minutes: u8,
    set_seconds: u8,
    target_ts: u32,
    now_ts: u32,
    repeat: bool,
}

impl CountdownFace {
    pub const fn new_static() -> Self {
        CountdownFace {
            watch_face_index: 0,
            mode: Mode::Reset,
            selection: 0,
            hours: 0,
            minutes: DEFAULT_MINUTES,
            seconds: 0,
            set_hours: 0,
            set_minutes: DEFAULT_MINUTES,
            set_seconds: 0,
            target_ts: 0,
            now_ts: 0,
            repeat: false,
        }
    }

    fn tz_offset(settings: &Settings) -> u32 {
        (TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)] as i32 * 60) as u32
    }

    fn store(&mut self) {
        self.set_hours = self.hours;
        self.set_minutes = self.minutes;
        self.set_seconds = self.seconds;
    }

    fn load(&mut self) {
        self.hours = self.set_hours;
        self.minutes = self.set_minutes;
        self.seconds = self.set_seconds;
    }

    fn schedule(&mut self, settings: &Settings) {
        let now = rtc::get_date_time();
        let new_now = utility::date_time_to_unix_time(now, Self::tz_offset(settings));
        self.target_ts = utility::offset_timestamp(
            new_now,
            self.hours as i8,
            self.minutes as i8,
            self.seconds as i8,
        );
        self.now_ts = new_now;
        let target_dt =
            utility::date_time_from_unix_time(self.target_ts, Self::tz_offset(settings));
        movement::schedule_background_task_for_face(self.watch_face_index, target_dt);
    }

    fn start(&mut self, settings: &Settings) {
        self.mode = Mode::Running;
        self.schedule(settings);
    }

    fn pause(&mut self) {
        self.mode = Mode::Paused;
        movement::cancel_background_task_for_face(self.watch_face_index);
        watch::slcd::clear_indicator(Indicator::Signal);
    }

    fn reset(&mut self) {
        self.mode = Mode::Reset;
        movement::cancel_background_task_for_face(self.watch_face_index);
        self.load();
    }

    fn ring(&mut self) {
        movement::play_alarm();
        self.reset();
    }

    fn times_up(&mut self, settings: &Settings) {
        if self.repeat {
            movement::play_alarm();
            self.load();
            self.schedule(settings);
        } else {
            self.ring();
        }
    }

    fn settings_increment(&mut self) {
        match self.selection {
            0 => self.hours = (self.hours + 1) % 24,
            1 => self.minutes = (self.minutes + 1) % 60,
            2 => self.seconds = (self.seconds + 1) % 60,
            _ => {}
        }
    }

    fn draw(&mut self) {
        let mut buf = [0u8; 11];
        buf[0] = b'C';
        buf[1] = b'D';
        buf[2] = b' ';
        buf[3] = b' ';

        match self.mode {
            Mode::Running => {
                let delta = self.target_ts.saturating_sub(self.now_ts);
                self.seconds = (delta % 60) as u8;
                let mins = delta / 60;
                self.hours = (mins / 60) as u8;
                self.minutes = (mins % 60) as u8;
            }
            Mode::Reset | Mode::Paused => {
                watch::slcd::clear_indicator(Indicator::Signal);
            }
            Mode::Setting => {}
        }

        buf[4] = b'0' + self.hours / 10;
        buf[5] = b'0' + self.hours % 10;
        buf[6] = b'0' + self.minutes / 10;
        buf[7] = b'0' + self.minutes % 10;
        buf[8] = b'0' + self.seconds / 10;
        buf[9] = b'0' + self.seconds % 10;

        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

impl WatchFace for CountdownFace {
    fn setup(&mut self, _settings: &Settings, watch_face_index: usize) {
        self.watch_face_index = watch_face_index;
    }

    fn activate(&mut self, settings: &Settings) {
        if self.mode == Mode::Running {
            let now = rtc::get_date_time();
            self.now_ts = utility::date_time_to_unix_time(now, Self::tz_offset(settings));
            watch::slcd::set_indicator(Indicator::Signal);
        }
        watch::slcd::set_colon();
        if self.repeat {
            watch::slcd::set_indicator(Indicator::Bell);
        }
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => self.draw(),
            Event::Tick => {
                if self.mode == Mode::Running {
                    self.now_ts += 1;
                }
                self.draw();
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => movement::move_to_next_face(),
            Event::Button(Button::Light, ButtonEvent::Up) => match self.mode {
                Mode::Running => movement::illuminate_led(),
                Mode::Paused => self.reset(),
                Mode::Reset => {
                    self.mode = Mode::Setting;
                }
                Mode::Setting => {
                    self.selection += 1;
                    if self.selection >= CD_SELECTIONS {
                        self.selection = 0;
                        self.mode = Mode::Reset;
                        self.store();
                    }
                }
            },
            Event::Button(Button::Alarm, ButtonEvent::Up) => match self.mode {
                Mode::Running => self.pause(),
                Mode::Reset | Mode::Paused => {
                    if !(self.hours == 0 && self.minutes == 0 && self.seconds == 0) {
                        self.start(settings);
                        watch::slcd::set_indicator(Indicator::Signal);
                    }
                }
                Mode::Setting => self.settings_increment(),
            },
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => match self.mode {
                Mode::Setting => {}
                _ => {
                    self.repeat = !self.repeat;
                    if self.repeat {
                        watch::slcd::set_indicator(Indicator::Bell);
                    } else {
                        watch::slcd::clear_indicator(Indicator::Bell);
                    }
                }
            },
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.mode == Mode::Setting {
                    match self.selection {
                        0 => self.hours = 0,
                        1 => self.minutes = 0,
                        _ => self.seconds = 0,
                    }
                }
            }
            Event::BackgroundTask => self.times_up(settings),
            _ => movement::default_loop_handler(event, settings),
        }
        self.draw();
    }

    fn resign(&mut self, _settings: &mut Settings) {
        if self.mode == Mode::Setting {
            self.selection = 0;
            self.mode = Mode::Reset;
            self.store();
        }
    }
}
