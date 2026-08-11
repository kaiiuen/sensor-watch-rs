//! Interval timer watch face.
//!
//! Port of the C `interval_face.c`. A configurable interval timer with warmup,
//! work, break, and cooldown phases. It is a pure state machine: it reacts to
//! a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const INTERVAL_TIMERS: u8 = 6;

const INTERVAL_FACE_STATE_DEFAULT: &str = "IT";
const INTERVAL_FACE_STATE_WARMUP: &str = "PR";
const INTERVAL_FACE_STATE_WORK: &str = "WO";
const INTERVAL_FACE_STATE_BREAK: &str = "BR";
const INTERVAL_FACE_STATE_COOLDOWN: &str = "CD";

/// Default timers: warmup, work, break, full rounds, cooldown.
const DEFAULT_TIMERS: [[i8; 5]; 6] = [
    [0, 40, 20, 0, 0],
    [0, 45, 15, 0, 0],
    [10, 20, 10, 8, 10],
    [0, 35, 0, 0, 0],
    [0, -25, -5, 0, 0],
    [0, -20, -5, 0, 0],
];

const INTRO_SEGDATA: [(u8, u8); 4] = [(1, 8), (0, 8), (0, 7), (1, 7)];
const BLINK_IDX: [u8; 12] = [3, 9, 4, 6, 4, 6, 8, 4, 6, 8, 4, 6];
const SETTING_PAGE_IDX: [u8; 12] = [1, 0, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4];

const INTERVAL_SETTING_0_TIMER_IDX: u8 = 0;
const INTERVAL_SETTING_1_CLEAR_YN: u8 = 1;
const INTERVAL_SETTING_2_WARMUP_MINUTES: u8 = 2;
const INTERVAL_SETTING_3_WARMUP_SECONDS: u8 = 3;
const INTERVAL_SETTING_4_WORK_MINUTES: u8 = 4;
const INTERVAL_SETTING_5_WORK_SECONDS: u8 = 5;
const INTERVAL_SETTING_6_WORK_ROUNDS: u8 = 6;
const INTERVAL_SETTING_7_BREAK_MINUTES: u8 = 7;
const INTERVAL_SETTING_8_BREAK_SECONDS: u8 = 8;
const INTERVAL_SETTING_9_FULL_ROUNDS: u8 = 9;
const INTERVAL_SETTING_10_COOLDOWN_MINUTES: u8 = 10;
const INTERVAL_SETTING_11_COOLDOWN_SECONDS: u8 = 11;
const INTERVAL_SETTING_MAX: u8 = 12;

/// A timer setting.
#[derive(Clone, Copy)]
struct IntervalTimerSetting {
    warmup_minutes: u8,
    warmup_seconds: u8,
    work_minutes: u8,
    work_seconds: u8,
    work_rounds: u8,
    break_minutes: u8,
    break_seconds: u8,
    full_rounds: u8,
    cooldown_minutes: u8,
    cooldown_seconds: u8,
}

impl IntervalTimerSetting {
    const fn zero() -> Self {
        IntervalTimerSetting {
            warmup_minutes: 0,
            warmup_seconds: 0,
            work_minutes: 0,
            work_seconds: 0,
            work_rounds: 0,
            break_minutes: 0,
            break_seconds: 0,
            full_rounds: 0,
            cooldown_minutes: 0,
            cooldown_seconds: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.warmup_minutes
            + self.warmup_seconds
            + self.work_minutes
            + self.work_seconds
            + self.break_minutes
            + self.break_seconds
            + self.cooldown_minutes
            + self.cooldown_seconds
            == 0
    }
}

/// The face state.
#[derive(Clone, Copy, PartialEq, Eq)]
enum IntervalFaceState {
    Intro,
    Waiting,
    Setting,
    Running,
    Pausing,
}

/// The interval face state.
pub struct IntervalFace {
    face_idx: usize,
    face_state: IntervalFaceState,
    is_active: bool,
    timer_idx: u8,
    timers: [IntervalTimerSetting; INTERVAL_TIMERS as usize],
    setting_idx: u8,
    ticks: i8,
    erase_timer_flag: bool,
    target_ts: u32,
    now_ts: u32,
    paused_ts: u32,
    timer_work_round: u8,
    timer_full_round: u8,
    timer_run_state: u8,
}

impl IntervalFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        IntervalFace {
            face_idx: 0,
            face_state: IntervalFaceState::Waiting,
            is_active: false,
            timer_idx: 0,
            timers: [IntervalTimerSetting::zero(); INTERVAL_TIMERS as usize],
            setting_idx: 0,
            ticks: 0,
            erase_timer_flag: false,
            target_ts: 0,
            now_ts: 0,
            paused_ts: 0,
            timer_work_round: 0,
            timer_full_round: 0,
            timer_run_state: 0,
        }
    }

    pub fn new() -> Self {
        IntervalFace::new_static()
    }

    fn get_now_ts() -> u32 {
        utility::date_time_to_unix_time(rtc::get_date_time(), 0)
    }

    fn button_beep(&self, settings: &Settings) {
        if settings.button_should_sound() {
            crate::movement::play_alarm_beeps(1, Note::C7);
        }
    }

    fn timer_write_info(&self, buf: &mut [u8; 11], timer_page: u8) {
        let t = &self.timers[self.timer_idx as usize];
        match timer_page {
            0 => {
                let d = INTERVAL_FACE_STATE_DEFAULT.as_bytes();
                buf[0] = d[0];
                buf[1] = d[1];
                buf[2] = b' ';
                buf[3] = b'0' + self.timer_idx + 1;
                buf[4] = b'C';
                buf[5] = b'L';
                buf[6] = b'E';
                buf[7] = b'A';
                buf[8] = b'R';
                buf[9] = if self.erase_timer_flag { b'y' } else { b'n' };
                watch::slcd::clear_colon();
            }
            1 => {
                let d = INTERVAL_FACE_STATE_WARMUP.as_bytes();
                buf[0] = d[0];
                buf[1] = d[1];
                buf[2] = b' ';
                buf[3] = b'0' + self.timer_idx + 1;
                buf[4] = b'0' + t.warmup_minutes / 10;
                buf[5] = b'0' + t.warmup_minutes % 10;
                buf[6] = b'0' + t.warmup_seconds / 10;
                buf[7] = b'0' + t.warmup_seconds % 10;
                buf[8] = b' ';
                buf[9] = b' ';
            }
            2 => {
                let d = INTERVAL_FACE_STATE_WORK.as_bytes();
                buf[0] = d[0];
                buf[1] = d[1];
                buf[2] = b' ';
                buf[3] = b'0' + self.timer_idx + 1;
                buf[4] = b'0' + t.work_minutes / 10;
                buf[5] = b'0' + t.work_minutes % 10;
                buf[6] = b'0' + t.work_seconds / 10;
                buf[7] = b'0' + t.work_seconds % 10;
                buf[8] = b'0' + t.work_rounds / 10;
                buf[9] = b'0' + t.work_rounds % 10;
            }
            3 => {
                let d = INTERVAL_FACE_STATE_BREAK.as_bytes();
                buf[0] = d[0];
                buf[1] = d[1];
                buf[2] = b' ';
                buf[3] = b'0' + self.timer_idx + 1;
                buf[4] = b'0' + t.break_minutes / 10;
                buf[5] = b'0' + t.break_minutes % 10;
                buf[6] = b'0' + t.break_seconds / 10;
                buf[7] = b'0' + t.break_seconds % 10;
                buf[8] = b'0' + t.full_rounds / 10;
                buf[9] = b'0' + t.full_rounds % 10;
                if t.full_rounds == 0 {
                    buf[9] = b'-';
                }
            }
            _ => {
                let d = INTERVAL_FACE_STATE_COOLDOWN.as_bytes();
                buf[0] = d[0];
                buf[1] = d[1];
                buf[2] = b' ';
                buf[3] = b'0' + self.timer_idx + 1;
                buf[4] = b'0' + t.cooldown_minutes / 10;
                buf[5] = b'0' + t.cooldown_minutes % 10;
                buf[6] = b'0' + t.cooldown_seconds / 10;
                buf[7] = b'0' + t.cooldown_seconds % 10;
                buf[8] = b' ';
                buf[9] = b' ';
            }
        }
    }

    fn face_draw(&self, subsecond: u8) {
        if !self.is_active {
            return;
        }
        let mut buf = [0u8; 11];
        let tmp: u8;
        if self.face_state == IntervalFaceState::Waiting && self.ticks >= 0 {
            let mut ticks = self.ticks % 12;
            if ticks == 0 {
                let t = &self.timers[self.timer_idx as usize];
                if t.warmup_minutes + t.warmup_seconds == 0 {
                    ticks = 3;
                }
            }
            tmp = (ticks / 3 + 1) as u8;
            self.timer_write_info(&mut buf, tmp);
            if tmp == 2 && self.timers[self.timer_idx as usize].work_rounds == 1 {
                buf[9] = b' ';
            }
            if subsecond % 2 == 0 && self.ticks < 24 {
                watch::slcd::clear_colon();
            } else {
                watch::slcd::set_colon();
            }
        } else if self.face_state == IntervalFaceState::Setting {
            if self.setting_idx == INTERVAL_SETTING_0_TIMER_IDX {
                let t = &self.timers[self.timer_idx as usize];
                tmp = if t.warmup_minutes + t.warmup_seconds == 0 {
                    1
                } else {
                    2
                };
            } else {
                tmp = SETTING_PAGE_IDX[self.setting_idx as usize];
            }
            self.timer_write_info(&mut buf, tmp);
            if subsecond % 2 == 1 && self.ticks != -2 {
                let idx = BLINK_IDX[self.setting_idx as usize] as usize;
                buf[idx] = b' ';
                if idx % 2 == 0 {
                    buf[idx + 1] = b' ';
                }
            }
            if self.setting_idx == INTERVAL_SETTING_6_WORK_ROUNDS
                || self.setting_idx == INTERVAL_SETTING_9_FULL_ROUNDS
            {
                watch::slcd::set_indicator(Indicator::Lap);
            } else {
                watch::slcd::clear_indicator(Indicator::Lap);
            }
        } else if self.face_state == IntervalFaceState::Running
            || self.face_state == IntervalFaceState::Pausing
        {
            let mut tmp2 = self.timer_full_round;
            match self.timer_run_state {
                0 => {
                    let d = INTERVAL_FACE_STATE_WARMUP.as_bytes();
                    buf[0] = d[0];
                    buf[1] = d[1];
                }
                1 => {
                    let d = INTERVAL_FACE_STATE_WORK.as_bytes();
                    buf[0] = d[0];
                    buf[1] = d[1];
                    if self.timers[self.timer_idx as usize].work_rounds > 1 {
                        tmp2 = self.timer_work_round;
                    }
                }
                2 => {
                    let d = INTERVAL_FACE_STATE_BREAK.as_bytes();
                    buf[0] = d[0];
                    buf[1] = d[1];
                }
                _ => {
                    let d = INTERVAL_FACE_STATE_COOLDOWN.as_bytes();
                    buf[0] = d[0];
                    buf[1] = d[1];
                }
            }
            let delta = if self.face_state == IntervalFaceState::Pausing {
                self.target_ts - self.paused_ts
            } else {
                self.target_ts - self.now_ts
            };
            if self.face_state == IntervalFaceState::Pausing {
                if self.now_ts % 2 == 1 {
                    watch::slcd::set_indicator(Indicator::Bell);
                } else {
                    watch::slcd::clear_indicator(Indicator::Bell);
                }
            }
            let mins = delta / 60;
            let secs = delta % 60;
            buf[2] = b' ';
            buf[3] = b'0' + self.timer_idx + 1;
            buf[4] = b'0' + (mins / 10) as u8;
            buf[5] = b'0' + (mins % 10) as u8;
            buf[6] = b'0' + (secs / 10) as u8;
            buf[7] = b'0' + (secs % 10) as u8;
            buf[8] = b'0' + (tmp2 + 1) / 10;
            buf[9] = b'0' + (tmp2 + 1) % 10;
        }
        if buf[0] != 0 {
            watch::slcd::display_character(buf[0], 0);
            watch::slcd::display_character(buf[1], 1);
            watch::slcd::set_pixel(2, 9);
            watch::slcd::display_string(core::str::from_utf8(&buf[3..]).unwrap_or(""), 3);
        }
    }

    fn initiate_setting(&mut self, subsecond: u8) {
        self.face_state = IntervalFaceState::Setting;
        self.setting_idx = INTERVAL_SETTING_0_TIMER_IDX;
        self.ticks = 0;
        self.erase_timer_flag = false;
        watch::slcd::set_colon();
        self.face_draw(subsecond);
    }

    fn resume_setting(&mut self, subsecond: u8) {
        self.face_state = IntervalFaceState::Waiting;
        self.ticks = 0;
        self.face_draw(subsecond);
        watch::slcd::clear_indicator(Indicator::Lap);
    }

    fn abort_quick_ticks(&mut self) {
        if self.ticks == -2 {
            self.ticks = -1;
        }
    }

    fn handle_alarm_button(&mut self) {
        let t = &mut self.timers[self.timer_idx as usize];
        match self.setting_idx {
            INTERVAL_SETTING_0_TIMER_IDX => {
                self.timer_idx = (self.timer_idx + 1) % INTERVAL_TIMERS;
                self.erase_timer_flag = false;
            }
            INTERVAL_SETTING_1_CLEAR_YN => self.erase_timer_flag = !self.erase_timer_flag,
            INTERVAL_SETTING_2_WARMUP_MINUTES => t.warmup_minutes = (t.warmup_minutes + 1) % 60,
            INTERVAL_SETTING_3_WARMUP_SECONDS => t.warmup_seconds = (t.warmup_seconds + 5) % 60,
            INTERVAL_SETTING_4_WORK_MINUTES => {
                t.work_minutes = (t.work_minutes + 1) % 60;
                if t.work_rounds == 0 {
                    t.work_rounds = 1;
                }
            }
            INTERVAL_SETTING_5_WORK_SECONDS => {
                t.work_seconds = (t.work_seconds + 5) % 60;
                if t.work_rounds == 0 {
                    t.work_rounds = 1;
                }
            }
            INTERVAL_SETTING_6_WORK_ROUNDS => t.work_rounds = (t.work_rounds + 1) % 100,
            INTERVAL_SETTING_7_BREAK_MINUTES => t.break_minutes = (t.break_minutes + 1) % 60,
            INTERVAL_SETTING_8_BREAK_SECONDS => t.break_seconds = (t.break_seconds + 5) % 60,
            INTERVAL_SETTING_9_FULL_ROUNDS => t.full_rounds = (t.full_rounds + 1) % 100,
            INTERVAL_SETTING_10_COOLDOWN_MINUTES => {
                t.cooldown_minutes = (t.cooldown_minutes + 1) % 60
            }
            INTERVAL_SETTING_11_COOLDOWN_SECONDS => {
                t.cooldown_seconds = (t.cooldown_seconds + 5) % 60
            }
            _ => {}
        }
    }

    fn set_next_timestamp(&mut self) {
        let t = self.timers[self.timer_idx as usize];
        let delta = match self.timer_run_state {
            0 => t.warmup_minutes as u32 * 60 + t.warmup_seconds as u32,
            1 => t.work_minutes as u32 * 60 + t.work_seconds as u32,
            2 => t.break_minutes as u32 * 60 + t.break_seconds as u32,
            _ => t.cooldown_minutes as u32 * 60 + t.cooldown_seconds as u32,
        };
        let delta = if delta == 0 { 1 } else { delta };
        self.target_ts += delta;
        let target_dt = utility::date_time_from_unix_time(self.target_ts, 0);
        movement::schedule_background_task_for_face(self.face_idx, target_dt);
        crate::movement::play_alarm_beeps(1, Note::F6);
    }

    fn init_timer_info(&mut self) {
        self.face_state = IntervalFaceState::Waiting;
        self.ticks = 0;
    }

    fn abort_running_timer(&mut self) {
        self.timer_work_round = 0;
        self.timer_full_round = 0;
        self.timer_run_state = 0;
        movement::cancel_background_task_for_face(self.face_idx);
        watch::slcd::clear_indicator(Indicator::Bell);
        crate::movement::play_alarm_beeps(1, Note::C8);
    }

    fn resume_paused_timer(&mut self) {
        self.now_ts = Self::get_now_ts();
        self.target_ts += self.now_ts - self.paused_ts;
        let target_dt = utility::date_time_from_unix_time(self.target_ts, 0);
        movement::schedule_background_task_for_face(self.face_idx, target_dt);
        self.face_state = IntervalFaceState::Running;
        watch::slcd::set_indicator(Indicator::Bell);
    }
}

impl WatchFace for IntervalFace {
    fn setup(&mut self, _settings: &Settings, watch_face_index: usize) {
        self.face_idx = watch_face_index;
        self.face_state = IntervalFaceState::Waiting;
        for i in 0..INTERVAL_TIMERS as usize {
            self.timers[i].work_rounds = 1;
        }
        for i in 0..6 {
            self.timers[i].warmup_seconds = DEFAULT_TIMERS[i][0] as u8;
            if DEFAULT_TIMERS[i][1] < 0 {
                self.timers[i].work_minutes = (-DEFAULT_TIMERS[i][1]) as u8;
            } else {
                self.timers[i].work_seconds = DEFAULT_TIMERS[i][1] as u8;
            }
            self.timers[i].work_rounds = 1;
            if DEFAULT_TIMERS[i][2] < 0 {
                self.timers[i].break_minutes = (-DEFAULT_TIMERS[i][2]) as u8;
            } else {
                self.timers[i].break_seconds = DEFAULT_TIMERS[i][2] as u8;
            }
            self.timers[i].full_rounds = DEFAULT_TIMERS[i][3] as u8;
            self.timers[i].cooldown_seconds = DEFAULT_TIMERS[i][4] as u8;
        }
    }

    fn activate(&mut self, _settings: &Settings) {
        self.erase_timer_flag = false;
        self.is_active = true;
        if self.face_state == IntervalFaceState::Waiting {
            self.face_state = IntervalFaceState::Intro;
            self.ticks = 0;
        } else {
            watch::slcd::set_colon();
        }
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Tick => {
                if self.face_state == IntervalFaceState::Intro {
                    if self.ticks == 4 {
                        self.face_state = IntervalFaceState::Waiting;
                        watch::slcd::set_colon();
                        self.init_timer_info();
                        self.face_draw(0);
                    } else {
                        let (x, y) = INTRO_SEGDATA[self.ticks as usize];
                        watch::slcd::set_pixel(x, y);
                        self.ticks += 1;
                    }
                } else if self.face_state == IntervalFaceState::Waiting && self.ticks >= 0 {
                    self.ticks += 1;
                    if (self.ticks % 12 == 9)
                        && self.timers[self.timer_idx as usize].cooldown_minutes
                            + self.timers[self.timer_idx as usize].cooldown_seconds
                            == 0
                    {
                        self.ticks += 3;
                    }
                    if self.ticks > 24 {
                        self.ticks = -1;
                    } else {
                        self.face_draw(0);
                    }
                } else if self.face_state == IntervalFaceState::Setting {
                    if self.ticks == -2 {
                        self.handle_alarm_button();
                    }
                    self.face_draw(0);
                } else if self.face_state == IntervalFaceState::Running
                    || self.face_state == IntervalFaceState::Pausing
                {
                    self.now_ts = Self::get_now_ts();
                    self.face_draw(0);
                }
            }
            Event::Activate => {
                watch::slcd::display_string(INTERVAL_FACE_STATE_DEFAULT, 0);
                if self.face_state != IntervalFaceState::Waiting {
                    self.face_draw(0);
                }
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.face_state == IntervalFaceState::Setting {
                    if self.setting_idx == INTERVAL_SETTING_0_TIMER_IDX {
                        if self.timers[self.timer_idx as usize].is_empty() {
                            self.setting_idx = INTERVAL_SETTING_1_CLEAR_YN;
                        }
                    } else if self.setting_idx == INTERVAL_SETTING_1_CLEAR_YN {
                        watch::slcd::set_colon();
                        if self.erase_timer_flag {
                            self.timers[self.timer_idx as usize] = IntervalTimerSetting::zero();
                            crate::movement::play_alarm_beeps(1, Note::C8);
                        }
                    } else if self.setting_idx == INTERVAL_SETTING_9_FULL_ROUNDS
                        && self.timers[self.timer_idx as usize].full_rounds == 0
                    {
                        self.setting_idx = INTERVAL_SETTING_11_COOLDOWN_SECONDS;
                    }
                    self.setting_idx += 1;
                    if self.setting_idx == INTERVAL_SETTING_MAX {
                        self.resume_setting(0);
                    } else {
                        self.face_draw(0);
                    }
                } else {
                    movement::illuminate_led();
                }
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.button_beep(settings);
                if self.face_state == IntervalFaceState::Setting {
                    self.resume_setting(0);
                } else {
                    if self.face_state == IntervalFaceState::Running
                        || self.face_state == IntervalFaceState::Pausing
                    {
                        self.abort_running_timer();
                    }
                    self.initiate_setting(0);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => match self.face_state {
                IntervalFaceState::Waiting => {
                    self.timer_idx = (self.timer_idx + 1) % INTERVAL_TIMERS;
                    self.ticks = 0;
                    self.face_draw(0);
                }
                IntervalFaceState::Setting => {
                    self.abort_quick_ticks();
                    self.handle_alarm_button();
                }
                IntervalFaceState::Running => {
                    self.button_beep(settings);
                    self.paused_ts = Self::get_now_ts();
                    self.face_state = IntervalFaceState::Pausing;
                    movement::cancel_background_task_for_face(self.face_idx);
                    self.face_draw(0);
                }
                IntervalFaceState::Pausing => {
                    self.button_beep(settings);
                    self.resume_paused_timer();
                    self.face_draw(0);
                }
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.face_state == IntervalFaceState::Setting
                    && self.setting_idx != INTERVAL_SETTING_1_CLEAR_YN
                {
                    self.ticks = -2;
                } else if self.face_state == IntervalFaceState::Waiting {
                    let t = &self.timers[self.timer_idx as usize];
                    if t.is_empty() {
                        self.button_beep(settings);
                        self.timer_idx = 0;
                        self.init_timer_info();
                    } else {
                        self.timer_work_round = 0;
                        self.timer_full_round = 0;
                        self.timer_run_state = if t.warmup_minutes + t.warmup_seconds != 0 {
                            0
                        } else if t.work_minutes + t.work_seconds != 0 {
                            1
                        } else if t.break_minutes + t.break_seconds != 0 {
                            2
                        } else {
                            3
                        };
                        self.now_ts = Self::get_now_ts();
                        self.target_ts = self.now_ts;
                        self.set_next_timestamp();
                        self.face_state = IntervalFaceState::Running;
                        watch::slcd::set_indicator(Indicator::Bell);
                        watch::slcd::set_colon();
                    }
                } else if self.face_state == IntervalFaceState::Running {
                    self.abort_running_timer();
                    self.init_timer_info();
                } else if self.face_state == IntervalFaceState::Pausing {
                    self.button_beep(settings);
                    self.resume_paused_timer();
                }
                self.face_draw(0);
            }
            Event::Button(Button::Alarm, ButtonEvent::LongUp) => self.abort_quick_ticks(),
            Event::BackgroundTask => {
                if self.timer_run_state == 0 {
                    let t = &self.timers[self.timer_idx as usize];
                    self.timer_run_state = if t.work_minutes + t.work_seconds != 0 {
                        1
                    } else if t.break_minutes + t.break_seconds != 0 {
                        2
                    } else if t.cooldown_minutes + t.cooldown_seconds != 0 {
                        3
                    } else {
                        4
                    };
                } else if self.timer_run_state == 1 {
                    self.timer_work_round += 1;
                    if self.timer_work_round == self.timers[self.timer_idx as usize].work_rounds {
                        self.timer_work_round = 0;
                        let t = &self.timers[self.timer_idx as usize];
                        if t.break_minutes + t.break_seconds != 0
                            && (t.full_rounds == 0
                                || (t.full_rounds != 0
                                    && self.timer_full_round + 1 < t.full_rounds))
                        {
                            self.timer_run_state = 2;
                        } else {
                            self.timer_full_round += 1;
                            if t.full_rounds != 0 && self.timer_full_round == t.full_rounds {
                                self.timer_run_state =
                                    if t.cooldown_minutes + t.cooldown_seconds != 0 {
                                        3
                                    } else {
                                        4
                                    };
                            } else {
                                self.timer_run_state = 1;
                            }
                        }
                    }
                } else if self.timer_run_state == 2 {
                    self.timer_full_round += 1;
                    self.timer_work_round = 0;
                    let t = &self.timers[self.timer_idx as usize];
                    if t.full_rounds != 0 && self.timer_full_round == t.full_rounds {
                        self.timer_run_state = if t.cooldown_minutes + t.cooldown_seconds != 0 {
                            3
                        } else {
                            4
                        };
                        self.timer_full_round -= 1;
                    } else if t.work_minutes + t.work_seconds != 0 {
                        self.timer_run_state = 1;
                    }
                } else if self.timer_run_state == 3 {
                    self.timer_run_state = 4;
                }
                if self.timer_run_state < 4 {
                    self.set_next_timestamp();
                } else {
                    self.face_state = IntervalFaceState::Waiting;
                    self.init_timer_info();
                    self.face_draw(0);
                    crate::movement::play_alarm_beeps(1, Note::C7);
                }
            }
            Event::Button(Button::Mode, ButtonEvent::LongPress) => {
                self.abort_quick_ticks();
                movement::move_to_face(0);
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        if self.face_state == IntervalFaceState::Setting {
            self.face_state = IntervalFaceState::Waiting;
        }
        watch::led::set_led_off();
        self.is_active = false;
    }
}
