//! Minimal clock watch face.
//!
//! Port of the C `minimal_clock_face.c`. Shows only the time (hour and minute)
//! in a large, uncluttered layout. It is a pure state machine: it renders on
//! wake and returns; it never keeps the CPU awake.

use crate::movement::types::{Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;

/// The state for the minimal clock face.
pub struct MinimalClockFace;

impl MinimalClockFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        MinimalClockFace
    }

    pub fn new() -> Self {
        MinimalClockFace
    }

    fn update_display(&self, settings: &Settings) {
        let mut date_time = rtc::get_date_time();
        let mut buf = [0u8; 11];

        if !settings.clock_mode_24h() {
            date_time.hour %= 12;
            if date_time.hour == 0 {
                date_time.hour = 12;
            }
        }
        buf[0] = b'0' + date_time.hour / 10;
        buf[1] = b'0' + date_time.hour % 10;
        buf[2] = b'0' + date_time.minute / 10;
        buf[3] = b'0' + date_time.minute % 10;

        watch::slcd::display_string(core::str::from_utf8(&buf[..4]).unwrap_or("  "), 4);
    }
}

impl WatchFace for MinimalClockFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        watch::slcd::set_colon();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => self.update_display(settings),
            _ => crate::movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
