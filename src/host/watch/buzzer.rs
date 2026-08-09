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

/// Host: no-op (the mock does not play audio yet).
pub fn play_signal() {}
/// Host: no-op.
pub fn play_note(_note: Note, _duration_ms: u16) {}
/// Host: no-op.
pub fn play_sequence(_note_sequence: *const i8, _callback: Option<fn()>) {}
