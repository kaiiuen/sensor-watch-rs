//! Databank watch face.
//!
//! Port of the C `databank_face.c`. Browsers through stored numeric constants
//! (pi digits, powers of two, etc.) six digits at a time. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::slcd;

/// The stored data pages: (label, digits).
const PI_DATA: [(&str, &str); 6] = [
    (
        "PI",
        "314159265358979323846264338327950288419716939937510582097494459230781640628620899862803482534211706798214808651328230664709384460955058223172535940812848111745028410270193852110555964462294895493038196442",
    ),
    ("S ", "9192631770"),
    ("31", "2147483648"),
    ("32", "4294967296"),
    ("63", "9223372036854775808"),
    ("64", "18446744073709551616"),
];

/// The databank face state.
pub struct DatabankFace {
    current_word: u8,
    databank_page: u8,
}

impl DatabankFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        DatabankFace {
            current_word: 0,
            databank_page: 0,
        }
    }

    pub fn new() -> Self {
        DatabankFace::new_static()
    }

    fn display(&self) {
        let mut buf = [0u8; 11];
        let (label, data) = PI_DATA[self.databank_page as usize];
        let lb = label.as_bytes();
        buf[0] = lb[0];
        buf[1] = lb[1];
        buf[2] = b'0' + self.current_word / 10;
        buf[3] = b'0' + self.current_word % 10;
        let db = data.as_bytes();
        let start = self.current_word as usize * 6;
        for i in 0..6 {
            if start + i < db.len() {
                buf[4 + i] = db[start + i];
            } else {
                buf[4 + i] = b' ';
            }
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn max_words(&self) -> u8 {
        let (_, data) = PI_DATA[self.databank_page as usize];
        (((data.len() - 1) / 6) + 1) as u8
    }
}

impl WatchFace for DatabankFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.current_word = 0;
        self.display();
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        let max_words = self.max_words();
        match event {
            Event::Activate => self.display(),
            Event::Tick => {}
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.current_word = (self.current_word + max_words - 1) % max_words;
                self.display();
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.databank_page =
                    (self.databank_page + PI_DATA.len() as u8 - 1) % PI_DATA.len() as u8;
                self.current_word = 0;
                self.display();
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.databank_page = (self.databank_page + 1) % PI_DATA.len() as u8;
                self.current_word = 0;
                self.display();
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.current_word = (self.current_word + 1) % max_words;
                self.display();
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
