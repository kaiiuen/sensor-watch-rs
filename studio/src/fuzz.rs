//! Fuzz testing for the watch-face simulation engine.
//!
//! Runs randomized button sequences and time inputs through the face engine to
//! ensure it never panics, never produces out-of-range display characters, and
//! stays within valid state. This is host-testable and gives confidence that the
//! face logic is robust against arbitrary input.

use super::face_sim::{FaceButton, FaceEngine, SimTime};

/// A simple deterministic PRNG (xorshift) so fuzz runs are reproducible.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn next_usize(&mut self, max: usize) -> usize {
        (self.next_u64() % max as u64) as usize
    }
}

/// Runs a fuzz pass over the given face name with a random button/tick sequence.
///
/// Returns the number of iterations run. Panics (or returns an error) if the
/// engine ever produces an invalid display character.
pub fn fuzz_face(name: &str, iterations: usize, seed: u64) -> Result<usize, String> {
    let mut rng = Rng::new(seed);
    let mut engine = FaceEngine::new(name);

    let mut time = SimTime {
        year: 2025,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
        second: 0,
        weekday: 3,
    };

    for i in 0..iterations {
        // Randomly tick or press a button.
        match rng.next_usize(4) {
            0 => engine.tick(),
            1 => engine.press(FaceButton::Light),
            2 => engine.press(FaceButton::Alarm),
            _ => {
                // Advance the simulated time by a random amount.
                time.second = (time.second + rng.next_usize(60) as u32) % 60;
                time.minute = (time.minute + rng.next_usize(60) as u32) % 60;
                time.hour = (time.hour + rng.next_usize(24) as u32) % 24;
            }
        }

        // Render and validate the display.
        let d = engine.render(&time);
        for &c in &d.chars {
            if c != ' ' && !c.is_ascii_graphic() {
                return Err(format!(
                    "invalid display char {c:?} (0x{:02X}) at iteration {i}",
                    c as u32
                ));
            }
        }
    }
    Ok(iterations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzz_stopwatch_is_stable() {
        assert!(fuzz_face("STOPWATCH", 5000, 42).is_ok());
    }

    #[test]
    fn fuzz_timer_is_stable() {
        assert!(fuzz_face("TIMER", 5000, 7).is_ok());
    }

    #[test]
    fn fuzz_diagnostics_is_stable() {
        if let Err(e) = fuzz_face("DIAGNOSTICS", 5000, 99) {
            panic!("fuzz failed: {e}");
        }
    }

    #[test]
    fn fuzz_clock_is_stable() {
        assert!(fuzz_face("SIMPLE_CLOCK", 5000, 1234).is_ok());
    }

    #[test]
    fn rng_is_deterministic() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(1);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}
