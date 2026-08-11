//! Fixed-size structured event storage for firmware diagnostics.
//!
//! This deliberately contains no formatting, allocation, atomics, or hardware
//! access. The firmware wrapper supplies synchronization and timestamps while
//! this type provides the bounded ring-buffer behavior that can be tested on a
//! host.

/// A compact diagnostic event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    /// Monotonically increasing event sequence, wrapping on overflow.
    pub sequence: u32,
    /// Caller-supplied timestamp (normally the packed RTC value).
    pub timestamp: u32,
    /// Stable event code.
    pub code: u8,
    /// Small event-specific value.
    pub data: u16,
}

/// A fixed-capacity FIFO ring. New events replace the oldest event when full.
pub struct EventLog<const N: usize> {
    entries: [Event; N],
    next: usize,
    len: usize,
    sequence: u32,
}

impl<const N: usize> EventLog<N> {
    /// Creates an empty log with sequence numbers starting at zero.
    pub const fn new() -> Self {
        Self {
            entries: [Event {
                sequence: 0,
                timestamp: 0,
                code: 0,
                data: 0,
            }; N],
            next: 0,
            len: 0,
            sequence: 0,
        }
    }

    /// Appends an event and returns its assigned sequence number.
    pub fn push(&mut self, timestamp: u32, code: u8, data: u16) -> u32 {
        let sequence = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);
        if N == 0 {
            return sequence;
        }
        self.entries[self.next] = Event {
            sequence,
            timestamp,
            code,
            data,
        };
        self.next = (self.next + 1) % N;
        self.len = (self.len + 1).min(N);
        sequence
    }

    /// Removes all stored events. Sequence numbers continue across a clear.
    pub fn clear(&mut self) {
        self.next = 0;
        self.len = 0;
    }

    /// Number of events currently retained.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns whether no events are currently retained.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the event at `index`, oldest first.
    pub fn get(&self, index: usize) -> Option<Event> {
        if index >= self.len || N == 0 {
            return None;
        }
        let oldest = (self.next + N - self.len) % N;
        Some(self.entries[(oldest + index) % N])
    }
}

impl<const N: usize> Default for EventLog<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{Event, EventLog};

    #[test]
    fn retains_events_in_order() {
        let mut log = EventLog::<3>::new();
        log.push(10, 1, 2);
        log.push(11, 2, 3);
        assert_eq!(log.len(), 2);
        assert_eq!(
            log.get(0),
            Some(Event {
                sequence: 0,
                timestamp: 10,
                code: 1,
                data: 2
            })
        );
        assert_eq!(log.get(1).unwrap().sequence, 1);
    }

    #[test]
    fn overwrites_oldest_when_full() {
        let mut log = EventLog::<2>::new();
        log.push(1, 7, 0);
        log.push(2, 8, 0);
        log.push(3, 9, 0);
        assert_eq!(log.len(), 2);
        assert_eq!(log.get(0).unwrap().sequence, 1);
        assert_eq!(log.get(1).unwrap().sequence, 2);
        assert_eq!(log.get(0).unwrap().timestamp, 2);
    }

    #[test]
    fn clear_keeps_sequence_monotonic() {
        let mut log = EventLog::<2>::new();
        assert_eq!(log.push(0, 1, 0), 0);
        log.clear();
        assert_eq!(log.len(), 0);
        assert_eq!(log.push(0, 2, 0), 1);
    }

    #[test]
    fn zero_capacity_is_safe() {
        let mut log = EventLog::<0>::new();
        assert_eq!(log.push(0, 1, 0), 0);
        assert_eq!(log.len(), 0);
        assert!(log.is_empty());
        assert_eq!(log.get(0), None);
    }

    #[test]
    fn is_empty_tracks_retained_events() {
        let mut log = EventLog::<1>::new();
        assert!(log.is_empty());
        log.push(0, 1, 0);
        assert!(!log.is_empty());
        log.clear();
        assert!(log.is_empty());
    }

    #[test]
    fn ring_invariants_hold_at_capacity_boundaries() {
        for capacity in 1..=4 {
            match capacity {
                1 => assert_ring_contents::<1>(),
                2 => assert_ring_contents::<2>(),
                3 => assert_ring_contents::<3>(),
                4 => assert_ring_contents::<4>(),
                _ => unreachable!(),
            }
        }
    }

    fn assert_ring_contents<const N: usize>() {
        let mut log = EventLog::<N>::new();
        for value in 0..(N as u32 * 3 + 1) {
            assert_eq!(log.push(value, value as u8, value as u16), value);
            assert!(log.len() <= N);
            for index in 0..log.len() {
                assert_eq!(
                    log.get(index).expect("valid ring index").data,
                    (value + 1 - log.len() as u32 + index as u32) as u16
                );
            }
        }
        assert_eq!(log.get(N), None);
    }
}
