//! Host deepsleep shim: backup-register storage routed through the `Hw` seam.
//!
//! The real `src/watch/deepsleep.rs` manages sleep modes and the RTC BACKUP
//! registers. On host, sleep entry/exit is a no-op and the backup registers are
//! modeled on the installed mock so `save_load`/`solar_time` (which persist a
//! location and load/save state) run deterministically in tests.

use super::seam;

/// Stores data in one of the RTC's backup registers (0-7).
/// Host: forwards to `Hw::store_backup_data`.
pub fn store_backup_data(data: u32, reg: u8) {
    seam::with_current_hw(|hw| hw.store_backup_data(data, reg));
}

/// Gets 32 bits of data from the RTC's BACKUP register (0-7).
/// Host: forwards to `Hw::get_backup_data`.
pub fn get_backup_data(reg: u8) -> u32 {
    seam::with_current_hw(|hw| hw.get_backup_data(reg))
}

/// Host: no-op (the mock does not model device standby).
pub fn enter_standby() {}

/// Host: no-op.
pub fn enter_backup_mode() {}

/// Host: no-op.
pub fn enter_sleep_mode() {}
