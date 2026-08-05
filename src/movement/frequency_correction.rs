//! Frequency correction watch face.
//!
//! Port of the C `frequency_correction_face.c`. Adjusts the RTC frequency
//! correction value and selects a periodic event output. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd;

/// The frequency correction face state.
pub struct FrequencyCorrectionFace {
    period_event_output: u8,
}

impl FrequencyCorrectionFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        FrequencyCorrectionFace {
            period_event_output: 0,
        }
    }

    pub fn new() -> Self {
        FrequencyCorrectionFace::new_static()
    }

    fn update_display(&self) {
        let mut buf = [0u8; 11];
        buf[0] = b'F';
        buf[1] = b'C';
        buf[2] = b'0' + self.period_event_output / 10;
        buf[3] = b'0' + self.period_event_output % 10;
        let v = rtc::freqcorr_read();
        write_num(&mut buf, v.unsigned_abs() as u32, 4, 6);
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
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

impl WatchFace for FrequencyCorrectionFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => self.update_display(),
            Event::Tick => {}
            Event::Button(Button::Light, ButtonEvent::Down) => {
                let freqcorr = rtc::freqcorr_read();
                if freqcorr < 127 {
                    rtc::freqcorr_write(freqcorr + 1, 0);
                }
                self.update_display();
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                let freqcorr = rtc::freqcorr_read();
                if freqcorr > -127 {
                    rtc::freqcorr_write(freqcorr - 1, 0);
                }
                self.update_display();
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.period_event_output = (self.period_event_output + 1) % 8;
                self.update_display();
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
