//! Movement framework core.
//!
//! An event-driven, interrupt-powered dispatcher. The CPU is a start/stop
//! resource: it wakes only to react to a single event, then immediately
//! returns to STANDBY. All timekeeping is owned by the RTC, never by the CPU.

pub mod activity;
pub mod alarm;
pub mod astronomy;
pub mod blinky;
pub mod board;
pub mod breathing;
pub mod butterfly_game;
pub mod close_enough;
pub mod couch_to_5k;
pub mod countdown;
pub mod counter;
pub mod databank;
pub mod day_one;
pub mod deadline;
pub mod debounce;
pub mod decimal_time;
pub mod diagnostics;
pub mod discgolf;
pub mod dual_timer;
pub mod endless_runner;
pub mod fault;
pub mod flashlight;
pub mod french_revolutionary;
pub mod geomancy;
pub mod habit;
pub mod higher_lower_game;
pub mod interval;
pub mod invaders;
pub mod kitchen_conversions;
pub mod mars_time;
pub mod menstrual_cycle;
pub mod metronome;
pub mod minimal_clock;
pub mod moon_phase;
pub mod morsecalc;
pub mod orrery;
pub mod periodic;
pub mod persist;
pub mod planetary_hours;
pub mod planetary_time;
pub mod probability;
pub mod pulsometer;
pub mod randonaut;
pub mod ratemeter;
pub mod repetition_minute;
pub mod rpn_calculator;
pub mod rpn_calculator_alt;
pub mod sailing;
pub mod ships_bell;
pub mod simon;
pub mod simple_calculator;
pub mod simple_clock;
pub mod simple_clock_bin_led;
pub mod simple_coin_flip;
pub mod solstice;
pub mod stats;
pub mod stock_stopwatch;
pub mod stopwatch;
pub mod sunrise_sunset;
pub mod tachymeter;
pub mod tally;
pub mod tarot;
pub mod tempchart;
pub mod time_left;
pub mod timer;
pub mod tomato;
pub mod toss_up;
pub mod totp;
pub mod totp_lfs;
pub mod tuning_tones;
pub mod types;
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

/// The global movement state.
pub static mut MOVEMENT_STATE: MovementState = MovementState::new_static();

/// The list of watch faces. Faces are static instances — there is no heap, so
/// nothing can be allocated and nothing can grow.
pub static mut WATCH_FACES: [Option<&'static mut dyn WatchFace>; MOVEMENT_NUM_FACES] =
    [const { None }; MOVEMENT_NUM_FACES];

/// The static simple clock face instance.
static mut SIMPLE_CLOCK: simple_clock::SimpleClockFace =
    simple_clock::SimpleClockFace::new_static();

/// The static countdown face instance.
static mut COUNTDOWN: countdown::CountdownFace = countdown::CountdownFace::new_static();

/// The static alarm face instance.
static mut ALARM: alarm::AlarmFace = alarm::AlarmFace::new_static();

/// The static counter face instance.
static mut COUNTER: counter::CounterFace = counter::CounterFace::new_static();

/// The static world clock face instance.
static mut WORLD_CLOCK: world_clock::WorldClockFace = world_clock::WorldClockFace::new_static();

/// The static world clock 2 face instance.
static mut WORLD_CLOCK2: world_clock2::WorldClock2Face =
    world_clock2::WorldClock2Face::new_static();

/// The static simple clock bin LED face instance.
static mut SIMPLE_CLOCK_BIN_LED: simple_clock_bin_led::SimpleClockBinLedFace =
    simple_clock_bin_led::SimpleClockBinLedFace::new_static();

/// The static diagnostics face instance.
static mut DIAGNOSTICS: diagnostics::DiagnosticsFace = diagnostics::DiagnosticsFace::new_static();

/// The static flashlight face instance.
static mut FLASHLIGHT: flashlight::FlashlightFace = flashlight::FlashlightFace::new_static();

/// The static decimal time face instance.
static mut DECIMAL_TIME: decimal_time::DecimalTimeFace =
    decimal_time::DecimalTimeFace::new_static();

/// The static week number clock face instance.
static mut WEEKNUMBER: weeknumber::WeekNumberClockFace =
    weeknumber::WeekNumberClockFace::new_static();

/// The static minimal clock face instance.
static mut MINIMAL_CLOCK: minimal_clock::MinimalClockFace =
    minimal_clock::MinimalClockFace::new_static();

/// The static blinky face instance.
static mut BLINKY: blinky::BlinkyFace = blinky::BlinkyFace::new_static();

/// The static tally face instance.
static mut TALLY: tally::TallyFace = tally::TallyFace::new_static();

/// The static ships bell face instance.
static mut SHIPS_BELL: ships_bell::ShipsBellFace = ships_bell::ShipsBellFace::new_static();

/// The static close-enough clock face instance.
static mut CLOSE_ENOUGH: close_enough::CloseEnoughClockFace =
    close_enough::CloseEnoughClockFace::new_static();

/// The static moon phase face instance.
static mut MOON_PHASE: moon_phase::MoonPhaseFace = moon_phase::MoonPhaseFace::new_static();

/// The static stopwatch face instance.
static mut STOPWATCH: stopwatch::StopwatchFace = stopwatch::StopwatchFace::new_static();

/// The static timer face instance.
static mut TIMER: timer::TimerFace = timer::TimerFace::new_static();

/// The static French Revolutionary face instance.
static mut FRENCH_REVOLUTIONARY: french_revolutionary::FrenchRevolutionaryFace =
    french_revolutionary::FrenchRevolutionaryFace::new_static();

/// The static Mars time face instance.
static mut MARS_TIME: mars_time::MarsTimeFace = mars_time::MarsTimeFace::new_static();

/// The static sailing face instance.
static mut SAILING: sailing::SailingFace = sailing::SailingFace::new_static();

/// The static metronome face instance.
static mut METRONOME: metronome::MetronomeFace = metronome::MetronomeFace::new_static();

/// The static tachymeter face instance.
static mut TACHYMETER: tachymeter::TachymeterFace = tachymeter::TachymeterFace::new_static();

/// The static pulsometer face instance.
static mut PULSOMETER: pulsometer::PulsometerFace = pulsometer::PulsometerFace::new_static();

/// The static ratemeter face instance.
static mut RATEMETER: ratemeter::RatemeterFace = ratemeter::RatemeterFace::new_static();

/// The static probability face instance.
static mut PROBABILITY: probability::ProbabilityFace = probability::ProbabilityFace::new_static();

/// The static simple coin flip face instance.
static mut SIMPLE_COIN_FLIP: simple_coin_flip::SimpleCoinFlipFace =
    simple_coin_flip::SimpleCoinFlipFace::new_static();

/// The static toss-up face instance.
static mut TOSS_UP: toss_up::TossUpFace = toss_up::TossUpFace::new_static();

/// The static databank face instance.
static mut DATABANK: databank::DatabankFace = databank::DatabankFace::new_static();

/// The static habit face instance.
static mut HABIT: habit::HabitFace = habit::HabitFace::new_static();

/// The static tomato face instance.
static mut TOMATO: tomato::TomatoFace = tomato::TomatoFace::new_static();

/// The static deadline face instance.
static mut DEADLINE: deadline::DeadlineFace = deadline::DeadlineFace::new_static();

/// The static breathing face instance.
static mut BREATHING: breathing::BreathingFace = breathing::BreathingFace::new_static();

/// The static periodic table face instance.
static mut PERIODIC: periodic::PeriodicFace = periodic::PeriodicFace::new_static();

/// The static tuning tones face instance.
static mut TUNING_TONES: tuning_tones::TuningTonesFace =
    tuning_tones::TuningTonesFace::new_static();

/// The static wake face instance.
static mut WAKE: wake::WakeFace = wake::WakeFace::new_static();

/// The static kitchen conversions face instance.
static mut KITCHEN_CONVERSIONS: kitchen_conversions::KitchenConversionsFace =
    kitchen_conversions::KitchenConversionsFace::new_static();

/// The static wareki face instance.
static mut WAREKI: wareki::WarekiFace = wareki::WarekiFace::new_static();

/// The static tarot face instance.
static mut TAROT: tarot::TarotFace = tarot::TarotFace::new_static();

/// The static randonaut face instance.
static mut RANDONAUT: randonaut::RandonautFace = randonaut::RandonautFace::new_static();

/// The static day one face instance.
static mut DAY_ONE: day_one::DayOneFace = day_one::DayOneFace::new_static();

/// The static time left face instance.
static mut TIME_LEFT: time_left::TimeLeftFace = time_left::TimeLeftFace::new_static();

/// The static disc golf face instance.
static mut DISCGOLF: discgolf::DiscgolfFace = discgolf::DiscgolfFace::new_static();

/// The static menstrual cycle face instance.
static mut MENSTRUAL_CYCLE: menstrual_cycle::MenstrualCycleFace =
    menstrual_cycle::MenstrualCycleFace::new_static();

/// The static butterfly game face instance.
static mut BUTTERFLY_GAME: butterfly_game::ButterflyGameFace =
    butterfly_game::ButterflyGameFace::new_static();

/// The static simon face instance.
static mut SIMON: simon::SimonFace = simon::SimonFace::new_static();

/// The static invaders face instance.
static mut INVADERS: invaders::InvadersFace = invaders::InvadersFace::new_static();

/// The static higher/lower game face instance.
static mut HIGHER_LOWER_GAME: higher_lower_game::HigherLowerGameFace =
    higher_lower_game::HigherLowerGameFace::new_static();

/// The static endless runner face instance.
static mut ENDLESS_RUNNER: endless_runner::EndlessRunnerFace =
    endless_runner::EndlessRunnerFace::new_static();

/// The static geomancy face instance.
static mut GEOMANCY: geomancy::GeomancyFace = geomancy::GeomancyFace::new_static();

/// The static repetition minute face instance.
static mut REPETITION_MINUTE: repetition_minute::RepetitionMinuteFace =
    repetition_minute::RepetitionMinuteFace::new_static();

/// The static wyoscan face instance.
static mut WYOSCAN: wyoscan::WyoscanFace = wyoscan::WyoscanFace::new_static();

/// The static couch to 5k face instance.
static mut COUCH_TO_5K: couch_to_5k::CouchTo5kFace = couch_to_5k::CouchTo5kFace::new_static();

/// The static simple calculator face instance.
static mut SIMPLE_CALCULATOR: simple_calculator::SimpleCalculatorFace =
    simple_calculator::SimpleCalculatorFace::new_static();

/// The static RPN calculator face instance.
static mut RPN_CALCULATOR: rpn_calculator::RpnCalculatorFace =
    rpn_calculator::RpnCalculatorFace::new_static();

/// The static TOTP face instance.
static mut TOTP: totp::TotpFace = totp::TotpFace::new_static();

/// The static stock stopwatch face instance.
static mut STOCK_STOPWATCH: stock_stopwatch::StockStopwatchFace =
    stock_stopwatch::StockStopwatchFace::new_static();

/// The static activity face instance.
static mut ACTIVITY: activity::ActivityFace = activity::ActivityFace::new_static();

/// The static interval face instance.
static mut INTERVAL: interval::IntervalFace = interval::IntervalFace::new_static();

/// The static TOTP LFS face instance.
static mut TOTP_LFS: totp_lfs::TotpFaceLfs = totp_lfs::TotpFaceLfs::new_static();

/// The static wordle face instance.
static mut WORDLE: wordle::WordleFace = wordle::WordleFace::new_static();

/// The static planetary time face instance.
static mut PLANETARY_TIME: planetary_time::PlanetaryTimeFace =
    planetary_time::PlanetaryTimeFace::new_static();

/// The static planetary hours face instance.
static mut PLANETARY_HOURS: planetary_hours::PlanetaryHoursFace =
    planetary_hours::PlanetaryHoursFace::new_static();

/// The static sunrise/sunset face instance.
static mut SUNRISE_SUNSET: sunrise_sunset::SunriseSunsetFace =
    sunrise_sunset::SunriseSunsetFace::new_static();

/// The static astronomy face instance.
static mut ASTRONOMY: astronomy::AstronomyFace = astronomy::AstronomyFace::new_static();

/// The static orrery face instance.
static mut ORRERY: orrery::OrreryFace = orrery::OrreryFace::new_static();

/// The static solstice face instance.
static mut SOLSTICE: solstice::SolsticeFace = solstice::SolsticeFace::new_static();

/// The static morsecalc face instance.
static mut MORSECALC: morsecalc::MorsecalcFace = morsecalc::MorsecalcFace::new_static();

/// The static tempchart face instance.
static mut TEMPCHART: tempchart::TempchartFace = tempchart::TempchartFace::new_static();

/// The static dual timer face instance.
static mut DUAL_TIMER: dual_timer::DualTimerFace = dual_timer::DualTimerFace::new_static();

/// The static RPN calculator alt face instance.
static mut RPN_CALCULATOR_ALT: rpn_calculator_alt::RpnCalculatorAltFace =
    rpn_calculator_alt::RpnCalculatorAltFace::new_static();

/// Scheduled background tasks per face (packed RTC time).
pub static mut SCHEDULED_TASKS: [u32; MOVEMENT_NUM_FACES] = [0; MOVEMENT_NUM_FACES];

/// The pending event that woke the CPU.
pub static mut PENDING_EVENT: Event = Event::Tick;

/// A fast-tick counter (128 Hz) used for long-press detection.
pub static mut FAST_TICKS: u16 = 0;

/// Handles background tasks for all faces.
fn handle_background_tasks() {
    unsafe {
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
        Event::Button(Button::Mode, ButtonEvent::LongPress) => move_to_face(0),
        _ => {}
    }
}

/// Saves the current settings to flash so they survive a reset.
///
/// Faces should call this after changing any setting.
pub fn save_settings() {
    unsafe {
        persist::save(&MOVEMENT_STATE.settings);
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
pub fn move_to_next_face() {
    unsafe {
        let face_max = MOVEMENT_NUM_FACES;
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
    unsafe {
        MOVEMENT_STATE.is_buzzing = true;
        buzzer::enable_buzzer();
    }
}

/// Plays the alarm.
pub fn play_alarm() {
    play_alarm_beeps(5, BuzzerNote::C8);
}

/// Plays alarm beeps.
pub fn play_alarm_beeps(rounds: u8, alarm_note: BuzzerNote) {
    let mut rounds = rounds;
    if rounds == 0 {
        rounds = 1;
    }
    if rounds > 20 {
        rounds = 20;
    }
    unsafe {
        MOVEMENT_STATE.alarm_note = alarm_note;
        MOVEMENT_STATE.is_buzzing = true;
    }
    buzzer::enable_buzzer();
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

/// App init: called once at boot.
pub fn app_init() {
    unsafe {
        rtc::freqcorr_write(22, 0);
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
        }
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

        // React to the single pending event.
        let event = PENDING_EVENT;
        if let Some(face) = WATCH_FACES[MOVEMENT_STATE.current_face_idx].as_deref_mut() {
            face.loop_(event, &mut MOVEMENT_STATE.settings);
        }

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
    watch::spi::disable_spi();
}

// --- Interrupt callbacks ---

fn cb_light_btn_interrupt() {
    unsafe {
        let pin_level = watch::gpio::get_pin_level(watch::extint::BTN_LIGHT);
        if let Some(ev) = debounce::update(Button::Light, pin_level) {
            stats::press_light();
            PENDING_EVENT = ev;
        }
    }
}

fn cb_mode_btn_interrupt() {
    unsafe {
        let pin_level = watch::gpio::get_pin_level(watch::extint::BTN_MODE);
        if let Some(ev) = debounce::update(Button::Mode, pin_level) {
            stats::press_mode();
            PENDING_EVENT = ev;
        }
    }
}

fn cb_alarm_btn_interrupt() {
    unsafe {
        let pin_level = watch::gpio::get_pin_level(watch::extint::BTN_ALARM);
        if let Some(ev) = debounce::update(Button::Alarm, pin_level) {
            stats::press_alarm();
            PENDING_EVENT = ev;
        }
    }
}

fn cb_alarm_fired() {
    unsafe {
        PENDING_EVENT = Event::BackgroundTask;
    }
}

/// The 1 Hz tick callback: wakes the CPU to render the current face.
pub fn cb_tick() {
    unsafe {
        PENDING_EVENT = Event::Tick;
    }
}

/// The 128 Hz fast tick: tracks time for long-press detection.
pub fn cb_fast_tick() {
    unsafe {
        FAST_TICKS = FAST_TICKS.wrapping_add(1);
        for button in [Button::Light, Button::Mode, Button::Alarm] {
            if let Some(ev) = debounce::check_long_press(button, FAST_TICKS) {
                PENDING_EVENT = ev;
            }
        }
    }
}
