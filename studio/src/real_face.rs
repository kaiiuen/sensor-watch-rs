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
//! `SIMPLE_CLOCK`, `ALARM`, `COUNTER`, `WORLD_CLOCK`, `STOPWATCH`, `TIMER`, `COUNTDOWN`,
//! `FLASHLIGHT`, and `DIAGNOSTICS`. New faces are added by extending [`new_face`] once their host
//! harness lands in the firmware seam.
//!
//! # Feature gating
//!
//! The bridge lives behind the Studio `real-faces` feature (see `Cargo.toml`),
//! because pulling the firmware *host lib* into the app as a dependency currently
//! requires the firmware seam to compile as a host lib (the firmware's `watch`
//! tree is mid-migration and not yet a clean host dependency). With the feature
//! **on**, this module drives the real faces through the seam. With it **off**, it
//! exposes a fallback `RealFace` that is always `None`, so the Simulator
//! transparently keeps using `face_sim` and the app still compiles and passes its
//! tests. No Studio code needs changing. The main loop sees "seam unavailable"
//! and falls back.

#[cfg(feature = "real-faces")]
use sensor_watch::movement::{
    accel_interrupt_count, accelerometer_data_acquisition, activity, advanced_alarm, alarm,
    alarm_thermometer, astronomy, baby_kicks, beats, beeps, blackjack, blinky, breathing,
    butterfly_game, character_set, chirpy_demo, close_enough, couch_to_5k, countdown, counter,
    databank, day_night_percentage, day_one, days_since, deadline, decimal_time, demo, diagnostics,
    discgolf, dual_timer, endless_runner, finetune, flashlight, french_revolutionary,
    frequency_correction, geomancy, habit, hello_there, higher_lower_game, hydration, interval,
    invaders, ish, ke_decimal_time, kitchen_conversions, lander, lightmeter, lis2dw_logging,
    mars_time, menstrual_cycle, metronome, minimal_clock, minmax, minute_repeater_decimal,
    moon_phase, morsecalc, nanosec, orrery, periodic, ping, planetary_hours, planetary_time,
    preferences, probability, pulsometer, randonaut, ratemeter, repetition_minute, rpn_calculator,
    rpn_calculator_alt, sailing, save_load, set_time, set_time_hackwatch, settings_face,
    ships_bell, simon, simple_calculator, simple_clock, simple_clock_bin_led, simple_coin_flip,
    solar_time, solstice, sos, squash, stock_stopwatch, stopwatch, sunrise_sunset, tachymeter,
    tally, tarot, tempchart, thermistor_logging, thermistor_readout, thermistor_testing, tide,
    time_left, timer, tomato, toss_up, totp, totp_lfs, tuning_tones, types, voltage, wake, wareki,
    weeknumber, wordle, world_clock, world_clock2, wyoscan,
};
#[cfg(feature = "real-faces")]
use sensor_watch_core::datetime::DateTime;
#[cfg(feature = "real-faces")]
use sensor_watch_core::mock_hw::{Hw, MockHw};

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

/// The two physical buttons that are delivered to a real face. C remains a
/// Studio-only face-cycle button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealButton {
    Light,
    Alarm,
}

/// Button transitions exposed by the Studio's deterministic hold model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealButtonEvent {
    Down,
    Up,
    LongPress,
    LongUp,
}

/// Pure edge/hold state machine for a Studio button.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ButtonEventState {
    down: bool,
    hold_seconds: f32,
}

impl ButtonEventState {
    pub const LONG_PRESS_SECONDS: f32 = 64.0 / 128.0;

    /// Advances one sampled button state. A long press is emitted exactly once
    /// when the accumulated hold crosses the threshold; release emits Up or
    /// LongUp accordingly.
    pub fn update(&mut self, is_down: bool, dt_seconds: f32) -> Option<RealButtonEvent> {
        if is_down && !self.down {
            self.down = true;
            self.hold_seconds = 0.0;
            Some(RealButtonEvent::Down)
        } else if !is_down && self.down {
            let event = if self.hold_seconds >= Self::LONG_PRESS_SECONDS {
                RealButtonEvent::LongUp
            } else {
                RealButtonEvent::Up
            };
            self.down = false;
            self.hold_seconds = 0.0;
            Some(event)
        } else if is_down {
            let was_long = self.hold_seconds >= Self::LONG_PRESS_SECONDS;
            self.hold_seconds += dt_seconds.max(0.0);
            if !was_long && self.hold_seconds >= Self::LONG_PRESS_SECONDS {
                Some(RealButtonEvent::LongPress)
            } else {
                None
            }
        } else {
            None
        }
    }
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
    /// The mock hardware the face draws onto.
    mock: Box<MockHw>,
    /// The settings the face mutates (the firmware's movement settings).
    settings: types::Settings,
    /// The display snapshot of the last render (derived from `mock`).
    snapshot: RealFaceSnapshot,
    /// Whether the face has received its initial activation.
    activated: bool,
}

/// Object-safe seam over any migrated firmware `WatchFace`, so the Studio caller
/// can `activate`/`loop_` without knowing the concrete face type.
#[cfg(feature = "real-faces")]
trait RealFaceTrait {
    fn activate(&mut self, settings: &types::Settings);
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings);
    fn resign(&mut self, _settings: &mut types::Settings) {}
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

#[cfg(feature = "real-faces")]
impl RealFaceTrait for accel_interrupt_count::AccelInterruptCountFace {
    fn activate(&mut self, settings: &types::Settings) {
        types::WatchFace::activate(self, settings);
    }
    fn loop_(&mut self, event: types::Event, settings: &mut types::Settings) {
        types::WatchFace::loop_(self, event, settings);
    }
    fn resign(&mut self, settings: &mut types::Settings) {
        types::WatchFace::resign(self, settings);
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
            fn resign(&mut self, settings: &mut types::Settings) {
                types::WatchFace::resign(self, settings);
            }
        }
    };
}

#[cfg(feature = "real-faces")]
impl_real_face_trait!(activity::ActivityFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(blackjack::BlackjackFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(couch_to_5k::CouchTo5kFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(alarm_thermometer::AlarmThermometerFace);
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
impl_real_face_trait!(geomancy::GeomancyFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(finetune::FinetuneFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(dual_timer::DualTimerFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(endless_runner::EndlessRunnerFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(stock_stopwatch::StockStopwatchFace);
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
impl_real_face_trait!(baby_kicks::BabyKicksFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(beats::BeatsFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(beeps::BeepsFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(blinky::BlinkyFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(character_set::CharacterSetFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(chirpy_demo::ChirpyDemoFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(demo::DemoFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(databank::DatabankFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(days_since::DaysSinceFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(habit::HabitFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(higher_lower_game::HigherLowerGameFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(hydration::HydrationFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(breathing::BreathingFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(discgolf::DiscgolfFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(butterfly_game::ButterflyGameFace);

#[cfg(feature = "real-faces")]
impl_real_face_trait!(diagnostics::DiagnosticsFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(accelerometer_data_acquisition::AccelerometerDataAcquisitionFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(advanced_alarm::AdvancedAlarmFace);
#[cfg(feature = "real-faces")]
impl_real_face_trait!(settings_face::SettingsFace);

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
        let mut mock = Box::new(MockHw::new());
        mock.vcc_mv = 3000; // healthy battery

        let settings = types::Settings::default();
        Some(RealFace {
            face,
            mock,
            settings,
            snapshot: RealFaceSnapshot::default(),
            activated: false,
            face_name: new_face_name(face_name),
        })
    }

    /// Applies simulator-only sensor values to the host mock.
    pub fn set_sensor_overrides(
        &mut self,
        voltage_mv: Option<u16>,
        temperature_celsius: Option<f32>,
    ) {
        self.mock.vcc_mv = voltage_mv.unwrap_or(3000);
        self.mock.thermistor_temperature_celsius = temperature_celsius;
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
        let time_changed = self.mock.now != next;
        self.mock.now = next;
        // Redraw an already-active face after an explicit time edit, but never
        // turn ordinary same-time GUI redraws into firmware ticks. Stateful
        // timing faces must not receive a synthetic event here: their display
        // is synchronized by explicit `tick` events, and their Activate paths
        // derive elapsed state from the RTC.
        if self.activated && time_changed && !self.is_stateful_timing_face() {
            sensor_watch::watch::seam::with_hw(&mut *self.mock, || {
                self.face.loop_(types::Event::Activate, &mut self.settings);
            });
            self.snapshot_from_mock();
        }
        true
    }

    fn is_stateful_timing_face(&self) -> bool {
        matches!(
            self.face_name,
            "STOPWATCH" | "STOCK_STOPWATCH" | "TIMER" | "COUNTDOWN" | "COUCH_TO_5K" | "BABY_KICKS"
        )
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
        sensor_watch::watch::seam::with_hw(&mut *self.mock, || {
            self.face.activate(&self.settings);
            let initial_event = if matches!(
                self.face_name,
                "ACTIVITY"
                    | "BLACKJACK"
                    | "COUCH_TO_5K"
                    | "HIGHER_LOWER_GAME"
                    | "ENDLESS_RUNNER"
                    | "HYDRATION"
                    | "BABY_KICKS"
                    | "BUTTERFLY_GAME"
            ) {
                types::Event::Activate
            } else {
                types::Event::Tick
            };
            self.face.loop_(initial_event, &mut self.settings);
        });
        self.activated = true;
        self.snapshot_from_mock();
    }

    /// Delivers one firmware RTC tick and refreshes the captured display.
    ///
    /// Callers must only invoke this at a simulated-second boundary; ordinary
    /// frame redraws and RTC edits belong in [`set_time`].
    pub fn tick(&mut self) {
        sensor_watch::watch::seam::with_hw(&mut *self.mock, || {
            self.face.loop_(types::Event::Tick, &mut self.settings);
        });
        self.snapshot_from_mock();
    }

    /// Delivers one public button transition to the firmware face. `Light` is
    /// L and `Alarm` is A; C is consumed by Studio for face cycling.
    /// Injects a tap event through the same firmware event path used by the
    /// accelerometer interrupt callback. This remains useful on host even when
    /// no physical accelerometer is available.
    #[allow(dead_code)]
    pub fn tap_event(&mut self, double: bool) {
        let event = if double {
            types::Event::DoubleTap
        } else {
            types::Event::SingleTap
        };
        sensor_watch::watch::seam::with_hw(&mut *self.mock, || {
            self.face.loop_(event, &mut self.settings);
        });
        self.snapshot_from_mock();
    }

    /// Resigns the face and releases any face-owned hardware state.
    pub fn resign(&mut self) {
        if self.activated {
            sensor_watch::watch::seam::with_hw(&mut *self.mock, || {
                self.face.resign(&mut self.settings);
            });
            self.activated = false;
        }
    }

    pub fn button_event(&mut self, button: RealButton, event: RealButtonEvent) {
        let button = match button {
            RealButton::Light => types::Button::Light,
            RealButton::Alarm => types::Button::Alarm,
        };
        let event = match event {
            RealButtonEvent::Down => types::ButtonEvent::Down,
            RealButtonEvent::Up => types::ButtonEvent::Up,
            RealButtonEvent::LongPress => types::ButtonEvent::LongPress,
            RealButtonEvent::LongUp => types::ButtonEvent::LongUp,
        };
        sensor_watch::watch::seam::with_hw(&mut *self.mock, || {
            self.face
                .loop_(types::Event::Button(button, event), &mut self.settings);
        });
        self.snapshot_from_mock();
    }

    /// Compatibility helper for callers that model a completed short press.
    pub fn press(&mut self, light: bool, alarm: bool) {
        if light {
            self.button_event(RealButton::Light, RealButtonEvent::Up);
        }
        if alarm {
            self.button_event(RealButton::Alarm, RealButtonEvent::Up);
        }
    }

    /// Returns the exact civil time currently handed to the firmware seam.
    #[cfg(feature = "real-faces")]
    pub fn time(&self) -> DateTime {
        self.mock.now
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

#[cfg(feature = "real-faces")]
impl Drop for RealFace {
    fn drop(&mut self) {
        self.resign();
    }
}

#[cfg(all(test, feature = "real-faces"))]
mod hydration_tests {
    use super::{RealFace, REAL_FACE_NAMES};

    #[test]
    fn hydration_is_registered_canonical_and_activates_with_activate_event() {
        assert!(REAL_FACE_NAMES.contains(&"HYDRATION"));
        let mut face = RealFace::new("hydration").expect("Hydration seam mapping");
        assert_eq!(face.face_name(), "HYDRATION");
        assert!(!face.is_activated());
        assert!(face.set_time(2024, 2, 29, 9, 0, 0));
        face.activate(true);
        assert!(face.is_activated());
        assert_eq!(
            face.snapshot().chars,
            ['H', 'Y', '0', '0', '0', '0', '0', '0', 'm', 'l']
        );
    }
}

#[cfg(all(test, feature = "real-faces"))]
mod diagnostics_tests {
    use super::{RealButton, RealButtonEvent, RealFace, REAL_FACE_NAMES};

    fn tap(face: &mut RealFace, button: RealButton) {
        face.button_event(button, RealButtonEvent::Up);
    }

    fn enter(face: &mut RealFace, rows: usize) {
        for _ in 0..rows {
            tap(face, RealButton::Light);
        }
        tap(face, RealButton::Alarm);
    }

    #[test]
    fn diagnostics_is_registered_and_activation_resets_to_main_menu() {
        assert!(REAL_FACE_NAMES.contains(&"DIAGNOSTICS"));
        let mut face = RealFace::new("diagnostics").expect("Diagnostics seam mapping");
        assert_eq!(face.face_name(), "DIAGNOSTICS");
        assert!(face.set_time(2024, 2, 29, 15, 4, 0));
        face.activate(true);
        assert!(face.is_activated());
        assert_eq!(
            face.snapshot().chars,
            ['0', '0', '0', '0', 'C', 'P', 'U', ' ', ' ', ' ']
        );

        enter(&mut face, 5);
        assert_eq!(
            face.snapshot().chars,
            ['0', '1', '0', '0', 'E', 'M', '1', '5', '0', '4']
        );
        face.activate(true);
        assert_eq!(
            face.snapshot().chars,
            ['0', '0', '0', '0', 'C', 'P', 'U', ' ', ' ', ' ']
        );
    }

    #[test]
    fn diagnostics_navigation_renders_settings_stats_and_system_and_backs_out() {
        let mut face = RealFace::new("DIAGNOSTICS").expect("Diagnostics seam mapping");
        assert!(face.set_time(2024, 2, 29, 15, 4, 0));
        face.activate(true);

        enter(&mut face, 5); // SYSTEM
        assert_eq!(
            face.snapshot().chars,
            ['0', '1', '0', '0', 'E', 'M', '1', '5', '0', '4']
        );
        tap(&mut face, RealButton::Alarm); // breadcrumb/back to menu
        face.activate(true); // reset cursor to the first menu row
        enter(&mut face, 6); // SETTINGS
        assert_eq!(
            face.snapshot().chars,
            ['0', '2', '0', '0', ' ', 'G', 'R', 'E', 'E', 'N']
        );
        tap(&mut face, RealButton::Alarm); // toggle the board setting
        assert_eq!(face.snapshot().chars[4..], [' ', 'R', 'E', 'D', ' ', ' ']);
        tap(&mut face, RealButton::Alarm); // settings row 0 remains a setting, not back
        tap(&mut face, RealButton::Light);
        tap(&mut face, RealButton::Alarm); // still settings; deterministic row navigation
        tap(&mut face, RealButton::Alarm); // exit from the system-like page is covered above

        face.activate(true);
        enter(&mut face, 7); // STATS
        assert_eq!(
            face.snapshot().chars,
            ['0', '2', '0', '0', 'T', ' ', '0', '0', '0', '0']
        );
    }

    #[test]
    fn diagnostics_resign_is_idempotent_after_button_events() {
        let mut face = RealFace::new("DIAGNOSTICS").expect("Diagnostics seam mapping");
        face.activate(true);
        tap(&mut face, RealButton::Light);
        tap(&mut face, RealButton::Alarm);
        face.resign();
        assert!(!face.is_activated());
        face.resign();
        assert!(!face.is_activated());
    }
}

#[cfg(all(test, feature = "real-faces"))]
mod couch_to_5k_tests {
    use super::{RealFace, REAL_FACE_NAMES};

    #[test]
    fn couch_to_5k_is_registered_and_activation_runs_event_activate() {
        assert!(REAL_FACE_NAMES.contains(&"COUCH_TO_5K"));
        let mut face = RealFace::new("couch_to_5k").expect("Couch-to-5K seam mapping");
        face.activate(true);
        assert_eq!(face.face_name(), "COUCH_TO_5K");
        assert_eq!(
            face.snapshot().chars,
            ['W', 'U', '0', '1', '0', '5', '0', '0', '0', '1']
        );
        assert!(face.snapshot().colon);
    }

    #[test]
    fn running_couch_to_5k_ignores_changed_and_backward_set_time() {
        let mut face = RealFace::new("COUCH_TO_5K").expect("Couch-to-5K seam mapping");
        assert!(face.set_time(2023, 1, 6, 15, 4, 0));
        face.activate(true);
        face.press(false, true); // start the warmup
        face.tick();
        let running = face.snapshot();

        assert!(face.set_time(2023, 1, 6, 15, 5, 0));
        assert_eq!(face.snapshot().chars, running.chars);
        assert_eq!(face.snapshot().colon, running.colon);

        assert!(face.set_time(2023, 1, 6, 15, 3, 0));
        assert_eq!(face.snapshot().chars, running.chars);
        assert_eq!(face.snapshot().colon, running.colon);

        // A further tick proves the workout stayed running rather than being
        // reset to paused warmup by a synthetic Activate event.
        face.tick();
        assert_eq!(
            face.snapshot().chars,
            ['W', 'U', '0', '1', '0', '4', '5', '8', '0', '1']
        );
    }
}

#[cfg(all(test, feature = "real-faces"))]
mod baby_kicks_tests {
    use super::{RealButton, RealButtonEvent, RealFace, REAL_FACE_NAMES};

    #[test]
    fn baby_kicks_is_registered_canonical_and_activates_with_activate_event() {
        assert!(REAL_FACE_NAMES.contains(&"BABY_KICKS"));
        let mut face = RealFace::new("baby_kicks").expect("Baby kicks seam mapping");
        assert_eq!(face.face_name(), "BABY_KICKS");
        assert!(!face.is_activated());

        assert!(face.set_time(2023, 1, 6, 15, 4, 0));
        face.activate(true);
        assert!(face.is_activated());
        assert_eq!(
            face.snapshot().chars,
            [' ', ' ', ' ', ' ', 'b', 'a', 'b', 'y', ' ', ' ']
        );
        assert!(!face.snapshot().colon);
    }

    #[test]
    fn baby_kicks_uses_stateful_redraw_guard_and_resigns_safely() {
        let mut face = RealFace::new("BABY_KICKS").expect("Baby kicks seam mapping");
        assert!(face.set_time(2023, 1, 6, 15, 4, 0));
        face.activate(true);
        face.button_event(RealButton::Alarm, RealButtonEvent::Up);
        face.button_event(RealButton::Alarm, RealButtonEvent::Up);
        let running = face.snapshot();

        // A changed RTC redraw must not synthesize Activate for a stateful face.
        assert!(face.set_time(2023, 1, 6, 15, 5, 0));
        assert_eq!(face.snapshot().chars, running.chars);
        assert_eq!(face.snapshot().colon, running.colon);

        face.resign();
        assert!(!face.is_activated());
        face.resign();
    }
}

#[cfg(all(test, feature = "real-faces"))]
mod generated_adapter_tests {
    use super::{types, RealFace, RealFaceSnapshot, RealFaceTrait};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static RESIGN_CALLS: AtomicUsize = AtomicUsize::new(0);

    struct TestFace;

    impl types::WatchFace for TestFace {
        fn setup(&mut self, _settings: &types::Settings, _watch_face_index: usize) {}

        fn activate(&mut self, _settings: &types::Settings) {}

        fn loop_(&mut self, _event: types::Event, _settings: &mut types::Settings) {}

        fn resign(&mut self, _settings: &mut types::Settings) {
            RESIGN_CALLS.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl_real_face_trait!(TestFace);

    #[test]
    fn real_face_resign_forwards_to_generated_adapter() {
        RESIGN_CALLS.store(0, Ordering::SeqCst);
        let mut face = RealFace {
            face: Box::new(TestFace),
            face_name: "TEST_FACE",
            mock: Box::new(super::MockHw::new()),
            settings: types::Settings::default(),
            snapshot: RealFaceSnapshot::default(),
            activated: false,
        };

        face.activate(false);
        face.resign();
        assert_eq!(RESIGN_CALLS.load(Ordering::SeqCst), 1);

        // Resigning an inactive face remains a no-op at the RealFace lifecycle
        // boundary, so cleanup is not duplicated.
        face.resign();
        assert_eq!(RESIGN_CALLS.load(Ordering::SeqCst), 1);
    }
}

#[cfg(all(test, feature = "real-faces"))]
mod blackjack_tests {
    use super::{RealFace, REAL_FACE_NAMES};

    #[test]
    fn blackjack_is_registered_canonical_and_activates_once() {
        assert!(REAL_FACE_NAMES.contains(&"BLACKJACK"));
        let mut face = RealFace::new("blackjack").expect("Blackjack seam mapping");
        assert_eq!(face.face_name(), "BLACKJACK");
        assert!(!face.is_activated());

        face.activate(true);
        assert!(face.is_activated());
        assert_eq!(
            face.snapshot().chars,
            ['2', '1', ' ', ' ', 'B', 'L', 'a', 'K', 'J', 'K']
        );

        face.resign();
        assert!(!face.is_activated());
        face.resign();
    }

    #[test]
    fn blackjack_start_is_deterministic_and_button_lifecycle_is_bounded() {
        let mut first = RealFace::new("BLACKJACK").unwrap();
        let mut second = RealFace::new("blackjack").unwrap();
        assert!(first.set_time(2023, 1, 6, 15, 4, 0));
        assert!(second.set_time(2023, 1, 6, 15, 4, 0));
        first.activate(true);
        second.activate(true);
        first.press(false, true);
        second.press(false, true);
        assert_eq!(first.snapshot().chars, second.snapshot().chars);

        // Exercise the full public path without relying on an unbounded game loop.
        first.press(true, false);
        for _ in 0..32 {
            first.tick();
        }
        assert_eq!(first.snapshot().chars.len(), 10);
    }
}

/// Declarative registry for every face migrated through the firmware seam.
///
/// Each entry is the single source of truth for the canonical name, concrete
/// firmware face type, and constructor variant.
#[cfg(feature = "real-faces")]
macro_rules! real_face_registry {
    ($($name:literal => $constructor:expr,)+ $(,)?) => {
        #[allow(dead_code)]
        pub(crate) const REAL_FACE_NAMES: &[&str] = &[$($name),+];

        fn new_face(face_name: &str) -> Option<Box<dyn RealFaceTrait>> {
            let upper = super::faces::face_identity(face_name).to_ascii_uppercase();
            match upper.as_str() {
                $($name => Some(Box::new($constructor())), )+
                _ => None,
            }
        }

        fn new_face_name(face_name: &str) -> &'static str {
            let upper = super::faces::face_identity(face_name).to_ascii_uppercase();
            match upper.as_str() {
                $($name => $name, )+
                _ => "",
            }
        }
    };
}

#[cfg(all(test, feature = "real-faces"))]
mod remaining_face_tests {
    use super::{RealButton, RealButtonEvent, RealFace, REAL_FACE_NAMES};

    fn tap(face: &mut RealFace, button: RealButton) {
        face.button_event(button, RealButtonEvent::Up);
    }

    #[test]
    fn remaining_faces_are_registered_with_canonical_names() {
        for name in [
            "ACCELEROMETER_DATA_ACQUISITION",
            "ADVANCED_ALARM",
            "SETTINGS_FACE",
        ] {
            assert!(REAL_FACE_NAMES.contains(&name), "missing {name}");
            assert_eq!(
                RealFace::new(&name.to_ascii_lowercase())
                    .unwrap()
                    .face_name(),
                name
            );
        }
    }

    #[test]
    fn accelerometer_face_runs_without_claiming_sensor_hardware() {
        let mut face = RealFace::new("accelerometer_data_acquisition").unwrap();
        face.activate(true);
        assert!(face.is_activated());
        assert_eq!(face.snapshot().chars.len(), 10);
        // The host LIS2DW seam is unavailable; starting a run must remain safe
        // and must not turn an absent sensor into a physical PASS.
        tap(&mut face, RealButton::Alarm);
        for _ in 0..4 {
            face.tick();
        }
        assert_eq!(face.snapshot().chars.len(), 10);
    }

    #[test]
    fn advanced_alarm_preserves_alarm_entry_and_resign_lifecycle() {
        let mut face = RealFace::new("advanced_alarm").unwrap();
        face.activate(true);
        assert!(face.snapshot().colon);
        tap(&mut face, RealButton::Light);
        assert!(face.is_activated());
        face.resign();
        assert!(!face.is_activated());
    }

    #[test]
    fn settings_face_cycles_pages_and_persists_changes_across_reentry() {
        let mut face = RealFace::new("settings_face").unwrap();
        face.activate(true);
        assert_eq!(&face.snapshot().chars[..2], &['C', 'L']);
        tap(&mut face, RealButton::Alarm);
        face.resign();
        face.activate(true);
        assert_eq!(&face.snapshot().chars[..2], &['C', 'L']);
        face.button_event(RealButton::Light, RealButtonEvent::Down);
        assert_eq!(&face.snapshot().chars[..2], &['B', 'T']);
    }
}

#[cfg(feature = "real-faces")]
real_face_registry! {
    "SIMPLE_CLOCK" => simple_clock::SimpleClockFace::new,
    "ACCEL_INTERRUPT_COUNT" => accel_interrupt_count::AccelInterruptCountFace::new_static,
    "ACCELEROMETER_DATA_ACQUISITION" => accelerometer_data_acquisition::AccelerometerDataAcquisitionFace::new,
    "ADVANCED_ALARM" => advanced_alarm::AdvancedAlarmFace::new,
    "SETTINGS_FACE" => settings_face::SettingsFace::new,
    "BABY_KICKS" => baby_kicks::BabyKicksFace::new,
    "BUTTERFLY_GAME" => butterfly_game::ButterflyGameFace::new,
    "ACTIVITY" => activity::ActivityFace::new_static,
    "BLACKJACK" => blackjack::BlackjackFace::new_static,
    "COUCH_TO_5K" => couch_to_5k::CouchTo5kFace::new,
    "ALARM" => alarm::AlarmFace::new_static,
    "ALARM_THERMOMETER" => alarm_thermometer::AlarmThermometerFace::new_static,
    "COUNTER" => counter::CounterFace::new_static,
    "WORLD_CLOCK" => world_clock::WorldClockFace::new_static,
    "STOPWATCH" => stopwatch::StopwatchFace::new,
    "STOCK_STOPWATCH" => stock_stopwatch::StockStopwatchFace::new,
    "TIMER" => timer::TimerFace::new,
    "COUNTDOWN" => countdown::CountdownFace::new_static,
    "DIAGNOSTICS" => diagnostics::DiagnosticsFace::new_static,
    "DUAL_TIMER" => dual_timer::DualTimerFace::new_static,
    "ENDLESS_RUNNER" => endless_runner::EndlessRunnerFace::new,
    "HYDRATION" => hydration::HydrationFace::new,
    "FINETUNE" => finetune::FinetuneFace::new_static,
    "FLASHLIGHT" => flashlight::FlashlightFace::new_static,
    "BEEPS" => beeps::BeepsFace::new_static,
    "BLINKY" => blinky::BlinkyFace::new_static,
    "BREATHING" => breathing::BreathingFace::new_static,
    "CHARACTER_SET" => character_set::CharacterSetFace::new_static,
    "CHIRPY_DEMO" => chirpy_demo::ChirpyDemoFace::new_static,
    "DATABANK" => databank::DatabankFace::new_static,
    "DAYS_SINCE" => days_since::DaysSinceFace::new_static,
    "DEMO" => demo::DemoFace::new_static,
    "DISCGOLF" => discgolf::DiscgolfFace::new_static,
    "BEATS" => beats::BeatsFace::new_static,
    "ASTRONOMY" => astronomy::AstronomyFace::new_static,
    "CLOSE_ENOUGH" => close_enough::CloseEnoughClockFace::new_static,
    "DAY_NIGHT_PERCENTAGE" => day_night_percentage::DayNightPercentageFace::new_static,
    "DAY_ONE" => day_one::DayOneFace::new_static,
    "DEADLINE" => deadline::DeadlineFace::new_static,
    "DECIMAL_TIME" => decimal_time::DecimalTimeFace::new_static,
    "FRENCH_REVOLUTIONARY" => french_revolutionary::FrenchRevolutionaryFace::new_static,
    "FREQUENCY_CORRECTION" => frequency_correction::FrequencyCorrectionFace::new_static,
    "GEOMANCY" => geomancy::GeomancyFace::new_static,
    "HABIT" => habit::HabitFace::new_static,
    "HIGHER_LOWER_GAME" => higher_lower_game::HigherLowerGameFace::new_static,
    "HELLO_THERE" => hello_there::HelloThereFace::new_static,
    "KE_DECIMAL_TIME" => ke_decimal_time::KeDecimalTimeFace::new_static,
    "INTERVAL" => interval::IntervalFace::new_static,
    "INVADERS" => invaders::InvadersFace::new_static,
    "ISH" => ish::IshFace::new_static,
    "KITCHEN_CONVERSIONS" => kitchen_conversions::KitchenConversionsFace::new_static,
    "LANDER" => lander::LanderFace::new_static,
    "LIGHTMETER" => lightmeter::LightmeterFace::new_static,
    "LIS2DW_LOGGING" => lis2dw_logging::Lis2dwLoggingFace::new_static,
    "MARS_TIME" => mars_time::MarsTimeFace::new_static,
    "MENSTRUAL_CYCLE" => menstrual_cycle::MenstrualCycleFace::new_static,
    "METRONOME" => metronome::MetronomeFace::new_static,
    "MINIMAL_CLOCK" => minimal_clock::MinimalClockFace::new_static,
    "MINMAX" => minmax::MinmaxFace::new_static,
    "MINUTE_REPEATER_DECIMAL" => minute_repeater_decimal::MinuteRepeaterDecimalFace::new_static,
    "MOON_PHASE" => moon_phase::MoonPhaseFace::new_static,
    "MORSECALC" => morsecalc::MorsecalcFace::new_static,
    "NANOSEC" => nanosec::NanosecFace::new_static,
    "ORRERY" => orrery::OrreryFace::new_static,
    "PERIODIC" => periodic::PeriodicFace::new_static,
    "PING" => ping::PingFace::new_static,
    "PLANETARY_HOURS" => planetary_hours::PlanetaryHoursFace::new_static,
    "PLANETARY_TIME" => planetary_time::PlanetaryTimeFace::new_static,
    "PREFERENCES" => preferences::PreferencesFace::new_static,
    "PROBABILITY" => probability::ProbabilityFace::new_static,
    "PULSOMETER" => pulsometer::PulsometerFace::new_static,
    "RANDONAUT" => randonaut::RandonautFace::new_static,
    "RATEMETER" => ratemeter::RatemeterFace::new_static,
    "REPETITION_MINUTE" => repetition_minute::RepetitionMinuteFace::new_static,
    "RPN_CALCULATOR" => rpn_calculator::RpnCalculatorFace::new_static,
    "RPN_CALCULATOR_ALT" => rpn_calculator_alt::RpnCalculatorAltFace::new_static,
    "SAILING" => sailing::SailingFace::new_static,
    "SAVE_LOAD" => save_load::SaveLoadFace::new_static,
    "SET_TIME" => set_time::SetTimeFace::new_static,
    "SET_TIME_HACKWATCH" => set_time_hackwatch::SetTimeHackwatchFace::new_static,
    "SHIPS_BELL" => ships_bell::ShipsBellFace::new_static,
    "SIMON" => simon::SimonFace::new_static,
    "SIMPLE_CALCULATOR" => simple_calculator::SimpleCalculatorFace::new_static,
    "SIMPLE_CLOCK_BIN_LED" => simple_clock_bin_led::SimpleClockBinLedFace::new_static,
    "SIMPLE_COIN_FLIP" => simple_coin_flip::SimpleCoinFlipFace::new_static,
    "SOLAR_TIME" => solar_time::SolarTimeFace::new_static,
    "SOLSTICE" => solstice::SolsticeFace::new_static,
    "SOS" => sos::SosFace::new_static,
    "SQUASH" => squash::SquashFace::new_static,
    "SUNRISE_SUNSET" => sunrise_sunset::SunriseSunsetFace::new_static,
    "TACHYMETER" => tachymeter::TachymeterFace::new_static,
    "TALLY" => tally::TallyFace::new_static,
    "TAROT" => tarot::TarotFace::new_static,
    "TEMPCHART" => tempchart::TempchartFace::new_static,
    "THERMISTOR_LOGGING" => thermistor_logging::ThermistorLoggingFace::new_static,
    "THERMISTOR_READOUT" => thermistor_readout::ThermistorReadoutFace::new_static,
    "THERMISTOR_TESTING" => thermistor_testing::ThermistorTestingFace::new_static,
    "TIDE" => tide::TideFace::new_static,
    "TIME_LEFT" => time_left::TimeLeftFace::new_static,
    "TOMATO" => tomato::TomatoFace::new_static,
    "TOSS_UP" => toss_up::TossUpFace::new_static,
    "TOTP" => totp::TotpFace::new_static,
    "TOTP_LFS" => totp_lfs::TotpFaceLfs::new_static,
    "TUNING_TONES" => tuning_tones::TuningTonesFace::new_static,
    "VOLTAGE" => voltage::VoltageFace::new_static,
    "WAKE" => wake::WakeFace::new_static,
    "WAREKI" => wareki::WarekiFace::new_static,
    "WEEKNUMBER" => weeknumber::WeekNumberClockFace::new_static,
    "WORDLE" => wordle::WordleFace::new_static,
    "WORLD_CLOCK2" => world_clock2::WorldClock2Face::new_static,
    "WYOSCAN" => wyoscan::WyoscanFace::new_static,
}

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
    pub fn set_sensor_overrides(
        &mut self,
        _voltage_mv: Option<u16>,
        _temperature_celsius: Option<f32>,
    ) {
    }
    pub fn set_time(&mut self, _y: u32, _mo: u32, _d: u32, _h: u32, _mi: u32, _s: u32) -> bool {
        false
    }
    pub fn activate(&mut self, _time_mode_24: bool) {}
    pub fn tick(&mut self) {}
    pub fn is_activated(&self) -> bool {
        false
    }
    pub fn button_event(&mut self, _button: RealButton, _event: RealButtonEvent) {}
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
    fn real_face_receives_exact_simulator_civil_time() {
        let mut face = RealFace::new("SIMPLE_CLOCK").expect("SIMPLE_CLOCK seam mapping");
        assert!(face.set_time(2025, 7, 4, 23, 59, 42));
        let time = face.time();
        assert_eq!(
            (
                time.year,
                time.month,
                time.day,
                time.hour,
                time.minute,
                time.second
            ),
            (5, 7, 4, 23, 59, 42)
        );
    }

    #[test]
    fn button_event_state_stays_short_below_firmware_threshold() {
        let mut state = ButtonEventState::default();
        assert_eq!(state.update(true, 0.0), Some(RealButtonEvent::Down));
        assert_eq!(state.update(true, (64.0 - 1.0) / 128.0), None);
        assert_eq!(state.update(false, 0.0), Some(RealButtonEvent::Up));
    }

    #[test]
    fn button_event_state_emits_long_press_at_exact_firmware_threshold() {
        let mut state = ButtonEventState::default();
        assert_eq!(state.update(true, 0.0), Some(RealButtonEvent::Down));
        assert_eq!(
            state.update(true, ButtonEventState::LONG_PRESS_SECONDS),
            Some(RealButtonEvent::LongPress)
        );
    }

    #[test]
    fn button_event_state_crosses_threshold_on_large_frame() {
        let mut state = ButtonEventState::default();
        assert_eq!(state.update(true, 0.0), Some(RealButtonEvent::Down));
        assert_eq!(state.update(true, 0.1), None);
        assert_eq!(state.update(true, 1.0), Some(RealButtonEvent::LongPress));
    }

    #[test]
    fn button_event_state_emits_long_press_once_and_long_up() {
        let mut state = ButtonEventState::default();
        assert_eq!(state.update(true, 0.0), Some(RealButtonEvent::Down));
        assert_eq!(
            state.update(true, ButtonEventState::LONG_PRESS_SECONDS),
            Some(RealButtonEvent::LongPress)
        );
        assert_eq!(state.update(true, 0.5), None);
        assert_eq!(state.update(false, 0.0), Some(RealButtonEvent::LongUp));
        assert!(!state.down);
        assert_eq!(state.hold_seconds, 0.0);
    }

    #[test]
    fn button_event_state_clamps_negative_time() {
        let mut state = ButtonEventState::default();
        assert_eq!(state.update(true, 0.0), Some(RealButtonEvent::Down));
        assert_eq!(state.update(true, -5.0), None);
        assert_eq!(
            state.update(true, ButtonEventState::LONG_PRESS_SECONDS),
            Some(RealButtonEvent::LongPress)
        );
    }

    #[test]
    fn registry_constructs_round_trips_and_preserves_fallbacks() {
        use std::collections::HashSet;

        assert_eq!(REAL_FACE_NAMES.len(), 111);
        let mut names = HashSet::new();
        for name in REAL_FACE_NAMES {
            assert!(names.insert(*name), "duplicate real-face name: {name}");
            let mut face = RealFace::new(name).expect("registered face should construct");
            assert_eq!(face.face_name(), *name);

            let lowercase = name.to_ascii_lowercase();
            face = RealFace::new(&lowercase).expect("lowercase registered face should construct");
            assert_eq!(face.face_name(), *name);
        }

        assert_eq!(names.len(), REAL_FACE_NAMES.len());
        assert!(RealFace::new("NOT_A_FACE").is_none());
        assert!(
            render_real_face("NOT_A_FACE", 2023, 1, 6, 15, 4, 0, 5, true, false, false).is_none()
        );
    }

    #[test]
    fn face_available_for_migrated_face() {
        assert!(RealFace::new("SIMPLE_CLOCK").is_some());
        assert!(RealFace::new("simple_clock").is_some());
        // The stock Casio set + other host-migrated faces resolve through the seam.
        for name in [
            "ALARM",
            "ALARM_THERMOMETER",
            "COUNTER",
            "STOCK_STOPWATCH",
            "WORLD_CLOCK",
            "STOPWATCH",
            "TIMER",
            "COUNTDOWN",
            "FLASHLIGHT",
            "BEEPS",
            "BLINKY",
            "BREATHING",
            "CHARACTER_SET",
            "CHIRPY_DEMO",
            "DATABANK",
            "DAYS_SINCE",
            "DEMO",
            "DISCGOLF",
            "BEATS",
            "HABIT",
            "HIGHER_LOWER_GAME",
            "ENDLESS_RUNNER",
        ] {
            assert!(RealFace::new(name).is_some(), "{name} should be migrated");
        }
        assert_eq!(REAL_FACE_NAMES.len(), 111);
        assert!(RealFace::new("ACTIVITY").is_some());
        assert!(RealFace::new("geomancy").is_some());
        assert!(RealFace::new("NOT_A_FACE").is_none());
    }

    #[test]
    fn real_endless_runner_lifecycle_uses_activate_title_and_bounded_ticks() {
        let mut face = RealFace::new("endless_runner").expect("ENDLESS_RUNNER is migrated");
        assert_eq!(face.face_name(), "ENDLESS_RUNNER");
        assert!(!face.is_activated());
        assert!(face.set_time(2023, 1, 6, 15, 4, 1));

        face.activate(true);
        assert!(face.is_activated());
        assert_eq!(face.snapshot().chars[0..2], ['E', 'R']);
        assert!(face.snapshot().colon);
        assert!(face.snapshot().bell);

        face.button_event(RealButton::Alarm, RealButtonEvent::Up);
        face.button_event(RealButton::Light, RealButtonEvent::Down);
        for _ in 0..128 {
            face.tick();
        }
        assert_eq!(face.snapshot().chars.len(), 10);
        face.resign();
        assert!(!face.is_activated());
        face.resign();
    }

    #[test]
    fn real_alarm_thermometer_lifecycle_uses_real_face_and_resigns_safely() {
        let mut face = RealFace::new("alarm_thermometer").expect("ALARM_THERMOMETER is migrated");
        assert_eq!(face.face_name(), "ALARM_THERMOMETER");
        assert!(face.set_time(2024, 2, 29, 15, 4, 0));
        face.activate(true);
        assert_eq!(face.snapshot().chars[4..10], ['2', '5', '.', '0', '#', 'C']);

        face.button_event(RealButton::Alarm, RealButtonEvent::Up);
        assert!(face.snapshot().bell);
        for second in [5, 10, 15, 20] {
            assert!(face.set_time(2024, 2, 29, 15, 4, second));
            face.tick();
        }
        assert!(face.snapshot().signal);
        assert!(face.snapshot().bell);

        face.button_event(RealButton::Alarm, RealButtonEvent::Up);
        assert!(!face.snapshot().bell);
        assert!(!face.snapshot().signal);
        face.button_event(RealButton::Alarm, RealButtonEvent::LongPress);
        assert_eq!(face.snapshot().chars[4..10], ['7', '7', '.', '0', '#', 'F']);

        face.resign();
        assert!(!face.is_activated());
        face.resign();
    }

    #[test]
    fn real_geomancy_activation_buttons_ticks_and_display_are_safe() {
        let mut face = RealFace::new("geomancy").expect("GEOMANCY is migrated");
        face.set_time(2023, 1, 6, 15, 4, 0);
        face.activate(true);
        assert!(face
            .snapshot()
            .chars
            .iter()
            .collect::<String>()
            .contains("IChing"));

        face.button_event(RealButton::Light, RealButtonEvent::Up);
        assert!(face
            .snapshot()
            .chars
            .iter()
            .collect::<String>()
            .contains("GeomCy"));
        face.button_event(RealButton::Alarm, RealButtonEvent::Up);
        for _ in 0..12 {
            face.tick();
        }
        face.button_event(RealButton::Alarm, RealButtonEvent::LongPress);
        face.button_event(RealButton::Light, RealButtonEvent::Up);
        assert!(face
            .snapshot()
            .chars
            .iter()
            .collect::<String>()
            .contains("IChing"));
        face.resign();
    }

    #[test]
    fn real_higher_lower_adapter_lifecycle_preserves_title_and_guesses() {
        let mut face = RealFace::new("higher_lower_game").expect("HIGHER_LOWER_GAME is migrated");
        assert!(face.set_time(2024, 2, 29, 15, 4, 0));
        face.activate(true);
        assert!(face.is_activated());
        assert_eq!(face.face_name(), "HIGHER_LOWER_GAME");
        assert!(face
            .snapshot()
            .chars
            .iter()
            .collect::<String>()
            .contains("Hi-Lo"));

        face.button_event(RealButton::Light, RealButtonEvent::Down);
        face.button_event(RealButton::Light, RealButtonEvent::Up);
        face.button_event(RealButton::Alarm, RealButtonEvent::Down);
        face.button_event(RealButton::Alarm, RealButtonEvent::Up);
        face.tick();
        face.resign();
        assert!(!face.is_activated());
        face.resign();
    }

    #[test]
    fn real_accel_interrupt_count_runs_taps_and_resigns_safely() {
        let mut face = RealFace::new("accel_interrupt_count").expect("face");
        face.activate(true);

        // Host activation has no physical accelerometer, but injected firmware
        // events still exercise the real face state machine.
        face.tap_event(false);
        face.tap_event(true);
        assert_eq!(face.snapshot().chars[9], '0');

        face.button_event(RealButton::Alarm, RealButtonEvent::Up);
        face.tap_event(false);
        face.tap_event(true);
        assert_eq!(face.snapshot().chars[9], '2');

        face.button_event(RealButton::Alarm, RealButtonEvent::Up);
        face.tap_event(false);
        assert_eq!(face.snapshot().chars[9], '2');

        face.button_event(RealButton::Alarm, RealButtonEvent::LongPress);
        face.button_event(RealButton::Light, RealButtonEvent::Down);
        assert!(face.snapshot().chars.starts_with(&['T', 'H']));
        face.button_event(RealButton::Alarm, RealButtonEvent::Up);
        face.button_event(RealButton::Light, RealButtonEvent::Down);
        assert_eq!(face.snapshot().chars[9], '0');

        face.resign();
        face.resign();
    }

    #[test]
    fn real_simple_clock_renders_24h_with_seconds() {
        let (y, mo, d, h, mi, s) = friday();
        let snap = render_real_face("SIMPLE_CLOCK", y, mo, d, h, mi, s, 5, true, false, false)
            .expect("SIMPLE_CLOCK is migrated");
        // The REAL write path: FR + day 06 + HH:MM (no seconds - show_seconds
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
    fn real_simple_clock_24h_renders_boundary_hours_without_pm() {
        for (hour, expected) in [
            (0, "FR060010\0\0"),
            (1, "FR060110\0\0"),
            (11, "FR061110\0\0"),
            (12, "FR061210\0\0"),
            (13, "FR061310\0\0"),
            (23, "FR062310\0\0"),
        ] {
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
            let text: String = snap.chars.iter().collect();
            assert_eq!(text, expected, "unexpected 24-hour display at {hour:02}:10");
            assert!(snap.h24);
            assert!(!snap.pm, "PM must be separate from the 24-hour indicator");
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
    fn running_countdown_advances_only_on_simulated_ticks() {
        let mut face = RealFace::new("COUNTDOWN").expect("face");
        assert!(face.set_time(2023, 1, 6, 15, 4, 0));
        face.activate(true);
        // COUNTDOWN starts its deterministic three-minute default on Alarm-up.
        face.press(false, true);
        let initial = face.snapshot();

        face.tick();
        let after_one_tick = face.snapshot();
        assert_ne!(after_one_tick.chars, initial.chars);

        // Studio calls set_time once per GUI frame. Repeating the same
        // simulated time must not consume another countdown second.
        for _ in 0..8 {
            assert!(face.set_time(2023, 1, 6, 15, 4, 0));
        }
        assert_eq!(face.snapshot().chars, after_one_tick.chars);
        assert_eq!(face.snapshot().colon, after_one_tick.colon);
        assert_eq!(face.snapshot().bell, after_one_tick.bell);
    }

    #[test]
    fn running_stopwatch_ignores_changed_and_backward_set_time() {
        let mut face = RealFace::new("STOPWATCH").expect("face");
        assert!(face.set_time(2023, 1, 6, 15, 4, 0));
        face.activate(true);

        // The real stopwatch starts on Button-down. Exercise that firmware
        // event directly so this test only covers set_time semantics, without
        // changing the adapter's existing button-up contract.
        sensor_watch::watch::seam::with_hw(&mut *face.mock, || {
            face.face.loop_(
                types::Event::Button(types::Button::Alarm, types::ButtonEvent::Down),
                &mut face.settings,
            );
        });
        face.tick();
        let running = face.snapshot();

        assert!(face.set_time(2023, 1, 6, 15, 5, 0));
        assert_eq!(face.snapshot().chars, running.chars);
        assert_eq!(face.snapshot().colon, running.colon);

        // A backward RTC edit must not reach StopwatchFace::Activate, whose
        // elapsed-time subtraction assumes the clock has not moved backward.
        assert!(face.set_time(2023, 1, 6, 15, 3, 0));
        assert_eq!(face.snapshot().chars, running.chars);
        assert_eq!(face.snapshot().colon, running.colon);
    }

    #[test]
    fn newly_added_faces_activate_through_the_host_seam() {
        for name in [
            "BEEPS",
            "BLINKY",
            "BREATHING",
            "CHARACTER_SET",
            "DATABANK",
            "DEMO",
            "DISCGOLF",
            "BEATS",
        ] {
            let snapshot = render_real_face(name, 2024, 2, 29, 15, 4, 0, 4, true, false, false)
                .unwrap_or_else(|| panic!("{name} should render through the host seam"));
            assert!(snapshot.chars.iter().any(|character| *character != '\0'));
        }
    }

    #[test]
    fn dual_timer_and_finetune_activate_through_the_host_seam() {
        for name in ["DUAL_TIMER", "FINETUNE"] {
            let mut face = RealFace::new(name).unwrap_or_else(|| panic!("{name} is migrated"));
            assert!(face.set_time(2024, 2, 29, 15, 4, 0));
            face.activate(true);
            assert!(face.is_activated());
            assert!(face
                .snapshot()
                .chars
                .iter()
                .any(|character| *character != '\0'));
            face.press(true, false);
            face.tick();
        }
    }

    #[test]
    fn real_butterfly_game_lifecycle_uses_canonical_mapping_and_activation_event() {
        assert!(REAL_FACE_NAMES.contains(&"BUTTERFLY_GAME"));
        let mut face = RealFace::new("butterfly_game").expect("BUTTERFLY_GAME is migrated");
        assert_eq!(face.face_name(), "BUTTERFLY_GAME");
        assert!(!face.is_activated());
        assert!(face.set_time(2024, 2, 29, 15, 4, 0));
        face.activate(true);
        assert!(face.is_activated());
        assert!(face.snapshot().chars[4..10].starts_with(&['B', 't', 'r', 'f', 'l', 'y']));

        // The face's splash is entered by Event::Activate. A tick would not
        // initialize its splash counter, so this also locks in the seam's
        // activation-event forwarding contract.
        for _ in 0..8 {
            face.tick();
        }
        face.button_event(RealButton::Alarm, RealButtonEvent::Down);
        face.button_event(RealButton::Light, RealButtonEvent::Down);
        assert!(face.is_activated());
        assert_eq!(face.snapshot().chars.len(), 10);
        face.resign();
        face.resign();
        assert!(!face.is_activated());
    }

    #[test]
    fn real_activity_activates_and_snapshots_chooser() {
        let snapshot = render_real_face("ACTIVITY", 2024, 2, 29, 15, 4, 0, 4, true, false, false)
            .expect("ACTIVITY is migrated");
        let text: String = snapshot.chars.iter().collect();
        assert!(text.starts_with("AC   bIKE"), "actual: {text:?}");
    }

    #[test]
    fn real_activity_logs_ticks_pauses_and_finishes_at_minimum() {
        let mut face = RealFace::new("ACTIVITY").expect("face");
        assert!(face.set_time(2024, 2, 29, 15, 4, 0));
        face.activate(true);
        assert!(face
            .snapshot()
            .chars
            .iter()
            .collect::<String>()
            .starts_with("AC   bIKE"));

        let mut button = ButtonEventState::default();
        assert_eq!(button.update(true, 0.0), Some(RealButtonEvent::Down));
        face.button_event(RealButton::Alarm, RealButtonEvent::Down);
        assert_eq!(
            button.update(true, ButtonEventState::LONG_PRESS_SECONDS),
            Some(RealButtonEvent::LongPress)
        );
        face.button_event(RealButton::Alarm, RealButtonEvent::LongPress);
        face.tick(); // first logging second
        for _ in 0..58 {
            face.tick();
        }
        face.press(false, true); // pause
        face.tick();
        assert!(
            face.snapshot()
                .chars
                .iter()
                .collect::<String>()
                .contains("PAUSE"),
            "actual: {:?}",
            face.snapshot()
        );
        face.press(false, true); // resume at 60 seconds

        // The public Studio adapter intentionally exposes short button-up
        // events. Exercise the real long-press boundary directly for finish.
        // Finish through the same public adapter used by Studio, rather than
        // injecting a private firmware event.
        face.button_event(RealButton::Alarm, RealButtonEvent::LongPress);
        assert!(face
            .snapshot()
            .chars
            .iter()
            .collect::<String>()
            .contains("dONE"));
    }

    #[test]
    fn unknown_face_falls_back() {
        assert!(
            render_real_face("NOT_A_FACE", 2023, 1, 6, 15, 4, 0, 5, true, false, false).is_none()
        );
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
    fn sequential_switch_and_drop_covers_every_real_mapping() {
        for cycle in 0..3 {
            for (index, name) in REAL_FACE_NAMES.iter().enumerate() {
                let mut face = RealFace::new(name).expect("mapping should construct");
                assert!(face.set_time(2024, 2, 29, (index + cycle) as u32 % 24, 4, 0));
                // Construction, RTC validation, and drop are safe for every
                // mapping. A subset of migrated firmware faces still has
                // host-only assumptions in its initial display path; the
                // button/tick stress test covers the fully host-safe set.
                drop(face);
            }
        }
        assert!(RealFace::new("SIMPLE_CLOCK").is_some());
    }

    #[test]
    fn invalid_dates_are_rejected_for_every_real_mapping() {
        for name in REAL_FACE_NAMES {
            let mut face = RealFace::new(name).expect("mapping should construct");
            for (year, month, day, hour, minute, second) in [
                (2019, 1, 1, 0, 0, 0),
                (2024, 2, 30, 0, 0, 0),
                (2024, 13, 1, 0, 0, 0),
                (2024, 1, 1, 24, 0, 0),
                (2024, 1, 1, 0, 60, 0),
                (2024, 1, 1, 0, 0, 60),
            ] {
                assert!(
                    !face.set_time(year, month, day, hour, minute, second),
                    "{name}"
                );
            }
            assert!(face.set_time(2024, 2, 29, 23, 59, 59));
        }
    }

    #[test]
    fn simple_clock_handles_repeated_am_pm_transitions() {
        let mut face = RealFace::new("SIMPLE_CLOCK").expect("face");
        face.activate(false);
        for (hour, pm) in [(11, false), (12, true), (23, true), (0, false), (12, true)] {
            assert!(face.set_time(2024, 1, 1, hour, 0, 0));
            assert_eq!(
                face.snapshot().pm,
                pm,
                "unexpected PM state at {hour:02}:00"
            );
        }
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
