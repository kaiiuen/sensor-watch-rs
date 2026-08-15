//! Save/load watch face.
//!
//! Port of the C `save_load_face.c`. Saves and loads the watch state (backup
//! registers and time) to/from slots. It is a pure state machine: it reacts to
//! a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::deepsleep;
use crate::watch::rtc;
use crate::watch::slcd;

const SAVE_LOAD_SLOTS: u8 = 3;

/// A saved state.
#[derive(Clone, Copy)]
struct Savefile {
    version: u8,
    b0: u32,
    b1: u32,
    b2: u32,
    b3: u32,
    b4: u32,
    b5: u32,
    b6: u32,
    b7: u32,
    rtc: rtc::DateTime,
}

impl Savefile {
    const fn zero() -> Self {
        Savefile {
            version: 0,
            b0: 0,
            b1: 0,
            b2: 0,
            b3: 0,
            b4: 0,
            b5: 0,
            b6: 0,
            b7: 0,
            rtc: rtc::DateTime {
                second: 0,
                minute: 0,
                hour: 0,
                day: 0,
                month: 0,
                year: 0,
            },
        }
    }
}

/// The save/load face state.
pub struct SaveLoadFace {
    index: u8,
    update_timeout: u8,
    slot: [Savefile; SAVE_LOAD_SLOTS as usize],
}

impl SaveLoadFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        SaveLoadFace {
            index: 0,
            update_timeout: 0,
            slot: [Savefile::zero(); SAVE_LOAD_SLOTS as usize],
        }
    }

    pub fn new() -> Self {
        SaveLoadFace::new_static()
    }

    fn save(&mut self) {
        let savefile = Savefile {
            version: 1,
            b0: deepsleep::get_backup_data(0),
            b1: deepsleep::get_backup_data(1),
            b2: deepsleep::get_backup_data(2),
            b3: deepsleep::get_backup_data(3),
            b4: deepsleep::get_backup_data(4),
            b5: deepsleep::get_backup_data(5),
            b6: deepsleep::get_backup_data(6),
            b7: deepsleep::get_backup_data(7),
            rtc: rtc::get_date_time(),
        };
        self.slot[self.index as usize] = savefile;
    }

    fn load(&mut self, settings: &mut Settings) -> Result<(), ()> {
        let s = self.slot[self.index as usize];
        deepsleep::store_backup_data(s.b0, 0);
        settings.reg = s.b0;
        deepsleep::store_backup_data(s.b1, 1);
        deepsleep::store_backup_data(s.b2, 2);
        deepsleep::store_backup_data(s.b3, 3);
        deepsleep::store_backup_data(s.b4, 4);
        deepsleep::store_backup_data(s.b5, 5);
        deepsleep::store_backup_data(s.b6, 6);
        deepsleep::store_backup_data(s.b7, 7);
        rtc::set_date_time(s.rtc)
    }

    fn update_display(&self) {
        let mut buf = [0u8; 11];
        buf[0] = b'S';
        buf[1] = b'L';
        buf[2] = b' ';
        buf[3] = b'0' + self.index;
        let s = self.slot[self.index as usize];
        if s.version != 0 {
            buf[4] = b'0' + (s.rtc.year + 20) / 10;
            buf[5] = b'0' + (s.rtc.year + 20) % 10;
            buf[6] = b'0' + s.rtc.month / 10;
            buf[7] = b'0' + s.rtc.month % 10;
            buf[8] = b'0' + s.rtc.day / 10;
            buf[9] = b'0' + s.rtc.day % 10;
        } else {
            buf[4] = b'n';
            buf[5] = b'o';
            buf[6] = b' ';
            buf[7] = b'd';
            buf[8] = b'a';
            buf[9] = b't';
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

impl WatchFace for SaveLoadFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.index = 0;
        self.update_timeout = 0;
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => self.update_display(),
            Event::Tick => {
                if self.update_timeout > 0 {
                    self.update_timeout -= 1;
                    if self.update_timeout == 0 {
                        self.update_display();
                    }
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.index = (self.index + 1) % SAVE_LOAD_SLOTS;
                self.update_display();
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.save();
                slcd::display_string("Saved ", 4);
                self.update_timeout = 3;
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.slot[self.index as usize].version != 0 {
                    let message = if self.load(settings).is_ok() {
                        "Loaded"
                    } else {
                        "Error "
                    };
                    slcd::display_string(message, 4);
                    self.update_timeout = 3;
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}

#[cfg(all(test, feature = "hostmock", not(target_arch = "arm")))]
mod tests {
    use super::*;
    use crate::movement::types::WatchFace;
    use crate::watch::seam;
    use sensor_watch_core::mock_hw::MockHw;

    #[test]
    fn malformed_saved_rtc_reports_error_after_restoring_state() {
        let mut mock = MockHw::new();
        let mut settings = Settings::default();
        let mut face = SaveLoadFace::new();
        let registers = [
            0x1234_5678,
            0x0102_0304,
            0x1111_2222,
            0x3333_4444,
            0x5555_6666,
            0x7777_8888,
            0x9999_AAAA,
            0xBBBB_CCCC,
        ];
        face.slot[0] = Savefile {
            version: 1,
            b0: registers[0],
            b1: registers[1],
            b2: registers[2],
            b3: registers[3],
            b4: registers[4],
            b5: registers[5],
            b6: registers[6],
            b7: registers[7],
            rtc: rtc::DateTime {
                second: 0,
                minute: 0,
                hour: 0,
                day: 0,
                month: 0,
                year: 0,
            },
        };

        face.activate(&settings);
        seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
        seam::with_hw(&mut mock, || {
            face.loop_(
                Event::Button(Button::Alarm, ButtonEvent::LongPress),
                &mut settings,
            )
        });

        assert_eq!(mock.text(), "SL 0Error");
        assert_eq!(settings.reg, registers[0]);
        for (reg, expected) in registers.iter().enumerate() {
            assert_eq!(mock.backup(reg as u8), *expected);
        }
    }
}
