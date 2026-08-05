//! Activity tracker watch face.
//!
//! Port of the C `activity_face.c`. Logs activities (running, biking, etc.)
//! with start time and duration. It is a pure state machine: it reacts to a
//! single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const MAX_ACTIVITY_SECONDS: u16 = 28800;
const ACTIVITY_LOG_SZ: usize = 99;
const ACTIVITY_MIN_LENGTH_SEC: u16 = 60;

const ACTIVITY_NAMES: [&str; 14] = [
    " bIKE ", "uuaLK ", "  rUn ", "DAnCE ", " yOgA ", "CrOSS ", "Suuinn", "ELLIP ", "  gYnn",
    "  rOuu", "SOCCEr", " FOOTb", " bALL ", "  SKI ",
];

const ENABLED_ACTIVITIES: [u8; 14] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
const NUM_ENABLED_ACTIVITIES: u8 = 14;

const ACTM_CHOOSE: u8 = 0;
const ACTM_LOGGING: u8 = 1;
const ACTM_PAUSED: u8 = 2;
const ACTM_DONE: u8 = 3;
const ACTM_LOGSIZE: u8 = 4;
const ACTM_CHIRP: u8 = 5;
const ACTM_CHIRPING: u8 = 6;
const ACTM_CLEAR: u8 = 7;
const ACTM_CLEAR_CONFIRM: u8 = 8;
const ACTM_CLEAR_DONE: u8 = 9;

/// A logged activity.
#[derive(Clone, Copy)]
struct ActivityItem {
    start_time: rtc::DateTime,
    total_sec: u16,
    pause_sec: u16,
    activity_type: u8,
}

/// The activity animation pixels.
const ACTIVITY_ANIM_PIXELS: [(u8, u8); 6] = [(1, 4), (0, 5), (0, 6), (1, 6), (2, 5), (2, 4)];

/// The activity face state.
pub struct ActivityFace {
    mode: u8,
    type_ix: u8,
    counter: u16,
    start_time: rtc::DateTime,
    curr_total_sec: u16,
    curr_pause_sec: u16,
    le_state: u8,
    log: [ActivityItem; ACTIVITY_LOG_SZ],
    log_count: u8,
}

impl ActivityFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        ActivityFace {
            mode: ACTM_CHOOSE,
            type_ix: 0,
            counter: 0,
            start_time: rtc::DateTime {
                second: 0,
                minute: 0,
                hour: 0,
                day: 0,
                month: 0,
                year: 0,
            },
            curr_total_sec: 0,
            curr_pause_sec: 0,
            le_state: 0,
            log: [ActivityItem {
                start_time: rtc::DateTime {
                    second: 0,
                    minute: 0,
                    hour: 0,
                    day: 0,
                    month: 0,
                    year: 0,
                },
                total_sec: 0,
                pause_sec: 0,
                activity_type: 0,
            }; ACTIVITY_LOG_SZ],
            log_count: 0,
        }
    }

    pub fn new() -> Self {
        ActivityFace::new_static()
    }

    fn display_choice(&self) {
        watch::slcd::display_string("AC", 0);
        if self.log_count as usize >= ACTIVITY_LOG_SZ {
            watch::slcd::display_string(" FULL ", 4);
        } else {
            let ix = ENABLED_ACTIVITIES[self.type_ix as usize];
            watch::slcd::display_string(ACTIVITY_NAMES[ix as usize], 4);
        }
    }

    fn update_logging_screen(&self, settings: &Settings) {
        watch::slcd::display_string("AC  ", 0);
        if self.le_state == 1 {
            let now = rtc::get_date_time();
            let now_ts = utility::date_time_to_unix_time(now, 0);
            let start_ts = utility::date_time_to_unix_time(self.start_time, 0);
            let total_seconds = now_ts - start_ts;
            let duration = utility::seconds_to_duration(total_seconds);
            let mut buf = [0u8; 11];
            buf[0] = b' ';
            buf[1] = b'0' + duration.hours / 10;
            buf[2] = b'0' + duration.hours % 10;
            buf[3] = b'0' + duration.minutes / 10;
            buf[4] = b'0' + duration.minutes % 10;
            buf[5] = b' ';
            buf[6] = b' ';
            watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
            watch::slcd::set_colon();
            watch::slcd::set_indicator(Indicator::Lap);
            watch::slcd::clear_indicator(Indicator::Pm);
            watch::slcd::clear_indicator(Indicator::H24);
            return;
        }

        if (self.counter % 5) < 3 {
            watch::slcd::set_indicator(Indicator::Lap);
            watch::slcd::clear_indicator(Indicator::Pm);
            watch::slcd::clear_indicator(Indicator::H24);
            if self.mode == ACTM_PAUSED {
                watch::slcd::display_string(" PAUSE", 4);
                watch::slcd::clear_colon();
            } else {
                let duration = utility::seconds_to_duration(self.curr_total_sec as u32);
                let mut buf = [0u8; 11];
                if self.curr_total_sec < 600 {
                    buf[0] = b' ';
                    buf[1] = b' ';
                    buf[2] = b' ';
                    buf[3] = b'0' + duration.minutes;
                    buf[4] = b'0' + duration.seconds / 10;
                    buf[5] = b'0' + duration.seconds % 10;
                    watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
                    watch::slcd::clear_colon();
                } else if self.curr_total_sec < 3600 {
                    buf[0] = b' ';
                    buf[1] = b' ';
                    buf[2] = b'0' + duration.minutes / 10;
                    buf[3] = b'0' + duration.minutes % 10;
                    buf[4] = b'0' + duration.seconds / 10;
                    buf[5] = b'0' + duration.seconds % 10;
                    watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
                    watch::slcd::clear_colon();
                } else {
                    buf[0] = b' ';
                    buf[1] = b'0' + duration.hours;
                    buf[2] = b'0' + duration.minutes / 10;
                    buf[3] = b'0' + duration.minutes % 10;
                    buf[4] = b'0' + duration.seconds / 10;
                    buf[5] = b'0' + duration.seconds % 10;
                    watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
                    watch::slcd::set_colon();
                }
            }
        } else {
            watch::slcd::clear_indicator(Indicator::Lap);
            let now = rtc::get_date_time();
            let mut hour = now.hour;
            let mut set_leading_zero = false;
            if !settings.clock_mode_24h() {
                watch::slcd::clear_indicator(Indicator::H24);
                if hour < 12 {
                    watch::slcd::clear_indicator(Indicator::Pm);
                } else {
                    watch::slcd::set_indicator(Indicator::Pm);
                }
                hour %= 12;
                if hour == 0 {
                    hour = 12;
                }
            } else {
                watch::slcd::clear_indicator(Indicator::Pm);
                if !settings.clock_24h_leading_zero() {
                    watch::slcd::set_indicator(Indicator::H24);
                } else if hour < 10 {
                    set_leading_zero = true;
                }
            }
            let mut buf = [0u8; 11];
            buf[0] = b'0' + hour / 10;
            buf[1] = b'0' + hour % 10;
            buf[2] = b'0' + now.minute / 10;
            buf[3] = b'0' + now.minute % 10;
            buf[4] = b' ';
            buf[5] = b' ';
            watch::slcd::set_colon();
            watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
            if set_leading_zero {
                watch::slcd::display_string("0", 4);
            }
        }
    }

    fn finish_logging(&mut self) {
        if self.curr_total_sec >= ACTIVITY_MIN_LENGTH_SEC
            && self.log_count as usize + 1 < ACTIVITY_LOG_SZ
        {
            let itm = &mut self.log[self.log_count as usize];
            itm.start_time = self.start_time;
            itm.total_sec = self.curr_total_sec;
            itm.pause_sec = self.curr_pause_sec;
            itm.activity_type = self.type_ix;
            self.log_count += 1;
        }
        self.mode = ACTM_DONE;
        watch::slcd::clear_indicator(Indicator::Lap);
        self.counter = 6;
        watch::slcd::clear_display();
        watch::slcd::display_string("AC   dONE ", 0);
    }

    fn handle_tick(&mut self, settings: &Settings) {
        if self.mode == ACTM_LOGGING || self.mode == ACTM_PAUSED {
            self.counter += 1;
            self.curr_total_sec += 1;
            if self.mode == ACTM_PAUSED {
                self.curr_pause_sec += 1;
            }
            if self.curr_total_sec == MAX_ACTIVITY_SECONDS {
                self.finish_logging();
            } else {
                self.update_logging_screen(settings);
            }
        } else if self.mode == ACTM_DONE {
            if self.counter == 0 {
                movement::move_to_face(0);
            } else {
                let cd = self.counter % 6;
                let (x, y) = ACTIVITY_ANIM_PIXELS[cd as usize];
                watch::slcd::clear_pixel(x, y);
                self.counter -= 1;
                let cd = self.counter % 6;
                let (x, y) = ACTIVITY_ANIM_PIXELS[cd as usize];
                watch::slcd::set_pixel(x, y);
            }
        } else if self.mode == ACTM_LOGSIZE || self.mode == ACTM_CHIRP || self.mode == ACTM_CLEAR {
            self.counter += 1;
            let mut timeout = 20;
            if self.mode == ACTM_CLEAR {
                timeout = 10;
            } else if self.mode == ACTM_CHIRP {
                timeout = 120;
            }
            if self.counter > timeout {
                self.mode = ACTM_CHOOSE;
                self.display_choice();
            }
        } else if self.mode == ACTM_CLEAR_CONFIRM {
            self.counter += 1;
            if self.counter % 2 == 0 {
                watch::slcd::display_string("CLEAR ", 4);
            } else {
                watch::slcd::display_string("      ", 4);
            }
            if self.counter > 12 {
                self.mode = ACTM_CHOOSE;
                self.display_choice();
            }
        } else if self.mode == ACTM_CLEAR_DONE {
            self.counter += 1;
            if self.counter == 7 {
                self.mode = ACTM_CHOOSE;
                self.display_choice();
                return;
            }
            let mut buf = [b' '; 7];
            let mut n_zeros = self.counter + 1;
            if n_zeros > 6 {
                n_zeros = 6;
            }
            for i in 0..n_zeros {
                buf[i as usize] = b'0';
            }
            watch::slcd::display_string(core::str::from_utf8(&buf[..6]).unwrap_or("      "), 4);
        }
    }

    fn alarm_long(&mut self, settings: &Settings) {
        if self.mode == ACTM_CHOOSE {
            if self.log_count as usize >= ACTIVITY_LOG_SZ {
                return;
            }
            self.start_time = rtc::get_date_time();
            self.curr_total_sec = 0;
            self.curr_pause_sec = 0;
            self.counter = 0;
            self.mode = ACTM_LOGGING;
            watch::slcd::set_indicator(Indicator::Lap);
            self.update_logging_screen(settings);
        } else if self.mode == ACTM_LOGGING || self.mode == ACTM_PAUSED {
            self.finish_logging();
        } else if self.mode == ACTM_CLEAR {
            if self.log_count == 0 {
                return;
            }
            self.mode = ACTM_CLEAR_CONFIRM;
            self.counter = 0;
        } else if self.mode == ACTM_CLEAR_CONFIRM {
            self.log = [ActivityItem {
                start_time: rtc::DateTime {
                    second: 0,
                    minute: 0,
                    hour: 0,
                    day: 0,
                    month: 0,
                    year: 0,
                },
                total_sec: 0,
                pause_sec: 0,
                activity_type: 0,
            }; ACTIVITY_LOG_SZ];
            self.log_count = 0;
            self.mode = ACTM_CLEAR_DONE;
            self.counter = 0;
            watch::slcd::display_string("0     ", 4);
        }
    }

    fn alarm_short(&mut self, settings: &Settings) {
        if self.mode == ACTM_CHOOSE {
            self.type_ix = (self.type_ix + 1) % NUM_ENABLED_ACTIVITIES;
            self.display_choice();
        } else if self.mode == ACTM_LOGGING {
            self.mode = ACTM_PAUSED;
            self.counter = 0;
            self.update_logging_screen(settings);
        } else if self.mode == ACTM_PAUSED {
            self.mode = ACTM_LOGGING;
            self.counter = 0;
            self.update_logging_screen(settings);
        }
    }

    fn light_short(&mut self) {
        if self.mode == ACTM_CHOOSE {
            self.mode = ACTM_LOGSIZE;
            self.counter = 0;
            let mut buf = [0u8; 11];
            buf[0] = b'A';
            buf[1] = b'C';
            buf[2] = b' ';
            buf[3] = b' ';
            buf[4] = b'L';
            buf[5] = b'#';
            buf[6] = b'g';
            buf[7] = b'0' + self.log_count / 100;
            buf[8] = b'0' + (self.log_count / 10) % 10;
            buf[9] = b'0' + self.log_count % 10;
            watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        } else if self.mode == ACTM_LOGSIZE {
            self.mode = ACTM_CHIRP;
            self.counter = 0;
            watch::slcd::display_string("AC  CHIRP ", 0);
        } else if self.mode == ACTM_CHIRP {
            self.mode = ACTM_CLEAR;
            self.counter = 0;
            watch::slcd::display_string("AC  CLEAR ", 0);
        } else if self.mode == ACTM_CLEAR || self.mode == ACTM_CLEAR_CONFIRM {
            self.mode = ACTM_CHOOSE;
            self.display_choice();
        } else if self.mode == ACTM_LOGGING || self.mode == ACTM_PAUSED {
            movement::illuminate_led();
        }
    }
}

impl WatchFace for ActivityFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Activate => {
                if self.le_state != 0 && self.mode == ACTM_LOGGING {
                    self.le_state = 2;
                    let now = rtc::get_date_time();
                    let now_ts = utility::date_time_to_unix_time(now, 0);
                    let start_ts = utility::date_time_to_unix_time(self.start_time, 0);
                    self.curr_total_sec = (now_ts - start_ts) as u16;
                    self.update_logging_screen(settings);
                } else {
                    self.le_state = 0;
                    self.mode = ACTM_CHOOSE;
                    self.type_ix = 0;
                    self.display_choice();
                }
            }
            Event::Tick => self.handle_tick(settings),
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                if self.mode != ACTM_LOGGING
                    && self.mode != ACTM_PAUSED
                    && self.mode != ACTM_CHIRPING
                {
                    movement::move_to_next_face();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => self.light_short(),
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.le_state != 2 {
                    self.alarm_short(settings);
                } else {
                    self.le_state = 0;
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => self.alarm_long(settings),
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
