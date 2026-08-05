//! Settings watch face.
//!
//! Port of the C `settings_face.c`. Lets the user configure various options on
//! the watch: clock mode, button beep, signal beep, alarm beep, timeout, low
//! energy mode, LED duration and LED colors. It is a pure state machine: it
//! reacts to a single event and returns; it never keeps the CPU awake.
//!
//! Note: the C code blinks the setting value (showing it on alternating
//! subseconds). The Rust `Event::subsecond()` always returns 0 in this
//! framework, so the values are shown continuously instead.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, ClockMode, Event, Settings, WatchFace};
use crate::watch::buzzer::Note;
use crate::watch::slcd;

/// The settings face state. The C uses a dynamically-built table of function
/// pointers; here we use a `match` on the current page index instead.
pub struct SettingsFace {
    current_page: u8,
    /// The total number of settings screens.
    num_settings: u8,
    /// The first page index that is an LED color page.
    led_color_start: u8,
    /// One past the last LED color page index.
    led_color_end: u8,
}

impl SettingsFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        // Baseline of 6 settings (clock, beep, signal, alarm, timeout, LED
        // duration), plus the low-energy setting and the red/green LED color
        // settings (both channels present on this board).
        SettingsFace {
            current_page: 0,
            num_settings: 6 + 1 + 2,
            led_color_start: 7,
            led_color_end: 9,
        }
    }

    pub fn new() -> Self {
        SettingsFace::new_static()
    }

    fn clock_setting_display(&self) {
        slcd::display_string("CL", 0);
        if movement::clock_mode_24h() != ClockMode::H12 {
            slcd::display_string("24h", 4);
        } else {
            slcd::display_string("12h", 4);
        }
    }

    fn clock_setting_advance() {
        let next = match movement::clock_mode_24h() {
            ClockMode::H12 => ClockMode::H24,
            ClockMode::H24 => ClockMode::H024,
            ClockMode::H024 => ClockMode::H12,
        };
        movement::set_clock_mode_24h(next);
    }

    fn beep_setting_display(&self) {
        slcd::display_string("BT", 0);
        slcd::display_string(" beep ", 4);
        if movement::button_should_sound() {
            if movement::button_volume() {
                // H for HIGH
                slcd::display_string(" H", 2);
            } else {
                // L for LOW
                slcd::display_string(" L", 2);
            }
        } else {
            // N for NONE
            slcd::display_string(" N", 2);
        }
    }

    fn beep_setting_advance() {
        if !movement::button_should_sound() {
            // was muted. make it soft.
            movement::set_button_should_sound(true);
            movement::set_button_volume(false);
            movement::play_note(Note::C7, 0);
        } else if !movement::button_volume() {
            // was soft. make it loud.
            movement::set_button_volume(true);
            movement::play_note(Note::C7, 0);
        } else {
            // was loud. make it silent.
            movement::set_button_should_sound(false);
        }
    }

    fn signal_setting_display(&self) {
        slcd::display_string("SI", 0);
        slcd::display_string("SIGNAL", 4);
        if movement::signal_volume() {
            // H for HIGH
            slcd::display_string(" H", 2);
        } else {
            // L for LOW
            slcd::display_string(" L", 2);
        }
    }

    fn signal_setting_advance() {
        if !movement::signal_volume() {
            // was soft. make it loud.
            movement::set_signal_volume(true);
        } else {
            // was loud. make it soft.
            movement::set_signal_volume(false);
        }
        movement::play_signal();
    }

    fn alarm_setting_display(&self) {
        slcd::display_string("AL", 0);
        slcd::display_string("ALARM ", 4);
        if movement::alarm_volume() {
            // H for HIGH
            slcd::display_string(" H", 2);
        } else {
            // L for LOW
            slcd::display_string(" L", 2);
        }
    }

    fn alarm_setting_advance() {
        if !movement::alarm_volume() {
            // was soft. make it loud.
            movement::set_alarm_volume(true);
        } else {
            // was loud. make it soft.
            movement::set_alarm_volume(false);
        }
        movement::play_alarm();
    }

    fn timeout_setting_display(&self) {
        slcd::display_string("TO", 0);
        let s = match movement::get_fast_tick_timeout() {
            0 => "60 SeC",
            1 => "2 n&in",
            2 => "5 n&in",
            _ => "30n&in",
        };
        slcd::display_string(s, 4);
    }

    fn timeout_setting_advance() {
        movement::set_fast_tick_timeout(movement::get_fast_tick_timeout() + 1);
    }

    fn low_energy_setting_display(&self) {
        slcd::display_string("LE", 0);
        let s = match movement::get_low_energy_timeout() {
            0 => " Never",
            1 => "10n&in",
            2 => "1 hour",
            3 => "2 hour",
            4 => "6 hour",
            5 => "12 hr",
            6 => " 1 day",
            _ => " 7 day",
        };
        slcd::display_string(s, 4);
    }

    fn low_energy_setting_advance() {
        movement::set_low_energy_timeout(movement::get_low_energy_timeout() + 1);
    }

    fn led_duration_setting_display(&self) {
        slcd::display_string("LT", 0);
        let dwell = movement::get_backlight_dwell();
        if dwell == 0 {
            slcd::display_string("instnt", 4);
        } else if dwell == 0b111 {
            slcd::display_string("no LEd", 4);
        } else {
            let mut buf = [0u8; 6];
            buf[0] = b' ';
            buf[1] = b'0' + ((dwell * 2 - 1) % 10);
            buf[2] = b' ';
            buf[3] = b'S';
            buf[4] = b'e';
            buf[5] = b'C';
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("      "), 4);
        }
    }

    fn led_duration_setting_advance() {
        movement::set_backlight_dwell(movement::get_backlight_dwell() + 1);
        if movement::get_backlight_dwell() > 3 {
            // set all bits to disable the LED
            movement::set_backlight_dwell(0b111);
        }
    }

    fn red_led_setting_display(&self) {
        slcd::display_string("LT", 0);
        slcd::display_string(" red  ", 4);
        let (red, _, _) = movement::backlight_color();
        let mut buf = [0u8; 2];
        buf[0] = b'0' + red / 10;
        buf[1] = b'0' + red % 10;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("  "), 2);
    }

    fn red_led_setting_advance() {
        let (red, green, blue) = movement::backlight_color();
        movement::set_backlight_color(red.wrapping_add(1), green, blue);
    }

    fn green_led_setting_display(&self) {
        slcd::display_string("LT", 0);
        slcd::display_string(" green", 4);
        let (_, green, _) = movement::backlight_color();
        let mut buf = [0u8; 2];
        buf[0] = b'0' + green / 10;
        buf[1] = b'0' + green % 10;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("  "), 2);
    }

    fn green_led_setting_advance() {
        let (red, green, blue) = movement::backlight_color();
        movement::set_backlight_color(red, green.wrapping_add(1), blue);
    }

    /// Renders the current settings page.
    fn display_page(&self) {
        match self.current_page {
            0 => self.clock_setting_display(),
            1 => self.beep_setting_display(),
            2 => self.signal_setting_display(),
            3 => self.alarm_setting_display(),
            4 => self.timeout_setting_display(),
            5 => self.low_energy_setting_display(),
            6 => self.led_duration_setting_display(),
            7 => self.red_led_setting_display(),
            8 => self.green_led_setting_display(),
            _ => {}
        }
    }

    /// Advances the value of the current settings page.
    fn advance_page(&self) {
        match self.current_page {
            0 => Self::clock_setting_advance(),
            1 => Self::beep_setting_advance(),
            2 => Self::signal_setting_advance(),
            3 => Self::alarm_setting_advance(),
            4 => Self::timeout_setting_advance(),
            5 => Self::low_energy_setting_advance(),
            6 => Self::led_duration_setting_advance(),
            7 => Self::red_led_setting_advance(),
            8 => Self::green_led_setting_advance(),
            _ => {}
        }
    }
}

impl WatchFace for SettingsFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.current_page = 0;
        movement::request_tick_frequency(4); // we need to manually blink some pixels
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Button(Button::Light, ButtonEvent::Down) => {
                self.current_page = (self.current_page + 1) % self.num_settings;
                slcd::clear_display();
                self.display_page();
            }
            Event::Tick | Event::Activate => {
                self.display_page();
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                movement::force_led_off();
                movement::move_to_next_face();
                return;
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.advance_page();
            }
            Event::BackgroundTask => {
                movement::move_to_face(0);
            }
            _ => {
                movement::default_loop_handler(event, _settings);
                return;
            }
        }

        // Keep the LED lit (showing the color blend) while on an LED color page.
        if self.current_page >= self.led_color_start && self.current_page < self.led_color_end {
            let (red, green, _) = movement::backlight_color();
            // this bitwise math turns #000 into #000000, #111 into #111111, etc.
            movement::force_led_on(red | (red << 4), green | (green << 4), 0);
        } else {
            movement::force_led_off();
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        movement::force_led_off();
        movement::store_settings();
    }
}
