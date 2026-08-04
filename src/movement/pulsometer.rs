//! Pulsometer watch face.
//!
//! Port of the C `pulsometer_face.c`. Measures heart rate by counting pulses
//! while the alarm button is held. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::slcd::Indicator;

const PULSOMETER_FACE_TITLE: &str = "PL";
const PULSOMETER_FACE_CALIBRATION_DEFAULT: i8 = 30;
const PULSOMETER_FACE_CALIBRATION_INCREMENT: i8 = 10;
const PULSOMETER_FACE_FREQUENCY_FACTOR: u32 = 4;
const PULSOMETER_FACE_FREQUENCY: u32 = 1 << PULSOMETER_FACE_FREQUENCY_FACTOR;

/// The pulsometer face state.
pub struct PulsometerFace {
    measuring: bool,
    pulses: i16,
    ticks: i16,
    calibration: i8,
}

impl PulsometerFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        PulsometerFace {
            measuring: false,
            pulses: 0,
            ticks: 0,
            calibration: PULSOMETER_FACE_CALIBRATION_DEFAULT,
        }
    }

    pub fn new() -> Self {
        PulsometerFace::new_static()
    }

    fn display_title(&self) {
        watch::slcd::display_string(PULSOMETER_FACE_TITLE, 0);
    }

    fn display_calibration(&self) {
        let mut buf = [0u8; 3];
        buf[0] = b'0' + (self.calibration / 10) as u8;
        buf[1] = b'0' + (self.calibration % 10) as u8;
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("  "), 2);
    }

    fn display_measurement(&self) {
        let mut buf = [0u8; 7];
        let v = self.pulses;
        let mut i = 6;
        let mut n = v;
        loop {
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
            if i == 4 || n == 0 {
                break;
            }
            i -= 1;
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("      "), 4);
    }

    fn indicate(&self) {
        if self.measuring {
            watch::slcd::set_indicator(Indicator::Lap);
        } else {
            watch::slcd::clear_indicator(Indicator::Lap);
        }
    }

    fn start_measurement(&mut self) {
        self.measuring = true;
        self.pulses = i16::MAX;
        self.ticks = 0;
        self.indicate();
    }

    fn measure(&mut self) {
        if !self.measuring {
            return;
        }
        self.ticks += 1;
        let ticks_per_minute = 60 << PULSOMETER_FACE_FREQUENCY_FACTOR;
        let pulses_while_button_held = ticks_per_minute as f32 / self.ticks as f32;
        let mut calibrated = pulses_while_button_held * self.calibration as f32;
        calibrated += 0.5;
        self.pulses = calibrated as i16;
        self.display_measurement();
    }

    fn stop_measurement(&mut self) {
        self.measuring = false;
        self.display_measurement();
        self.indicate();
    }

    fn cycle_calibration(&mut self, increment: i8) {
        if self.measuring {
            return;
        }
        if self.calibration <= 0 {
            self.calibration = 1;
        }
        let last = self.calibration;
        self.calibration += increment;
        if self.calibration > 39 {
            self.calibration = if last == 39 { 1 } else { 39 };
        }
        self.display_calibration();
    }
}

impl WatchFace for PulsometerFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.measuring = false;
        self.display_title();
        self.display_calibration();
        self.display_measurement();
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Button(Button::Alarm, ButtonEvent::Down) => self.start_measurement(),
            Event::Button(Button::Alarm, ButtonEvent::Up)
            | Event::Button(Button::Alarm, ButtonEvent::LongUp) => self.stop_measurement(),
            Event::Tick => self.measure(),
            Event::Button(Button::Light, ButtonEvent::Up) => self.cycle_calibration(1),
            Event::Button(Button::Light, ButtonEvent::LongUp) => {
                self.cycle_calibration(PULSOMETER_FACE_CALIBRATION_INCREMENT)
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
