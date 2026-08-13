//! Physical-presence authorization for mutating shell commands.
//!
//! The Alarm button is the board's service button. Mutations are authorized only
//! while it is held, and only for a bounded window. The bounded window is a
//! second defence against a missed release edge or a wedged input path.

use super::types::{Button, ButtonEvent, Event};

/// The button used as the physical-presence/service control.
pub const SERVICE_BUTTON: Button = Button::Alarm;

/// One authorization window, in 128 Hz fast-tick units (30 seconds).
pub const AUTH_WINDOW_TICKS: u16 = 128 * 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellAuthorization {
    held: bool,
    expires_at: u16,
}

impl ShellAuthorization {
    pub const fn new() -> Self {
        Self {
            held: false,
            expires_at: 0,
        }
    }

    /// Updates physical presence from a debounced movement event.
    pub fn observe(&mut self, event: Event, now: u16) {
        match event {
            Event::Button(SERVICE_BUTTON, ButtonEvent::Down) => {
                self.held = true;
                // FAST_TICKS is u16 and wraps. Store the window end; the
                // comparison below uses wrapping subtraction and remains
                // correct because the window is much shorter than one wrap.
                self.expires_at = now.wrapping_add(AUTH_WINDOW_TICKS);
            }
            Event::Button(SERVICE_BUTTON, ButtonEvent::Up | ButtonEvent::LongUp) => {
                self.revoke();
            }
            _ => {}
        }
    }

    /// Returns true only while the service button is held and the window lives.
    /// Expiry also clears the held state, so an old press cannot authorize later.
    pub fn is_authorized(&mut self, now: u16) -> bool {
        if !self.held || now.wrapping_sub(self.expires_at) < u16::MAX / 2 {
            self.revoke();
            false
        } else {
            true
        }
    }

    pub const fn held(&self) -> bool {
        self.held
    }

    pub const fn expires_at(&self) -> u16 {
        self.expires_at
    }

    pub fn revoke(&mut self) {
        self.held = false;
        self.expires_at = 0;
    }
}

impl Default for ShellAuthorization {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_locked() {
        let mut auth = ShellAuthorization::new();
        assert!(!auth.is_authorized(0));
        assert!(!auth.held());
    }

    #[test]
    fn service_button_unlocks_and_release_revokes() {
        let mut auth = ShellAuthorization::new();
        auth.observe(Event::Button(SERVICE_BUTTON, ButtonEvent::Down), 10);
        assert!(auth.is_authorized(10));
        auth.observe(Event::Button(SERVICE_BUTTON, ButtonEvent::Up), 11);
        assert!(!auth.is_authorized(11));
    }

    #[test]
    fn authorization_expires_automatically() {
        let mut auth = ShellAuthorization::new();
        auth.observe(Event::Button(SERVICE_BUTTON, ButtonEvent::Down), 100);
        assert!(auth.is_authorized(100 + AUTH_WINDOW_TICKS - 1));
        assert!(!auth.is_authorized(100 + AUTH_WINDOW_TICKS));
        assert!(!auth.held());
    }

    #[test]
    fn unrelated_button_does_not_unlock() {
        let mut auth = ShellAuthorization::new();
        auth.observe(Event::Button(Button::Mode, ButtonEvent::Down), 1);
        assert!(!auth.is_authorized(1));
    }
}
