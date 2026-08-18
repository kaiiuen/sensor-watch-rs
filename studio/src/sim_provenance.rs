//! User-facing truth about how Studio obtains simulator face output.

#[allow(dead_code)]
pub const TOTAL_FACE_COUNT: usize = 111;
#[allow(dead_code)]
pub const REAL_FACE_COUNT: usize = 108;
#[allow(dead_code)]
pub const APPROXIMATION_FACE_COUNT: usize = 3;

#[cfg(feature = "real-faces")]
pub const STATUS: &str = "Simulation provenance: default real-faces runs 108 of 111 faces from the actual firmware face source files through translated host movement and HAL seams with MockHw. The other 3 faces use the separate face_sim approximation.";

#[cfg(not(feature = "real-faces"))]
pub const STATUS: &str = "Simulation provenance: real-faces is disabled, so all 111 faces use the separate face_sim approximation. No firmware face source files run in this mode.";

pub const LIMITATIONS: &str = "This is not full ARM firmware simulation or hardware simulation. MMIO, interrupts, sensors, power, RTC oscillator accuracy, peripheral electrical behavior, and some scheduling are modeled or stubbed and can diverge from a physical watch.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_counts_add_up() {
        assert_eq!(REAL_FACE_COUNT + APPROXIMATION_FACE_COUNT, TOTAL_FACE_COUNT);
    }

    #[cfg(feature = "real-faces")]
    #[test]
    fn real_face_status_names_path_and_counts() {
        assert!(STATUS.contains("default real-faces"));
        assert!(STATUS.contains("108 of 111"));
        assert!(STATUS.contains("actual firmware face source files"));
        assert!(STATUS.contains("translated host movement and HAL seams with MockHw"));
        assert!(STATUS.contains("3 faces"));
        assert!(STATUS.contains("face_sim approximation"));
    }

    #[cfg(not(feature = "real-faces"))]
    #[test]
    fn fallback_status_names_path_and_counts() {
        assert!(STATUS.contains("real-faces is disabled"));
        assert!(STATUS.contains("all 111 faces"));
        assert!(STATUS.contains("face_sim approximation"));
        assert!(STATUS.contains("No firmware face source files run"));
    }
}
