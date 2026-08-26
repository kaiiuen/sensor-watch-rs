//! Bounded runtime policy for the debug UART shell.
//!
//! The persisted preference is separate from the live session. A reboot never
//! enables the UART by itself: a user must consent again from Settings or
//! Diagnostics. This keeps the default and boot state power-safe.

use super::types::Settings;

/// Fixed capacity of the replaceable UART RX wake-event queue.
pub const UART_WAKE_RX_CAPACITY: usize = 64;

/// Hardware-independent status translated by a validated adapter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UartInterruptStatus(u8);

impl UartInterruptStatus {
    pub const RX_COMPLETE: Self = Self(1 << 0);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Coalesced events delivered to the main loop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UartWakeEvents(u8);

impl UartWakeEvents {
    pub const RX: Self = Self(1 << 0);
    pub const OVERFLOW: Self = Self(1 << 1);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, event: Self) -> bool {
        self.0 & event.0 == event.0
    }

    const fn insert(&mut self, event: Self) {
        self.0 |= event.0;
    }
}

/// Allocation-free state shared by the bounded adapter and main loop.
///
/// A full queue drops newer bytes and records overflow. Repeated RX status only
/// sets one pending event; parsing is never performed here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UartWakeState {
    enabled: bool,
    events: UartWakeEvents,
    bytes: [u8; UART_WAKE_RX_CAPACITY],
    read: usize,
    write: usize,
    len: usize,
}

impl UartWakeState {
    pub const fn new() -> Self {
        Self {
            enabled: false,
            events: UartWakeEvents::empty(),
            bytes: [0; UART_WAKE_RX_CAPACITY],
            read: 0,
            write: 0,
            len: 0,
        }
    }

    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn enable_uart_wake(&mut self) {
        self.enabled = true;
    }

    pub fn disable_uart_wake(&mut self) {
        self.enabled = false;
        self.events = UartWakeEvents::empty();
        self.read = 0;
        self.write = 0;
        self.len = 0;
    }

    /// Captures status only; no register access or parsing occurs.
    pub fn capture_interrupt_status(&mut self, status: UartInterruptStatus) {
        if self.enabled && status.contains(UartInterruptStatus::RX_COMPLETE) {
            self.events.insert(UartWakeEvents::RX);
        }
    }

    /// Enqueues at most one byte and never waits.
    pub fn enqueue_rx(&mut self, byte: u8) {
        if !self.enabled {
            return;
        }
        if self.len == UART_WAKE_RX_CAPACITY {
            self.events.insert(UartWakeEvents::OVERFLOW);
            return;
        }
        self.bytes[self.write] = byte;
        self.write = (self.write + 1) % UART_WAKE_RX_CAPACITY;
        self.len += 1;
        self.events.insert(UartWakeEvents::RX);
    }

    pub fn take_wake_events(&mut self) -> UartWakeEvents {
        let events = self.events;
        self.events = UartWakeEvents::empty();
        events
    }

    /// Drains at most `out.len()` bytes in the main loop.
    pub fn drain_rx(&mut self, out: &mut [u8]) -> usize {
        let mut drained = 0;
        while drained < out.len() && self.len != 0 {
            out[drained] = self.bytes[self.read];
            self.read = (self.read + 1) % UART_WAKE_RX_CAPACITY;
            self.len -= 1;
            drained += 1;
        }
        if self.len == 0 {
            self.events.0 &= !UartWakeEvents::RX.0;
        }
        drained
    }

    pub const fn queued_len(&self) -> usize {
        self.len
    }
}

impl Default for UartWakeState {
    fn default() -> Self {
        Self::new()
    }
}

/// UART wake never replaces the RTC/timer standby wake source.
pub const fn standby_wake_allowed(_uart_wake_enabled: bool) -> bool {
    true
}

/// UART inactivity timeout in the app's 128 Hz fast-tick units (five minutes).
pub const UART_IDLE_TIMEOUT_TICKS: u32 = 128 * 60 * 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UartRuntimePolicy {
    enabled: bool,
    last_activity: u32,
}

impl UartRuntimePolicy {
    pub const fn new() -> Self {
        Self {
            enabled: false,
            last_activity: 0,
        }
    }

    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Deliberate consent from a physical on-watch face.
    pub fn enable(&mut self, settings: &mut Settings, now: u32) {
        settings.set_uart_shell_enabled(true);
        self.enabled = true;
        self.last_activity = now;
    }

    /// Explicit user disable. This also clears the persisted preference.
    pub fn disable(&mut self, settings: &mut Settings) {
        settings.set_uart_shell_enabled(false);
        self.enabled = false;
        self.last_activity = 0;
    }

    /// Restore the safe boot state. A saved preference is not consent.
    pub fn boot(&mut self) {
        self.enabled = false;
        self.last_activity = 0;
    }

    /// Returns whether the live session remains enabled after this loop.
    pub fn observe_poll(&mut self, now: u32, had_input: bool) -> bool {
        if !self.enabled {
            return false;
        }
        if had_input {
            self.last_activity = now;
            true
        } else if now.wrapping_sub(self.last_activity) >= UART_IDLE_TIMEOUT_TICKS {
            self.enabled = false;
            false
        } else {
            true
        }
    }

    pub const fn last_activity(self) -> u32 {
        self.last_activity
    }

    /// Marks the session released; the hardware layer performs the pin reset.
    pub fn release(&mut self) {
        self.enabled = false;
        self.last_activity = 0;
    }
}

impl Default for UartRuntimePolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_off_even_when_preference_is_defaulted() {
        let settings = Settings::default();
        let policy = UartRuntimePolicy::new();
        assert!(!settings.uart_shell_enabled());
        assert!(!policy.enabled());
    }

    #[test]
    fn consent_enables_and_explicit_disable_persists() {
        let mut settings = Settings::default();
        let mut policy = UartRuntimePolicy::new();
        policy.enable(&mut settings, 10);
        assert!(settings.uart_shell_enabled());
        assert!(policy.enabled());
        policy.disable(&mut settings);
        assert!(!settings.uart_shell_enabled());
        assert!(!policy.enabled());
    }

    #[test]
    fn inactivity_timeout_disables_only_the_live_session() {
        let mut settings = Settings::default();
        let mut policy = UartRuntimePolicy::new();
        policy.enable(&mut settings, 100);
        assert!(policy.observe_poll(100 + UART_IDLE_TIMEOUT_TICKS - 1, false));
        assert!(!policy.observe_poll(100 + UART_IDLE_TIMEOUT_TICKS, false));
        assert!(settings.uart_shell_enabled());
    }

    #[test]
    fn input_refreshes_the_bounded_window() {
        let mut settings = Settings::default();
        let mut policy = UartRuntimePolicy::new();
        policy.enable(&mut settings, 0);
        assert!(policy.observe_poll(100, true));
        assert_eq!(policy.last_activity(), 100);
        assert!(policy.observe_poll(100 + UART_IDLE_TIMEOUT_TICKS - 1, false));
    }

    #[test]
    fn release_requires_the_hardware_shutdown_path() {
        let mut policy = UartRuntimePolicy::new();
        let mut settings = Settings::default();
        policy.enable(&mut settings, 1);
        policy.release();
        assert!(!policy.enabled());
    }

    #[test]
    fn persisted_preference_round_trips_without_becoming_live_consent() {
        let mut settings = Settings::default();
        settings.set_uart_shell_enabled(true);
        let restored = settings;
        let mut policy = UartRuntimePolicy::new();
        policy.boot();
        assert!(restored.uart_shell_enabled());
        assert!(!policy.enabled());
    }

    #[test]
    fn reboot_requires_new_physical_consent() {
        let mut settings = Settings::default();
        let mut policy = UartRuntimePolicy::new();
        policy.enable(&mut settings, 1);
        policy.boot();
        assert!(settings.uart_shell_enabled());
        assert!(!policy.enabled());
    }

    #[test]
    fn wake_flags_are_captured_and_coalesced() {
        let mut wake = UartWakeState::new();
        wake.enable_uart_wake();
        wake.capture_interrupt_status(UartInterruptStatus::RX_COMPLETE);
        wake.capture_interrupt_status(UartInterruptStatus::RX_COMPLETE);
        assert!(wake.take_wake_events().contains(UartWakeEvents::RX));
        assert_eq!(wake.take_wake_events(), UartWakeEvents::empty());
    }

    #[test]
    fn wake_ring_reports_overflow_without_overwriting_old_data() {
        let mut wake = UartWakeState::new();
        wake.enable_uart_wake();
        for byte in 0..UART_WAKE_RX_CAPACITY as u8 {
            wake.enqueue_rx(byte);
        }
        wake.enqueue_rx(0xff);
        assert!(wake.take_wake_events().contains(UartWakeEvents::OVERFLOW));
        let mut out = [0; UART_WAKE_RX_CAPACITY];
        assert_eq!(wake.drain_rx(&mut out), UART_WAKE_RX_CAPACITY);
        assert_eq!(out[0], 0);
        assert_eq!(
            out[UART_WAKE_RX_CAPACITY - 1],
            UART_WAKE_RX_CAPACITY as u8 - 1
        );
    }

    #[test]
    fn wake_enable_disable_is_fail_safe() {
        let mut wake = UartWakeState::new();
        wake.enqueue_rx(1);
        assert_eq!(wake.queued_len(), 0);
        wake.enable_uart_wake();
        wake.enqueue_rx(2);
        wake.disable_uart_wake();
        wake.enqueue_rx(3);
        assert_eq!(wake.queued_len(), 0);
    }

    #[test]
    fn standby_policy_does_not_depend_on_uart_wake() {
        assert!(standby_wake_allowed(false));
        assert!(standby_wake_allowed(true));
    }

    #[test]
    fn main_loop_drain_is_bounded_and_keeps_parser_out_of_wake_path() {
        let mut wake = UartWakeState::new();
        wake.enable_uart_wake();
        for byte in b"help\\n" {
            wake.enqueue_rx(*byte);
        }
        let mut first = [0; 2];
        assert_eq!(wake.drain_rx(&mut first), 2);
        assert_eq!(&first, b"he");
        const MAIN_LOOP_DRAIN_BOUND: usize = 4;
        let mut rest = [0; MAIN_LOOP_DRAIN_BOUND];
        assert_eq!(rest.len(), MAIN_LOOP_DRAIN_BOUND);
        assert_eq!(wake.drain_rx(&mut rest), 4);
        assert_eq!(&rest, b"lp\\n");
        assert_eq!(wake.queued_len(), 0);
    }
}
