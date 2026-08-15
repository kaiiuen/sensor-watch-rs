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

#![allow(clippy::result_unit_err)]

use crate::background_tasks::BackgroundTaskRegistry;
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
    fn get_analog_pin_level(&mut self, _pin: (u8, u8)) -> u16 {
        0
    }
    fn i2c_write16(&mut self, _addr: i16, _reg: u8, _data: u16) -> Result<(), ()> {
        Err(())
    }
    fn i2c_read16(&mut self, _addr: i16, _reg: u8) -> Result<u16, ()> {
        Err(())
    }
    /// The movement hooks a face calls via `crate::movement::...`. Provided with
    /// a default no-op so the mock is usable before any framework wiring exists.
    fn set_tick_rate(&mut self, _show_seconds: bool) {}
    fn play_signal(&mut self) {}
    fn default_loop_handler(&mut self, _event: Event, _settings: &Settings) {}
    /// Schedules a face-indexed one-shot task. Backends that do not model
    /// scheduling may keep the default no-op implementation.
    fn schedule_background_task_for_face(&mut self, _face_index: usize, _date_time: DateTime) {}
    /// Cancels a face-indexed one-shot task.
    fn cancel_background_task_for_face(&mut self, _face_index: usize) {}
    /// Turns off the bi-color LED (`watch::led::set_led_off`).
    fn set_led_off(&mut self) {}
    /// Configures a GPIO pin's direction (`watch::gpio::set_pin_direction`).
    /// `out` is true for `Direction::Out`, false otherwise; host shims translate.
    fn set_pin_direction(&mut self, _pin: (u8, u8), _out: bool) {}
    /// Sets a GPIO pin's output level (`watch::gpio::set_pin_level`).
    fn set_pin_level(&mut self, _pin: (u8, u8), _level: bool) {}
    /// Reads a GPIO pin's level for a non-button (e.g. flashlight output) pin
    /// (`watch::gpio::get_pin_level`, for pins outside the three buttons).
    fn read_pin_level(&mut self, _pin: (u8, u8)) -> bool {
        false
    }
    /// Sets the RTC date/time (`watch::rtc::set_date_time`).
    fn set_date_time(&mut self, _dt: DateTime) {}
    /// Clears the entire LCD display (`watch::slcd::clear_display`).
    fn clear_display(&mut self) {}
    /// Sets the bi-color LED to a raw (red, green) brightness pair
    /// (`watch::led::set_led_color`); red/green/yellow setters all funnel here.
    fn set_led_color(&mut self, _red: u8, _green: u8) {}
    /// Reads an RTC backup register (`watch::deepsleep::get_backup_data`).
    fn get_backup_data(&mut self, _reg: u8) -> u32 {
        0
    }
    /// Writes an RTC backup register (`watch::deepsleep::store_backup_data`).
    fn store_backup_data(&mut self, _data: u32, _reg: u8) {}
    /// Clears all five indicator segments at once (`watch::slcd::clear_all_indicators`).
    fn clear_all_indicators(&mut self) {}
    /// Starts the tick (colon) animation for `duration` ms
    /// (`watch::slcd::start_tick_animation`).
    fn start_tick_animation(&mut self, _duration: u32) {}
    /// Writes the RTC frequency-correction register (`watch::rtc::freqcorr_write`).
    fn freqcorr_write(&mut self, _value: i16, _sign: i16) {}
    /// Reads the RTC frequency-correction register (`watch::rtc::freqcorr_read`).
    fn freqcorr_read(&mut self) -> i16 {
        0
    }
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
    /// The segment shadow for one-off `set_pixel` / `clear_pixel` writes.
    pub segments: BTreeMap<(u8, u8), bool>,
    /// GPIO output/level shadow for non-button pins (e.g. the flashlight face's
    /// A2 output), keyed by the `(port, pin)` tuple.
    pub pin_levels: BTreeMap<(u8, u8), bool>,
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
    /// The 8 RTC backup registers, indexed 0..8 (`deepsleep::get/store_backup_data`).
    pub backup: [u32; 8],
    /// The last LED color written via `set_led_color` as (red, green).
    pub led_color: (u8, u8),
    /// Optional OPT3001 result register value; `None` means no sensor.
    pub opt3001_result: Option<u16>,
    /// Raw 16-bit thermistor ADC sample; zero means no sensor.
    pub thermistor_raw: u16,
    /// Per-backend host scheduler state.
    pub background_tasks: BackgroundTaskRegistry,
}

impl Default for MockHw {
    fn default() -> Self {
        MockHw {
            chars: [' '; 10],
            colon: false,
            indicators: [false; 5],
            segments: BTreeMap::new(),
            pin_levels: BTreeMap::new(),
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
            backup: [0; 8],
            led_color: (0, 0),
            opt3001_result: None,
            thermistor_raw: 0,
            background_tasks: BackgroundTaskRegistry::new(),
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

    /// The current level of a non-button GPIO pin (`true` = high), as written by
    /// `set_pin_level`. Defaults to `false` (e.g. for the flashlight face when it
    /// has not yet toggled A2 on).
    pub fn pin_level(&self, pin: (u8, u8)) -> bool {
        *self.pin_levels.get(&pin).unwrap_or(&false)
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

    /// Explicitly polls one due task. The task is cleared before the caller
    /// injects `Event::BackgroundTask` into the selected face; host polling does
    /// not dispatch events or claim hardware wake/timing behavior.
    pub fn poll_due_background_task(&mut self) -> Option<usize> {
        self.background_tasks.poll_due(self.now)
    }

    /// Clears every LCD character slot to a blank, matching `slcd::clear_display`.
    pub fn clear_display(&mut self) {
        self.chars.fill(' ');
    }

    /// Returns the value of RTC backup register `reg` (0-7).
    pub fn backup(&self, reg: u8) -> u32 {
        if (reg as usize) < self.backup.len() {
            self.backup[reg as usize]
        } else {
            0
        }
    }

    /// Seeds RTC backup register `reg` (0-7) with `data`.
    pub fn set_backup(&mut self, reg: u8, data: u32) {
        if (reg as usize) < self.backup.len() {
            self.backup[reg as usize] = data;
        }
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
    fn schedule_background_task_for_face(&mut self, face_index: usize, date_time: DateTime) {
        self.background_tasks
            .schedule(face_index, self.now, date_time);
    }
    fn cancel_background_task_for_face(&mut self, face_index: usize) {
        self.background_tasks.cancel(face_index);
    }

    fn get_analog_pin_level(&mut self, _pin: (u8, u8)) -> u16 {
        self.thermistor_raw
    }

    fn i2c_write16(&mut self, _addr: i16, _reg: u8, _data: u16) -> Result<(), ()> {
        if self.opt3001_result.is_some() {
            Ok(())
        } else {
            Err(())
        }
    }

    fn i2c_read16(&mut self, _addr: i16, _reg: u8) -> Result<u16, ()> {
        self.opt3001_result.ok_or(())
    }
    fn set_led_off(&mut self) {}
    fn set_pin_direction(&mut self, _pin: (u8, u8), _out: bool) {}
    fn set_pin_level(&mut self, pin: (u8, u8), level: bool) {
        self.pin_levels.insert(pin, level);
    }
    fn read_pin_level(&mut self, pin: (u8, u8)) -> bool {
        self.pin_level(pin)
    }
    fn set_date_time(&mut self, dt: DateTime) {
        self.now = dt;
    }
    fn clear_display(&mut self) {
        MockHw::clear_display(self);
    }
    fn set_led_color(&mut self, red: u8, green: u8) {
        self.led_color = (red, green);
    }
    fn get_backup_data(&mut self, reg: u8) -> u32 {
        MockHw::backup(self, reg)
    }
    fn store_backup_data(&mut self, data: u32, reg: u8) {
        MockHw::set_backup(self, reg, data);
    }
    fn clear_all_indicators(&mut self) {
        self.indicators.fill(false);
    }
    fn start_tick_animation(&mut self, _duration: u32) {
        self.tick_animation = true;
    }
    fn freqcorr_write(&mut self, _value: i16, _sign: i16) {}
    fn freqcorr_read(&mut self) -> i16 {
        0
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
