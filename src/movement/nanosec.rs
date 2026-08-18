//! Nanosec watch face.
//!
//! Port of the C `nanosec_face.c`. Fine frequency correction for the RTC with
//! temperature compensation profiles. It is a pure state machine: it reacts to
//! a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::rtc;
use crate::watch::slcd;
use crate::watch::utility;

const DITHERING: i32 = 31;
const NANOSEC_MAX_SCREEN: u8 = 7;
const NANOSEC_PROFILE_COUNT: u8 = 5;

/// The nanosec state.
pub struct NanosecState {
    freq_correction: i16,
    center_temperature: i16,
    quadratic_tempco: i16,
    cubic_tempco: i16,
    correction_profile: u8,
    correction_cadence: u8,
    aging_ppm_pa: i16,
    last_correction_time: u32,
}

impl NanosecState {
    const fn new() -> Self {
        NanosecState {
            freq_correction: 0,
            center_temperature: 2500,
            quadratic_tempco: 0,
            cubic_tempco: 0,
            correction_profile: 3,
            correction_cadence: 10,
            aging_ppm_pa: 0,
            last_correction_time: 0,
        }
    }
}

/// The nanosec face state.
pub struct NanosecFace {
    state: NanosecState,
    screen: u8,
    changed: bool,
    freq_correction_residual: i16,
    freq_correction_previous: i16,
}

impl NanosecFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        NanosecFace {
            state: NanosecState::new(),
            screen: 0,
            changed: false,
            freq_correction_residual: 0,
            freq_correction_previous: -30000,
        }
    }

    pub fn new() -> Self {
        NanosecFace::new_static()
    }

    fn init_profile(&mut self) {
        self.changed = true;
        self.state.correction_cadence = 10;
        let date_time = rtc::get_date_time();
        self.state.last_correction_time = utility::date_time_to_unix_time(date_time, 0);
        match self.state.correction_profile {
            0 | 1 => {
                self.state.freq_correction = 0;
                self.state.center_temperature = 2500;
                self.state.quadratic_tempco = 0;
                self.state.cubic_tempco = 0;
                self.state.aging_ppm_pa = 0;
            }
            2 => {
                self.state.freq_correction = 0;
                self.state.center_temperature = 2500;
                self.state.quadratic_tempco = 3400;
                self.state.cubic_tempco = 0;
                self.state.aging_ppm_pa = 0;
            }
            3 => {
                self.state.freq_correction = 0;
                self.state.center_temperature = 2500;
                self.state.quadratic_tempco = 3400;
                self.state.cubic_tempco = 1360;
                self.state.aging_ppm_pa = 0;
            }
            _ => {
                self.state.freq_correction = 1768;
                self.state.center_temperature = 2653;
                self.state.quadratic_tempco = 4091;
                self.state.cubic_tempco = 1359;
                self.state.aging_ppm_pa = 0;
            }
        }
    }

    fn internal_write_rtc_correction(&mut self, value: i16, sign: i16) {
        if sign == 0 {
            if value == self.freq_correction_previous {
                return;
            }
            self.freq_correction_previous = value;
        } else {
            if value == -self.freq_correction_previous {
                return;
            }
            self.freq_correction_previous = -value;
        }
        rtc::freqcorr_write(value, sign);
    }

    fn apply_rtc_correction(&mut self, correction: i16) {
        let correction = correction + self.freq_correction_residual;
        let mut correction_lr = correction as i32 * 2 / DITHERING;
        if correction_lr & 1 != 0 {
            if correction_lr > 0 {
                correction_lr += 1;
            } else {
                correction_lr -= 1;
            }
        }
        correction_lr >>= 1;
        self.freq_correction_residual = correction - correction_lr as i16 * DITHERING as i16;
        if correction_lr > 127 {
            self.internal_write_rtc_correction(127, 0);
        } else if correction_lr < -127 {
            self.internal_write_rtc_correction(127, 1);
        } else if correction_lr < 0 {
            self.internal_write_rtc_correction((-correction_lr) as i16, 1);
        } else {
            self.internal_write_rtc_correction(correction_lr as i16, 0);
        }
    }

    fn get_aging(&self) -> f32 {
        let date_time = rtc::get_date_time();
        let years = (utility::date_time_to_unix_time(date_time, 0)
            - self.state.last_correction_time) as f32
            / 31536000.0;
        years * self.state.aging_ppm_pa as f32 / 100.0
    }

    fn update_display(&self) {
        let mut buf = [0u8; 11];
        match self.screen {
            0 => {
                buf[0] = b'F';
                buf[1] = b'C';
                buf[2] = b' ';
                buf[3] = b' ';
                write_num(
                    &mut buf,
                    self.state.freq_correction.unsigned_abs() as u32,
                    4,
                    6,
                );
            }
            1 => {
                buf[0] = b'T';
                buf[1] = b'0';
                buf[2] = b' ';
                buf[3] = b' ';
                write_num(
                    &mut buf,
                    self.state.center_temperature.unsigned_abs() as u32,
                    4,
                    6,
                );
            }
            2 => {
                buf[0] = b'2';
                buf[1] = b'C';
                buf[2] = b' ';
                buf[3] = b' ';
                write_num(
                    &mut buf,
                    self.state.quadratic_tempco.unsigned_abs() as u32,
                    4,
                    6,
                );
            }
            3 => {
                buf[0] = b'3';
                buf[1] = b'C';
                buf[2] = b' ';
                buf[3] = b' ';
                write_num(
                    &mut buf,
                    self.state.cubic_tempco.unsigned_abs() as u32,
                    4,
                    6,
                );
            }
            4 => {
                buf[0] = b'P';
                buf[1] = b'R';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b' ';
                buf[5] = b' ';
                buf[6] = b' ';
                buf[7] = b'P';
                buf[8] = b'0' + self.state.correction_profile;
            }
            5 => {
                buf[0] = b'C';
                buf[1] = b'D';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b' ';
                buf[5] = b' ';
                buf[6] = b' ';
                buf[7] = b'0' + self.state.correction_cadence / 10;
                buf[8] = b'0' + self.state.correction_cadence % 10;
            }
            6 => {
                buf[0] = b'A';
                buf[1] = b'A';
                buf[2] = b' ';
                buf[3] = b' ';
                write_num(
                    &mut buf,
                    self.state.aging_ppm_pa.unsigned_abs() as u32,
                    4,
                    6,
                );
            }
            _ => {}
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn value_increase(&mut self, delta: i16) {
        self.changed = true;
        match self.screen {
            0 => self.state.freq_correction += delta,
            1 => self.state.center_temperature += delta,
            2 => self.state.quadratic_tempco += delta,
            3 => self.state.cubic_tempco += delta,
            4 => {
                let mut p =
                    (self.state.correction_profile as i16 + delta) % NANOSEC_PROFILE_COUNT as i16;
                if p < 0 {
                    p += NANOSEC_PROFILE_COUNT as i16;
                }
                self.state.correction_profile = p as u8;
            }
            5 => {
                let c = self.state.correction_cadence;
                self.state.correction_cadence = match c {
                    1 => {
                        if delta > 0 {
                            5
                        } else {
                            60
                        }
                    }
                    5 => {
                        if delta > 0 {
                            10
                        } else {
                            1
                        }
                    }
                    10 => {
                        if delta > 0 {
                            20
                        } else {
                            5
                        }
                    }
                    20 => {
                        if delta > 0 {
                            60
                        } else {
                            10
                        }
                    }
                    _ => {
                        if delta > 0 {
                            1
                        } else {
                            20
                        }
                    }
                };
            }
            6 => self.state.aging_ppm_pa += delta,
            _ => {}
        }
        self.update_display();
    }

    fn next_edit_screen(&mut self) {
        self.screen = (self.screen + 1) % NANOSEC_MAX_SCREEN;
        self.update_display();
    }
}

/// Writes a number right-aligned into the buffer at the given offset.
fn write_num(buf: &mut [u8; 11], value: u32, offset: usize, width: usize) {
    let mut v = value;
    let mut i = offset + width - 1;
    loop {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if i == offset || v == 0 {
            break;
        }
        i -= 1;
    }
}

impl WatchFace for NanosecFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.changed = false;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => {
                self.screen = 0;
                self.update_display();
            }
            Event::Tick => {}
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                if self.screen == 0 {
                    movement::move_to_next_face();
                } else {
                    self.next_edit_screen();
                }
            }
            Event::Button(Button::Mode, ButtonEvent::LongPress) => self.next_edit_screen(),
            Event::Button(Button::Light, ButtonEvent::Up) => self.value_increase(1),
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                if self.screen == 4 {
                    self.init_profile();
                    self.screen = 0;
                    self.update_display();
                } else {
                    self.value_increase(50);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => self.value_increase(-1),
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.screen == 4 {
                    self.value_increase(-1);
                } else {
                    self.value_increase(-50);
                }
            }
            Event::BackgroundTask => {
                // Legacy nanosec profiles are no longer an independent
                // compensation model. The authoritative stored profile path
                // reads a validated sensor and otherwise fails closed.
                let manual_ppm = self.state.freq_correction / 100;
                let _result = movement::rtc_calibration_store::apply(manual_ppm);
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}

    fn wants_background_task(&mut self, _settings: &Settings) -> bool {
        if self.state.correction_profile == 0 {
            return false;
        }
        let date_time = rtc::get_date_time();
        date_time.minute % self.state.correction_cadence == 0
    }
}
