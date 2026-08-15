//! Deterministic host coverage for the REAL Couch-to-5K face.

use crate::movement::couch_to_5k::CouchTo5kFace;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::seam;
use sensor_watch_core::mock_hw::MockHw;

fn send(mock: &mut MockHw, face: &mut CouchTo5kFace, settings: &mut Settings, event: Event) {
    seam::with_hw(mock, || face.loop_(event, settings));
}

fn activated() -> (MockHw, CouchTo5kFace, Settings) {
    let mut mock = MockHw::new();
    let settings = Settings::default();
    let mut face = CouchTo5kFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    send(&mut mock, &mut face, &mut settings.clone(), Event::Activate);
    (mock, face, settings)
}

#[test]
fn activation_shows_paused_warmup_and_colon() {
    let (mock, _, _) = activated();
    assert_eq!(mock.text(), "WU01050001");
    assert!(mock.colon);
}

#[test]
fn pause_and_resume_bound_ticks_without_advancing_while_paused() {
    let (mut mock, mut face, mut settings) = activated();
    let initial = mock.chars;

    send(&mut mock, &mut face, &mut settings, Event::Tick);
    assert_eq!(mock.chars, initial);

    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::Up),
    );
    send(&mut mock, &mut face, &mut settings, Event::Tick);
    assert_eq!(mock.text(), "WU01045901");

    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::Up),
    );
    let paused = mock.chars;
    send(&mut mock, &mut face, &mut settings, Event::Tick);
    assert_eq!(mock.chars, paused);
}

#[test]
fn bounded_ticks_transition_from_warmup_to_run() {
    let (mut mock, mut face, mut settings) = activated();
    send(
        &mut mock,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::Up),
    );

    for _ in 0..300 {
        send(&mut mock, &mut face, &mut settings, Event::Tick);
    }
    assert_eq!(mock.text(), "WU01000001");

    send(&mut mock, &mut face, &mut settings, Event::Tick);
    assert_eq!(mock.text(), "RU01010002");
}

fn complete_session(mock: &mut MockHw, face: &mut CouchTo5kFace, settings: &mut Settings) {
    send(
        mock,
        face,
        settings,
        Event::Button(Button::Alarm, ButtonEvent::Up),
    );
    for _ in 0..10_000 {
        send(mock, face, settings, Event::Tick);
        if mock.text().starts_with("--") {
            return;
        }
    }
    panic!("Couch-to-5K session did not complete within the bound");
}

#[test]
fn completion_advances_and_wraps_session_with_bounded_display() {
    let (mut mock, mut face, mut settings) = activated();

    for session in 0..27 {
        complete_session(&mut mock, &mut face, &mut settings);
        assert!(mock.text().starts_with("--"));
        send(
            &mut mock,
            &mut face,
            &mut settings,
            Event::Button(Button::Light, ButtonEvent::Up),
        );
        assert!(
            mock.text()
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        );
        if session < 26 {
            assert!(!mock.text().starts_with("WU01"));
        }
    }

    // Light-up changes the session state but the unchanged firmware face does
    // not redraw that branch; the next bounded tick exposes the wrapped state.
    send(&mut mock, &mut face, &mut settings, Event::Tick);
    assert_eq!(mock.text(), "WU01050001");
}

#[test]
fn resign_is_safe_before_and_after_activation() {
    let mut face = CouchTo5kFace::new();
    let mut settings = Settings::default();
    face.resign(&mut settings);

    let mut mock = MockHw::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    send(&mut mock, &mut face, &mut settings, Event::Activate);
    face.resign(&mut settings);
    face.resign(&mut settings);
}
