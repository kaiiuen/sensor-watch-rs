//! Minute repeater (decimal) watch face.
//!
//! Port of the C `minute_repeater_decimal_face.c`. A clock that can chime the
//! time in decimal minutes (hours, tens, minutes). It is a pure state machine:
//! it reacts to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

/// The minute repeater decimal face state.
pub struct MinuteRepeaterDecimalFace {
    signal_enabled: bool,
    previous_date_time: u32,
    last_battery_check: u8,
    battery_low: bool,
    alarm_enabled: bool,
}

impl MinuteRepeaterDecimalFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        MinuteRepeaterDecimalFace {
            signal_enabled: false,
            previous_date_time: 0xFFFF_FFFF,
            last_battery_check: 0xFF,
            battery_low: false,
            alarm_enabled: false,
        }
    }

    pub fn new() -> Self {
        MinuteRepeaterDecimalFace::new_static()
    }

    fn update_alarm_indicator(&mut self, settings_alarm_enabled: bool) {
        self.alarm_enabled = settings_alarm_enabled;
        if self.alarm_enabled {
            watch::slcd::set_indicator(Indicator::Signal);
        } else {
            watch::slcd::clear_indicator(Indicator::Signal);
        }
    }

    fn play_hour_chime(&self) {
        crate::movement::play_alarm_beeps(1, Note::C6);
    }

    fn play_tens_chime(&self) {
        crate::movement::play_alarm_beeps(1, Note::E6);
        crate::movement::play_alarm_beeps(1, Note::C6);
    }

    fn play_minute_chime(&self) {
        crate::movement::play_alarm_beeps(1, Note::E6);
    }
}

impl WatchFace for MinuteRepeaterDecimalFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, settings: &Settings) {
        if watch::slcd::tick_animation_is_running() {
            watch::slcd::stop_tick_animation();
        }
        if settings.clock_mode_24h() {
            watch::slcd::set_indicator(Indicator::H24);
        }
        if self.signal_enabled {
            watch::slcd::set_indicator(Indicator::Bell);
        } else {
            watch::slcd::clear_indicator(Indicator::Bell);
        }
        self.update_alarm_indicator(settings.alarm_enabled());
        watch::slcd::set_colon();
        self.previous_date_time = 0xFFFF_FFFF;
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => {
                let mut buf = [0u8; 11];
                let date_time = rtc::get_date_time();
                let previous = self.previous_date_time;
                self.previous_date_time = date_time.to_reg();
                if date_time.day != self.last_battery_check {
                    self.last_battery_check = date_time.day;
                    watch::adc::enable_adc();
                    let voltage = watch::adc::get_vcc_voltage();
                    watch::adc::disable_adc();
                    self.battery_low = voltage < 2200;
                }
                if self.battery_low {
                    watch::slcd::set_indicator(Indicator::Lap);
                }
                if (date_time.to_reg() >> 6) == (previous >> 6) {
                    buf[0] = b'0' + date_time.second / 10;
                    buf[1] = b'0' + date_time.second % 10;
                    watch::slcd::display_string(core::str::from_utf8(&buf[..2]).unwrap_or("  "), 8);
                } else if (date_time.to_reg() >> 12) == (previous >> 12) {
                    buf[0] = b'0' + date_time.minute / 10;
                    buf[1] = b'0' + date_time.minute % 10;
                    buf[2] = b'0' + date_time.second / 10;
                    buf[3] = b'0' + date_time.second % 10;
                    watch::slcd::display_string(core::str::from_utf8(&buf[..4]).unwrap_or(""), 6);
                } else {
                    let mut hour = date_time.hour;
                    if !settings.clock_mode_24h() {
                        if hour < 12 {
                            watch::slcd::clear_indicator(Indicator::Pm);
                        } else {
                            watch::slcd::set_indicator(Indicator::Pm);
                        }
                        hour %= 12;
                        if hour == 0 {
                            hour = 12;
                        }
                    }
                    let weekday = utility::get_weekday(date_time);
                    let wb = weekday.as_bytes();
                    buf[0] = wb[0];
                    buf[1] = wb[1];
                    buf[2] = b'0' + date_time.day / 10;
                    buf[3] = b'0' + date_time.day % 10;
                    buf[4] = b'0' + hour / 10;
                    buf[5] = b'0' + hour % 10;
                    buf[6] = b'0' + date_time.minute / 10;
                    buf[7] = b'0' + date_time.minute % 10;
                    buf[8] = b'0' + date_time.second / 10;
                    buf[9] = b'0' + date_time.second % 10;
                    watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
                }
                if self.alarm_enabled != settings.alarm_enabled() {
                    self.update_alarm_indicator(settings.alarm_enabled());
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.signal_enabled = !self.signal_enabled;
                if self.signal_enabled {
                    watch::slcd::set_indicator(Indicator::Bell);
                } else {
                    watch::slcd::clear_indicator(Indicator::Bell);
                }
            }
            Event::BackgroundTask => {
                movement::play_signal();
            }
            Event::Button(Button::Light, ButtonEvent::LongUp) => {
                let date_time = rtc::get_date_time();
                let mut hours = date_time.hour;
                let tens = date_time.minute / 10;
                let minutes = date_time.minute % 10;
                if !settings.clock_mode_24h() {
                    hours %= 12;
                    if hours == 0 {
                        hours = 12;
                    }
                }
                for _ in 0..hours {
                    self.play_hour_chime();
                }
                for _ in 0..tens {
                    self.play_tens_chime();
                }
                for _ in 0..minutes {
                    self.play_minute_chime();
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        if !self.signal_enabled {
            return false;
        }
        let date_time = rtc::get_date_time();
        date_time.minute == 0
    }
}
