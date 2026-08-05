//! Beats (Swatch Internet Time) watch face.
//!
//! Port of the C `beats_face.c`. Displays the current Swatch Internet Time, or
//! .beat time, using UTC plus one hour (Biel Mean Time). It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Event, Settings, WatchFace};
use crate::watch;

/// The tick frequency (power of two) used to refresh the fractional beat.
const BEAT_REFRESH_FREQUENCY: u8 = 8;

/// Converts a time to centibeats (0-99999).
fn clock2beats(hours: u32, minutes: u32, seconds: u32, subseconds: u32) -> u32 {
    // Total milliseconds since midnight.
    let ms = ((hours * 3600 + minutes * 60 + seconds) * 1000)
        + ((subseconds * 1000) / BEAT_REFRESH_FREQUENCY as u32);
    // 1 beat = 86.4 seconds = 86400 ms, so 1 centibeat = 864 ms.
    let centibeats = ms / 864;
    centibeats % 100000
}

/// The beats face state.
pub struct BeatsFace {
    next_subsecond_update: i8,
    last_centibeat_displayed: u32,
}

impl BeatsFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        BeatsFace {
            next_subsecond_update: 0,
            last_centibeat_displayed: 0,
        }
    }

    pub fn new() -> Self {
        BeatsFace::new_static()
    }
}

impl WatchFace for BeatsFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.next_subsecond_update = 0;
        self.last_centibeat_displayed = 0;
        movement::request_tick_frequency(BEAT_REFRESH_FREQUENCY);
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => {
                let date_time = movement::get_utc_date_time();
                let bmt_hour = (date_time.hour + 1) % 24;
                let centibeats = clock2beats(
                    bmt_hour as u32,
                    date_time.minute as u32,
                    date_time.second as u32,
                    event.subsecond() as u32,
                );
                if centibeats == self.last_centibeat_displayed {
                    // We missed this update; try again next subsecond.
                    self.next_subsecond_update =
                        (event.subsecond() as i8 + 1) % BEAT_REFRESH_FREQUENCY as i8;
                } else {
                    self.next_subsecond_update =
                        (event.subsecond() as i8 + 1 + (BEAT_REFRESH_FREQUENCY as i8 * 2 / 3))
                            % BEAT_REFRESH_FREQUENCY as i8;
                    self.last_centibeat_displayed = centibeats;
                }
                let mut buf = [0u8; 16];
                let mut v = centibeats;
                let mut i = 15;
                loop {
                    buf[i] = b'0' + (v % 10) as u8;
                    v /= 10;
                    if v == 0 {
                        break;
                    }
                    i -= 1;
                }
                watch::slcd::display_string("beat", 0);
                watch::slcd::display_string(core::str::from_utf8(&buf[i..]).unwrap_or(""), 4);
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
