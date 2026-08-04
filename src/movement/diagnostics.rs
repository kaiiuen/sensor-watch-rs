//! Diagnostics watch face.
//!
//! A "task manager + device manager + storage manager + services" all-in-one
//! diagnostics face. It is the last face in the cycle. It presents a
//! categorized menu (CPU, Memory, Storage, Hardware, Software, System) and
//! shows detailed info for the selected category.
//!
//! Navigation (using the standard key bindings):
//! - Bottom-left (Mode): cycle to the next watch face
//! - Top-left (Light): scroll down one row / move the cursor
//! - Bottom-right (Alarm): select / enter a submenu / exit a submenu

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::{self, Indicator};

/// The menu categories.
const MENU_ITEMS: [&str; 6] = ["CPU", "MEM", "STO", "HW", "SW", "SYS"];

/// The diagnostics face state.
pub struct DiagnosticsFace {
    /// The currently selected menu row.
    cursor: u8,
    /// The currently open category (0-5), or 6 for the main menu.
    screen: u8,
}

impl DiagnosticsFace {
    pub const fn new_static() -> Self {
        DiagnosticsFace {
            cursor: 0,
            screen: 6, // start on the main menu
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
        // Show a cursor indicator.
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

    /// Draws the current screen.
    fn draw(&self) {
        match self.screen {
            0 => self.draw_cpu(),
            1 => self.draw_memory(),
            2 => self.draw_storage(),
            3 => self.draw_hardware(),
            4 => self.draw_software(),
            5 => self.draw_system(),
            _ => self.draw_menu(),
        }
    }
}

impl WatchFace for DiagnosticsFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        // Show the main menu on entry.
        self.screen = 6;
        self.cursor = 0;
        self.draw();
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            // Top-left (Light) button: scroll down one row / move the cursor.
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.screen == 6 {
                    // On the main menu, move the cursor down.
                    self.cursor = (self.cursor + 1) % MENU_ITEMS.len() as u8;
                } else {
                    // Inside a submenu, scroll through info rows (single page
                    // for now, so this just re-draws).
                }
                self.draw();
            }
            // Bottom-right (Alarm) button: select / enter / exit.
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.screen == 6 {
                    // Enter the selected category.
                    self.screen = self.cursor;
                } else {
                    // Exit back to the main menu.
                    self.screen = 6;
                }
                self.draw();
            }
            Event::Activate => self.draw(),
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
