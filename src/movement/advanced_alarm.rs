//! Advanced Alarm watch face.
//!
//! Port of the C `advanced_alarm_face.c` from Second Movement. Implements up to
//! 16 alarm slots, each with a day mode, hour, minute, pitch, and beep rounds.
//! It is a pure state machine: it reacts to a single event and returns; it never
//! keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, ClockMode, Event, Settings, WatchFace};
use crate::watch::buzzer::Note;
use crate::watch::rtc;
use crate::watch::slcd;
use crate::watch::slcd::Indicator;

/// Number of alarm slots.
const ALARM_ALARMS: usize = 16;
/// Number of day settings (MO..SU, each day, one time, workday, weekend).
const ALARM_DAY_STATES: u8 = 11;
const ALARM_DAY_EACH_DAY: u8 = 7;
const ALARM_DAY_ONE_TIME: u8 = 8;
const ALARM_DAY_WORKDAY: u8 = 9;
const ALARM_DAY_WEEKEND: u8 = 10;
/// Maximum number of beep rounds (including short and long alarms).
const ALARM_MAX_BEEP_ROUNDS: u8 = 11;
/// Number of settings states.
const ALARM_SETTING_STATES: u8 = 6;

/// Day-of-week display strings (custom, 3-char).
const DOW_CUSTOM: [&str; 11] = [
    "MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN", "DAY", "1t ", "M-F", "WKD",
];

/// The three pitch notes.
const BUZZER_NOTES: [Note; 3] = [Note::B6, Note::C8, Note::A8];

/// A single alarm slot.
#[derive(Clone, Copy, Debug)]
struct AlarmSetting {
    day: u8,
    hour: u8,
    minute: u8,
    beeps: u8,
    pitch: u8,
    enabled: bool,
}

impl AlarmSetting {
    const fn new() -> Self {
        AlarmSetting {
            day: ALARM_DAY_EACH_DAY,
            hour: 0,
            minute: 0,
            beeps: 5,
            pitch: 1,
            enabled: false,
        }
    }
}

/// The advanced alarm face state.
pub struct AdvancedAlarmFace {
    alarm_idx: u8,
    alarm_playing_idx: u8,
    setting_state: u8,
    alarm_handled_minute: i8,
    alarm_quick_ticks: bool,
    is_setting: bool,
    wait_ticks: i8,
    alarm: [AlarmSetting; ALARM_ALARMS],
}

impl AdvancedAlarmFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        AdvancedAlarmFace {
            alarm_idx: 0,
            alarm_playing_idx: 0,
            setting_state: 0,
            alarm_handled_minute: -1,
            alarm_quick_ticks: false,
            is_setting: false,
            wait_ticks: -1,
            alarm: [AlarmSetting::new(); ALARM_ALARMS],
        }
    }

    pub fn new() -> Self {
        AdvancedAlarmFace::new_static()
    }

    /// Computes the ISO weekday index (0=Monday .. 6=Sunday).
    fn get_weekday_idx(dt: rtc::DateTime) -> u8 {
        let mut year = dt.year as i32 + 20;
        let mut month = dt.month as i32;
        if month <= 2 {
            month += 12;
            year -= 1;
        }
        ((dt.day as i32 + 13 * (month + 1) / 5 + year + year / 4 + 525 - 2) % 7) as u8
    }

    fn alarm_set_signal(&self) {
        if self.alarm[self.alarm_idx as usize].enabled {
            slcd::set_indicator(Indicator::Signal);
        } else {
            slcd::clear_indicator(Indicator::Signal);
        }
    }

    fn alarm_show_alarm_on_text(&self) {
        let on = self.alarm[self.alarm_idx as usize].enabled;
        slcd::display_string(if on { "on" } else { "--" }, 8);
    }

    fn draw(&mut self, subsecond: u8) {
        let set_leading_zero = movement::clock_mode_24h() == ClockMode::H024;
        let mut i = 0u8;
        if self.is_setting {
            i = self.alarm[self.alarm_idx as usize].day + 1;
        }
        let mut h = self.alarm[self.alarm_idx as usize].hour;
        match movement::clock_mode_24h() {
            ClockMode::H12 => {
                if h >= 12 {
                    slcd::set_indicator(Indicator::Pm);
                    h %= 12;
                } else {
                    slcd::clear_indicator(Indicator::Pm);
                }
                if h == 0 {
                    h = 12;
                }
            }
            _ => slcd::set_indicator(Indicator::H24),
        }

        let blinking = self.is_setting
            && subsecond % 2 == 1
            && self.setting_state < 4
            && !self.alarm_quick_ticks;

        // Alarm slot number (top right).
        if self.setting_state == 0 && blinking {
            slcd::display_string("  ", 2);
        } else {
            let mut buf = [0u8; 2];
            buf[0] = b'0' + (self.alarm_idx + 1) / 10;
            buf[1] = b'0' + (self.alarm_idx + 1) % 10;
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("  "), 2);
        }
        // Day (top left).
        if self.setting_state == 1 && blinking {
            slcd::display_string("   ", 0);
        } else {
            let idx = (i as usize).min(DOW_CUSTOM.len() - 1);
            slcd::display_string(DOW_CUSTOM[idx], 0);
        }
        // Hour (hours position).
        if self.setting_state == 2 && blinking {
            slcd::display_string("  ", 4);
        } else {
            let mut buf = [0u8; 2];
            if set_leading_zero {
                buf[0] = b'0' + h / 10;
                buf[1] = b'0' + h % 10;
            } else {
                buf[0] = if h / 10 == 0 { b' ' } else { b'0' + h / 10 };
                buf[1] = b'0' + h % 10;
            }
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("  "), 4);
        }
        // Minute (minutes position).
        if self.setting_state == 3 && blinking {
            slcd::display_string("  ", 6);
        } else {
            let mut buf = [0u8; 2];
            buf[0] = b'0' + self.alarm[self.alarm_idx as usize].minute / 10;
            buf[1] = b'0' + self.alarm[self.alarm_idx as usize].minute % 10;
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("  "), 6);
        }

        if self.is_setting {
            slcd::display_string("  ", 8);
            // Pitch level indicator (segments 5D, 5G, 5A).
            if subsecond % 2 == 0 || self.setting_state != 4 {
                for k in 0..=self.alarm[self.alarm_idx as usize].pitch.min(2) {
                    slcd::set_pixel(k, 3);
                }
            }
            // Beep rounds indicator.
            if subsecond % 2 == 0 || self.setting_state != 5 {
                let beeps = self.alarm[self.alarm_idx as usize].beeps;
                if beeps == ALARM_MAX_BEEP_ROUNDS - 1 {
                    slcd::display_character(b'L', 9);
                } else if beeps == 0 {
                    slcd::display_character(b'o', 9);
                } else {
                    slcd::display_character(b'0' + beeps, 9);
                }
            }
        } else {
            self.alarm_show_alarm_on_text();
        }

        self.alarm_set_signal();
    }

    fn initiate_setting(&mut self, subsecond: u8) {
        self.is_setting = true;
        self.setting_state = 0;
        movement::request_tick_frequency(4);
        self.draw(subsecond);
    }

    fn resume_setting(&mut self, subsecond: u8) {
        self.is_setting = false;
        movement::request_tick_frequency(1);
        self.draw(subsecond);
    }

    fn update_alarm_enabled(&mut self) {
        let mut active_alarms = false;
        let now = movement::get_local_date_time();
        let weekday_idx = Self::get_weekday_idx(now);
        let now_minutes_of_day = now.hour as u16 * 60 + now.minute as u16;
        for i in 0..ALARM_ALARMS {
            if self.alarm[i].enabled {
                let day = self.alarm[i].day;
                if day == ALARM_DAY_EACH_DAY || day == ALARM_DAY_ONE_TIME {
                    active_alarms = true;
                    break;
                }
                let alarm_minutes_of_day =
                    self.alarm[i].hour as u16 * 60 + self.alarm[i].minute as u16;
                if (day == weekday_idx && alarm_minutes_of_day >= now_minutes_of_day)
                    || ((weekday_idx + 1) % 7 == day && alarm_minutes_of_day <= now_minutes_of_day)
                    || (day == ALARM_DAY_WORKDAY
                        && (weekday_idx < 4
                            || (weekday_idx == 4 && alarm_minutes_of_day >= now_minutes_of_day)
                            || (weekday_idx == 6 && alarm_minutes_of_day <= now_minutes_of_day)))
                    || (day == ALARM_DAY_WEEKEND
                        && (weekday_idx == 5
                            || (weekday_idx == 6 && alarm_minutes_of_day >= now_minutes_of_day)
                            || (weekday_idx == 4 && alarm_minutes_of_day <= now_minutes_of_day)))
                {
                    active_alarms = true;
                    break;
                }
            }
        }
        movement::set_alarm_enabled(active_alarms);
    }

    fn play_short_beep(&mut self, pitch_idx: u8) {
        // A short double beep sequence. The buzzer copies it synchronously.
        let mut beep_sequence: [i8; 7] = [0, 4, -1, 4, 0, 6, 0];
        beep_sequence[0] = BUZZER_NOTES[pitch_idx as usize] as i8;
        beep_sequence[4] = BUZZER_NOTES[pitch_idx as usize] as i8;
        movement::play_sequence(&beep_sequence, None);
    }

    fn indicate_beep(&mut self) {
        let beeps = self.alarm[self.alarm_idx as usize].beeps;
        let pitch = self.alarm[self.alarm_idx as usize].pitch;
        if beeps == 0 {
            self.play_short_beep(pitch);
        } else {
            movement::play_alarm_beeps(1, BUZZER_NOTES[pitch as usize]);
        }
    }

    fn abort_quick_ticks(&mut self) {
        if self.alarm_quick_ticks {
            self.alarm[self.alarm_idx as usize].enabled = true;
            self.alarm_quick_ticks = false;
            movement::request_tick_frequency(4);
        }
    }
}

impl WatchFace for AdvancedAlarmFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        slcd::set_colon();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Tick => {
                if self.alarm_quick_ticks {
                    if self.setting_state == 2 {
                        let a = &mut self.alarm[self.alarm_idx as usize];
                        a.hour = (a.hour + 1) % 24;
                    } else if self.setting_state == 3 {
                        let a = &mut self.alarm[self.alarm_idx as usize];
                        a.minute = (a.minute + 1) % 60;
                    } else {
                        self.abort_quick_ticks();
                    }
                } else if !self.is_setting {
                    if self.wait_ticks >= 0 {
                        self.wait_ticks += 1;
                    }
                    if self.wait_ticks == 2 {
                        // Extra-long press of the alarm button: back to alarm 1.
                        self.wait_ticks = -1;
                        if self.alarm_idx != 0 {
                            self.alarm[self.alarm_idx as usize].enabled ^= true;
                            self.alarm_set_signal();
                            self.alarm_show_alarm_on_text();
                            self.alarm_idx = 0;
                        }
                    }
                }
                self.draw(0);
            }
            Event::Activate => self.draw(0),
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if !self.is_setting {
                    movement::illuminate_led();
                    self.initiate_setting(0);
                    return;
                }
                self.setting_state += 1;
                if self.setting_state >= ALARM_SETTING_STATES {
                    self.resume_setting(0);
                }
                self.draw(0);
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.is_setting {
                    self.resume_setting(0);
                } else {
                    self.initiate_setting(0);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if !self.is_setting {
                    self.wait_ticks = -1;
                    self.alarm_idx = (self.alarm_idx + 1) % ALARM_ALARMS as u8;
                } else {
                    match self.setting_state {
                        0 => self.alarm_idx = (self.alarm_idx + 1) % ALARM_ALARMS as u8,
                        1 => {
                            let a = &mut self.alarm[self.alarm_idx as usize];
                            a.day = (a.day + 1) % ALARM_DAY_STATES;
                        }
                        2 => {
                            self.abort_quick_ticks();
                            let a = &mut self.alarm[self.alarm_idx as usize];
                            a.hour = (a.hour + 1) % 24;
                        }
                        3 => {
                            self.abort_quick_ticks();
                            let a = &mut self.alarm[self.alarm_idx as usize];
                            a.minute = (a.minute + 1) % 60;
                        }
                        4 => {
                            let a = &mut self.alarm[self.alarm_idx as usize];
                            a.pitch = (a.pitch + 1) % 3;
                            self.indicate_beep();
                        }
                        5 => {
                            let a = &mut self.alarm[self.alarm_idx as usize];
                            a.beeps = (a.beeps + 1) % ALARM_MAX_BEEP_ROUNDS;
                            if a.beeps <= 1 {
                                self.indicate_beep();
                            }
                        }
                        _ => {}
                    }
                    if self.setting_state > 0 {
                        self.alarm[self.alarm_idx as usize].enabled = true;
                    }
                }
                self.draw(0);
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if !self.is_setting {
                    self.alarm[self.alarm_idx as usize].enabled ^= true;
                    self.wait_ticks = 0;
                } else {
                    match self.setting_state {
                        0 => self.alarm_idx = 0,
                        2 | 3 => {
                            movement::request_tick_frequency(8);
                            self.alarm_quick_ticks = true;
                        }
                        _ => {}
                    }
                }
                self.draw(0);
            }
            Event::Button(Button::Alarm, ButtonEvent::LongUp) => {
                if self.is_setting {
                    if self.setting_state == 2 || self.setting_state == 3 {
                        self.abort_quick_ticks();
                    }
                } else {
                    self.wait_ticks = -1;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::ReallyLongPress) => {
                self.wait_ticks = 0;
            }
            Event::BackgroundTask => {
                let playing = self.alarm_playing_idx as usize;
                let beeps = self.alarm[playing].beeps;
                let pitch = self.alarm[playing].pitch;
                if beeps == 0 {
                    self.play_short_beep(pitch);
                } else {
                    let rounds = if beeps == ALARM_MAX_BEEP_ROUNDS - 1 {
                        20
                    } else {
                        beeps
                    };
                    movement::play_alarm_beeps(rounds, BUZZER_NOTES[pitch as usize]);
                }
                // One-time alarm: erase it.
                if self.alarm[playing].day == ALARM_DAY_ONE_TIME {
                    self.alarm[playing].day = ALARM_DAY_EACH_DAY;
                    self.alarm[playing].minute = 0;
                    self.alarm[playing].hour = 0;
                    self.alarm[playing].beeps = 5;
                    self.alarm[playing].pitch = 1;
                    self.alarm[playing].enabled = false;
                    self.update_alarm_enabled();
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.is_setting = false;
        self.update_alarm_enabled();
        movement::force_led_off();
        self.alarm_quick_ticks = false;
        self.wait_ticks = -1;
        movement::request_tick_frequency(1);
    }

    fn advise(&mut self, _settings: &Settings) {
        let now = movement::get_local_date_time();
        // Failsafe: never fire more than one alarm within a minute.
        if self.alarm_handled_minute == now.minute as i8 {
            return;
        }
        self.alarm_handled_minute = now.minute as i8;
        let weekday_idx = Self::get_weekday_idx(now);
        let mut wants = false;
        for i in 0..ALARM_ALARMS {
            if self.alarm[i].enabled
                && self.alarm[i].minute == now.minute
                && self.alarm[i].hour == now.hour
            {
                self.alarm_playing_idx = i as u8;
                let day = self.alarm[i].day;
                if day == ALARM_DAY_EACH_DAY || day == ALARM_DAY_ONE_TIME {
                    wants = true;
                }
                if day == weekday_idx {
                    wants = true;
                }
                if day == ALARM_DAY_WORKDAY && weekday_idx < 5 {
                    wants = true;
                }
                if day == ALARM_DAY_WEEKEND && weekday_idx >= 5 {
                    wants = true;
                }
            }
        }
        if wants {
            return;
        }
        self.alarm_handled_minute = -1;
        // Update the movement's alarm indicator five times an hour.
        if now.minute % 12 == 0 {
            self.update_alarm_enabled();
        }
    }

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        let now = movement::get_local_date_time();
        let weekday_idx = Self::get_weekday_idx(now);
        for i in 0..ALARM_ALARMS {
            if self.alarm[i].enabled
                && self.alarm[i].minute == now.minute
                && self.alarm[i].hour == now.hour
            {
                let day = self.alarm[i].day;
                if day == ALARM_DAY_EACH_DAY || day == ALARM_DAY_ONE_TIME {
                    return true;
                }
                if day == weekday_idx {
                    return true;
                }
                if day == ALARM_DAY_WORKDAY && weekday_idx < 5 {
                    return true;
                }
                if day == ALARM_DAY_WEEKEND && weekday_idx >= 5 {
                    return true;
                }
            }
        }
        false
    }
}
