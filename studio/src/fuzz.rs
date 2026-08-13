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
        if max == 0 {
            return 0;
        }
        (self.next_u64() % max as u64) as usize
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn advance_time(time: &mut SimTime, seconds: u32) {
    let total = time.hour * 3600 + time.minute * 60 + time.second + seconds;
    time.hour = (total / 3600) % 24;
    time.minute = (total % 3600) / 60;
    time.second = total % 60;

    let days = total / (24 * 3600);
    for _ in 0..days {
        time.day += 1;
        time.weekday = (time.weekday + 1) % 7;
        if time.day > days_in_month(time.year, time.month) {
            time.day = 1;
            time.month += 1;
            if time.month > 12 {
                time.month = 1;
                time.year += 1;
            }
        }
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
                // Advance by up to three days so carries exercise clock, date,
                // month, year, and weekday rollovers without making fuzz runs
                // unnecessarily expensive.
                advance_time(&mut time, 1 + rng.next_usize(3 * 24 * 60 * 60) as u32);
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

    #[cfg(feature = "real-faces")]
    use crate::real_face::RealFace;

    #[cfg(feature = "real-faces")]
    fn assert_valid_snapshot(
        name: &str,
        step: usize,
        snapshot: crate::real_face::RealFaceSnapshot,
    ) {
        for (position, character) in snapshot.chars.into_iter().enumerate() {
            assert!(
                character == '\0' || character == ' ' || character.is_ascii_graphic(),
                "{name} emitted invalid character {character:?} at step {step}, position {position}"
            );
        }
    }

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

    #[cfg(feature = "real-faces")]
    #[test]
    fn real_face_random_sequences_cover_interactive_mappings() {
        let valid_times = [
            (2023, 1, 1, 0, 0, 0),
            (2024, 2, 29, 11, 59, 59),
            (2024, 6, 30, 12, 0, 0),
            (2082, 12, 31, 23, 59, 58),
        ];
        let mut rng = Rng::new(0x5eed_cafe);
        let mut steps = 0usize;
        // These are the migrated stock faces whose host paths support arbitrary
        // button/tick input. Every mapping still receives lifecycle and date
        // coverage in the RealFace tests; faces with extra host-only setup are
        // intentionally excluded from arbitrary interaction stress.
        let sequence_faces = [
            "SIMPLE_CLOCK",
            "ALARM",
            "COUNTER",
            "WORLD_CLOCK",
            "STOPWATCH",
            "TIMER",
            "COUNTDOWN",
            "FLASHLIGHT",
        ];

        for name in sequence_faces {
            let mut face = RealFace::new(name).expect("mapping should construct");
            let (year, month, day, hour, minute, second) =
                valid_times[rng.next_usize(valid_times.len())];
            assert!(face.set_time(year, month, day, hour, minute, second));
            face.activate(rng.next_usize(2) == 0);
            assert_valid_snapshot(name, steps, face.snapshot());

            // Keep this deliberately bounded and sequential: the firmware seam is
            // a single global slot and must never be exercised by worker threads.
            for _ in 0..32 {
                match rng.next_usize(5) {
                    0 => face.tick(),
                    1 => face.press(true, false),
                    2 => face.press(false, true),
                    3 => face.press(true, true),
                    _ => {
                        let (year, month, day, hour, minute, second) =
                            valid_times[rng.next_usize(valid_times.len())];
                        assert!(face.set_time(year, month, day, hour, minute, second));
                    }
                }
                steps += 1;
                assert_valid_snapshot(name, steps, face.snapshot());
            }
        }

        // The complete mapping is covered by RealFace's lifecycle tests; this
        // fuzz pass intentionally targets only faces safe for arbitrary input.
        assert_eq!(steps, sequence_faces.len() * 32);
    }

    #[test]
    fn rng_is_deterministic() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(1);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn rng_zero_bound_returns_zero() {
        assert_eq!(Rng::new(42).next_usize(0), 0);
    }

    #[test]
    fn time_advance_rolls_date_month_year_and_weekday() {
        let mut time = SimTime {
            year: 2025,
            month: 12,
            day: 31,
            hour: 23,
            minute: 59,
            second: 59,
            weekday: 3,
        };
        advance_time(&mut time, 1);
        assert_eq!(
            (
                time.year,
                time.month,
                time.day,
                time.hour,
                time.minute,
                time.second,
                time.weekday
            ),
            (2026, 1, 1, 0, 0, 0, 4)
        );
    }
}
