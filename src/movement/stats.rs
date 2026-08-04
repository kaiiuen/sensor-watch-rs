//! Statistics tracking.
//!
//! Tracks usage and health counters: button presses per button, buzzer rings,
//! brownouts, errors, warnings, and resets. Counters are stored in the RTC
//! backup registers so they survive resets (fixed, no growth).

use crate::watch::deepsleep;

/// Backup register indices for the stats counters.
const REG_BTN_LIGHT: u8 = 4;
const REG_BTN_MODE: u8 = 5;
const REG_BTN_ALARM: u8 = 6;
const REG_BUZZER: u8 = 7;
// (registers 0-3 are used for settings/location/birthdate/reserved)

/// A set of statistics counters.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub btn_light: u32,
    pub btn_mode: u32,
    pub btn_alarm: u32,
    pub buzzer_rings: u32,
    pub brownouts: u32,
    pub errors: u32,
    pub warnings: u32,
    pub resets: u32,
}

/// Reads the current statistics from the backup registers.
pub fn read() -> Stats {
    Stats {
        btn_light: deepsleep::get_backup_data(REG_BTN_LIGHT),
        btn_mode: deepsleep::get_backup_data(REG_BTN_MODE),
        btn_alarm: deepsleep::get_backup_data(REG_BTN_ALARM),
        buzzer_rings: deepsleep::get_backup_data(REG_BUZZER),
        brownouts: 0,
        errors: 0,
        warnings: 0,
        resets: 0,
    }
}

/// Increments the light button press counter.
pub fn press_light() {
    let v = deepsleep::get_backup_data(REG_BTN_LIGHT).wrapping_add(1);
    deepsleep::store_backup_data(v, REG_BTN_LIGHT);
}

/// Increments the mode button press counter.
pub fn press_mode() {
    let v = deepsleep::get_backup_data(REG_BTN_MODE).wrapping_add(1);
    deepsleep::store_backup_data(v, REG_BTN_MODE);
}

/// Increments the alarm button press counter.
pub fn press_alarm() {
    let v = deepsleep::get_backup_data(REG_BTN_ALARM).wrapping_add(1);
    deepsleep::store_backup_data(v, REG_BTN_ALARM);
}

/// Increments the buzzer ring counter.
pub fn buzzer_ring() {
    let v = deepsleep::get_backup_data(REG_BUZZER).wrapping_add(1);
    deepsleep::store_backup_data(v, REG_BUZZER);
}
