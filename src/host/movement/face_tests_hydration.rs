//! Deterministic host coverage for the unchanged REAL hydration face.
//!
//! This seam validates foreground state/rendering only. Host policy deliberately
//! does not claim wake delivery, alarm delivery, scheduler dispatch, persistence,
//! or timezone-aware behavior.

use super::hydration::HydrationFace;
use super::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::seam;
use sensor_watch_core::mock_hw::{MockHw, dt};

fn settings() -> Settings {
    let mut settings = Settings::default();
    settings.set_clock_mode_24h(true);
    settings
}

fn send(hw: &mut MockHw, face: &mut HydrationFace, settings: &mut Settings, event: Event) {
    seam::with_hw(hw, || face.loop_(event, settings));
}

fn activated(hour: u8) -> (MockHw, HydrationFace, Settings) {
    let mut hw = MockHw::new();
    hw.set_time(dt(2024, 2, 29, hour, 0, 0));
    let settings = settings();
    let mut face = HydrationFace::new();
    seam::with_hw(&mut hw, || face.activate(&settings));
    (hw, face, settings)
}

#[test]
fn activation_and_display_use_the_real_face() {
    let (hw, _, _) = activated(9);
    assert_eq!(hw.text(), "HY000000ml");
}

#[test]
fn intake_add_remove_and_upper_clamp_are_deterministic() {
    let (mut hw, mut face, mut settings) = activated(9);
    for _ in 0..99 {
        send(
            &mut hw,
            &mut face,
            &mut settings,
            Event::Button(Button::Alarm, ButtonEvent::Up),
        );
    }
    assert!(hw.text().contains("9900ml"));
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::Up),
    );
    assert!(hw.text().contains("9900ml"));
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::Up),
    );
    assert!(hw.text().contains("9800ml"));
    for _ in 0..99 {
        send(
            &mut hw,
            &mut face,
            &mut settings,
            Event::Button(Button::Light, ButtonEvent::Up),
        );
    }
    assert!(hw.text().contains("0000ml"));
}

#[test]
fn settings_navigation_wraps_and_values_advance() {
    let (mut hw, mut face, mut settings) = activated(9);
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::LongPress),
    );
    assert!(hw.text().starts_with("GLAS"));

    for prefix in ["GOAL", "WAKE", "SLEE", "INTE", "GLAS"] {
        send(
            &mut hw,
            &mut face,
            &mut settings,
            Event::Button(Button::Light, ButtonEvent::Up),
        );
        assert!(hw.text().starts_with(prefix), "actual: {}", hw.text());
    }
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::Up),
    );
    assert!(hw.text().starts_with("GLAS"));
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Mode, ButtonEvent::Up),
    );
    assert!(hw.text().starts_with("HY"));
}

#[test]
fn deviation_and_equal_wake_sleep_render_without_panicking() {
    let (mut hw, mut face, mut settings) = activated(9);
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::LongPress),
    );
    assert!(hw.text().ends_with("ml"));

    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::LongPress),
    );
    // GLASS -> GOAL -> WAKE; reset wake to the default, then advance it to 9.
    for _ in 0..2 {
        send(
            &mut hw,
            &mut face,
            &mut settings,
            Event::Button(Button::Light, ButtonEvent::Up),
        );
    }
    for _ in 0..2 {
        send(
            &mut hw,
            &mut face,
            &mut settings,
            Event::Button(Button::Alarm, ButtonEvent::Up),
        );
    }
    // Return to tracking, then enter settings and make sleep equal to wake.
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Mode, ButtonEvent::Up),
    );
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::LongPress),
    );
    for _ in 0..3 {
        send(
            &mut hw,
            &mut face,
            &mut settings,
            Event::Button(Button::Light, ButtonEvent::Up),
        );
    }
    // SLEEP is now selected; reset is 22, then wrap around to 9.
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::LongPress),
    );
    for _ in 0..11 {
        send(
            &mut hw,
            &mut face,
            &mut settings,
            Event::Button(Button::Alarm, ButtonEvent::Up),
        );
    }
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Mode, ButtonEvent::Up),
    );
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::LongPress),
    );
    assert!(hw.text().contains("0000ml"));
}

#[test]
fn empty_log_and_log_navigation_are_safe() {
    let (mut hw, mut face, mut settings) = activated(9);
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::ReallyLongPress),
    );
    assert!(hw.text().contains("no dat"));
    for _ in 0..3 {
        send(
            &mut hw,
            &mut face,
            &mut settings,
            Event::Button(Button::Light, ButtonEvent::Up),
        );
        send(
            &mut hw,
            &mut face,
            &mut settings,
            Event::Button(Button::Alarm, ButtonEvent::Up),
        );
    }
    assert!(hw.text().starts_with("LOG"));
}

#[test]
fn cross_midnight_and_alert_predicates_are_deterministic() {
    let (mut hw, mut face, mut settings) = activated(2);
    // Default wake/sleep is 07:00..22:00; configure the cross-midnight case.
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::LongPress),
    );
    for _ in 0..2 {
        send(
            &mut hw,
            &mut face,
            &mut settings,
            Event::Button(Button::Light, ButtonEvent::Up),
        );
    }
    // Wake is selected: reset to 7, then advance 15 hours to 22.
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::LongPress),
    );
    for _ in 0..15 {
        send(
            &mut hw,
            &mut face,
            &mut settings,
            Event::Button(Button::Alarm, ButtonEvent::Up),
        );
    }
    // Move to sleep, reset 22, then advance nine hours to 07:00.
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::Up),
    );
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Alarm, ButtonEvent::LongPress),
    );
    for _ in 0..9 {
        send(
            &mut hw,
            &mut face,
            &mut settings,
            Event::Button(Button::Alarm, ButtonEvent::Up),
        );
    }
    // Enable the alert policy, then return to tracking.
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::LongPress),
    );
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Mode, ButtonEvent::Up),
    );
    hw.set_time(dt(2024, 2, 29, 2, 0, 0));
    seam::with_hw(&mut hw, || assert!(face.wants_background_task(&settings)));
    hw.set_time(dt(2024, 2, 29, 12, 0, 0));
    seam::with_hw(&mut hw, || assert!(!face.wants_background_task(&settings)));
}

#[test]
fn alert_enable_and_resign_are_safe() {
    let (mut hw, mut face, mut settings) = activated(9);
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::LongPress),
    );
    // Toggle the alert policy; delivery remains outside this host seam.
    send(
        &mut hw,
        &mut face,
        &mut settings,
        Event::Button(Button::Light, ButtonEvent::LongPress),
    );
    seam::with_hw(&mut hw, || face.resign(&mut settings));
    seam::with_hw(&mut hw, || face.resign(&mut settings));
}
