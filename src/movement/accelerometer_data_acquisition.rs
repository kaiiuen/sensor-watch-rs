//! Accelerometer data acquisition watch face.
//!
//! Port of the C `accelerometer_data_acquisition_face.c`. Records accelerometer
//! data to flash (requires the optional accelerometer and SPI flash). It is a
//! pure state machine: it reacts to a single event and returns; it never keeps
//! the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd::Indicator;

const SECONDS_TO_RECORD: u8 = 15;
const STORAGE_PAGE_SIZE: i16 = 64;
const SAMPLE_SIZE: i16 = 6;
const MODE_IDLE: u8 = 0;
const MODE_COUNTDOWN: u8 = 1;
const MODE_SENSING: u8 = 2;
const MODE_SETTINGS: u8 = 3;
const SETTINGS_PAGE_SOUND: u8 = 0;
const SETTINGS_PAGE_DELAY: u8 = 1;
const SETTINGS_PAGE_REPEAT: u8 = 2;

const ACTIVITY_TYPES: [&str; 15] = [
    "TE", "ID", "OF", "SL", "WH", "WA", "WB", "JO", "RU", "BI", "HI", "EL", "SU", "SD", "WL",
];

/// The accelerometer data acquisition face state.
pub struct AccelerometerDataAcquisitionFace {
    mode: u8,
    activity_type_index: u8,
    countdown_length: u8,
    countdown_ticks: u8,
    reading_ticks: u8,
    repeat_interval: u32,
    repeat_ticks: u32,
    settings_page: u8,
    beep_with_countdown: bool,
    next_available_page: i16,
}

impl AccelerometerDataAcquisitionFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        AccelerometerDataAcquisitionFace {
            mode: MODE_IDLE,
            activity_type_index: 0,
            countdown_length: 3,
            countdown_ticks: 0,
            reading_ticks: 0,
            repeat_interval: 0,
            repeat_ticks: 0,
            settings_page: 0,
            beep_with_countdown: true,
            // The pointer is the start of the next 6-byte sample. Begin at
            // the last sample-aligned position that fits in the 8 KiB area.
            next_available_page: 8192 - SAMPLE_SIZE,
        }
    }

    pub fn new() -> Self {
        AccelerometerDataAcquisitionFace::new_static()
    }

    fn update(&self) {
        let mut buf = [0u8; 11];
        let ticks = match self.mode {
            MODE_IDLE => self.countdown_length,
            MODE_COUNTDOWN => self.countdown_ticks,
            MODE_SENSING => self.reading_ticks,
            _ => 0,
        };
        let at = ACTIVITY_TYPES[self.activity_type_index as usize].as_bytes();
        buf[0] = at[0];
        buf[1] = at[1];
        buf[2] = b'0' + ticks / 10;
        buf[3] = b'0' + ticks % 10;
        buf[4] = b'r';
        buf[5] = b'e';
        buf[6..10].copy_from_slice(&storage_display_suffix(self.next_available_page));
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        watch::slcd::set_colon();
        if self.next_available_page > 8110 {
            watch::slcd::display_string("<1", 6);
        }
        if self.beep_with_countdown {
            watch::slcd::set_indicator(Indicator::Bell);
        } else {
            watch::slcd::clear_indicator(Indicator::Bell);
        }
        if self.reading_ticks != 0 {
            watch::slcd::set_indicator(Indicator::Signal);
        } else {
            watch::slcd::clear_indicator(Indicator::Signal);
        }
        if self.repeat_ticks != 0 {
            watch::slcd::set_indicator(Indicator::Lap);
        } else {
            watch::slcd::clear_indicator(Indicator::Lap);
        }
    }

    fn update_settings(&self) {
        watch::slcd::clear_colon();
        if self.beep_with_countdown {
            watch::slcd::set_indicator(Indicator::Bell);
        } else {
            watch::slcd::clear_indicator(Indicator::Bell);
        }
        match self.settings_page {
            SETTINGS_PAGE_SOUND => {
                let mut buf = [0u8; 11];
                buf[0] = b'S';
                buf[1] = b'O';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b'B';
                buf[5] = b'e';
                buf[6] = b'e';
                buf[7] = b'p';
                buf[8] = b' ';
                buf[9] = if self.beep_with_countdown { b'Y' } else { b'N' };
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            SETTINGS_PAGE_DELAY => {
                let mut buf = [0u8; 11];
                buf[0] = b'D';
                buf[1] = b'L';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b'0' + self.countdown_length / 10;
                buf[5] = b'0' + self.countdown_length % 10;
                buf[6] = b' ';
                buf[7] = b'S';
                buf[8] = b'e';
                buf[9] = b'C';
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
            }
            _ => {
                if self.repeat_interval == 0 {
                    watch::slcd::display_string("rE  none  ", 0);
                } else {
                    let mut buf = [0u8; 11];
                    buf[0] = b'r';
                    buf[1] = b'E';
                    buf[2] = b' ';
                    buf[3] = b' ';
                    let m = self.repeat_interval / 60;
                    buf[4] = b'0' + (m / 10) as u8;
                    buf[5] = b'0' + (m % 10) as u8;
                    buf[6] = b'n';
                    buf[7] = b'&';
                    buf[8] = b'i';
                    buf[9] = b'n';
                    watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
                }
            }
        }
    }

    fn advance_current_setting(&mut self) {
        match self.settings_page {
            SETTINGS_PAGE_SOUND => self.beep_with_countdown = !self.beep_with_countdown,
            SETTINGS_PAGE_DELAY => {
                self.countdown_length = match self.countdown_length {
                    1 => 3,
                    3 => 10,
                    10 => 30,
                    _ => 1,
                };
            }
            _ => {
                self.repeat_interval = match self.repeat_interval {
                    0 => 60,
                    60 => 600,
                    600 => 1800,
                    1800 => 3600,
                    _ => 0,
                };
            }
        }
    }

    fn start_reading(&mut self) {
        self.reading_ticks = SECONDS_TO_RECORD + 1;
        if self.beep_with_countdown {
            crate::movement::play_alarm_beeps(1, Note::C6);
        }
    }

    fn continue_reading(&mut self) {
        if self.reading_ticks > 0 {
            self.reading_ticks -= 1;
            // Read a raw accelerometer sample and store it (if a sensor is
            // present and there is flash room).
            if crate::movement::accelerometer_begin() {
                let r = crate::watch::lis2dw::get_raw_reading();
                if let Some(next_page) = normalize_storage_pointer(self.next_available_page) {
                    self.next_available_page = next_page;
                    let mut buf = [0u8; 6];
                    buf[0..2].copy_from_slice(&r.x.to_le_bytes());
                    buf[2..4].copy_from_slice(&r.y.to_le_bytes());
                    buf[4..6].copy_from_slice(&r.z.to_le_bytes());
                    let row = (self.next_available_page / 256) as u32;
                    let off = (self.next_available_page % 256) as u32;
                    if crate::watch::storage::write(row, off, &buf) {
                        self.next_available_page -= SAMPLE_SIZE;
                    }
                }
            }
            if self.reading_ticks == 0 {
                self.mode = MODE_IDLE;
                crate::movement::play_alarm_beeps(1, Note::C4);
                crate::movement::play_alarm_beeps(1, Note::C4);
                self.repeat_ticks = self.repeat_interval;
            }
        }
    }
}

/// Render the storage counter without allowing a full-storage sentinel to enter
/// the digit arithmetic. The suffix occupies the two record digits and `#o`.
fn storage_display_suffix(pointer: i16) -> [u8; 4] {
    if pointer < 0 {
        return *b" FUL";
    }

    let record_count = (8192i32 - pointer as i32) / 82;
    let record_count = record_count.clamp(0, 99) as u8;
    [
        b'0' + record_count / 10,
        b'0' + record_count % 10,
        b'#',
        b'o',
    ]
}

/// Move a sample pointer back to the last offset where a 6-byte write fits in
/// the current 64-byte NVM page. The storage driver deliberately rejects page
/// crossing writes, so leaving the pointer at (for example) offset 62 would
/// otherwise make recording retry the same failed write forever.
fn normalize_storage_pointer(pointer: i16) -> Option<i16> {
    if pointer < 0 {
        return None;
    }
    let offset = pointer % STORAGE_PAGE_SIZE;
    Some(pointer - (offset + SAMPLE_SIZE - STORAGE_PAGE_SIZE).max(0))
}

#[cfg(test)]
mod tests {
    use super::{normalize_storage_pointer, storage_display_suffix};

    #[test]
    fn displays_the_final_valid_storage_slot() {
        assert_eq!(storage_display_suffix(0), *b"99#o");
    }

    #[test]
    fn displays_full_storage_without_digit_arithmetic() {
        assert_eq!(storage_display_suffix(-6), *b" FUL");
    }

    #[test]
    fn fresh_storage_starts_at_a_writable_sample_offset() {
        let face = super::AccelerometerDataAcquisitionFace::new_static();
        assert_eq!(face.next_available_page, 8186);
        assert_eq!(
            normalize_storage_pointer(face.next_available_page),
            Some(8186)
        );
    }

    #[test]
    fn normalizes_sample_pointer_at_page_boundary() {
        assert_eq!(normalize_storage_pointer(64), Some(64));
        assert_eq!(normalize_storage_pointer(58), Some(58));
        assert_eq!(normalize_storage_pointer(62), Some(58));
        assert_eq!(normalize_storage_pointer(60), Some(58));
        assert_eq!(normalize_storage_pointer(-4), None);
    }
}

impl WatchFace for AccelerometerDataAcquisitionFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => match self.mode {
                MODE_IDLE => {
                    self.update();
                    if self.repeat_ticks > 0 {
                        self.repeat_ticks -= 1;
                        if self.repeat_ticks == 0 {
                            self.countdown_ticks = self.countdown_length;
                            self.mode = MODE_COUNTDOWN;
                        }
                    }
                }
                MODE_COUNTDOWN => {
                    if self.next_available_page < 0 {
                        self.countdown_ticks = 0;
                        self.repeat_ticks = 0;
                        self.mode = MODE_IDLE;
                    }
                    if self.countdown_ticks > 0 {
                        self.countdown_ticks -= 1;
                        if self.countdown_ticks == 0 {
                            self.mode = MODE_SENSING;
                            self.start_reading();
                        } else if self.countdown_ticks < 3 && self.beep_with_countdown {
                            crate::movement::play_alarm_beeps(1, Note::C5);
                        }
                    }
                    self.update();
                }
                MODE_SENSING => {
                    if self.reading_ticks > 0 {
                        self.continue_reading();
                    }
                    self.update();
                }
                MODE_SETTINGS => self.update_settings(),
                _ => {}
            },
            Event::Button(Button::Light, ButtonEvent::Up) => match self.mode {
                MODE_IDLE => {
                    self.activity_type_index =
                        (self.activity_type_index + 1) % ACTIVITY_TYPES.len() as u8;
                    self.update();
                }
                MODE_SETTINGS => {
                    self.settings_page += 1;
                    if self.settings_page > SETTINGS_PAGE_REPEAT {
                        self.settings_page = 0;
                        self.mode = MODE_IDLE;
                        self.update();
                    } else {
                        self.update_settings();
                    }
                }
                _ => {}
            },
            Event::Button(Button::Light, ButtonEvent::LongPress) => movement::illuminate_led(),
            Event::Button(Button::Alarm, ButtonEvent::Up) => match self.mode {
                MODE_IDLE => {
                    self.countdown_ticks = self.countdown_length;
                    self.mode = MODE_COUNTDOWN;
                    self.update();
                }
                MODE_COUNTDOWN => {
                    self.countdown_ticks = 0;
                    self.mode = MODE_IDLE;
                    self.update();
                }
                MODE_SETTINGS => {
                    self.advance_current_setting();
                    self.update_settings();
                }
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.mode == MODE_IDLE {
                    self.repeat_ticks = 0;
                    self.mode = MODE_SETTINGS;
                    self.update_settings();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.mode = MODE_IDLE;
        self.settings_page = 0;
        self.countdown_ticks = 0;
        self.repeat_ticks = 0;
        self.reading_ticks = 0;
    }
}
