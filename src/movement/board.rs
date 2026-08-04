//! Board configuration.
//!
//! Stores the board type (green/red/blue/pro) and the buzzer voltage. This is
//! configurable from the diagnostics face so a freshly-flashed watch can be
//! set up without recompiling. Board presets affect LED polarity and buzzer
//! voltage.

use crate::watch::deepsleep;

/// Backup register for the board configuration.
const REG_BOARD: u8 = 7;

/// The board type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Board {
    Green = 0,
    Red = 1,
    Blue = 2,
    Pro = 3,
}

/// The board configuration.
#[derive(Clone, Copy, Debug)]
pub struct BoardConfig {
    pub board: Board,
    pub buzzer_voltage: u8, // in tenths of a volt (0-90 = 0.0V-9.0V)
}

impl BoardConfig {
    /// Reads the board config from the backup register.
    pub fn read() -> Self {
        let reg = deepsleep::get_backup_data(REG_BOARD);
        BoardConfig {
            board: match reg & 0x3 {
                1 => Board::Red,
                2 => Board::Blue,
                3 => Board::Pro,
                _ => Board::Green,
            },
            buzzer_voltage: ((reg >> 8) & 0xFF) as u8,
        }
    }

    /// Writes the board config to the backup register.
    pub fn write(&self) {
        let reg = (self.board as u32 & 0x3) | ((self.buzzer_voltage as u32 & 0xFF) << 8);
        deepsleep::store_backup_data(reg, REG_BOARD);
    }

    /// Returns true if the LED polarity should be inverted (common-anode).
    ///
    /// The Red dev board and Pro use a common-anode LED, so the polarity is
    /// inverted relative to the common-cathode green/blue boards.
    pub fn invert_led_polarity(&self) -> bool {
        matches!(self.board, Board::Red | Board::Pro)
    }
}

/// Applies the board config to the hardware (LED polarity, buzzer voltage).
///
/// Called once at boot after loading the config.
pub fn apply() {
    let cfg = BoardConfig::read();
    crate::watch::led::set_invert_polarity(cfg.invert_led_polarity());
    crate::watch::buzzer::set_voltage(cfg.buzzer_voltage);
}
