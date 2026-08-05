//! Simple clock (binary LED) watch face.
//!
//! Port of the C `simple_clock_bin_led_face.c`. A clock that can flash the
//! time in binary on the LED. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

/// The simple clock bin LED face state.
pub struct SimpleClockBinLedFace {
    signal_enabled: bool,
    previous_date_time: u32,
    last_battery_check: u8,
    battery_low: bool,
    alarm_enabled: bool,
    flashing_state: u8,
    flashing_value: u8,
    ticks: u8,
}

impl SimpleClockBinLedFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        SimpleClockBinLedFace {
            signal_enabled: false,
            previous_date_time: 0xFFFF_FFFF,
            last_battery_check: 0xFF,
            battery_low: false,
            alarm_enabled: false,
            flashing_state: 0,
            flashing_value: 0,
            ticks: 0,
        }
    }

    pub fn new() -> Self {
        SimpleClockBinLedFace::new_static()
    }

    fn update_alarm_indicator(&mut self, settings_alarm_enabled: bool) {
        self.alarm_enabled = settings_alarm_enabled;
        if self.alarm_enabled {
            watch::slcd::set_indicator(Indicator::Signal);
        } else {
            watch::slcd::clear_indicator(Indicator::Signal);
        }
    }

    fn display_left_aligned(&self, value: u8) {
        if value >= 10 {
            watch::slcd::display_character(b'0' + value / 10, 4);
            watch::slcd::display_character(b'0' + value % 10, 5);
        } else {
            watch::slcd::display_character(b'0' + value, 4);
            watch::slcd::display_character(b' ', 5);
        }
    }
}

impl WatchFace for SimpleClockBinLedFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, settings: &Settings) {
        if watch::slcd::tick_animation_is_running() {
            watch::slcd::stop_tick_animation();
        }
        if settings.clock_mode_24h() && !settings.clock_24h_leading_zero() {
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
                let date_time = rtc::get_date_time();
                if self.flashing_state > 0 {
                    if self.ticks > 0 {
                        self.ticks -= 1;
                    } else {
                        if self.flashing_state & 64 != 0 {
                            self.flashing_state &= 63;
                            self.ticks = if self.flashing_value & 1 != 0 { 7 } else { 1 };
                            movement::illuminate_led();
                        } else {
                            watch::led::set_led_off();
                            if self.flashing_state & 128 == 0 {
                                self.flashing_value >>= 1;
                            }
                            if self.flashing_value != 0 || self.flashing_state & 128 != 0 {
                                self.flashing_state &= 127;
                                self.flashing_state |= 64;
                                self.ticks = 6;
                            } else if self.flashing_state & 1 != 0 {
                                self.flashing_state = 2 + 128;
                                self.flashing_value = date_time.minute;
                                self.display_left_aligned(self.flashing_value);
                                self.ticks = 9;
                            } else {
                                self.flashing_state = 0;
                                self.previous_date_time = 0xFFFF_FFFF;
                                watch::slcd::set_colon();
                            }
                        }
                    }
                } else {
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
                    let mut buf = [0u8; 11];
                    let mut set_leading_zero = false;
                    if (date_time.to_reg() >> 6) == (previous >> 6) {
                        buf[0] = b'0' + date_time.second / 10;
                        buf[1] = b'0' + date_time.second % 10;
                        watch::slcd::display_string(
                            core::str::from_utf8(&buf[..2]).unwrap_or("  "),
                            8,
                        );
                    } else if (date_time.to_reg() >> 12) == (previous >> 12) {
                        buf[0] = b'0' + date_time.minute / 10;
                        buf[1] = b'0' + date_time.minute % 10;
                        buf[2] = b'0' + date_time.second / 10;
                        buf[3] = b'0' + date_time.second % 10;
                        watch::slcd::display_string(
                            core::str::from_utf8(&buf[..4]).unwrap_or(""),
                            6,
                        );
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
                        } else if settings.clock_24h_leading_zero() && hour < 10 {
                            set_leading_zero = true;
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
                        watch::slcd::display_string(
                            core::str::from_utf8(&buf[..]).unwrap_or(""),
                            0,
                        );
                    }
                    if set_leading_zero {
                        watch::slcd::display_string("0", 4);
                    }
                    if self.alarm_enabled != settings.alarm_enabled() {
                        self.update_alarm_indicator(settings.alarm_enabled());
                    }
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
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.flashing_state == 0 {
                    let mut date_time = rtc::get_date_time();
                    self.flashing_state = 1 + 128;
                    self.ticks = 4;
                    if !settings.clock_mode_24h() {
                        date_time.hour %= 12;
                        if date_time.hour == 0 {
                            date_time.hour = 12;
                        }
                    }
                    watch::slcd::display_string("      ", 4);
                    self.display_left_aligned(date_time.hour);
                    self.flashing_value = if date_time.hour > 12 {
                        date_time.hour - 12
                    } else {
                        date_time.hour
                    };
                    watch::led::set_led_off();
                    watch::slcd::clear_colon();
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
