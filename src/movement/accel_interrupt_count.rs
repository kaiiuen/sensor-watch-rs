//! Accelerometer interrupt count watch face.
//!
//! Port of the C `accel_interrupt_count_face.c`. Counts accelerometer
//! interrupts (requires the optional accelerometer). It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::slcd::Indicator;

/// The accel interrupt count face state.
pub struct AccelInterruptCountFace {
    count: u32,
    running: bool,
    is_setting: bool,
    threshold: u8,
    new_threshold: u8,
}

impl AccelInterruptCountFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        AccelInterruptCountFace {
            count: 0,
            running: false,
            is_setting: false,
            threshold: 10,
            new_threshold: 10,
        }
    }

    pub fn new() -> Self {
        AccelInterruptCountFace::new_static()
    }

    fn update_display(&self) {
        let mut buf = [0u8; 11];
        if self.running {
            watch::slcd::set_indicator(Indicator::Signal);
        } else {
            watch::slcd::clear_indicator(Indicator::Signal);
        }
        buf[0] = b'A';
        buf[1] = b'C';
        buf[2] = b'1';
        buf[3] = b'N';
        write_num(&mut buf, self.count, 4, 6);
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

/// Writes a number right-aligned into the buffer at the given offset.
fn write_num(buf: &mut [u8; 11], value: u32, offset: usize, width: usize) {
    if width == 0 {
        return;
    }

    let Some(end) = offset.checked_add(width).and_then(|end| end.checked_sub(1)) else {
        return;
    };
    if end >= buf.len() {
        return;
    }

    let mut v = value;
    let mut i = end;
    loop {
        let Some(slot) = buf.get_mut(i) else {
            return;
        };
        *slot = b'0' + (v % 10) as u8;
        v /= 10;
        if i == offset {
            break;
        }
        i -= 1;
    }
}

impl WatchFace for AccelInterruptCountFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.is_setting = false;
        // Enable tap detection so taps are counted.
        movement::enable_tap_detection_if_available();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        if self.is_setting {
            match event {
                Event::Button(Button::Light, ButtonEvent::Down) => {
                    self.new_threshold = (self.new_threshold + 1) % 64;
                    let mut buf = [0u8; 11];
                    buf[0] = b'T';
                    buf[1] = b'H';
                    buf[2] = b' ';
                    buf[3] = b' ';
                    write_num(&mut buf, self.new_threshold as u32, 4, 4);
                    watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
                }
                Event::Tick => {
                    let mut buf = [0u8; 11];
                    buf[0] = b'T';
                    buf[1] = b'H';
                    buf[2] = b' ';
                    buf[3] = b' ';
                    write_num(&mut buf, self.new_threshold as u32, 4, 4);
                    watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
                }
                Event::Button(Button::Alarm, ButtonEvent::Up) => {
                    self.threshold = self.new_threshold;
                    self.is_setting = false;
                }
                _ => movement::default_loop_handler(event, settings),
            }
        } else {
            match event {
                Event::Button(Button::Light, ButtonEvent::Down) => {
                    movement::illuminate_led();
                    if !self.running {
                        self.count = 0;
                    }
                    self.update_display();
                }
                Event::Button(Button::Alarm, ButtonEvent::Up) => {
                    self.running = !self.running;
                    self.update_display();
                }
                Event::Activate | Event::Tick => self.update_display(),
                // Count accelerometer taps while running.
                Event::SingleTap | Event::DoubleTap => {
                    if self.running {
                        self.count = self.count.wrapping_add(1);
                    }
                    self.update_display();
                }
                Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                    if !self.running {
                        self.new_threshold = self.threshold;
                        self.is_setting = true;
                    }
                    return;
                }
                _ => movement::default_loop_handler(event, settings),
            }
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        movement::disable_tap_detection_if_available();
    }

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::write_num;

    #[test]
    fn writes_number_within_buffer() {
        let mut buf = [b' '; 11];
        write_num(&mut buf, 42, 4, 4);
        assert_eq!(&buf[4..8], b"0042");
    }

    #[test]
    fn ignores_empty_or_out_of_bounds_field() {
        let mut buf = [b' '; 11];
        write_num(&mut buf, 42, 4, 0);
        write_num(&mut buf, 42, 9, 4);
        assert_eq!(buf, [b' '; 11]);
    }

    #[test]
    fn zero_pads_zero_to_the_left_boundary() {
        let mut buf = [b' '; 11];
        write_num(&mut buf, 0, 4, 4);
        assert_eq!(&buf[4..8], b"0000");
    }

    #[test]
    fn width_zero_never_writes() {
        let mut buf = [b' '; 11];
        write_num(&mut buf, 42, 4, 0);
        assert_eq!(buf, [b' '; 11]);
    }
}
