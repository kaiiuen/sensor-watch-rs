//! Alarm watch face.
//!
//! Port of the C `alarm_face.c`, adapted to the event-driven model. Supports
//! multiple alarms with day-of-week, hour, minute, pitch, and beep settings.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note as BuzzerNote;
use crate::watch::rtc::{self, DateTime};
use crate::watch::slcd::Indicator;

const ALARM_ALARMS: usize = 16;
const ALARM_DAY_STATES: u8 = 11;
const ALARM_DAY_EACH_DAY: u8 = 7;
const ALARM_DAY_ONE_TIME: u8 = 8;
const ALARM_DAY_WORKDAY: u8 = 9;
const ALARM_DAY_WEEKEND: u8 = 10;
const ALARM_MAX_BEEP_ROUNDS: u8 = 11;
const ALARM_SETTING_STATES: u8 = 6;

const DOW_STRINGS: [&str; 12] = [
    "AL", "MO", "TU", "WE", "TH", "FR", "SA", "SO", "ED", "1t", "MF", "WN",
];
const BLINK_IDX: [usize; 6] = [2, 0, 4, 6, 8, 9];
const BLINK_IDX2: [usize; 6] = [3, 1, 5, 7, 8, 9];
const BUZZER_NOTES: [BuzzerNote; 3] = [BuzzerNote::B6, BuzzerNote::C8, BuzzerNote::A8];
const BUZZER_SEGDATA: [[u8; 2]; 3] = [[0, 3], [1, 3], [2, 2]];

/// A single alarm slot.
#[derive(Clone, Copy)]
struct Alarm {
    enabled: bool,
    day: u8,
    hour: u8,
    minute: u8,
    pitch: u8,
    beeps: u8,
}

impl Alarm {
    const fn new() -> Self {
        Alarm {
            enabled: false,
            day: ALARM_DAY_EACH_DAY,
            hour: 0,
            minute: 0,
            pitch: 1,
            beeps: 5,
        }
    }
}

/// The alarm face state.
pub struct AlarmFace {
    watch_face_index: usize,
    alarm_idx: usize,
    alarm_playing_idx: usize,
    setting_state: u8,
    alarm_handled_minute: i8,
    alarm_quick_ticks: bool,
    is_setting: bool,
    alarm: [Alarm; ALARM_ALARMS],
}

impl AlarmFace {
    pub const fn new_static() -> Self {
        AlarmFace {
            watch_face_index: 0,
            alarm_idx: 0,
            alarm_playing_idx: 0,
            setting_state: 0,
            alarm_handled_minute: -1,
            alarm_quick_ticks: false,
            is_setting: false,
            alarm: [Alarm::new(); ALARM_ALARMS],
        }
    }

    fn get_weekday_idx(dt: DateTime) -> u8 {
        let mut year = dt.year as i32 + 20;
        let mut month = dt.month as i32;
        if month <= 2 {
            month += 12;
            year -= 1;
        }
        ((dt.day as i32 + 13 * (month + 1) / 5 + year + year / 4 + 525 - 2) % 7) as u8
    }

    fn set_signal(&self) {
        if self.alarm[self.alarm_idx].enabled {
            watch::slcd::set_indicator(Indicator::Signal);
        } else {
            watch::slcd::clear_indicator(Indicator::Signal);
        }
    }

    fn draw(&mut self, settings: &Settings, subsecond: u8) {
        let mut buf = [0u8; 11];
        let mut i = 0;
        if self.is_setting {
            i = (self.alarm[self.alarm_idx].day + 1) as usize;
        }
        let mut set_leading_zero = false;
        let mut h = self.alarm[self.alarm_idx].hour;
        if !settings.clock_mode_24h() {
            if h >= 12 {
                watch::slcd::set_indicator(Indicator::Pm);
                h %= 12;
            } else {
                watch::slcd::clear_indicator(Indicator::Pm);
            }
            if h == 0 {
                h = 12;
            }
        } else {
            watch::slcd::set_indicator(Indicator::H24);
            if settings.clock_24h_leading_zero() && h < 10 {
                set_leading_zero = true;
            }
        }

        let dow = DOW_STRINGS[i.min(11)];
        let db = dow.as_bytes();
        buf[0] = db[0];
        buf[1] = db[1];
        buf[2] = b'0' + (self.alarm_idx as u8 + 1) / 10;
        buf[3] = b'0' + (self.alarm_idx as u8 + 1) % 10;
        buf[4] = if set_leading_zero {
            b'0' + h / 10
        } else {
            b' '
        };
        buf[5] = b'0' + h % 10;
        buf[6] = b'0' + self.alarm[self.alarm_idx].minute / 10;
        buf[7] = b'0' + self.alarm[self.alarm_idx].minute % 10;

        // Blink items in settings mode.
        if self.is_setting
            && subsecond % 2 == 1
            && self.setting_state < 4
            && !self.alarm_quick_ticks
        {
            buf[BLINK_IDX[self.setting_state as usize]] = b' ';
            buf[BLINK_IDX2[self.setting_state as usize]] = b' ';
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);

        if self.is_setting {
            // Draw pitch level indicator.
            if subsecond.is_multiple_of(2) || self.setting_state != 4 {
                for k in 0..=self.alarm[self.alarm_idx].pitch.min(2) {
                    watch::slcd::set_pixel(
                        BUZZER_SEGDATA[k as usize][0],
                        BUZZER_SEGDATA[k as usize][1],
                    );
                }
            }
            // Draw beep rounds indicator.
            if subsecond.is_multiple_of(2) || self.setting_state != 5 {
                let beeps = self.alarm[self.alarm_idx].beeps;
                if beeps == ALARM_MAX_BEEP_ROUNDS - 1 {
                    watch::slcd::display_character(b'L', BLINK_IDX[5] as u8);
                } else if beeps == 0 {
                    watch::slcd::display_character(b'o', BLINK_IDX[5] as u8);
                } else {
                    watch::slcd::display_character(b'0' + beeps, BLINK_IDX[5] as u8);
                }
            }
        }
        self.set_signal();
    }

    fn update_alarm_enabled(&mut self, settings: &mut Settings) {
        let mut active_alarms = false;
        let mut now: Option<DateTime> = None;
        let mut weekday_idx = 0;
        let mut now_minutes_of_day = 0;
        for i in 0..ALARM_ALARMS {
            if self.alarm[i].enabled {
                if self.alarm[i].day == ALARM_DAY_EACH_DAY
                    || self.alarm[i].day == ALARM_DAY_ONE_TIME
                {
                    active_alarms = true;
                    break;
                } else {
                    if now.is_none() {
                        let n = rtc::get_date_time();
                        weekday_idx = Self::get_weekday_idx(n);
                        now_minutes_of_day = n.hour as u16 * 60 + n.minute as u16;
                        now = Some(n);
                    }
                    let alarm_minutes_of_day =
                        self.alarm[i].hour as u16 * 60 + self.alarm[i].minute as u16;
                    if (self.alarm[i].day == weekday_idx
                        && alarm_minutes_of_day >= now_minutes_of_day)
                        || ((weekday_idx + 1) % 7 == self.alarm[i].day
                            && alarm_minutes_of_day <= now_minutes_of_day)
                        || (self.alarm[i].day == ALARM_DAY_WORKDAY
                            && (weekday_idx < 4
                                || (weekday_idx == 4
                                    && alarm_minutes_of_day >= now_minutes_of_day)
                                || (weekday_idx == 6
                                    && alarm_minutes_of_day <= now_minutes_of_day)))
                        || (self.alarm[i].day == ALARM_DAY_WEEKEND
                            && (weekday_idx == 5
                                || (weekday_idx == 6
                                    && alarm_minutes_of_day >= now_minutes_of_day)
                                || (weekday_idx == 4
                                    && alarm_minutes_of_day <= now_minutes_of_day)))
                    {
                        active_alarms = true;
                        break;
                    }
                }
            }
        }
        settings.set_alarm_enabled(active_alarms);
    }

    fn play_short_beep(&self, pitch_idx: u8) {
        let note = BUZZER_NOTES[pitch_idx as usize];
        crate::movement::play_alarm_beeps(1, note);
    }

    fn indicate_beep(&self) {
        let beeps = self.alarm[self.alarm_idx].beeps;
        let pitch = self.alarm[self.alarm_idx].pitch;
        if beeps == 0 {
            self.play_short_beep(pitch);
        } else {
            crate::movement::play_alarm_beeps(1, BUZZER_NOTES[pitch as usize]);
        }
    }
}

impl WatchFace for AlarmFace {
    fn setup(&mut self, _settings: &Settings, watch_face_index: usize) {
        self.watch_face_index = watch_face_index;
    }

    fn activate(&mut self, _settings: &Settings) {
        watch::slcd::set_colon();
    }

    fn resign(&mut self, settings: &mut Settings) {
        self.is_setting = false;
        self.update_alarm_enabled(settings);
        watch::led::set_led_off();
        self.alarm_quick_ticks = false;
    }

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        let now = rtc::get_date_time();
        if self.alarm_handled_minute == now.minute as i8 {
            return false;
        }
        self.alarm_handled_minute = now.minute as i8;
        for i in 0..ALARM_ALARMS {
            if self.alarm[i].enabled
                && self.alarm[i].minute == now.minute
                && self.alarm[i].hour == now.hour
            {
                self.alarm_playing_idx = i;
                if self.alarm[i].day == ALARM_DAY_EACH_DAY
                    || self.alarm[i].day == ALARM_DAY_ONE_TIME
                {
                    return true;
                }
                let weekday_idx = Self::get_weekday_idx(now);
                if self.alarm[i].day == weekday_idx {
                    return true;
                }
                if self.alarm[i].day == ALARM_DAY_WORKDAY && weekday_idx < 5 {
                    return true;
                }
                if self.alarm[i].day == ALARM_DAY_WEEKEND && weekday_idx >= 5 {
                    return true;
                }
            }
        }
        self.alarm_handled_minute = -1;
        false
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Tick | Event::Activate => {
                if self.alarm_quick_ticks {
                    if self.setting_state == 2 {
                        self.alarm[self.alarm_idx].hour =
                            (self.alarm[self.alarm_idx].hour + 1) % 24;
                    } else if self.setting_state == 3 {
                        self.alarm[self.alarm_idx].minute =
                            (self.alarm[self.alarm_idx].minute + 1) % 60;
                    } else {
                        self.alarm_quick_ticks = false;
                    }
                }
                self.draw(settings, event.subsecond());
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if !self.is_setting {
                    movement::illuminate_led();
                    self.is_setting = true;
                    self.setting_state = 0;
                } else {
                    self.setting_state += 1;
                    if self.setting_state >= ALARM_SETTING_STATES {
                        self.is_setting = false;
                    }
                }
                self.draw(settings, event.subsecond());
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.is_setting {
                    self.is_setting = false;
                } else {
                    self.is_setting = true;
                    self.setting_state = 0;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if !self.is_setting {
                    self.alarm_idx = (self.alarm_idx + 1) % ALARM_ALARMS;
                } else {
                    match self.setting_state {
                        0 => self.alarm_idx = (self.alarm_idx + 1) % ALARM_ALARMS,
                        1 => {
                            self.alarm[self.alarm_idx].day =
                                (self.alarm[self.alarm_idx].day + 1) % ALARM_DAY_STATES
                        }
                        2 => {
                            self.alarm[self.alarm_idx].hour =
                                (self.alarm[self.alarm_idx].hour + 1) % 24
                        }
                        3 => {
                            self.alarm[self.alarm_idx].minute =
                                (self.alarm[self.alarm_idx].minute + 1) % 60
                        }
                        4 => {
                            self.alarm[self.alarm_idx].pitch =
                                (self.alarm[self.alarm_idx].pitch + 1) % 3;
                            self.indicate_beep();
                        }
                        5 => {
                            self.alarm[self.alarm_idx].beeps =
                                (self.alarm[self.alarm_idx].beeps + 1) % ALARM_MAX_BEEP_ROUNDS;
                            if self.alarm[self.alarm_idx].beeps <= 1 {
                                self.indicate_beep();
                            }
                        }
                        _ => {}
                    }
                    if self.setting_state > 0 {
                        self.alarm[self.alarm_idx].enabled = true;
                    }
                }
                self.draw(settings, event.subsecond());
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if !self.is_setting {
                    self.alarm[self.alarm_idx].enabled ^= true;
                } else {
                    match self.setting_state {
                        0 => self.alarm_idx = 0,
                        2 | 3 => self.alarm_quick_ticks = true,
                        _ => {}
                    }
                }
                self.draw(settings, event.subsecond());
            }
            Event::Button(Button::Alarm, ButtonEvent::LongUp) => {
                self.alarm_quick_ticks = false;
            }
            Event::BackgroundTask => {
                let playing = self.alarm[self.alarm_playing_idx];
                crate::movement::play_alarm_beeps(
                    if playing.beeps == ALARM_MAX_BEEP_ROUNDS - 1 {
                        20
                    } else {
                        playing.beeps
                    },
                    BUZZER_NOTES[playing.pitch as usize],
                );
                if playing.day == ALARM_DAY_ONE_TIME {
                    self.alarm[self.alarm_playing_idx] = Alarm::new();
                }
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }
}
