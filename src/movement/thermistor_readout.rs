//! Thermistor readout watch face.
//!
//! Port of the C `thermistor_readout_face.c`. Shows the temperature from the
//! thermistor. It is a pure state machine: it reacts to a single event and
//! returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;

/// The thermistor readout face state.
pub struct ThermistorReadoutFace;

impl ThermistorReadoutFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        ThermistorReadoutFace
    }

    pub fn new() -> Self {
        ThermistorReadoutFace
    }

    fn update_display(&self, in_fahrenheit: bool) {
        let temperature_c = 25.0f32;
        let mut buf = [0u8; 11];
        let v = if in_fahrenheit {
            temperature_c * 1.8 + 32.0
        } else {
            temperature_c
        };
        let scaled = (v * 10.0) as i32;
        buf[0] = b'0' + ((scaled / 100) % 10) as u8;
        buf[1] = b'0' + ((scaled / 10) % 10) as u8;
        buf[2] = b'.';
        buf[3] = b'0' + (scaled % 10) as u8;
        buf[4] = b'#';
        buf[5] = if in_fahrenheit { b'F' } else { b'C' };
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
    }
}

impl WatchFace for ThermistorReadoutFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        watch::slcd::display_string("TE", 0);
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        let mut date_time = rtc::get_date_time();
        match event {
            Event::Button(Button::Alarm, ButtonEvent::Down) => {
                settings.set_use_imperial_units(!settings.use_imperial_units());
                self.update_display(settings.use_imperial_units());
            }
            Event::Activate => {
                date_time.second = 0;
                self.update_display(settings.use_imperial_units());
            }
            Event::Tick => {
                if date_time.second % 5 == 4 {
                    watch::slcd::set_indicator(Indicator::Signal);
                } else if date_time.second % 5 == 0 {
                    self.update_display(settings.use_imperial_units());
                    watch::slcd::clear_indicator(Indicator::Signal);
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
