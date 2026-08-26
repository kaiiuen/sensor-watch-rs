//! Host LED shim: no-op LED control, plus color setters routed to the `Hw` seam.
//!
//! The real `src/watch/led.rs` drives the TCC0 PWM outputs for the bi-color LED.
//! On host, `set_led_off` forwards to the `Hw::set_led_off` hook (a no-op by
//! default) so faces like `alarm` that turn the LED off in `resign` compile and
//! run unchanged. Brighter/color writes funnel through `Hw::set_led_color`, so a
//! mock can record which color (red/green/yellow) a face requested.

use super::seam;

/// Host model of the board LED polarity setting.
pub fn set_invert_polarity(_invert: bool) {}

/// Host model: enabling LEDs has no physical effect.
pub fn enable_leds() {}

/// Turns the LED off. Host: forwards to the `Hw` seam (no-op by default).
pub fn set_led_off() {
    seam::with_current_hw(|hw| hw.set_led_off());
}

/// Compatibility alias for the movement LED lifecycle API.
pub fn disable_leds() {
    set_led_off();
}

/// Sets the LED to a custom color by modulating each output's duty cycle.
/// Host: records via `Hw::set_led_color`.
pub fn set_led_color(red: u8, green: u8) {
    seam::with_current_hw(|hw| hw.set_led_color(red, green));
}

/// Sets the red LED to full brightness, green off.
pub fn set_led_red() {
    set_led_color(255, 0);
}

/// Sets the green LED to full brightness, red off.
pub fn set_led_green() {
    set_led_color(0, 255);
}

/// Sets both red and green LEDs to full brightness.
pub fn set_led_yellow() {
    set_led_color(255, 255);
}
