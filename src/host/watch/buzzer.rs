//! Host buzzer shim: no-op note playback + the shared `Note` type.
//!
//! `src/watch/buzzer.rs` defines the full 87-entry `Note` enum used by movement
//! state and by faces. On host, playback is a no-op (a mock could record it later
//! if a face needs to assert on beeps); the enum is provided so the real
//! `movement/types.rs` (which uses `Note` in `MovementState` and `BuzzerPriority`)
//! compiles unchanged on host.

/// A musical note. Fields are `#[repr(u8)]` indices; for host builds only the
/// variant set (and the `Rest` sentinel) matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[allow(non_camel_case_types)]
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

/// Host model of the board buzzer voltage configuration.
pub fn set_voltage(voltage: u8) -> Result<(), ()> {
    (voltage <= 90).then_some(()).ok_or(())
}

/// Host: no-op (the mock does not play audio yet).
pub fn play_signal() {}
/// Host: no-op.
pub fn play_note(_note: Note, _duration_ms: u16) {}
/// Host: no-op.
pub fn play_sequence(_note_sequence: &[i8], _callback: Option<fn()>) {}

/// The oscillator period (in clock ticks) for each [`Note`], matching the real
/// `src/watch/buzzer.rs` so faces that index `NOTE_PERIODS[Note as usize]`
/// (e.g. `tuning_tones`) compile unchanged on host.
pub const NOTE_PERIODS: [u16; 87] = [
    18182, 17161, 16197, 15288, 14430, 13620, 12857, 12134, 11453, 10811, 10204, 9631, 9091, 8581,
    8099, 7645, 7216, 6811, 6428, 6068, 5727, 5405, 5102, 4816, 4545, 4290, 4050, 3822, 3608, 3405,
    3214, 3034, 2863, 2703, 2551, 2408, 2273, 2145, 2025, 1911, 1804, 1703, 1607, 1517, 1432, 1351,
    1276, 1204, 1136, 1073, 1012, 956, 902, 851, 804, 758, 716, 676, 638, 602, 568, 536, 506, 478,
    451, 426, 402, 379, 358, 338, 319, 301, 284, 268, 253, 239, 225, 213, 201, 190, 179, 169, 159,
    150, 142, 134, 127,
];

/// Host: no-op.
pub fn set_buzzer_period(_period: u32) {}
/// Host: no-op.
pub fn set_buzzer_on() {}
/// Host: no-op.
pub fn set_buzzer_off() {}
