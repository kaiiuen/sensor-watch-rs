//! Focused host coverage for the REAL accelerometer interrupt count face.

use super::accel_interrupt_count::AccelInterruptCountFace;
use super::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::seam;
use sensor_watch_core::mock_hw::{Indicator, MockHw};

fn render(
    face: &mut AccelInterruptCountFace,
    event: Event,
    hw: &mut MockHw,
    settings: &mut Settings,
) {
    seam::with_hw(hw, || face.loop_(event, settings));
}

#[test]
fn real_accel_interrupt_count_activation_and_start_stop_are_safe_without_sensor() {
    let mut hw = MockHw::new();
    let mut settings = Settings::default();
    let mut face = AccelInterruptCountFace::new();

    // Host activation deliberately reports no accelerometer, but must still
    // leave the face usable for injected tap events.
    seam::with_hw(&mut hw, || face.activate(&settings));
    render(&mut face, Event::Tick, &mut hw, &mut settings);
    assert_eq!(&hw.chars[..4], &['A', 'C', '1', 'N']);
    assert!(!hw.indicator(Indicator::Signal));

    render(
        &mut face,
        Event::Button(Button::Alarm, ButtonEvent::Up),
        &mut hw,
        &mut settings,
    );
    assert!(hw.indicator(Indicator::Signal));
    render(
        &mut face,
        Event::Button(Button::Alarm, ButtonEvent::Up),
        &mut hw,
        &mut settings,
    );
    assert!(!hw.indicator(Indicator::Signal));

    // Resigning twice is intentionally harmless, including with no sensor.
    face.resign(&mut settings);
    face.resign(&mut settings);
}

#[test]
fn real_accel_interrupt_count_counts_injected_taps_only_while_running() {
    let mut hw = MockHw::new();
    let mut settings = Settings::default();
    let mut face = AccelInterruptCountFace::new();
    seam::with_hw(&mut hw, || face.activate(&settings));

    render(&mut face, Event::SingleTap, &mut hw, &mut settings);
    render(&mut face, Event::DoubleTap, &mut hw, &mut settings);
    assert_eq!(&hw.chars[4..10], &['0', '0', '0', '0', '0', '0']);

    render(
        &mut face,
        Event::Button(Button::Alarm, ButtonEvent::Up),
        &mut hw,
        &mut settings,
    );
    render(&mut face, Event::SingleTap, &mut hw, &mut settings);
    render(&mut face, Event::DoubleTap, &mut hw, &mut settings);
    assert_eq!(&hw.chars[4..10], &['0', '0', '0', '0', '0', '2']);

    render(
        &mut face,
        Event::Button(Button::Alarm, ButtonEvent::Up),
        &mut hw,
        &mut settings,
    );
    render(&mut face, Event::SingleTap, &mut hw, &mut settings);
    assert_eq!(&hw.chars[4..10], &['0', '0', '0', '0', '0', '2']);
}

#[test]
fn real_accel_interrupt_count_threshold_setting_and_stopped_reset_work() {
    let mut hw = MockHw::new();
    let mut settings = Settings::default();
    let mut face = AccelInterruptCountFace::new();
    seam::with_hw(&mut hw, || face.activate(&settings));

    render(
        &mut face,
        Event::Button(Button::Alarm, ButtonEvent::LongPress),
        &mut hw,
        &mut settings,
    );
    render(
        &mut face,
        Event::Button(Button::Light, ButtonEvent::Down),
        &mut hw,
        &mut settings,
    );
    assert_eq!(&hw.chars[..8], &['T', 'H', ' ', ' ', '0', '0', '1', '1']);
    render(&mut face, Event::Tick, &mut hw, &mut settings);
    render(
        &mut face,
        Event::Button(Button::Alarm, ButtonEvent::Up),
        &mut hw,
        &mut settings,
    );

    render(
        &mut face,
        Event::Button(Button::Light, ButtonEvent::Down),
        &mut hw,
        &mut settings,
    );
    assert_eq!(&hw.chars[4..10], &['0', '0', '0', '0', '0', '0']);
}
