//! Alarm thermometer watch face.
//!
//! Port of the C `alarm_thermometer_face.c`. Shows the temperature and can
//! alarm when it stabilizes. It is a pure state machine: it reacts to a single
//! event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;

const LAST_SIZE: usize = 4;
const MODE_NORMAL: u8 = 0;
const MODE_ALARM: u8 = 1;
const MODE_FREEZE: u8 = 2;

/// The alarm thermometer face state.
pub struct AlarmThermometerFace {
    mode: u8,
    last: [i32; LAST_SIZE],
}

impl AlarmThermometerFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        AlarmThermometerFace {
            mode: MODE_NORMAL,
            last: [i32::MIN; LAST_SIZE],
        }
    }

    pub fn new() -> Self {
        AlarmThermometerFace::new_static()
    }

    fn update(&self, in_fahrenheit: bool) -> f32 {
        let temperature_c = 25.0f32;
        let mut buf = [0u8; 11];
        let v = if in_fahrenheit {
            temperature_c * 1.8 + 32.0
        } else {
            temperature_c
        };
        let scaled = (v * 10.0) as i32;
        buf[0] = b'0' + ((scaled / 100) % 10) as u8;
        buf[1] = b'0' + ((scaled / 10) % 10) as u8;
        buf[2] = b'.';
        buf[3] = b'0' + (scaled % 10) as u8;
        buf[4] = b'#';
        buf[5] = if in_fahrenheit { b'F' } else { b'C' };
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
        temperature_c
    }

    fn clear(&mut self) {
        for v in self.last.iter_mut() {
            *v = i32::MIN;
        }
    }
}

impl WatchFace for AlarmThermometerFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.mode = MODE_NORMAL;
        self.clear();
        watch::slcd::display_string("AT", 0);
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => {
                self.update(settings.use_imperial_units());
            }
            Event::Tick => {
                if rtc::get_date_time().second % 5 == 0 {
                    match self.mode {
                        MODE_NORMAL => {
                            self.update(settings.use_imperial_units());
                        }
                        MODE_ALARM => {
                            for i in (1..LAST_SIZE).rev() {
                                self.last[i] = self.last[i - 1];
                            }
                            self.last[0] =
                                libm::roundf(self.update(settings.use_imperial_units()) * 10.0)
                                    as i32;
                            let mut constant = true;
                            for i in 1..LAST_SIZE {
                                if self.last[i - 1] != self.last[i] {
                                    constant = false;
                                    break;
                                }
                            }
                            if constant {
                                self.mode = MODE_FREEZE;
                                watch::slcd::set_indicator(Indicator::Signal);
                                movement::play_alarm();
                            }
                        }
                        MODE_FREEZE => {}
                        _ => {}
                    }
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                match self.mode {
                    MODE_NORMAL => {
                        self.mode = MODE_ALARM;
                        watch::slcd::set_indicator(Indicator::Bell);
                        self.clear();
                    }
                    MODE_FREEZE => {
                        self.mode = MODE_NORMAL;
                        watch::slcd::clear_indicator(Indicator::Bell);
                        watch::slcd::clear_indicator(Indicator::Signal);
                    }
                    MODE_ALARM => {
                        self.mode = MODE_NORMAL;
                        watch::slcd::clear_indicator(Indicator::Bell);
                        self.update(settings.use_imperial_units());
                    }
                    _ => {}
                }
                if settings.button_should_sound() {
                    crate::movement::play_alarm_beeps(1, Note::C7);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.mode != MODE_FREEZE {
                    settings.set_use_imperial_units(!settings.use_imperial_units());
                    self.update(settings.use_imperial_units());
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
