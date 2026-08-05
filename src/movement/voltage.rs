//! Voltage watch face.
//!
//! Port of the C `voltage_face.c`. Shows the battery voltage. It is a pure
//! state machine: it reacts to a single event and returns; it never keeps the
//! CPU awake.

use crate::movement;
use crate::movement::types::{Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;

/// The voltage face state.
pub struct VoltageFace;

impl VoltageFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        VoltageFace
    }

    pub fn new() -> Self {
        VoltageFace
    }

    fn update_display(&self) {
        watch::adc::enable_adc();
        let voltage = watch::adc::get_vcc_voltage() as f32 / 1000.0;
        watch::adc::disable_adc();
        let mut buf = [0u8; 11];
        buf[0] = b'B';
        buf[1] = b'A';
        buf[2] = b' ';
        buf[3] = b' ';
        let v = (voltage * 100.0) as u32;
        buf[4] = b'0' + (v / 100) as u8;
        buf[5] = b'.';
        buf[6] = b'0' + ((v / 10) % 10) as u8;
        buf[7] = b'0' + (v % 10) as u8;
        buf[8] = b' ';
        buf[9] = b'V';
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

impl WatchFace for VoltageFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => self.update_display(),
            Event::Tick => {
                let date_time = rtc::get_date_time();
                if date_time.second % 5 == 4 {
                    watch::slcd::set_indicator(Indicator::Signal);
                } else if date_time.second % 5 == 0 {
                    self.update_display();
                    watch::slcd::clear_indicator(Indicator::Signal);
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
