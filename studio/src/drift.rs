//! Drift calibration.
//!
//! Measures the watch's crystal drift (parts-per-million) by comparing the
//! watch's reported time against a reference (NTP). Over a measurement window,
//! the accumulated error is converted to a PPM correction that can be applied
//! to the RTC frequency-correction register.

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

    /// Records a sample. The first call sets the start; the second sets the end
    /// and computes the PPM.
    pub fn record(&mut self, watch_seconds: u64, reference_seconds: u64) {
        let sample = DriftSample {
            watch_seconds,
            reference_seconds,
        };
        if self.start.is_none() {
            self.start = Some(sample);
        } else {
            self.end = Some(sample);
            if let (Some(s), Some(e)) = (self.start, self.end) {
                let elapsed = e.reference_seconds.saturating_sub(s.reference_seconds) as f64;
                let watch_elapsed = e.watch_seconds.saturating_sub(s.watch_seconds) as f64;
                let error = watch_elapsed - elapsed;
                self.ppm = compute_ppm(elapsed, error);
            }
        }
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
        s.record(1_700_000_000, 1_700_000_000);
        // End: reference advanced 100000s, but the watch advanced 100010s
        // (it ran 10s fast). Error = +10s over 100000s = +100 ppm.
        s.record(1_700_100_010, 1_700_100_000);
        assert!((s.ppm - 100.0).abs() < 1e-6);
    }
}
