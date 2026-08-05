//! ISH (vague time) watch face.
//!
//! Port of the C `ish_face.c`. Displays an intentionally vague time with three
//! configurable vagueness levels. It is a pure state machine: it renders on
//! wake and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, ClockMode, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc::DateTime;

/// Minimum and maximum vagueness levels.
const ISH_LEVEL_MIN: u8 = 1;
const ISH_LEVEL_MAX: u8 = 3;

/// The ISH face state.
pub struct IshFace {
    vagueness_level: u8,
    last_displayed_minute: u8,
}

impl IshFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        IshFace {
            vagueness_level: 1,
            last_displayed_minute: 0xFF,
        }
    }

    pub fn new() -> Self {
        IshFace::new_static()
    }

    /// Returns true if the vague time should update based on the current minute.
    fn should_update(&mut self, date_time: DateTime) -> bool {
        let current_minute = date_time.minute;
        if current_minute != self.last_displayed_minute {
            self.last_displayed_minute = current_minute;
            return true;
        }
        false
    }

    /// Updates the display with the current vague time.
    fn update_display(&mut self, date_time: DateTime) {
        let mut buf = [0u8; 8];
        let mut hour = date_time.hour;
        let minute = date_time.minute;
        // Support 12/24h mode.
        if movement::clock_mode_24h() == ClockMode::H12 {
            hour = hour % 12;
            if hour == 0 {
                hour = 12;
            }
        }
        let mut len;
        match self.vagueness_level {
            1 => {
                // Level 1: Hour, switch at the 30 minute mark.
                if minute >= 30 {
                    hour = (hour + 1) % 24;
                }
                buf[0] = b'0' + hour / 10;
                buf[1] = b'0' + hour % 10;
                len = 2;
            }
            2 => {
                // Level 2: Half hour, "o" instead of "0" to signify vagueness.
                let mut h = hour;
                let min_str: [u8; 2];
                if minute < 15 || minute >= 45 {
                    if minute >= 45 {
                        h = (hour + 1) % 24;
                    }
                    min_str = *b"0o";
                } else {
                    min_str = *b"3o";
                }
                buf[0] = b'0' + h / 10;
                buf[1] = b'0' + h % 10;
                buf[2] = min_str[0];
                buf[3] = min_str[1];
                len = 4;
            }
            3 => {
                // Level 3: Quarter hour.
                let mut h = hour;
                let min_str: [u8; 2];
                if minute < 8 {
                    min_str = *b"00";
                } else if minute < 23 {
                    min_str = *b"15";
                } else if minute < 38 {
                    min_str = *b"30";
                } else if minute < 53 {
                    min_str = *b"45";
                } else {
                    h = (hour + 1) % 24;
                    min_str = *b"00";
                }
                buf[0] = b'0' + h / 10;
                buf[1] = b'0' + h % 10;
                buf[2] = min_str[0];
                buf[3] = min_str[1];
                len = 4;
            }
            _ => {
                buf[0] = b'0' + hour / 10;
                buf[1] = b'0' + hour % 10;
                len = 2;
            }
        }
        // Pad buf with spaces to 5 characters to clear leftover segments.
        while len < 5 {
            buf[len] = b' ';
            len += 1;
        }

        watch::slcd::display_string("ISH", 0);
        watch::slcd::display_string(core::str::from_utf8(&buf[..5]).unwrap_or(""), 4);
        watch::slcd::set_colon();
        watch::slcd::display_string("  ", 8);
    }
}

impl WatchFace for IshFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.last_displayed_minute = 0xFF; // Force update on activation.
        let date_time = movement::get_local_date_time();
        self.update_display(date_time);
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Tick => {
                let date_time = movement::get_local_date_time();
                if self.should_update(date_time) {
                    self.update_display(date_time);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                // Cycle through vagueness levels 1 -> 2 -> 3 -> 1.
                self.vagueness_level += 1;
                if self.vagueness_level > ISH_LEVEL_MAX {
                    self.vagueness_level = ISH_LEVEL_MIN;
                }
                self.last_displayed_minute = 0xFF; // Force update.
                let date_time = movement::get_local_date_time();
                self.update_display(date_time);
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
