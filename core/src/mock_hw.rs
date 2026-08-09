//! The hardware seam: a `Hw` trait plus a reference mock backend.
//!
//! # Why this exists
//!
//! The firmware faces (`movement/*.rs` in the `sensor-watch` crate) call
//! `watch::slcd::display_string(...)`, `watch::rtc::get_date_time()`,
//! `watch::gpio::get_pin_level(...)`, `watch::adc::get_vcc_voltage()`, and
//! `watch::slcd::set_indicator(...)` against SAM L22 MMIO registers. This trait
//! captures *exactly* those methods with identical signatures, so a face can be
//! written (or transcribed) against [`Hw`] and then be driven by either a wrapper
//! over the real hardware or over [`MockHw`] on the host.
//!
//! This is the foundation the simulator/fuzzer need: instead of Studio's
//! hand-written `studio/src/face_sim.rs` reimplementation (which can drift from
//! the firmware), the *real* face code can run against a mock that records what
//! it wrote to the LCD. See [`crate::hostsim`] for the proof-of-concept that runs
//! `simple_clock` through this.

use crate::datetime::DateTime;
use crate::settings::Settings;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

/// The LCD indicator segments, mirroring `watch::slcd::Indicator`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indicator {
    Signal = 0,
    Bell = 1,
    Pm = 2,
    H24 = 3,
    Lap = 4,
}

/// The buttons used by the movement framework, mirroring `watch::gpio`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Button {
    Light,
    Mode,
    Alarm,
}

/// A button press event (`watch::movement::ButtonEvent`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonEvent {
    Down,
    Up,
    LongPress,
    LongUp,
    ReallyLongPress,
}

/// The closed set of events that wake the CPU (`watch::movement::Event`).
///
/// Kept local so `core` can express the `Hw::default_loop_handler` hook without
/// depending on the (arm-only) firmware `movement` module. Must stay in lockstep
/// with `src/movement/types.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A watch face entered the foreground.
    Activate,
    /// The RTC ticked (once per second).
    Tick,
    /// A scheduled background task is due.
    BackgroundTask,
    /// A button was pressed.
    Button(Button, ButtonEvent),
    /// The accelerometer detected a single tap.
    SingleTap,
    /// The accelerometer detected a double tap.
    DoubleTap,
    /// The accelerometer detected motion (wake-on-motion).
    AccelerometerWake,
}

impl Event {
    /// Returns the subsecond value (0 on non-tick events).
    pub fn subsecond(&self) -> u8 {
        0
    }
}

/// The methods the firmware faces call on the hardware.
///
/// Keep this list *deliberately minimal*: it is the seam for porting faces, so
/// grow it one method at a time as each face needs it. Initial members are those
/// `simple_clock` needs.
pub trait Hw {
    /// Displays a string at a 0-9 digit position (`watch::slcd::display_string`).
    /// A space clears a digit.
    fn display_string(&mut self, s: &str, pos: u8);
    /// Turns the colon on.
    fn set_colon(&mut self);
    /// Turns the colon off.
    fn clear_colon(&mut self);
    /// Sets an indicator segment (`watch::slcd::set_indicator`).
    fn set_indicator(&mut self, i: Indicator);
    /// Clears an indicator segment (`watch::slcd::clear_indicator`).
    fn clear_indicator(&mut self, i: Indicator);
    /// Sets a raw (com, seg) pixel (`watch::slcd::set_pixel`), for one-off icons.
    fn set_pixel(&mut self, com: u8, seg: u8);
    /// Clears a raw (com, seg) pixel (`watch::slcd::clear_pixel`).
    fn clear_pixel(&mut self, com: u8, seg: u8);
    /// Whether the tick animation is running (`watch::slcd::tick_animation_is_running`).
    fn tick_animation_is_running(&mut self) -> bool;
    /// Stops the tick animation (`watch::slcd::stop_tick_animation`).
    fn stop_tick_animation(&mut self);
    /// Reads the current time (`watch::rtc::get_date_time`).
    fn get_date_time(&mut self) -> DateTime;
    /// Reads an input pin (`watch::gpio::get_pin_level`), mapped to a button.
    fn get_button_level(&mut self, _button: Button) -> bool {
        false
    }
    /// Returns an approximate VCC in millivolts (`watch::adc::get_vcc_voltage`).
    fn get_vcc_voltage(&mut self) -> u16;
    /// The movement hooks a face calls via `crate::movement::...`. Provided with
    /// a default no-op so the mock is usable before any framework wiring exists.
    fn set_tick_rate(&mut self, _show_seconds: bool) {}
    fn play_signal(&mut self) {}
    fn default_loop_handler(&mut self, _event: Event, _settings: &Settings) {}
}

/// A reference display-level mock of the hardware.
///
/// It records the LCD characters, colon, indicator flags, and segment shadow a
/// face produces, plus a seeded clock and button levels, so a host test can drive
/// a face's `loop_()`/`activate()` and assert on the resulting LCD snapshot.
#[derive(Clone, Debug)]
pub struct MockHw {
    /// The 10 digit slots (position 0..10), ' ' = blank.
    pub chars: [char; 10],
    /// Colon on/off.
    pub colon: bool,
    /// Indicator flags, indexed by [`Indicator`].
    pub indicators: [bool; 5],
    /// Segment shadow for one-off `set_pixel` / `clear_pixel` writes.
    pub segments: BTreeMap<(u8, u8), bool>,
    /// What `get_date_time()` returns.
    pub now: DateTime,
    /// What `get_button_level()` returns per button.
    pub buttons: BTreeMap<Button, bool>,
    /// What `get_vcc_voltage()` returns.
    pub vcc_mv: u16,
    /// True while a tick animation is considered running.
    pub tick_animation: bool,
    /// Number of `display_string` calls (useful to assert "no redundant redraw").
    pub display_string_calls: u64,
    /// Number of `get_date_time` calls.
    pub rtc_reads: u64,
}

impl Default for MockHw {
    fn default() -> Self {
        MockHw {
            chars: [' '; 10],
            colon: false,
            indicators: [false; 5],
            segments: BTreeMap::new(),
            now: DateTime {
                second: 0,
                minute: 0,
                hour: 0,
                day: 0,
                month: 0,
                year: 0,
            },
            buttons: BTreeMap::new(),
            vcc_mv: 0,
            tick_animation: false,
            display_string_calls: 0,
            rtc_reads: 0,
        }
    }
}

impl MockHw {
    /// The default state a face test starts from (blank LCD, a known time).
    pub fn new() -> Self {
        Self::default()
    }

    /// True if indicator `i` is currently set.
    pub fn indicator(&self, i: Indicator) -> bool {
        self.indicators[i as usize]
    }

    /// The current LCD text as a `String`, mapping blank cells (space or the
    /// trailing NUL bytes the firmware's `write_*` leaves behind) to nothing,
    /// so assertions read naturally (e.g. `"FR06150400"`). Note the firmware
    /// places the weekday and day adjacent (`FR06`) with the colon handled by a
    /// separate segment, so there is no embedded space.
    pub fn text(&self) -> String {
        let raw = self.chars.iter().collect::<String>();
        raw.trim_end_matches([' ', '\0']).to_string()
    }

    /// Seeds the simulated RTC clock.
    pub fn set_time(&mut self, dt: DateTime) {
        self.now = dt;
    }
}

impl Hw for MockHw {
    fn display_string(&mut self, s: &str, pos: u8) {
        self.display_string_calls += 1;
        for (i, c) in s.chars().enumerate() {
            let p = pos as usize + i;
            if p < self.chars.len() {
                self.chars[p] = c;
            }
        }
    }
    fn set_colon(&mut self) {
        self.colon = true;
    }
    fn clear_colon(&mut self) {
        self.colon = false;
    }
    fn set_indicator(&mut self, i: Indicator) {
        self.indicators[i as usize] = true;
    }
    fn clear_indicator(&mut self, i: Indicator) {
        self.indicators[i as usize] = false;
    }
    fn set_pixel(&mut self, com: u8, seg: u8) {
        self.segments.insert((com, seg), true);
    }
    fn clear_pixel(&mut self, com: u8, seg: u8) {
        self.segments.insert((com, seg), false);
    }
    fn tick_animation_is_running(&mut self) -> bool {
        self.tick_animation
    }
    fn stop_tick_animation(&mut self) {
        self.tick_animation = false;
    }
    fn get_date_time(&mut self) -> DateTime {
        self.rtc_reads += 1;
        self.now
    }
    fn get_button_level(&mut self, button: Button) -> bool {
        *self.buttons.get(&button).unwrap_or(&false)
    }
    fn get_vcc_voltage(&mut self) -> u16 {
        self.vcc_mv
    }
}

/// Builds a `DateTime` on the reference year (2020) so `utility::get_weekday`
/// and friends work. Mirrors the firmware's packed register meaning.
pub fn dt(y: u16, mo: u8, d: u8, h: u8, mi: u8, s: u8) -> DateTime {
    DateTime {
        second: s,
        minute: mi,
        hour: h,
        day: d,
        month: mo,
        year: (y - crate::datetime::WATCH_RTC_REFERENCE_YEAR) as u8,
    }
}
