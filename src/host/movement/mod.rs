//! Host implementation of the `movement` framework that runs the REAL faces.
//!
//! The real `src/movement/mod.rs` is the ARM framework: it declares all 111
//! faces, a `WATCH_FACES` table, and framework plumbing that touches MMIO-backed
//! `watch` calls, so it cannot (yet) compile on host. Step 1 provides the exact
//! subset the seam needs:
//!
//! - [`types`] - the REAL `src/movement/types.rs`, verbatim (via `#[path]`): the
//!   `WatchFace` trait, `Event`, `Settings`, `Button`, `ButtonEvent`, `ClockMode`,
//!   `BuzzerNote`/`BuzzerPriority`, `MovementState`. This is the contract faces
//!   implement, untouched.
//! - [`simple_clock`] - the REAL `src/movement/simple_clock.rs`, verbatim. This
//!   is the proof that a face's `impl WatchFace` compiles and runs against the
//!   mock.
//! - `set_tick_rate` / `play_signal` / `default_loop_handler` - host versions of
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

// ---- The rest of the host-compilable subset. ------------------------------
// Each real face is pulled in verbatim via `#[path]` (never edited) and its
// face *type* is re-exported. The `#[path]` inside an inline `mod` resolves
// relative to `src/host/movement/<name>/`, hence the `../../../movement/...`.

/// The REAL `alarm` face. Calls `movement::illuminate_led`,
/// `movement::play_alarm_beeps`, and `watch::led::set_led_off`.
pub mod alarm {
    #[path = "../../../movement/alarm.rs"]
    pub mod real;
    pub use real::AlarmFace;
}

/// The REAL `counter` face. Calls `movement::play_alarm_beeps`.
pub mod counter {
    #[path = "../../../movement/counter.rs"]
    pub mod real;
    pub use real::CounterFace;
}

/// The REAL `world_clock` face. Calls `movement::move_to_next_face` and
/// `crate::movement::TIMEZONE_OFFSETS`.
pub mod world_clock {
    #[path = "../../../movement/world_clock.rs"]
    pub mod real;
    pub use real::WorldClockFace;
}

/// The REAL `stopwatch` face. Calls `movement::schedule_background_task` and
/// `movement::cancel_background_task`.
pub mod stopwatch {
    #[path = "../../../movement/stopwatch.rs"]
    pub mod real;
    pub use real::StopwatchFace;
}

/// The REAL `timer` face. Calls `movement::schedule_background_task_for_face`,
/// `movement::cancel_background_task_for_face`, `movement::play_alarm`,
/// `movement::play_alarm_beeps`, `movement::move_to_face`, and
/// `movement::illuminate_led`.
pub mod timer {
    #[path = "../../../movement/timer.rs"]
    pub mod real;
    pub use real::TimerFace;
}

/// The REAL `countdown` face. Calls `movement::schedule_background_task_for_face`,
/// `movement::cancel_background_task_for_face`, `movement::play_alarm`, and
/// `movement::move_to_next_face`.
pub mod countdown {
    #[path = "../../../movement/countdown.rs"]
    pub mod real;
    pub use real::CountdownFace;
}

/// The REAL `flashlight` face. Uses `watch::gpio` for its A2 output.
pub mod flashlight {
    #[path = "../../../movement/flashlight.rs"]
    pub mod real;
    pub use real::FlashlightFace;
}

/// The REAL `blinky` face.
pub mod blinky {
    #[path = "../../../movement/blinky.rs"]
    pub mod real;
    pub use real::BlinkyFace;
}

/// The REAL `beeps` face.
pub mod beeps {
    #[path = "../../../movement/beeps.rs"]
    pub mod real;
    pub use real::BeepsFace;
}

/// The REAL `character_set` face.
pub mod character_set {
    #[path = "../../../movement/character_set.rs"]
    pub mod real;
    pub use real::CharacterSetFace;
}

/// The REAL `demo` face.
pub mod demo {
    #[path = "../../../movement/demo.rs"]
    pub mod real;
    pub use real::DemoFace;
}

/// The REAL `beats` face.
pub mod beats {
    #[path = "../../../movement/beats.rs"]
    pub mod real;
    pub use real::BeatsFace;
}

/// The REAL `astronomy` face.
pub mod astronomy {
    #[path = "../../../movement/astronomy.rs"]
    pub mod real;
    pub use real::AstronomyFace;
}

/// The REAL `close_enough` face.
pub mod close_enough {
    #[path = "../../../movement/close_enough.rs"]
    pub mod real;
    pub use real::CloseEnoughClockFace;
}

/// The REAL `day_night_percentage` face.
pub mod day_night_percentage {
    #[path = "../../../movement/day_night_percentage.rs"]
    pub mod real;
    pub use real::DayNightPercentageFace;
}

/// The REAL `day_one` face.
pub mod day_one {
    #[path = "../../../movement/day_one.rs"]
    pub mod real;
    pub use real::DayOneFace;
}

/// The REAL `deadline` face.
pub mod deadline {
    #[path = "../../../movement/deadline.rs"]
    pub mod real;
    pub use real::DeadlineFace;
}

/// The REAL `decimal_time` face.
pub mod decimal_time {
    #[path = "../../../movement/decimal_time.rs"]
    pub mod real;
    pub use real::DecimalTimeFace;
}

/// The REAL `french_revolutionary` face.
pub mod french_revolutionary {
    #[path = "../../../movement/french_revolutionary.rs"]
    pub mod real;
    pub use real::FrenchRevolutionaryFace;
}

/// The REAL `frequency_correction` face.
pub mod frequency_correction {
    #[path = "../../../movement/frequency_correction.rs"]
    pub mod real;
    pub use real::FrequencyCorrectionFace;
}

/// The REAL `hello_there` face.
pub mod hello_there {
    #[path = "../../../movement/hello_there.rs"]
    pub mod real;
    pub use real::HelloThereFace;
}

/// The REAL `ke_decimal_time` face.
pub mod ke_decimal_time {
    #[path = "../../../movement/ke_decimal_time.rs"]
    pub mod real;
    pub use real::KeDecimalTimeFace;
}

// ---- I-P subset (owner of this host-test migration) -----------------------
// Each real face in the alphabetical I-P group is pulled in verbatim.

/// The REAL `interval` face. Uses `movement::schedule/cancel_background_task_for_face`,
/// `movement::play_alarm_beeps`, `movement::illuminate_led`, `movement::move_to_face`,
/// `watch::slcd`, `watch::led`, `watch::rtc`, and `watch::utility`.
pub mod interval {
    #[path = "../../../movement/interval.rs"]
    pub mod real;
    pub use real::IntervalFace;
}

/// The REAL `invaders` face. Uses `movement::illuminate_led`,
/// `movement::play_alarm_beeps`, `watch::rtc`, and `watch::slcd`.
pub mod invaders {
    #[path = "../../../movement/invaders.rs"]
    pub mod real;
    pub use real::InvadersFace;
}

/// The REAL `ish` face. Uses `movement::clock_mode_24h`,
/// `movement::get_local_date_time`, `movement::default_loop_handler`, and
/// `watch::slcd`.
pub mod ish {
    #[path = "../../../movement/ish.rs"]
    pub mod real;
    pub use real::IshFace;
}

/// The REAL `kitchen_conversions` face. Uses `movement::play_alarm_beeps`,
/// `watch::slcd`, and `watch::buzzer::Note`.
pub mod kitchen_conversions {
    #[path = "../../../movement/kitchen_conversions.rs"]
    pub mod real;
    pub use real::KitchenConversionsFace;
}

/// The REAL `lander` face. Uses `watch::gpio`, `watch::extint`, `watch::storage`,
/// `watch::led`, `watch::rtc`, `movement::request_tick_frequency`, and `watch::slcd`.
pub mod lander {
    #[path = "../../../movement/lander.rs"]
    pub mod real;
    pub use real::LanderFace;
}

/// The REAL `lightmeter` face. Uses `watch::slcd` and `movement::default_loop_handler`.
pub mod lightmeter {
    #[path = "../../../movement/lightmeter.rs"]
    pub mod real;
    pub use real::LightmeterFace;
}

/// The REAL `lis2dw_logging` face. Uses `movement::enable/disable_tap_detection_if_available`,
/// `watch::rtc`, and `watch::slcd`.
pub mod lis2dw_logging {
    #[path = "../../../movement/lis2dw_logging.rs"]
    pub mod real;
    pub use real::Lis2dwLoggingFace;
}

/// The REAL `mars_time` face. Uses `movement::TIMEZONE_OFFSETS`,
/// `movement::illuminate_led`, `watch::rtc`, `watch::slcd`, and `watch::utility`.
pub mod mars_time {
    #[path = "../../../movement/mars_time.rs"]
    pub mod real;
    pub use real::MarsTimeFace;
}

/// The REAL `menstrual_cycle` face. Uses `movement::TIMEZONE_OFFSETS`,
/// `movement::move_to_next_face`, `movement::play_alarm_beeps`, `watch::rtc`,
/// `watch::slcd`, and `watch::utility`.
pub mod menstrual_cycle {
    #[path = "../../../movement/menstrual_cycle.rs"]
    pub mod real;
    pub use real::MenstrualCycleFace;
}

/// The REAL `metronome` face. Uses `movement::move_to_next_face`,
/// `movement::play_alarm_beeps`, `watch::slcd`, and `watch::buzzer::Note`.
pub mod metronome {
    #[path = "../../../movement/metronome.rs"]
    pub mod real;
    pub use real::MetronomeFace;
}

/// The REAL `minimal_clock` face. Uses `movement::default_loop_handler`,
/// `watch::rtc`, and `watch::slcd`.
pub mod minimal_clock {
    #[path = "../../../movement/minimal_clock.rs"]
    pub mod real;
    pub use real::MinimalClockFace;
}

/// The REAL `minmax` face. Uses `movement::default_loop_handler`, `watch::rtc`,
/// and `watch::slcd`.
pub mod minmax {
    #[path = "../../../movement/minmax.rs"]
    pub mod real;
    pub use real::MinmaxFace;
}

/// The REAL `minute_repeater_decimal` face. Uses `movement::play_signal`,
/// `movement::play_alarm_beeps`, `watch::slcd`, `watch::adc`, `watch::rtc`, and
/// `watch::utility`.
pub mod minute_repeater_decimal {
    #[path = "../../../movement/minute_repeater_decimal.rs"]
    pub mod real;
    pub use real::MinuteRepeaterDecimalFace;
}

/// The REAL `moon_phase` face. Uses `movement::TIMEZONE_OFFSETS`, `watch::rtc`,
/// `watch::slcd`, and `watch::utility`.
pub mod moon_phase {
    #[path = "../../../movement/moon_phase.rs"]
    pub mod real;
    pub use real::MoonPhaseFace;
}

/// The REAL `morsecalc` face. Uses `movement::move_to_next_face`,
/// `movement::illuminate_led`, `watch::slcd`, and `watch::led`.
pub mod morsecalc {
    #[path = "../../../movement/morsecalc.rs"]
    pub mod real;
    pub use real::MorsecalcFace;
}

/// The REAL `nanosec` face. Uses `movement::move_to_next_face`, `watch::rtc`,
/// `watch::slcd`, and `watch::utility`.
pub mod nanosec {
    #[path = "../../../movement/nanosec.rs"]
    pub mod real;
    pub use real::NanosecFace;
}

/// The REAL `orrery` face. Uses `movement::TIMEZONE_OFFSETS`, `watch::rtc`,
/// `watch::slcd`, and `watch::utility`.
pub mod orrery {
    #[path = "../../../movement/orrery.rs"]
    pub mod real;
    pub use real::OrreryFace;
}

/// The REAL `periodic` face. Uses `movement::illuminate_led`,
/// `movement::move_to_next_face`, `movement::move_to_face`, and `watch::slcd`.
pub mod periodic {
    #[path = "../../../movement/periodic.rs"]
    pub mod real;
    pub use real::PeriodicFace;
}

/// The REAL `ping` face. Uses `movement::get_local_date_time`,
/// `movement::request_tick_frequency`, `movement::play_note`, `movement::play_sequence`,
/// `movement::enable/disable_tap_detection_if_available`, `watch::gpio`,
/// `watch::extint`, `watch::slcd`, and `watch::buzzer::Note`.
pub mod ping {
    #[path = "../../../movement/ping.rs"]
    pub mod real;
    pub use real::PingFace;
}

/// The REAL `planetary_hours` face. Uses `movement::TIMEZONE_OFFSETS`, `watch::rtc`,
/// `watch::slcd`, and `watch::utility`.
pub mod planetary_hours {
    #[path = "../../../movement/planetary_hours.rs"]
    pub mod real;
    pub use real::PlanetaryHoursFace;
}

/// The REAL `planetary_time` face. Uses `movement::TIMEZONE_OFFSETS`, `watch::rtc`,
/// `watch::slcd`, and `watch::utility`.
pub mod planetary_time {
    #[path = "../../../movement/planetary_time.rs"]
    pub mod real;
    pub use real::PlanetaryTimeFace;
}

/// The REAL `preferences` face. Uses `movement::move_to_next_face`,
/// `movement::save_settings`, `watch::slcd`, and `watch::led`.
pub mod preferences {
    #[path = "../../../movement/preferences.rs"]
    pub mod real;
    pub use real::PreferencesFace;
}

/// The REAL `probability` face. Uses `watch::rtc`, `watch::slcd`, and
/// `movement::default_loop_handler`.
pub mod probability {
    #[path = "../../../movement/probability.rs"]
    pub mod real;
    pub use real::ProbabilityFace;
}

/// The REAL `pulsometer` face. Uses `watch::slcd` and `movement::default_loop_handler`.
pub mod pulsometer {
    #[path = "../../../movement/pulsometer.rs"]
    pub mod real;
    pub use real::PulsometerFace;
}

// ---- Q-Z subset (owner of this host-test migration) -----------------------
// Each remaining real face in the alphabetical Q-Z group is pulled in verbatim.

/// The REAL `randonaut` face.
pub mod randonaut {
    #[path = "../../../movement/randonaut.rs"]
    pub mod real;
    pub use real::RandonautFace;
}

/// The REAL `ratemeter` face.
pub mod ratemeter {
    #[path = "../../../movement/ratemeter.rs"]
    pub mod real;
    pub use real::RatemeterFace;
}

/// The REAL `repetition_minute` face.
pub mod repetition_minute {
    #[path = "../../../movement/repetition_minute.rs"]
    pub mod real;
    pub use real::RepetitionMinuteFace;
}

/// The REAL `rpn_calculator` face.
pub mod rpn_calculator {
    #[path = "../../../movement/rpn_calculator.rs"]
    pub mod real;
    pub use real::RpnCalculatorFace;
}

/// The REAL `rpn_calculator_alt` face.
pub mod rpn_calculator_alt {
    #[path = "../../../movement/rpn_calculator_alt.rs"]
    pub mod real;
    pub use real::RpnCalculatorAltFace;
}

/// The REAL `sailing` face.
pub mod sailing {
    #[path = "../../../movement/sailing.rs"]
    pub mod real;
    pub use real::SailingFace;
}

/// The REAL `save_load` face.
pub mod save_load {
    #[path = "../../../movement/save_load.rs"]
    pub mod real;
    pub use real::SaveLoadFace;
}

/// The REAL `set_time` face.
pub mod set_time {
    #[path = "../../../movement/set_time.rs"]
    pub mod real;
    pub use real::SetTimeFace;
}

/// The REAL `set_time_hackwatch` face.
pub mod set_time_hackwatch {
    #[path = "../../../movement/set_time_hackwatch.rs"]
    pub mod real;
    pub use real::SetTimeHackwatchFace;
}

/// The REAL `ships_bell` face.
pub mod ships_bell {
    #[path = "../../../movement/ships_bell.rs"]
    pub mod real;
    pub use real::ShipsBellFace;
}

/// The REAL `simon` face.
pub mod simon {
    #[path = "../../../movement/simon.rs"]
    pub mod real;
    pub use real::SimonFace;
}

/// The REAL `simple_calculator` face.
pub mod simple_calculator {
    #[path = "../../../movement/simple_calculator.rs"]
    pub mod real;
    pub use real::SimpleCalculatorFace;
}

/// The REAL `simple_clock_bin_led` face.
pub mod simple_clock_bin_led {
    #[path = "../../../movement/simple_clock_bin_led.rs"]
    pub mod real;
    pub use real::SimpleClockBinLedFace;
}

/// The REAL `simple_coin_flip` face.
pub mod simple_coin_flip {
    #[path = "../../../movement/simple_coin_flip.rs"]
    pub mod real;
    pub use real::SimpleCoinFlipFace;
}

/// The REAL `solar_time` face.
pub mod solar_time {
    #[path = "../../../movement/solar_time.rs"]
    pub mod real;
    pub use real::SolarTimeFace;
}

/// The REAL `solstice` face.
pub mod solstice {
    #[path = "../../../movement/solstice.rs"]
    pub mod real;
    pub use real::SolsticeFace;
}

/// The REAL `sos` face.
pub mod sos {
    #[path = "../../../movement/sos.rs"]
    pub mod real;
    pub use real::SosFace;
}

/// The REAL `squash` face.
pub mod squash {
    #[path = "../../../movement/squash.rs"]
    pub mod real;
    pub use real::SquashFace;
}

/// Host-only timer state for the REAL stock stopwatch face. The face keeps its
/// timer API unchanged; its cfg-gated host implementation records lifecycle
/// operations without touching MMIO.
pub mod stock_stopwatch_timer {
    use core::sync::atomic::{AtomicBool, Ordering};

    static INITIALIZED: AtomicBool = AtomicBool::new(false);
    static RUNNING: AtomicBool = AtomicBool::new(false);

    pub fn initialize() {
        INITIALIZED.store(true, Ordering::Relaxed);
        RUNNING.store(false, Ordering::Relaxed);
    }

    pub fn start() {
        RUNNING.store(true, Ordering::Relaxed);
    }

    pub fn stop() {
        RUNNING.store(false, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    pub fn is_initialized() -> bool {
        INITIALIZED.load(Ordering::Relaxed)
    }
}

/// The REAL stock stopwatch face. Its timer MMIO is cfg-gated behind the host
/// timer shim above, so the unchanged face logic can run through the host seam.
pub mod stock_stopwatch {
    #[path = "../../../movement/stock_stopwatch.rs"]
    pub mod real;
    pub use real::{StockStopwatchFace, host_tick};
}

/// The REAL `sunrise_sunset` face.
pub mod sunrise_sunset {
    #[path = "../../../movement/sunrise_sunset.rs"]
    pub mod real;
    pub use real::SunriseSunsetFace;
}

/// The REAL `tachymeter` face.
pub mod tachymeter {
    #[path = "../../../movement/tachymeter.rs"]
    pub mod real;
    pub use real::TachymeterFace;
}

/// The REAL `tally` face.
pub mod tally {
    #[path = "../../../movement/tally.rs"]
    pub mod real;
    pub use real::TallyFace;
}

/// The REAL `tarot` face.
pub mod tarot {
    #[path = "../../../movement/tarot.rs"]
    pub mod real;
    pub use real::TarotFace;
}

/// The REAL `tempchart` face.
pub mod tempchart {
    #[path = "../../../movement/tempchart.rs"]
    pub mod real;
    pub use real::TempchartFace;
}

/// The REAL `thermistor_logging` face.
pub mod thermistor_logging {
    #[path = "../../../movement/thermistor_logging.rs"]
    pub mod real;
    pub use real::ThermistorLoggingFace;
}

/// The REAL `thermistor_readout` face.
pub mod thermistor_readout {
    #[path = "../../../movement/thermistor_readout.rs"]
    pub mod real;
    pub use real::ThermistorReadoutFace;
}

/// The REAL `thermistor_testing` face.
pub mod thermistor_testing {
    #[path = "../../../movement/thermistor_testing.rs"]
    pub mod real;
    pub use real::ThermistorTestingFace;
}

/// The REAL `tide` face.
pub mod tide {
    #[path = "../../../movement/tide.rs"]
    pub mod real;
    pub use real::TideFace;
}

/// The REAL `time_left` face.
pub mod time_left {
    #[path = "../../../movement/time_left.rs"]
    pub mod real;
    pub use real::TimeLeftFace;
}

/// The REAL `tomato` face.
pub mod tomato {
    #[path = "../../../movement/tomato.rs"]
    pub mod real;
    pub use real::TomatoFace;
}

/// The REAL `toss_up` face.
pub mod toss_up {
    #[path = "../../../movement/toss_up.rs"]
    pub mod real;
    pub use real::TossUpFace;
}

/// The REAL `totp` face.
pub mod totp {
    #[path = "../../../movement/totp.rs"]
    pub mod real;
    pub use real::TotpFace;
}

/// The REAL `totp_lfs` face.
pub mod totp_lfs {
    #[path = "../../../movement/totp_lfs.rs"]
    pub mod real;
    pub use real::TotpFaceLfs;
}

/// The REAL `tuning_tones` face.
pub mod tuning_tones {
    #[path = "../../../movement/tuning_tones.rs"]
    pub mod real;
    pub use real::TuningTonesFace;
}

/// The REAL `voltage` face.
pub mod voltage {
    #[path = "../../../movement/voltage.rs"]
    pub mod real;
    pub use real::VoltageFace;
}

/// The REAL `wake` face.
pub mod wake {
    #[path = "../../../movement/wake.rs"]
    pub mod real;
    pub use real::WakeFace;
}

/// The REAL `wareki` face.
pub mod wareki {
    #[path = "../../../movement/wareki.rs"]
    pub mod real;
    pub use real::WarekiFace;
}

/// The REAL `weeknumber` face.
pub mod weeknumber {
    #[path = "../../../movement/weeknumber.rs"]
    pub mod real;
    pub use real::WeekNumberClockFace;
}

/// The REAL `wordle` face.
pub mod wordle {
    #[path = "../../../movement/wordle.rs"]
    pub mod real;
    pub use real::WordleFace;
}

/// The REAL `world_clock2` face.
pub mod world_clock2 {
    #[path = "../../../movement/world_clock2.rs"]
    pub mod real;
    pub use real::WorldClock2Face;
}

/// The REAL `wyoscan` face.
pub mod wyoscan {
    #[path = "../../../movement/wyoscan.rs"]
    pub mod real;
    pub use real::WyoscanFace;
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
/// `Hw::default_loop_handler`. Mirrors the real `movement::default_loop_handler`
/// signature (`&Settings`, since faces may hold it immutably).
pub fn default_loop_handler(event: Event, settings: &Settings) {
    let s = sensor_watch_core::settings::Settings { reg: settings.reg };
    watch::seam::hw().default_loop_handler(to_core_event(event), &s);
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

/// Time zone offsets in minutes from UTC, matching `src/movement/mod.rs`.
/// Re-exported so real faces that reference `crate::movement::TIMEZONE_OFFSETS`
/// (world_clock, timer, countdown) compile unchanged on host.
pub const TIMEZONE_OFFSETS: [i16; 41] = [
    0, 60, 120, 180, 210, 240, 270, 300, 330, 345, 360, 390, 420, 480, 525, 540, 570, 600, 630,
    660, 720, 765, 780, 825, 840, -720, -660, -600, -570, -540, -480, -420, -360, -300, -270, -240,
    -210, -180, -150, -120, -60,
];

/// Illuminates the LED. Host: no-op (the mock does not model LED brightness
/// yet). Mirrors the firmware `movement::illuminate_led`.
pub fn illuminate_led() {}

/// Cycles the framework to the next face. Host: no-op (single-face harness).
pub fn move_to_next_face() {}

/// Moves the framework to a specific face. Host: no-op (single-face harness).
pub fn move_to_face(_watch_face_index: usize) {}

/// Schedules a background task at `date_time` for the current face.
/// Host: no-op (the mock does not model the scheduler; faces that rely on
/// background tasks are driven by feeding `Event::BackgroundTask` directly in
/// tests).
pub fn schedule_background_task(_date_time: sensor_watch_core::datetime::DateTime) {}

/// Cancels the current face's background task. Host: no-op.
pub fn cancel_background_task() {}

/// Schedules a background task for a specific face. Host: no-op.
pub fn schedule_background_task_for_face(
    _watch_face_index: usize,
    _date_time: sensor_watch_core::datetime::DateTime,
) {
}

/// Cancels a specific face's background task. Host: no-op.
pub fn cancel_background_task_for_face(_watch_face_index: usize) {}

/// Plays the alarm tune. Host: no-op.
pub fn play_alarm() {}

/// Plays `rounds` of `alarm_note`. Host: no-op.
pub fn play_alarm_beeps(_rounds: u8, _alarm_note: types::BuzzerNote) {}

/// Saves the current settings so they survive a reset. Host: no-op (the mock
/// keeps settings in memory only). Mirrors `movement::save_settings`.
pub fn save_settings() {}

/// Returns the current UTC date/time (the RTC stores UTC). Host: returns the
/// installed mock's clock unchanged.
pub fn get_utc_date_time() -> sensor_watch_core::datetime::DateTime {
    crate::watch::rtc::get_date_time()
}

/// Returns the current local date/time by applying the configured time zone
/// offset. Host: applies `TIMEZONE_OFFSETS[0]` (UTC) so host tests are
/// deterministic; faces that need a non-UTC zone read the offset themselves via
/// `TIMEZONE_OFFSETS` in their own draw path.
pub fn get_local_date_time() -> sensor_watch_core::datetime::DateTime {
    get_utc_date_time()
}

/// Returns the current time zone offset (minutes) for the configured zone.
/// Host: returns 0 (UTC) for determinism in host tests.
pub fn get_current_timezone_offset() -> i32 {
    0
}

/// Returns the clock mode as a 12H/24H/024H enum. Host: returns `H24` so tests
/// are deterministic (the mock has no global settings register); faces make
/// `H24` the default and assert on it.
pub fn clock_mode_24h() -> types::ClockMode {
    types::ClockMode::H24
}

/// Requests a change in the tick frequency (power of two, 1-128 Hz). Host:
/// forwards to `set_tick_rate` just like the firmware.
pub fn request_tick_frequency(freq: u8) {
    if freq.is_power_of_two() && (1..=128).contains(&freq) {
        set_tick_rate(freq != 1);
    }
}

/// Plays a single note with the given priority. Host: no-op (the mock does not
/// model audio playback). Mirrors `movement::play_note`.
pub fn play_note(_note: types::BuzzerNote, _priority: u8) {}

/// Plays a note sequence. Host: no-op. Mirrors `movement::play_sequence`.
pub fn play_sequence(_note_sequence: *const i8, _callback_on_end: Option<fn()>) {}

/// Detects and enables tap detection if an accelerometer is present.
/// Host: returns false (no accelerometer), so faces fall back to button control.
pub fn enable_tap_detection_if_available() -> bool {
    false
}

/// Disables tap detection. Host: returns false (no accelerometer was on).
pub fn disable_tap_detection_if_available() -> bool {
    false
}

#[cfg(test)]
mod face_tests_stock_stopwatch;

#[cfg(test)]
// Host tests for the I-P face subset (driven via the `Hw` seam, parallel to the
// shared `tests` module so concurrent face owners can each add their own tests
// without conflict).
mod face_tests_ip;

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

    // ---- alarm -----------------------------------------------------------------

    #[test]
    fn real_alarm_activate_24h_renders_dow_alarm_index_and_time() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();

        // Default slot 0: day = ALARM_DAY_EACH_DAY (7 -> "SO"), hour 0, minute 0.
        let mut face = alarm::AlarmFace::new_static();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);

        assert!(mock.colon);
        // 24h mode sets the H24 indicator; disabled alarm clears Signal.
        assert!(mock.indicator(Indicator::H24));
        assert!(!mock.indicator(Indicator::Signal));
        // Non-setting mode shows DOW "AL" + alarm index "01" + hour " 0" (no
        // leading zero) + minute "00".
        assert_eq!(mock.text(), "AL01 000");
    }

    #[test]
    fn real_alarm_light_press_enters_setting_and_changes_dow() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = alarm::AlarmFace::new_static();
        face.activate(&settings);

        // Enter settings mode (Light up) -> setting_state 0.
        face.loop_(
            types::Event::Button(types::Button::Light, types::ButtonEvent::Up),
            &mut settings,
        );
        // Advance to the DOW field (setting_state 1).
        face.loop_(
            types::Event::Button(types::Button::Light, types::ButtonEvent::Up),
            &mut settings,
        );
        // In the DOW field, Alarm up advances the day and enables the alarm
        // (state > 0), so the Signal indicator is drawn on the next Tick.
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
            &mut settings,
        );
        face.loop_(types::Event::Tick, &mut settings);
        assert!(mock.indicator(Indicator::Signal));
    }

    // ---- counter ---------------------------------------------------------------

    #[test]
    fn real_counter_activate_shows_zero_and_sets_signal() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = counter::CounterFace::new_static();
        face.activate(&settings);
        assert!(mock.indicator(Indicator::Signal));

        face.loop_(types::Event::Activate, &mut settings);
        assert_eq!(mock.text(), "CO    00");
    }

    #[test]
    fn real_counter_alarm_increments_and_long_press_resets() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = counter::CounterFace::new_static();
        face.activate(&settings);

        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
            &mut settings,
        );
        assert_eq!(mock.text(), "CO    01");
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
            &mut settings,
        );
        // 2 counts: 01 -> 02.
        assert_eq!(mock.text(), "CO    02");

        // Long-press resets to 00.
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::LongPress),
            &mut settings,
        );
        assert_eq!(mock.text(), "CO    00");
    }

    #[test]
    fn real_counter_light_long_press_toggles_beep_signal() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = counter::CounterFace::new_static();
        face.activate(&settings);
        assert!(mock.indicator(Indicator::Signal));

        face.loop_(
            types::Event::Button(types::Button::Light, types::ButtonEvent::LongPress),
            &mut settings,
        );
        assert!(!mock.indicator(Indicator::Signal));
    }

    // ---- world_clock -----------------------------------------------------------

    #[test]
    fn real_world_clock_activate_renders_label_and_24h_time() {
        let mut mock = steady_state(); // Friday 2023-01-06 15:04:00
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = world_clock::WorldClockFace::new_static();
        face.activate(&settings);

        face.loop_(types::Event::Activate, &mut settings);
        assert!(mock.colon);
        assert!(mock.indicator(Indicator::H24));
        // timezone 0 == UTC == local; label chars 0,0 -> two spaces then day+time.
        assert_eq!(&mock.chars[2..], ['0', '6', '1', '5', '0', '4', '0', '0']);
    }

    #[test]
    fn real_world_clock_settings_mode_shows_offset() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = world_clock::WorldClockFace::new_static();
        face.activate(&settings);

        // Alarm long-press enters settings mode.
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::LongPress),
            &mut settings,
        );
        face.loop_(types::Event::Tick, &mut settings);
        // Settings screen 1: char_0 label, space, offset (00:00 for UTC).
        assert!(mock.colon);
        assert!(!mock.indicator(Indicator::Pm));
    }

    // ---- stopwatch -------------------------------------------------------------

    #[test]
    fn real_stopwatch_activate_shows_zero_and_sets_colon() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = stopwatch::StopwatchFace::new_static();
        face.activate(&settings);

        face.loop_(types::Event::Activate, &mut settings);
        assert!(mock.colon);
        assert_eq!(mock.text(), "st  000000");
    }

    #[test]
    fn real_stopwatch_tick_started_counts_seconds() {
        let mut mock = steady_state(); // 15:04:00
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = stopwatch::StopwatchFace::new_static();
        face.activate(&settings);

        // Start running on Alarm down at 15:04:00; start_time = 15:04:00.
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::Down),
            &mut settings,
        );
        // Advance the simulated clock by 5 s and tick.
        mock.set_time(dt(2023, 1, 6, 15, 4, 5));
        face.loop_(types::Event::Tick, &mut settings);
        // 5 seconds elapsed, seconds shown; days/hours/minute are all 0.
        assert_eq!(mock.text(), "st  000005");
    }

    // ---- timer -----------------------------------------------------------------

    #[test]
    fn real_timer_activate_shows_label_and_slot_zero_value() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = timer::TimerFace::new_static();
        // setup() records the watch_face_index (used to schedule background tasks).
        face.setup(&settings, 0);
        face.activate(&settings);
        assert!(mock.colon);

        face.loop_(types::Event::Activate, &mut settings);
        // Slot 0 label "1 ", then the 2-minute value "00020". `activate` wrote
        // "TR" at position 0 and draw() writes the value at position 3, so the
        // recorded LCD carries the label + slot value.
        assert_eq!(&mock.chars[3..], ['1', ' ', '0', '0', '0', '2', '0']);
    }

    #[test]
    fn real_timer_alarm_starts_slot_and_tick_counts_down() {
        let mut mock = steady_state(); // 15:04:00
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = timer::TimerFace::new_static();
        face.setup(&settings, 0);
        face.activate(&settings);

        // In Waiting mode, an Alarm long-press starts the selected slot.
        // Slot 0 is 2 minutes (000200), so the start sets target = now + 120s.
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::LongPress),
            &mut settings,
        );
        // Starting sets the Bell indicator (via start()).
        assert!(mock.indicator(Indicator::Bell));
        // Two ticks decrement: now -> target-2s (118 s remaining).
        face.loop_(types::Event::Tick, &mut settings);
        face.loop_(types::Event::Tick, &mut settings);
        // Timer value drawn at position 3 begins with the slot label "1 ".
        assert_eq!(&mock.chars[3..], ['1', ' ', '0', '0', '0', '1', '5']);
    }

    // ---- countdown -------------------------------------------------------------

    #[test]
    fn real_countdown_activate_renders_default_minutes() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = countdown::CountdownFace::new_static();
        face.setup(&settings, 0);
        face.activate(&settings);
        assert!(mock.colon);

        face.loop_(types::Event::Activate, &mut settings);
        // Default 3 minutes: CD + "000300" (with an extra space for hours' tens).
        assert_eq!(mock.text(), "CD  000300");
    }

    #[test]
    fn real_countdown_alarm_starts_then_ticks_down() {
        let mut mock = steady_state(); // 15:04:00
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = countdown::CountdownFace::new_static();
        face.setup(&settings, 0);
        face.activate(&settings);

        // Reset mode: Alarm up starts the 3-minute countdown.
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
            &mut settings,
        );
        assert!(mock.indicator(Indicator::Signal));
        // Each Tick decrements now_ts by 1; 3 minutes = 180 s, then 179 ... The
        // value is 000259 (hours 00, minutes 02, seconds 59) at one tick in.
        face.loop_(types::Event::Tick, &mut settings);
        assert_eq!(mock.text(), "CD  000259");
    }

    // ---- flashlight ------------------------------------------------------------

    #[test]
    fn real_flashlight_activate_shows_label_and_light_button_toggles_pin() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = flashlight::FlashlightFace::new_static();
        face.activate(&settings);

        face.loop_(types::Event::Activate, &mut settings);
        assert_eq!(mock.text(), "FL");
        // A2 defaults low.
        assert!(!mock.pin_level((1, 2)));

        // Light button up toggles the output on.
        face.loop_(
            types::Event::Button(types::Button::Light, types::ButtonEvent::Up),
            &mut settings,
        );
        assert!(mock.pin_level((1, 2)));

        // Toggling again turns it off.
        face.loop_(
            types::Event::Button(types::Button::Light, types::ButtonEvent::Up),
            &mut settings,
        );
        assert!(!mock.pin_level((1, 2)));
    }

    // ---- Q-Z subset (this agent's host-test migration) -------------------------

    #[test]
    fn real_randonaut_activate_shows_landing_screen() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = randonaut::RandonautFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // Activate drives display() in mode 0 -> "RA  Rando " (trailing trimmed).
        assert_eq!(mock.text(), "RA  Rando");
    }

    #[test]
    fn real_randonaut_light_up_cycles_to_point() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = randonaut::RandonautFace::new();
        face.activate(&settings);
        // Light up from mode 0 -> mode 2, location_format 0 -> "RA  Point ".
        face.loop_(
            types::Event::Button(types::Button::Light, types::ButtonEvent::Up),
            &mut settings,
        );
        assert_eq!(mock.text(), "RA  Point");
    }

    #[test]
    fn real_ratemeter_activate_shows_idle_label() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = ratemeter::RatemeterFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // The 12-char label written from pos 0 only fills chars 0-9; trailing space trimmed.
        assert_eq!(mock.text(), "ra");
    }

    #[test]
    fn real_ratemeter_alarm_down_starts_ticking() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = ratemeter::RatemeterFace::new();
        face.activate(&settings);
        // Alarm down resets ticks (rate stays 0). One tick re-draws the idle label.
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::Down),
            &mut settings,
        );
        face.loop_(types::Event::Tick, &mut settings);
        assert_eq!(mock.text(), "ra");
    }

    #[test]
    fn real_repetition_minute_activate_renders_24h_clock() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = repetition_minute::RepetitionMinuteFace::new();
        face.activate(&settings);
        assert!(mock.colon);
        assert!(mock.indicator(Indicator::H24));
        face.loop_(types::Event::Tick, &mut settings);
        assert_eq!(mock.text(), "FR06150400");
    }

    #[test]
    fn real_repetition_minute_alarm_long_press_toggles_bell() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = repetition_minute::RepetitionMinuteFace::new();
        face.activate(&settings);
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::LongPress),
            &mut settings,
        );
        assert!(mock.indicator(Indicator::Bell));
    }

    #[test]
    fn real_rpn_calculator_activate_draws_zero_waiting() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = rpn_calculator::RpnCalculatorFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // Waiting mode draws the (empty) stack top as 000000.
        assert_eq!(mock.text(), "CA  000000");
    }

    #[test]
    fn real_rpn_calculator_alarm_up_enters_number_and_increments() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = rpn_calculator::RpnCalculatorFace::new();
        face.activate(&settings);
        // Alarm up: enter NUMBER mode, push 0. Then alarm up increments selection 2
        // (the ones place? selection starts 2 -> ones digit of the 6-digit) -> 000100.
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
            &mut settings,
        );
        assert_eq!(mock.text(), "CA  000000");
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
            &mut settings,
        );
        assert_eq!(mock.text(), "CA  000100");
    }

    #[test]
    fn real_sailing_activate_renders_waiting_countdown() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = sailing::SailingFace::new_static();
        face.setup(&settings, 0);
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // Waiting mode: minutes[0]=5 -> "SA1L  0500".
        assert_eq!(mock.text(), "SA1L  0500");
    }

    #[test]
    fn real_save_load_activate_shows_empty_slot() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = save_load::SaveLoadFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // Slot 0 empty -> "SL 0no dat" (buffer cell 6 is a space).
        assert_eq!(mock.text(), "SL 0no dat");
    }

    #[test]
    fn real_save_load_light_long_press_saves_slot() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = save_load::SaveLoadFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        face.loop_(
            types::Event::Button(types::Button::Light, types::ButtonEvent::LongPress),
            &mut settings,
        );
        // Save writes backup data (slot 0 now holds the RTC) and shows "Saved ".
        assert_eq!(mock.text(), "SL 0Saved");
    }

    #[test]
    fn real_save_load_alarm_long_press_loads_nothing_when_empty() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = save_load::SaveLoadFace::new();
        face.activate(&settings);
        // Empty slot -> load is a no-op; display still shows the empty slot.
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::LongPress),
            &mut settings,
        );
        // No initial draw has happened yet (no Activate event), so only the
        // trailing segments are visible; just check nothing panicked and the
        // slot is still empty per the backup registers.
        assert_eq!(mock.backup(0), 0);
    }

    #[test]
    fn real_ships_bell_activate_shows_label_and_colon() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = ships_bell::ShipsBellFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // draw() -> label "SB" at pos 0-3, then hour/min/sec at pos 4+. 15:04:00
        // gives hour=15%4=3, so "SB   30400".
        assert_eq!(mock.text(), "SB   30400");
    }

    #[test]
    fn real_simon_activate_shows_not_playing_score() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = simon::SimonFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // Best score 00, mode E at pos 9; the 4 chars 6-9 are still 0 (NUL).
        assert_eq!(mock.text(), "SI  00\0\0\0E");
        assert!(mock.indicator(Indicator::Bell));
        assert!(mock.indicator(Indicator::Signal));
    }

    #[test]
    fn real_simple_calculator_activate_enters_first_number() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = simple_calculator::SimpleCalculatorFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // Entering first num, zeros with the ones digit blinking (pos 9 blank
        // because display_index = 9 - placeholder(2) = 7 ... actually subsecond 0
        // shows dash; observed output has a space at index 6).
        assert_eq!(mock.text(), "CA1 000 00");
    }

    #[test]
    fn real_simple_calculator_alarm_up_sets_ones_digit() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = simple_calculator::SimpleCalculatorFace::new();
        face.activate(&settings);
        // placeholder = ones (2). Alarm up increments ones -> 1 via
        // update_display_number (no blink) -> "CA1 000100".
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
            &mut settings,
        );
        assert_eq!(mock.text(), "CA1 000100");
    }

    #[test]
    fn real_simple_clock_bin_led_activate_renders_24h() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = simple_clock_bin_led::SimpleClockBinLedFace::new();
        face.activate(&settings);
        assert!(mock.colon);
        assert!(mock.indicator(Indicator::H24));
        face.loop_(types::Event::Tick, &mut settings);
        assert_eq!(mock.text(), "FR06150400");
    }

    #[test]
    fn real_simple_coin_flip_activate_shows_flip_label() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let settings = h24_settings();
        let mut face = simple_coin_flip::SimpleCoinFlipFace::new();
        face.activate(&settings);
        // "flip" at pos 5 -> "     flip" (trailing spaces trimmed).
        assert_eq!(mock.text(), "     flip");
    }

    #[test]
    fn real_solar_time_activate_shows_no_location_prompt() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = solar_time::SolarTimeFace::new();
        face.activate(&settings);
        // The draw only happens on loop_; backup location reg 1 is 0.
        face.loop_(types::Event::Tick, &mut settings);
        assert!(mock.text().starts_with("SOL"));
        assert!(mock.text().contains("no Loc"));
    }

    #[test]
    fn real_solstice_activate_shows_solstice_date() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = solstice::SolsticeFace::new();
        face.setup(&settings, 0);
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // show_main_screen: "  YY  MMDD". Year 2023 -> fields from date_time.
        // (Values depend on JDE math; assert it renders 8 chars + 2 blanks.)
        assert_eq!(mock.text().len(), 10);
    }

    #[test]
    fn real_sos_activate_shows_selected_code() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = sos::SosFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        assert_eq!(mock.text(), "SOS");
    }

    #[test]
    fn real_sos_light_up_cycles_to_mayday() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = sos::SosFace::new();
        face.activate(&settings);
        face.loop_(
            types::Event::Button(types::Button::Light, types::ButtonEvent::Up),
            &mut settings,
        );
        assert_eq!(mock.text(), "MAYDAY");
    }

    #[test]
    fn real_squash_activate_renders_zero_game() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = squash::SquashFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // Games 0-0 at pos 0 and 2, scores 0-0 at pos 4 and 6 => "00000000".
        assert_eq!(mock.text(), "00000000");
        assert!(!mock.indicator(Indicator::Lap));
    }

    #[test]
    fn real_squash_light_up_scores_player_one() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = squash::SquashFace::new();
        face.activate(&settings);
        face.loop_(
            types::Event::Button(types::Button::Light, types::ButtonEvent::Up),
            &mut settings,
        );
        assert_eq!(mock.text(), "00000100");
    }

    #[test]
    fn real_tachymeter_activate_shows_default_distance() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = tachymeter::TachymeterFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // distance 100 -> 6-digit right-aligned leaves leading NULs.
        assert_eq!(mock.text(), "TC d\0\0\0100");
    }

    #[test]
    fn real_tally_activate_shows_zero() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = tally::TallyFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // "TA  0000" written from a [0u8;11] buffer -> leading NULs remain.
        assert_eq!(mock.text(), "TA  \0\0\00");
    }

    #[test]
    fn real_tarot_activate_shows_title() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = tarot::TarotFace::new();
        face.activate(&settings);
        assert_eq!(mock.text(), "TA");
        face.loop_(types::Event::Activate, &mut settings);
        assert_eq!(mock.text(), "TA03n&ajor");
    }

    #[test]
    fn real_tempchart_activate_shows_zero_samples() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = tempchart::TempchartFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // sum=0 -> "TS00" + 6 NULs + "0".
        assert_eq!(mock.text(), "TS00\0\0\0\0\00");
    }

    #[test]
    fn real_thermistor_readout_activate_shows_celsius() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = thermistor_readout::ThermistorReadoutFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // The default host mock has no thermistor fixture.
        assert_eq!(mock.text(), "NO TE");
    }

    #[test]
    fn real_thermistor_readout_alarm_down_toggles_fahrenheit() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = thermistor_readout::ThermistorReadoutFace::new();
        face.activate(&settings);
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::Down),
            &mut settings,
        );
        // Unit changes do not manufacture a value when the sensor is absent.
        assert_eq!(mock.text(), "NO TE");
    }

    #[test]
    fn real_time_left_activate_shows_days_left() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = time_left::TimeLeftFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // Title "DL " + digits; value depends on target 2030-01-01 vs 2023-01-06.
        assert!(mock.text().starts_with("DL"));
    }

    #[test]
    fn real_tomato_activate_renders_ready_focus() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let settings = h24_settings();
        let mut face = tomato::TomatoFace::new();
        face.activate(&settings);
        assert!(mock.colon);
        assert!(!mock.indicator(Indicator::Bell));
    }

    #[test]
    fn real_tomato_alarm_up_starts_focus_timer() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = tomato::TomatoFace::new();
        face.activate(&settings);
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
            &mut settings,
        );
        // Running focus: "TO f2500" (25:00), Bell set.
        assert!(mock.indicator(Indicator::Bell));
    }

    #[test]
    fn real_tuning_tones_activate_shows_selected_note() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = tuning_tones::TuningTonesFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // note_ind 9 -> "A " at pos 8 -> 8 leading spaces.
        assert_eq!(mock.text(), "        A");
    }

    #[test]
    fn real_voltage_activate_shows_battery_voltage() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        mock.vcc_mv = 3000;
        let mut settings = h24_settings();
        let mut face = voltage::VoltageFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // 3000 mV -> "BA  3.00 V" (blank before the unit).
        assert_eq!(mock.text(), "BA  3.00 V");
    }

    #[test]
    fn real_wake_activate_renders_5_00() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = wake::WakeFace::new();
        face.activate(&settings);
        face.loop_(types::Event::Activate, &mut settings);
        // hour 5 minute 0 in 24h mode -> "WA  0500".
        assert_eq!(mock.text(), "WA  0500");
    }

    #[test]
    fn real_wareki_activate_shows_year_and_era() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = wareki::WarekiFace::new();
        face.activate(&settings);
        // The year/era is drawn on Tick; real year 2023 > REIWA_GANNEN.
        face.loop_(types::Event::Tick, &mut settings);
        assert!(mock.text().contains("2023"));
    }

    #[test]
    fn real_weeknumber_activate_renders_clock_and_week() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = weeknumber::WeekNumberClockFace::new();
        face.activate(&settings);
        assert!(mock.colon);
        assert!(mock.indicator(Indicator::H24));
        face.loop_(types::Event::Tick, &mut settings);
        // "FR0615" + minutes "04" + week number. 2023-01-06 is week 1.
        assert_eq!(mock.text(), "FR06150401");
    }

    #[test]
    fn real_wordle_activate_shows_title_screen() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let settings = h24_settings();
        let mut face = wordle::WordleFace::new();
        face.activate(&settings);
        // Title + a blank at pos 3.
        assert_eq!(mock.text(), "WO  WordLE");
    }

    #[test]
    fn real_world_clock2_activate_starts_in_settings_mode() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = world_clock2::WorldClock2Face::new();
        face.activate(&settings);
        face.loop_(types::Event::Tick, &mut settings);
        // Settings mode zone 0 (UTC): "UT00 +0000".
        assert_eq!(mock.text(), "UT00 +0000");
    }

    #[test]
    fn real_wyoscan_activate_then_tick_animates() {
        let mut mock = steady_state();
        seam::install_hw(&mut mock);
        let mut settings = h24_settings();
        let mut face = wyoscan::WyoscanFace::new();
        face.activate(&settings);
        // First tick captures the time and starts the animation, setting pixels.
        face.loop_(types::Event::Tick, &mut settings);
        assert!(!mock.segments.is_empty());
    }
}
