//! Preferences watch face.
//!
//! Port of the C `preferences_face.c`. Lets the user adjust watch preferences
//! (clock mode, button sound, timeout, low energy, LED). It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;

const PREFERENCES_FACE_NUM_PREFERENCES: u8 = 7;
const TITLES: [&str; 7] = [
    "CL        ",
    "BT  Beep  ",
    "TO        ",
    "LE        ",
    "LT        ",
    "LT   grn  ",
    "LT   red  ",
];

/// The preferences face state.
pub struct PreferencesFace {
    current_page: u8,
}

impl PreferencesFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        PreferencesFace { current_page: 0 }
    }

    pub fn new() -> Self {
        PreferencesFace::new_static()
    }
}

impl WatchFace for PreferencesFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.current_page = 0;
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Tick | Event::Activate => {}
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                watch::led::set_led_off();
                movement::move_to_next_face();
                return;
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                self.current_page = (self.current_page + 1) % PREFERENCES_FACE_NUM_PREFERENCES;
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => match self.current_page {
                0 => settings.set_clock_mode_24h(!settings.clock_mode_24h()),
                1 => settings.set_button_should_sound(!settings.button_should_sound()),
                2 => settings.set_to_interval(settings.to_interval() + 1),
                3 => settings.set_le_interval(settings.le_interval() + 1),
                4 => {
                    let mut d = settings.led_duration() + 1;
                    if d > 3 {
                        d = 0b111;
                    }
                    settings.set_led_duration(d);
                }
                5 => settings.set_led_green_color(settings.led_green_color() + 1),
                6 => settings.set_led_red_color(settings.led_red_color() + 1),
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.current_page == 0 && settings.clock_mode_24h() {
                    settings.set_clock_24h_leading_zero(!settings.clock_24h_leading_zero());
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }

        watch::slcd::display_string(TITLES[self.current_page as usize], 0);

        if self.current_page >= 5 {
            let red = if settings.led_red_color() != 0 {
                0xF | (settings.led_red_color() << 4)
            } else {
                0
            };
            let green = if settings.led_green_color() != 0 {
                0xF | (settings.led_green_color() << 4)
            } else {
                0
            };
            watch::led::set_led_color(red, green);
        } else {
            watch::led::set_led_off();
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        watch::led::set_led_off();
        crate::movement::save_settings();
    }
}
