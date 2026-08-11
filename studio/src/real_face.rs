//! Bridge from the Studio Simulator to the REAL firmware faces through the
//! firmware host `Hw` seam.
//!
//! The firmware crate (`sensor-watch`) has a host `[lib]` (the `hostmock`
//! feature): on the host, `sensor_watch::movement::simple_clock` is the *real*
//! face source pulled in verbatim (see `src/host/movement/`), and the HAL free
//! functions (`slcd::*`, `rtc::get_date_time`, ...) forward to whatever `Hw`
//! backend is installed via the global seam. The Studio app drives the real
//! face's `WatchFace::activate`/`loop_` against a reusable `MockHw` (from
//! `sensor_watch_core::mock_hw`), so the rendered digits and indicators come
//! from the same code the firmware runs instead of the hand-written `face_sim`
//! reimplementation (`face_sim` remains the fallback for faces not yet migrated
//! through the seam).
//!
//! The host-migrated faces wired up here are the stock Casio set plus the other
//! faces whose host harness has landed in the firmware seam: `SIMPLE_CLOCK`,
//! `ALARM`, `COUNTER`, `WORLD_CLOCK`, `STOPWATCH`, `TIMER`, `COUNTDOWN`, and
//! `FLASHLIGHT`. New faces are added by extending [`new_face`] once their host
//! harness lands in the firmware seam.
//!
//! # Feature gating
//!
//! The bridge lives behind the Studio `real-faces` feature (see `Cargo.toml`),
//! because pulling the firmware *host lib* into the app as a dependency currently
//! requires the firmware seam to compile as a host lib (the firmware's `watch`
//! tree is mid-migration and not yet a clean host dependency). With the feature
//! **on**, this module drives the real faces through the seam; with it **off**
//! (the default), it exposes a fallback `RealFace` that is always `None`, so the
//! Simulator transparently keeps using `face_sim` and the app still compiles and
//! passes its tests. No Studio code needs changing; the main loop just sees
//! "seam unavailable" and falls back.

#[cfg(feature = "real-faces")]
use sensor_watch::movement::{
    alarm, astronomy, close_enough, countdown, counter, day_night_percentage, day_one, deadline,
    decimal_time, flashlight, french_revolutionary, frequency_correction, hello_there, interval,
    invaders, ish, ke_decimal_time, kitchen_conversions, lander, lightmeter, lis2dw_logging,
    mars_time, menstrual_cycle, metronome, minimal_clock, minmax,
    minute_repeater_decimal, moon_phase, morsecalc, nanosec, orrery, periodic, ping,
    planetary_hours, planetary_time, preferences, probability, pulsometer, randonaut, ratemeter,
    repetition_minute, rpn_calculator, rpn_calculator_alt, sailing, save_load, set_time,
    set_time_hackwatch, ships_bell, simon, simple_calculator, simple_clock, simple_clock_bin_led,
    simple_coin_flip, solar_time, solstice, sos, squash, stopwatch, sunrise_sunset, tachymeter,
    tally, tarot, tempchart, thermistor_logging, thermistor_readout, thermistor_testing, tide,
    time_left, timer, tomato, toss_up, totp, totp_lfs, tuning_tones, types, voltage, wake, wareki,
    weeknumber, wordle, world_clock, world_clock2, wyoscan,
};
#[cfg(feature = "real-faces")]
use sensor_watch_core::datetime::DateTime;
#[cfg(feature = "real-faces")]
use sensor_watch_core::mock_hw::{Hw, MockHw};

#[cfg(feature = "real-faces")]
use std::sync::{Mutex, MutexGuard};

/// Serializes access to the firmware's single-slot global `Hw` seam.
///
/// The seam (`sensor_watch::watch::seam`) dispatches to a single global
/// `MockHw` at a time and is explicitly single-threaded. Studio tests drive
/// several `RealFace`s in parallel, so a face must hold this lock for its whole
/// lifetime to guarantee that the mock it installed is the one the seam still
/// points at when it writes. Dropping the guard (on `RealFace` drop) opens the
/// slot for the next face.
#[cfg(feature = "real-faces")]
static SEAM_LOCK: Mutex<()> = Mutex::new(());

/// A snapshot of what a real face wrote to the mock LCD, in Studio terms.
#[derive(Clone, Copy, Debug, Default)]
pub struct RealFaceSnapshot {
    /// The 10 LCD characters captured on the mock, position 0..10.
    pub chars: [char; 10],
    /// Whether the colon segment is on.
    pub colon: bool,
    /// The indicator flags, indexed+ordered like the LCD label row used by
    /// Studio's SVG mapping: signal, bell, pm, h24, lap.
    pub signal: bool,
    pub bell: bool,
    pub pm: bool,
    pub h24: bool,
    pub lap: bool,
}

// ---------------------------------------------------------------------------
// Real implementation (feature `real-faces` on).
// ---------------------------------------------------------------------------

/// A running real face. Holds the per-face state plus the mock it records onto.
#[cfg(feature = "real-faces")]
pub struct RealFace {
    /// The firmware `WatchFace`'s state.
    face: Box<dyn RealFaceTrait>,
    /// The name of the face this instance runs (used by the app to detect face
    /// switches).
    face_name: &'static str,
    /// The mock hardware the face draws onto. Boxed so its heap address is
    /// stable: `install_hw` stores a raw pointer to it in the global seam, and a
    /// move would invalidate that pointer (the seam would point at dead stack
    /// memory).
    mock: Box<MockHw>,
    /// The settings the face mutates (the firmware's movement settings).
    settings: types::Settings,
    /// The display snapshot of the last render (derived from `mock`).
    snapshot: RealFaceSnapshot,
    /// Whether the face has received its initial activation.
    activated: bool,
    /// Holds the global seam lock so this face's mock stays installed for its
    /// whole lifetime (see `SEAM_LOCK`).
    _seam_guard: MutexGuard<'static, ()>,
}

/// Object-safe seam over any migrated firmware `WatchFace`, so the Studio caller
/// can `activate`/`loop_` without knowing the concrete face type.
#[cfg(feature = "real-faces")]
trait RealFaceTrait {
    fn activate(&mut self, settings: &types::Settings);
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings);
}

// `SimpleClockFace`'s `WatchFace` impl supplies these via the REAL trait;
// forward the real types through so the untouched firmware face binds to the
// object-safe wrapper above.
#[cfg(feature = "real-faces")]
impl RealFaceTrait for simple_clock::SimpleClockFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

// The other host-migrated faces wired into [`new_face`] forward the same way.
// Each is the REAL firmware face; the `WatchFace` impl is the untouched trait.
// Keep this bridge local to the Studio adapter so the real firmware sources stay
// verbatim.
#[cfg(feature = "real-faces")]
macro_rules! impl_real_face_trait {
    ($face:path) => {
        impl RealFaceTrait for $face {
            fn activate(&mut self, settings: &types::Settings) {
                types::WatchFace::activate(self, settings);
            }
            fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
                types::WatchFace::loop_(self, event, settings);
            }
        }
    };
}

#[cfg(feature = "real-faces")]
impl_real_face_trait!(astronomy::AstronomyFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(close_enough::CloseEnoughClockFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(day_night_percentage::DayNightPercentageFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(day_one::DayOneFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(deadline::DeadlineFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(decimal_time::DecimalTimeFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(french_revolutionary::FrenchRevolutionaryFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(frequency_correction::FrequencyCorrectionFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(hello_there::HelloThereFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(ke_decimal_time::KeDecimalTimeFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(interval::IntervalFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(invaders::InvadersFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(ish::IshFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(kitchen_conversions::KitchenConversionsFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(lander::LanderFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(lightmeter::LightmeterFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(lis2dw_logging::Lis2dwLoggingFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(mars_time::MarsTimeFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(menstrual_cycle::MenstrualCycleFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(metronome::MetronomeFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(minimal_clock::MinimalClockFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(minmax::MinmaxFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(minute_repeater_decimal::MinuteRepeaterDecimalFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(moon_phase::MoonPhaseFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(morsecalc::MorsecalcFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(nanosec::NanosecFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(orrery::OrreryFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(periodic::PeriodicFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(ping::PingFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(planetary_hours::PlanetaryHoursFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(planetary_time::PlanetaryTimeFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(preferences::PreferencesFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(probability::ProbabilityFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(pulsometer::PulsometerFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(randonaut::RandonautFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(ratemeter::RatemeterFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(repetition_minute::RepetitionMinuteFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(rpn_calculator::RpnCalculatorFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(rpn_calculator_alt::RpnCalculatorAltFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(sailing::SailingFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(save_load::SaveLoadFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(set_time::SetTimeFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(set_time_hackwatch::SetTimeHackwatchFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(ships_bell::ShipsBellFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(simon::SimonFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(simple_calculator::SimpleCalculatorFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(simple_clock_bin_led::SimpleClockBinLedFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(simple_coin_flip::SimpleCoinFlipFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(solar_time::SolarTimeFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(solstice::SolsticeFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(sos::SosFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(squash::SquashFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(sunrise_sunset::SunriseSunsetFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(tachymeter::TachymeterFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(tally::TallyFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(tarot::TarotFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(tempchart::TempchartFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(thermistor_logging::ThermistorLoggingFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(thermistor_readout::ThermistorReadoutFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(thermistor_testing::ThermistorTestingFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(tide::TideFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(time_left::TimeLeftFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(tomato::TomatoFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(toss_up::TossUpFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(totp::TotpFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(totp_lfs::TotpFaceLfs);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(tuning_tones::TuningTonesFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(voltage::VoltageFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(wake::WakeFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(wareki::WarekiFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(weeknumber::WeekNumberClockFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(wordle::WordleFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(world_clock2::WorldClock2Face);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(wyoscan::WyoscanFace);
#[cfg(feature = "real-faces")]
impl RealFaceTrait for alarm::AlarmFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFaceTrait for counter::CounterFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFaceTrait for world_clock::WorldClockFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFaceTrait for stopwatch::StopwatchFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFaceTrait for timer::TimerFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFaceTrait for countdown::CountdownFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFaceTrait for flashlight::FlashlightFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
}

#[cfg(feature = "real-faces")]
impl RealFace {
    /// Creates a running real face for `face_name`, if a real face of that name
    /// has been migrated into the firmware seam. Returns `None` otherwise.
    pub fn new(face_name: &str) -> Option<RealFace> {
        let face = new_face(face_name)?;
        let _seam_guard = SEAM_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut mock = Box::new(MockHw::new());
        mock.vcc_mv = 3000; // healthy battery
                            // Install the mock into the host `Hw` seam so the real face's HAL calls
                            // (`slcd::*`, `rtc::get_date_time`, ...) forward to this mock instead of
                            // panicking with "no Hw installed". The `Drop` impl clears it when this
                            // face is dropped so the global slot doesn't leak between faces.
        sensor_watch::watch::seam::install_hw(&mut *mock);
        let settings = types::Settings::default();
        Some(RealFace {
            face,
            mock,
            settings,
            snapshot: RealFaceSnapshot::default(),
            activated: false,
            face_name: new_face_name(face_name),
            _seam_guard,
        })
    }

    /// Sets the mock's RTC clock to the given wall-clock date/time.
    pub fn set_time(
        &mut self,
        year: u32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
    ) -> bool {
        let reference_year = sensor_watch_core::datetime::WATCH_RTC_REFERENCE_YEAR as u32;
        let max_year = reference_year + 63;
        let days_in_month = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
            2 => 28,
            _ => 0,
        };
        if !(reference_year..=max_year).contains(&year)
            || !(1..=12).contains(&month)
            || !(1..=days_in_month).contains(&day)
            || hour >= 24
            || minute >= 60
            || second >= 60
        {
            return false;
        }

        let next = DateTime {
            second: second as u8,
            minute: minute as u8,
            hour: hour as u8,
            day: day as u8,
            month: month as u8,
            year: (year - reference_year) as u8,
        };
        self.mock.now = next;
        // Refresh an already-active face without re-running activate. This keeps
        // the display's AM/PM and date fields synchronized after a time edit
        // while preserving stateful face navigation.
        if self.activated {
            self.face.loop_(types::Event::Tick, &mut self.settings);
            self.snapshot_from_mock();
        }
        true
    }

    /// Ready the face the way the firmware does at power-up: tell the face it's
    /// entering the foreground, then let it draw the current time. The clock
    /// mode (12/24) is mirrored to the firmware settings so both render paths
    /// agree with the watch settings in the app.
    pub fn activate(&mut self, time_mode_24: bool) {
        self.settings.set_clock_mode_24h(time_mode_24);
        // Some firmware faces only set the positive indicator during activate,
        // so clear both mode labels first to prevent a stale PM/H24 label when
        // switching between 12-hour and 24-hour display modes.
        self.mock
            .clear_indicator(sensor_watch_core::mock_hw::Indicator::Pm);
        self.mock
            .clear_indicator(sensor_watch_core::mock_hw::Indicator::H24);
        self.face.activate(&self.settings);
        self.face.loop_(types::Event::Tick, &mut self.settings);
        self.activated = true;
        self.snapshot_from_mock();
    }

    /// Delivers one firmware RTC tick and refreshes the captured display.
    ///
    /// Callers must only invoke this at a simulated-second boundary; ordinary
    /// frame redraws and RTC edits belong in [`set_time`].
    pub fn tick(&mut self) {
        self.face.loop_(types::Event::Tick, &mut self.settings);
        self.snapshot_from_mock();
    }

    /// Drives a button press into the face. `Light` is the L button; `Alarm` is
    /// the A button (the C button cycles faces in the app rather than reaching
    /// the face).
    pub fn press(&mut self, light: bool, alarm: bool) {
        if light {
            self.face.loop_(
                types::Event::Button(types::Button::Light, types::ButtonEvent::Up),
                &mut self.settings,
            );
        }
        if alarm {
            self.face.loop_(
                types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
                &mut self.settings,
            );
        }
        self.snapshot_from_mock();
    }

    /// Whether the firmware face has received its initial activation.
    pub fn is_activated(&self) -> bool {
        self.activated
    }

    /// The current display snapshot (LCD chars + indicators).
    pub fn snapshot(&self) -> RealFaceSnapshot {
        self.snapshot
    }

    /// The name of the face this instance runs (used by the app to detect face
    /// switches).
    pub fn face_name(&self) -> &str {
        self.face_name
    }

    fn snapshot_from_mock(&mut self) {
        let m = &self.mock;
        self.snapshot = RealFaceSnapshot {
            chars: m.chars,
            colon: m.colon,
            signal: m.indicator(sensor_watch_core::mock_hw::Indicator::Signal),
            bell: m.indicator(sensor_watch_core::mock_hw::Indicator::Bell),
            pm: m.indicator(sensor_watch_core::mock_hw::Indicator::Pm),
            h24: m.indicator(sensor_watch_core::mock_hw::Indicator::H24),
            lap: m.indicator(sensor_watch_core::mock_hw::Indicator::Lap),
        };
    }
}

/// Clears the mock from the host `Hw` seam so the global slot doesn't leak
/// between faces (e.g. when the Studio app swaps the simulated face).
#[cfg(feature = "real-faces")]
impl Drop for RealFace {
    fn drop(&mut self) {
        sensor_watch::watch::seam::clear_hw();
    }
}

/// Returns a heap-allocated real face for `face_name`, if a face of that name is
/// migrated through the firmware seam. Matrix the name against the firmware's
/// upper-cased face-const name so presets ("SIMPLE_CLOCK", "simple_clock", ...)
/// resolve.
///
/// Faces not yet migrated through the seam (or whose real type needs extra setup
/// beyond a plain constructor) are intentionally absent, so the app falls back to
/// `face_sim` for them.
#[cfg(feature = "real-faces")]
fn new_face(face_name: &str) -> Option<Box<dyn RealFaceTrait>> {
    let upper = face_name.to_ascii_uppercase();
    match upper.as_str() {
        "SIMPLE_CLOCK" => Some(Box::new(simple_clock::SimpleClockFace::new())),
        "ALARM" => Some(Box::new(alarm::AlarmFace::new_static())),
        "COUNTER" => Some(Box::new(counter::CounterFace::new_static())),
        "WORLD_CLOCK" => Some(Box::new(world_clock::WorldClockFace::new_static())),
        "STOPWATCH" => Some(Box::new(stopwatch::StopwatchFace::new())),
        "TIMER" => Some(Box::new(timer::TimerFace::new())),
        "COUNTDOWN" => Some(Box::new(countdown::CountdownFace::new_static())),
        "FLASHLIGHT" => Some(Box::new(flashlight::FlashlightFace::new_static())),
        "ASTRONOMY" => Some(Box::new(astronomy::AstronomyFace::new_static())),
        "CLOSE_ENOUGH" => Some(Box::new(close_enough::CloseEnoughClockFace::new_static())),
        "DAY_NIGHT_PERCENTAGE" => Some(Box::new(day_night_percentage::DayNightPercentageFace::new_static())),
        "DAY_ONE" => Some(Box::new(day_one::DayOneFace::new_static())),
        "DEADLINE" => Some(Box::new(deadline::DeadlineFace::new_static())),
        "DECIMAL_TIME" => Some(Box::new(decimal_time::DecimalTimeFace::new_static())),
        "FRENCH_REVOLUTIONARY" => Some(Box::new(french_revolutionary::FrenchRevolutionaryFace::new_static())),
        "FREQUENCY_CORRECTION" => Some(Box::new(frequency_correction::FrequencyCorrectionFace::new_static())),
        "HELLO_THERE" => Some(Box::new(hello_there::HelloThereFace::new_static())),
        "KE_DECIMAL_TIME" => Some(Box::new(ke_decimal_time::KeDecimalTimeFace::new_static())),
        "INTERVAL" => Some(Box::new(interval::IntervalFace::new_static())),
        "INVADERS" => Some(Box::new(invaders::InvadersFace::new_static())),
        "ISH" => Some(Box::new(ish::IshFace::new_static())),
        "KITCHEN_CONVERSIONS" => Some(Box::new(
            kitchen_conversions::KitchenConversionsFace::new_static(),
        )),
        "LANDER" => Some(Box::new(lander::LanderFace::new_static())),
        "LIGHTMETER" => Some(Box::new(lightmeter::LightmeterFace::new_static())),
        "LIS2DW_LOGGING" => Some(Box::new(lis2dw_logging::Lis2dwLoggingFace::new_static())),
        "MARS_TIME" => Some(Box::new(mars_time::MarsTimeFace::new_static())),
        "MENSTRUAL_CYCLE" => Some(Box::new(menstrual_cycle::MenstrualCycleFace::new_static())),
        "METRONOME" => Some(Box::new(metronome::MetronomeFace::new_static())),
        "MINIMAL_CLOCK" => Some(Box::new(minimal_clock::MinimalClockFace::new_static())),
        "MINMAX" => Some(Box::new(minmax::MinmaxFace::new_static())),
        "MINUTE_REPEATER_DECIMAL" => Some(Box::new(
            minute_repeater_decimal::MinuteRepeaterDecimalFace::new_static(),
        )),
        "MOON_PHASE" => Some(Box::new(moon_phase::MoonPhaseFace::new_static())),
        "MORSECALC" => Some(Box::new(morsecalc::MorsecalcFace::new_static())),
        "NANOSEC" => Some(Box::new(nanosec::NanosecFace::new_static())),
        "ORRERY" => Some(Box::new(orrery::OrreryFace::new_static())),
        "PERIODIC" => Some(Box::new(periodic::PeriodicFace::new_static())),
        "PING" => Some(Box::new(ping::PingFace::new_static())),
        "PLANETARY_HOURS" => Some(Box::new(planetary_hours::PlanetaryHoursFace::new_static())),
        "PLANETARY_TIME" => Some(Box::new(planetary_time::PlanetaryTimeFace::new_static())),
        "PREFERENCES" => Some(Box::new(preferences::PreferencesFace::new_static())),
        "PROBABILITY" => Some(Box::new(probability::ProbabilityFace::new_static())),
        "PULSOMETER" => Some(Box::new(pulsometer::PulsometerFace::new_static())),
        "RANDONAUT" => Some(Box::new(randonaut::RandonautFace::new_static())),
        "RATEMETER" => Some(Box::new(ratemeter::RatemeterFace::new_static())),
        "REPETITION_MINUTE" => Some(Box::new(
            repetition_minute::RepetitionMinuteFace::new_static(),
        )),
        "RPN_CALCULATOR" => Some(Box::new(rpn_calculator::RpnCalculatorFace::new_static())),
        "RPN_CALCULATOR_ALT" => Some(Box::new(
            rpn_calculator_alt::RpnCalculatorAltFace::new_static(),
        )),
        "SAILING" => Some(Box::new(sailing::SailingFace::new_static())),
        "SAVE_LOAD" => Some(Box::new(save_load::SaveLoadFace::new_static())),
        "SET_TIME" => Some(Box::new(set_time::SetTimeFace::new_static())),
        "SET_TIME_HACKWATCH" => Some(Box::new(
            set_time_hackwatch::SetTimeHackwatchFace::new_static(),
        )),
        "SHIPS_BELL" => Some(Box::new(ships_bell::ShipsBellFace::new_static())),
        "SIMON" => Some(Box::new(simon::SimonFace::new_static())),
        "SIMPLE_CALCULATOR" => Some(Box::new(
            simple_calculator::SimpleCalculatorFace::new_static(),
        )),
        "SIMPLE_CLOCK_BIN_LED" => Some(Box::new(
            simple_clock_bin_led::SimpleClockBinLedFace::new_static(),
        )),
        "SIMPLE_COIN_FLIP" => Some(Box::new(simple_coin_flip::SimpleCoinFlipFace::new_static())),
        "SOLAR_TIME" => Some(Box::new(solar_time::SolarTimeFace::new_static())),
        "SOLSTICE" => Some(Box::new(solstice::SolsticeFace::new_static())),
        "SOS" => Some(Box::new(sos::SosFace::new_static())),
        "SQUASH" => Some(Box::new(squash::SquashFace::new_static())),
        "SUNRISE_SUNSET" => Some(Box::new(sunrise_sunset::SunriseSunsetFace::new_static())),
        "TACHYMETER" => Some(Box::new(tachymeter::TachymeterFace::new_static())),
        "TALLY" => Some(Box::new(tally::TallyFace::new_static())),
        "TAROT" => Some(Box::new(tarot::TarotFace::new_static())),
        "TEMPCHART" => Some(Box::new(tempchart::TempchartFace::new_static())),
        "THERMISTOR_LOGGING" => Some(Box::new(
            thermistor_logging::ThermistorLoggingFace::new_static(),
        )),
        "THERMISTOR_READOUT" => Some(Box::new(
            thermistor_readout::ThermistorReadoutFace::new_static(),
        )),
        "THERMISTOR_TESTING" => Some(Box::new(
            thermistor_testing::ThermistorTestingFace::new_static(),
        )),
        "TIDE" => Some(Box::new(tide::TideFace::new_static())),
        "TIME_LEFT" => Some(Box::new(time_left::TimeLeftFace::new_static())),
        "TOMATO" => Some(Box::new(tomato::TomatoFace::new_static())),
        "TOSS_UP" => Some(Box::new(toss_up::TossUpFace::new_static())),
        "TOTP" => Some(Box::new(totp::TotpFace::new_static())),
        "TOTP_LFS" => Some(Box::new(totp_lfs::TotpFaceLfs::new_static())),
        "TUNING_TONES" => Some(Box::new(tuning_tones::TuningTonesFace::new_static())),
        "VOLTAGE" => Some(Box::new(voltage::VoltageFace::new_static())),
        "WAKE" => Some(Box::new(wake::WakeFace::new_static())),
        "WAREKI" => Some(Box::new(wareki::WarekiFace::new_static())),
        "WEEKNUMBER" => Some(Box::new(weeknumber::WeekNumberClockFace::new_static())),
        "WORDLE" => Some(Box::new(wordle::WordleFace::new_static())),
        "WORLD_CLOCK2" => Some(Box::new(world_clock2::WorldClock2Face::new_static())),
        "WYOSCAN" => Some(Box::new(wyoscan::WyoscanFace::new_static())),
        _ => None,
    }
}

/// The canonical upper-cased name of the face `face_name` resolves to, mirroring
/// [`new_face`]. Used to detect face switches in the app.
#[cfg(feature = "real-faces")]
fn new_face_name(face_name: &str) -> &'static str {
    let upper = face_name.to_ascii_uppercase();
    match upper.as_str() {
        "SIMPLE_CLOCK" => "SIMPLE_CLOCK",
        "ALARM" => "ALARM",
        "COUNTER" => "COUNTER",
        "WORLD_CLOCK" => "WORLD_CLOCK",
        "STOPWATCH" => "STOPWATCH",
        "TIMER" => "TIMER",
        "COUNTDOWN" => "COUNTDOWN",
        "FLASHLIGHT" => "FLASHLIGHT",
        "ASTRONOMY" => "ASTRONOMY",
        "CLOSE_ENOUGH" => "CLOSE_ENOUGH",
        "DAY_NIGHT_PERCENTAGE" => "DAY_NIGHT_PERCENTAGE",
        "DAY_ONE" => "DAY_ONE",
        "DEADLINE" => "DEADLINE",
        "DECIMAL_TIME" => "DECIMAL_TIME",
        "FRENCH_REVOLUTIONARY" => "FRENCH_REVOLUTIONARY",
        "FREQUENCY_CORRECTION" => "FREQUENCY_CORRECTION",
        "HELLO_THERE" => "HELLO_THERE",
        "KE_DECIMAL_TIME" => "KE_DECIMAL_TIME",
        "INTERVAL" => "INTERVAL",
        "INVADERS" => "INVADERS",
        "ISH" => "ISH",
        "KITCHEN_CONVERSIONS" => "KITCHEN_CONVERSIONS",
        "LANDER" => "LANDER",
        "LIGHTMETER" => "LIGHTMETER",
        "LIS2DW_LOGGING" => "LIS2DW_LOGGING",
        "MARS_TIME" => "MARS_TIME",
        "MENSTRUAL_CYCLE" => "MENSTRUAL_CYCLE",
        "METRONOME" => "METRONOME",
        "MINIMAL_CLOCK" => "MINIMAL_CLOCK",
        "MINMAX" => "MINMAX",
        "MINUTE_REPEATER_DECIMAL" => "MINUTE_REPEATER_DECIMAL",
        "MOON_PHASE" => "MOON_PHASE",
        "MORSECALC" => "MORSECALC",
        "NANOSEC" => "NANOSEC",
        "ORRERY" => "ORRERY",
        "PERIODIC" => "PERIODIC",
        "PING" => "PING",
        "PLANETARY_HOURS" => "PLANETARY_HOURS",
        "PLANETARY_TIME" => "PLANETARY_TIME",
        "PREFERENCES" => "PREFERENCES",
        "PROBABILITY" => "PROBABILITY",
        "PULSOMETER" => "PULSOMETER",
        "RANDONAUT" => "RANDONAUT",
        "RATEMETER" => "RATEMETER",
        "REPETITION_MINUTE" => "REPETITION_MINUTE",
        "RPN_CALCULATOR" => "RPN_CALCULATOR",
        "RPN_CALCULATOR_ALT" => "RPN_CALCULATOR_ALT",
        "SAILING" => "SAILING",
        "SAVE_LOAD" => "SAVE_LOAD",
        "SET_TIME" => "SET_TIME",
        "SET_TIME_HACKWATCH" => "SET_TIME_HACKWATCH",
        "SHIPS_BELL" => "SHIPS_BELL",
        "SIMON" => "SIMON",
        "SIMPLE_CALCULATOR" => "SIMPLE_CALCULATOR",
        "SIMPLE_CLOCK_BIN_LED" => "SIMPLE_CLOCK_BIN_LED",
        "SIMPLE_COIN_FLIP" => "SIMPLE_COIN_FLIP",
        "SOLAR_TIME" => "SOLAR_TIME",
        "SOLSTICE" => "SOLSTICE",
        "SOS" => "SOS",
        "SQUASH" => "SQUASH",
        "SUNRISE_SUNSET" => "SUNRISE_SUNSET",
        "TACHYMETER" => "TACHYMETER",
        "TALLY" => "TALLY",
        "TAROT" => "TAROT",
        "TEMPCHART" => "TEMPCHART",
        "THERMISTOR_LOGGING" => "THERMISTOR_LOGGING",
        "THERMISTOR_READOUT" => "THERMISTOR_READOUT",
        "THERMISTOR_TESTING" => "THERMISTOR_TESTING",
        "TIDE" => "TIDE",
        "TIME_LEFT" => "TIME_LEFT",
        "TOMATO" => "TOMATO",
        "TOSS_UP" => "TOSS_UP",
        "TOTP" => "TOTP",
        "TOTP_LFS" => "TOTP_LFS",
        "TUNING_TONES" => "TUNING_TONES",
        "VOLTAGE" => "VOLTAGE",
        "WAKE" => "WAKE",
        "WAREKI" => "WAREKI",
        "WEEKNUMBER" => "WEEKNUMBER",
        "WORDLE" => "WORDLE",
        "WORLD_CLOCK2" => "WORLD_CLOCK2",
        "WYOSCAN" => "WYOSCAN",
        _ => "",
    }
}

// ---------------------------------------------------------------------------
// Fallback implementation (feature `real-faces` off).
//
// Keeps a stable `RealFace` API so `main.rs` needs no `cfg`: `new` always
// returns `None`, so the Simulator transparently falls back to `face_sim`.
// ---------------------------------------------------------------------------

/// Placeholder when the firmware seam is not enabled. `new` always yields `None`.
#[cfg(not(feature = "real-faces"))]
pub struct RealFace {
    _private: (),
}

#[cfg(not(feature = "real-faces"))]
impl RealFace {
    pub fn new(_face_name: &str) -> Option<RealFace> {
        None
    }
    pub fn set_time(&mut self, _y: u32, _mo: u32, _d: u32, _h: u32, _mi: u32, _s: u32) -> bool {
        false
    }
    pub fn activate(&mut self, _time_mode_24: bool) {}
    pub fn tick(&mut self) {}
    pub fn press(&mut self, _light: bool, _alarm: bool) {}
    pub fn snapshot(&self) -> RealFaceSnapshot {
        RealFaceSnapshot::default()
    }
    pub fn face_name(&self) -> &str {
        ""
    }
}

/// Runs the real face `face_name` through the seam for the current time +
/// button flags and returns its captured LCD chars. `None` means no real face of
/// that name is available (or the seam is disabled), so the caller should fall
/// back to `face_sim`.
///
/// This is a stateless one-shot convenience API for hosting the rendered frame
/// without keeping a long-lived [`RealFace`]; the interactive Simulator instead
/// keeps a running [`RealFace`] so button/tick state persists across frames.
#[cfg(not(feature = "real-faces"))]
#[allow(dead_code)]
pub fn render_real_face(
    _face_name: &str,
    _year: u32,
    _month: u32,
    _day: u32,
    _hour: u32,
    _minute: u32,
    _second: u32,
    _weekday: u32,
    _time_mode_24: bool,
    _press_light: bool,
    _press_alarm: bool,
) -> Option<RealFaceSnapshot> {
    None
}

#[cfg(feature = "real-faces")]
#[allow(dead_code)]
pub fn render_real_face(
    face_name: &str,
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    _weekday: u32,
    time_mode_24: bool,
    press_light: bool,
    press_alarm: bool,
) -> Option<RealFaceSnapshot> {
    let mut face = RealFace::new(face_name)?;
    if !face.set_time(year, month, day, hour, minute, second) {
        return None;
    }
    face.activate(time_mode_24);
    face.press(press_light, press_alarm);
    Some(face.snapshot())
}

#[cfg(all(test, feature = "real-faces"))]
mod tests {
    use super::*;

    /// A known Friday (2023-01-06) afternoon in the app's `(year, month, day,
    /// hour, minute, second)` order, matching the reference core tests.
    fn friday() -> (u32, u32, u32, u32, u32, u32) {
        (2023, 1, 6, 15, 4, 0)
    }

    #[test]
    fn face_available_for_migrated_face() {
        assert!(RealFace::new("SIMPLE_CLOCK").is_some());
        assert!(RealFace::new("simple_clock").is_some());
        // The stock Casio set + other host-migrated faces resolve through the seam.
        for name in [
            "ALARM",
            "COUNTER",
            "WORLD_CLOCK",
            "STOPWATCH",
            "TIMER",
            "COUNTDOWN",
            "FLASHLIGHT",
        ] {
            assert!(RealFace::new(name).is_some(), "{name} should be migrated");
        }
        // MMIO-only stock_stopwatch and unknown names still fall back in the app.
        assert!(RealFace::new("STOCK_STOPWATCH").is_none());
        assert!(RealFace::new("NOT_A_FACE").is_none());
    }

    #[test]
    fn real_simple_clock_renders_24h_with_seconds() {
        let (y, mo, d, h, mi, s) = friday();
        let snap = render_real_face("SIMPLE_CLOCK", y, mo, d, h, mi, s, 5, true, false, false)
            .expect("SIMPLE_CLOCK is migrated");
        // The REAL write path: FR + day 06 + HH:MM (no seconds — show_seconds
        // defaults to false). Colon on, 24h indicator set.
        let text: String = snap.chars.iter().collect();
        assert_eq!(text.trim_end_matches([' ', '\0']), "FR061504");
        assert!(snap.colon);
        assert!(snap.h24);
    }

    #[test]
    fn real_simple_clock_12h_sets_pm() {
        let (y, mo, d, h, mi, s) = friday();
        // 15:04 is PM in 12-hour mode.
        let snap =
            render_real_face("SIMPLE_CLOCK", y, mo, d, h, mi, s, 5, false, false, false).unwrap();
        assert!(snap.pm);
        assert!(!snap.h24);
    }

    #[test]
    fn real_simple_clock_12h_renders_all_boundary_hours() {
        let cases = [
            (0, "FR061210\0\0", false),
            (1, "FR060110\0\0", false),
            (11, "FR061110\0\0", false),
            (12, "FR061210\0\0", true),
            (13, "FR060110\0\0", true),
            (23, "FR061110\0\0", true),
        ];
        for (hour, expected, pm) in cases {
            let snap = render_real_face(
                "SIMPLE_CLOCK",
                2023,
                1,
                6,
                hour,
                10,
                0,
                5,
                false,
                false,
                false,
            )
            .unwrap();
            let text: String = snap.chars.iter().collect();
            assert_eq!(text, expected, "unexpected 12-hour display at {hour:02}:10");
            assert_eq!(snap.pm, pm, "unexpected PM indicator at {hour:02}:10");
            assert!(!snap.h24);
        }
    }

    #[test]
    fn real_simple_clock_24h_renders_midnight_noon_and_late_night() {
        for hour in [0, 12, 23] {
            let snap = render_real_face(
                "SIMPLE_CLOCK",
                2023,
                1,
                6,
                hour,
                10,
                0,
                5,
                true,
                false,
                false,
            )
            .unwrap();
            assert!(snap.h24);
            assert!(!snap.pm);
        }
    }

    #[test]
    fn real_simple_clock_time_and_mode_changes_refresh_without_reactivation() {
        let mut face = RealFace::new("SIMPLE_CLOCK").expect("face");
        assert!(face.set_time(2023, 1, 6, 23, 10, 0));
        face.activate(false);
        assert!(face.snapshot().pm);

        assert!(face.set_time(2023, 1, 6, 1, 10, 0));
        let am = face.snapshot();
        assert!(!am.pm);
        assert_eq!(
            am.chars,
            ['F', 'R', '0', '6', '0', '1', '1', '0', '\0', '\0']
        );

        face.activate(true);
        let h24 = face.snapshot();
        assert!(h24.h24);
        assert!(!h24.pm);
    }

    #[test]
    fn unmigrated_face_falls_back() {
        assert!(render_real_face(
            "STOCK_STOPWATCH",
            2023,
            1,
            6,
            15,
            4,
            0,
            5,
            true,
            false,
            false
        )
        .is_none());
    }

    #[test]
    fn real_faces_can_switch_and_recreate() {
        let mut first = RealFace::new("SIMPLE_CLOCK").expect("first face");
        first.set_time(2023, 1, 6, 15, 4, 0);
        first.activate(true);
        drop(first);

        let mut second = RealFace::new("ALARM").expect("second face");
        assert_eq!(second.face_name(), "ALARM");
        assert!(second.set_time(2023, 1, 6, 15, 4, 0));
        second.activate(true);
        drop(second);

        assert!(RealFace::new("SIMPLE_CLOCK").is_some());
    }

    #[test]
    fn repeated_switch_and_drop_does_not_deadlock_the_seam() {
        for i in 0..64 {
            let name = if i % 2 == 0 { "SIMPLE_CLOCK" } else { "ALARM" };
            let mut face = RealFace::new(name).expect("migrated face");
            assert!(face.set_time(2023, 1, 6, i % 24, 4, i % 60));
            face.activate(i % 3 == 0);
            drop(face);
        }
    }

    #[test]
    fn concurrent_switch_and_drop_is_serialized() {
        let workers = (0..4)
            .map(|worker| {
                std::thread::spawn(move || {
                    for iteration in 0..16 {
                        let name = if (worker + iteration) % 2 == 0 {
                            "SIMPLE_CLOCK"
                        } else {
                            "ALARM"
                        };
                        let mut face = RealFace::new(name).expect("migrated face");
                        assert!(face.set_time(2023, 1, 6, 12, iteration, 0));
                        face.activate(iteration % 2 == 0);
                    }
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            worker.join().expect("seam worker panicked");
        }
        assert!(RealFace::new("SIMPLE_CLOCK").is_some());
    }

    #[test]
    fn repeated_ticks_keep_the_face_alive() {
        let mut face = RealFace::new("SIMPLE_CLOCK").expect("face");
        assert!(face.set_time(2023, 1, 6, 15, 4, 0));
        face.activate(true);
        let before = face.snapshot();
        face.tick();
        let after_one = face.snapshot();
        face.tick();
        let after_two = face.snapshot();
        assert!(before.colon || after_one.colon || after_two.colon);
    }

    #[test]
    fn invalid_dates_are_rejected_without_wrapping() {
        let mut face = RealFace::new("SIMPLE_CLOCK").expect("face");
        assert!(!face.set_time(2019, 1, 1, 0, 0, 0));
        assert!(!face.set_time(2084, 1, 1, 0, 0, 0));
        assert!(!face.set_time(2023, 0, 1, 0, 0, 0));
        assert!(!face.set_time(2023, 13, 1, 0, 0, 0));
        assert!(!face.set_time(2023, 2, 29, 0, 0, 0));
        assert!(!face.set_time(2023, 1, 32, 0, 0, 0));
        assert!(!face.set_time(2023, 1, 1, 24, 0, 0));
        assert!(!face.set_time(2023, 1, 1, 0, 60, 0));
        assert!(!face.set_time(2023, 1, 1, 0, 0, 60));
        assert!(face.set_time(2024, 2, 29, 23, 59, 59));
    }

    #[test]
    fn real_alarm_renders_24h() {
        let (y, mo, d, h, mi, s) = friday();
        let snap = render_real_face("ALARM", y, mo, d, h, mi, s, 5, true, false, false)
            .expect("ALARM is migrated");
        // The REAL alarm face writes a day-of-week + alarm index + time.
        let text: String = snap.chars.iter().collect();
        assert_eq!(text.trim_end_matches([' ', '\0']), "AL01 000");
        assert!(snap.colon);
    }

    #[test]
    fn newly_migrated_faces_run_through_host_seam() {
        for name in [
            "ASTRONOMY",
            "CLOSE_ENOUGH",
            "DAY_NIGHT_PERCENTAGE",
            "DAY_ONE",
            "DEADLINE",
            "DECIMAL_TIME",
            "FRENCH_REVOLUTIONARY",
            "FREQUENCY_CORRECTION",
            "HELLO_THERE",
            "KE_DECIMAL_TIME",
        ] {
            let snapshot = render_real_face(name, 2023, 1, 6, 15, 4, 0, 5, true, false, false);
            assert!(snapshot.is_some(), "{name} should render through host seam");
        }
    }

    #[test]
    fn real_counter_renders_zero() {
        let (y, mo, d, h, mi, s) = friday();
        let snap = render_real_face("COUNTER", y, mo, d, h, mi, s, 5, true, false, false)
            .expect("COUNTER is migrated");
        // The REAL counter face only renders on Activate or button presses;
        // RealFace::activate sends Tick, so the display stays blank. The
        // signal indicator is set during activate (beep_on defaults to true).
        let text: String = snap.chars.iter().collect();
        assert_eq!(text.trim_end_matches([' ', '\0']), "");
        assert!(snap.signal);
    }
}
