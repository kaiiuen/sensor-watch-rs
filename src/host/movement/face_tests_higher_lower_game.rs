//! Deterministic host coverage for the REAL higher/lower game face.

use crate::movement::higher_lower_game::HigherLowerGameFace;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::seam;
use sensor_watch_core::mock_hw::{MockHw, dt};

fn seeded_face() -> (MockHw, HigherLowerGameFace, Settings) {
    let mut mock = MockHw::new();
    mock.set_time(dt(2024, 2, 29, 15, 4, 0));
    (mock, HigherLowerGameFace::new(), Settings::default())
}

fn send(mock: &mut MockHw, face: &mut HigherLowerGameFace, settings: &mut Settings, event: Event) {
    seam::with_hw(mock, || face.loop_(event, settings));
}

#[test]
fn real_higher_lower_activates_with_title_and_starts_from_light_up() {
    let (mut mock, mut face, settings) = seeded_face();
    let mut settings = settings;

    seam::with_hw(&mut mock, || face.activate(&settings));
    send(&mut mock, &mut face, &mut settings, Event::Activate);
    assert_eq!(mock.text(), "GA  Hi-Lo");

    // Down edges and Alarm do not start the game; Light-up is the game start.
    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::Down),
    );
    assert_eq!(mock.text(), "GA  Hi-Lo");
    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::Up),
    );
    assert_ne!(mock.text(), "GA  Hi-Lo");
    assert!(mock.text().contains('0') || mock.text().contains('1') || mock.text().contains('2'));
}

#[test]
fn real_higher_lower_light_and_alarm_up_are_distinct_guesses() {
    let (mut mock, mut face, settings) = seeded_face();
    let mut settings = settings;
    seam::with_hw(&mut mock, || face.activate(&settings));
    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::Up),
    );

    let before_guess = mock.chars;
    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::Down),
    );
    assert_eq!(mock.chars, before_guess);

    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::Up),
    );
    assert!(matches!(
        mock.text().get(0..2),
        Some("HI" | "LO" | "==" | "GO")
    ));

    // The other Up transition is a valid independent guess path and must not
    // panic regardless of whether the first guess ended the round.
    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::Up),
    );
    assert!(mock.text().len() >= 2);
}

#[test]
fn real_higher_lower_score_and_end_states_are_safe() {
    let (mut mock, mut face, settings) = seeded_face();
    let mut settings = settings;
    seam::with_hw(&mut mock, || face.activate(&settings));
    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::Up),
    );

    // Exercise more inputs than a complete score can require. This covers the
    // Lose/Win -> ShowScore -> TitleScreen transitions and the fresh-game reset.
    for _ in 0..=u8::MAX {
        send(
            &mut mock,
            &mut face,
            &mut settings,
            Event::Button(Button::Alarm, ButtonEvent::Up),
        );
    }
    assert!(mock.text().len() >= 2);
}

#[test]
fn real_higher_lower_is_deterministic_for_the_same_rtc_seed() {
    let (mut left_mock, mut left_face, left_settings) = seeded_face();
    let (mut right_mock, mut right_face, right_settings) = seeded_face();
    let mut left_settings = left_settings;
    let mut right_settings = right_settings;

    for (left, right) in [
        (Event::Activate, Event::Activate),
        (
            Event::Button(Button::Light, ButtonEvent::Up),
            Event::Button(Button::Light, ButtonEvent::Up),
        ),
        (
            Event::Button(Button::Alarm, ButtonEvent::Up),
            Event::Button(Button::Alarm, ButtonEvent::Up),
        ),
        (
            Event::Button(Button::Light, ButtonEvent::Up),
            Event::Button(Button::Light, ButtonEvent::Up),
        ),
        (Event::Tick, Event::Tick),
    ] {
        if matches!(left, Event::Activate) {
            seam::with_hw(&mut left_mock, || left_face.activate(&left_settings));
            seam::with_hw(&mut right_mock, || right_face.activate(&right_settings));
        }
        send(&mut left_mock, &mut left_face, &mut left_settings, left);
        send(&mut right_mock, &mut right_face, &mut right_settings, right);
        assert_eq!(left_mock.chars, right_mock.chars);
        assert_eq!(left_mock.colon, right_mock.colon);
    }
}
