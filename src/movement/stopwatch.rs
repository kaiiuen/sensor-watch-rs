//! Stopwatch watch face.
//!
//! Port of the C `stopwatch_face.c`. A simple stopwatch that counts elapsed
//! time. While running, it schedules a far-future background task so the watch
//! stays awake (the CPU still sleeps between ticks; it just never drops into
//! low-energy mode). It is a pure state machine: it reacts to a single event
//! and returns.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc::{self, DateTime};
use crate::watch::utility;

/// A distant-future date (January 1, 2083) used to keep the watch awake while
/// the stopwatch is running.
const DISTANT_FUTURE: DateTime = DateTime {
    second: 0,
    minute: 0,
    hour: 0,
    day: 1,
    month: 1,
    year: 63,
};

/// The stopwatch face state.
pub struct StopwatchFace {
    running: bool,
    start_time: DateTime,
    seconds_counted: u32,
}

impl StopwatchFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        StopwatchFace {
            running: false,
            start_time: DateTime {
                second: 0,
                minute: 0,
                hour: 0,
                day: 0,
                month: 0,
                year: 0,
            },
            seconds_counted: 0,
        }
    }

    pub fn new() -> Self {
        StopwatchFace::new_static()
    }

    fn update_display(&mut self, show_seconds: bool) {
        if self.running {
            let now = rtc::get_date_time();
            let now_ts = utility::date_time_to_unix_time(now, 0);
            let start_ts = utility::date_time_to_unix_time(self.start_time, 0);
            self.seconds_counted = now_ts - start_ts;
        }

        if self.seconds_counted >= 3456000 {
            // Display maxes out just shy of 40 days.
            self.running = false;
            movement::cancel_background_task();
            watch::slcd::display_string("st39235959", 0);
            return;
        }

        let duration = utility::seconds_to_duration(self.seconds_counted);
        let mut buf = [0u8; 11];
        buf[0] = b's';
        buf[1] = b't';
        buf[2] = b' ';
        buf[3] = b' ';
        buf[4] = b'0' + duration.hours / 10;
        buf[5] = b'0' + duration.hours % 10;
        buf[6] = b'0' + duration.minutes / 10;
        buf[7] = b'0' + duration.minutes % 10;
        buf[8] = b' ';
        buf[9] = b' ';
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);

        if duration.days != 0 {
            let mut db = [0u8; 2];
            db[0] = b'0' + ((duration.days % 100) / 10) as u8;
            db[1] = b'0' + (duration.days % 10) as u8;
            watch::slcd::display_string(core::str::from_utf8(&db[..]).unwrap_or("  "), 2);
        }

        if show_seconds {
            let mut sb = [0u8; 2];
            sb[0] = b'0' + duration.seconds / 10;
            sb[1] = b'0' + duration.seconds % 10;
            watch::slcd::display_string(core::str::from_utf8(&sb[..]).unwrap_or("  "), 8);
        }
    }
}

impl WatchFace for StopwatchFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        if watch::slcd::tick_animation_is_running() {
            watch::slcd::stop_tick_animation();
        }
        if self.running {
            // Keep the watch awake while the stopwatch is on screen.
            movement::schedule_background_task(DISTANT_FUTURE);
        }
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => {
                watch::slcd::set_colon();
                if self.start_time.to_reg() == 0 {
                    watch::slcd::display_string("st  000000", 0);
                } else {
                    self.update_display(true);
                }
            }
            Event::Tick => {
                if self.start_time.to_reg() == 0 {
                    watch::slcd::display_string("st  000000", 0);
                } else {
                    self.update_display(true);
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                movement::illuminate_led();
                if !self.running {
                    self.start_time = DateTime {
                        second: 0,
                        minute: 0,
                        hour: 0,
                        day: 0,
                        month: 0,
                        year: 0,
                    };
                    self.seconds_counted = 0;
                    watch::slcd::display_string("st  000000", 0);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Down) => {
                if settings.button_should_sound() {
                    crate::movement::play_alarm_beeps(1, crate::watch::buzzer::Note::C8);
                }
                self.running = !self.running;
                if self.running {
                    if self.start_time.to_reg() == 0 {
                        self.start_time = rtc::get_date_time();
                    } else {
                        let mut timestamp =
                            utility::date_time_to_unix_time(rtc::get_date_time(), 0);
                        timestamp -= self.seconds_counted;
                        self.start_time = utility::date_time_from_unix_time(timestamp, 0);
                    }
                    movement::schedule_background_task(DISTANT_FUTURE);
                } else {
                    movement::cancel_background_task();
                }
            }
            Event::BackgroundTask => {
                // Keepalive: the stopwatch is running; just re-render.
                self.update_display(true);
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        movement::cancel_background_task();
    }
}
