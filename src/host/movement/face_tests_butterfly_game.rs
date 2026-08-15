//! Deterministic host coverage for the REAL butterfly game face.

use crate::movement::butterfly_game::ButterflyGameFace;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::seam;
use sensor_watch_core::mock_hw::{MockHw, dt};

fn seeded() -> (MockHw, ButterflyGameFace, Settings) {
    let mut mock = MockHw::new();
    mock.set_time(dt(2024, 2, 29, 15, 4, 0));
    (mock, ButterflyGameFace::new(), Settings::default())
}

fn send(mock: &mut MockHw, face: &mut ButterflyGameFace, settings: &mut Settings, event: Event) {
    seam::with_hw(mock, || face.loop_(event, settings));
}

fn activate(mock: &mut MockHw, face: &mut ButterflyGameFace, settings: &Settings) {
    seam::with_hw(mock, || face.activate(settings));
    send(mock, face, &mut settings.clone(), Event::Activate);
}

#[test]
fn splash_activation_and_sound_selection_use_real_events() {
    let (mut mock, mut face, settings) = seeded();
    let mut settings = settings;
    activate(&mut mock, &mut face, &settings);
    assert!(mock.text().contains("Btrfly"));

    for _ in 0..8 {
        send(&mut mock, &mut face, &mut settings, Event::Tick);
    }
    assert!(mock.text().contains("snd y"));

    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::Down),
    );
    assert!(mock.text().contains("snd n"));
}

#[test]
fn goal_screen_and_light_alarm_inputs_are_bounded() {
    let (mut mock, mut face, settings) = seeded();
    let mut settings = settings;
    activate(&mut mock, &mut face, &settings);

    // Splash -> sound -> continue -> reset -> goal selection.
    for _ in 0..2 {
        send(
            &mut mock,
            &mut face,
            &mut settings,
            Event::Button(Button::Light, ButtonEvent::Down),
        );
    }
    assert!(mock.text().contains("GOaL 6"), "actual: {:?}", mock.chars);

    for _ in 0..2 {
        send(
            &mut mock,
            &mut face,
            &mut settings,
            Event::Button(Button::Alarm, ButtonEvent::Down),
        );
    }
    assert!(mock.text().contains("GOaL 3"));

    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::Down),
    );
    assert_eq!(mock.chars.len(), 10);
}

#[test]
fn round_transitions_score_and_game_end_remain_safe() {
    let (mut mock, mut face, settings) = seeded();
    let mut settings = settings;
    activate(&mut mock, &mut face, &settings);

    // Select the three-point goal through the reset path.
    for _ in 0..2 {
        send(
            &mut mock,
            &mut face,
            &mut settings,
            Event::Button(Button::Light, ButtonEvent::Down),
        );
    }
    for _ in 0..2 {
        send(
            &mut mock,
            &mut face,
            &mut settings,
            Event::Button(Button::Alarm, ButtonEvent::Down),
        );
    }
    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::Down),
    );

    // Three deterministic, bounded rounds. The RTC seed makes the generated
    // shape sequence repeatable, while the upper bound covers every possible
    // wrong-shape delay (1..=10).
    for _ in 0..3 {
        send(
            &mut mock,
            &mut face,
            &mut settings,
            Event::Button(Button::Light, ButtonEvent::Down),
        );
        for _ in 0..100 {
            send(&mut mock, &mut face, &mut settings, Event::Tick);
        }
        send(
            &mut mock,
            &mut face,
            &mut settings,
            Event::Button(Button::Light, ButtonEvent::Down),
        );
        for _ in 0..8 {
            send(&mut mock, &mut face, &mut settings, Event::Tick);
        }
    }

    assert!(
        mock.text().contains("pl1  wins"),
        "actual: {:?}",
        mock.chars
    );
    for _ in 0..32 {
        send(&mut mock, &mut face, &mut settings, Event::Tick);
    }
    assert!(mock.text().contains("GOaL"));
    assert_eq!(mock.chars.len(), 10);
}

#[test]
fn losing_rounds_and_resign_never_escape_display_bounds() {
    let (mut mock, mut face, settings) = seeded();
    let mut settings = settings;
    activate(&mut mock, &mut face, &settings);

    for _ in 0..256 {
        send(
            &mut mock,
            &mut face,
            &mut settings,
            Event::Button(Button::Alarm, ButtonEvent::Down),
        );
        send(&mut mock, &mut face, &mut settings, Event::Tick);
    }
    face.resign(&mut settings);
    assert!(mock.chars.iter().all(|c| c.is_ascii()));
    assert_eq!(mock.chars.len(), 10);
}
