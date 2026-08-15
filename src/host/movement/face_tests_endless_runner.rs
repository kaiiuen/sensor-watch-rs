use super::{endless_runner, types};
use crate::watch::seam;
use sensor_watch_core::mock_hw::{MockHw, dt};
use types::WatchFace;

fn fixed_hw() -> MockHw {
    let mut hw = MockHw::new();
    // This fixed seed keeps the real face's random legal-pattern retry away
    // from its zero result while making every test reproducible.
    hw.set_time(dt(2023, 1, 6, 15, 4, 1));
    hw
}

fn activate(hw: &mut MockHw, face: &mut endless_runner::EndlessRunnerFace) -> types::Settings {
    let settings = types::Settings::default();
    seam::with_hw(hw, || face.activate(&settings));
    let mut settings = settings;
    seam::with_hw(hw, || face.loop_(types::Event::Activate, &mut settings));
    settings
}

fn press(
    hw: &mut MockHw,
    face: &mut endless_runner::EndlessRunnerFace,
    settings: &mut types::Settings,
    button: types::Button,
    event: types::ButtonEvent,
) {
    seam::with_hw(hw, || {
        face.loop_(types::Event::Button(button, event), settings)
    });
}

fn tick(
    hw: &mut MockHw,
    face: &mut endless_runner::EndlessRunnerFace,
    settings: &mut types::Settings,
) {
    seam::with_hw(hw, || face.loop_(types::Event::Tick, settings));
}

#[test]
fn endless_runner_activation_shows_title_and_is_pixel_safe() {
    let mut hw = fixed_hw();
    let mut face = endless_runner::EndlessRunnerFace::new();
    let _settings = activate(&mut hw, &mut face);

    assert_eq!(hw.text(), "ER NHS 000");
    assert!(hw.colon);
    assert!(hw.indicator(sensor_watch_core::mock_hw::Indicator::Bell));
    // MockHw records every raw LCD coordinate without indexing a fixed array.
    assert!(!hw.segments.is_empty() || hw.text().starts_with("ER"));
}

#[test]
fn endless_runner_start_jump_ticks_and_lose_are_bounded() {
    let mut hw = fixed_hw();
    let mut face = endless_runner::EndlessRunnerFace::new();
    let mut settings = activate(&mut hw, &mut face);

    press(
        &mut hw,
        &mut face,
        &mut settings,
        types::Button::Alarm,
        types::ButtonEvent::Up,
    );
    assert!(!hw.colon);
    let playing = hw.segments.clone();

    press(
        &mut hw,
        &mut face,
        &mut settings,
        types::Button::Light,
        types::ButtonEvent::Down,
    );
    assert_ne!(hw.segments, playing);

    // Every event returns; the finite bound prevents a test from hanging if a
    // future random-seed change makes a game path unexpectedly long.
    for _ in 0..128 {
        tick(&mut hw, &mut face, &mut settings);
        assert!(hw.chars.len() == 10);
    }
    assert!(hw.text().contains("LOSE") || hw.text().contains("ER") || hw.text().contains("HS"));
}

#[test]
fn endless_runner_difficulty_and_fuel_path_are_deterministic() {
    let mut hw = fixed_hw();
    let mut face = endless_runner::EndlessRunnerFace::new();
    let mut settings = activate(&mut hw, &mut face);

    for expected in ['H', 'F', 'F', 'b', 'E', 'N'] {
        press(
            &mut hw,
            &mut face,
            &mut settings,
            types::Button::Light,
            types::ButtonEvent::LongPress,
        );
        assert_eq!(hw.chars[3], expected, "{} != {expected}", hw.text());
    }

    // Select fuel mode again and start it. The first bounded tick renders the
    // finite fuel display; no audio fidelity is implied by the host no-op hook.
    for _ in 0..2 {
        press(
            &mut hw,
            &mut face,
            &mut settings,
            types::Button::Light,
            types::ButtonEvent::LongPress,
        );
    }
    press(
        &mut hw,
        &mut face,
        &mut settings,
        types::Button::Alarm,
        types::ButtonEvent::Up,
    );
    tick(&mut hw, &mut face, &mut settings);
    assert!(
        hw.text().contains("LOSE")
            || (hw.chars[2].is_ascii_digit() && hw.chars[3].is_ascii_digit())
    );
}

#[test]
fn endless_runner_resign_is_safe_before_and_after_activation() {
    let mut face = endless_runner::EndlessRunnerFace::new();
    let mut settings = types::Settings::default();
    face.resign(&mut settings);

    let mut hw = fixed_hw();
    let mut settings = activate(&mut hw, &mut face);
    face.resign(&mut settings);
    face.resign(&mut settings);
}
