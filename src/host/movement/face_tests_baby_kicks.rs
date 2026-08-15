//! Deterministic host coverage for the unchanged REAL baby-kicks face.

use crate::movement::baby_kicks::BabyKicksFace;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::seam;
use sensor_watch_core::mock_hw::{MockHw, dt};

fn send(mock: &mut MockHw, face: &mut BabyKicksFace, settings: &mut Settings, event: Event) {
    seam::with_hw(mock, || face.loop_(event, settings));
}

fn activated() -> (MockHw, BabyKicksFace, Settings) {
    let mut mock = MockHw::new();
    mock.set_time(dt(2023, 1, 6, 15, 4, 0));
    let mut settings = Settings::default();
    let mut face = BabyKicksFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    send(&mut mock, &mut face, &mut settings, Event::Activate);
    (mock, face, settings)
}

#[test]
fn activation_shows_baby_splash_with_bounded_display() {
    let (mock, _, _) = activated();
    assert_eq!(mock.text(), "    baby");
    assert!(!mock.colon);
}

#[test]
fn alarm_starts_then_counts_movements_and_long_press_undoes() {
    let (mut mock, mut face, mut settings) = activated();

    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::Up),
    );
    assert_eq!(mock.text(), "  00000000");
    assert!(mock.colon);

    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::Up),
    );
    assert_eq!(mock.text(), "  00010001");

    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::LongPress),
    );
    assert_eq!(mock.text(), "  00000000");
}

#[test]
fn mode_long_press_resets_active_session_to_splash() {
    let (mut mock, mut face, mut settings) = activated();
    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::Up),
    );
    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Mode, ButtonEvent::LongPress),
    );
    assert_eq!(mock.text(), "    baby");
    assert!(!mock.colon);
}

#[test]
fn elapsed_timeout_displays_to_and_reset_remains_safe() {
    let (mut mock, mut face, mut settings) = activated();
    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::Up),
    );

    mock.set_time(dt(2023, 1, 6, 16, 44, 0));
    send(&mut mock, &mut face, &mut settings, Event::BackgroundTask);
    assert_eq!(mock.text(), "TO  000000");

    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Mode, ButtonEvent::LongPress),
    );
    assert_eq!(mock.text(), "    baby");
}

#[test]
fn display_bounds_hold_at_two_stretch_and_four_movement_digits() {
    let (mut mock, mut face, mut settings) = activated();
    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::Up),
    );
    for _ in 0..12 {
        send(
            &mut mock,
            &mut face,
            &mut settings,
            Event::Button(Button::Alarm, ButtonEvent::Up),
        );
    }
    assert_eq!(mock.text().len(), 10);
    assert!(
        mock.text()
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ')
    );
}

#[test]
fn resign_is_safe_before_and_after_activation() {
    let mut face = BabyKicksFace::new();
    let mut settings = Settings::default();
    face.resign(&mut settings);

    let mut mock = MockHw::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    send(&mut mock, &mut face, &mut settings, Event::Activate);
    face.resign(&mut settings);
    face.resign(&mut settings);
}
