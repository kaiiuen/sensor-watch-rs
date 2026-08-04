//! Close-enough clock watch face.
//!
//! Port of the C `close_enough_clock_face.c`. Shows the time in words, rounded
//! to the nearest five minutes ("10 past 3", "20 to 4", etc.). It is a pure
//! state machine: it renders on wake and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;

/// The words for each five-minute period.
const WORDS: [&str; 12] = [
    "  ", " 5", "10", "15", "20", "25", "30", "35", "40", "45", "50", "55",
];
const PAST_WORD: &str = " P";
const TO_WORD: &str = " 2";
const OCLOCK_WORD: &str = "OC";

/// When within the five-minute period we switch from "X past HH" to "X to HH+1".
const HOUR_SWITCH_INDEX: i32 = 8;

/// The close-enough clock face state.
pub struct CloseEnoughClockFace {
    prev_five_minute_period: i32,
    prev_min_checked: i32,
    last_battery_check: u8,
    battery_low: bool,
    alarm_enabled: bool,
}

impl CloseEnoughClockFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        CloseEnoughClockFace {
            prev_five_minute_period: -1,
            prev_min_checked: -1,
            last_battery_check: 0xFF,
            battery_low: false,
            alarm_enabled: false,
        }
    }

    pub fn new() -> Self {
        CloseEnoughClockFace::new_static()
    }

    fn update_alarm_indicator(&mut self, settings_alarm_enabled: bool) {
        self.alarm_enabled = settings_alarm_enabled;
        if self.alarm_enabled {
            watch::slcd::set_indicator(Indicator::Bell);
        } else {
            watch::slcd::clear_indicator(Indicator::Bell);
        }
    }
}

impl WatchFace for CloseEnoughClockFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, settings: &Settings) {
        if watch::slcd::tick_animation_is_running() {
            watch::slcd::stop_tick_animation();
        }
        if settings.clock_mode_24h() {
            watch::slcd::set_indicator(Indicator::H24);
        }
        self.update_alarm_indicator(settings.alarm_enabled());
        self.prev_five_minute_period = -1;
        self.prev_min_checked = -1;
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => self.draw_clock(settings),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}

impl CloseEnoughClockFace {
    fn draw_clock(&mut self, settings: &mut Settings) {
        let mut buf = [0u8; 11];
        let date_time = rtc::get_date_time();

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

        // Same minute, skip.
        if date_time.minute as i32 == self.prev_min_checked {
            return;
        }
        self.prev_min_checked = date_time.minute as i32;

        let mut five_minute_period = (date_time.minute / 5) % 12;
        let mut show_next_hour = false;

        // If we are more than halfway to the next five-minute interval, move up.
        if (date_time.minute % 5) > 2 {
            if five_minute_period == 11 {
                show_next_hour = true;
            }
            five_minute_period = (five_minute_period + 1) % 12;
        }

        // Same five-minute period, skip.
        if five_minute_period as i32 == self.prev_five_minute_period {
            return;
        }

        let mut close_enough_hour = date_time.hour as i32;
        if five_minute_period as i32 >= HOUR_SWITCH_INDEX || show_next_hour {
            close_enough_hour = (close_enough_hour + 1) % 24;
        }

        if !settings.clock_mode_24h() {
            if close_enough_hour < 12 {
                watch::slcd::clear_indicator(Indicator::Pm);
            } else {
                watch::slcd::set_indicator(Indicator::Pm);
            }
            close_enough_hour %= 12;
            if close_enough_hour == 0 {
                close_enough_hour = 12;
            }
        }

        let weekday = crate::watch::utility::get_weekday(date_time);
        let wb = weekday.as_bytes();
        buf[0] = wb[0];
        buf[1] = wb[1];
        buf[2] = b'0' + date_time.day / 10;
        buf[3] = b'0' + date_time.day % 10;

        if five_minute_period == 0 {
            // "HH  OC"
            buf[4] = b'0' + (close_enough_hour / 10) as u8;
            buf[5] = b'0' + (close_enough_hour % 10) as u8;
            let w = WORDS[0].as_bytes();
            buf[6] = w[0];
            buf[7] = w[1];
            let o = OCLOCK_WORD.as_bytes();
            buf[8] = o[0];
            buf[9] = o[1];
        } else {
            let first = if five_minute_period as i32 >= HOUR_SWITCH_INDEX {
                WORDS[(12 - five_minute_period) as usize]
            } else {
                WORDS[five_minute_period as usize]
            };
            let second = if five_minute_period as i32 >= HOUR_SWITCH_INDEX {
                TO_WORD
            } else {
                PAST_WORD
            };
            let f = first.as_bytes();
            buf[4] = f[0];
            buf[5] = f[1];
            let s = second.as_bytes();
            buf[6] = s[0];
            buf[7] = s[1];
            buf[8] = b'0' + (close_enough_hour / 10) as u8;
            buf[9] = b'0' + (close_enough_hour % 10) as u8;
        }

        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        self.prev_five_minute_period = five_minute_period as i32;

        if self.alarm_enabled != settings.alarm_enabled() {
            self.update_alarm_indicator(settings.alarm_enabled());
        }
    }
}
