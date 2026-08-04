//! Diagnostics watch face.
//!
//! A "task manager + device manager + storage manager + services" all-in-one
//! diagnostics face. It is the last face in the cycle. It presents a
//! categorized menu and shows detailed info for the selected category.
//!
//! Navigation (using the standard key bindings):
//! - Bottom-left (Mode): cycle to the next watch face
//! - Top-left (Light): scroll down one row / move the cursor
//! - Bottom-right (Alarm): select / enter a submenu / exit a submenu

use crate::movement;
use crate::movement::board::{Board, BoardConfig};
use crate::movement::stats;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::{self, Indicator};

/// The main menu categories.
const MENU_ITEMS: [&str; 8] = ["CPU", "MEM", "STO", "HW", "SW", "SYS", "SET", "STA"];

/// The diagnostics face state.
pub struct DiagnosticsFace {
    /// The currently selected menu row.
    cursor: u8,
    /// The currently open category (0-7), or 8 for the main menu.
    screen: u8,
    /// The submenu row within the settings/stats pages.
    subrow: u8,
}

impl DiagnosticsFace {
    pub const fn new_static() -> Self {
        DiagnosticsFace {
            cursor: 0,
            screen: 8, // start on the main menu
            subrow: 0,
        }
    }

    /// Draws the main menu (one row at a time, showing the cursor).
    fn draw_menu(&self) {
        let mut buf = [0u8; 11];
        buf[0] = b'D';
        buf[1] = b'I';
        buf[2] = b'A';
        buf[3] = b'G';
        buf[4] = b' ';
        let item = MENU_ITEMS[(self.cursor as usize).min(MENU_ITEMS.len() - 1)];
        let ib = item.as_bytes();
        buf[5] = ib[0];
        buf[6] = ib[1];
        buf[7] = ib[2];
        buf[8] = b'>';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the CPU info page.
    fn draw_cpu(&self) {
        let mut buf = [0u8; 11];
        buf[0] = b'C';
        buf[1] = b'P';
        buf[2] = b'U';
        buf[3] = b' ';
        buf[4] = b' ';
        buf[5] = b'C';
        buf[6] = b'T';
        buf[7] = b'X';
        buf[8] = b'M';
        buf[9] = b'0';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the memory info page.
    fn draw_memory(&self) {
        let mut buf = [0u8; 11];
        buf[0] = b'M';
        buf[1] = b'E';
        buf[2] = b'M';
        buf[3] = b' ';
        buf[4] = b' ';
        buf[5] = b'3';
        buf[6] = b'2';
        buf[7] = b'K';
        buf[8] = b'B';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the storage info page.
    fn draw_storage(&self) {
        let mut buf = [0u8; 11];
        buf[0] = b'S';
        buf[1] = b'T';
        buf[2] = b'O';
        buf[3] = b' ';
        buf[4] = b' ';
        buf[5] = b'8';
        buf[6] = b'K';
        buf[7] = b'B';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the hardware info page.
    fn draw_hardware(&self) {
        let mut buf = [0u8; 11];
        buf[0] = b'H';
        buf[1] = b'W';
        buf[2] = b' ';
        buf[3] = b' ';
        buf[4] = b'S';
        buf[5] = b'A';
        buf[6] = b'M';
        buf[7] = b'L';
        buf[8] = b'2';
        buf[9] = b'2';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the software info page.
    fn draw_software(&self) {
        let mut buf = [0u8; 11];
        buf[0] = b'S';
        buf[1] = b'W';
        buf[2] = b' ';
        buf[3] = b' ';
        buf[4] = b'R';
        buf[5] = b'S';
        buf[6] = b'T';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the system info page.
    fn draw_system(&self) {
        let dt = rtc::get_date_time();
        let mut buf = [0u8; 11];
        buf[0] = b'S';
        buf[1] = b'Y';
        buf[2] = b'S';
        buf[3] = b' ';
        buf[4] = b' ';
        buf[5] = b'0' + dt.hour / 10;
        buf[6] = b'0' + dt.hour % 10;
        buf[7] = b'0' + dt.minute / 10;
        buf[8] = b'0' + dt.minute % 10;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the settings submenu.
    fn draw_settings(&self) {
        let cfg = BoardConfig::read();
        let mut buf = [0u8; 11];
        match self.subrow {
            0 => {
                // LED color / board preset.
                buf[0] = b'L';
                buf[1] = b'E';
                buf[2] = b'D';
                buf[3] = b' ';
                buf[4] = b' ';
                let name = match cfg.board {
                    Board::Green => "GR",
                    Board::Red => "RD",
                    Board::Blue => "BL",
                    Board::Pro => "PR",
                };
                let nb = name.as_bytes();
                buf[5] = nb[0];
                buf[6] = nb[1];
            }
            1 => {
                // Buzzer voltage.
                buf[0] = b'B';
                buf[1] = b'Z';
                buf[2] = b'R';
                buf[3] = b' ';
                buf[4] = b' ';
                buf[5] = b'0' + cfg.buzzer_voltage / 10;
                buf[6] = b'.';
                buf[7] = b'0' + cfg.buzzer_voltage % 10;
                buf[8] = b'V';
            }
            _ => {}
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the stats submenu.
    fn draw_stats(&self) {
        let s = stats::read();
        let mut buf = [0u8; 11];
        match self.subrow {
            0 => {
                buf[0] = b'L';
                buf[1] = b'T';
                buf[2] = b' ';
                buf[3] = b' ';
                write_count(&mut buf, s.btn_light, 4);
            }
            1 => {
                buf[0] = b'M';
                buf[1] = b'D';
                buf[2] = b' ';
                buf[3] = b' ';
                write_count(&mut buf, s.btn_mode, 4);
            }
            2 => {
                buf[0] = b'B';
                buf[1] = b'Z';
                buf[2] = b' ';
                buf[3] = b' ';
                write_count(&mut buf, s.buzzer_rings, 4);
            }
            _ => {}
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the current screen.
    fn draw(&self) {
        match self.screen {
            0 => self.draw_cpu(),
            1 => self.draw_memory(),
            2 => self.draw_storage(),
            3 => self.draw_hardware(),
            4 => self.draw_software(),
            5 => self.draw_system(),
            6 => self.draw_settings(),
            7 => self.draw_stats(),
            _ => self.draw_menu(),
        }
    }
}

/// Writes a count into the buffer at the given offset (right-aligned, 6 digits).
fn write_count(buf: &mut [u8; 11], count: u32, offset: usize) {
    let mut v = count;
    for i in (offset..offset + 6).rev() {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
}

impl WatchFace for DiagnosticsFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        // Show the main menu on entry.
        self.screen = 8;
        self.cursor = 0;
        self.subrow = 0;
        self.draw();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            // Top-left (Light) button: scroll down one row / move the cursor.
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.screen == 8 {
                    // On the main menu, move the cursor down.
                    self.cursor = (self.cursor + 1) % MENU_ITEMS.len() as u8;
                } else if self.screen == 6 || self.screen == 7 {
                    // Inside settings/stats, scroll through submenu rows.
                    self.subrow = (self.subrow + 1) % 3;
                }
                self.draw();
            }
            // Bottom-right (Alarm) button: select / enter / exit / toggle.
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.screen == 8 {
                    // Enter the selected category.
                    self.screen = self.cursor;
                    self.subrow = 0;
                } else if self.screen == 6 {
                    // Settings: toggle the selected setting.
                    let mut cfg = BoardConfig::read();
                    match self.subrow {
                        0 => {
                            // Toggle the board preset.
                            cfg.board = match cfg.board {
                                Board::Green => Board::Red,
                                Board::Red => Board::Blue,
                                Board::Blue => Board::Pro,
                                Board::Pro => Board::Green,
                            };
                            cfg.write();
                        }
                        1 => {
                            // Adjust buzzer voltage up by 0.1V (cycle 0-9V).
                            cfg.buzzer_voltage = (cfg.buzzer_voltage + 1) % 91;
                            cfg.write();
                        }
                        _ => {}
                    }
                } else if self.screen == 7 {
                    // Stats: nothing to toggle, just exit.
                    self.screen = 8;
                } else {
                    // Exit back to the main menu.
                    self.screen = 8;
                }
                self.draw();
            }
            Event::Activate => self.draw(),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
