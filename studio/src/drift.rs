//! Drift calibration.
//!
//! Measures the watch's crystal drift (parts-per-million) by comparing the
//! watch's reported time against a reference (NTP). Over a measurement window,
//! the accumulated error is converted to a PPM correction that can be applied
//! to the RTC frequency-correction register.

/// Minimum useful interval between drift samples. Shorter intervals are
/// dominated by the one-second resolution of the shell's `time` reading.
pub const MIN_SAMPLE_INTERVAL_SECONDS: u64 = 60;

/// A validated drift result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriftMeasurement {
    pub elapsed_seconds: u64,
    pub error_seconds: i64,
    pub ppm: f64,
    /// The integer argument for the firmware's `drift N` command. The command
    /// applies the opposite sign of the measured rate.
    pub correction_ppm: i32,
}

/// Computes the drift in parts-per-million given the elapsed real seconds and
/// the accumulated error in seconds.
///
/// `ppm = (error_seconds / elapsed_seconds) * 1_000_000`
pub fn compute_ppm(elapsed_seconds: f64, error_seconds: f64) -> f64 {
    if elapsed_seconds <= 0.0 {
        return 0.0;
    }
    (error_seconds / elapsed_seconds) * 1_000_000.0
}

/// Returns the integer correction sent to the firmware shell.
pub fn recommended_correction(ppm: f64) -> i32 {
    if !ppm.is_finite() {
        return 0;
    }
    (-ppm.round()).clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

/// Validates two epoch-based samples and calculates the measured drift.
///
/// Rejecting malformed samples here keeps the UI from displaying a plausible
/// but meaningless result when the watch clock moved backwards or the user
/// records both samples too close together.
pub fn measure(start: DriftSample, end: DriftSample) -> Result<DriftMeasurement, String> {
    if start.reference_seconds == 0
        || end.reference_seconds == 0
        || start.watch_seconds == 0
        || end.watch_seconds == 0
    {
        return Err("Samples must contain non-zero epoch timestamps".into());
    }
    if end.reference_seconds <= start.reference_seconds {
        return Err("The end reference must be later than the start reference".into());
    }
    let elapsed = end.reference_seconds - start.reference_seconds;
    if elapsed < MIN_SAMPLE_INTERVAL_SECONDS {
        return Err(format!(
            "Wait at least {MIN_SAMPLE_INTERVAL_SECONDS} seconds between samples"
        ));
    }
    if end.watch_seconds < start.watch_seconds {
        return Err("The watch time moved backwards. Reset and record again".into());
    }
    let watch_elapsed = end.watch_seconds - start.watch_seconds;
    let error = watch_elapsed as i128 - elapsed as i128;
    let error_seconds = error.clamp(i64::MIN as i128, i64::MAX as i128) as i64;
    let ppm = compute_ppm(elapsed as f64, error_seconds as f64);
    Ok(DriftMeasurement {
        elapsed_seconds: elapsed,
        error_seconds,
        ppm,
        correction_ppm: recommended_correction(ppm),
    })
}

/// A drift measurement sample: the watch's reported time and the reference time
/// at the same moment.
#[derive(Clone, Copy, Debug)]
pub struct DriftSample {
    /// The watch's reported UNIX time (seconds).
    pub watch_seconds: u64,
    /// The reference (NTP) UNIX time (seconds).
    pub reference_seconds: u64,
}

/// A drift calibration session.
#[derive(Clone, Debug)]
pub struct DriftSession {
    /// The start sample.
    pub start: Option<DriftSample>,
    /// The end sample.
    pub end: Option<DriftSample>,
    /// The computed PPM correction (set once both samples exist).
    pub ppm: f64,
}

impl DriftSession {
    pub fn new() -> Self {
        DriftSession {
            start: None,
            end: None,
            ppm: 0.0,
        }
    }

    /// Records a sample. The first call sets the start. The second validates
    /// the pair and computes the PPM. A failed second sample is not retained.
    pub fn record(
        &mut self,
        watch_seconds: u64,
        reference_seconds: u64,
    ) -> Result<&'static str, String> {
        let sample = DriftSample {
            watch_seconds,
            reference_seconds,
        };
        if watch_seconds == 0 || reference_seconds == 0 {
            return Err("Samples must contain non-zero epoch timestamps".into());
        }
        if self.start.is_none() {
            self.start = Some(sample);
            return Ok("start");
        }
        let start = self.start.expect("checked above");
        let measurement = measure(start, sample)?;
        self.end = Some(sample);
        self.ppm = measurement.ppm;
        Ok("end")
    }

    /// Resets the session.
    pub fn reset(&mut self) {
        self.start = None;
        self.end = None;
        self.ppm = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_elapsed_returns_zero() {
        assert_eq!(compute_ppm(0.0, 5.0), 0.0);
    }

    #[test]
    fn positive_drift() {
        // 10 ppm fast: watch gains 1 second over 100,000 seconds.
        let ppm = compute_ppm(100_000.0, 1.0);
        assert!((ppm - 10.0).abs() < 1e-9);
    }

    #[test]
    fn session_computes_ppm() {
        let mut s = DriftSession::new();
        // Start: watch and reference agree at ~2023.
        s.record(1_700_000_000, 1_700_000_000).unwrap();
        // End: reference advanced 100000s, but the watch advanced 100010s
        // (it ran 10s fast). Error = +10s over 100000s = +100 ppm.
        s.record(1_700_100_010, 1_700_100_000).unwrap();
        assert!((s.ppm - 100.0).abs() < 1e-6);
        assert_eq!(recommended_correction(s.ppm), -100);
    }

    #[test]
    fn rejects_short_and_reversed_samples() {
        let start = DriftSample {
            watch_seconds: 100,
            reference_seconds: 100,
        };
        let short = DriftSample {
            watch_seconds: 101,
            reference_seconds: 101,
        };
        assert!(measure(start, short).is_err());
        let reversed = DriftSample {
            watch_seconds: 99,
            reference_seconds: 200,
        };
        assert!(measure(start, reversed).is_err());
    }
}
