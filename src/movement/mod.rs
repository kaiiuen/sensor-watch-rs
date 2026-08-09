//! Movement framework core.
//!
//! An event-driven, interrupt-powered dispatcher. The CPU is a start/stop
//! resource: it wakes only to react to a single event, then immediately
//! returns to STANDBY. All timekeeping is owned by the RTC, never by the CPU.

pub mod accel_interrupt_count;
pub mod accelerometer_data_acquisition;
pub mod activity;
pub mod advanced_alarm;
pub mod alarm;
pub mod alarm_thermometer;
pub mod astronomy;
pub mod baby_kicks;
pub mod battery;
pub mod beats;
pub mod beeps;
pub mod blackjack;
pub mod blinky;
pub mod board;
pub mod breathing;
pub mod butterfly_game;
pub mod character_set;
pub mod chirpy_demo;
pub mod close_enough;
pub mod couch_to_5k;
pub mod countdown;
pub mod counter;
pub mod databank;
pub mod day_night_percentage;
pub mod day_one;
pub mod days_since;
pub mod deadline;
pub mod debounce;
pub mod decimal_time;
pub mod demo;
pub mod diagnostics;
pub mod discgolf;
pub mod dual_timer;
pub mod endless_runner;
pub mod fault;
pub mod finetune;
pub mod flashlight;
pub mod french_revolutionary;
pub mod frequency_correction;
pub mod geomancy;
pub mod habit;
pub mod hello_there;
pub mod higher_lower_game;
pub mod hydration;
pub mod interval;
pub mod invaders;
pub mod ish;
pub mod ke_decimal_time;
pub mod kitchen_conversions;
pub mod lander;
pub mod lightmeter;
pub mod lis2dw_logging;
pub mod mars_time;
pub mod menstrual_cycle;
pub mod metronome;
pub mod minimal_clock;
pub mod minmax;
pub mod minute_repeater_decimal;
pub mod moon_phase;
pub mod morsecalc;
pub mod nanosec;
pub mod orrery;
pub mod periodic;
pub mod persist;
pub mod ping;
pub mod planetary_hours;
pub mod planetary_time;
pub mod preferences;
pub mod probability;
pub mod pulsometer;
pub mod randonaut;
pub mod ratemeter;
pub mod repetition_minute;
pub mod rpn_calculator;
pub mod rpn_calculator_alt;
pub mod sailing;
pub mod save_load;
pub mod set_time;
pub mod set_time_hackwatch;
pub mod settings_face;
pub mod ships_bell;
pub mod simon;
pub mod simple_calculator;
pub mod simple_clock;
pub mod simple_clock_bin_led;
pub mod simple_coin_flip;
pub mod solar_time;
pub mod solstice;
pub mod sos;
pub mod squash;
pub mod stats;
pub mod stock_stopwatch;
pub mod stopwatch;
pub mod sunrise_sunset;
pub mod tachymeter;
pub mod tally;
pub mod tarot;
pub mod tempchart;
pub mod thermistor_logging;
pub mod thermistor_readout;
pub mod thermistor_testing;
pub mod tide;
pub mod time_left;
pub mod timer;
pub mod tomato;
pub mod toss_up;
pub mod totp;
pub mod totp_lfs;
pub mod tuning_tones;
pub mod types;
pub mod voltage;
pub mod wake;
pub mod wareki;
pub mod weeknumber;
pub mod wordle;
pub mod world_clock;
pub mod world_clock2;
pub mod wyoscan;

use crate::movement::types::*;
use crate::watch;
use crate::watch::buzzer::{self, Note as BuzzerNote};
use crate::watch::rtc::{self, DateTime};
use crate::watch::utility;

/// The global movement state.
pub static mut MOVEMENT_STATE: MovementState = MovementState::new_static();

/// The list of watch faces. Faces are static instances — there is no heap, so
/// nothing can be allocated and nothing can grow.
pub static mut WATCH_FACES: [Option<&'static mut dyn WatchFace>; MOVEMENT_NUM_FACES] =
    [const { None }; MOVEMENT_NUM_FACES];

/// The static simple clock face instance.
#[used]
static mut SIMPLE_CLOCK: simple_clock::SimpleClockFace =
    simple_clock::SimpleClockFace::new_static();

/// The static countdown face instance.
#[used]
static mut COUNTDOWN: countdown::CountdownFace = countdown::CountdownFace::new_static();

/// The static alarm face instance.
#[used]
static mut ALARM: alarm::AlarmFace = alarm::AlarmFace::new_static();

/// The static advanced alarm face instance.
#[used]
static mut ADVANCED_ALARM: advanced_alarm::AdvancedAlarmFace =
    advanced_alarm::AdvancedAlarmFace::new_static();

/// The static counter face instance.
#[used]
static mut COUNTER: counter::CounterFace = counter::CounterFace::new_static();

/// The static world clock face instance.
#[used]
static mut WORLD_CLOCK: world_clock::WorldClockFace = world_clock::WorldClockFace::new_static();

/// The static world clock 2 face instance.
#[used]
static mut WORLD_CLOCK2: world_clock2::WorldClock2Face =
    world_clock2::WorldClock2Face::new_static();

/// The static simple clock bin LED face instance.
#[used]
static mut SIMPLE_CLOCK_BIN_LED: simple_clock_bin_led::SimpleClockBinLedFace =
    simple_clock_bin_led::SimpleClockBinLedFace::new_static();

/// The static minute repeater decimal face instance.
#[used]
static mut MINUTE_REPEATER_DECIMAL: minute_repeater_decimal::MinuteRepeaterDecimalFace =
    minute_repeater_decimal::MinuteRepeaterDecimalFace::new_static();

/// The static day/night percentage face instance.
#[used]
static mut DAY_NIGHT_PERCENTAGE: day_night_percentage::DayNightPercentageFace =
    day_night_percentage::DayNightPercentageFace::new_static();

/// The static set time face instance.
#[used]
static mut SET_TIME: set_time::SetTimeFace = set_time::SetTimeFace::new_static();

/// The static preferences face instance.
#[used]
static mut PREFERENCES: preferences::PreferencesFace = preferences::PreferencesFace::new_static();

/// The static finetune face instance.
#[used]
static mut FINETUNE: finetune::FinetuneFace = finetune::FinetuneFace::new_static();

/// The static save/load face instance.
#[used]
static mut SAVE_LOAD: save_load::SaveLoadFace = save_load::SaveLoadFace::new_static();

/// The static nanosec face instance.
#[used]
static mut NANOSEC: nanosec::NanosecFace = nanosec::NanosecFace::new_static();

/// The static set time hackwatch face instance.
#[used]
static mut SET_TIME_HACKWATCH: set_time_hackwatch::SetTimeHackwatchFace =
    set_time_hackwatch::SetTimeHackwatchFace::new_static();

/// The static voltage face instance.
#[used]
static mut VOLTAGE: voltage::VoltageFace = voltage::VoltageFace::new_static();

/// The static hello there face instance.
#[used]
static mut HELLO_THERE: hello_there::HelloThereFace = hello_there::HelloThereFace::new_static();

/// The static character set face instance.
#[used]
static mut CHARACTER_SET: character_set::CharacterSetFace =
    character_set::CharacterSetFace::new_static();

/// The static beeps face instance.
#[used]
static mut BEEPS: beeps::BeepsFace = beeps::BeepsFace::new_static();

/// The static demo face instance.
#[used]
static mut DEMO: demo::DemoFace = demo::DemoFace::new_static();

/// The static frequency correction face instance.
#[used]
static mut FREQUENCY_CORRECTION: frequency_correction::FrequencyCorrectionFace =
    frequency_correction::FrequencyCorrectionFace::new_static();

/// The static chirpy demo face instance.
#[used]
static mut CHIRPY_DEMO: chirpy_demo::ChirpyDemoFace = chirpy_demo::ChirpyDemoFace::new_static();

/// The static LIS2DW logging face instance.
#[used]
static mut LIS2DW_LOGGING: lis2dw_logging::Lis2dwLoggingFace =
    lis2dw_logging::Lis2dwLoggingFace::new_static();

/// The static thermistor readout face instance.
#[used]
static mut THERMISTOR_READOUT: thermistor_readout::ThermistorReadoutFace =
    thermistor_readout::ThermistorReadoutFace::new_static();

/// The static min/max face instance.
#[used]
static mut MINMAX: minmax::MinmaxFace = minmax::MinmaxFace::new_static();

/// The static lightmeter face instance.
#[used]
static mut LIGHTMETER: lightmeter::LightmeterFace = lightmeter::LightmeterFace::new_static();

/// The static thermistor logging face instance.
#[used]
static mut THERMISTOR_LOGGING: thermistor_logging::ThermistorLoggingFace =
    thermistor_logging::ThermistorLoggingFace::new_static();

/// The static thermistor testing face instance.
#[used]
static mut THERMISTOR_TESTING: thermistor_testing::ThermistorTestingFace =
    thermistor_testing::ThermistorTestingFace::new_static();

/// The static alarm thermometer face instance.
#[used]
static mut ALARM_THERMOMETER: alarm_thermometer::AlarmThermometerFace =
    alarm_thermometer::AlarmThermometerFace::new_static();

/// The static accelerometer data acquisition face instance.
#[used]
static mut ACCELEROMETER_DATA_ACQUISITION:
    accelerometer_data_acquisition::AccelerometerDataAcquisitionFace =
    accelerometer_data_acquisition::AccelerometerDataAcquisitionFace::new_static();

/// The static accel interrupt count face instance.
#[used]
static mut ACCEL_INTERRUPT_COUNT: accel_interrupt_count::AccelInterruptCountFace =
    accel_interrupt_count::AccelInterruptCountFace::new_static();

/// The static diagnostics face instance.
#[used]
static mut DIAGNOSTICS: diagnostics::DiagnosticsFace = diagnostics::DiagnosticsFace::new_static();

/// The static flashlight face instance.
#[used]
static mut FLASHLIGHT: flashlight::FlashlightFace = flashlight::FlashlightFace::new_static();

/// The static decimal time face instance.
#[used]
static mut DECIMAL_TIME: decimal_time::DecimalTimeFace =
    decimal_time::DecimalTimeFace::new_static();

/// The static week number clock face instance.
#[used]
static mut WEEKNUMBER: weeknumber::WeekNumberClockFace =
    weeknumber::WeekNumberClockFace::new_static();

/// The static minimal clock face instance.
#[used]
static mut MINIMAL_CLOCK: minimal_clock::MinimalClockFace =
    minimal_clock::MinimalClockFace::new_static();

/// The static blinky face instance.
#[used]
static mut BLINKY: blinky::BlinkyFace = blinky::BlinkyFace::new_static();

/// The static tally face instance.
#[used]
static mut TALLY: tally::TallyFace = tally::TallyFace::new_static();

/// The static ships bell face instance.
#[used]
static mut SHIPS_BELL: ships_bell::ShipsBellFace = ships_bell::ShipsBellFace::new_static();

/// The static close-enough clock face instance.
#[used]
static mut CLOSE_ENOUGH: close_enough::CloseEnoughClockFace =
    close_enough::CloseEnoughClockFace::new_static();

/// The static moon phase face instance.
#[used]
static mut MOON_PHASE: moon_phase::MoonPhaseFace = moon_phase::MoonPhaseFace::new_static();

/// The static stopwatch face instance.
#[used]
static mut STOPWATCH: stopwatch::StopwatchFace = stopwatch::StopwatchFace::new_static();

/// The static timer face instance.
#[used]
static mut TIMER: timer::TimerFace = timer::TimerFace::new_static();

/// The static French Revolutionary face instance.
#[used]
static mut FRENCH_REVOLUTIONARY: french_revolutionary::FrenchRevolutionaryFace =
    french_revolutionary::FrenchRevolutionaryFace::new_static();

/// The static Mars time face instance.
#[used]
static mut MARS_TIME: mars_time::MarsTimeFace = mars_time::MarsTimeFace::new_static();

/// The static sailing face instance.
#[used]
static mut SAILING: sailing::SailingFace = sailing::SailingFace::new_static();

/// The static metronome face instance.
#[used]
static mut METRONOME: metronome::MetronomeFace = metronome::MetronomeFace::new_static();

/// The static tachymeter face instance.
#[used]
static mut TACHYMETER: tachymeter::TachymeterFace = tachymeter::TachymeterFace::new_static();

/// The static pulsometer face instance.
#[used]
static mut PULSOMETER: pulsometer::PulsometerFace = pulsometer::PulsometerFace::new_static();

/// The static ratemeter face instance.
#[used]
static mut RATEMETER: ratemeter::RatemeterFace = ratemeter::RatemeterFace::new_static();

/// The static probability face instance.
#[used]
static mut PROBABILITY: probability::ProbabilityFace = probability::ProbabilityFace::new_static();

/// The static simple coin flip face instance.
#[used]
static mut SIMPLE_COIN_FLIP: simple_coin_flip::SimpleCoinFlipFace =
    simple_coin_flip::SimpleCoinFlipFace::new_static();

/// The static toss-up face instance.
#[used]
static mut TOSS_UP: toss_up::TossUpFace = toss_up::TossUpFace::new_static();

/// The static databank face instance.
#[used]
static mut DATABANK: databank::DatabankFace = databank::DatabankFace::new_static();

/// The static habit face instance.
#[used]
static mut HABIT: habit::HabitFace = habit::HabitFace::new_static();

/// The static tomato face instance.
#[used]
static mut TOMATO: tomato::TomatoFace = tomato::TomatoFace::new_static();

/// The static deadline face instance.
#[used]
static mut DEADLINE: deadline::DeadlineFace = deadline::DeadlineFace::new_static();

/// The static breathing face instance.
#[used]
static mut BREATHING: breathing::BreathingFace = breathing::BreathingFace::new_static();

/// The static periodic table face instance.
#[used]
static mut PERIODIC: periodic::PeriodicFace = periodic::PeriodicFace::new_static();

/// The static tuning tones face instance.
#[used]
static mut TUNING_TONES: tuning_tones::TuningTonesFace =
    tuning_tones::TuningTonesFace::new_static();

/// The static wake face instance.
#[used]
static mut WAKE: wake::WakeFace = wake::WakeFace::new_static();

/// The static kitchen conversions face instance.
#[used]
static mut KITCHEN_CONVERSIONS: kitchen_conversions::KitchenConversionsFace =
    kitchen_conversions::KitchenConversionsFace::new_static();

/// The static wareki face instance.
#[used]
static mut WAREKI: wareki::WarekiFace = wareki::WarekiFace::new_static();

/// The static tarot face instance.
#[used]
static mut TAROT: tarot::TarotFace = tarot::TarotFace::new_static();

/// The static randonaut face instance.
#[used]
static mut RANDONAUT: randonaut::RandonautFace = randonaut::RandonautFace::new_static();

/// The static day one face instance.
#[used]
static mut DAY_ONE: day_one::DayOneFace = day_one::DayOneFace::new_static();

/// The static time left face instance.
#[used]
static mut TIME_LEFT: time_left::TimeLeftFace = time_left::TimeLeftFace::new_static();

/// The static disc golf face instance.
#[used]
static mut DISCGOLF: discgolf::DiscgolfFace = discgolf::DiscgolfFace::new_static();

/// The static menstrual cycle face instance.
#[used]
static mut MENSTRUAL_CYCLE: menstrual_cycle::MenstrualCycleFace =
    menstrual_cycle::MenstrualCycleFace::new_static();

/// The static butterfly game face instance.
#[used]
static mut BUTTERFLY_GAME: butterfly_game::ButterflyGameFace =
    butterfly_game::ButterflyGameFace::new_static();

/// The static simon face instance.
#[used]
static mut SIMON: simon::SimonFace = simon::SimonFace::new_static();

/// The static invaders face instance.
#[used]
static mut INVADERS: invaders::InvadersFace = invaders::InvadersFace::new_static();

/// The static higher/lower game face instance.
#[used]
static mut HIGHER_LOWER_GAME: higher_lower_game::HigherLowerGameFace =
    higher_lower_game::HigherLowerGameFace::new_static();

/// The static endless runner face instance.
#[used]
static mut ENDLESS_RUNNER: endless_runner::EndlessRunnerFace =
    endless_runner::EndlessRunnerFace::new_static();

/// The static geomancy face instance.
#[used]
static mut GEOMANCY: geomancy::GeomancyFace = geomancy::GeomancyFace::new_static();

/// The static repetition minute face instance.
#[used]
static mut REPETITION_MINUTE: repetition_minute::RepetitionMinuteFace =
    repetition_minute::RepetitionMinuteFace::new_static();

/// The static wyoscan face instance.
#[used]
static mut WYOSCAN: wyoscan::WyoscanFace = wyoscan::WyoscanFace::new_static();

/// The static couch to 5k face instance.
#[used]
static mut COUCH_TO_5K: couch_to_5k::CouchTo5kFace = couch_to_5k::CouchTo5kFace::new_static();

/// The static simple calculator face instance.
#[used]
static mut SIMPLE_CALCULATOR: simple_calculator::SimpleCalculatorFace =
    simple_calculator::SimpleCalculatorFace::new_static();

/// The static RPN calculator face instance.
#[used]
static mut RPN_CALCULATOR: rpn_calculator::RpnCalculatorFace =
    rpn_calculator::RpnCalculatorFace::new_static();

/// The static TOTP face instance.
#[used]
static mut TOTP: totp::TotpFace = totp::TotpFace::new_static();

/// The static stock stopwatch face instance.
#[used]
static mut STOCK_STOPWATCH: stock_stopwatch::StockStopwatchFace =
    stock_stopwatch::StockStopwatchFace::new_static();

/// The static activity face instance.
#[used]
static mut ACTIVITY: activity::ActivityFace = activity::ActivityFace::new_static();

/// The static hydration face instance.
#[used]
static mut HYDRATION: hydration::HydrationFace = hydration::HydrationFace::new_static();

/// The static interval face instance.
#[used]
static mut INTERVAL: interval::IntervalFace = interval::IntervalFace::new_static();

/// The static TOTP LFS face instance.
#[used]
static mut TOTP_LFS: totp_lfs::TotpFaceLfs = totp_lfs::TotpFaceLfs::new_static();

/// The static wordle face instance.
#[used]
static mut WORDLE: wordle::WordleFace = wordle::WordleFace::new_static();

/// The static planetary time face instance.
#[used]
static mut PLANETARY_TIME: planetary_time::PlanetaryTimeFace =
    planetary_time::PlanetaryTimeFace::new_static();

/// The static planetary hours face instance.
#[used]
static mut PLANETARY_HOURS: planetary_hours::PlanetaryHoursFace =
    planetary_hours::PlanetaryHoursFace::new_static();

/// The static sunrise/sunset face instance.
#[used]
static mut SUNRISE_SUNSET: sunrise_sunset::SunriseSunsetFace =
    sunrise_sunset::SunriseSunsetFace::new_static();

/// The static astronomy face instance.
#[used]
static mut ASTRONOMY: astronomy::AstronomyFace = astronomy::AstronomyFace::new_static();

/// The static orrery face instance.
#[used]
static mut ORRERY: orrery::OrreryFace = orrery::OrreryFace::new_static();

/// The static solstice face instance.
#[used]
static mut SOLSTICE: solstice::SolsticeFace = solstice::SolsticeFace::new_static();

/// The static SOS face instance.
#[used]
static mut SOS: sos::SosFace = sos::SosFace::new_static();

/// The static days since face instance.
#[used]
static mut DAYS_SINCE: days_since::DaysSinceFace = days_since::DaysSinceFace::new_static();

/// The static tide face instance.
#[used]
static mut TIDE: tide::TideFace = tide::TideFace::new_static();

/// The static blackjack face instance.
#[used]
static mut BLACKJACK: blackjack::BlackjackFace = blackjack::BlackjackFace::new_static();

/// The static squash face instance.
#[used]
static mut SQUASH: squash::SquashFace = squash::SquashFace::new_static();

/// The static lander face instance.
#[used]
static mut LANDER: lander::LanderFace = lander::LanderFace::new_static();

/// The static ping face instance.
#[used]
static mut PING: ping::PingFace = ping::PingFace::new_static();

/// The static baby kicks face instance.
#[used]
static mut BABY_KICKS: baby_kicks::BabyKicksFace = baby_kicks::BabyKicksFace::new_static();

/// The static settings face instance.
#[used]
static mut SETTINGS_FACE: settings_face::SettingsFace = settings_face::SettingsFace::new_static();

/// The static morsecalc face instance.
#[used]
static mut MORSECALC: morsecalc::MorsecalcFace = morsecalc::MorsecalcFace::new_static();

/// The static tempchart face instance.
#[used]
static mut TEMPCHART: tempchart::TempchartFace = tempchart::TempchartFace::new_static();

/// The static dual timer face instance.
#[used]
static mut DUAL_TIMER: dual_timer::DualTimerFace = dual_timer::DualTimerFace::new_static();

/// The static RPN calculator alt face instance.
#[used]
static mut RPN_CALCULATOR_ALT: rpn_calculator_alt::RpnCalculatorAltFace =
    rpn_calculator_alt::RpnCalculatorAltFace::new_static();

/// The static ISH (vague time) face instance.
#[used]
static mut ISH: ish::IshFace = ish::IshFace::new_static();

/// The static solar time face instance.
#[used]
static mut SOLAR_TIME: solar_time::SolarTimeFace = solar_time::SolarTimeFace::new_static();

/// The static Kè decimal time face instance.
#[used]
static mut KE_DECIMAL_TIME: ke_decimal_time::KeDecimalTimeFace =
    ke_decimal_time::KeDecimalTimeFace::new_static();

/// The static beats face instance.
#[used]
static mut BEATS: beats::BeatsFace = beats::BeatsFace::new_static();

/// Forces the linker to retain every face's vtable and methods.
///
/// The faces are stored as `static mut` and referenced only through
/// `addr_of_mut!` in `app_setup`. The linker's `--gc-sections` cannot see these
/// raw-pointer references, so it strips the faces' code and vtables (leaving a
/// nearly-empty firmware). Referencing every face from a `#[used]` static makes
/// the retention explicit and keeps the whole framework in the binary.
#[used]
static mut FACE_RETAIN: [&dyn WatchFace; MOVEMENT_NUM_FACES] = [
    unsafe { &*core::ptr::addr_of_mut!(SIMPLE_CLOCK) },
    unsafe { &*core::ptr::addr_of_mut!(COUNTDOWN) },
    unsafe { &*core::ptr::addr_of_mut!(ALARM) },
    unsafe { &*core::ptr::addr_of_mut!(ADVANCED_ALARM) },
    unsafe { &*core::ptr::addr_of_mut!(COUNTER) },
    unsafe { &*core::ptr::addr_of_mut!(WORLD_CLOCK) },
    unsafe { &*core::ptr::addr_of_mut!(WORLD_CLOCK2) },
    unsafe { &*core::ptr::addr_of_mut!(SIMPLE_CLOCK_BIN_LED) },
    unsafe { &*core::ptr::addr_of_mut!(DIAGNOSTICS) },
    unsafe { &*core::ptr::addr_of_mut!(FLASHLIGHT) },
    unsafe { &*core::ptr::addr_of_mut!(DECIMAL_TIME) },
    unsafe { &*core::ptr::addr_of_mut!(WEEKNUMBER) },
    unsafe { &*core::ptr::addr_of_mut!(MINIMAL_CLOCK) },
    unsafe { &*core::ptr::addr_of_mut!(BLINKY) },
    unsafe { &*core::ptr::addr_of_mut!(TALLY) },
    unsafe { &*core::ptr::addr_of_mut!(SHIPS_BELL) },
    unsafe { &*core::ptr::addr_of_mut!(CLOSE_ENOUGH) },
    unsafe { &*core::ptr::addr_of_mut!(MOON_PHASE) },
    unsafe { &*core::ptr::addr_of_mut!(STOPWATCH) },
    unsafe { &*core::ptr::addr_of_mut!(TIMER) },
    unsafe { &*core::ptr::addr_of_mut!(FRENCH_REVOLUTIONARY) },
    unsafe { &*core::ptr::addr_of_mut!(MARS_TIME) },
    unsafe { &*core::ptr::addr_of_mut!(SAILING) },
    unsafe { &*core::ptr::addr_of_mut!(METRONOME) },
    unsafe { &*core::ptr::addr_of_mut!(TACHYMETER) },
    unsafe { &*core::ptr::addr_of_mut!(PULSOMETER) },
    unsafe { &*core::ptr::addr_of_mut!(RATEMETER) },
    unsafe { &*core::ptr::addr_of_mut!(PROBABILITY) },
    unsafe { &*core::ptr::addr_of_mut!(SIMPLE_COIN_FLIP) },
    unsafe { &*core::ptr::addr_of_mut!(TOSS_UP) },
    unsafe { &*core::ptr::addr_of_mut!(DATABANK) },
    unsafe { &*core::ptr::addr_of_mut!(HABIT) },
    unsafe { &*core::ptr::addr_of_mut!(TOMATO) },
    unsafe { &*core::ptr::addr_of_mut!(DEADLINE) },
    unsafe { &*core::ptr::addr_of_mut!(BREATHING) },
    unsafe { &*core::ptr::addr_of_mut!(PERIODIC) },
    unsafe { &*core::ptr::addr_of_mut!(TUNING_TONES) },
    unsafe { &*core::ptr::addr_of_mut!(WAKE) },
    unsafe { &*core::ptr::addr_of_mut!(KITCHEN_CONVERSIONS) },
    unsafe { &*core::ptr::addr_of_mut!(WAREKI) },
    unsafe { &*core::ptr::addr_of_mut!(TAROT) },
    unsafe { &*core::ptr::addr_of_mut!(RANDONAUT) },
    unsafe { &*core::ptr::addr_of_mut!(DAY_ONE) },
    unsafe { &*core::ptr::addr_of_mut!(TIME_LEFT) },
    unsafe { &*core::ptr::addr_of_mut!(DISCGOLF) },
    unsafe { &*core::ptr::addr_of_mut!(MENSTRUAL_CYCLE) },
    unsafe { &*core::ptr::addr_of_mut!(BUTTERFLY_GAME) },
    unsafe { &*core::ptr::addr_of_mut!(SIMON) },
    unsafe { &*core::ptr::addr_of_mut!(INVADERS) },
    unsafe { &*core::ptr::addr_of_mut!(HIGHER_LOWER_GAME) },
    unsafe { &*core::ptr::addr_of_mut!(ENDLESS_RUNNER) },
    unsafe { &*core::ptr::addr_of_mut!(GEOMANCY) },
    unsafe { &*core::ptr::addr_of_mut!(REPETITION_MINUTE) },
    unsafe { &*core::ptr::addr_of_mut!(WYOSCAN) },
    unsafe { &*core::ptr::addr_of_mut!(COUCH_TO_5K) },
    unsafe { &*core::ptr::addr_of_mut!(SIMPLE_CALCULATOR) },
    unsafe { &*core::ptr::addr_of_mut!(RPN_CALCULATOR) },
    unsafe { &*core::ptr::addr_of_mut!(TOTP) },
    unsafe { &*core::ptr::addr_of_mut!(STOCK_STOPWATCH) },
    unsafe { &*core::ptr::addr_of_mut!(ACTIVITY) },
    unsafe { &*core::ptr::addr_of_mut!(INTERVAL) },
    unsafe { &*core::ptr::addr_of_mut!(TOTP_LFS) },
    unsafe { &*core::ptr::addr_of_mut!(WORDLE) },
    unsafe { &*core::ptr::addr_of_mut!(PLANETARY_TIME) },
    unsafe { &*core::ptr::addr_of_mut!(PLANETARY_HOURS) },
    unsafe { &*core::ptr::addr_of_mut!(SUNRISE_SUNSET) },
    unsafe { &*core::ptr::addr_of_mut!(ASTRONOMY) },
    unsafe { &*core::ptr::addr_of_mut!(ORRERY) },
    unsafe { &*core::ptr::addr_of_mut!(SOLSTICE) },
    unsafe { &*core::ptr::addr_of_mut!(MORSECALC) },
    unsafe { &*core::ptr::addr_of_mut!(TEMPCHART) },
    unsafe { &*core::ptr::addr_of_mut!(DUAL_TIMER) },
    unsafe { &*core::ptr::addr_of_mut!(RPN_CALCULATOR_ALT) },
    unsafe { &*core::ptr::addr_of_mut!(MINUTE_REPEATER_DECIMAL) },
    unsafe { &*core::ptr::addr_of_mut!(DAY_NIGHT_PERCENTAGE) },
    unsafe { &*core::ptr::addr_of_mut!(SET_TIME) },
    unsafe { &*core::ptr::addr_of_mut!(PREFERENCES) },
    unsafe { &*core::ptr::addr_of_mut!(FINETUNE) },
    unsafe { &*core::ptr::addr_of_mut!(SAVE_LOAD) },
    unsafe { &*core::ptr::addr_of_mut!(NANOSEC) },
    unsafe { &*core::ptr::addr_of_mut!(SET_TIME_HACKWATCH) },
    unsafe { &*core::ptr::addr_of_mut!(VOLTAGE) },
    unsafe { &*core::ptr::addr_of_mut!(HELLO_THERE) },
    unsafe { &*core::ptr::addr_of_mut!(CHARACTER_SET) },
    unsafe { &*core::ptr::addr_of_mut!(BEEPS) },
    unsafe { &*core::ptr::addr_of_mut!(DEMO) },
    unsafe { &*core::ptr::addr_of_mut!(FREQUENCY_CORRECTION) },
    unsafe { &*core::ptr::addr_of_mut!(CHIRPY_DEMO) },
    unsafe { &*core::ptr::addr_of_mut!(LIS2DW_LOGGING) },
    unsafe { &*core::ptr::addr_of_mut!(THERMISTOR_READOUT) },
    unsafe { &*core::ptr::addr_of_mut!(MINMAX) },
    unsafe { &*core::ptr::addr_of_mut!(LIGHTMETER) },
    unsafe { &*core::ptr::addr_of_mut!(THERMISTOR_LOGGING) },
    unsafe { &*core::ptr::addr_of_mut!(THERMISTOR_TESTING) },
    unsafe { &*core::ptr::addr_of_mut!(ALARM_THERMOMETER) },
    unsafe { &*core::ptr::addr_of_mut!(ACCELEROMETER_DATA_ACQUISITION) },
    unsafe { &*core::ptr::addr_of_mut!(ACCEL_INTERRUPT_COUNT) },
    unsafe { &*core::ptr::addr_of_mut!(HYDRATION) },
    unsafe { &*core::ptr::addr_of_mut!(SOS) },
    unsafe { &*core::ptr::addr_of_mut!(LANDER) },
    unsafe { &*core::ptr::addr_of_mut!(PING) },
    unsafe { &*core::ptr::addr_of_mut!(BABY_KICKS) },
    unsafe { &*core::ptr::addr_of_mut!(SETTINGS_FACE) },
    unsafe { &*core::ptr::addr_of_mut!(ISH) },
    unsafe { &*core::ptr::addr_of_mut!(SOLAR_TIME) },
    unsafe { &*core::ptr::addr_of_mut!(KE_DECIMAL_TIME) },
    unsafe { &*core::ptr::addr_of_mut!(BEATS) },
    unsafe { &*core::ptr::addr_of_mut!(DAYS_SINCE) },
    unsafe { &*core::ptr::addr_of_mut!(TIDE) },
    unsafe { &*core::ptr::addr_of_mut!(BLACKJACK) },
    unsafe { &*core::ptr::addr_of_mut!(SQUASH) },
];

/// Scheduled background tasks per face (packed RTC time).
pub static mut SCHEDULED_TASKS: [u32; MOVEMENT_NUM_FACES] = [0; MOVEMENT_NUM_FACES];

/// The pending event that woke the CPU.
pub static mut PENDING_EVENT: Event = Event::Tick;

/// A fast-tick counter (128 Hz) used for long-press detection.
pub static mut FAST_TICKS: u16 = 0;

/// Set by the per-minute alarm so the main loop runs the all-face background
/// task pass (`handle_background_tasks`) once per minute, in main context.
pub static mut RUN_BACKGROUND_TASKS: bool = false;

/// The serial command shell.
pub static mut SHELL: watch::shell::Shell = watch::shell::Shell::new_static();

/// Handles background tasks for all faces.
fn handle_background_tasks() {
    unsafe {
        // First, give every face a chance to advise the framework of its needs
        // (alarms, background work, DST sensitivity). This runs once per minute.
        for face in WATCH_FACES.iter_mut() {
            if let Some(face) = face.as_deref_mut() {
                face.advise(&MOVEMENT_STATE.settings);
            }
        }
        for face in WATCH_FACES.iter_mut() {
            if let Some(face) = face.as_deref_mut()
                && face.wants_background_task(&MOVEMENT_STATE.settings)
            {
                face.loop_(Event::BackgroundTask, &mut MOVEMENT_STATE.settings);
            }
        }
    }
}

/// Handles scheduled background tasks.
fn handle_scheduled_tasks() {
    unsafe {
        let date_time = rtc::get_date_time();
        for (i, task) in SCHEDULED_TASKS.iter_mut().enumerate() {
            if *task != 0 && *task <= date_time.to_reg() {
                *task = 0;
                if let Some(face) = WATCH_FACES[i].as_deref_mut() {
                    face.loop_(Event::BackgroundTask, &mut MOVEMENT_STATE.settings);
                }
            }
        }
    }
}

/// Illuminates the LED.
pub fn illuminate_led() {
    // In the brown-out safe state, the LED is disabled to avoid the load that
    // re-triggers the reboot loop.
    if crate::movement::fault::in_safe_state() {
        return;
    }
    unsafe {
        let s = &MOVEMENT_STATE.settings;
        if s.led_duration() != 0b111 {
            let red = if s.led_red_color() != 0 {
                0xF | (s.led_red_color() << 4)
            } else {
                0
            };
            let green = if s.led_green_color() != 0 {
                0xF | (s.led_green_color() << 4)
            } else {
                0
            };
            watch::led::set_led_color(red, green);
        }
    }
}

/// The default button handler: mode advances faces, light illuminates.
pub fn default_loop_handler(event: Event, _settings: &Settings) {
    match event {
        Event::Button(Button::Mode, ButtonEvent::Up) => move_to_next_face(),
        Event::Button(Button::Light, ButtonEvent::Down) => illuminate_led(),
        Event::Button(Button::Light, ButtonEvent::Up)
        | Event::Button(Button::Light, ButtonEvent::LongUp) => {
            if unsafe { MOVEMENT_STATE.settings.led_duration() } == 0 {
                force_led_off();
            }
        }
        Event::Button(Button::Mode, ButtonEvent::LongPress) => {
            // Long-press Mode: if on face 0 and a secondary list exists, jump to
            // it; otherwise jump back to face 0.
            let secondary = MOVEMENT_SECONDARY_FACE_INDEX;
            if secondary != 0 && unsafe { MOVEMENT_STATE.current_face_idx } == 0 {
                move_to_face(secondary);
            } else {
                move_to_face(0);
            }
        }
        _ => {}
    }
}

/// Saves the current settings to flash so they survive a reset.
///
/// Faces should call this after changing any setting.
pub fn save_settings() {
    unsafe {
        let reg = MOVEMENT_STATE.settings.reg;
        // Only hit flash when settings actually changed. This avoids wearing
        // out the EEPROM area by rewriting identical settings on every wake.
        if reg != MOVEMENT_STATE.last_saved_settings_reg {
            MOVEMENT_STATE.last_saved_settings_reg = reg;
            persist::save(&MOVEMENT_STATE.settings);
        }
    }
}

/// Moves to the given watch face.
pub fn move_to_face(watch_face_index: usize) {
    unsafe {
        MOVEMENT_STATE.watch_face_changed = true;
        MOVEMENT_STATE.next_face_idx = watch_face_index;
    }
}

/// Moves to the next watch face.
///
/// If a secondary face list is configured, rotation is bounded to the current
/// list: primary faces (0..SECONDARY) cycle within the primary list, and
/// secondary faces (SECONDARY..NUM) cycle within the secondary list.
pub fn move_to_next_face() {
    unsafe {
        let secondary = MOVEMENT_SECONDARY_FACE_INDEX;
        let face_max = if secondary != 0 && MOVEMENT_STATE.current_face_idx < secondary {
            secondary
        } else {
            MOVEMENT_NUM_FACES
        };
        move_to_face((MOVEMENT_STATE.current_face_idx + 1) % face_max);
    }
}

/// Schedules a background task for the current face.
pub fn schedule_background_task(date_time: DateTime) {
    unsafe {
        schedule_background_task_for_face(MOVEMENT_STATE.current_face_idx, date_time);
    }
}

/// Cancels the background task for the current face.
pub fn cancel_background_task() {
    unsafe {
        cancel_background_task_for_face(MOVEMENT_STATE.current_face_idx);
    }
}

/// Schedules a background task for a specific face.
pub fn schedule_background_task_for_face(watch_face_index: usize, date_time: DateTime) {
    unsafe {
        let now = rtc::get_date_time();
        if date_time.to_reg() > now.to_reg() {
            SCHEDULED_TASKS[watch_face_index] = date_time.to_reg();
        }
    }
}

/// Cancels the background task for a specific face.
pub fn cancel_background_task_for_face(watch_face_index: usize) {
    unsafe {
        SCHEDULED_TASKS[watch_face_index] = 0;
    }
}

/// Plays the signal tune.
pub fn play_signal() {
    // In the brown-out safe state, the buzzer is disabled to avoid re-triggering
    // the reboot loop.
    if crate::movement::fault::in_safe_state() {
        return;
    }
    unsafe {
        MOVEMENT_STATE.is_buzzing = true;
        buzzer::enable_buzzer();
    }
}

/// Plays a single note with the given priority (0 = button, 1 = signal, 2 = alarm).
///
/// A lower-priority note does not interrupt a higher-priority one.
pub fn play_note(note: BuzzerNote, priority: u8) {
    if crate::movement::fault::in_safe_state() {
        return;
    }
    if (priority as u8) < unsafe { MOVEMENT_STATE.pending_sequence_priority as u8 } {
        return;
    }
    // Store the priority as the current pending priority.
    unsafe {
        MOVEMENT_STATE.pending_sequence_priority = match priority {
            0 => BuzzerPriority::Button,
            1 => BuzzerPriority::Signal,
            _ => BuzzerPriority::Alarm,
        };
    }
    buzzer::set_buzzer_period(crate::watch::buzzer::NOTE_PERIODS[note as usize] as u32);
    buzzer::set_buzzer_on();
}

/// Plays the alarm.
pub fn play_alarm() {
    play_alarm_beeps(5, BuzzerNote::C8);
}

/// Plays alarm beeps.
pub fn play_alarm_beeps(_rounds: u8, alarm_note: BuzzerNote) {
    if crate::movement::fault::in_safe_state() {
        return;
    }
    unsafe {
        MOVEMENT_STATE.alarm_note = alarm_note;
        MOVEMENT_STATE.is_buzzing = true;
        MOVEMENT_STATE.pending_sequence_priority = BuzzerPriority::Alarm;
    }
    buzzer::enable_buzzer();
}

/// Plays a note sequence (a pointer to (note, duration) pairs ending in 0).
///
/// The sequence plays at signal priority, so it cannot be cancelled by a button
/// note but can be interrupted by an alarm.
pub fn play_sequence(note_sequence: *const i8, _callback_on_end: Option<fn()>) {
    if crate::movement::fault::in_safe_state() {
        return;
    }
    if (BuzzerPriority::Signal as u8) < unsafe { MOVEMENT_STATE.pending_sequence_priority as u8 } {
        return;
    }
    unsafe { MOVEMENT_STATE.pending_sequence_priority = BuzzerPriority::Signal };
    buzzer::play_sequence(note_sequence, _callback_on_end);
}

/// Claims a backup register (4-7).
pub fn claim_backup_register() -> u8 {
    unsafe {
        if MOVEMENT_STATE.next_available_backup_register >= 8 {
            return 0;
        }
        let reg = MOVEMENT_STATE.next_available_backup_register;
        MOVEMENT_STATE.next_available_backup_register += 1;
        reg
    }
}

/// Returns the current UTC date/time (the RTC stores UTC; no offset applied).
pub fn get_utc_date_time() -> DateTime {
    rtc::get_date_time()
}

/// Returns the local date/time by applying the configured time zone offset.
pub fn get_local_date_time() -> DateTime {
    let utc = rtc::get_date_time();
    let offset = get_current_timezone_offset();
    utility::date_time_convert_zone(utc, 0, (offset * 60) as u32)
}

/// Returns the date/time in the given zone index (minutes offset from UTC).
pub fn get_date_time_in_zone(zone_index: u8) -> DateTime {
    let utc = rtc::get_date_time();
    let offset = get_current_timezone_offset_for_zone(zone_index);
    utility::date_time_convert_zone(utc, 0, (offset * 60) as u32)
}

/// Returns the current UTC timestamp (seconds since 1970).
pub fn get_utc_timestamp() -> u32 {
    utility::date_time_to_unix_time(rtc::get_date_time(), 0)
}

/// Sets the UTC date/time and reschedules alarms.
pub fn set_utc_date_time(date_time: DateTime) {
    rtc::set_date_time(date_time);
}

/// Sets the local date/time (converts back to UTC using the configured zone).
pub fn set_local_date_time(date_time: DateTime) {
    let offset = get_current_timezone_offset();
    let utc = utility::date_time_convert_zone(date_time, (offset * 60) as u32, 0);
    rtc::set_date_time(utc);
}

/// Sets the UTC timestamp (seconds since 1970).
pub fn set_utc_timestamp(timestamp: u32) {
    rtc::set_date_time(utility::date_time_from_unix_time(timestamp, 0));
}

/// Returns the current time zone index.
pub fn get_timezone_index() -> u8 {
    unsafe { MOVEMENT_STATE.settings.time_zone() }
}

/// Sets the time zone index and persists the setting.
pub fn set_timezone_index(index: u8) {
    unsafe {
        MOVEMENT_STATE.settings.set_time_zone(index);
        save_settings();
    }
}

/// Returns the current time zone offset (minutes) for the configured zone.
pub fn get_current_timezone_offset() -> i32 {
    let idx = get_timezone_index();
    get_current_timezone_offset_for_zone(idx)
}

/// Returns the time zone offset (minutes) for a given zone index, applying DST.
pub fn get_current_timezone_offset_for_zone(zone_index: u8) -> i32 {
    let idx = (zone_index as usize).min(watch::zones::NUM_ZONE_NAMES - 1);
    let defn = &watch::zones::ZONE_DEFNS[idx];
    if defn.rules_len == 0 {
        // No DST rules: return the standard offset.
        return defn.offset_inc_minutes as i32 * watch::utz::OFFSET_INCREMENT * 60;
    }
    // Compute the DST-aware offset for the current UTC time.
    let utc = rtc::get_date_time();
    let utc_ts = utility::date_time_to_unix_time(utc, 0);
    let base_offset = defn.offset_inc_minutes as i32 * watch::utz::OFFSET_INCREMENT;
    // Convert UTC to the zone's local time (using standard offset first).
    let local = utility::date_time_from_unix_time(utc_ts, (base_offset * 60) as u32);
    let udt = date_time_to_udate(local);
    let mut zone = watch::utz::UZone {
        name: "",
        offset: watch::utz::UOffset {
            hours: (base_offset / 60) as i8,
            minutes: (base_offset % 60).unsigned_abs() as u8,
        },
        rules: &watch::zones::ZONE_RULES
            [defn.rules_idx as usize..defn.rules_idx as usize + defn.rules_len as usize],
        abrev_formatter: "",
        src: core::ptr::null(),
    };
    let mut offset = watch::utz::UOffset::default();
    watch::utz::get_current_offset(&mut zone, &udt, &mut offset);
    (offset.hours as i32 * 60 + offset.minutes as i32) * 60
}

/// Converts a `DateTime` to the utz `UDateTime` format.
fn date_time_to_udate(dt: DateTime) -> watch::utz::UDateTime {
    watch::utz::UDateTime {
        date: watch::utz::UDate {
            year: dt.year,
            month: dt.month,
            dayofmonth: dt.day,
            dayofweek: 0,
        },
        time: watch::utz::UTime {
            hour: dt.hour,
            minute: dt.minute,
            second: dt.second,
        },
    }
}

/// Returns the clock mode as a 12H/24H/024H enum.
pub fn clock_mode_24h() -> ClockMode {
    unsafe {
        if MOVEMENT_STATE.settings.clock_mode_24h() {
            if MOVEMENT_STATE.settings.clock_24h_leading_zero() {
                ClockMode::H024
            } else {
                ClockMode::H24
            }
        } else {
            ClockMode::H12
        }
    }
}

/// Sets the clock mode from a 12H/24H/024H enum.
pub fn set_clock_mode_24h(mode: ClockMode) {
    unsafe {
        let (h24, leading_zero) = match mode {
            ClockMode::H12 => (false, false),
            ClockMode::H24 => (true, false),
            ClockMode::H024 => (true, true),
        };
        MOVEMENT_STATE.settings.set_clock_mode_24h(h24);
        MOVEMENT_STATE
            .settings
            .set_clock_24h_leading_zero(leading_zero);
        save_settings();
    }
}

/// Returns whether the button should sound.
pub fn button_should_sound() -> bool {
    unsafe { MOVEMENT_STATE.settings.button_should_sound() }
}

/// Sets whether the button should sound.
pub fn set_button_should_sound(v: bool) {
    unsafe {
        MOVEMENT_STATE.settings.set_button_should_sound(v);
        save_settings();
    }
}

/// Returns the button volume (false = soft, true = loud).
pub fn button_volume() -> bool {
    unsafe { MOVEMENT_STATE.settings.button_volume() }
}

/// Sets the button volume.
pub fn set_button_volume(v: bool) {
    unsafe {
        MOVEMENT_STATE.settings.set_button_volume(v);
        save_settings();
    }
}

/// Returns the signal volume (false = soft, true = loud).
pub fn signal_volume() -> bool {
    unsafe { MOVEMENT_STATE.settings.signal_volume() }
}

/// Sets the signal volume.
pub fn set_signal_volume(v: bool) {
    unsafe {
        MOVEMENT_STATE.settings.set_signal_volume(v);
        save_settings();
    }
}

/// Returns the alarm volume (false = soft, true = loud).
pub fn alarm_volume() -> bool {
    unsafe { MOVEMENT_STATE.settings.alarm_volume() }
}

/// Sets the alarm volume.
pub fn set_alarm_volume(v: bool) {
    unsafe {
        MOVEMENT_STATE.settings.set_alarm_volume(v);
        save_settings();
    }
}

/// Returns the fast-tick timeout (inactivity interval) setting.
pub fn get_fast_tick_timeout() -> u8 {
    unsafe { MOVEMENT_STATE.settings.to_interval() }
}

/// Sets the fast-tick timeout.
pub fn set_fast_tick_timeout(v: u8) {
    unsafe {
        MOVEMENT_STATE.settings.set_to_interval(v);
        save_settings();
    }
}

/// Returns the low-energy timeout setting.
pub fn get_low_energy_timeout() -> u8 {
    unsafe { MOVEMENT_STATE.settings.le_interval() }
}

/// Sets the low-energy timeout.
pub fn set_low_energy_timeout(v: u8) {
    unsafe {
        MOVEMENT_STATE.settings.set_le_interval(v);
        save_settings();
    }
}

/// Returns whether at least one alarm is enabled.
pub fn alarm_enabled() -> bool {
    unsafe { MOVEMENT_STATE.settings.alarm_enabled() }
}

/// Sets the global alarm-enabled flag.
pub fn set_alarm_enabled(v: bool) {
    unsafe {
        MOVEMENT_STATE.settings.set_alarm_enabled(v);
        save_settings();
    }
}

/// Returns the backlight dwell (LED duration) setting.
pub fn get_backlight_dwell() -> u8 {
    unsafe { MOVEMENT_STATE.settings.led_duration() }
}

/// Sets the backlight dwell.
pub fn set_backlight_dwell(v: u8) {
    unsafe {
        MOVEMENT_STATE.settings.set_led_duration(v);
        save_settings();
    }
}

/// Returns the backlight color.
pub fn backlight_color() -> (u8, u8, u8) {
    unsafe {
        (
            MOVEMENT_STATE.settings.led_red_color(),
            MOVEMENT_STATE.settings.led_green_color(),
            0,
        )
    }
}

/// Sets the backlight color.
pub fn set_backlight_color(red: u8, green: u8, blue: u8) {
    unsafe {
        MOVEMENT_STATE.settings.set_led_red_color(red);
        MOVEMENT_STATE.settings.set_led_green_color(green);
        let _ = blue;
        save_settings();
    }
}

/// Forces the LED on with an arbitrary RGB color.
pub fn force_led_on(red: u8, green: u8, blue: u8) {
    // In the brown-out safe state, the LED is dimmed to avoid the load that
    // re-triggers the reboot loop.
    if crate::movement::fault::in_safe_state() {
        return;
    }
    watch::led::enable_leds();
    watch::led::set_led_color_rgb(red, green, blue);
}

/// Forces the LED off.
pub fn force_led_off() {
    watch::led::disable_leds();
}

/// Requests a change in the tick frequency (power of two, 1-128 Hz).
pub fn request_tick_frequency(freq: u8) {
    if freq.is_power_of_two() && (1..=128).contains(&freq) {
        set_tick_rate(freq != 1);
    }
}

/// Requests the watch to enter sleep mode (low-energy).
pub fn request_sleep() {
    watch::deepsleep::enter_sleep_mode();
}

/// Requests the watch to wake from sleep.
pub fn request_wake() {
    // Waking is handled by the interrupt that woke us; nothing to do here.
}

/// Timeout callback indices (matching Second Movement's timeout indices).
#[repr(u8)]
pub enum TimeoutIndex {
    LightButton = 0,
    ModeButton = 1,
    AlarmButton = 2,
    Led = 3,
    Resign = 4,
    Sleep = 5,
    Minute = 6,
}

/// Registers a compare callback at the given timeout index and target time.
pub fn register_timeout(index: TimeoutIndex, target: DateTime) {
    watch::rtc::register_comp_callback(compare_timeout_dispatcher, target.to_reg(), index as usize);
}

/// Disables a compare callback at the given timeout index.
pub fn disable_timeout(index: TimeoutIndex) {
    watch::rtc::disable_comp_callback(index as usize);
}

/// A single compare-callback dispatcher that routes to the timeout handler.
///
/// Each timeout index stores the same dispatcher; the timeout index is derived
/// from which slot fired via the per-slot target check inside the RTC queue.
fn compare_timeout_dispatcher() {
    // The RTC compare queue already fired the slot's callback; map it to an
    // event. All indexed timeouts converge to a generic wakeup.
    unsafe {
        PENDING_EVENT = Event::BackgroundTask;
    }
}

/// Stores the current settings to flash.
pub fn store_settings() {
    save_settings();
}

/// Applies a crystal drift correction (parts per million) to the RTC.
///
/// The RTC frequency-correction register compensates for crystal drift. A
/// positive value slows the clock; a negative value speeds it up. This is the
/// same mechanism the finetune face uses, exposed here so the companion app or
/// a calibration routine can set it.
pub fn apply_drift_correction(ppm: i16) {
    // The SAM L22 FREQCORR register: value 0-127 with a sign bit. Each step
    // corrects roughly 0.95 ppm at 1 kHz. Clamp to the valid range.
    let clamped = ppm.clamp(-127, 127);
    let sign = if clamped < 0 { 1 } else { 0 };
    let value = clamped.unsigned_abs();
    rtc::freqcorr_write(value as i16, sign);
}

/// Returns the current drift correction in parts per million.
pub fn get_drift_correction() -> i16 {
    rtc::freqcorr_read()
}

/// Detects and enables the accelerometer on the 9-pin connector.
///
/// Returns true if a LIS2DW is present. When present, the watch can use
/// tap detection and wake-on-motion.
pub fn accelerometer_begin() -> bool {
    watch::i2c::enable_i2c();
    let present = watch::lis2dw::begin();
    if !present {
        watch::i2c::disable_i2c();
    }
    present
}

/// Enables tap detection if an accelerometer is available.
pub fn enable_tap_detection_if_available() -> bool {
    if !accelerometer_begin() {
        return false;
    }
    watch::lis2dw::configure_tap_threshold(12, watch::lis2dw::TAP_THS_Z_Z_AXIS_ENABLE);
    watch::lis2dw::configure_tap_duration(2, 2, 2);
    watch::lis2dw::set_low_noise_mode(true);
    watch::lis2dw::set_data_rate(watch::lis2dw::DataRate::H400);
    watch::lis2dw::set_mode(watch::lis2dw::Mode::LowPower);
    watch::lis2dw::enable_double_tap();
    watch::lis2dw::configure_int1(
        watch::lis2dw::CTRL4_INT1_SINGLE_TAP | watch::lis2dw::CTRL4_INT1_DOUBLE_TAP,
    );
    watch::lis2dw::enable_interrupts();
    // Route the accelerometer INT1 (on A4) to the EIC so a tap wakes the CPU.
    watch::extint::register_interrupt_callback(
        watch::extint::A4,
        accelerometer_interrupt,
        watch::extint::Trigger::Rising,
    );
    true
}

/// The accelerometer INT1 interrupt handler.
pub fn accelerometer_interrupt() {
    handle_accelerometer_event();
}

/// Disables tap detection.
pub fn disable_tap_detection_if_available() -> bool {
    if !accelerometer_begin() {
        return false;
    }
    watch::lis2dw::set_low_noise_mode(false);
    watch::lis2dw::set_data_rate(watch::lis2dw::DataRate::Lowest);
    watch::lis2dw::set_mode(watch::lis2dw::Mode::LowPower);
    watch::lis2dw::disable_double_tap();
    watch::lis2dw::configure_tap_threshold(0, 0);
    watch::lis2dw::disable_interrupts();
    true
}

/// Polls the accelerometer interrupt source and reports tap events.
///
/// Called from the accelerometer interrupt. Sets the pending event for a
/// single or double tap.
pub fn handle_accelerometer_event() {
    let int_src = watch::lis2dw::get_interrupt_source();
    if int_src & watch::lis2dw::INTERRUPT_SRC_DOUBLE_TAP != 0 {
        unsafe { PENDING_EVENT = Event::DoubleTap };
    } else if int_src & watch::lis2dw::INTERRUPT_SRC_SINGLE_TAP != 0 {
        unsafe { PENDING_EVENT = Event::SingleTap };
    }
}

/// Returns the accelerometer background data rate.
pub fn get_accelerometer_background_rate() -> watch::lis2dw::DataRate {
    watch::lis2dw::get_data_rate()
}

/// Sets the accelerometer background data rate.
pub fn set_accelerometer_background_rate(rate: watch::lis2dw::DataRate) {
    watch::lis2dw::set_data_rate(rate);
}

/// Returns the accelerometer motion threshold.
pub fn get_accelerometer_motion_threshold() -> u8 {
    watch::lis2dw::get_wakeup_threshold()
}

/// Sets the accelerometer motion threshold and enables wake-on-motion.
pub fn set_accelerometer_motion_threshold(threshold: u8) {
    watch::lis2dw::configure_wakeup_threshold(threshold);
}

/// Returns the temperature from the accelerometer (or 0 if absent).
pub fn get_temperature() -> f32 {
    if !accelerometer_begin() {
        return 0.0;
    }
    watch::lis2dw::get_temperature() as f32
}

/// App init: called once at boot.
pub fn app_init() {
    unsafe {
        MOVEMENT_STATE = MovementState::new();

        // Load persisted settings, or apply defaults on first boot.
        if let Some(saved) = persist::load() {
            MOVEMENT_STATE.settings = saved;
        } else {
            MOVEMENT_STATE.settings.set_clock_mode_24h(false);
            MOVEMENT_STATE.settings.set_led_red_color(0x0);
            MOVEMENT_STATE.settings.set_led_green_color(0xF);
            MOVEMENT_STATE.settings.set_button_should_sound(true);
            MOVEMENT_STATE.settings.set_to_interval(0);
            MOVEMENT_STATE.settings.set_le_interval(2);
            MOVEMENT_STATE.settings.set_led_duration(1);
            // First boot: apply a default frequency-correction baseline. On
            // later boots we do NOT touch FREQCORR, so any crystal calibration
            // (finetune face / drift correction) survives a reset.
            rtc::freqcorr_write(22, 0);
        }
        // Remember what we loaded so the dirty-check in save_settings doesn't
        // rewrite identical settings on the first wake.
        MOVEMENT_STATE.last_saved_settings_reg = MOVEMENT_STATE.settings.reg;
        MOVEMENT_STATE.next_available_backup_register = 4;
    }
}

/// Sets the wake rate based on whether seconds are displayed.
///
/// - Seconds shown: wake once per second (1 Hz tick).
/// - Seconds hidden: wake once per minute (power-saving). The RTC alarm
///   fires at :00 each minute to advance the clock.
pub fn set_tick_rate(show_seconds: bool) {
    // Always keep the 128 Hz fast tick for long-press detection.
    if show_seconds {
        rtc::register_tick_callback(cb_tick);
    } else {
        rtc::disable_tick_callback();
        // Schedule a wake at the top of each minute.
        let now = rtc::get_date_time();
        let mut target = now;
        target.second = 0;
        target.minute = (target.minute + 1) % 60;
        rtc::schedule_wakeup(cb_tick, target);
    }
}

/// App setup: called when entering the foreground.
pub fn app_setup() {
    unsafe {
        watch::deepsleep::store_backup_data(MOVEMENT_STATE.settings.reg, 0);

        // Set up the 1-minute alarm for background tasks.
        let alarm_time = DateTime {
            second: 59,
            minute: 0,
            hour: 0,
            day: 0,
            month: 0,
            year: 0,
        };
        rtc::register_alarm_callback(cb_alarm_fired, alarm_time, rtc::AlarmMatch::Ss);

        // Register a fast tick for long-press detection.
        rtc::register_periodic_callback(cb_fast_tick, 128);

        // Set the wake rate based on the seconds-display setting.
        set_tick_rate(MOVEMENT_STATE.settings.show_seconds());

        // Register the button interrupts.
        watch::extint::enable_external_interrupts();
        watch::extint::register_interrupt_callback(
            watch::extint::BTN_MODE,
            cb_mode_btn_interrupt,
            watch::extint::Trigger::Both,
        );
        watch::extint::register_interrupt_callback(
            watch::extint::BTN_LIGHT,
            cb_light_btn_interrupt,
            watch::extint::Trigger::Both,
        );
        watch::extint::register_interrupt_callback(
            watch::extint::BTN_ALARM,
            cb_alarm_btn_interrupt,
            watch::extint::Trigger::Both,
        );

        watch::slcd::enable_display();

        // Enable the debug UART for the serial shell (TX on A4, RX on A2).
        watch::uart::enable_uart(Some(watch::extint::A4), Some(watch::extint::A2), 9600);

        // Detect an optional accelerometer on the 9-pin connector. If present,
        // tap detection and wake-on-motion are available to faces.
        let has_accel = accelerometer_begin();
        let _ = has_accel;

        // Register the watch faces (static instances, no heap).
        if WATCH_FACES[0].is_none() {
            WATCH_FACES[0] = Some(&mut *core::ptr::addr_of_mut!(SIMPLE_CLOCK));
            WATCH_FACES[1] = Some(&mut *core::ptr::addr_of_mut!(COUNTDOWN));
            WATCH_FACES[2] = Some(&mut *core::ptr::addr_of_mut!(ALARM));
            WATCH_FACES[3] = Some(&mut *core::ptr::addr_of_mut!(COUNTER));
            WATCH_FACES[4] = Some(&mut *core::ptr::addr_of_mut!(WORLD_CLOCK));
            WATCH_FACES[5] = Some(&mut *core::ptr::addr_of_mut!(DIAGNOSTICS));
            WATCH_FACES[6] = Some(&mut *core::ptr::addr_of_mut!(FLASHLIGHT));
            WATCH_FACES[7] = Some(&mut *core::ptr::addr_of_mut!(DECIMAL_TIME));
            WATCH_FACES[8] = Some(&mut *core::ptr::addr_of_mut!(WEEKNUMBER));
            WATCH_FACES[9] = Some(&mut *core::ptr::addr_of_mut!(MINIMAL_CLOCK));
            WATCH_FACES[10] = Some(&mut *core::ptr::addr_of_mut!(BLINKY));
            WATCH_FACES[11] = Some(&mut *core::ptr::addr_of_mut!(TALLY));
            WATCH_FACES[12] = Some(&mut *core::ptr::addr_of_mut!(SHIPS_BELL));
            WATCH_FACES[13] = Some(&mut *core::ptr::addr_of_mut!(CLOSE_ENOUGH));
            WATCH_FACES[14] = Some(&mut *core::ptr::addr_of_mut!(MOON_PHASE));
            WATCH_FACES[15] = Some(&mut *core::ptr::addr_of_mut!(STOPWATCH));
            WATCH_FACES[16] = Some(&mut *core::ptr::addr_of_mut!(TIMER));
            WATCH_FACES[17] = Some(&mut *core::ptr::addr_of_mut!(FRENCH_REVOLUTIONARY));
            WATCH_FACES[18] = Some(&mut *core::ptr::addr_of_mut!(MARS_TIME));
            WATCH_FACES[19] = Some(&mut *core::ptr::addr_of_mut!(SAILING));
            WATCH_FACES[20] = Some(&mut *core::ptr::addr_of_mut!(METRONOME));
            WATCH_FACES[21] = Some(&mut *core::ptr::addr_of_mut!(TACHYMETER));
            WATCH_FACES[22] = Some(&mut *core::ptr::addr_of_mut!(PULSOMETER));
            WATCH_FACES[23] = Some(&mut *core::ptr::addr_of_mut!(RATEMETER));
            WATCH_FACES[24] = Some(&mut *core::ptr::addr_of_mut!(PROBABILITY));
            WATCH_FACES[25] = Some(&mut *core::ptr::addr_of_mut!(SIMPLE_COIN_FLIP));
            WATCH_FACES[26] = Some(&mut *core::ptr::addr_of_mut!(TOSS_UP));
            WATCH_FACES[27] = Some(&mut *core::ptr::addr_of_mut!(DATABANK));
            WATCH_FACES[28] = Some(&mut *core::ptr::addr_of_mut!(HABIT));
            WATCH_FACES[29] = Some(&mut *core::ptr::addr_of_mut!(TOMATO));
            WATCH_FACES[30] = Some(&mut *core::ptr::addr_of_mut!(DEADLINE));
            WATCH_FACES[31] = Some(&mut *core::ptr::addr_of_mut!(BREATHING));
            WATCH_FACES[32] = Some(&mut *core::ptr::addr_of_mut!(PERIODIC));
            WATCH_FACES[33] = Some(&mut *core::ptr::addr_of_mut!(TUNING_TONES));
            WATCH_FACES[34] = Some(&mut *core::ptr::addr_of_mut!(WAKE));
            WATCH_FACES[35] = Some(&mut *core::ptr::addr_of_mut!(KITCHEN_CONVERSIONS));
            WATCH_FACES[36] = Some(&mut *core::ptr::addr_of_mut!(WAREKI));
            WATCH_FACES[37] = Some(&mut *core::ptr::addr_of_mut!(TAROT));
            WATCH_FACES[38] = Some(&mut *core::ptr::addr_of_mut!(RANDONAUT));
            WATCH_FACES[39] = Some(&mut *core::ptr::addr_of_mut!(DAY_ONE));
            WATCH_FACES[40] = Some(&mut *core::ptr::addr_of_mut!(TIME_LEFT));
            WATCH_FACES[41] = Some(&mut *core::ptr::addr_of_mut!(DISCGOLF));
            WATCH_FACES[42] = Some(&mut *core::ptr::addr_of_mut!(MENSTRUAL_CYCLE));
            WATCH_FACES[43] = Some(&mut *core::ptr::addr_of_mut!(BUTTERFLY_GAME));
            WATCH_FACES[44] = Some(&mut *core::ptr::addr_of_mut!(SIMON));
            WATCH_FACES[45] = Some(&mut *core::ptr::addr_of_mut!(INVADERS));
            WATCH_FACES[46] = Some(&mut *core::ptr::addr_of_mut!(HIGHER_LOWER_GAME));
            WATCH_FACES[47] = Some(&mut *core::ptr::addr_of_mut!(ENDLESS_RUNNER));
            WATCH_FACES[48] = Some(&mut *core::ptr::addr_of_mut!(GEOMANCY));
            WATCH_FACES[49] = Some(&mut *core::ptr::addr_of_mut!(REPETITION_MINUTE));
            WATCH_FACES[50] = Some(&mut *core::ptr::addr_of_mut!(WYOSCAN));
            WATCH_FACES[51] = Some(&mut *core::ptr::addr_of_mut!(COUCH_TO_5K));
            WATCH_FACES[52] = Some(&mut *core::ptr::addr_of_mut!(SIMPLE_CALCULATOR));
            WATCH_FACES[53] = Some(&mut *core::ptr::addr_of_mut!(RPN_CALCULATOR));
            WATCH_FACES[54] = Some(&mut *core::ptr::addr_of_mut!(TOTP));
            WATCH_FACES[55] = Some(&mut *core::ptr::addr_of_mut!(STOCK_STOPWATCH));
            WATCH_FACES[56] = Some(&mut *core::ptr::addr_of_mut!(ACTIVITY));
            WATCH_FACES[57] = Some(&mut *core::ptr::addr_of_mut!(INTERVAL));
            WATCH_FACES[58] = Some(&mut *core::ptr::addr_of_mut!(TOTP_LFS));
            WATCH_FACES[59] = Some(&mut *core::ptr::addr_of_mut!(WORDLE));
            WATCH_FACES[60] = Some(&mut *core::ptr::addr_of_mut!(PLANETARY_TIME));
            WATCH_FACES[61] = Some(&mut *core::ptr::addr_of_mut!(PLANETARY_HOURS));
            WATCH_FACES[62] = Some(&mut *core::ptr::addr_of_mut!(SUNRISE_SUNSET));
            WATCH_FACES[63] = Some(&mut *core::ptr::addr_of_mut!(ASTRONOMY));
            WATCH_FACES[64] = Some(&mut *core::ptr::addr_of_mut!(ORRERY));
            WATCH_FACES[65] = Some(&mut *core::ptr::addr_of_mut!(SOLSTICE));
            WATCH_FACES[66] = Some(&mut *core::ptr::addr_of_mut!(MORSECALC));
            WATCH_FACES[67] = Some(&mut *core::ptr::addr_of_mut!(TEMPCHART));
            WATCH_FACES[68] = Some(&mut *core::ptr::addr_of_mut!(DUAL_TIMER));
            WATCH_FACES[69] = Some(&mut *core::ptr::addr_of_mut!(RPN_CALCULATOR_ALT));
            WATCH_FACES[70] = Some(&mut *core::ptr::addr_of_mut!(WORLD_CLOCK2));
            WATCH_FACES[71] = Some(&mut *core::ptr::addr_of_mut!(SIMPLE_CLOCK_BIN_LED));
            WATCH_FACES[72] = Some(&mut *core::ptr::addr_of_mut!(MINUTE_REPEATER_DECIMAL));
            WATCH_FACES[73] = Some(&mut *core::ptr::addr_of_mut!(DAY_NIGHT_PERCENTAGE));
            WATCH_FACES[74] = Some(&mut *core::ptr::addr_of_mut!(SET_TIME));
            WATCH_FACES[75] = Some(&mut *core::ptr::addr_of_mut!(PREFERENCES));
            WATCH_FACES[76] = Some(&mut *core::ptr::addr_of_mut!(FINETUNE));
            WATCH_FACES[77] = Some(&mut *core::ptr::addr_of_mut!(SAVE_LOAD));
            WATCH_FACES[78] = Some(&mut *core::ptr::addr_of_mut!(NANOSEC));
            WATCH_FACES[79] = Some(&mut *core::ptr::addr_of_mut!(SET_TIME_HACKWATCH));
            WATCH_FACES[80] = Some(&mut *core::ptr::addr_of_mut!(VOLTAGE));
            WATCH_FACES[81] = Some(&mut *core::ptr::addr_of_mut!(HELLO_THERE));
            WATCH_FACES[82] = Some(&mut *core::ptr::addr_of_mut!(CHARACTER_SET));
            WATCH_FACES[83] = Some(&mut *core::ptr::addr_of_mut!(BEEPS));
            WATCH_FACES[84] = Some(&mut *core::ptr::addr_of_mut!(DEMO));
            WATCH_FACES[85] = Some(&mut *core::ptr::addr_of_mut!(FREQUENCY_CORRECTION));
            WATCH_FACES[86] = Some(&mut *core::ptr::addr_of_mut!(CHIRPY_DEMO));
            WATCH_FACES[87] = Some(&mut *core::ptr::addr_of_mut!(LIS2DW_LOGGING));
            WATCH_FACES[88] = Some(&mut *core::ptr::addr_of_mut!(THERMISTOR_READOUT));
            WATCH_FACES[89] = Some(&mut *core::ptr::addr_of_mut!(MINMAX));
            WATCH_FACES[90] = Some(&mut *core::ptr::addr_of_mut!(LIGHTMETER));
            WATCH_FACES[91] = Some(&mut *core::ptr::addr_of_mut!(THERMISTOR_LOGGING));
            WATCH_FACES[92] = Some(&mut *core::ptr::addr_of_mut!(THERMISTOR_TESTING));
            WATCH_FACES[93] = Some(&mut *core::ptr::addr_of_mut!(ALARM_THERMOMETER));
            WATCH_FACES[94] = Some(&mut *core::ptr::addr_of_mut!(
                ACCELEROMETER_DATA_ACQUISITION
            ));
            WATCH_FACES[95] = Some(&mut *core::ptr::addr_of_mut!(ACCEL_INTERRUPT_COUNT));
            WATCH_FACES[96] = Some(&mut *core::ptr::addr_of_mut!(ADVANCED_ALARM));
            WATCH_FACES[97] = Some(&mut *core::ptr::addr_of_mut!(HYDRATION));
            WATCH_FACES[98] = Some(&mut *core::ptr::addr_of_mut!(SOS));
            WATCH_FACES[99] = Some(&mut *core::ptr::addr_of_mut!(LANDER));
            WATCH_FACES[100] = Some(&mut *core::ptr::addr_of_mut!(PING));
            WATCH_FACES[101] = Some(&mut *core::ptr::addr_of_mut!(BABY_KICKS));
            WATCH_FACES[102] = Some(&mut *core::ptr::addr_of_mut!(SETTINGS_FACE));
            WATCH_FACES[103] = Some(&mut *core::ptr::addr_of_mut!(ISH));
            WATCH_FACES[104] = Some(&mut *core::ptr::addr_of_mut!(SOLAR_TIME));
            WATCH_FACES[105] = Some(&mut *core::ptr::addr_of_mut!(KE_DECIMAL_TIME));
            WATCH_FACES[106] = Some(&mut *core::ptr::addr_of_mut!(BEATS));
            WATCH_FACES[107] = Some(&mut *core::ptr::addr_of_mut!(DAYS_SINCE));
            WATCH_FACES[108] = Some(&mut *core::ptr::addr_of_mut!(TIDE));
            WATCH_FACES[109] = Some(&mut *core::ptr::addr_of_mut!(BLACKJACK));
            WATCH_FACES[110] = Some(&mut *core::ptr::addr_of_mut!(SQUASH));
        }

        for (i, face) in WATCH_FACES.iter_mut().enumerate() {
            if let Some(face) = face.as_deref_mut() {
                face.setup(&MOVEMENT_STATE.settings, i);
            }
        }

        if let Some(face) = WATCH_FACES[MOVEMENT_STATE.current_face_idx].as_deref_mut() {
            face.activate(&MOVEMENT_STATE.settings);
        }
    }
}

/// The main app loop: react to a single pending event, then return so the
/// caller can enter STANDBY. The CPU never stays awake here.
pub fn app_loop() {
    unsafe {
        // Handle a pending face change first.
        if MOVEMENT_STATE.watch_face_changed {
            if let Some(face) = WATCH_FACES[MOVEMENT_STATE.current_face_idx].as_deref_mut() {
                face.resign(&mut MOVEMENT_STATE.settings);
            }
            MOVEMENT_STATE.current_face_idx = MOVEMENT_STATE.next_face_idx;
            watch::slcd::clear_display();
            if let Some(face) = WATCH_FACES[MOVEMENT_STATE.current_face_idx].as_deref_mut() {
                face.activate(&MOVEMENT_STATE.settings);
            }
            MOVEMENT_STATE.watch_face_changed = false;
        }

        // Handle scheduled background tasks.
        if SCHEDULED_TASKS.iter().any(|&t| t != 0) {
            handle_scheduled_tasks();
        }

        // Run the per-minute all-face background task pass (advise + wants) if
        // the minute alarm requested it. Done here in main-loop context, not
        // the ISR.
        if RUN_BACKGROUND_TASKS {
            RUN_BACKGROUND_TASKS = false;
            handle_background_tasks();
        }

        // React to the single pending event.
        let event = PENDING_EVENT;
        if let Some(face) = WATCH_FACES[MOVEMENT_STATE.current_face_idx].as_deref_mut() {
            face.loop_(event, &mut MOVEMENT_STATE.settings);
        }

        // Poll the serial shell for incoming commands.
        SHELL.poll();

        // Persist any settings a face may have changed.
        save_settings();

        // Release any peripherals a face may have enabled, so nothing is left
        // on to drain the battery while the CPU sleeps.
        release_peripherals();

        // After reacting, always return to STANDBY. The CPU never stays awake.
        PENDING_EVENT = Event::Tick;
    }
}

/// Disables peripherals that should not remain on while the CPU sleeps.
///
/// The LCD, RTC, and buttons must stay on (they retain the display and wake
/// the CPU). Everything else — ADC, I2C, SPI, UART — is released so it cannot
/// drain the battery.
fn release_peripherals() {
    watch::adc::disable_adc();
    watch::i2c::disable_i2c();
    // Reconfigure the I2C pins to floating inputs so a sensor board cannot
    // backward-power itself through the bus while the CPU sleeps.
    watch::i2c::pins_to_floating_before_sleep();
    watch::spi::disable_spi();
}

// --- Interrupt callbacks ---

fn cb_light_btn_interrupt() {
    unsafe {
        // The 128 Hz fast tick samples the button pins and feeds the debouncer
        // continuously (see cb_fast_tick), so a clean press is detected even
        // without multiple edges. This edge interrupt wakes the CPU and runs an
        // immediate sample so the response feels immediate.
        if let Some(ev) = debounce::update(
            Button::Light,
            watch::gpio::get_pin_level(watch::extint::BTN_LIGHT),
        ) {
            if is_press(&ev) {
                stats::press_light();
            }
            PENDING_EVENT = ev;
        }
    }
}

fn cb_mode_btn_interrupt() {
    unsafe {
        if let Some(ev) = debounce::update(
            Button::Mode,
            watch::gpio::get_pin_level(watch::extint::BTN_MODE),
        ) {
            if is_press(&ev) {
                stats::press_mode();
            }
            PENDING_EVENT = ev;
        }
    }
}

fn cb_alarm_btn_interrupt() {
    unsafe {
        if let Some(ev) = debounce::update(
            Button::Alarm,
            watch::gpio::get_pin_level(watch::extint::BTN_ALARM),
        ) {
            if is_press(&ev) {
                stats::press_alarm();
            }
            PENDING_EVENT = ev;
        }
    }
}

fn cb_alarm_fired() {
    unsafe {
        // Ask the main loop to run the per-minute all-face background pass in
        // main context, and wake with a background event for the active face.
        RUN_BACKGROUND_TASKS = true;
        PENDING_EVENT = Event::BackgroundTask;
    }
}

/// The 1 Hz tick callback: wakes the CPU to render the current face.
pub fn cb_tick() {
    unsafe {
        // Monitor the RTC heartbeat (detect a frozen clock).
        fault::check_heartbeat();
        PENDING_EVENT = Event::Tick;
    }
}

/// The 128 Hz fast tick: tracks time for long-press detection.
pub fn cb_fast_tick() {
    unsafe {
        FAST_TICKS = FAST_TICKS.wrapping_add(1);
        // Sample the button pins and feed the debouncer periodically. The EIC
        // edge interrupts feed an initial sample; this periodic resampling lets
        // the debounce filter converge even on a clean (single-edge) press,
        // which would otherwise never reach the required sample count.
        sample_buttons();
        for button in [Button::Light, Button::Mode, Button::Alarm] {
            if let Some(ev) = debounce::check_long_press(button, FAST_TICKS) {
                PENDING_EVENT = ev;
            }
        }
    }
}

/// Reads the current button levels and feeds them to the debouncer.
fn sample_buttons() {
    unsafe {
        if let Some(ev) = debounce::update(
            Button::Light,
            watch::gpio::get_pin_level(watch::extint::BTN_LIGHT),
        ) {
            if is_press(&ev) {
                stats::press_light();
            }
            PENDING_EVENT = ev;
        }
        if let Some(ev) = debounce::update(
            Button::Mode,
            watch::gpio::get_pin_level(watch::extint::BTN_MODE),
        ) {
            if is_press(&ev) {
                stats::press_mode();
            }
            PENDING_EVENT = ev;
        }
        if let Some(ev) = debounce::update(
            Button::Alarm,
            watch::gpio::get_pin_level(watch::extint::BTN_ALARM),
        ) {
            if is_press(&ev) {
                stats::press_alarm();
            }
            PENDING_EVENT = ev;
        }
    }
}

/// Returns true if a debounced event is a fresh button-down press.
fn is_press(ev: &Event) -> bool {
    matches!(
        ev,
        Event::Button(Button::Light, ButtonEvent::Down)
            | Event::Button(Button::Mode, ButtonEvent::Down)
            | Event::Button(Button::Alarm, ButtonEvent::Down)
    )
}
