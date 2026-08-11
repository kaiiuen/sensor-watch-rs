//! Firmware structured event log.
//!
//! This fixed-size, no-heap ring is kept local to the firmware crate so the
//! ARM binary does not need to link the host-test support crate.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub sequence: u32,
    pub timestamp: u32,
    pub code: u8,
    pub data: u16,
}

struct EventLog<const N: usize> {
    entries: [Event; N],
    next: usize,
    len: usize,
    sequence: u32,
}

impl<const N: usize> EventLog<N> {
    const fn new() -> Self {
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

    fn push(&mut self, timestamp: u32, code: u8, data: u16) {
        let sequence = self.sequence;
        self.sequence = self.sequence.wrapping_add(1);
        if N == 0 {
            return;
        }
        self.entries[self.next] = Event {
            sequence,
            timestamp,
            code,
            data,
        };
        self.next = (self.next + 1) % N;
        self.len = (self.len + 1).min(N);
    }

    fn clear(&mut self) {
        self.next = 0;
        self.len = 0;
    }

    fn len(&self) -> usize {
        self.len
    }

    fn get(&self, index: usize) -> Option<Event> {
        if index >= self.len || N == 0 {
            return None;
        }
        let oldest = (self.next + N - self.len) % N;
        Some(self.entries[(oldest + index) % N])
    }
}

pub const CAPACITY: usize = 16;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventCode {
    Fault = 1,
    Reset = 2,
    Boot = 3,
    Shell = 4,
}

static mut LOG: EventLog<CAPACITY> = EventLog::new();

pub fn record(timestamp: u32, code: EventCode, data: u16) {
    critical_section::with(|_| unsafe { LOG.push(timestamp, code as u8, data) });
}

pub fn record_untimed(code: EventCode, data: u16) {
    record(0, code, data);
}

pub fn for_each(mut visit: impl FnMut(Event)) {
    let mut snapshot = [Event {
        sequence: 0,
        timestamp: 0,
        code: 0,
        data: 0,
    }; CAPACITY];
    let mut count = 0;
    critical_section::with(|_| unsafe {
        while count < LOG.len() {
            if let Some(event) = LOG.get(count) {
                snapshot[count] = event;
                count += 1;
            } else {
                break;
            }
        }
    });
    for event in snapshot[..count].iter().copied() {
        visit(event);
    }
}

pub fn clear() {
    critical_section::with(|_| unsafe { LOG.clear() });
}
