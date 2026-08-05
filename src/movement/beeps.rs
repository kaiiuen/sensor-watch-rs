//! Beeps watch face.
//!
//! Port of the C `beeps_face.c`. Cycles through and plays each buzzer note. It
//! is a pure state machine: it reacts to a single event and returns; it never
//! keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;

const FREQUENCIES: [&str; 87] = [
    "   5500", "   5827", "   6174", "   6541", "   6930", "   7342", "   7778", "   8241",
    "   8731", "   9250", "   9800", "  10383", "  11000", "  11654", "  12347", "  13081",
    "  13859", "  14683", "  15556", "  16481", "  17461", "  18500", "  19600", "  20765",
    "  22000", "  23308", "  24694", "  26163", "  27718", "  29366", "  31113", "  32963",
    "  34923", "  36999", "  39200", "  41530", "  44000", "  46616", "  49388", "  52325",
    "  55437", "  58733", "  62225", "  65925", "  69846", "  73999", "  78399", "  83061",
    "  88000", "  93233", "  98777", " 104650", " 110873", " 117466", " 124451", " 131851",
    " 139691", " 147998", " 156798", " 166122", " 176000", " 186466", " 197553", " 209300",
    " 221746", " 234932", " 248902", " 263702", " 279383", " 295996", " 313596", " 332244",
    " 352000", " 372931", " 395107", " 418601", " 443492", " 469863", " 497803", " 527404",
    " 558765", " 591991", " 627193", " 664488", " 704000", " 745862", " 790213",
];

/// The beeps face state.
pub struct BeepsFace {
    frequency: u8,
}

impl BeepsFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        BeepsFace { frequency: 0 }
    }

    pub fn new() -> Self {
        BeepsFace::new_static()
    }

    fn update_lcd(&self) {
        let mut buf = [0u8; 11];
        buf[0] = b'H';
        buf[1] = b'Z';
        buf[2] = b' ';
        let f = FREQUENCIES[self.frequency as usize].as_bytes();
        for (i, &c) in f.iter().take(7).enumerate() {
            buf[3 + i] = c;
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

impl WatchFace for BeepsFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => self.update_lcd(),
            Event::Button(Button::Light, ButtonEvent::Down) => {
                self.frequency = (self.frequency + 1) % 87;
                self.update_lcd();
            }
            Event::Button(Button::Alarm, ButtonEvent::Down) => {
                let note = match self.frequency {
                    0 => Note::A1,
                    1 => Note::A1SharpB1Flat,
                    2 => Note::B1,
                    3 => Note::C2,
                    4 => Note::C2SharpD2Flat,
                    5 => Note::D2,
                    6 => Note::D2SharpE2Flat,
                    7 => Note::E2,
                    8 => Note::F2,
                    9 => Note::F2SharpG2Flat,
                    10 => Note::G2,
                    11 => Note::G2SharpA2Flat,
                    12 => Note::A2,
                    13 => Note::A2SharpB2Flat,
                    14 => Note::B2,
                    15 => Note::C3,
                    16 => Note::C3SharpD3Flat,
                    17 => Note::D3,
                    18 => Note::D3SharpE3Flat,
                    19 => Note::E3,
                    20 => Note::F3,
                    21 => Note::F3SharpG3Flat,
                    22 => Note::G3,
                    23 => Note::G3SharpA3Flat,
                    24 => Note::A3,
                    25 => Note::A3SharpB3Flat,
                    26 => Note::B3,
                    27 => Note::C4,
                    28 => Note::C4SharpD4Flat,
                    29 => Note::D4,
                    30 => Note::D4SharpE4Flat,
                    31 => Note::E4,
                    32 => Note::F4,
                    33 => Note::F4SharpG4Flat,
                    34 => Note::G4,
                    35 => Note::G4SharpA4Flat,
                    36 => Note::A4,
                    37 => Note::A4SharpB4Flat,
                    38 => Note::B4,
                    39 => Note::C5,
                    40 => Note::C5SharpD5Flat,
                    41 => Note::D5,
                    42 => Note::D5SharpE5Flat,
                    43 => Note::E5,
                    44 => Note::F5,
                    45 => Note::F5SharpG5Flat,
                    46 => Note::G5,
                    47 => Note::G5SharpA5Flat,
                    48 => Note::A5,
                    49 => Note::A5SharpB5Flat,
                    50 => Note::B5,
                    51 => Note::C6,
                    52 => Note::C6SharpD6Flat,
                    53 => Note::D6,
                    54 => Note::D6SharpE6Flat,
                    55 => Note::E6,
                    56 => Note::F6,
                    57 => Note::F6SharpG6Flat,
                    58 => Note::G6,
                    59 => Note::G6SharpA6Flat,
                    60 => Note::A6,
                    61 => Note::A6SharpB6Flat,
                    62 => Note::B6,
                    63 => Note::C7,
                    64 => Note::C7SharpD7Flat,
                    65 => Note::D7,
                    66 => Note::D7SharpE7Flat,
                    67 => Note::E7,
                    68 => Note::F7,
                    69 => Note::F7SharpG7Flat,
                    70 => Note::G7,
                    71 => Note::G7SharpA7Flat,
                    72 => Note::A7,
                    73 => Note::A7SharpB7Flat,
                    74 => Note::B7,
                    75 => Note::C8,
                    76 => Note::C8SharpD8Flat,
                    77 => Note::D8,
                    78 => Note::D8SharpE8Flat,
                    79 => Note::E8,
                    80 => Note::F8,
                    81 => Note::F8SharpG8Flat,
                    82 => Note::G8,
                    83 => Note::G8SharpA8Flat,
                    84 => Note::A8,
                    85 => Note::A8SharpB8Flat,
                    _ => Note::B8,
                };
                crate::movement::play_alarm_beeps(1, note);
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
