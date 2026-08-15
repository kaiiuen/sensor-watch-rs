use super::{activity, types};
use crate::watch::seam;
use sensor_watch_core::mock_hw::{MockHw, dt};
use types::WatchFace;

fn settings() -> types::Settings {
    let mut settings = types::Settings::default();
    settings.set_clock_mode_24h(true);
    settings
}

#[test]
fn real_activity_activates_and_shows_chooser() {
    let mut mock = MockHw::new();
    mock.set_time(dt(2024, 2, 29, 15, 4, 0));
    let mut settings = settings();
    let mut face = activity::ActivityFace::new();

    seam::with_hw(&mut mock, || face.activate(&settings));
    seam::with_hw(&mut mock, || {
        face.loop_(types::Event::Activate, &mut settings)
    });

    assert_eq!(mock.text(), "AC   bIKE");
}

#[test]
fn real_activity_ticks_logging_and_preserves_pause_boundary() {
    let mut mock = MockHw::new();
    mock.set_time(dt(2024, 2, 29, 15, 4, 0));
    let mut settings = settings();
    let mut face = activity::ActivityFace::new();

    seam::with_hw(&mut mock, || {
        face.loop_(types::Event::Activate, &mut settings)
    });
    seam::with_hw(&mut mock, || {
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::LongPress),
            &mut settings,
        )
    });
    for _ in 0..59 {
        seam::with_hw(&mut mock, || face.loop_(types::Event::Tick, &mut settings));
    }

    // A pause tick still advances total elapsed time, but is rendered as PAUSE.
    seam::with_hw(&mut mock, || {
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
            &mut settings,
        )
    });
    seam::with_hw(&mut mock, || face.loop_(types::Event::Tick, &mut settings));
    assert!(mock.text().contains("PAUSE"), "actual: {}", mock.text());

    // Resume at the 60-second minimum, then finish: the real face must accept
    // the exact minimum boundary and render its done screen.
    seam::with_hw(&mut mock, || {
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
            &mut settings,
        )
    });
    seam::with_hw(&mut mock, || {
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::LongPress),
            &mut settings,
        )
    });
    assert_eq!(mock.text(), "AC   dONE");
}

#[test]
fn real_activity_short_finish_still_shows_done_boundary() {
    let mut mock = MockHw::new();
    mock.set_time(dt(2024, 2, 29, 15, 4, 0));
    let mut settings = settings();
    let mut face = activity::ActivityFace::new();

    seam::with_hw(&mut mock, || {
        face.loop_(types::Event::Activate, &mut settings)
    });
    seam::with_hw(&mut mock, || {
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::LongPress),
            &mut settings,
        )
    });
    seam::with_hw(&mut mock, || {
        face.loop_(
            types::Event::Button(types::Button::Alarm, types::ButtonEvent::LongPress),
            &mut settings,
        )
    });
    assert!(mock.text().contains("dONE"), "actual: {}", mock.text());
}
