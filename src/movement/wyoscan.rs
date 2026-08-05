//! Wyoscan watch face.
//!
//! Port of the C `wyoscan_face.c`. Slowly renders the time left-to-right,
//! scanning across the LCD segments like the Wyoscan watch. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Event, Settings, WatchFace};
use crate::watch::rtc;
use crate::watch::slcd;

const MAX_ILLUMINATED_SEGMENTS: usize = 32;

/// The segment animation map for each digit.
const SEGMENT_MAP: [&str; 10] = [
    "AXFBDEXC", // 0
    "BXXXCXXX", // 1
    "ABGEXXXD", // 2
    "ABGXXXCD", // 3
    "FXGBXXXC", // 4
    "AXFXGXCD", // 5
    "AXFEDCXG", // 6
    "AXXBXXCX", // 7
    "AFGCDEXB", // 8
    "AFGBXXCD", // 9
];

/// The pixel mapping for each digit position and segment (A-F).
const CLOCK_MAPPING: [[(u8, u8); 7]; 6] = [
    [
        (1, 18),
        (2, 19),
        (0, 19),
        (1, 18),
        (0, 18),
        (2, 18),
        (1, 19),
    ],
    [
        (2, 20),
        (2, 21),
        (1, 21),
        (0, 21),
        (0, 20),
        (1, 17),
        (1, 20),
    ],
    [
        (0, 22),
        (2, 23),
        (0, 23),
        (0, 22),
        (1, 22),
        (2, 22),
        (1, 23),
    ],
    [(2, 1), (2, 10), (0, 1), (0, 0), (1, 0), (2, 0), (1, 1)],
    [(2, 2), (2, 3), (0, 4), (0, 3), (0, 2), (1, 2), (1, 3)],
    [(2, 4), (2, 5), (1, 6), (0, 6), (0, 5), (1, 4), (1, 5)],
];

/// The wyoscan face state.
pub struct WyoscanFace {
    animate: bool,
    animation: u8,
    start: usize,
    end: usize,
    total_frames: u8,
    time_digits: [u8; 6],
    illuminated_segments: [(u8, u8); MAX_ILLUMINATED_SEGMENTS],
    colon: bool,
}

impl WyoscanFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        WyoscanFace {
            animate: false,
            animation: 0,
            start: 0,
            end: 0,
            total_frames: 64,
            time_digits: [0; 6],
            illuminated_segments: [(0, 0); MAX_ILLUMINATED_SEGMENTS],
            colon: false,
        }
    }

    pub fn new() -> Self {
        WyoscanFace::new_static()
    }
}

impl WatchFace for WyoscanFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.total_frames = 64;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => {}
            Event::Tick => {
                if !self.animate {
                    let date_time = rtc::get_date_time();
                    self.start = 0;
                    self.end = 0;
                    self.animation = 0;
                    self.animate = true;
                    self.time_digits[0] = date_time.hour / 10;
                    self.time_digits[1] = date_time.hour % 10;
                    self.time_digits[2] = date_time.minute / 10;
                    self.time_digits[3] = date_time.minute % 10;
                    self.time_digits[4] = date_time.second / 10;
                    self.time_digits[5] = date_time.second % 10;
                }
                if self.animate {
                    if (self.end + 1) % MAX_ILLUMINATED_SEGMENTS == self.start {
                        let (x, y) = self.illuminated_segments[self.start];
                        if x != 99 && y != 99 {
                            slcd::clear_pixel(x, y);
                        }
                        self.start = (self.start + 1) % MAX_ILLUMINATED_SEGMENTS;
                    }
                    if self.animation < self.total_frames - MAX_ILLUMINATED_SEGMENTS as u8 {
                        if self.animation % 32 == 0 {
                            if self.colon {
                                slcd::set_colon();
                            } else {
                                slcd::clear_colon();
                            }
                            self.colon = !self.colon;
                        }
                        let position = (self.animation / 8) % 6;
                        let segments = SEGMENT_MAP[self.time_digits[position as usize] as usize];
                        let segment = self.animation % segments.len() as u8;
                        let seg = segments.as_bytes()[segment as usize];
                        if seg == b'X' {
                            self.illuminated_segments[self.end] = (99, 99);
                            self.end = (self.end + 1) % MAX_ILLUMINATED_SEGMENTS;
                            self.animation += 1;
                            return;
                        }
                        let (x, y) = CLOCK_MAPPING[position as usize][(seg - b'A') as usize];
                        slcd::set_pixel(x, y);
                        self.illuminated_segments[self.end] = (x, y);
                        self.end = (self.end + 1) % MAX_ILLUMINATED_SEGMENTS;
                    } else if self.animation >= self.total_frames - MAX_ILLUMINATED_SEGMENTS as u8
                        && self.animation < self.total_frames
                    {
                        self.end = (self.end + 1) % MAX_ILLUMINATED_SEGMENTS;
                    } else {
                        self.animate = false;
                    }
                    self.animation += 1;
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
