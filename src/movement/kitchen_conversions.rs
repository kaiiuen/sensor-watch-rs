//! Kitchen conversions watch face.
//!
//! Port of the C `kitchen_conversions_face.c`. Converts between cooking units
//! (weight, temperature, volume) with US/UK locale support. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd::Indicator;

const DISPLAY_DIGITS: u8 = 6;
const SCREEN_NUM: u8 = 5;
const MEASURES_COUNT: u8 = 3;
const WEIGHT: u8 = 0;
const TEMP: u8 = 1;
const VOL: u8 = 2;

const MEASURES: [&str; 3] = ["WeIght", " Temp", " VOL"];

const WEIGHT_COUNT: u8 = 4;
const TEMP_COUNT: u8 = 3;
const VOL_COUNT: u8 = 9;
const UNITS_COUNT: [u8; 3] = [WEIGHT_COUNT, TEMP_COUNT, VOL_COUNT];

/// A unit: name, UK factor, US factor, linear factor.
struct Unit {
    name: &'static str,
    conv_factor_uk: f64,
    conv_factor_us: f64,
    linear_factor: i16,
}

const WEIGHTS: [Unit; 4] = [
    Unit {
        name: " g",
        conv_factor_uk: 1.0,
        conv_factor_us: 1.0,
        linear_factor: 0,
    },
    Unit {
        name: " kg",
        conv_factor_uk: 1000.0,
        conv_factor_us: 1000.0,
        linear_factor: 0,
    },
    Unit {
        name: "Ounce",
        conv_factor_uk: 28.34952,
        conv_factor_us: 28.34952,
        linear_factor: 0,
    },
    Unit {
        name: " Pound",
        conv_factor_uk: 453.5924,
        conv_factor_us: 453.5924,
        linear_factor: 0,
    },
];

const TEMPS: [Unit; 3] = [
    Unit {
        name: " # C",
        conv_factor_uk: 1.8,
        conv_factor_us: 1.8,
        linear_factor: 32,
    },
    Unit {
        name: " # F",
        conv_factor_uk: 1.0,
        conv_factor_us: 1.0,
        linear_factor: 0,
    },
    Unit {
        name: "Gas Mk",
        conv_factor_uk: 25.0,
        conv_factor_us: 25.0,
        linear_factor: 250,
    },
];

const VOLS: [Unit; 9] = [
    Unit {
        name: "  n&L",
        conv_factor_uk: 1.0,
        conv_factor_us: 1.0,
        linear_factor: 0,
    },
    Unit {
        name: "   L",
        conv_factor_uk: 1000.0,
        conv_factor_us: 1000.0,
        linear_factor: 0,
    },
    Unit {
        name: " Fl Oz",
        conv_factor_uk: 28.41306,
        conv_factor_us: 29.57353,
        linear_factor: 0,
    },
    Unit {
        name: " Tbsp",
        conv_factor_uk: 17.75816,
        conv_factor_us: 14.78677,
        linear_factor: 0,
    },
    Unit {
        name: " Tsp",
        conv_factor_uk: 5.919388,
        conv_factor_us: 4.928922,
        linear_factor: 0,
    },
    Unit {
        name: "  Cup",
        conv_factor_uk: 284.1306,
        conv_factor_us: 236.5882,
        linear_factor: 0,
    },
    Unit {
        name: " Pint",
        conv_factor_uk: 568.2612,
        conv_factor_us: 473.1765,
        linear_factor: 0,
    },
    Unit {
        name: " Quart",
        conv_factor_uk: 1136.522,
        conv_factor_us: 946.353,
        linear_factor: 0,
    },
    Unit {
        name: "Gallon",
        conv_factor_uk: 4546.09,
        conv_factor_us: 3785.412,
        linear_factor: 0,
    },
];

/// The page.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Measurement,
    From,
    To,
    Input,
    Result,
}

/// The kitchen conversions face state.
pub struct KitchenConversionsFace {
    pg: Page,
    measurement_i: u8,
    from_i: u8,
    from_is_us: bool,
    to_i: u8,
    to_is_us: bool,
    selection_value: u32,
    selection_index: u8,
    light_held: bool,
}

impl KitchenConversionsFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        KitchenConversionsFace {
            pg: Page::Measurement,
            measurement_i: 0,
            from_i: 0,
            from_is_us: false,
            to_i: 0,
            to_is_us: false,
            selection_value: 0,
            selection_index: 0,
            light_held: false,
        }
    }

    pub fn new() -> Self {
        KitchenConversionsFace::new_static()
    }

    fn reset_state(&mut self, settings: &Settings) {
        self.pg = Page::Measurement;
        self.measurement_i = 0;
        self.from_i = 0;
        self.from_is_us = settings.use_imperial_units();
        self.to_i = 0;
        self.to_is_us = settings.use_imperial_units();
        self.selection_value = 0;
        self.selection_index = 0;
        self.light_held = false;
    }

    fn pow_10(n: u8) -> u32 {
        let mut result = 1u32;
        for _ in 0..n {
            result *= 10;
        }
        result
    }

    fn get_unit_list(&self, measurement_i: u8) -> &'static [Unit] {
        match measurement_i {
            TEMP => &TEMPS,
            VOL => &VOLS,
            _ => &WEIGHTS,
        }
    }

    fn increment_input(&mut self) {
        let digit =
            self.selection_value / Self::pow_10(DISPLAY_DIGITS - 1 - self.selection_index) % 10;
        let place = Self::pow_10(DISPLAY_DIGITS - 1 - self.selection_index);
        if digit != 9 {
            self.selection_value += place;
        } else {
            self.selection_value -= 9 * place;
        }
    }

    fn display_units(&self, measurement_i: u8, list_i: u8) {
        let list = self.get_unit_list(measurement_i);
        watch::slcd::display_string(list[list_i as usize].name, 4);
    }

    fn display(&self, settings: &Settings, subsec: u8) {
        watch::slcd::clear_display();
        match self.pg {
            Page::Measurement => {
                watch::slcd::display_string("Un", 0);
                watch::slcd::display_string(MEASURES[self.measurement_i as usize], 4);
            }
            Page::From => {
                self.display_units(self.measurement_i, self.from_i);
                if self.measurement_i == VOL {
                    watch::slcd::display_string("F", 3);
                    let locale = if self.from_is_us { "A " } else { "GB" };
                    watch::slcd::display_string(locale, 0);
                } else {
                    watch::slcd::display_string("Fr", 0);
                }
            }
            Page::To => {
                self.display_units(self.measurement_i, self.to_i);
                if self.measurement_i == VOL {
                    watch::slcd::display_string("T", 3);
                    let locale = if self.to_is_us { "A " } else { "GB" };
                    watch::slcd::display_string(locale, 0);
                } else {
                    watch::slcd::display_string("To", 0);
                }
            }
            Page::Input => {
                let mut buf = [0u8; 7];
                write_num(&mut buf, self.selection_value);
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("000000"), 4);
                if self.measurement_i == TEMP && self.from_i == 2 {
                    watch::slcd::display_string("  ", 8);
                }
                if subsec % 2 == 1 {
                    watch::slcd::display_string(" ", 4 + self.selection_index);
                }
                watch::slcd::display_string("In", 0);
            }
            Page::Result => {
                let froms = &self.get_unit_list(self.measurement_i)[self.from_i as usize];
                let tos = &self.get_unit_list(self.measurement_i)[self.to_i as usize];
                let f_conv = if self.from_is_us {
                    froms.conv_factor_us
                } else {
                    froms.conv_factor_uk
                };
                let t_conv = if self.to_is_us {
                    tos.conv_factor_us
                } else {
                    tos.conv_factor_uk
                };
                let to_base =
                    (self.selection_value as f64 * f_conv) + 100.0 * froms.linear_factor as f64;
                let conversion = (to_base - 100.0 * tos.linear_factor as f64) / t_conv;

                let lower_bound = if self.measurement_i == TEMP && self.to_i == 2 {
                    100.0
                } else {
                    0.0
                };
                if conversion >= 1000000.0 || conversion < lower_bound {
                    watch::slcd::set_indicator(Indicator::Bell);
                    watch::slcd::display_string("Err", 5);
                    if settings.button_should_sound() {
                        crate::movement::play_alarm_beeps(1, Note::G6);
                        crate::movement::play_alarm_beeps(1, Note::C7);
                    }
                } else {
                    let rounded = (conversion + 0.5) as u32;
                    let mut buf = [0u8; 7];
                    write_num(&mut buf, rounded);
                    watch::slcd::display_string(
                        core::str::from_utf8(&buf[..]).unwrap_or("000000"),
                        4,
                    );
                    if rounded < 10 {
                        watch::slcd::display_string("00", 7);
                    } else if rounded < 100 {
                        watch::slcd::display_string("0", 7);
                    }
                    if settings.button_should_sound() {
                        crate::movement::play_alarm_beeps(1, Note::G6);
                        crate::movement::play_alarm_beeps(1, Note::C7);
                    }
                }
                watch::slcd::display_string("=", 1);
            }
        }
    }
}

/// Writes a number into a 6-digit buffer.
fn write_num(buf: &mut [u8; 7], value: u32) {
    let mut v = value;
    for i in (0..6).rev() {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
}

impl WatchFace for KitchenConversionsFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, settings: &Settings) {
        self.reset_state(settings);
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => self.display(settings, 0),
            Event::Tick => {
                if self.pg == Page::Input {
                    self.display(settings, 0);
                    if self.light_held {
                        self.increment_input();
                    }
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                match self.pg {
                    Page::Measurement => {
                        self.measurement_i = (self.measurement_i + 1) % MEASURES_COUNT;
                    }
                    Page::From => {
                        self.from_i = (self.from_i + 1) % UNITS_COUNT[self.measurement_i as usize];
                    }
                    Page::To => {
                        self.to_i = (self.to_i + 1) % UNITS_COUNT[self.measurement_i as usize];
                    }
                    Page::Input => self.increment_input(),
                    Page::Result => {}
                }
                if self.pg != Page::Result {
                    self.display(settings, 0);
                }
                self.light_held = false;
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.pg == Page::Input {
                    if self.selection_index
                        < (DISPLAY_DIGITS - 1)
                            - 2 * (self.measurement_i == TEMP && self.from_i == 2) as u8
                    {
                        self.selection_index += 1;
                    } else {
                        self.pg = Page::Result;
                        self.display(settings, 0);
                    }
                } else {
                    if self.pg == Page::Result {
                        self.reset_state(settings);
                    } else {
                        self.pg = match self.pg {
                            Page::Measurement => Page::From,
                            Page::From => Page::To,
                            Page::To => Page::Input,
                            _ => Page::Result,
                        };
                    }
                    if settings.button_should_sound() {
                        crate::movement::play_alarm_beeps(1, Note::C7);
                    }
                }
                self.display(settings, 0);
                self.light_held = false;
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.pg != Page::Measurement {
                    match self.pg {
                        Page::Measurement => self.measurement_i = 0,
                        Page::From => {
                            self.from_i = 0;
                            self.from_is_us = settings.use_imperial_units();
                        }
                        Page::To => {
                            self.to_i = 0;
                            self.to_is_us = settings.use_imperial_units();
                        }
                        Page::Input => {
                            self.selection_index = 0;
                            self.selection_value = 0;
                        }
                        Page::Result => self.selection_index = 0,
                    }
                    self.pg = match self.pg {
                        Page::From => Page::Measurement,
                        Page::To => Page::From,
                        Page::Input => Page::To,
                        _ => Page::Input,
                    };
                    self.display(settings, 0);
                    if settings.button_should_sound() {
                        crate::movement::play_alarm_beeps(1, Note::C8);
                    }
                    self.light_held = false;
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.measurement_i == VOL {
                    if self.pg == Page::From {
                        self.from_is_us = !self.from_is_us;
                    } else if self.pg == Page::To {
                        self.to_is_us = !self.to_is_us;
                    }
                    if self.pg == Page::From || self.pg == Page::To {
                        self.display(settings, 0);
                        if settings.button_should_sound() {
                            crate::movement::play_alarm_beeps(1, Note::E7);
                        }
                    }
                }
                if self.pg == Page::Input {
                    self.light_held = true;
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongUp) => {
                self.light_held = false;
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
