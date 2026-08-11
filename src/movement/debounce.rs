//! Button debouncing.
//!
//! Mechanical buttons bounce for 5-20 ms, producing multiple spurious
//! edges. This module filters the raw EIC interrupts by requiring a stable
//! reading over several consecutive samples before accepting a state change.

use crate::movement::types::{Button, ButtonEvent, Event};

/// Number of consecutive stable samples required to accept a state change.
///
/// The button interrupts fire on both edges; we require the pin to read the
/// same level this many times in a row before treating it as a real press or
/// release. This filters out bounce.
const DEBOUNCE_SAMPLES: u8 = 4;

/// Per-button debounce state.
struct DebounceState {
    /// The last accepted stable level (true = pressed).
    stable_level: bool,
    /// The current candidate level being sampled.
    candidate_level: bool,
    /// How many consecutive samples matched the candidate.
    sample_count: u8,
    /// The timestamp (in fast ticks) when the button went down, for long-press.
    down_timestamp: u16,
    /// Whether a long-press has already been reported for this press.
    long_reported: bool,
    /// Whether a really-long-press has already been reported for this press.
    really_long_reported: bool,
}

impl DebounceState {
    const fn new() -> Self {
        DebounceState {
            stable_level: false,
            candidate_level: false,
            sample_count: 0,
            down_timestamp: 0,
            long_reported: false,
            really_long_reported: false,
        }
    }
}

/// Debounce state for the three buttons.
static mut LIGHT: DebounceState = DebounceState::new();
static mut MODE: DebounceState = DebounceState::new();
static mut ALARM: DebounceState = DebounceState::new();

/// Feeds a raw pin reading into the debouncer for a button.
///
/// Returns `Some(event)` when a debounced state change is accepted, or `None`
/// while the reading is still bouncing or unchanged.
pub fn update(button: Button, raw_level: bool, fast_ticks: u16) -> Option<Event> {
    unsafe {
        let state = match button {
            Button::Light => &mut LIGHT,
            Button::Mode => &mut MODE,
            Button::Alarm => &mut ALARM,
        };

        // If the raw reading matches the candidate, count it; otherwise reset.
        if raw_level == state.candidate_level {
            state.sample_count = state.sample_count.saturating_add(1);
        } else {
            state.candidate_level = raw_level;
            state.sample_count = 1;
        }

        // Only accept once the reading is stable for enough samples.
        if state.sample_count < DEBOUNCE_SAMPLES {
            return None;
        }

        // If the stable level hasn't changed, nothing to report.
        if state.stable_level == state.candidate_level {
            return None;
        }

        // Accept the change.
        state.stable_level = state.candidate_level;
        state.sample_count = 0;

        if state.stable_level {
            // Rising edge: button pressed.
            state.down_timestamp = fast_ticks;
            state.long_reported = false;
            state.really_long_reported = false;
            Some(Event::Button(button, ButtonEvent::Down))
        } else {
            // Falling edge: button released.
            let long = state.long_reported;
            state.down_timestamp = 0;
            if long {
                Some(Event::Button(button, ButtonEvent::LongUp))
            } else {
                Some(Event::Button(button, ButtonEvent::Up))
            }
        }
    }
}

/// Called on each fast tick to detect long-presses.
///
/// Returns `Some(event)` when a button has been held long enough to qualify
/// as a long-press.
pub fn check_long_press(button: Button, fast_ticks: u16) -> Option<Event> {
    unsafe {
        let state = match button {
            Button::Light => &mut LIGHT,
            Button::Mode => &mut MODE,
            Button::Alarm => &mut ALARM,
        };
        if state.stable_level
            && !state.long_reported
            && fast_ticks.wrapping_sub(state.down_timestamp)
                >= crate::movement::types::MOVEMENT_LONG_PRESS_TICKS
        {
            state.long_reported = true;
            return Some(Event::Button(button, ButtonEvent::LongPress));
        }
        if state.stable_level
            && !state.really_long_reported
            && fast_ticks.wrapping_sub(state.down_timestamp)
                >= crate::movement::types::MOVEMENT_REALLY_LONG_PRESS_TICKS
        {
            state.really_long_reported = true;
            return Some(Event::Button(button, ButtonEvent::ReallyLongPress));
        }
        None
    }
}
