//! Dual timer watch face.
//!
//! Port of the C `dual_timer_face.c`. Two independent stopwatches driven by a
//! 128 Hz TC2 hardware counter. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc::{self, DateTime};
use crate::watch::slcd;

/// A distant-future date used to keep the watch awake while running.
const DISTANT_FUTURE: DateTime = DateTime {
    second: 0,
    minute: 0,
    hour: 0,
    day: 1,
    month: 1,
    year: 63,
};

/// The 1 Hz tick counter.
static mut TICKS: u32 = 0;
static mut IS_RUNNING: bool = false;

/// A duration.
#[derive(Clone, Copy)]
struct Duration {
    centiseconds: u8,
    seconds: u8,
    minutes: u8,
    hours: u8,
    days: u8,
}

impl Duration {
    const fn zero() -> Self {
        Duration {
            centiseconds: 0,
            seconds: 0,
            minutes: 0,
            hours: 0,
            days: 0,
        }
    }
}

fn ticks_to_duration(mut ticks: u32) -> Duration {
    let mut hours = 0u8;
    let mut days = 0u8;
    while ticks >= (60 * 60) {
        ticks -= (60 * 60);
        hours += 1;
        if hours >= 24 {
            hours -= 24;
            days += 1;
        }
    }
    Duration {
        centiseconds: 0,
        seconds: (ticks % 60) as u8,
        minutes: (ticks / 60) as u8,
        hours,
        days,
    }
}

/// The dual timer face state.
pub struct DualTimerFace {
    running: [bool; 2],
    start_ticks: [u32; 2],
    stop_ticks: [u32; 2],
    duration: [Duration; 2],
    show: bool,
}

impl DualTimerFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        DualTimerFace {
            running: [false; 2],
            start_ticks: [0; 2],
            stop_ticks: [0; 2],
            duration: [Duration::zero(); 2],
            show: false,
        }
    }

    pub fn new() -> Self {
        DualTimerFace::new_static()
    }

    fn start_timer(&mut self, timer: usize) {
        if !unsafe { IS_RUNNING } {
            unsafe { IS_RUNNING = true };
            self.start_ticks[timer] = 0;
            self.stop_ticks[timer] = 0;
            unsafe { TICKS = 0 };
            movement::schedule_background_task(DISTANT_FUTURE);
        } else {
            self.start_ticks[timer] = unsafe { TICKS };
            self.stop_ticks[timer] = unsafe { TICKS };
        }
        self.running[timer] = true;
    }

    fn stop_timer(&mut self, timer: usize) {
        self.stop_ticks[timer] = unsafe { TICKS };
        self.duration[timer] = ticks_to_duration(self.stop_ticks[timer] - self.start_ticks[timer]);
        self.running[timer] = false;
        if !self.running[1 - timer] {
            unsafe { IS_RUNNING = false };
            movement::cancel_background_task();
        }
    }

    fn display(&mut self) {
        let mut buf = [0u8; 11];
        let timer = if self.running[self.show as usize] {
            ticks_to_duration(
                self.stop_ticks[self.show as usize] - self.start_ticks[self.show as usize],
            )
        } else {
            self.duration[self.show as usize]
        };
        let other = ticks_to_duration(
            self.stop_ticks[!self.show as usize] - self.start_ticks[!self.show as usize],
        );
        if timer.days > 0 {
            buf[0] = b'0' + timer.days / 10;
            buf[1] = b'0' + timer.days % 10;
            buf[2] = b'0' + timer.hours / 10;
            buf[3] = b'0' + timer.hours % 10;
            buf[4] = b'0' + timer.minutes / 10;
            buf[5] = b'0' + timer.minutes % 10;
        } else if timer.hours > 0 {
            buf[0] = b'0' + timer.hours / 10;
            buf[1] = b'0' + timer.hours % 10;
            buf[2] = b'0' + timer.minutes / 10;
            buf[3] = b'0' + timer.minutes % 10;
            buf[4] = b'0' + timer.seconds / 10;
            buf[5] = b'0' + timer.seconds % 10;
        } else {
            buf[0] = b'0' + timer.minutes / 10;
            buf[1] = b'0' + timer.minutes % 10;
            buf[2] = b'0' + timer.seconds / 10;
            buf[3] = b'0' + timer.seconds % 10;
            buf[4] = b'0' + timer.centiseconds / 10;
            buf[5] = b'0' + timer.centiseconds % 10;
        }
        slcd::display_string(core::str::from_utf8(&buf[..6]).unwrap_or(""), 4);
        slcd::display_string(if self.show { "B" } else { "A" }, 0);
        slcd::display_string(
            if self.running[!self.show as usize] && (unsafe { TICKS } % 100) < 50 {
                "+"
            } else {
                " "
            },
            1,
        );
        let oi = if other.days > 0 {
            other.days
        } else if other.hours > 0 {
            other.hours
        } else if other.minutes > 0 {
            other.minutes
        } else if other.seconds > 0 {
            other.seconds
        } else {
            other.centiseconds
        };
        if self.stop_ticks[!self.show as usize] - self.start_ticks[!self.show as usize] > 0 {
            let mut ob = [0u8; 3];
            ob[0] = b'0' + oi / 10;
            ob[1] = b'0' + oi % 10;
            slcd::display_string(core::str::from_utf8(&ob[..2]).unwrap_or("  "), 2);
        } else {
            slcd::display_string("  ", 2);
        }
        if timer.centiseconds > 50 || !self.running[self.show as usize] {
            slcd::set_colon();
        } else {
            slcd::clear_colon();
        }
    }
}

impl WatchFace for DualTimerFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {
        unsafe { TICKS = 0 };
    }

    fn activate(&mut self, _settings: &Settings) {
        if unsafe { IS_RUNNING } {
            movement::schedule_background_task(DISTANT_FUTURE);
        }
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        if (unsafe { TICKS } - self.start_ticks[0]) >= 1105919999 {
            self.stop_timer(0);
        }
        if (unsafe { TICKS } - self.start_ticks[1]) >= 1105919999 {
            self.stop_timer(1);
        }
        match event {
            Event::Activate => {
                slcd::set_colon();
                if unsafe { IS_RUNNING } {
                    if self.running[0] {
                        self.show = false;
                    } else {
                        self.show = true;
                    }
                } else if self.stop_ticks[0] > 0 || self.stop_ticks[1] > 0 {
                    self.display();
                } else {
                    slcd::display_string("A   000000", 0);
                }
            }
            Event::Tick => {
                if unsafe { IS_RUNNING } {
                    unsafe { TICKS += 1 };
                    if self.running[0] {
                        self.stop_ticks[0] = unsafe { TICKS };
                    }
                    if self.running[1] {
                        self.stop_ticks[1] = unsafe { TICKS };
                    }
                    self.display();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                self.running[1] = !self.running[1];
                if self.running[1] {
                    self.start_timer(1);
                } else {
                    self.stop_timer(1);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Down) => {
                self.running[0] = !self.running[0];
                if self.running[0] {
                    self.start_timer(0);
                } else {
                    self.stop_timer(0);
                }
            }
            Event::Button(Button::Mode, ButtonEvent::Down) => {
                self.show = !self.show;
                self.display();
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => {}
            Event::Button(Button::Mode, ButtonEvent::LongPress) => movement::move_to_next_face(),
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        movement::cancel_background_task();
    }
}
