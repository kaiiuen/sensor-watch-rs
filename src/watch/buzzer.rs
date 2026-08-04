//! Piezo buzzer driver.
//!
//! Port of the C `watch_buzzer.c` and the private buzzer helpers. The buzzer is
//! driven by TCC0 (shared with the LED) plus a TC3 timer for non-blocking note
//! sequences.

use crate::watch::gpio::{self, Direction, Function, Pin};
use crate::watch::led::{disable_leds, enable_leds};
use atsaml22j::tc0::count8::ctrla::{Modeselect, Prescalerselect};
use atsaml22j::tcc0::RegisterBlock as Tcc0;

/// The buzzer pin (PA27) and its TCC channel.
const BUZZER: Pin = Pin(0, 27);
const BUZZER_TCC_CHANNEL: usize = 1;

/// PMUX function value for the TCC0 output (function F = 5).
const TCC_PINMUX: u8 = 5;

/// The configured buzzer voltage (in tenths of a volt, 0-90).
static mut BUZZER_VOLTAGE: u8 = 0;

/// Sets the buzzer voltage (in tenths of a volt).
///
/// This is stored and applied to the buzzer drive. On boards with an
/// adjustable buzzer supply, this controls the output level.
pub fn set_voltage(voltage: u8) {
    unsafe {
        BUZZER_VOLTAGE = voltage;
    }
}

/// Returns the configured buzzer voltage (in tenths of a volt).
pub fn voltage() -> u8 {
    unsafe { BUZZER_VOLTAGE }
}

/// Note periods for the notes (1 MHz clock / frequency).
pub const NOTE_PERIODS: [u16; 87] = [
    18182, 17161, 16197, 15288, 14430, 13620, 12857, 12134, 11453, 10811, 10204, 9631, 9091, 8581,
    8099, 7645, 7216, 6811, 6428, 6068, 5727, 5405, 5102, 4816, 4545, 4290, 4050, 3822, 3608, 3405,
    3214, 3034, 2863, 2703, 2551, 2408, 2273, 2145, 2025, 1911, 1804, 1703, 1607, 1517, 1432, 1351,
    1276, 1204, 1136, 1073, 1012, 956, 902, 851, 804, 758, 716, 676, 638, 602, 568, 536, 506, 478,
    451, 426, 402, 379, 358, 338, 319, 301, 284, 268, 253, 239, 225, 213, 201, 190, 179, 169, 159,
    150, 142, 134, 127,
];

/// A musical note, corresponding to the index into `NOTE_PERIODS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Note {
    A1 = 0,
    A1SharpB1Flat,
    B1,
    C2,
    C2SharpD2Flat,
    D2,
    D2SharpE2Flat,
    E2,
    F2,
    F2SharpG2Flat,
    G2,
    G2SharpA2Flat,
    A2,
    A2SharpB2Flat,
    B2,
    C3,
    C3SharpD3Flat,
    D3,
    D3SharpE3Flat,
    E3,
    F3,
    F3SharpG3Flat,
    G3,
    G3SharpA3Flat,
    A3,
    A3SharpB3Flat,
    B3,
    C4,
    C4SharpD4Flat,
    D4,
    D4SharpE4Flat,
    E4,
    F4,
    F4SharpG4Flat,
    G4,
    G4SharpA4Flat,
    A4,
    A4SharpB4Flat,
    B4,
    C5,
    C5SharpD5Flat,
    D5,
    D5SharpE5Flat,
    E5,
    F5,
    F5SharpG5Flat,
    G5,
    G5SharpA5Flat,
    A5,
    A5SharpB5Flat,
    B5,
    C6,
    C6SharpD6Flat,
    D6,
    D6SharpE6Flat,
    E6,
    F6,
    F6SharpG6Flat,
    G6,
    G6SharpA6Flat,
    A6,
    A6SharpB6Flat,
    B6,
    C7,
    C7SharpD7Flat,
    D7,
    D7SharpE7Flat,
    E7,
    F7,
    F7SharpG7Flat,
    G7,
    G7SharpA7Flat,
    A7,
    A7SharpB7Flat,
    B7,
    C8,
    C8SharpD8Flat,
    D8,
    D8SharpE8Flat,
    E8,
    F8,
    F8SharpG8Flat,
    G8,
    G8SharpA8Flat,
    A8,
    A8SharpB8Flat,
    B8,
    /// No sound.
    Rest,
}

/// Returns a reference to the TCC0 peripheral register block.
fn tcc0() -> &'static Tcc0 {
    // SAFETY: the TCC0 register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { &*atsaml22j::Tcc0::PTR }
}

/// Returns a reference to the TC3 (alias of TC0) COUNT8 register block.
fn tc3() -> &'static atsaml22j::tc0::count8::Count8 {
    // SAFETY: the TC3 register block lives at a fixed address for the whole
    // program.
    unsafe { (*atsaml22j::Tc3::PTR).count8() }
}

/// Returns a reference to the GCLK peripheral register block.
fn gclk() -> &'static atsaml22j::gclk::RegisterBlock {
    // SAFETY: the GCLK register block lives at a fixed address for the whole
    // program.
    unsafe { &*atsaml22j::Gclk::PTR }
}

/// Returns a reference to the MCLK peripheral register block.
fn mclk() -> &'static atsaml22j::mclk::RegisterBlock {
    // SAFETY: the MCLK register block lives at a fixed address for the whole
    // program.
    unsafe { &*atsaml22j::Mclk::PTR }
}

/// Waits for the TCC to finish synchronizing.
fn tcc_sync() {
    while tcc0().syncbusy().read().bits() != 0 {}
}

/// Waits for the TC to finish synchronizing.
fn tc_sync() {
    while tc3().syncbusy().read().bits() != 0 {}
}

/// Returns true if the TCC0 peripheral is enabled.
fn tcc_is_enabled() -> bool {
    tcc0().ctrla().read().enable().bit_is_set()
}

/// Enables or disables the TCC's RUNSTDBY bit.
fn tcc_write_runstdby(value: bool) {
    tcc0().ctrla().modify(|_, w| w.enable().clear_bit());
    tcc0().ctrla().modify(|_, w| w.runstdby().bit(value));
    tcc0().ctrla().modify(|_, w| w.enable().set_bit());
    tcc_sync();
}

/// Enables the TCC peripheral, which drives the buzzer.
pub fn enable_buzzer() {
    if !tcc_is_enabled() {
        enable_leds();
    }
}

/// Sets the period of the buzzer.
///
/// `period = 1000000 / freq`.
pub fn set_buzzer_period(period: u32) {
    // SAFETY: writing valid period/compare-buffer values.
    unsafe {
        tcc0().perbuf().write(|w| w.bits(period));
        tcc0()
            .ccbuf(BUZZER_TCC_CHANNEL)
            .write(|w| w.bits(period / 2));
    }
}

/// Disables the TCC peripheral that drives the buzzer.
pub fn disable_buzzer() {
    disable_leds();
}

/// Turns the buzzer output on.
pub fn set_buzzer_on() {
    gpio::set_pin_direction(BUZZER, Direction::Out);
    gpio::set_pin_function(BUZZER, Function::Mux(TCC_PINMUX));
}

/// Turns the buzzer output off.
pub fn set_buzzer_off() {
    gpio::set_pin_direction(BUZZER, Direction::Off);
    gpio::set_pin_function(BUZZER, Function::Off);
}

/// Plays a note for a given duration (blocking).
pub fn play_note(note: Note, _duration_ms: u16) {
    if note == Note::Rest {
        set_buzzer_off();
    } else {
        set_buzzer_period(NOTE_PERIODS[note as usize] as u32);
        set_buzzer_on();
    }
    // TODO: blocking delay is not yet ported; the caller should provide one.
    set_buzzer_off();
}

// --- Non-blocking note sequences (TC3 timer) ---

static mut SEQ_POSITION: u16 = 0;
static mut TONE_TICKS: i8 = 0;
static mut REPEAT_COUNTER: i8 = -1;
static mut CALLBACK_RUNNING: bool = false;
static mut SEQUENCE: *const i8 = core::ptr::null();
static mut CB_FINISHED: Option<fn()> = None;

/// Starts the TC3 timer.
fn tc3_start() {
    tc3().ctrla().modify(|_, w| w.enable().set_bit());
    unsafe { CALLBACK_RUNNING = true };
}

/// Stops the TC3 timer.
fn tc3_stop() {
    tc3().ctrla().modify(|_, w| w.enable().clear_bit());
    tc_sync();
    unsafe { CALLBACK_RUNNING = false };
}

/// Initializes TC3 for a 64 Hz interrupt.
fn tc3_initialize() {
    mclk().apbcmask().modify(|_, w| w.tc3_().set_bit());
    gclk()
        .pchctrl(24)
        .write(|w| w.r#gen().gclk3().chen().set_bit());
    tc3_stop();
    tc3().ctrla().write(|w| w.swrst().set_bit());
    tc_sync();
    tc3().ctrla().modify(|_, w| {
        w.prescaler().variant(Prescalerselect::Div64);
        w.mode().variant(Modeselect::Count8);
        w.runstdby().set_bit()
    });
    // 32 kHz / 64 / 8 = 64 Hz.
    // SAFETY: writing a valid PER value.
    unsafe {
        tc3().per().write(|w| w.bits(7));
    }
    tc3().intenset().modify(|_, w| w.ovf().set_bit());
    cortex_m::peripheral::NVIC::unpend(atsaml22j::Interrupt::TC3);
    // SAFETY: unmasking a valid interrupt is safe.
    unsafe { cortex_m::peripheral::NVIC::unmask(atsaml22j::Interrupt::TC3) };
}

/// Plays a sequence of notes in a non-blocking way.
///
/// `note_sequence` is a pointer to a sequence of (note, duration) tuples ending
/// with a zero. A negative note value rewinds the sequence by that many notes;
/// the following byte is the loop count.
pub fn play_sequence(note_sequence: *const i8, callback_on_end: Option<fn()>) {
    if unsafe { CALLBACK_RUNNING } {
        tc3_stop();
    }
    set_buzzer_off();
    unsafe {
        SEQUENCE = note_sequence;
        CB_FINISHED = callback_on_end;
        SEQ_POSITION = 0;
        TONE_TICKS = 0;
        REPEAT_COUNTER = -1;
    }
    enable_buzzer();
    tc3_initialize();
    tcc_write_runstdby(true);
    tc3_start();
}

/// The 64 Hz sequence callback.
fn cb_watch_buzzer_seq() {
    unsafe {
        if TONE_TICKS == 0 {
            let seq = SEQUENCE;
            let pos = SEQ_POSITION as isize;
            if *seq.add(pos as usize) < 0 && *seq.add(pos as usize + 1) != 0 {
                // Repeat indicator found.
                if REPEAT_COUNTER == -1 {
                    REPEAT_COUNTER = *seq.add(pos as usize + 1);
                } else {
                    REPEAT_COUNTER -= 1;
                }
                if REPEAT_COUNTER > 0 {
                    // Rewind.
                    let rewind = *seq.add(pos as usize) as isize * -2;
                    if pos as i16 > rewind as i16 {
                        SEQ_POSITION = (pos + rewind) as u16;
                    } else {
                        SEQ_POSITION = 0;
                    }
                } else {
                    // Continue.
                    SEQ_POSITION = (pos + 2) as u16;
                    REPEAT_COUNTER = -1;
                }
            }
            let pos = SEQ_POSITION as isize;
            if *seq.add(pos as usize) != 0 && *seq.add(pos as usize + 1) != 0 {
                // Read note.
                let note = *seq.add(pos as usize) as u8;
                if note != Note::Rest as u8 {
                    set_buzzer_period(NOTE_PERIODS[note as usize] as u32);
                    set_buzzer_on();
                } else {
                    set_buzzer_off();
                }
                TONE_TICKS = *seq.add(pos as usize + 1);
                SEQ_POSITION = (pos + 2) as u16;
            } else {
                // End the sequence.
                abort_sequence();
                if let Some(cb) = CB_FINISHED {
                    cb();
                }
            }
        } else {
            TONE_TICKS -= 1;
        }
    }
}

/// Aborts a playing sequence.
pub fn abort_sequence() {
    if unsafe { CALLBACK_RUNNING } {
        tc3_stop();
    }
    set_buzzer_off();
    tcc_write_runstdby(false);
}

/// The TC3 interrupt handler.
///
/// The PAC's `rt` feature declares `extern "C" { fn TC3(); }` and places it in
/// the vector table, so we provide the matching `#[no_mangle]` symbol here.
#[unsafe(no_mangle)]
pub extern "C" fn TC3() {
    cb_watch_buzzer_seq();
    tc3().intflag().write(|w| w.ovf().set_bit());
}
