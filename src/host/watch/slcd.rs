//! Host SLCD shim: routes LCD writes through the `Hw` seam.
//!
//! Mirrors the subset of `src/watch/slcd.rs` that faces call, forwarding to the
//! installed [`MockHw`]. Indicator segments are the shared
//! `sensor_watch_core::mock_hw::Indicator` type.

use super::seam;
// Re-exported so real faces (`use crate::watch::slcd::Indicator;`) resolve on host.
pub use sensor_watch_core::mock_hw::Indicator;

/// Displays a string at digit position 0-9. A space clears that digit.
pub fn display_string(string: &str, position: u8) {
    if !sensor_watch_core::safety::valid_display_position(position) {
        return;
    }
    seam::with_current_hw(|hw| hw.display_string(string, position));
}

/// Displays a single character at `position`, applying the same segment-map
/// character substitutions as the real `src/watch/slcd.rs` so host `text()`
/// snapshots match what the firmware actually draws.
pub fn display_character(character: u8, position: u8) {
    if !sensor_watch_core::safety::valid_display_position(position) {
        return;
    }
    let mut character = if sensor_watch_core::safety::valid_display_character(character) {
        character
    } else {
        b' '
    };

    if position == 4 || position == 6 {
        if character == b'7' {
            character = b'&';
        } else if character == b'A' {
            character = b'a';
        } else if character == b'o' {
            character = b'O';
        } else if character == b'L' {
            character = b'!';
        } else if character == b'M' || character == b'm' || character == b'N' {
            character = b'n';
        } else if character == b'c' {
            character = b'C';
        } else if character == b'J' {
            character = b'j';
        } else if character == b't' || character == b'T' {
            character = b'+';
        } else if character == b'y' || character == b'Y' {
            character = b'4';
        } else if character == b'v'
            || character == b'V'
            || character == b'U'
            || character == b'W'
            || character == b'w'
        {
            character = b'u';
        }
    } else {
        if character == b'u' {
            character = b'v';
        } else if character == b'j' {
            character = b'J';
        }
    }
    if position > 1 && character == b'T' {
        character = b't';
    }
    if position == 1 {
        if character == b'a' {
            character = b'A';
        } else if character == b'o' {
            character = b'O';
        } else if character == b'i' {
            character = b'l';
        } else if character == b'n' {
            character = b'N';
        } else if character == b'r' {
            character = b'R';
        } else if character == b'd' {
            character = b'D';
        } else if character == b'v' || character == b'V' || character == b'u' {
            character = b'U';
        } else if character == b'b' {
            character = b'B';
        } else if character == b'c' {
            character = b'C';
        }
    } else if character == b'R' {
        character = b'r';
    }
    if position != 0 && character == b'I' {
        character = b'l';
    }

    let buf = [character];
    let s = core::str::from_utf8(&buf).unwrap_or(" ");
    seam::with_current_hw(|hw| hw.display_string(s, position));
}

/// Turns the colon on.
pub fn set_colon() {
    seam::with_current_hw(|hw| hw.set_colon());
}

/// Turns the colon off.
pub fn clear_colon() {
    seam::with_current_hw(|hw| hw.clear_colon());
}

/// Sets an indicator segment.
pub fn set_indicator(indicator: Indicator) {
    seam::with_current_hw(|hw| hw.set_indicator(indicator));
}

/// Clears an indicator segment.
pub fn clear_indicator(indicator: Indicator) {
    seam::with_current_hw(|hw| hw.clear_indicator(indicator));
}

/// Sets a raw (com, seg) pixel.
pub fn set_pixel(com: u8, seg: u8) {
    if com <= 2 && seg < 32 {
        seam::with_current_hw(|hw| hw.set_pixel(com, seg));
    }
}

/// Clears a raw (com, seg) pixel.
pub fn clear_pixel(com: u8, seg: u8) {
    if com <= 2 && seg < 32 {
        seam::with_current_hw(|hw| hw.clear_pixel(com, seg));
    }
}

/// Clears the entire display.
pub fn clear_display() {
    seam::with_current_hw(|hw| hw.clear_display());
}

/// True while the tick animation is running.
pub fn tick_animation_is_running() -> bool {
    seam::with_current_hw(|hw| hw.tick_animation_is_running())
}

/// Stops the tick animation.
pub fn stop_tick_animation() {
    seam::with_current_hw(|hw| hw.stop_tick_animation());
}

/// Clears all indicator segments at once.
pub fn clear_all_indicators() {
    seam::with_current_hw(|hw| hw.clear_all_indicators());
}

/// Starts the tick (colon) animation for `duration` ms.
pub fn start_tick_animation(duration: u32) {
    seam::with_current_hw(|hw| hw.start_tick_animation(duration));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_display_inputs_fail_closed_without_a_host_backend() {
        display_character(b'A', 10);
        display_string("x", 10);
        set_pixel(3, 0);
        clear_pixel(0, 32);
    }
}
