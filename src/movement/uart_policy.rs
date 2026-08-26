//! Bounded runtime policy for the debug UART shell.
//!
//! The persisted preference is separate from the live session. A reboot never
//! enables the UART by itself: a user must consent again from Settings or
//! Diagnostics. This keeps the default and boot state power-safe.

use super::types::Settings;

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
}
