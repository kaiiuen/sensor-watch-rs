//! LIS2DW accelerometer driver.
//!
//! Port of the C `lis2dw.c` from Second Movement. Talks to the LIS2DW12
//! accelerometer on the 9-pin connector over I2C. Provides configuration of
//! data rate, mode, range, tap detection, and wake-on-motion, plus raw reading
//! and FIFO access.

use crate::watch::i2c;

/// The I2C address of the LIS2DW (SA0 high).
const ADDRESS: i16 = 0x19;

// Registers.
const REG_OUT_TEMP_L: u8 = 0x0D;
const REG_OUT_TEMP_H: u8 = 0x0E;
const REG_WHO_AM_I: u8 = 0x0F;
const REG_CTRL1: u8 = 0x20;
const REG_CTRL2: u8 = 0x21;
const REG_CTRL3: u8 = 0x22;
const REG_CTRL4_INT1: u8 = 0x23;
const REG_CTRL5_INT2: u8 = 0x24;
const REG_CTRL6: u8 = 0x25;
const REG_STATUS: u8 = 0x27;
const REG_OUT_X_L: u8 = 0x28;
const REG_FIFO_CTRL: u8 = 0x2E;
const REG_FIFO_SAMPLE: u8 = 0x2F;
const REG_TAP_THS_X: u8 = 0x30;
const REG_TAP_THS_Z: u8 = 0x32;
const REG_INT1_DUR: u8 = 0x33;
const REG_WAKE_UP_THS: u8 = 0x34;
const REG_WAKE_UP_DUR: u8 = 0x35;
const REG_WAKE_UP_SRC: u8 = 0x38;
const REG_ALL_INT_SRC: u8 = 0x3B;
const REG_CTRL7: u8 = 0x3F;

// Values.
const WHO_AM_I_VAL: u8 = 0x44;
const CTRL2_VAL_BOOT: u8 = 0b10000000;
const CTRL2_VAL_SOFT_RESET: u8 = 0b01000000;
const CTRL2_VAL_BDU: u8 = 0b00001000;
const CTRL2_VAL_IF_ADD_INC: u8 = 0b00000100;
/// INT1 source mask for a single tap.
pub const CTRL4_INT1_SINGLE_TAP: u8 = 0b01000000;
/// INT1 source mask for a double tap.
pub const CTRL4_INT1_DOUBLE_TAP: u8 = 0b00001000;
const WAKE_UP_THS_ENABLE_DOUBLE_TAP: u8 = 0b10000000;
const WAKE_UP_THS_VAL_SLEEP_ON: u8 = 0b01000000;
const WAKE_UP_DUR_STATIONARY: u8 = 0b00010000;
const CTRL6_VAL_LOW_NOISE: u8 = 0b00000100;
const CTRL7_VAL_INTERRUPTS_ENABLE: u8 = 0b00100000;
const CTRL7_VAL_DRDY_PULSED: u8 = 0b10000000;
const CTRL3_VAL_LIR: u8 = 0b00010000;
const CTRL6_VAL_FDS_HIGH: u8 = 0b00001000;
const STATUS_VAL_DRDY: u8 = 0b00000001;
/// Tap-detection enable bit for the Z axis.
pub const TAP_THS_Z_Z_AXIS_ENABLE: u8 = 0b00100000;
/// Interrupt source mask for a single tap.
pub const INTERRUPT_SRC_SINGLE_TAP: u8 = 0b00000100;
/// Interrupt source mask for a double tap.
pub const INTERRUPT_SRC_DOUBLE_TAP: u8 = 0b00001000;
const FIFO_CTRL_MODE_COLLECT_AND_STOP: u8 = 0b00100000;
const FIFO_CTRL_MODE_OFF: u8 = 0b00000000;
const FIFO_CTRL_FTH: u8 = 0b00011111;
const FIFO_SAMPLE_OVERRUN: u8 = 0b01000000;
const FIFO_SAMPLE_COUNT: u8 = 0b00111111;
const RANGE_16_G: u8 = 0b11;

/// A raw accelerometer reading.
#[derive(Clone, Copy, Debug, Default)]
pub struct Reading {
    pub x: i16,
    pub y: i16,
    pub z: i16,
}

/// An acceleration measurement in g.
#[derive(Clone, Copy, Debug, Default)]
pub struct Measurement {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// A FIFO of raw readings.
#[derive(Clone, Copy, Debug, Default)]
pub struct Fifo {
    pub count: u8,
    pub readings: [Reading; 32],
}

/// Data rate selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DataRate {
    PowerDown = 0,
    Lowest = 0b0001,
    H12_5 = 0b0010,
    H25 = 0b0011,
    H50 = 0b0100,
    H100 = 0b0101,
    H200 = 0b0110,
    H400 = 0b0111,
    H800 = 0b1000,
    H1600 = 0b1001,
}

/// Operating mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    LowPower = 0b00,
    HighPerformance = 0b01,
    OnDemand = 0b10,
}

/// Measurement range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Range {
    G16 = 0b11,
    G8 = 0b10,
    G4 = 0b01,
    G2 = 0b00,
}

/// Interrupt notification type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IntNotification {
    Pulsed = 0,
    Latched = 1,
}

/// Interrupt source bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum InterruptSource {
    SleepChange = 0b00100000,
    SixD = 0b00010000,
    DoubleTap = 0b00001000,
    SingleTap = 0b00000100,
    WakeUp = 0b00000010,
    Freefall = 0b00000001,
}

/// Low power mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LowPowerMode {
    Lp1 = 0b00,
    Lp2 = 0b01,
    Lp3 = 0b10,
    Lp4 = 0b11,
}

/// Bandwidth filtering mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Bandwidth {
    Div2 = 0b00,
    Div4 = 0b01,
    Div10 = 0b10,
    Div20 = 0b11,
}

/// Filter type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Filter {
    LowPass = 0,
    HighPass = 1,
}

/// FIFO mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FifoMode {
    Off = 0b000,
    CollectAndStop = 0b001,
    ContinuousToFifo = 0b011,
    BypassToContinuous = 0b100,
    CollectContinuous = 0b110,
}

/// Initializes the accelerometer.
pub fn begin() -> bool {
    if get_device_id() != WHO_AM_I_VAL {
        return false;
    }
    i2c::write8(ADDRESS, REG_CTRL2, CTRL2_VAL_BOOT);
    i2c::write8(ADDRESS, REG_CTRL2, CTRL2_VAL_SOFT_RESET);
    // Enable block-data-update and address auto-increment.
    i2c::write8(ADDRESS, REG_CTRL2, CTRL2_VAL_BDU | CTRL2_VAL_IF_ADD_INC);
    true
}

/// Returns the WHO_AM_I device ID.
pub fn get_device_id() -> u8 {
    i2c::read8(ADDRESS, REG_WHO_AM_I)
}

/// Returns true if new data is available.
pub fn have_new_data() -> bool {
    i2c::read8(ADDRESS, REG_STATUS) & STATUS_VAL_DRDY != 0
}

/// Reads a raw 3-axis reading (consecutive register reads).
pub fn get_raw_reading() -> Reading {
    let reg = [REG_OUT_X_L | 0x80];
    i2c::send(ADDRESS, &reg);
    let mut buf = [0u8; 6];
    i2c::receive(ADDRESS, &mut buf);
    Reading {
        x: u16::from_le_bytes([buf[0], buf[1]]) as i16,
        y: u16::from_le_bytes([buf[2], buf[3]]) as i16,
        z: u16::from_le_bytes([buf[4], buf[5]]) as i16,
    }
}

/// Reads a raw reading and converts it to an acceleration in g.
pub fn get_acceleration_measurement(out_reading: &mut Reading) -> Measurement {
    let reading = get_raw_reading();
    *out_reading = reading;
    let lsb_value: f32 = match get_range() {
        Range::G2 => 4.0,
        Range::G4 => 8.0,
        Range::G8 => 16.0,
        Range::G16 => 48.0,
    };
    Measurement {
        x: lsb_value * (reading.x as f32 / 64000.0),
        y: lsb_value * (reading.y as f32 / 64000.0),
        z: lsb_value * (reading.z as f32 / 64000.0),
    }
}

/// Reads the temperature (raw).
pub fn get_temperature() -> u16 {
    let lo = i2c::read8(ADDRESS, REG_OUT_TEMP_L);
    let hi = i2c::read8(ADDRESS, REG_OUT_TEMP_H);
    u16::from_le_bytes([lo, hi])
}

/// Sets the data rate.
pub fn set_data_rate(rate: DataRate) {
    let val = i2c::read8(ADDRESS, REG_CTRL1) & !(0b1111 << 4);
    i2c::write8(ADDRESS, REG_CTRL1, val | ((rate as u8) << 4));
}

/// Returns the current data rate.
pub fn get_data_rate() -> DataRate {
    match i2c::read8(ADDRESS, REG_CTRL1) >> 4 {
        0 => DataRate::PowerDown,
        0b0001 => DataRate::Lowest,
        0b0010 => DataRate::H12_5,
        0b0011 => DataRate::H25,
        0b0100 => DataRate::H50,
        0b0101 => DataRate::H100,
        0b0110 => DataRate::H200,
        0b0111 => DataRate::H400,
        0b1000 => DataRate::H800,
        _ => DataRate::H1600,
    }
}

/// Sets the operating mode.
pub fn set_mode(mode: Mode) {
    let val = i2c::read8(ADDRESS, REG_CTRL1) & !0b1100;
    i2c::write8(ADDRESS, REG_CTRL1, val | ((mode as u8) << 2));
}

/// Returns the operating mode.
pub fn get_mode() -> Mode {
    match (i2c::read8(ADDRESS, REG_CTRL1) >> 2) & 0b11 {
        0b01 => Mode::HighPerformance,
        0b10 => Mode::OnDemand,
        _ => Mode::LowPower,
    }
}

/// Sets the low-power mode.
pub fn set_low_power_mode(mode: LowPowerMode) {
    let val = i2c::read8(ADDRESS, REG_CTRL1) & !0b11;
    i2c::write8(ADDRESS, REG_CTRL1, val | (mode as u8));
}

/// Returns the low-power mode.
pub fn get_low_power_mode() -> LowPowerMode {
    match i2c::read8(ADDRESS, REG_CTRL1) & 0b11 {
        0b01 => LowPowerMode::Lp2,
        0b10 => LowPowerMode::Lp3,
        0b11 => LowPowerMode::Lp4,
        _ => LowPowerMode::Lp1,
    }
}

/// Sets the bandwidth filter.
pub fn set_bandwidth_filtering(bw: Bandwidth) {
    let val = i2c::read8(ADDRESS, REG_CTRL6) & !(0b11 << 6);
    i2c::write8(ADDRESS, REG_CTRL6, val | ((bw as u8) << 6));
}

/// Returns the bandwidth filter.
pub fn get_bandwidth_filtering() -> Bandwidth {
    match (i2c::read8(ADDRESS, REG_CTRL6) >> 6) & 0b11 {
        0b01 => Bandwidth::Div4,
        0b10 => Bandwidth::Div10,
        0b11 => Bandwidth::Div20,
        _ => Bandwidth::Div2,
    }
}

/// Sets the measurement range.
pub fn set_range(range: Range) {
    let val = i2c::read8(ADDRESS, REG_CTRL6) & !(RANGE_16_G << 4);
    i2c::write8(ADDRESS, REG_CTRL6, val | ((range as u8) << 4));
}

/// Returns the measurement range.
pub fn get_range() -> Range {
    match (i2c::read8(ADDRESS, REG_CTRL6) >> 4) & 0b11 {
        0b11 => Range::G16,
        0b10 => Range::G8,
        0b01 => Range::G4,
        _ => Range::G2,
    }
}

/// Sets the filter type (low/high pass).
pub fn set_filter_type(filter: Filter) {
    let val = i2c::read8(ADDRESS, REG_CTRL6) & !CTRL6_VAL_FDS_HIGH;
    i2c::write8(ADDRESS, REG_CTRL6, val | ((filter as u8) << 3));
}

/// Returns the filter type.
pub fn get_filter_type() -> Filter {
    if i2c::read8(ADDRESS, REG_CTRL6) & CTRL6_VAL_FDS_HIGH != 0 {
        Filter::HighPass
    } else {
        Filter::LowPass
    }
}

/// Sets low-noise mode.
pub fn set_low_noise_mode(on: bool) {
    let val = i2c::read8(ADDRESS, REG_CTRL6) & !CTRL6_VAL_LOW_NOISE;
    i2c::write8(
        ADDRESS,
        REG_CTRL6,
        val | if on { CTRL6_VAL_LOW_NOISE } else { 0 },
    );
}

/// Returns low-noise mode.
pub fn get_low_noise_mode() -> bool {
    i2c::read8(ADDRESS, REG_CTRL6) & CTRL6_VAL_LOW_NOISE != 0
}

/// Enables the FIFO.
pub fn enable_fifo() {
    i2c::write8(
        ADDRESS,
        REG_FIFO_CTRL,
        FIFO_CTRL_MODE_COLLECT_AND_STOP | FIFO_CTRL_FTH,
    );
}

/// Disables the FIFO.
pub fn disable_fifo() {
    i2c::write8(ADDRESS, REG_FIFO_CTRL, FIFO_CTRL_MODE_OFF);
}

/// Reads the FIFO, returning true if it overran.
pub fn read_fifo(fifo: &mut Fifo, timeout: u32) -> bool {
    let temp = i2c::read8(ADDRESS, REG_FIFO_SAMPLE);
    let overrun = temp & FIFO_SAMPLE_OVERRUN != 0;
    // The register exposes a 6-bit count, but the sensor FIFO and our storage
    // buffer both hold at most 32 samples. Keep `count` consistent with the
    // number of readings actually written so callers cannot index past the
    // fixed-size buffer when the device reports a saturated/invalid count.
    let available = (temp & FIFO_SAMPLE_COUNT).min(fifo.readings.len() as u8);
    fifo.count = available.min(timeout.min(u8::MAX as u32) as u8);
    for i in 0..fifo.count as usize {
        fifo.readings[i] = get_raw_reading();
    }
    overrun
}

/// Clears the FIFO.
pub fn clear_fifo() {
    i2c::write8(ADDRESS, REG_FIFO_CTRL, FIFO_CTRL_MODE_OFF);
    i2c::write8(
        ADDRESS,
        REG_FIFO_CTRL,
        FIFO_CTRL_MODE_COLLECT_AND_STOP | FIFO_CTRL_FTH,
    );
}

/// Enables double-tap detection.
pub fn enable_double_tap() {
    let config = i2c::read8(ADDRESS, REG_WAKE_UP_THS);
    i2c::write8(
        ADDRESS,
        REG_WAKE_UP_THS,
        config | WAKE_UP_THS_ENABLE_DOUBLE_TAP,
    );
}

/// Disables double-tap detection.
pub fn disable_double_tap() {
    let config = i2c::read8(ADDRESS, REG_WAKE_UP_THS);
    i2c::write8(
        ADDRESS,
        REG_WAKE_UP_THS,
        config & !WAKE_UP_THS_ENABLE_DOUBLE_TAP,
    );
}

/// Enables sleep detection.
pub fn enable_sleep() {
    let config = i2c::read8(ADDRESS, REG_WAKE_UP_THS);
    i2c::write8(ADDRESS, REG_WAKE_UP_THS, config | WAKE_UP_THS_VAL_SLEEP_ON);
}

/// Disables sleep detection.
pub fn disable_sleep() {
    let config = i2c::read8(ADDRESS, REG_WAKE_UP_THS);
    i2c::write8(ADDRESS, REG_WAKE_UP_THS, config & !WAKE_UP_THS_VAL_SLEEP_ON);
}

/// Enables stationary-motion detection.
pub fn enable_stationary_motion_detection() {
    let config = i2c::read8(ADDRESS, REG_WAKE_UP_DUR);
    i2c::write8(ADDRESS, REG_WAKE_UP_DUR, config | WAKE_UP_DUR_STATIONARY);
}

/// Disables stationary-motion detection.
pub fn disable_stationary_motion_detection() {
    let config = i2c::read8(ADDRESS, REG_WAKE_UP_DUR);
    i2c::write8(ADDRESS, REG_WAKE_UP_DUR, config & !WAKE_UP_DUR_STATIONARY);
}

/// Configures the wake-up threshold.
pub fn configure_wakeup_threshold(threshold: u8) {
    let config = i2c::read8(ADDRESS, REG_WAKE_UP_THS) & 0b11000000;
    i2c::write8(ADDRESS, REG_WAKE_UP_THS, config | threshold);
}

/// Configures the 6D threshold.
pub fn configure_6d_threshold(threshold: u8) {
    let config = i2c::read8(ADDRESS, REG_TAP_THS_X) & 0b01100000;
    i2c::write8(ADDRESS, REG_TAP_THS_X, config | ((threshold & 0b11) << 5));
}

/// Configures tap detection on the Z axis.
pub fn configure_tap_threshold(threshold_z: u8, axes_to_enable: u8) {
    let mut configuration = axes_to_enable & 0b00100000;
    if axes_to_enable & TAP_THS_Z_Z_AXIS_ENABLE != 0 {
        configuration |= threshold_z & 0b00011111;
    }
    i2c::write8(ADDRESS, REG_TAP_THS_Z, configuration);
}

/// Configures tap duration (latency, quiet, shock).
pub fn configure_tap_duration(latency: u8, quiet: u8, shock: u8) {
    let configuration = (latency << 4) | ((quiet & 0b11) << 2) | (shock & 0b11);
    i2c::write8(ADDRESS, REG_INT1_DUR, configuration);
}

/// Configures the INT1 sources.
pub fn configure_int1(sources: u8) {
    i2c::write8(ADDRESS, REG_CTRL4_INT1, sources);
}

/// Configures the INT2 sources.
pub fn configure_int2(sources: u8) {
    i2c::write8(ADDRESS, REG_CTRL5_INT2, sources);
}

/// Sets the interrupt notification type.
pub fn set_int_notification(val: IntNotification) {
    let config = i2c::read8(ADDRESS, REG_CTRL3);
    if val == IntNotification::Latched {
        i2c::write8(ADDRESS, REG_CTRL3, config | CTRL3_VAL_LIR);
    } else {
        i2c::write8(ADDRESS, REG_CTRL3, config & !CTRL3_VAL_LIR);
    }
}

/// Enables interrupts.
pub fn enable_interrupts() {
    let config = i2c::read8(ADDRESS, REG_CTRL7);
    i2c::write8(ADDRESS, REG_CTRL7, config | CTRL7_VAL_INTERRUPTS_ENABLE);
}

/// Disables interrupts.
pub fn disable_interrupts() {
    let config = i2c::read8(ADDRESS, REG_CTRL7);
    i2c::write8(ADDRESS, REG_CTRL7, config & !CTRL7_VAL_INTERRUPTS_ENABLE);
}

/// Configures pulsed DRDY interrupts.
pub fn pulsed_drdy_interrupts() {
    let config = i2c::read8(ADDRESS, REG_CTRL7);
    i2c::write8(ADDRESS, REG_CTRL7, config | CTRL7_VAL_DRDY_PULSED);
}

/// Configures latched DRDY interrupts.
pub fn latched_drdy_interrupts() {
    let config = i2c::read8(ADDRESS, REG_CTRL7);
    i2c::write8(ADDRESS, REG_CTRL7, config & !CTRL7_VAL_DRDY_PULSED);
}

/// Returns the wake-up source.
pub fn get_wakeup_source() -> u8 {
    i2c::read8(ADDRESS, REG_WAKE_UP_SRC)
}

/// Returns the interrupt source.
pub fn get_interrupt_source() -> u8 {
    i2c::read8(ADDRESS, REG_ALL_INT_SRC)
}

/// Returns the wake-up threshold.
pub fn get_wakeup_threshold() -> u8 {
    i2c::read8(ADDRESS, REG_WAKE_UP_THS) & 0b00111111
}
