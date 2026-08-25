//! Settings persistence.
//!
//! Saves the movement settings to flash so they survive a reset, and loads
//! them back on boot. Uses the wear-leveled storage to avoid wearing out a
//! single flash row.

use crate::movement::types::Settings;
use crate::watch::storage;

/// The offset within the storage row where the settings live.
const SETTINGS_OFFSET: u32 = 0;
const SETTINGS_NAMESPACE: u32 = 0x5357_0001;

/// A magic value written alongside the settings to detect valid stored data.
const SETTINGS_MAGIC: u32 = 0x5357_0001; // "SW" + version

/// Loads the settings from flash.
///
/// Returns the stored settings if valid data is present, or `None` if no
/// valid settings have been saved yet (e.g. first boot).
pub fn load() -> Option<Settings> {
    let mut buf = [0u8; 12];
    let has_uart_field =
        storage::wear_leveled_read_namespaced(SETTINGS_NAMESPACE, SETTINGS_OFFSET, &mut buf);
    if !has_uart_field {
        // Older records contain only the register. Treat them as UART-off,
        // preserving safe migration instead of failing boot settings entirely.
        let mut legacy = [0u8; 8];
        if !storage::wear_leveled_read_namespaced(SETTINGS_NAMESPACE, SETTINGS_OFFSET, &mut legacy)
        {
            return None;
        }
        buf[..8].copy_from_slice(&legacy);
    }
    let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != SETTINGS_MAGIC {
        return None;
    }
    let reg = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
    Some(Settings {
        reg,
        uart_shell_enabled: has_uart_field && buf[8] == 1,
    })
}

/// Saves the settings to flash.
///
/// Writes the magic value and the settings register using wear leveling so
/// repeated saves don't wear out a single row.
pub fn save(settings: &Settings) -> bool {
    let mut buf = [0u8; 12];
    buf[0..4].copy_from_slice(&SETTINGS_MAGIC.to_le_bytes());
    buf[4..8].copy_from_slice(&settings.reg.to_le_bytes());
    buf[8] = settings.uart_shell_enabled as u8;
    storage::wear_leveled_write_namespaced(SETTINGS_NAMESPACE, SETTINGS_OFFSET, &buf)
}
