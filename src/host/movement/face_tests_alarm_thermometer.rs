use super::{alarm_thermometer, types};
use crate::watch::seam;
use sensor_watch_core::mock_hw::{Indicator, MockHw, dt};
use types::WatchFace;

fn settings() -> types::Settings {
    let mut settings = types::Settings::default();
    settings.set_clock_mode_24h(true);
    settings
}

fn render(
    face: &mut alarm_thermometer::AlarmThermometerFace,
    mock: &mut MockHw,
    event: types::Event,
    settings: &mut types::Settings,
) {
    seam::with_hw(mock, || face.loop_(event, settings));
}

#[test]
fn real_alarm_thermometer_activation_displays_fixed_celsius_temperature() {
    let mut mock = MockHw::new();
    mock.set_time(dt(2024, 2, 29, 15, 4, 0));
    let mut settings = settings();
    let mut face = alarm_thermometer::AlarmThermometerFace::new();

    seam::with_hw(&mut mock, || face.activate(&settings));
    assert_eq!(mock.text(), "AT");

    render(&mut face, &mut mock, types::Event::Activate, &mut settings);
    // The host sensor is deliberately the face's known fixed 25°C model, not
    // fake hardware input: the real face writes "25.0#C" at LCD offset 4.
    assert_eq!(mock.text(), "AT  25.0#C");
}

#[test]
fn real_alarm_thermometer_alarm_bell_freezes_after_four_stable_samples() {
    let mut mock = MockHw::new();
    let mut settings = settings();
    let mut face = alarm_thermometer::AlarmThermometerFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));

    render(
        &mut face,
        &mut mock,
        types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
        &mut settings,
    );
    assert!(mock.indicator(Indicator::Bell));

    // Samples are taken only when seconds % 5 == 0. Four identical 25°C
    // samples are therefore required before the real face enters FREEZE.
    for second in [5, 10, 15, 20] {
        mock.set_time(dt(2024, 2, 29, 15, 4, second));
        render(&mut face, &mut mock, types::Event::Tick, &mut settings);
    }
    assert!(mock.indicator(Indicator::Signal));
    assert!(mock.indicator(Indicator::Bell));

    render(
        &mut face,
        &mut mock,
        types::Event::Button(types::Button::Alarm, types::ButtonEvent::Up),
        &mut settings,
    );
    assert!(!mock.indicator(Indicator::Bell));
    assert!(!mock.indicator(Indicator::Signal));
}

#[test]
fn real_alarm_thermometer_long_press_toggles_units_and_resign_is_safe() {
    let mut mock = MockHw::new();
    let mut settings = settings();
    let mut face = alarm_thermometer::AlarmThermometerFace::new();
    seam::with_hw(&mut mock, || face.activate(&settings));
    render(&mut face, &mut mock, types::Event::Activate, &mut settings);

    render(
        &mut face,
        &mut mock,
        types::Event::Button(types::Button::Alarm, types::ButtonEvent::LongPress),
        &mut settings,
    );
    assert!(settings.use_imperial_units());
    assert_eq!(mock.text(), "AT  77.0#F");

    seam::with_hw(&mut mock, || face.resign(&mut settings));
    // A second resign is safe for this face and mirrors the adapter's drop path.
    seam::with_hw(&mut mock, || face.resign(&mut settings));
}
