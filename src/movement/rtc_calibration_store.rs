//! The single firmware path for persisted, temperature-compensated RTC drift.
//!
//! A missing, old, or corrupt record is deliberately indistinguishable from a
//! disabled calibration. No default environmental reading is substituted.

use crate::watch::{rtc, storage, thermistor};
use sensor_watch_core::rtc_calibration::RtcCalibration;

pub const PROFILE_OFFSET: u32 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyResult {
    Applied,
    AlreadyApplied,
    Disabled,
    SensorUnavailable,
}

pub fn load() -> Option<RtcCalibration> {
    let mut bytes = [0u8; sensor_watch_core::rtc_calibration::SERIALIZED_LEN];
    if !storage::wear_leveled_read_namespaced(
        storage::RTC_CALIBRATION_NAMESPACE,
        PROFILE_OFFSET,
        &mut bytes,
    ) {
        return None;
    }
    RtcCalibration::decode(&bytes)
}

pub fn save(profile: RtcCalibration) -> bool {
    if !profile.is_enabled() {
        return false;
    }
    storage::wear_leveled_write_namespaced(
        storage::RTC_CALIBRATION_NAMESPACE,
        PROFILE_OFFSET,
        &profile.encode(),
    )
}

/// Applies one bounded correction using a validated thermistor measurement.
/// `manual_ppm` is kept separate so this path never erases a manual correction.
pub fn apply(manual_ppm: i16) -> ApplyResult {
    let Some(profile) = load() else {
        return ApplyResult::Disabled;
    };

    let mut sensor = thermistor::Thermistor::new();
    sensor.begin();
    let Ok(temperature_c) = sensor.read_celsius() else {
        return ApplyResult::SensorUnavailable;
    };
    if !temperature_c.is_finite() {
        return ApplyResult::SensorUnavailable;
    }

    let correction = libm::roundf(profile.combined_correction_ppm(manual_ppm as f32, temperature_c))
        .clamp(-127.0, 127.0) as i16;
    if rtc::freqcorr_read() == correction {
        return ApplyResult::AlreadyApplied;
    }
    let sign = i16::from(correction < 0);
    rtc::freqcorr_write(correction.unsigned_abs() as i16, sign);
    ApplyResult::Applied
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watch::seam;
    use sensor_watch_core::mock_hw::{Hw, MockHw};

    static STORAGE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn persistence_round_trip() {
        let _lock = STORAGE_LOCK.lock().unwrap();
        let profile = RtcCalibration::new(-12.3, 1.25, 23.5);
        assert!(save(profile));
        let loaded = load().unwrap();
        assert!((loaded.base_ppm - profile.base_ppm).abs() <= 0.05);
    }

    #[test]
    fn corrupt_record_is_ignored() {
        let _lock = STORAGE_LOCK.lock().unwrap();
        // Calibration owns rows 12..17 after the wear-log expansion. Clear the
        // whole namespace partition so no older profile can mask the corrupt one.
        for row in 12..18 {
            assert!(storage::erase(row));
        }
        let corrupt = [0x52u8; sensor_watch_core::rtc_calibration::SERIALIZED_LEN];
        assert!(storage::wear_leveled_write_namespaced(
            storage::RTC_CALIBRATION_NAMESPACE,
            PROFILE_OFFSET,
            &corrupt,
        ));
        assert!(load().is_none());
    }

    #[test]
    fn unavailable_sensor_fails_closed_without_writing_rtc() {
        let _lock = STORAGE_LOCK.lock().unwrap();
        let mut hw = MockHw::default();
        let profile = RtcCalibration::new(10.0, 2.0, 25.0);
        assert!(save(profile));
        let result = seam::with_hw(&mut hw, || apply(4));
        assert_eq!(result, ApplyResult::SensorUnavailable);
        assert_eq!(hw.freqcorr_read(), 0);
    }
}
