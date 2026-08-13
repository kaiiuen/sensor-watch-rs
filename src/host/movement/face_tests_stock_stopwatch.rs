//! Deterministic host coverage for the REAL stock stopwatch face.

use crate::movement::stock_stopwatch::{self, StockStopwatchFace};
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::seam;
use sensor_watch_core::mock_hw::{Indicator, MockHw};

#[test]
fn real_stock_stopwatch_activate_tick_and_buttons() {
    let mut mock = MockHw::new();
    let mut settings = Settings::default();
    let mut face = StockStopwatchFace::new();

    face.setup(&settings, 0);
    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || face.loop_(Event::Activate, &mut settings));
    assert!(mock.text().starts_with("ST"));
    assert!(mock.colon);
    assert!(!mock.indicator(Indicator::Lap));

    // Exercise the real TC2 entry point through the host timer shim. One second
    // at 128 Hz must cause the face's Tick path to redraw the seconds field.
    let before_tick_draws = mock.display_string_calls;
    seam::with_hw(&mut mock, || {
        face.loop_(
            Event::Button(Button::Alarm, ButtonEvent::Down),
            &mut settings,
        );
    });
    for _ in 0..128 {
        stock_stopwatch::host_tick();
    }
    seam::with_hw(&mut mock, || face.loop_(Event::Tick, &mut settings));
    assert!(mock.display_string_calls > before_tick_draws);
    assert!(mock.text().starts_with("ST"));

    // Light while running captures a lap, then a second press releases it.
    seam::with_hw(&mut mock, || {
        face.loop_(
            Event::Button(Button::Light, ButtonEvent::Down),
            &mut settings,
        );
    });
    assert!(mock.indicator(Indicator::Lap));
    seam::with_hw(&mut mock, || {
        face.loop_(
            Event::Button(Button::Light, ButtonEvent::Down),
            &mut settings,
        );
    });
    assert!(!mock.indicator(Indicator::Lap));

    // Stop the global timer state so this test leaves the firmware-style face
    // singleton in its inactive state for any following host tests.
    seam::with_hw(&mut mock, || {
        face.loop_(
            Event::Button(Button::Alarm, ButtonEvent::Down),
            &mut settings,
        );
    });
}
