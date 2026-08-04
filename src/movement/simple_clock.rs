//! Simple clock watch face.
//!
//! Port of the C `simple_clock_face.c`. Displays the weekday, day, and time,
//! with a battery-low indicator and an optional hourly signal.

use crate::movement::types::{Event, EventType, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc::{self, DateTime};
use crate::watch::slcd::Indicator;
use crate::watch::utility;

/// The state for the simple clock face.
pub struct SimpleClockFace {
    signal_enabled: bool,
    watch_face_index: usize,
    previous_date_time: u32,
    last_battery_check: u8,
    battery_low: bool,
    alarm_enabled: bool,
}

impl SimpleClockFace {
    pub fn new() -> Self {
        SimpleClockFace {
            signal_enabled: false,
            watch_face_index: 0,
            previous_date_time: 0xFFFF_FFFF,
            last_battery_check: 0xFF,
            battery_low: false,
            alarm_enabled: false,
        }
    }

    fn update_alarm_indicator(&mut self, settings_alarm_enabled: bool) {
        self.alarm_enabled = settings_alarm_enabled;
        if self.alarm_enabled {
            watch::slcd::set_indicator(Indicator::Signal);
        } else {
            watch::slcd::clear_indicator(Indicator::Signal);
        }
    }
}

impl WatchFace for SimpleClockFace {
    fn setup(&mut self, _settings: &Settings, watch_face_index: usize) {
        self.watch_face_index = watch_face_index;
    }

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

    fn loop_(&mut self, event: Event, settings: &Settings) -> bool {
        let mut buf = [0u8; 11];
        let pos: u8;

        match event.event_type {
            EventType::Activate | EventType::Tick | EventType::LowEnergyUpdate => {
                let date_time = rtc::get_date_time();
                let previous_date_time = self.previous_date_time;
                self.previous_date_time = date_time.to_reg();

                // Check the battery voltage once a day.
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

                let mut set_leading_zero = false;
                if (date_time.to_reg() >> 6) == (previous_date_time >> 6)
                    && event.event_type != EventType::LowEnergyUpdate
                {
                    // Only seconds changed; update just the seconds.
                    write_seconds(&mut buf, date_time);
                    watch::slcd::display_string(core::str::from_utf8(&buf[..4]).unwrap_or("  "), 8);
                    return true;
                } else if (date_time.to_reg() >> 12) == (previous_date_time >> 12)
                    && event.event_type != EventType::LowEnergyUpdate
                {
                    // Only minutes/seconds changed.
                    pos = 6;
                    write_minutes_seconds(&mut buf, date_time);
                } else {
                    // Everything changed; render it all.
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
                    if settings.clock_mode_24h() && settings.clock_24h_leading_zero() && hour < 10 {
                        set_leading_zero = true;
                    }
                    pos = 0;
                    if event.event_type == EventType::LowEnergyUpdate {
                        if !watch::slcd::tick_animation_is_running() {
                            watch::slcd::start_tick_animation(500);
                        }
                        write_low_energy(&mut buf, date_time, hour);
                    } else {
                        write_full(&mut buf, date_time, hour);
                    }
                }
                watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), pos);

                if set_leading_zero {
                    watch::slcd::display_string("0", 4);
                }

                if self.alarm_enabled != settings.alarm_enabled() {
                    self.update_alarm_indicator(settings.alarm_enabled());
                }
            }
            EventType::AlarmLongPress => {
                self.signal_enabled = !self.signal_enabled;
                if self.signal_enabled {
                    watch::slcd::set_indicator(Indicator::Bell);
                } else {
                    watch::slcd::clear_indicator(Indicator::Bell);
                }
            }
            EventType::BackgroundTask => {
                crate::movement::play_signal();
            }
            _ => return crate::movement::default_loop_handler(event, settings),
        }

        true
    }

    fn resign(&mut self, _settings: &Settings) {}

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        if !self.signal_enabled {
            return false;
        }
        let date_time = rtc::get_date_time();
        date_time.minute == 0
    }
}

fn write_seconds(buf: &mut [u8; 11], dt: DateTime) {
    buf[0] = b'0' + dt.second / 10;
    buf[1] = b'0' + dt.second % 10;
}

fn write_minutes_seconds(buf: &mut [u8; 11], dt: DateTime) {
    buf[0] = b'0' + dt.minute / 10;
    buf[1] = b'0' + dt.minute % 10;
    buf[2] = b'0' + dt.second / 10;
    buf[3] = b'0' + dt.second % 10;
}

fn write_low_energy(buf: &mut [u8; 11], dt: DateTime, hour: u8) {
    let weekday = utility::get_weekday(dt);
    let wb = weekday.as_bytes();
    buf[0] = wb[0];
    buf[1] = wb[1];
    buf[2] = b'0' + dt.day / 10;
    buf[3] = b'0' + dt.day % 10;
    buf[4] = b'0' + hour / 10;
    buf[5] = b'0' + hour % 10;
    buf[6] = b'0' + dt.minute / 10;
    buf[7] = b'0' + dt.minute % 10;
}

fn write_full(buf: &mut [u8; 11], dt: DateTime, hour: u8) {
    let weekday = utility::get_weekday(dt);
    let wb = weekday.as_bytes();
    buf[0] = wb[0];
    buf[1] = wb[1];
    buf[2] = b'0' + dt.day / 10;
    buf[3] = b'0' + dt.day % 10;
    buf[4] = b'0' + hour / 10;
    buf[5] = b'0' + hour % 10;
    buf[6] = b'0' + dt.minute / 10;
    buf[7] = b'0' + dt.minute % 10;
    buf[8] = b'0' + dt.second / 10;
    buf[9] = b'0' + dt.second % 10;
}
