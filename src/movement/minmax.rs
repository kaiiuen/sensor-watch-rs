//! Min/max temperature watch face.
//!
//! Port of the C `minmax_face.c`. Shows the min or max temperature logged over
//! the day. It is a pure state machine: it reacts to a single event and
//! returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd;

const LOGGING_DATA_POINTS: usize = 24;

/// The min/max face state.
pub struct MinmaxFace {
    show_min: bool,
    have_logged: bool,
    hourly_mins: [f32; LOGGING_DATA_POINTS],
    hourly_maxs: [f32; LOGGING_DATA_POINTS],
}

impl MinmaxFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        MinmaxFace {
            show_min: true,
            have_logged: false,
            hourly_mins: [0.0; LOGGING_DATA_POINTS],
            hourly_maxs: [0.0; LOGGING_DATA_POINTS],
        }
    }

    pub fn new() -> Self {
        MinmaxFace::new_static()
    }

    fn get_displayed_temperature_c(&self) -> f32 {
        let mut min_temp = 1000.0f32;
        let mut max_temp = -1000.0f32;
        for i in 0..LOGGING_DATA_POINTS {
            if self.hourly_maxs[i] > max_temp {
                max_temp = self.hourly_maxs[i];
            }
            if self.hourly_mins[i] < min_temp {
                min_temp = self.hourly_mins[i];
            }
        }
        if self.show_min { min_temp } else { max_temp }
    }

    fn log_data(&mut self) {
        let pos = rtc::get_date_time().hour as usize;
        let temp_c = 25.0f32;
        if !self.have_logged {
            self.have_logged = true;
            for i in 0..LOGGING_DATA_POINTS {
                self.hourly_mins[i] = temp_c;
                self.hourly_maxs[i] = temp_c;
            }
        } else if rtc::get_date_time().minute < 2 {
            self.hourly_mins[pos] = temp_c;
            self.hourly_maxs[pos] = temp_c;
        } else if self.hourly_mins[pos] > temp_c {
            self.hourly_mins[pos] = temp_c;
        } else if self.hourly_maxs[pos] < temp_c {
            self.hourly_maxs[pos] = temp_c;
        }
    }

    fn update_display(&self, temperature_c: f32, in_fahrenheit: bool) {
        let mut buf = [0u8; 11];
        let v = if in_fahrenheit {
            temperature_c * 1.8 + 32.0
        } else {
            temperature_c
        };
        let scaled = libm::roundf(v) as i32;
        buf[0] = b'0' + ((scaled / 100) % 10) as u8;
        buf[1] = b'0' + ((scaled / 10) % 10) as u8;
        buf[2] = b'0' + (scaled % 10) as u8;
        buf[3] = b'#';
        buf[4] = if in_fahrenheit { b'F' } else { b'C' };
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
    }
}

impl WatchFace for MinmaxFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.show_min = true;
        watch::slcd::display_string("MN", 0);
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => {
                let temp_c = self.get_displayed_temperature_c();
                self.update_display(temp_c, settings.use_imperial_units());
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                settings.set_use_imperial_units(!settings.use_imperial_units());
                let temp_c = self.get_displayed_temperature_c();
                self.update_display(temp_c, settings.use_imperial_units());
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.show_min = !self.show_min;
                watch::slcd::display_string(if self.show_min { "MN" } else { "MX" }, 0);
                let temp_c = self.get_displayed_temperature_c();
                self.update_display(temp_c, settings.use_imperial_units());
            }
            Event::BackgroundTask => self.log_data(),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        true
    }
}
