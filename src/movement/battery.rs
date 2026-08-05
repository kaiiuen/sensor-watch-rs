//! Battery configuration and life estimation.
//!
//! Stores the installed battery type so the diagnostics face can estimate the
//! remaining charge and days of life from the measured voltage. The battery type
//! is persisted in a backup register so it survives resets.

use crate::watch::deepsleep;

/// Backup register for the battery type.
const REG_BATTERY: u8 = 3;

/// The 20 mm coin-cell family supported by the Sensor Watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BatteryType {
    Cr2012 = 0,
    Cr2016 = 1,
    Cr2025 = 2,
    Cr2032 = 3,
    Cr2050 = 4,
}

/// The nominal capacity (mAh) of each battery type.
const CAPACITY_MAH: [u16; 5] = [55, 95, 160, 222, 340];

/// The nominal voltage (mV) of each battery type.
const NOMINAL_MV: [u16; 5] = [3000, 3000, 3000, 3000, 3000];

impl BatteryType {
    /// Returns the nominal capacity in mAh.
    pub fn capacity_mah(self) -> u16 {
        CAPACITY_MAH[self as usize]
    }

    /// Returns the nominal voltage in mV.
    pub fn nominal_mv(self) -> u16 {
        NOMINAL_MV[self as usize]
    }

    /// Returns the full battery name.
    pub fn name(self) -> &'static str {
        match self {
            BatteryType::Cr2012 => "CR2012",
            BatteryType::Cr2016 => "CR2016",
            BatteryType::Cr2025 => "CR2025",
            BatteryType::Cr2032 => "CR2032",
            BatteryType::Cr2050 => "CR2050",
        }
    }
}

/// Reads the configured battery type.
pub fn battery_type() -> BatteryType {
    let reg = deepsleep::get_backup_data(REG_BATTERY);
    match reg & 0x7 {
        1 => BatteryType::Cr2016,
        2 => BatteryType::Cr2025,
        3 => BatteryType::Cr2032,
        4 => BatteryType::Cr2050,
        _ => BatteryType::Cr2012,
    }
}

/// Sets the configured battery type.
pub fn set_battery_type(b: BatteryType) {
    deepsleep::store_backup_data(b as u32 & 0x7, REG_BATTERY);
}

/// Estimates the remaining charge percentage from the measured voltage.
///
/// Uses a linear approximation between the nominal voltage (100%) and a
/// low-voltage cutoff (~2.0 V = 0%). This is a rough estimate; the real
/// discharge curve is non-linear.
pub fn charge_percent(voltage_mv: u16) -> u8 {
    let nominal = battery_type().nominal_mv();
    const CUTOFF_MV: u16 = 2000;
    if voltage_mv >= nominal {
        return 100;
    }
    if voltage_mv <= CUTOFF_MV {
        return 0;
    }
    let pct = (voltage_mv - CUTOFF_MV) as u32 * 100 / (nominal - CUTOFF_MV) as u32;
    pct.min(100) as u8
}

/// Estimates the days of life remaining from the measured voltage and the
/// configured battery capacity.
///
/// Uses the average power draw (~10 µA) to convert capacity to days, scaled by
/// the estimated charge percentage.
pub fn days_remaining(voltage_mv: u16) -> u32 {
    let capacity = battery_type().capacity_mah() as u32;
    let pct = charge_percent(voltage_mv) as u32;
    // 10 µA average draw. capacity_mAh * 1000 / 0.010 mA = hours, / 24 = days.
    let full_days = capacity * 1000 / 10 / 24;
    full_days * pct / 100
}
