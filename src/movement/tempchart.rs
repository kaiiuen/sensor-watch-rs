//! Temperature chart watch face.
//!
//! Port of the C `tempchart_face.c`. Records temperature samples into a
//! histogram and shows the total count. It is a pure state machine: it reacts
//! to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd;

/// The temperature chart face state.
pub struct TempchartFace {
    stat: [u8; 24 * 70],
    num_div: u16,
}

impl TempchartFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        TempchartFace {
            stat: [0; 24 * 70],
            num_div: 0,
        }
    }

    pub fn new() -> Self {
        TempchartFace::new_static()
    }

    fn display(&self) {
        let mut sum = 0u32;
        for &v in self.stat.iter() {
            sum += v as u32;
        }
        let mut buf = [0u8; 11];
        buf[0] = b'T';
        buf[1] = b'S';
        buf[2] = b'0' + ((self.num_div / 10) % 10) as u8;
        buf[3] = b'0' + (self.num_div % 10) as u8;
        write_num(&mut buf, sum, 4, 6);
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

impl WatchFace for TempchartFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => self.display(),
            Event::Tick => {}
            Event::BackgroundTask => {
                // Sample temperature (approximate from RTC day).
                let date_time = rtc::get_date_time();
                let temp = (date_time.day % 70) as i32;
                if temp < 0 || temp >= 70 {
                    return;
                }
                let idx = date_time.hour as usize + temp as usize * 24;
                if self.stat[idx] == 255 {
                    self.num_div += 1;
                    for v in self.stat.iter_mut() {
                        *v = (*v + 1) >> 1;
                    }
                }
                self.stat[idx] += 1;
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        let date_time = rtc::get_date_time();
        date_time.minute % 5 == 0
    }
}
