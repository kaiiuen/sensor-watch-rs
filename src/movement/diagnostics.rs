//! Diagnostics watch face.
//!
//! A "task manager + device manager + storage manager + services" all-in-one
//! diagnostics face. It is the last face in the cycle. It presents a
//! hierarchical menu tree so the watch can be configured and inspected
//! entirely on-device.
//!
//! Navigation (using the standard key bindings):
//! - Bottom-left (Mode): cycle to the next watch face
//! - Top-left (Light): scroll down one row / move the cursor
//! - Bottom-right (Alarm): select / enter a submenu / exit a submenu
//! - Hold bottom-right (Alarm long-press): fast-repeat an adjustment
//!
//! Screen real estate is used for breadcrumb tracking:
//! - The DAY indicator (2 digits) shows which watch face you are on.
//! - The DATE indicator (2 digits) shows the submenu depth from the main
//!   diagnostics menu (00 = main menu, 01 = category, 02 = submenu).

use crate::movement;
use crate::movement::battery;
use crate::movement::board::{Board, BoardConfig};
use crate::movement::stats;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::rtc;
use crate::watch::slcd;

/// The main menu categories (up to 6 characters each).
const MENU_ITEMS: [&str; 9] = [
    "CPU   ", "MEMORY", "STORAG", "HARDWR", "SOFTWR", "SYSTEM", "SETTNG", "STATS ", "BATTER",
];

/// The diagnostics face state.
pub struct DiagnosticsFace {
    /// The currently selected menu row.
    cursor: u8,
    /// The currently open category (0-7), or 8 for the main menu.
    screen: u8,
    /// The submenu row within the settings/stats pages.
    subrow: u8,
    /// The previous screen, for breadcrumb tracking.
    prev_screen: u8,
    /// The current watch face index (set at setup).
    face_index: u8,
}

impl DiagnosticsFace {
    pub const fn new_static() -> Self {
        DiagnosticsFace {
            cursor: 0,
            screen: 8, // start on the main menu
            subrow: 0,
            prev_screen: 8,
            face_index: 0,
        }
    }

    /// Shows the breadcrumb using the day/date indicators.
    fn show_breadcrumb(&self) {
        // DAY indicator: which watch face we are on.
        let day = self.face_index;
        // DATE indicator: submenu depth (00 = main menu, 01 = category, 02+ = submenu).
        let date = if self.screen == 9 {
            0
        } else if self.screen == 6 || self.screen == 7 || self.screen == 8 {
            2
        } else {
            1
        };

        // Use the day-of-month digits (positions 2-3) for the face index,
        // and the day-of-week digits (positions 0-1) for the depth.
        let mut buf = [0u8; 4];
        buf[0] = b'0' + date / 10;
        buf[1] = b'0' + date % 10;
        buf[2] = b'0' + day / 10;
        buf[3] = b'0' + day % 10;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the main menu (one row at a time, showing the cursor).
    fn draw_menu(&self) {
        let mut buf = [0u8; 11];
        let item = MENU_ITEMS[(self.cursor as usize).min(MENU_ITEMS.len() - 1)];
        let ib = item.as_bytes();
        // Show the category name (up to 6 chars) in the main clock line.
        for (i, &c) in ib.iter().take(6).enumerate() {
            buf[4 + i] = c;
        }
        // Cursor indicator.
        buf[3] = b'>';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the CPU info page.
    fn draw_cpu(&self) {
        let mut buf = [0u8; 11];
        let s = "CPU  ";
        let sb = s.as_bytes();
        for (i, &c) in sb.iter().enumerate() {
            buf[i] = c;
        }
        buf[6] = b'C';
        buf[7] = b'T';
        buf[8] = b'X';
        buf[9] = b'M';
        buf[10] = b'0';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the memory info page.
    fn draw_memory(&self) {
        let mut buf = [0u8; 11];
        let s = "MEMORY";
        let sb = s.as_bytes();
        for (i, &c) in sb.iter().enumerate() {
            buf[i] = c;
        }
        buf[6] = b'3';
        buf[7] = b'2';
        buf[8] = b'K';
        buf[9] = b'B';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the storage info page.
    fn draw_storage(&self) {
        let mut buf = [0u8; 11];
        let s = "STORAG";
        let sb = s.as_bytes();
        for (i, &c) in sb.iter().enumerate() {
            buf[i] = c;
        }
        buf[6] = b'8';
        buf[7] = b'K';
        buf[8] = b'B';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the hardware info page.
    fn draw_hardware(&self) {
        let mut buf = [0u8; 11];
        let s = "HARDWR";
        let sb = s.as_bytes();
        for (i, &c) in sb.iter().enumerate() {
            buf[i] = c;
        }
        buf[6] = b'S';
        buf[7] = b'A';
        buf[8] = b'M';
        buf[9] = b'L';
        buf[10] = b'2';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the software info page.
    fn draw_software(&self) {
        let mut buf = [0u8; 11];
        let s = "SOFTWR";
        let sb = s.as_bytes();
        for (i, &c) in sb.iter().enumerate() {
            buf[i] = c;
        }
        buf[6] = b'R';
        buf[7] = b'S';
        buf[8] = b'T';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the system info page.
    fn draw_system(&self) {
        let dt = rtc::get_date_time();
        let mut buf = [0u8; 11];
        let s = "SYSTEM";
        let sb = s.as_bytes();
        for (i, &c) in sb.iter().enumerate() {
            buf[i] = c;
        }
        buf[6] = b'0' + dt.hour / 10;
        buf[7] = b'0' + dt.hour % 10;
        buf[8] = b'0' + dt.minute / 10;
        buf[9] = b'0' + dt.minute % 10;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the settings submenu.
    fn draw_settings(&self) {
        let cfg = BoardConfig::read();
        let mut buf = [0u8; 11];
        match self.subrow {
            0 => {
                // LED color / board preset.
                let s = "LED   ";
                let sb = s.as_bytes();
                for (i, &c) in sb.iter().enumerate() {
                    buf[i] = c;
                }
                let name = match cfg.board {
                    Board::Green => "GREEN",
                    Board::Red => "RED  ",
                    Board::Blue => "BLUE ",
                    Board::Pro => "PRO  ",
                };
                let nb = name.as_bytes();
                for (i, &c) in nb.iter().take(6).enumerate() {
                    buf[5 + i] = c;
                }
            }
            1 => {
                // Buzzer voltage (0.1V increments).
                let s = "BUZZER";
                let sb = s.as_bytes();
                for (i, &c) in sb.iter().enumerate() {
                    buf[i] = c;
                }
                buf[6] = b'0' + cfg.buzzer_voltage / 10;
                buf[7] = b'.';
                buf[8] = b'0' + cfg.buzzer_voltage % 10;
                buf[9] = b'V';
            }
            2 => {
                // Power off (BACKUP mode).
                let s = "POWER ";
                let sb = s.as_bytes();
                for (i, &c) in sb.iter().enumerate() {
                    buf[i] = c;
                }
                let name = "OFF";
                let nb = name.as_bytes();
                for (i, &c) in nb.iter().enumerate() {
                    buf[6 + i] = c;
                }
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
                let label = "LIGHT ";
                let lb = label.as_bytes();
                for (i, &c) in lb.iter().enumerate() {
                    buf[i] = c;
                }
                write_count(&mut buf, s.btn_light, 6);
            }
            1 => {
                let label = "MODE  ";
                let lb = label.as_bytes();
                for (i, &c) in lb.iter().enumerate() {
                    buf[i] = c;
                }
                write_count(&mut buf, s.btn_mode, 6);
            }
            2 => {
                let label = "BUZZER";
                let lb = label.as_bytes();
                for (i, &c) in lb.iter().enumerate() {
                    buf[i] = c;
                }
                write_count(&mut buf, s.buzzer_rings, 6);
            }
            _ => {}
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    /// Draws the battery submenu.
    fn draw_battery(&self) {
        let mut buf = [0u8; 11];
        match self.subrow {
            0 => {
                // Battery type selection.
                let label = "TYPE  ";
                let lb = label.as_bytes();
                for (i, &c) in lb.iter().enumerate() {
                    buf[i] = c;
                }
                let name = battery::battery_type().name();
                let nb = name.as_bytes();
                for (i, &c) in nb.iter().take(6).enumerate() {
                    buf[5 + i] = c;
                }
            }
            1 => {
                // Charge percentage.
                let label = "CHARGE";
                let lb = label.as_bytes();
                for (i, &c) in lb.iter().enumerate() {
                    buf[i] = c;
                }
                crate::watch::adc::enable_adc();
                let v = crate::watch::adc::get_vcc_voltage();
                crate::watch::adc::disable_adc();
                let pct = battery::charge_percent(v);
                buf[6] = b'0' + pct / 10;
                buf[7] = b'0' + pct % 10;
                buf[8] = b'%';
            }
            2 => {
                // Days remaining.
                let label = "DAYS  ";
                let lb = label.as_bytes();
                for (i, &c) in lb.iter().enumerate() {
                    buf[i] = c;
                }
                crate::watch::adc::enable_adc();
                let v = crate::watch::adc::get_vcc_voltage();
                crate::watch::adc::disable_adc();
                let days = battery::days_remaining(v);
                write_count(&mut buf, days, 6);
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
            8 => self.draw_battery(),
            _ => self.draw_menu(),
        }
        self.show_breadcrumb();
    }

    /// Adjusts the buzzer voltage by the given amount (in 0.1V steps).
    fn adjust_buzzer(&self, delta: i8) {
        let mut cfg = BoardConfig::read();
        let mut v = cfg.buzzer_voltage as i16 + delta as i16;
        if v < 0 {
            v = 90;
        }
        if v > 90 {
            v = 0;
        }
        cfg.buzzer_voltage = v as u8;
        cfg.write();
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
    fn setup(&mut self, _settings: &Settings, watch_face_index: usize) {
        self.face_index = watch_face_index as u8;
    }

    fn activate(&mut self, _settings: &Settings) {
        // Show the main menu on entry.
        self.screen = 9;
        self.cursor = 0;
        self.subrow = 0;
        self.prev_screen = 9;
        self.draw();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            // Top-left (Light) button: scroll down one row / move the cursor.
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.screen == 9 {
                    // On the main menu, move the cursor down.
                    self.cursor = (self.cursor + 1) % MENU_ITEMS.len() as u8;
                } else if self.screen == 6 || self.screen == 7 || self.screen == 8 {
                    // Inside settings/stats/battery, scroll through submenu rows.
                    self.subrow = (self.subrow + 1) % 3;
                }
                self.draw();
            }
            // Bottom-right (Alarm) button: select / enter / exit / toggle.
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.screen == 9 {
                    // Enter the selected category.
                    self.prev_screen = self.screen;
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
                            self.adjust_buzzer(1);
                        }
                        2 => {
                            // Power off: save settings, then enter BACKUP mode.
                            crate::movement::save_settings();
                            crate::watch::deepsleep::enter_backup_mode();
                        }
                        _ => {}
                    }
                } else if self.screen == 8 {
                    // Battery: cycle the battery type.
                    if self.subrow == 0 {
                        let next = match battery::battery_type() {
                            battery::BatteryType::Cr2012 => battery::BatteryType::Cr2016,
                            battery::BatteryType::Cr2016 => battery::BatteryType::Cr2025,
                            battery::BatteryType::Cr2025 => battery::BatteryType::Cr2032,
                            battery::BatteryType::Cr2032 => battery::BatteryType::Cr2050,
                            battery::BatteryType::Cr2050 => battery::BatteryType::Cr2012,
                        };
                        battery::set_battery_type(next);
                    } else {
                        // Charge/days are read-only; exit.
                        self.screen = self.prev_screen;
                    }
                } else if self.screen == 7 {
                    // Stats: nothing to toggle, just exit.
                    self.screen = self.prev_screen;
                } else {
                    // Exit back to the main menu.
                    self.screen = 9;
                }
                self.draw();
            }
            // Hold bottom-right: fast-repeat an adjustment.
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.screen == 6 && self.subrow == 1 {
                    self.adjust_buzzer(1);
                    self.draw();
                }
            }
            Event::Activate => self.draw(),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
