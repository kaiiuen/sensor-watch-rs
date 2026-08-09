//! Statistics tracking.
//!
//! Tracks usage counters: button presses per button, buzzer rings. These are
//! session diagnostics held in RAM. They intentionally do NOT use the RTC
//! backup registers, because the backup registers are a scarce, shared resource
//! (only 8 exist) that must be reserved for data that must survive resets
//! (settings, board config, fault codes, battery type). Button/buzzer counters
//! are low-value and reset on power loss, which is acceptable for diagnostics.

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

/// The in-RAM counters.
static mut COUNTERS: Stats = Stats {
    btn_light: 0,
    btn_mode: 0,
    btn_alarm: 0,
    buzzer_rings: 0,
    brownouts: 0,
    errors: 0,
    warnings: 0,
    resets: 0,
};

/// Reads the current statistics.
pub fn read() -> Stats {
    unsafe { COUNTERS }
}

/// Increments the light button press counter.
pub fn press_light() {
    unsafe { COUNTERS.btn_light = COUNTERS.btn_light.wrapping_add(1) };
}

/// Increments the mode button press counter.
pub fn press_mode() {
    unsafe { COUNTERS.btn_mode = COUNTERS.btn_mode.wrapping_add(1) };
}

/// Increments the alarm button press counter.
pub fn press_alarm() {
    unsafe { COUNTERS.btn_alarm = COUNTERS.btn_alarm.wrapping_add(1) };
}

/// Increments the buzzer ring counter.
pub fn buzzer_ring() {
    unsafe { COUNTERS.buzzer_rings = COUNTERS.buzzer_rings.wrapping_add(1) };
}
