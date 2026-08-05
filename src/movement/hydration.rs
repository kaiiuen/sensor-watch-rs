//! Hydration watch face.
//!
//! Port of the C `hydration_face.c` from Second Movement. Monitors daily water
//! intake with tracking, settings, and log pages. It is a pure state machine:
//! it reacts to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::slcd;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

/// Display frequency (1 Hz).
const DISPLAY_FREQUENCY: u8 = 1;
/// Maximum possible water intake (9.9 l).
const MAX_WATER_INTAKE: u16 = 9900;
/// Number of settings pages.
const NUM_SETTINGS: u8 = 5;
/// Number of log entries.
const HYDRATION_LOG_ENTRIES: usize = 30;

/// Default values.
const DEFAULT_WATER_GLASS_ML: u16 = 100;
const DEFAULT_WATER_GOAL_ML: u16 = 1600;
const DEFAULT_WAKE_HOUR: u8 = 7;
const DEFAULT_SLEEP_HOUR: u8 = 22;
const DEFAULT_ALERT_INTERVAL: u8 = 2;

/// The current page.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Tracking,
    Settings,
    Log,
}

/// The current settings page.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Setting {
    WaterGlass,
    WaterGoal,
    WakeTime,
    SleepTime,
    AlertInterval,
}

/// The log display type.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LogType {
    Intake,
    Date,
    Deviation,
}

/// A single log entry.
#[derive(Clone, Copy)]
struct LogEntry {
    water_intake: u8,
    date: u16,
}

impl LogEntry {
    const fn new() -> Self {
        LogEntry {
            water_intake: 0,
            date: 0,
        }
    }
}

/// The hydration face state.
pub struct HydrationFace {
    water_intake: u16,
    water_glass: u16,
    water_goal: u16,
    wake_hour: u8,
    sleep_hour: u8,
    alert_interval: u8,
    face_index: usize,
    display_deviation: u8,
    alert_enabled: bool,
    page: Page,
    log: [LogEntry; HYDRATION_LOG_ENTRIES],
    log_type: LogType,
    log_index: u8,
    log_head: u8,
    settings_page: Setting,
}

impl HydrationFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        HydrationFace {
            water_intake: 0,
            water_glass: DEFAULT_WATER_GLASS_ML,
            water_goal: DEFAULT_WATER_GOAL_ML,
            wake_hour: DEFAULT_WAKE_HOUR,
            sleep_hour: DEFAULT_SLEEP_HOUR,
            alert_interval: DEFAULT_ALERT_INTERVAL,
            face_index: 0,
            display_deviation: 0,
            alert_enabled: false,
            page: Page::Tracking,
            log: [LogEntry::new(); HYDRATION_LOG_ENTRIES],
            log_type: LogType::Intake,
            log_index: 0,
            log_head: 0,
            settings_page: Setting::WaterGlass,
        }
    }

    pub fn new() -> Self {
        HydrationFace::new_static()
    }

    fn display_water_ml(&self, water_ml: u16) {
        let mut buf = [0u8; 8];
        let mut v = water_ml;
        for i in (0..4).rev() {
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
        buf[4] = b'm';
        buf[5] = b'l';
        slcd::display_string(core::str::from_utf8(&buf[..6]).unwrap_or("      "), 4);
    }

    fn settings_title_display(&self, title: &str) {
        slcd::display_string(title, 0);
        if self.alert_enabled {
            slcd::set_indicator(Indicator::Signal);
        } else {
            slcd::clear_indicator(Indicator::Signal);
        }
    }

    fn settings_blink(&self, subsecond: u8) -> bool {
        if subsecond % 2 == 0 {
            slcd::display_string("      ", 4);
            return true;
        }
        false
    }

    fn settings_water_glass_display(&self, subsecond: u8) {
        self.settings_title_display("GLASS");
        if self.settings_blink(subsecond) {
            return;
        }
        slcd::clear_colon();
        slcd::clear_indicator(Indicator::H24);
        slcd::clear_indicator(Indicator::Pm);
        self.display_water_ml(self.water_glass);
    }

    fn settings_water_goal_display(&self, subsecond: u8) {
        self.settings_title_display("GOAL ");
        if self.settings_blink(subsecond) {
            return;
        }
        self.display_water_ml(self.water_goal);
    }

    fn settings_wake_time_display(&self, subsecond: u8) {
        self.settings_title_display("WAKE ");
        if self.settings_blink(subsecond) {
            return;
        }
        slcd::set_colon();
        let mut hour = self.wake_hour;
        if movement::clock_mode_24h() == crate::movement::types::ClockMode::H12 {
            if self.wake_hour > 12 {
                slcd::set_indicator(Indicator::Pm);
                hour %= 12;
            } else {
                slcd::clear_indicator(Indicator::Pm);
            }
        } else {
            slcd::set_indicator(Indicator::H24);
        }
        let mut buf = [0u8; 4];
        buf[0] = b'0' + hour / 10;
        buf[1] = b'0' + hour % 10;
        buf[2] = b'0';
        buf[3] = b'0';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("    "), 4);
    }

    fn settings_sleep_time_display(&self, subsecond: u8) {
        self.settings_title_display("SLEEP");
        if self.settings_blink(subsecond) {
            return;
        }
        slcd::set_colon();
        let mut hour = self.sleep_hour;
        if movement::clock_mode_24h() == crate::movement::types::ClockMode::H12 {
            if self.sleep_hour >= 12 {
                slcd::set_indicator(Indicator::Pm);
                hour %= 12;
            } else {
                slcd::clear_indicator(Indicator::Pm);
            }
        } else {
            slcd::set_indicator(Indicator::H24);
        }
        let mut buf = [0u8; 4];
        buf[0] = b'0' + hour / 10;
        buf[1] = b'0' + hour % 10;
        buf[2] = b'0';
        buf[3] = b'0';
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("    "), 4);
    }

    fn settings_alert_interval_display(&self, subsecond: u8) {
        self.settings_title_display("INTER");
        if self.settings_blink(subsecond) {
            return;
        }
        slcd::clear_colon();
        slcd::clear_indicator(Indicator::H24);
        slcd::clear_indicator(Indicator::Pm);
        if self.alert_interval == 0 {
            slcd::display_string(" off  ", 4);
        } else {
            let mut buf = [0u8; 6];
            buf[0] = b' ';
            buf[1] = b' ';
            buf[2] = b'0' + self.alert_interval / 10;
            buf[3] = b'0' + self.alert_interval % 10;
            buf[4] = b'h';
            buf[5] = b' ';
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("      "), 4);
        }
    }

    fn settings_display(&self, subsecond: u8) {
        match self.settings_page {
            Setting::WaterGlass => self.settings_water_glass_display(subsecond),
            Setting::WaterGoal => self.settings_water_goal_display(subsecond),
            Setting::WakeTime => self.settings_wake_time_display(subsecond),
            Setting::SleepTime => self.settings_sleep_time_display(subsecond),
            Setting::AlertInterval => self.settings_alert_interval_display(subsecond),
        }
    }

    fn settings_advance(&mut self) {
        match self.settings_page {
            Setting::WaterGlass => {
                self.water_glass += 100;
                if self.water_glass > 1000 {
                    self.water_glass = 100;
                }
            }
            Setting::WaterGoal => {
                self.water_goal += self.water_glass;
                if self.water_goal > 3000 {
                    self.water_goal = 100;
                }
            }
            Setting::WakeTime => {
                self.wake_hour = (self.wake_hour + 1) % 24;
            }
            Setting::SleepTime => {
                self.sleep_hour = (self.sleep_hour + 1) % 24;
            }
            Setting::AlertInterval => {
                self.alert_interval += 1;
                if self.alert_interval > 6 {
                    self.alert_interval = 0;
                }
            }
        }
    }

    fn beep(&self) {
        if !movement::button_should_sound() {
            return;
        }
        watch::buzzer::set_buzzer_period(watch::buzzer::NOTE_PERIODS[Note::C7 as usize] as u32);
        watch::buzzer::set_buzzer_on();
    }

    fn get_expected_intake(&self, hours_since_wake: u8) -> u16 {
        let day_hours = (self.sleep_hour + 24 - self.wake_hour) % 24;
        (self.water_goal as u32 * hours_since_wake as u32 / day_hours as u32) as u16
    }

    fn log_intake(&mut self, ts: u32) {
        if self.water_intake == 0 {
            return;
        }
        self.log_head =
            (self.log_head + HYDRATION_LOG_ENTRIES as u8 - 1) % HYDRATION_LOG_ENTRIES as u8;
        let idx = self.log_head as usize;
        self.log[idx].water_intake = (self.water_intake / 100) as u8;
        self.log[idx].date = (ts / 86400) as u16;
    }

    fn tracking_display(&self) {
        slcd::display_string("HYDRA", 0);
        if self.alert_enabled {
            slcd::set_indicator(Indicator::Signal);
        } else {
            slcd::clear_indicator(Indicator::Signal);
        }
        if self.display_deviation == 0 {
            self.display_water_ml(self.water_intake);
            let percent = (self.water_intake as u32 * 10 / self.water_goal as u32) as u8;
            let mut buf = [0u8; 2];
            buf[0] = b'0' + percent / 10;
            buf[1] = b'0' + percent % 10;
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("  "), 2);
        } else {
            let now = movement::get_local_date_time();
            let hours_since_wake = (now.hour + 24 - self.wake_hour) % 24;
            let expected = self.get_expected_intake(hours_since_wake);
            let deviation = self.water_intake as i32 - expected as i32;
            self.display_water_ml(deviation.unsigned_abs() as u16);
            let sign = if deviation >= 0 { " +" } else { " -" };
            slcd::display_string(sign, 2);
        }
    }

    fn log_find_entry(&self, direction: i8) -> u8 {
        let start = self.log_index;
        let step = if direction > 0 {
            1
        } else {
            HYDRATION_LOG_ENTRIES as u8 - 1
        };
        let mut idx = (start + step) % HYDRATION_LOG_ENTRIES as u8;
        while idx != start {
            if self.log[idx as usize].date != 0 {
                return idx;
            }
            idx = (idx + step) % HYDRATION_LOG_ENTRIES as u8;
        }
        start
    }

    fn log_display(&self) {
        slcd::display_string("LOG", 0);
        let log = &self.log[self.log_index as usize];
        if log.date == 0 {
            slcd::display_string("no dat", 4);
            return;
        }
        let distance = (self.log_index + HYDRATION_LOG_ENTRIES as u8 - self.log_head)
            % HYDRATION_LOG_ENTRIES as u8;
        let index = distance + 1;
        match self.log_type {
            LogType::Intake => {
                let mut buf = [0u8; 2];
                buf[0] = b'0' + index / 10;
                buf[1] = b'0' + index % 10;
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("  "), 2);
                self.display_water_ml(log.water_intake as u16 * 100);
            }
            LogType::Deviation => {
                let deviation = log.water_intake as i32 * 100 - self.water_goal as i32;
                let sign = if deviation >= 0 { " +" } else { " -" };
                slcd::display_string(sign, 2);
                self.display_water_ml(deviation.unsigned_abs() as u16);
            }
            LogType::Date => {
                let mut buf = [0u8; 2];
                buf[0] = b'0' + index / 10;
                buf[1] = b'0' + index % 10;
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or("  "), 2);
                let dt = utility::date_time_from_unix_time(log.date as u32 * 86400, 0);
                let mut db = [0u8; 6];
                db[0] = b'0' + dt.day / 10;
                db[1] = b'0' + dt.day % 10;
                db[2] = b'0' + dt.month / 10;
                db[3] = b'0' + dt.month % 10;
                db[4] = b'0' + (dt.year + 20) / 10;
                db[5] = b'0' + (dt.year + 20) % 10;
                slcd::display_string(core::str::from_utf8(&db[..]).unwrap_or("      "), 4);
            }
        }
    }

    fn switch_to_tracking(&mut self) {
        movement::request_tick_frequency(DISPLAY_FREQUENCY);
        self.page = Page::Tracking;
        slcd::clear_colon();
        slcd::clear_indicator(Indicator::H24);
        slcd::clear_indicator(Indicator::Pm);
        self.tracking_display();
    }

    fn switch_to_settings(&mut self) {
        movement::request_tick_frequency(4);
        self.page = Page::Settings;
        self.settings_page = Setting::WaterGlass;
        self.settings_display(0);
    }

    fn switch_to_log(&mut self) {
        movement::request_tick_frequency(DISPLAY_FREQUENCY);
        self.page = Page::Log;
        self.log_index = self.log_head;
        self.log_type = LogType::Intake;
        self.log_display();
    }

    fn check_hydration_alert(&mut self) {
        let now = movement::get_local_date_time();
        let hour = now.hour;
        if self.sleep_hour == self.wake_hour {
            return;
        } else if self.sleep_hour < self.wake_hour {
            if hour > self.sleep_hour && hour <= self.wake_hour {
                return;
            }
        } else if hour > self.sleep_hour || hour <= self.wake_hour {
            return;
        }
        let mut due = hour == self.sleep_hour;
        let hours_since_wake = (hour + 24 - self.wake_hour) % 24;
        due |= self.alert_interval > 0 && hours_since_wake % self.alert_interval == 0;
        if !due {
            return;
        }
        let expected = self.get_expected_intake(hours_since_wake);
        if self.water_intake < expected {
            movement::request_wake();
            movement::move_to_face(self.face_index);
            movement::play_alarm();
        }
    }
}

impl WatchFace for HydrationFace {
    fn setup(&mut self, _settings: &Settings, watch_face_index: usize) {
        self.face_index = watch_face_index;
    }

    fn activate(&mut self, _settings: &Settings) {
        self.switch_to_tracking();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match self.page {
            Page::Tracking => self.tracking_loop(event, settings),
            Page::Settings => self.settings_loop(event, settings),
            Page::Log => self.log_loop(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}

    fn advise(&mut self, _settings: &Settings) {
        let now = movement::get_local_date_time();
        // Daily reset at wake time and log entry.
        if now.hour == self.wake_hour && now.minute == 0 {
            let mut ts = utility::date_time_to_unix_time(now, 0);
            if self.wake_hour < self.sleep_hour {
                ts -= 24 * 60 * 60;
            }
            self.log_intake(ts);
            self.water_intake = 0;
        }
        // Check for alert at every hour.
        if self.alert_enabled && now.minute == 0 {
            self.check_hydration_alert();
        }
    }

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        let now = movement::get_local_date_time();
        self.alert_enabled && now.minute == 0
    }
}

impl HydrationFace {
    fn tracking_loop(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => {
                slcd::clear_colon();
                self.tracking_display();
            }
            Event::Tick => {
                if self.display_deviation > 0 {
                    self.display_deviation -= 1;
                }
                self.tracking_display();
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.water_intake + self.water_glass > MAX_WATER_INTAKE {
                    self.water_intake = MAX_WATER_INTAKE;
                } else {
                    self.water_intake += self.water_glass;
                }
                self.tracking_display();
                self.beep();
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.water_intake < self.water_glass {
                    self.water_intake = 0;
                } else {
                    self.water_intake -= self.water_glass;
                }
                self.tracking_display();
                self.beep();
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.switch_to_settings();
                self.beep();
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.display_deviation = 2;
                self.tracking_display();
            }
            Event::Button(Button::Alarm, ButtonEvent::ReallyLongPress) => {
                self.display_deviation = 0;
                self.switch_to_log();
                self.beep();
            }
            Event::BackgroundTask => self.check_hydration_alert(),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn settings_loop(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => self.settings_display(0),
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.settings_page = match self.settings_page {
                    Setting::WaterGlass => Setting::WaterGoal,
                    Setting::WaterGoal => Setting::WakeTime,
                    Setting::WakeTime => Setting::SleepTime,
                    Setting::SleepTime => Setting::AlertInterval,
                    Setting::AlertInterval => Setting::WaterGlass,
                };
                self.settings_display(0);
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                self.switch_to_tracking();
                self.beep();
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.settings_advance();
                self.settings_display(0);
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                match self.settings_page {
                    Setting::WaterGlass => self.water_glass = DEFAULT_WATER_GLASS_ML,
                    Setting::WaterGoal => self.water_goal = DEFAULT_WATER_GOAL_ML,
                    Setting::WakeTime => self.wake_hour = DEFAULT_WAKE_HOUR,
                    Setting::SleepTime => self.sleep_hour = DEFAULT_SLEEP_HOUR,
                    Setting::AlertInterval => self.alert_interval = DEFAULT_ALERT_INTERVAL,
                }
                self.settings_display(0);
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.alert_enabled = !self.alert_enabled;
                self.settings_display(0);
                self.beep();
            }
            Event::BackgroundTask => self.check_hydration_alert(),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn log_loop(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => self.log_display(),
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.log_index = self.log_find_entry(1);
                self.log_display();
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.log_type = match self.log_type {
                    LogType::Intake => LogType::Date,
                    LogType::Date => LogType::Deviation,
                    LogType::Deviation => LogType::Intake,
                };
                self.log_display();
                self.beep();
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                self.switch_to_tracking();
                self.beep();
            }
            Event::BackgroundTask => self.check_hydration_alert(),
            _ => movement::default_loop_handler(event, settings),
        }
    }
}
