//! Host implementation of the `movement` framework that runs the REAL faces.
//!
//! The real `src/movement/mod.rs` is the ARM framework: it declares all 111
//! faces, a `WATCH_FACES` table, and framework plumbing that touches MMIO-backed
//! `watch` calls, so it cannot (yet) compile on host. Step 1 provides the exact
//! subset the seam needs:
//!
//! - [`types`] — the REAL `src/movement/types.rs`, verbatim (via `#[path]`): the
//!   `WatchFace` trait, `Event`, `Settings`, `Button`, `ButtonEvent`, `ClockMode`,
//!   `BuzzerNote`/`BuzzerPriority`, `MovementState`. This is the contract faces
//!   implement, untouched.
//! - [`simple_clock`] — the REAL `src/movement/simple_clock.rs`, verbatim. This
//!   is the proof that a face's `impl WatchFace` compiles and runs against the
//!   mock.
//! - `set_tick_rate` / `play_signal` / `default_loop_handler` — host versions of
//!   the three framework free functions the face calls, forwarded to the `Hw`
//!   seam.
//!
//! As more faces are migrated (step 2), each migrates here the same way as
//! `simple_clock`: `#[path]`-include the real file and re-export its face type.
//! The `Hw` trait grows a method only when a face's host build needs a call the
//! trait does not yet carry (keep it minimal).

pub mod types {
    // Reuse the real movement types verbatim so the trait/event contract is the
    // same code the firmware binary compiles (no drift).
    //
    // NOTE: `#[path]` for a module nested inside an inline `mod types {}` is
    // resolved relative to `src/host/movement/types/` (rustc appends the inline
    // module's own directory), so it needs one extra `../` over the natural
    // crate-relative read.
    #[path = "../../../movement/types.rs"]
    pub mod real;
    pub use real::{
        Button, ButtonEvent, BuzzerPriority, ClockMode, Event, MovementState, Settings, WatchFace,
    };
    // Re-export the `BuzzerNote` alias the faces / state use.
    pub use crate::watch::buzzer::Note as BuzzerNote;
}

/// The REAL `simple_clock` face, pulled in verbatim and re-exported so host
/// tests can drive its `WatchFace` impl against a mock.
pub mod simple_clock {
    #[path = "../../../movement/simple_clock.rs"]
    pub mod real;
    pub use real::SimpleClockFace;
}

use crate::watch;
use types::{Event, Settings};

/// Sets the wake rate based on whether seconds are shown. Host forwards to the
/// `Hw::set_tick_rate` hook.
pub fn set_tick_rate(show_seconds: bool) {
    watch::seam::hw().set_tick_rate(show_seconds);
}

/// Plays the signal tune. Host forwards to the `Hw::play_signal` hook.
pub fn play_signal() {
    watch::seam::hw().play_signal();
}

/// The default (no-handler) event dispatch. Host forwards to
/// `Hw::default_loop_handler`.
pub fn default_loop_handler(event: Event, settings: &mut Settings) {
    let s = sensor_watch_core::settings::Settings { reg: settings.reg };
    watch::seam::hw().default_loop_handler(to_core_event(event), &s);
    settings.reg = s.reg;
}

/// Converts the firmware movement `Event`/`Settings` to the shared core types
/// used by the `Hw` seam (they are isomorphic).
fn to_core_event(event: Event) -> sensor_watch_core::mock_hw::Event {
    use sensor_watch_core::mock_hw::{Button, Event as E};
    match event {
        Event::Activate => E::Activate,
        Event::Tick => E::Tick,
        Event::BackgroundTask => E::BackgroundTask,
        Event::Button(b, e) => E::Button(
            match b {
                types::Button::Light => Button::Light,
                types::Button::Mode => Button::Mode,
                types::Button::Alarm => Button::Alarm,
            },
            to_core_button_event(e),
        ),
        Event::SingleTap => E::SingleTap,
        Event::DoubleTap => E::DoubleTap,
        Event::AccelerometerWake => E::AccelerometerWake,
    }
}

/// Maps a firmware [`types::ButtonEvent`] onto its isomorphic core twin.
fn to_core_button_event(e: types::ButtonEvent) -> sensor_watch_core::mock_hw::ButtonEvent {
    use sensor_watch_core::mock_hw::ButtonEvent as BE;
    match e {
        types::ButtonEvent::Down => BE::Down,
        types::ButtonEvent::Up => BE::Up,
        types::ButtonEvent::LongPress => BE::LongPress,
        types::ButtonEvent::LongUp => BE::LongUp,
        types::ButtonEvent::ReallyLongPress => BE::ReallyLongPress,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watch::seam;
    use sensor_watch_core::mock_hw::{Indicator, MockHw, dt};
    // The real face's `impl WatchFace` provides `activate`/`loop_`; bring the
    // trait into scope so both methods can be called on `SimpleClockFace`.
    use types::WatchFace;

    /// Friday 2023-01-06 15:04:00, healthy battery.
    fn steady_state() -> MockHw {
        let mut hw = MockHw::new();
        hw.set_time(dt(2023, 1, 6, 15, 4, 0));
        hw.vcc_mv = 3000;
        hw
    }

    fn h24_settings() -> Settings {
        let mut s = Settings::default();
        s.set_clock_mode_24h(true);
        s
    }

    #[test]
    fn real_simple_clock_renders_24h_via_mock() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);

        let mut settings = h24_settings();
        settings.set_show_seconds(true);
        let mut face = simple_clock::SimpleClockFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Tick, &mut settings);

        // The REAL face write path (FR + day 06 + HH:MM:SS) recorded on the mock.
        assert_eq!(mock.text(), "FR06150400");
        assert!(mock.colon);
        assert!(mock.indicator(Indicator::H24));
    }

    #[test]
    fn real_simple_clock_battery_low_sets_lap_once() {
        let mut mock = MockHw::new();
        mock.set_time(dt(2023, 1, 6, 15, 4, 0));
        mock.vcc_mv = 2000; // below the 2200 mV threshold
        seam::install_hw(&mut mock);

        let mut settings = h24_settings();
        let mut face = simple_clock::SimpleClockFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Tick, &mut settings);
        assert!(mock.indicator(Indicator::Lap));
    }

    #[test]
    fn real_simple_clock_alarm_button_toggles_seconds() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);

        let mut settings = h24_settings();
        let mut face = simple_clock::SimpleClockFace::new();
        face.activate(&settings);
        face.loop_(
            types::Event::Button(
                types::Button::Alarm,
                types::ButtonEvent::Up, // firmware-typed event (real face contract)
            ),
            &mut settings,
        );
        assert!(settings.show_seconds());
    }
}
