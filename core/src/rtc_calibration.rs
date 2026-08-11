//! Pure temperature-compensated RTC calibration math.
//!
//! The model is opt-in: callers must provide a calibration before it can affect
//! a clock. Values are deliberately bounded before use or persistence.

pub const CALIBRATION_VERSION: u8 = 1;
pub const MAX_BASE_PPM: f32 = 1_000.0;
pub const MAX_TEMPERATURE_COEFFICIENT: f32 = 100.0;
pub const MIN_REFERENCE_TEMPERATURE_C: f32 = -40.0;
pub const MAX_REFERENCE_TEMPERATURE_C: f32 = 85.0;
pub const MAX_TEMPERATURE_C: f32 = 125.0;
pub const MIN_TEMPERATURE_C: f32 = -80.0;
pub const SERIALIZED_LEN: usize = 12;
const MAGIC: [u8; 2] = *b"RC";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtcCalibration {
    pub version: u8,
    /// Correction applied to the RTC at `reference_temperature_c`.
    pub base_ppm: f32,
    /// Change in correction, in ppm per degree Celsius.
    pub temperature_coefficient_ppm_per_c: f32,
    pub reference_temperature_c: f32,
}

impl RtcCalibration {
    pub const fn disabled() -> Self {
        Self {
            version: 0,
            base_ppm: 0.0,
            temperature_coefficient_ppm_per_c: 0.0,
            reference_temperature_c: 25.0,
        }
    }

    pub fn new(base_ppm: f32, coefficient: f32, reference_temperature_c: f32) -> Self {
        Self {
            version: CALIBRATION_VERSION,
            base_ppm: clamp_finite(base_ppm, -MAX_BASE_PPM, MAX_BASE_PPM),
            temperature_coefficient_ppm_per_c: clamp_finite(
                coefficient,
                -MAX_TEMPERATURE_COEFFICIENT,
                MAX_TEMPERATURE_COEFFICIENT,
            ),
            reference_temperature_c: clamp_finite(
                reference_temperature_c,
                MIN_REFERENCE_TEMPERATURE_C,
                MAX_REFERENCE_TEMPERATURE_C,
            ),
        }
    }

    pub const fn is_enabled(self) -> bool {
        self.version == CALIBRATION_VERSION
    }

    pub fn correction_ppm(self, temperature_c: f32) -> f32 {
        if !self.is_enabled() || !temperature_c.is_finite() {
            return 0.0;
        }
        let temperature = temperature_c.clamp(MIN_TEMPERATURE_C, MAX_TEMPERATURE_C);
        (self.base_ppm
            + self.temperature_coefficient_ppm_per_c * (temperature - self.reference_temperature_c))
            .clamp(-MAX_BASE_PPM, MAX_BASE_PPM)
    }

    /// Encodes a stable, endian-defined record with a checksum.
    pub fn encode(self) -> [u8; SERIALIZED_LEN] {
        let value = Self::new(
            self.base_ppm,
            self.temperature_coefficient_ppm_per_c,
            self.reference_temperature_c,
        );
        let mut out = [0u8; SERIALIZED_LEN];
        out[0..2].copy_from_slice(&MAGIC);
        out[2] = value.version;
        out[3..5].copy_from_slice(&quantize(value.base_ppm, 10.0).to_le_bytes());
        out[5..7].copy_from_slice(
            &quantize(value.temperature_coefficient_ppm_per_c, 100.0).to_le_bytes(),
        );
        out[7..9].copy_from_slice(&quantize(value.reference_temperature_c, 100.0).to_le_bytes());
        let checksum = checksum(&out[..9]);
        out[9..11].copy_from_slice(&checksum.to_le_bytes());
        out[11] = 0;
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != SERIALIZED_LEN
            || bytes[0..2] != MAGIC
            || bytes[2] != CALIBRATION_VERSION
            || checksum(&bytes[..9]) != u16::from_le_bytes([bytes[9], bytes[10]])
            || bytes[11] != 0
        {
            return None;
        }
        Some(Self::new(
            i16::from_le_bytes([bytes[3], bytes[4]]) as f32 / 10.0,
            i16::from_le_bytes([bytes[5], bytes[6]]) as f32 / 100.0,
            i16::from_le_bytes([bytes[7], bytes[8]]) as f32 / 100.0,
        ))
    }
}

fn clamp_finite(value: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        0.0_f32.clamp(min, max)
    }
}
fn quantize(value: f32, scale: f32) -> i16 {
    libm::roundf(value * scale).clamp(i16::MIN as f32, i16::MAX as f32) as i16
}
fn checksum(bytes: &[u8]) -> u16 {
    bytes
        .iter()
        .fold(0xA5A5u16, |sum, byte| sum.rotate_left(3) ^ *byte as u16)
}

/// Fits correction values (`-measured drift`) to a line versus temperature.
pub fn recommended_calibration(
    samples: &[(f32, f32)],
    reference_temperature_c: f32,
) -> Option<RtcCalibration> {
    if samples.len() < 2 {
        return None;
    }
    let reference =
        reference_temperature_c.clamp(MIN_REFERENCE_TEMPERATURE_C, MAX_REFERENCE_TEMPERATURE_C);
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut sxx = 0.0;
    let mut sxy = 0.0;
    for &(temp, correction) in samples {
        if !temp.is_finite() || !correction.is_finite() {
            return None;
        }
        let x = temp - reference;
        sx += x;
        sy += correction;
        sxx += x * x;
        sxy += x * correction;
    }
    let n = samples.len() as f32;
    let denominator = n * sxx - sx * sx;
    if denominator.abs() < 1.0e-6 {
        return None;
    }
    let coefficient = (n * sxy - sx * sy) / denominator;
    let base = (sy - coefficient * sx) / n;
    Some(RtcCalibration::new(base, coefficient, reference))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clamps_and_interpolates() {
        let c = RtcCalibration::new(2.0, 3.0, 20.0);
        assert_eq!(c.correction_ppm(25.0), 17.0);
        assert_eq!(RtcCalibration::new(2000.0, 200.0, 200.0).base_ppm, 1000.0);
    }
    #[test]
    fn round_trip_is_stable() {
        let c = RtcCalibration::new(-12.34, 1.25, 23.5);
        let decoded = RtcCalibration::decode(&c.encode()).unwrap();
        assert!((decoded.base_ppm - c.base_ppm).abs() <= 0.05);
        assert_eq!(
            decoded.temperature_coefficient_ppm_per_c,
            c.temperature_coefficient_ppm_per_c
        );
        assert_eq!(decoded.reference_temperature_c, c.reference_temperature_c);
    }
    #[test]
    fn rejects_corruption_wrong_version_and_reserved_bytes() {
        let c = RtcCalibration::new(1.0, 2.0, 25.0);
        let mut bytes = c.encode();
        bytes[4] ^= 1;
        assert!(RtcCalibration::decode(&bytes).is_none());
        bytes = c.encode();
        bytes[2] = 2;
        assert!(RtcCalibration::decode(&bytes).is_none());
        bytes = c.encode();
        bytes[11] = 1;
        assert!(RtcCalibration::decode(&bytes).is_none());
    }
    #[test]
    fn fits_line_deterministically() {
        let c = recommended_calibration(&[(15.0, -2.0), (25.0, 8.0), (35.0, 18.0)], 25.0).unwrap();
        assert!((c.base_ppm - 8.0).abs() < 1e-5);
        assert!((c.temperature_coefficient_ppm_per_c - 1.0).abs() < 1e-5);
    }
}
