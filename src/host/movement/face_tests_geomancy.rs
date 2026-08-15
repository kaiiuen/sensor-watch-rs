//! Deterministic host coverage for the REAL geomancy face.

use crate::movement::geomancy::GeomancyFace;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::seam;
use sensor_watch_core::mock_hw::{MockHw, dt};

#[test]
fn real_geomancy_activation_buttons_ticks_and_display_are_safe() {
    let mut mock = MockHw::new();
    mock.set_time(dt(2023, 1, 6, 15, 4, 0));
    let mut settings = Settings::default();
    let mut face = GeomancyFace::new();

    seam::with_hw(&mut mock, || face.activate(&settings));
    assert!(mock.text().contains("IChing"), "actual: {}", mock.text());

    // Light selects the geomantic-figure page. Alarm starts its deterministic
    // animation, and ticks drive it to the settled figure without panicking.
    seam::with_hw(&mut mock, || {
        face.loop_(Event::Button(Button::Light, ButtonEvent::Up), &mut settings)
    });
    assert!(mock.text().contains("GeomCy"), "actual: {}", mock.text());

    seam::with_hw(&mut mock, || {
        face.loop_(Event::Button(Button::Alarm, ButtonEvent::Up), &mut settings)
    });
    for _ in 0..12 {
        seam::with_hw(&mut mock, || face.loop_(Event::Tick, &mut settings));
    }

    // Long press redraws the settled figure with its caption enabled; another
    // page switch must remain safe after the animation has completed.
    seam::with_hw(&mut mock, || {
        face.loop_(
            Event::Button(Button::Alarm, ButtonEvent::LongPress),
            &mut settings,
        );
        face.loop_(Event::Button(Button::Light, ButtonEvent::Up), &mut settings);
    });
    assert!(mock.text().contains("IChing"), "actual: {}", mock.text());
}
