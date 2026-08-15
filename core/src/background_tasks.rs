//! Per-backend background-task scheduling for host tests.
//!
//! This models the firmware's one-shot, face-indexed scheduler without using
//! process-global mutable state. Host callers explicitly poll this registry and
//! inject the returned face's `BackgroundTask` event themselves.

use crate::datetime::DateTime;
use crate::safety::valid_datetime;

/// The number of watch faces in the firmware movement table.
pub const MOVEMENT_NUM_FACES: usize = 111;

const RTC_YEAR_CYCLE_SECONDS: u64 = (64 * 365 + 16) as u64 * 24 * 60 * 60;

/// Returns whether `target` is strictly after `now`, including the RTC's
/// supported year-63 to year-0 wrap.
pub fn is_future(now: DateTime, target: DateTime) -> bool {
    let now_timestamp = timestamp(now);
    let mut target_timestamp = timestamp(target);
    if target_timestamp <= now_timestamp {
        if now.year == 63 && target.year == 0 {
            target_timestamp += RTC_YEAR_CYCLE_SECONDS;
        } else {
            return false;
        }
    }
    target_timestamp > now_timestamp
}

fn timestamp(dt: DateTime) -> u64 {
    // This is only used for ordering valid RTC values. The civil-date details
    // match the firmware's utility conversion, while avoiding a dependency on
    // the host-only movement module.
    let mut days = 0u64;
    for year in 0..dt.year as u16 {
        days += if (2020 + year) % 4 == 0 { 366 } else { 365 };
    }
    for month in 1..dt.month {
        days += match month {
            2 if (2020 + dt.year as u16).is_multiple_of(4) => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
    }
    days += dt.day.saturating_sub(1) as u64;
    days * 86_400 + dt.hour as u64 * 3_600 + dt.minute as u64 * 60 + dt.second as u64
}

/// A face-indexed, one-shot background-task registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundTaskRegistry {
    tasks: [Option<DateTime>; MOVEMENT_NUM_FACES],
}

impl Default for BackgroundTaskRegistry {
    fn default() -> Self {
        Self {
            tasks: [None; MOVEMENT_NUM_FACES],
        }
    }
}

impl BackgroundTaskRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedules a task only when the face and date are valid and the target is
    /// strictly in the future. A valid future task replaces an existing task.
    pub fn schedule(&mut self, face_index: usize, now: DateTime, target: DateTime) {
        if face_index >= MOVEMENT_NUM_FACES
            || !valid_datetime(
                target.year,
                target.month,
                target.day,
                target.hour,
                target.minute,
                target.second,
            )
            || !is_future(now, target)
        {
            return;
        }
        self.tasks[face_index] = Some(target);
    }

    pub fn cancel(&mut self, face_index: usize) {
        if let Some(task) = self.tasks.get_mut(face_index) {
            *task = None;
        }
    }

    /// Clears and returns one due face, or `None` when no task is due.
    pub fn poll_due(&mut self, now: DateTime) -> Option<usize> {
        for (face_index, task) in self.tasks.iter_mut().enumerate() {
            if task.is_some_and(|target| !is_future(now, target)) {
                *task = None;
                return Some(face_index);
            }
        }
        None
    }

    pub fn scheduled(&self, face_index: usize) -> Option<DateTime> {
        self.tasks.get(face_index).copied().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(year: u8, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> DateTime {
        DateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
        }
    }

    #[test]
    fn empty_future_equal_past_cancel_replace_and_one_shot() {
        let now = dt(3, 1, 1, 0, 0, 0);
        let mut registry = BackgroundTaskRegistry::new();
        assert_eq!(registry.poll_due(now), None);
        registry.schedule(2, now, dt(3, 1, 1, 0, 0, 1));
        assert_eq!(registry.poll_due(now), None);
        registry.schedule(2, now, now);
        assert_eq!(registry.poll_due(dt(3, 1, 1, 0, 0, 1)), Some(2));
        assert_eq!(registry.poll_due(now), None);
        registry.schedule(2, now, dt(3, 1, 1, 0, 0, 2));
        registry.schedule(2, now, dt(3, 1, 1, 0, 0, 3));
        assert_eq!(registry.scheduled(2), Some(dt(3, 1, 1, 0, 0, 3)));
        registry.cancel(2);
        assert_eq!(registry.poll_due(dt(3, 1, 1, 0, 0, 3)), None);
        registry.schedule(2, now, dt(2, 12, 31, 23, 59, 59));
        assert_eq!(registry.scheduled(2), None);
    }

    #[test]
    fn invalid_dates_indices_and_year_wrap_match_firmware() {
        let now = dt(63, 12, 31, 23, 59, 59);
        let target = dt(0, 1, 1, 0, 0, 0);
        let mut registry = BackgroundTaskRegistry::new();
        registry.schedule(usize::MAX, now, target);
        registry.schedule(0, now, dt(0, 2, 30, 0, 0, 0));
        assert_eq!(registry.scheduled(0), None);
        registry.schedule(0, now, target);
        assert_eq!(registry.poll_due(now), None);
        assert_eq!(registry.poll_due(target), Some(0));
    }
}
