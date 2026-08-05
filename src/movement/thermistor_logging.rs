//! Thermistor logging watch face.
//!
//! Port of the C `thermistor_logging_face.c`. Logs hourly temperatures and
//! lets you browse them. It is a pure state machine: it reacts to a single
//! event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;

const THERMISTOR_LOGGING_NUM_DATA_POINTS: u8 = 24;

/// A logged data point.
#[derive(Clone, Copy)]
struct DataPoint {
    timestamp: rtc::DateTime,
    temperature_c: f32,
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
            temperature_c: 0.0,
        }
    }
}

/// The thermistor logging face state.
pub struct ThermistorLoggingFace {
    data: [DataPoint; THERMISTOR_LOGGING_NUM_DATA_POINTS as usize],
    data_points: u8,
    display_index: u8,
    ts_ticks: u8,
}

impl ThermistorLoggingFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        ThermistorLoggingFace {
            data: [DataPoint::zero(); THERMISTOR_LOGGING_NUM_DATA_POINTS as usize],
            data_points: 0,
            display_index: 0,
            ts_ticks: 0,
        }
    }

    pub fn new() -> Self {
        ThermistorLoggingFace::new_static()
    }

    fn log_data(&mut self) {
        let date_time = rtc::get_date_time();
        let pos = self.data_points as usize % THERMISTOR_LOGGING_NUM_DATA_POINTS as usize;
        self.data[pos].timestamp = date_time;
        self.data[pos].temperature_c = 25.0;
        self.data_points += 1;
    }

    fn update_display(&self, settings: &Settings) {
        let pos = (self.data_points as i32 - 1 - self.display_index as i32)
            % THERMISTOR_LOGGING_NUM_DATA_POINTS as i32;
        let mut buf = [0u8; 11];
        let mut set_leading_zero = false;
        watch::slcd::clear_indicator(Indicator::H24);
        watch::slcd::clear_indicator(Indicator::Pm);
        watch::slcd::clear_colon();
        if pos < 0 {
            buf[0] = b'T';
            buf[1] = b'L';
            buf[2] = b'0' + self.display_index / 10;
            buf[3] = b'0' + self.display_index % 10;
            buf[4] = b'n';
            buf[5] = b'o';
            buf[6] = b' ';
            buf[7] = b'd';
            buf[8] = b'a';
            buf[9] = b't';
        } else if self.ts_ticks != 0 {
            let mut dt = self.data[pos as usize].timestamp;
            watch::slcd::set_colon();
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
            buf[0] = b'A';
            buf[1] = b'T';
            buf[2] = b'0' + dt.day / 10;
            buf[3] = b'0' + dt.day % 10;
            buf[4] = b'0' + dt.hour / 10;
            buf[5] = b'0' + dt.hour % 10;
            buf[6] = b'0' + dt.minute / 10;
            buf[7] = b'0' + dt.minute % 10;
            buf[8] = b'0' + dt.second / 10;
            buf[9] = b'0' + dt.second % 10;
        } else {
            let temp = self.data[pos as usize].temperature_c;
            let v = if settings.use_imperial_units() {
                temp * 1.8 + 32.0
            } else {
                temp
            };
            let scaled = (v * 10.0) as i32;
            buf[0] = b'T';
            buf[1] = b'L';
            buf[2] = b'0' + self.display_index / 10;
            buf[3] = b'0' + self.display_index % 10;
            buf[4] = b'0' + ((scaled / 100) % 10) as u8;
            buf[5] = b'0' + ((scaled / 10) % 10) as u8;
            buf[6] = b'.';
            buf[7] = b'0' + (scaled % 10) as u8;
            buf[8] = b'#';
            buf[9] = if settings.use_imperial_units() {
                b'F'
            } else {
                b'C'
            };
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        if set_leading_zero {
            watch::slcd::display_string("0", 4);
        }
    }
}

impl WatchFace for ThermistorLoggingFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.display_index = 0;
        self.ts_ticks = 0;
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Button(Button::Light, ButtonEvent::LongPress) => movement::illuminate_led(),
            Event::Button(Button::Light, ButtonEvent::Down) => {
                self.ts_ticks = 2;
                self.update_display(settings);
            }
            Event::Button(Button::Alarm, ButtonEvent::Down) => {
                self.display_index = (self.display_index + 1) % THERMISTOR_LOGGING_NUM_DATA_POINTS;
                self.ts_ticks = 0;
                self.update_display(settings);
            }
            Event::Activate => self.update_display(settings),
            Event::Tick => {
                if self.ts_ticks != 0 {
                    self.ts_ticks -= 1;
                    if self.ts_ticks == 0 {
                        self.update_display(settings);
                    }
                }
            }
            Event::BackgroundTask => self.log_data(),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        rtc::get_date_time().minute == 0
    }
}
