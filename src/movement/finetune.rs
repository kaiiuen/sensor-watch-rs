//! Finetune watch face.
//!
//! Port of the C `finetune_face.c`. Lets the user fine-tune the clock by
//! adding or removing subseconds. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd;
use crate::watch::utility;

/// The finetune face state.
pub struct FinetuneFace {
    total_adjustment: i32,
    finetune_page: u8,
    last_correction_time: u32,
    freq_correction: i32,
}

impl FinetuneFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        FinetuneFace {
            total_adjustment: 0,
            finetune_page: 0,
            last_correction_time: 0,
            freq_correction: 0,
        }
    }

    pub fn new() -> Self {
        FinetuneFace::new_static()
    }

    fn get_hours_passed(&self) -> f32 {
        let current_time = utility::date_time_to_unix_time(rtc::get_date_time(), 0);
        (current_time - self.last_correction_time) as f32 / 3600.0
    }

    fn get_correction(&self) -> f32 {
        self.total_adjustment as f32 / (self.get_hours_passed() * 3600.0) * 1000.0
    }

    fn update_display(&self) {
        let mut buf = [0u8; 11];
        if self.finetune_page == 0 {
            buf[0] = b'F';
            buf[1] = b'T';
            let date_time = rtc::get_date_time();
            buf[8] = b'0' + date_time.second / 10;
            buf[9] = b'0' + date_time.second % 10;
            let a = self.total_adjustment.unsigned_abs();
            buf[4] = b'0' + ((a / 1000) % 10) as u8;
            buf[5] = b'0' + ((a / 100) % 10) as u8;
            buf[6] = b'0' + ((a / 10) % 10) as u8;
            buf[7] = b'0' + (a % 10) as u8;
            if self.total_adjustment < 0 {
                buf[2] = b'-';
                buf[3] = b'-';
            } else {
                buf[2] = b' ';
                buf[3] = b' ';
            }
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        } else if self.finetune_page == 1 {
            let hours = self.get_hours_passed();
            buf[0] = b'D';
            buf[1] = b'T';
            buf[2] = b' ';
            buf[3] = b' ';
            buf[4] = b'0' + ((hours as i32 / 1000) % 10) as u8;
            buf[5] = b'0' + ((hours as i32 / 100) % 10) as u8;
            buf[6] = b'0' + ((hours as i32 / 10) % 10) as u8;
            buf[7] = b'0' + (hours as i32 % 10) as u8;
            let frac = libm::fmodf(hours, 1.0) * 100.0;
            buf[8] = b'0' + (frac as i32 / 10) as u8;
            buf[9] = b'0' + (frac as i32 % 10) as u8;
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        } else if self.finetune_page == 2 {
            if self.get_hours_passed() < 6.0 {
                buf[0] = b' ';
                buf[1] = b'F';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b'6';
                buf[5] = b'H';
                buf[6] = b'R';
                buf[7] = b' ';
                buf[8] = b' ';
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            } else {
                let correction = self.get_correction();
                buf[0] = b' ';
                buf[1] = b'F';
                buf[2] = if self.total_adjustment < 0 {
                    b'-'
                } else {
                    b' '
                };
                buf[3] = b' ';
                let c = correction.abs();
                buf[4] = b'0' + ((c as i32 / 10) % 10) as u8;
                buf[5] = b'0' + (c as i32 % 10) as u8;
                let frac = libm::fmodf(c, 1.0) * 10000.0;
                buf[6] = b'0' + ((frac as i32 / 1000) % 10) as u8;
                buf[7] = b'0' + ((frac as i32 / 100) % 10) as u8;
                buf[8] = b'0' + ((frac as i32 / 10) % 10) as u8;
                buf[9] = b'0' + (frac as i32 % 10) as u8;
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
        }
    }

    fn adjust_subseconds(&mut self, delta: i32) {
        if delta > 500 {
            self.total_adjustment += delta - 1000;
        } else {
            self.total_adjustment += delta;
        }
        self.update_display();
        let mut date_time = rtc::get_date_time();
        if delta > 500 {
            date_time.second = (date_time.second + 1) % 60;
            if date_time.second == 0 {
                date_time.minute = (date_time.minute + 1) % 60;
                if date_time.minute == 0 {
                    date_time.hour = (date_time.hour + 1) % 24;
                    if date_time.hour == 0 {
                        date_time.day += 1;
                    }
                }
            }
            let _ = rtc::set_date_time(date_time);
        }
    }

    fn update_correction_time(&mut self) {
        self.last_correction_time = utility::date_time_to_unix_time(rtc::get_date_time(), 0);
        movement::move_to_face(0);
    }
}

impl WatchFace for FinetuneFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        slcd::display_string("FT", 0);
        self.total_adjustment = 0;
        self.finetune_page = 0;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => self.update_display(),
            Event::Tick => {
                if self.finetune_page != 0 {
                    let date_time = rtc::get_date_time();
                    if date_time.second == 0 {
                        watch::led::set_led_green();
                        watch::led::set_led_off();
                    }
                }
                self.update_display();
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                if self.finetune_page == 0 && self.total_adjustment == 0 {
                    movement::move_to_next_face();
                } else {
                    self.finetune_page = (self.finetune_page + 1) % 3;
                    self.update_display();
                }
            }
            Event::Button(Button::Mode, ButtonEvent::LongPress) => {
                self.finetune_page = (self.finetune_page + 1) % 3;
                self.update_display();
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.finetune_page == 0 {
                    self.adjust_subseconds(250);
                } else if self.finetune_page == 2 && self.get_hours_passed() >= 6.0 {
                    self.freq_correction += (self.get_correction() * 100.0) as i32;
                    self.update_correction_time();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.finetune_page == 0 {
                    self.adjust_subseconds(25);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.finetune_page == 0 {
                    self.adjust_subseconds(750);
                } else if self.finetune_page == 2 {
                    self.update_correction_time();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.finetune_page == 0 {
                    self.adjust_subseconds(975);
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        if self.total_adjustment != 0 {
            self.update_correction_time();
        }
    }
}
