//! Menstrual cycle tracker watch face.
//!
//! Port of the C `menstrual_cycle_face.c`. Tracks menstrual cycles and
//! estimates the next period and peak fertility window. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::rtc;
use crate::watch::slcd::Indicator;
use crate::watch::utility;

const TYPICAL_AVG_CYC: u8 = 28;
const SECONDS_PER_DAY: u32 = 86400;
const MENSTRUAL_CYCLE_FACE_NUM_PAGES: u8 = 6;

const TITLES: [&str; 6] = [
    "Prin   day",
    "Av  cycle ",
    "Peak Fert ",
    "Prishere  ",
    "Last Per  ",
    "    Reset ",
];

/// Packed date fields.
#[derive(Clone, Copy)]
struct Dates {
    first_day: u8,
    first_month: u8,
    first_year: u8,
    prev_day: u8,
    prev_month: u8,
    prev_year: u8,
}

impl Dates {
    fn is_zero(&self) -> bool {
        self.first_day == 0 && self.first_month == 0 && self.first_year == 0
    }
}

/// Packed cycle fields.
#[derive(Clone, Copy)]
struct Cycles {
    shortest_cycle: u8,
    longest_cycle: u8,
    average_cycle: u8,
    total_cycles: u8,
}

/// The menstrual cycle face state.
pub struct MenstrualCycleFace {
    current_page: u8,
    period_today: bool,
    reset_tracking: bool,
    days_prev_period: u8,
    utc_offset: u32,
    dates: Dates,
    cycles: Cycles,
}

impl MenstrualCycleFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        MenstrualCycleFace {
            current_page: 0,
            period_today: false,
            reset_tracking: false,
            days_prev_period: 0,
            utc_offset: 0,
            dates: Dates {
                first_day: 0,
                first_month: 0,
                first_year: 0,
                prev_day: 0,
                prev_month: 0,
                prev_year: 0,
            },
            cycles: Cycles {
                shortest_cycle: TYPICAL_AVG_CYC,
                longest_cycle: TYPICAL_AVG_CYC,
                average_cycle: TYPICAL_AVG_CYC,
                total_cycles: 0,
            },
        }
    }

    pub fn new() -> Self {
        MenstrualCycleFace::new_static()
    }

    fn beep(&self, settings: &Settings) {
        if settings.button_should_sound() {
            crate::movement::play_alarm_beeps(1, Note::E8);
        }
    }

    fn total_days_tracked(&self) -> u32 {
        if self.dates.is_zero() {
            return 0;
        }
        let start = rtc::DateTime {
            second: 0,
            minute: 0,
            hour: 0,
            day: self.dates.first_day,
            month: self.dates.first_month,
            year: self.dates.first_year,
        };
        let now = rtc::get_date_time();
        let unix_start = utility::date_time_to_unix_time(start, self.utc_offset);
        let unix_now = utility::date_time_to_unix_time(now, self.utc_offset);
        (unix_now - unix_start) / SECONDS_PER_DAY
    }

    fn days_till_period(&self) -> i8 {
        let days_left = (self.cycles.average_cycle as i32 * (self.cycles.total_cycles as i32 + 1))
            - self.total_days_tracked() as i32;
        if days_left < 0 { 0 } else { days_left as i8 }
    }

    fn reset_tracking(&mut self) {
        self.dates = Dates {
            first_day: 0,
            first_month: 0,
            first_year: 0,
            prev_day: 0,
            prev_month: 0,
            prev_year: 0,
        };
        self.cycles = Cycles {
            shortest_cycle: TYPICAL_AVG_CYC,
            longest_cycle: TYPICAL_AVG_CYC,
            average_cycle: TYPICAL_AVG_CYC,
            total_cycles: 0,
        };
        watch::slcd::clear_indicator(Indicator::Signal);
    }

    fn get_day_pk_fert(&self, which_day: u8) -> u8 {
        let prev = rtc::DateTime {
            second: 0,
            minute: 0,
            hour: 0,
            day: self.dates.prev_day,
            month: self.dates.prev_month,
            year: self.dates.prev_year,
        };
        let unix_prev = utility::date_time_to_unix_time(prev, self.utc_offset);
        let unix_pk = if which_day == 0 {
            unix_prev + ((self.cycles.shortest_cycle as u32 - 18) * SECONDS_PER_DAY)
        } else {
            unix_prev + ((self.cycles.longest_cycle as u32 - 11) * SECONDS_PER_DAY)
        };
        utility::date_time_from_unix_time(unix_pk, self.utc_offset).day
    }

    fn inside_fert_window(&self) -> bool {
        if self.dates.is_zero() {
            return false;
        }
        let now = rtc::get_date_time();
        let first = self.get_day_pk_fert(0);
        let last = self.get_day_pk_fert(1);
        if first > last {
            now.day >= first || now.day <= last
        } else {
            now.day >= first && now.day <= last
        }
    }

    fn update_shortest_longest_cycle(&mut self) {
        let prev = rtc::DateTime {
            second: 0,
            minute: 0,
            hour: 0,
            day: self.dates.prev_day,
            month: self.dates.prev_month,
            year: self.dates.prev_year,
        };
        let unix_prev = utility::date_time_to_unix_time(prev, self.utc_offset);
        let cycle_length = (self.total_days_tracked() - unix_prev / SECONDS_PER_DAY) as u8;
        if cycle_length < self.cycles.shortest_cycle {
            self.cycles.shortest_cycle = cycle_length;
        } else if cycle_length > self.cycles.longest_cycle {
            self.cycles.longest_cycle = cycle_length;
        }
    }
}

impl WatchFace for MenstrualCycleFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, settings: &Settings) {
        self.period_today = false;
        self.current_page = 0;
        self.reset_tracking = false;
        self.utc_offset = (movement::TIMEZONE_OFFSETS[(settings.time_zone() as usize).min(40)]
            as i32
            * 60) as u32;
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        let mut current_page = self.current_page;
        match event {
            Event::Tick | Event::Activate => {}
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                movement::move_to_next_face();
                return;
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                current_page = (current_page + 1) % MENSTRUAL_CYCLE_FACE_NUM_PAGES;
                self.current_page = current_page;
                self.days_prev_period = 0;
                watch::slcd::clear_indicator(Indicator::Bell);
                if watch::slcd::tick_animation_is_running() {
                    watch::slcd::stop_tick_animation();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => match current_page {
                3 => {
                    if self.period_today && self.total_days_tracked() != 0 {
                        self.update_shortest_longest_cycle();
                        let date_period = rtc::get_date_time();
                        self.dates.prev_day = date_period.day;
                        self.dates.prev_month = date_period.month;
                        self.dates.prev_year = date_period.year;
                        self.cycles.total_cycles += 1;
                        self.cycles.average_cycle =
                            (self.total_days_tracked() / self.cycles.total_cycles as u32) as u8;
                        self.period_today = !self.period_today;
                        self.beep(settings);
                    }
                }
                4 => {
                    if self.dates.is_zero() {
                        let unix_now =
                            utility::date_time_to_unix_time(rtc::get_date_time(), self.utc_offset);
                        let unix_prev = unix_now - (self.days_prev_period as u32 * SECONDS_PER_DAY);
                        let date_period =
                            utility::date_time_from_unix_time(unix_prev, self.utc_offset);
                        self.dates.first_day = date_period.day;
                        self.dates.first_month = date_period.month;
                        self.dates.first_year = date_period.year;
                        self.dates.prev_day = date_period.day;
                        self.dates.prev_month = date_period.month;
                        self.dates.prev_year = date_period.year;
                        self.beep(settings);
                    }
                }
                5 => {
                    if self.reset_tracking {
                        self.reset_tracking();
                        self.reset_tracking = !self.reset_tracking;
                        self.beep(settings);
                    }
                }
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::Up) => match current_page {
                3 => {
                    if self.total_days_tracked() != 0 {
                        self.period_today = !self.period_today;
                    }
                }
                4 => {
                    if self.dates.is_zero() {
                        self.days_prev_period = if self.days_prev_period > 99 {
                            0
                        } else {
                            self.days_prev_period + 1
                        };
                    }
                }
                5 => self.reset_tracking = !self.reset_tracking,
                _ => {}
            },
            _ => movement::default_loop_handler(event, settings),
        }

        watch::slcd::display_string(TITLES[current_page as usize], 0);
        if !self.dates.is_zero() {
            watch::slcd::set_indicator(Indicator::Signal);
        }

        let mut buf = [0u8; 11];
        match current_page {
            0 => {
                buf[0] = b'0' + (self.days_till_period() / 10) as u8;
                buf[1] = b'0' + (self.days_till_period() % 10) as u8;
                if self.inside_fert_window() {
                    watch::slcd::set_indicator(Indicator::Bell);
                }
                watch::slcd::display_string(core::str::from_utf8(&buf[..2]).unwrap_or("  "), 4);
            }
            1 => {
                buf[0] = b'0' + self.cycles.average_cycle / 10;
                buf[1] = b'0' + self.cycles.average_cycle % 10;
                watch::slcd::display_string(core::str::from_utf8(&buf[..2]).unwrap_or("  "), 2);
            }
            2 => {
                if !self.dates.is_zero() {
                    let first = self.get_day_pk_fert(0);
                    let last = self.get_day_pk_fert(1);
                    buf[0] = b'F';
                    buf[1] = b'r';
                    buf[2] = b'0' + first / 10;
                    buf[3] = b'0' + first % 10;
                    buf[4] = b' ';
                    buf[5] = b'T';
                    buf[6] = b'o';
                    buf[7] = b' ';
                    buf[8] = b'0' + last / 10;
                    buf[9] = b'0' + last % 10;
                    if self.inside_fert_window() {
                        watch::slcd::set_indicator(Indicator::Bell);
                    }
                    watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
                }
            }
            3 => {
                if self.dates.is_zero() {
                    watch::slcd::display_string("NA", 8);
                } else if self.period_today {
                    watch::slcd::display_string("y", 9);
                } else {
                    watch::slcd::display_string("n", 9);
                }
            }
            4 => {
                if self.dates.is_zero() {
                    buf[0] = b'0' + self.days_prev_period / 10;
                    buf[1] = b'0' + self.days_prev_period % 10;
                    watch::slcd::display_string(core::str::from_utf8(&buf[..2]).unwrap_or("  "), 8);
                } else if !watch::slcd::tick_animation_is_running() {
                    watch::slcd::start_tick_animation(500);
                }
            }
            _ => {
                if self.reset_tracking {
                    watch::slcd::display_string("y", 9);
                } else {
                    watch::slcd::display_string("n", 9);
                }
            }
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
