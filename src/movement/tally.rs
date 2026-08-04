//! Tally counter watch face.
//!
//! Port of the C `tally_face.c`. A tally counter (range -99 to 9999) with
//! optional beeps and a quick-repeat mode. It is a pure state machine: it
//! reacts to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd::Indicator;

const TALLY_FACE_MAX: i32 = 9999;
const TALLY_FACE_MIN: i32 = -99;

/// The tally face state.
pub struct TallyFace {
    tally_idx: i32,
    quick_ticks_running: bool,
    using_led: bool,
}

impl TallyFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        TallyFace {
            tally_idx: 0,
            quick_ticks_running: false,
            using_led: false,
        }
    }

    pub fn new() -> Self {
        TallyFace::new_static()
    }

    fn print_tally(&self, sound_on: bool) {
        if sound_on {
            watch::slcd::set_indicator(Indicator::Bell);
        } else {
            watch::slcd::clear_indicator(Indicator::Bell);
        }
        let mut buf = [0u8; 11];
        if self.tally_idx >= 0 {
            buf[0] = b'T';
            buf[1] = b'A';
            buf[2] = b' ';
            buf[3] = b' ';
            write_num(&mut buf, self.tally_idx, 4, 4);
        } else {
            buf[0] = b'T';
            buf[1] = b'A';
            buf[2] = b' ';
            buf[3] = b' ';
            buf[4] = b' ';
            write_num(&mut buf, self.tally_idx, 5, 3);
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn increment(&mut self, sound_on: bool) {
        let sound = !self.quick_ticks_running && sound_on;
        if self.tally_idx >= TALLY_FACE_MAX {
            if sound {
                crate::movement::play_alarm_beeps(1, Note::E7);
            }
        } else {
            self.tally_idx += 1;
            self.print_tally(sound_on);
            if sound {
                crate::movement::play_alarm_beeps(1, Note::E6);
            }
        }
    }

    fn decrement(&mut self, sound_on: bool) {
        let sound = !self.quick_ticks_running && sound_on;
        if self.tally_idx <= TALLY_FACE_MIN {
            if sound {
                crate::movement::play_alarm_beeps(1, Note::C5SharpD5Flat);
            }
        } else {
            self.tally_idx -= 1;
            self.print_tally(sound_on);
            if sound {
                crate::movement::play_alarm_beeps(1, Note::C6SharpD6Flat);
            }
        }
    }
}

/// Writes a signed number into the buffer at the given offset, right-aligned.
fn write_num(buf: &mut [u8; 11], value: i32, offset: usize, width: usize) {
    let mut v = value;
    if v < 0 {
        buf[offset] = b'-';
        v = -v;
    }
    let mut i = offset + width - 1;
    loop {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if i == offset || v == 0 {
            break;
        }
        i -= 1;
    }
}

impl WatchFace for TallyFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.quick_ticks_running = false;
        self.using_led = false;
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        if self.using_led {
            let mode = watch::gpio::get_pin_level(watch::extint::BTN_MODE);
            let light = watch::gpio::get_pin_level(watch::extint::BTN_LIGHT);
            let alarm = watch::gpio::get_pin_level(watch::extint::BTN_ALARM);
            if !mode && !light && !alarm {
                self.using_led = false;
            } else {
                if let Event::Button(Button::Light, ButtonEvent::Down)
                | Event::Button(Button::Alarm, ButtonEvent::Down) = event
                {
                    movement::illuminate_led();
                }
                return;
            }
        }

        match event {
            Event::Tick => {
                if self.quick_ticks_running {
                    let light = watch::gpio::get_pin_level(watch::extint::BTN_LIGHT);
                    let alarm = watch::gpio::get_pin_level(watch::extint::BTN_ALARM);
                    if light && alarm {
                        self.quick_ticks_running = false;
                    } else if light {
                        self.increment(settings.button_should_sound());
                    } else if alarm {
                        self.decrement(settings.button_should_sound());
                    } else {
                        self.quick_ticks_running = false;
                    }
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.decrement(settings.button_should_sound());
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.decrement(settings.button_should_sound());
                self.quick_ticks_running = true;
            }
            Event::Button(Button::Mode, ButtonEvent::LongPress) => {
                self.tally_idx = 0;
                self.print_tally(settings.button_should_sound());
                if settings.button_should_sound() {
                    crate::movement::play_alarm_beeps(1, Note::G6);
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.increment(settings.button_should_sound());
            }
            Event::Button(Button::Light, ButtonEvent::Down)
            | Event::Button(Button::Alarm, ButtonEvent::Down) => {
                if watch::gpio::get_pin_level(watch::extint::BTN_MODE) {
                    movement::illuminate_led();
                    self.using_led = true;
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.increment(settings.button_should_sound());
                self.quick_ticks_running = true;
            }
            Event::Activate => self.print_tally(settings.button_should_sound()),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
