//! LIS2DW logging watch face.
//!
//! Port of the C `lis2dw_logging_face.c`. Logs accelerometer interrupt events
//! (requires the optional accelerometer). It is a pure state machine: it
//! reacts to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;

const LIS2DW_LOGGING_NUM_DATA_POINTS: u8 = 96;

/// A logged data point.
#[derive(Clone, Copy)]
struct DataPoint {
    timestamp: rtc::DateTime,
    x_interrupts: u32,
    y_interrupts: u32,
    z_interrupts: u32,
}

impl DataPoint {
    const fn zero() -> Self {
        DataPoint {
            timestamp: rtc::DateTime {
                second: 0,
                minute: 0,
                hour: 0,
                day: 0,
                month: 0,
                year: 0,
            },
            x_interrupts: 0,
            y_interrupts: 0,
            z_interrupts: 0,
        }
    }
}

/// The LIS2DW logging face state.
pub struct Lis2dwLoggingFace {
    data: [DataPoint; LIS2DW_LOGGING_NUM_DATA_POINTS as usize],
    data_points: u16,
    display_index: u8,
    log_ticks: u8,
    axis_index: u8,
    interrupts: [u32; 3],
    x_interrupts_this_hour: u32,
    y_interrupts_this_hour: u32,
    z_interrupts_this_hour: u32,
}

impl Lis2dwLoggingFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        Lis2dwLoggingFace {
            data: [DataPoint::zero(); LIS2DW_LOGGING_NUM_DATA_POINTS as usize],
            data_points: 0,
            display_index: 0,
            log_ticks: 0,
            axis_index: 0,
            interrupts: [0; 3],
            x_interrupts_this_hour: 0,
            y_interrupts_this_hour: 0,
            z_interrupts_this_hour: 0,
        }
    }

    pub fn new() -> Self {
        Lis2dwLoggingFace::new_static()
    }

    fn update_display(&self, settings: &Settings, wakeup: bool) {
        let mut buf = [0u8; 11];
        if self.log_ticks != 0 {
            let pos = (self.data_points as i32 - 1 - self.display_index as i32)
                % LIS2DW_LOGGING_NUM_DATA_POINTS as i32;
            if pos < 0 {
                watch::slcd::clear_colon();
                buf[0] = b'N';
                buf[1] = b'O';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b'd';
                buf[5] = b'a';
                buf[6] = b't';
                buf[7] = b'a';
                buf[8] = b' ';
                buf[9] = b' ';
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
                return;
            }
            let mut dt = self.data[pos as usize].timestamp;
            watch::slcd::set_colon();
            let mut set_leading_zero = false;
            if !settings.clock_mode_24h() {
                if dt.hour > 11 {
                    watch::slcd::set_indicator(Indicator::Pm);
                }
                dt.hour %= 12;
                if dt.hour == 0 {
                    dt.hour = 12;
                }
            } else if !settings.clock_24h_leading_zero() {
                watch::slcd::set_indicator(Indicator::H24);
            } else if dt.hour < 10 {
                set_leading_zero = true;
            }
            let d = self.data[pos as usize];
            let total = d.x_interrupts + d.y_interrupts + d.z_interrupts;
            let prefix = match self.axis_index {
                0 => "3A",
                1 => "XA",
                2 => "YA",
                _ => "ZA",
            };
            let pb = prefix.as_bytes();
            buf[0] = pb[0];
            buf[1] = pb[1];
            buf[2] = b'0' + dt.hour / 10;
            buf[3] = b'0' + dt.hour % 10;
            buf[4] = b'0' + dt.minute / 10;
            buf[5] = b'0' + dt.minute % 10;
            let v = match self.axis_index {
                0 => total,
                1 => d.x_interrupts,
                2 => d.y_interrupts,
                _ => d.z_interrupts,
            };
            write_num(&mut buf, v, 6, 4);
            watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            if set_leading_zero {
                watch::slcd::display_string("0", 4);
            }
        } else {
            let date_time = rtc::get_date_time();
            watch::slcd::clear_colon();
            watch::slcd::clear_indicator(Indicator::Pm);
            watch::slcd::clear_indicator(Indicator::H24);
            let time_char = if (59 - date_time.second) < 10 {
                b'0' + (59 - date_time.second)
            } else if date_time.second % 2 == 1 {
                b'i'
            } else {
                b'_'
            };
            buf[0] = if wakeup { b'Y' } else { b' ' };
            buf[1] = if wakeup { b'X' } else { b' ' };
            buf[2] = if wakeup { b'Z' } else { b' ' };
            buf[3] = time_char;
            write_num(&mut buf, self.interrupts[0], 4, 2);
            write_num(&mut buf, self.interrupts[1], 6, 2);
            write_num(&mut buf, self.interrupts[2], 8, 2);
            watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        }
    }

    fn log_data(&mut self) {
        let mut date_time = rtc::get_date_time();
        if date_time.minute == 0 {
            date_time.hour = (date_time.hour + 23) % 24;
        }
        date_time.minute = (date_time.minute + 45) % 60;
        let pos = self.data_points as usize % LIS2DW_LOGGING_NUM_DATA_POINTS as usize;
        self.data[pos].timestamp = date_time;
        self.data[pos].x_interrupts = self.x_interrupts_this_hour;
        self.data[pos].y_interrupts = self.y_interrupts_this_hour;
        self.data[pos].z_interrupts = self.z_interrupts_this_hour;
        self.data_points = self.data_points.saturating_add(1);
        self.x_interrupts_this_hour = 0;
        self.y_interrupts_this_hour = 0;
        self.z_interrupts_this_hour = 0;
    }
}

/// Writes a number right-aligned into the buffer at the given offset.
fn write_num(buf: &mut [u8; 11], value: u32, offset: usize, width: usize) {
    let mut v = value;
    let mut i = offset + width - 1;
    loop {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if i == offset || v == 0 {
            break;
        }
        i -= 1;
    }
}

impl WatchFace for Lis2dwLoggingFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.display_index = 0;
        self.log_ticks = 0;
        // Enable tap detection so interrupts are counted.
        movement::enable_tap_detection_if_available();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Button(Button::Light, ButtonEvent::Down) => {
                self.axis_index = (self.axis_index + 1) % 4;
                self.log_ticks = 255;
                self.update_display(settings, false);
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.log_ticks != 0 {
                    self.display_index = (self.display_index + 1) % LIS2DW_LOGGING_NUM_DATA_POINTS;
                }
                self.log_ticks = 255;
                self.axis_index = 0;
                self.update_display(settings, false);
            }
            Event::Activate | Event::Tick => {
                if self.log_ticks > 0 {
                    self.log_ticks -= 1;
                } else {
                    self.display_index = 0;
                }
                self.update_display(settings, false);
            }
            // Count accelerometer taps as interrupts.
            Event::SingleTap | Event::DoubleTap => {
                self.interrupts[0] = self.interrupts[0].wrapping_add(1);
                self.x_interrupts_this_hour = self.x_interrupts_this_hour.wrapping_add(1);
                self.update_display(settings, true);
            }
            Event::BackgroundTask => self.log_data(),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        movement::disable_tap_detection_if_available();
    }

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        self.interrupts[2] = self.interrupts[1];
        self.interrupts[1] = self.interrupts[0];
        self.interrupts[0] = 0;
        let date_time = rtc::get_date_time();
        date_time.minute % 15 == 0
    }
}
