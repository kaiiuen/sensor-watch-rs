//! Focused host seam tests for the final three real-face migrations.

use super::{accelerometer_begin, clock_mode_24h, set_clock_mode_24h};
use super::{accelerometer_data_acquisition, advanced_alarm, settings_face, types};
use crate::watch::seam;
use sensor_watch_core::mock_hw::{MockHw, dt};
use types::WatchFace;

#[test]
fn accelerometer_capability_is_explicitly_unavailable() {
    assert!(!accelerometer_begin());

    let mut hw = MockHw::new();
    let mut settings = types::Settings::default();
    let mut face = accelerometer_data_acquisition::AccelerometerDataAcquisitionFace::new();
    seam::with_hw(&mut hw, || face.activate(&settings));
    seam::with_hw(&mut hw, || {
        face.loop_(types::Event::Activate, &mut settings)
    });
    assert_eq!(hw.text().len(), 10);
}

#[test]
fn advanced_alarm_and_settings_faces_use_bounded_display_output() {
    let mut hw = MockHw::new();
    hw.set_time(dt(2023, 1, 6, 7, 30, 0));
    let mut settings = types::Settings::default();

    let mut alarm = advanced_alarm::AdvancedAlarmFace::new();
    seam::with_hw(&mut hw, || alarm.activate(&settings));
    seam::with_hw(&mut hw, || {
        alarm.loop_(types::Event::Activate, &mut settings)
    });
    assert_eq!(hw.text().chars().count(), 10);

    let mut settings_face = settings_face::SettingsFace::new();
    seam::with_hw(&mut hw, || settings_face.activate(&settings));
    seam::with_hw(&mut hw, || {
        settings_face.loop_(types::Event::Activate, &mut settings)
    });
    assert_eq!(hw.text().chars().count(), 10);
}

#[test]
fn host_clock_mode_shadow_round_trips_the_three_firmware_modes() {
    for (mode, expected) in [
        (types::ClockMode::H12, types::ClockMode::H12),
        (types::ClockMode::H24, types::ClockMode::H24),
        (types::ClockMode::H024, types::ClockMode::H024),
    ] {
        set_clock_mode_24h(mode);
        assert_eq!(clock_mode_24h(), expected);
    }
}
