//! SOS / Morse code watch face.
//!
//! Transmits preprogrammed codes (SOS, etc.) via the buzzer using Morse code.
//! The user selects between preprogrammed codes with the Light button and
//! transmits the selected code with the Alarm button. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;

/// The preprogrammed codes. Each is a Morse sequence of dots ('.') and dashes
/// ('-'), with spaces between letters and '/' between words.
const CODES: [&str; 5] = [
    "SOS",       // ... --- ...
    "MAYDAY",    // -- .- -.-- -.. .- -.--
    "HELP",      // .... . .-.. .--.
    "SEND HELP", // ... . -. -.. / .... . .-.. .--.
    "OK",        // --- -.-
];

/// The Morse code table (A-Z, 0-9).
const MORSE: [&str; 36] = [
    ".-", "-...", "-.-.", "-..", ".", "..-.", "--.", "....", "..", ".---", // A-J
    "-.-", ".-..", "--", "-.", "---", ".--.", "--.-", ".-.", "...", "-", // K-T
    "..-", "...-", ".--", "-..-", "-.--", "--..", // U-Z
    "-----", ".----", "..---", "...--", "....-", ".....", "-....", "--...", "---..",
    "----.", // 0-9
];

/// The SOS face state.
pub struct SosFace {
    /// The currently selected code index.
    code_index: u8,
    /// Whether a transmission is in progress.
    transmitting: bool,
}

impl SosFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        SosFace {
            code_index: 0,
            transmitting: false,
        }
    }

    pub fn new() -> Self {
        SosFace::new_static()
    }

    /// Renders the current code name on the display.
    fn update_lcd(&self) {
        let mut buf = [0u8; 11];
        let name = CODES[self.code_index as usize].as_bytes();
        for (i, &c) in name.iter().take(10).enumerate() {
            buf[i] = c;
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Returns the Morse sequence for a single character, or None if unknown.
    fn morse_for(c: u8) -> Option<&'static str> {
        match c {
            b'A'..=b'Z' => Some(MORSE[(c - b'A') as usize]),
            b'0'..=b'9' => Some(MORSE[(c - b'0' + 26) as usize]),
            _ => None,
        }
    }

    /// Transmits the selected code as Morse via the buzzer.
    ///
    /// Each dot is one unit, each dash three units, with a one-unit gap between
    /// elements, a three-unit gap between letters, and a seven-unit gap between
    /// words. The buzzer is driven directly (blocking) so the transmission is
    /// exact; the CPU is awake only for the duration of the transmission.
    fn transmit(&self) {
        let code = CODES[self.code_index as usize];
        for &c in code.as_bytes() {
            if c == b' ' || c == b'/' {
                // Word gap: 7 units (minus the 3 already counted).
                delay_units(4);
                continue;
            }
            if let Some(seq) = Self::morse_for(c) {
                for (i, &m) in seq.as_bytes().iter().enumerate() {
                    if i > 0 {
                        // Element gap: 1 unit.
                        buzzer_off();
                        delay_units(1);
                    }
                    buzzer_on();
                    let unit = if m == b'.' { 1 } else { 3 };
                    delay_units(unit);
                }
                // Letter gap: 3 units (minus the 1 already counted).
                buzzer_off();
                delay_units(2);
            }
        }
        buzzer_off();
    }
}

/// Turns the buzzer on at a fixed tone.
fn buzzer_on() {
    watch::buzzer::set_buzzer_period(watch::buzzer::NOTE_PERIODS[Note::C6 as usize] as u32);
    watch::buzzer::set_buzzer_on();
}

/// Turns the buzzer off.
fn buzzer_off() {
    watch::buzzer::set_buzzer_off();
}

/// A crude blocking delay in Morse units (~60 ms each).
fn delay_units(units: u16) {
    for _ in 0..units {
        for _ in 0..60_000 {
            cortex_m::asm::nop();
        }
    }
}

impl WatchFace for SosFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.transmitting = false;
        self.update_lcd();
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => self.update_lcd(),
            Event::Button(Button::Light, ButtonEvent::Up) => {
                // Cycle through the codes.
                self.code_index = (self.code_index + 1) % CODES.len() as u8;
                self.update_lcd();
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                // Transmit the selected code.
                self.transmitting = true;
                self.transmit();
                self.transmitting = false;
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.transmitting = false;
        buzzer_off();
    }
}
