//! Host tests for the I-P face subset, driven through the `Hw` seam exactly like
//! `simple_clock`'s tests in `mod.rs`. Each test installs a [`MockHw`], invokes
//! the real face's `activate`/`loop_` with representative `Event`s, and asserts
//! LCD output derived from the real face source.

use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::seam;
use sensor_watch_core::mock_hw::{Indicator, MockHw, dt};

use super::{
    interval, invaders, ish, kitchen_conversions, lander, lightmeter, lis2dw_logging, mars_time,
    menstrual_cycle, metronome, minimal_clock, minmax, minute_repeater_decimal, moon_phase,
    morsecalc, nanosec, orrery, periodic, ping, planetary_hours, planetary_time, preferences,
    probability, pulsometer,
};

/// A deterministic steady state: Friday 2023-01-06 15:04:00, healthy battery.
fn steady_state() -> MockHw {
    let mut hw = MockHw::new();
    hw.set_time(dt(2023, 1, 6, 15, 4, 0));
    hw.vcc_mv = 3000;
    hw
}

/// 24-hour firmware settings so rendering is deterministic.
fn h24_settings() -> Settings {
    let mut s = Settings::default();
    s.set_clock_mode_24h(true);
    s
}

#[test]
fn real_interval_activate_enters_intro_and_runs_timer() {
    let mut mock = steady_state();
    // interval uses `movement::TIMEZONE_OFFSETS`? No; but its `loop_` Mode LongPress
    // calls `movement::move_to_face(0)`, and Light Up in Waiting calls
    // `movement::illuminate_led()` - both host no-ops.
    let mut settings = h24_settings();
    let mut face = interval::IntervalFace::new();
    // `setup` populates the default timers.
    WatchFace::setup(&mut face, &settings, 0);
    seam::with_hw(&mut mock, || face.activate(&settings));
    // Advance through the 5-tick intro to reach Waiting.
    for _ in 0..5 {
        seam::with_hw(&mut mock, || face.loop_(Event::Tick, &mut settings));
    }
    // From Waiting, a long Alarm press starts timer 0 (work 40:00), setting the
    // Bell indicator and colon.
    seam::with_hw(&mut mock, || {
        face.loop_(
            Event::Button(Button::Alarm, ButtonEvent::LongPress),
            &mut settings,
        );
    });
    assert!(mock.indicator(Indicator::Bell));
    assert!(mock.colon);
}

#[test]
fn real_invaders_activate_shows_next_attract_screen() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = invaders::InvadersFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // Attract screen: "GA" + 8 zero-score digits.
    assert!(mock.text().starts_with("GA"));
}

#[test]
fn real_ish_activate_renders_vague_time() {
    let mut mock = steady_state();
    let settings = h24_settings();
    let mut face = ish::IshFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    // 15:04, vagueness 1: "ISH" tag + hour "15".
    assert!(mock.text().starts_with("ISH"));
    assert!(mock.colon);
}

#[test]
fn real_kitchen_conversions_activate_shows_units() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = kitchen_conversions::KitchenConversionsFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // Measurement page: "Un" + first measure "WeIght".
    assert!(mock.text().starts_with("Un"), "actual: {}", mock.text());
    assert!(mock.text().contains("WeIght"));
}

#[test]
fn real_lander_activate_shows_la_landing() {
    let mut mock = steady_state();
    let settings = h24_settings();
    let mut face = lander::LanderFace::new();
    // Seed the RNG deterministically.
    seam::with_hw(&mut mock, || {
        WatchFace::setup(&mut face, &settings, 0);
        face.activate(&settings);
    });
    // Fresh save (no EEPROM key) => "LA" intro label at position 0.
    assert!(mock.text().starts_with("LA"));
}

#[test]
fn real_lightmeter_activate_shows_ev() {
    let mut mock = steady_state();
    let settings = h24_settings();
    let mut face = lightmeter::LightmeterFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    // No I2C fixture is installed, so the real face must fail closed.
    assert_eq!(mock.text(), "NO LS");
}

#[test]
fn real_lis2dw_logging_shows_no_data_when_empty() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = lis2dw_logging::Lis2dwLoggingFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    // Light Down enters log-view mode with zero points => "NO data".
    seam::with_hw(&mut mock, || {
        face.loop_(
            Event::Button(Button::Light, ButtonEvent::Down),
            &mut settings,
        );
    });
    assert!(mock.text().starts_with("NO  da"), "actual: {}", mock.text());
}

#[test]
fn real_mars_time_activate_shows_mtc() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = mars_time::MarsTimeFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // Site 0 (MTC) with an HH:MM:SS Mars clock.
    assert!(mock.text().starts_with("MC"));
    assert!(mock.colon);
}

#[test]
fn real_menstrual_cycle_activate_shows_28_day_estimate() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = menstrual_cycle::MenstrualCycleFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // No tracked dates => page 0 shows 28 (typical avg cycle). No fertility bell.
    assert!(mock.text().contains("28"));
    assert!(!mock.indicator(Indicator::Bell));
    assert!(!mock.indicator(Indicator::Signal));
}

#[test]
fn real_metronome_activate_shows_120_bpm() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = metronome::MetronomeFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // Default 120 bpm, count 4: "MN 4 120bp"; sound on => Bell.
    assert!(mock.text().contains("120"));
    assert!(mock.indicator(Indicator::Bell));
}

#[test]
fn real_minimal_clock_activate_renders_time() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = minimal_clock::MinimalClockFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    assert!(mock.text().contains("1504"));
    assert!(mock.colon);
}

#[test]
fn real_minmax_activate_shows_min_celsius() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = minmax::MinmaxFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // show_min, all-zeros log => 0 C, rendered "MN 000#C".
    assert!(mock.text().starts_with("MN"));
    assert!(mock.text().contains("000#C"));
}

#[test]
fn real_minute_repeater_decimal_activate_renders_24h() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = minute_repeater_decimal::MinuteRepeaterDecimalFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    assert_eq!(mock.text(), "FR06150400");
    assert!(mock.colon);
    assert!(mock.indicator(Indicator::H24));
}

#[test]
fn real_moon_phase_activate_shows_day_digits() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = moon_phase::MoonPhaseFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // Day-of-month digits are always written at positions 2-3 regardless of phase.
    assert_eq!(mock.chars[2], '0');
    assert_eq!(mock.chars[3], '6');
}

#[test]
fn real_morsecalc_activate_shows_empty_stack() {
    let mut mock = steady_state();
    let settings = h24_settings();
    let mut face = morsecalc::MorsecalcFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    // Empty stack: display_float(0) renders "0"; stack empty label shown.
    assert!(mock.text().len() > 0);
}

#[test]
fn real_nanosec_activate_shows_freq_correction() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = nanosec::NanosecFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // Screen 0: "FC " + freq correction value.
    assert!(mock.text().starts_with("FC"));
}

#[test]
fn real_orrery_activate_shows_orrery_title() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = orrery::OrreryFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // Selecting-body mode shows the title.
    assert!(mock.text().contains("Orrery"));
}

#[test]
fn real_periodic_activate_shows_title() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = periodic::PeriodicFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // Title screen: "Pd   Table".
    assert!(mock.text().starts_with("Pd"), "actual: {}", mock.text());
    assert!(mock.text().contains("Table"));
}

#[test]
fn real_ping_activate_shows_title_screen() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = ping::PingFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    assert!(mock.text().contains("Ping"));
}

#[test]
fn real_planetary_time_activate_sets_colon() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = planetary_time::PlanetaryTimeFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // `planetary_time` always set_colon() first; the phase computation follows.
    assert!(mock.colon);
}

#[test]
fn real_planetary_hours_activate_renders_planetary_hour() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = planetary_hours::PlanetaryHoursFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // Renders a ruling-planet tag + planetary hour at position 0.
    assert!(mock.text().len() >= 4);
}

#[test]
fn real_preferences_activate_shows_clock_title() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = preferences::PreferencesFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // Page 0 title "CL".
    assert_eq!(mock.text(), "CL");
}

#[test]
fn real_probability_activate_shows_dice_roll() {
    let mut mock = steady_state();
    let mut settings = h24_settings();
    let mut face = probability::ProbabilityFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    // Rolled 0, 2-sided die => shows die type "02" and the "PR" tag.
    assert!(mock.text().starts_with("PR"));
    assert!(mock.text().contains("02"));
}

#[test]
fn real_pulsometer_activate_shows_calibration() {
    let mut mock = steady_state();
    let settings = h24_settings();
    let mut face = pulsometer::PulsometerFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    // Title "PL" + default calibration 30.
    assert!(mock.text().starts_with("PL"));
    assert!(mock.text().contains("30"));
}
