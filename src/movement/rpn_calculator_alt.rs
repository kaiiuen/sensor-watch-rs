//! RPN calculator (alt) watch face.
//!
//! Port of the C `rpn_calculator_alt_face.c`. An alternative RPN calculator
//! with a function menu. It is a pure state machine: it reacts to a single
//! event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::slcd;

const CALC_MAX_STACK_SIZE: usize = 10;
const CALC_OPERATION: u8 = 0;
const CALC_NUMBER: u8 = 1;

/// A calculator function.
struct CalcFn {
    name: [u8; 2],
    input: u8,
    output: u8,
    id: u8,
}

const FN_NUMBER: u8 = 0;
const FN_ADD: u8 = 1;
const FN_SUB: u8 = 2;
const FN_MUL: u8 = 3;
const FN_DIV: u8 = 4;
const FN_POW: u8 = 5;
const FN_SQRT: u8 = 6;
const FN_LOG: u8 = 7;
const FN_LOG10: u8 = 8;
const FN_E: u8 = 9;
const FN_PI: u8 = 10;
const FN_COS: u8 = 11;
const FN_SIN: u8 = 12;
const FN_TAN: u8 = 13;
const FN_POP: u8 = 14;
const FN_SWAP: u8 = 15;
const FN_DUP: u8 = 16;
const FN_CLEAR: u8 = 17;
const FN_SIZE: u8 = 18;

const FUNCTIONS: [CalcFn; 19] = [
    CalcFn {
        name: [b'n', b'o'],
        input: 0,
        output: 1,
        id: FN_NUMBER,
    },
    CalcFn {
        name: [b'*', b' '],
        input: 2,
        output: 1,
        id: FN_ADD,
    },
    CalcFn {
        name: [b'-', b' '],
        input: 2,
        output: 1,
        id: FN_SUB,
    },
    CalcFn {
        name: [b'H', b' '],
        input: 2,
        output: 1,
        id: FN_MUL,
    },
    CalcFn {
        name: [b'/', b' '],
        input: 2,
        output: 1,
        id: FN_DIV,
    },
    CalcFn {
        name: [b'P', b'o'],
        input: 2,
        output: 1,
        id: FN_POW,
    },
    CalcFn {
        name: [b'S', b'r'],
        input: 1,
        output: 1,
        id: FN_SQRT,
    },
    CalcFn {
        name: [b'L', b'n'],
        input: 1,
        output: 1,
        id: FN_LOG,
    },
    CalcFn {
        name: [b'L', b'o'],
        input: 1,
        output: 1,
        id: FN_LOG10,
    },
    CalcFn {
        name: [b'e', b' '],
        input: 0,
        output: 1,
        id: FN_E,
    },
    CalcFn {
        name: [b'P', b'i'],
        input: 0,
        output: 1,
        id: FN_PI,
    },
    CalcFn {
        name: [b'C', b'o'],
        input: 1,
        output: 1,
        id: FN_COS,
    },
    CalcFn {
        name: [b'S', b'i'],
        input: 1,
        output: 1,
        id: FN_SIN,
    },
    CalcFn {
        name: [b'T', b'a'],
        input: 1,
        output: 1,
        id: FN_TAN,
    },
    CalcFn {
        name: [b'P', b'O'],
        input: 1,
        output: 0,
        id: FN_POP,
    },
    CalcFn {
        name: [b'S', b'W'],
        input: 2,
        output: 2,
        id: FN_SWAP,
    },
    CalcFn {
        name: [b'd', b'u'],
        input: 1,
        output: 1,
        id: FN_DUP,
    },
    CalcFn {
        name: [b'C', b'L'],
        input: 1,
        output: 0,
        id: FN_CLEAR,
    },
    CalcFn {
        name: [b'L', b'E'],
        input: 1,
        output: 0,
        id: FN_SIZE,
    },
];

const FUNCTIONS_LEN: u8 = 19;
const SECONDARY_FN_INDEX: u8 = 15;

/// The RPN calculator alt face state.
pub struct RpnCalculatorAltFace {
    stack: [f64; CALC_MAX_STACK_SIZE],
    stack_size: usize,
    mode: u8,
    fn_index: u8,
    min: f64,
    max: f64,
}

impl RpnCalculatorAltFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        RpnCalculatorAltFace {
            stack: [0.0; CALC_MAX_STACK_SIZE],
            stack_size: 0,
            mode: CALC_OPERATION,
            fn_index: 0,
            min: f64::NAN,
            max: f64::NAN,
        }
    }

    pub fn new() -> Self {
        RpnCalculatorAltFace::new_static()
    }

    fn top(&self) -> f64 {
        self.stack[self.stack_size - 1]
    }

    fn push(&mut self, x: f64) {
        self.stack[self.stack_size] = x;
        self.stack_size += 1;
    }

    fn pop(&mut self) -> f64 {
        self.stack_size -= 1;
        self.stack[self.stack_size]
    }

    fn show_number(&self, mut num: f64) {
        let mut buf = [0u8; 11];
        let negative = num < 0.0;
        let max_digits = if negative { 5 } else { 6 };
        if num.is_nan() {
            slcd::clear_colon();
            buf[0] = b' ';
            buf[1] = b' ';
            buf[2] = b'n';
            buf[3] = b'a';
            buf[4] = b'n';
            buf[5] = b' ';
            buf[6] = b' ';
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 2);
            return;
        }
        if negative {
            num = -num;
        }
        if num == 0.0 || (num >= 0.5 && (num - (num as i32 as f64)).abs() < 0.0001) {
            if libm::floor(libm::log10(num)) + 1.0 <= max_digits as f64 {
                let v = libm::round(num) as i32;
                buf[0] = b' ';
                buf[1] = b' ';
                if negative {
                    buf[2] = b'-';
                } else {
                    buf[2] = b' ';
                }
                let mut i = 3;
                let mut n = v.unsigned_abs();
                let mut digits = [0u8; 6];
                let mut dc = 0;
                if n == 0 {
                    digits[0] = 0;
                    dc = 1;
                } else {
                    while n > 0 {
                        digits[dc] = (n % 10) as u8;
                        n /= 10;
                        dc += 1;
                    }
                }
                for k in (0..dc).rev() {
                    buf[i] = b'0' + digits[k];
                    i += 1;
                }
                slcd::clear_colon();
                slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 2);
                return;
            }
        }
        if num < 1.0 && num >= 0.0999 {
            let digits = libm::round(num * 10000.0) as i32;
            buf[0] = b' ';
            buf[1] = b' ';
            buf[2] = if negative { b'-' } else { b' ' };
            buf[3] = b'0';
            buf[4] = b'0' + ((digits / 1000) % 10) as u8;
            buf[5] = b'0' + ((digits / 100) % 10) as u8;
            buf[6] = b'0' + ((digits / 10) % 10) as u8;
            buf[7] = b'0' + (digits % 10) as u8;
            slcd::set_colon();
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 2);
            return;
        }
        let mut exponent = 0i32;
        while num < 1.0 {
            num *= 10.0;
            exponent -= 1;
        }
        while num >= 10.0 {
            num /= 10.0;
            exponent += 1;
        }
        if exponent < -9 {
            buf[0] = b' ';
            buf[1] = b' ';
            buf[2] = b's';
            buf[3] = b'm';
            buf[4] = b'a';
            buf[5] = b'l';
            buf[6] = b'l';
            buf[7] = b' ';
            slcd::clear_colon();
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 2);
            return;
        }
        if exponent > 39 {
            buf[0] = b' ';
            buf[1] = b' ';
            buf[2] = b'b';
            buf[3] = b'i';
            buf[4] = b'g';
            buf[5] = b' ';
            buf[6] = b' ';
            buf[7] = b' ';
            slcd::clear_colon();
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 2);
            return;
        }
        let digits = libm::round(num * 10000.0) as i32;
        buf[0] = b'0' + ((exponent / 10) % 10) as u8;
        buf[1] = b'0' + (exponent % 10) as u8;
        buf[2] = if negative { b'-' } else { b' ' };
        buf[3] = b'0' + ((digits / 1000) % 10) as u8;
        buf[4] = b'0' + ((digits / 100) % 10) as u8;
        buf[5] = b'0' + ((digits / 10) % 10) as u8;
        buf[6] = b'0' + (digits % 10) as u8;
        slcd::set_colon();
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 2);
    }

    fn change_mode(&mut self, mode: u8) {
        self.mode = mode;
        self.fn_index = 0;
        self.show_fn(0);
    }

    fn adjust_number(&mut self, direction: i32) {
        if direction > 0 {
            self.min = self.top();
        } else {
            self.max = self.top();
        }
        let bound = if direction > 0 { self.max } else { self.min };
        if bound.is_nan() {
            if (direction as f64 * self.top()) < 0.0 {
                self.stack[self.stack_size - 1] = 0.0;
            } else if self.top() == 0.0 {
                self.stack[self.stack_size - 1] = direction as f64 * 10.0;
            } else {
                self.stack[self.stack_size - 1] *= 10.0;
            }
        } else {
            let mut c = (self.max + self.min) / 2.0;
            let mag = libm::log10((self.max - self.min).abs()) - 0.1;
            if mag > 0.0 {
                let div = libm::pow(10.0, libm::floor(mag));
                let sign = if c < 0.0 { -1.0 } else { 1.0 };
                c = sign * libm::floor(c.abs() / div) * div;
            }
            self.stack[self.stack_size - 1] = c;
        }
    }

    fn run_fn(&mut self, id: u8) {
        match id {
            FN_NUMBER => {
                self.push(10.0);
                self.min = f64::NAN;
                self.max = f64::NAN;
                self.change_mode(CALC_NUMBER);
            }
            FN_ADD => {
                let a = self.pop();
                let b = self.pop();
                self.push(a + b);
            }
            FN_SUB => {
                let a = self.pop();
                let b = self.pop();
                self.push(b - a);
            }
            FN_MUL => {
                let a = self.pop();
                let b = self.pop();
                self.push(a * b);
            }
            FN_DIV => {
                let a = self.pop();
                let b = self.pop();
                self.push(b / a);
            }
            FN_POW => {
                let a = self.pop();
                let b = self.pop();
                self.push(libm::pow(b, a));
            }
            FN_SQRT => {
                let x = self.pop();
                self.push(libm::sqrt(x));
            }
            FN_LOG => {
                let x = self.pop();
                self.push(libm::log(x));
            }
            FN_LOG10 => {
                let x = self.pop();
                self.push(libm::log10(x));
            }
            FN_E => self.push(core::f64::consts::E),
            FN_PI => self.push(core::f64::consts::PI),
            FN_COS => {
                let x = self.pop();
                self.push(libm::cos(x));
            }
            FN_SIN => {
                let x = self.pop();
                self.push(libm::sin(x));
            }
            FN_TAN => {
                let x = self.pop();
                self.push(libm::tan(x));
            }
            FN_POP => {
                self.stack_size -= 1;
            }
            FN_SWAP => {
                let a = self.pop();
                let b = self.pop();
                self.push(a);
                self.push(b);
            }
            FN_DUP => {
                let a = self.pop();
                self.push(a);
                self.push(a);
            }
            FN_CLEAR => self.stack_size = 0,
            FN_SIZE => {
                let a = self.stack_size as f64;
                self.push(a);
            }
            _ => {}
        }
    }

    fn show_fn(&self, subsecond: u8) {
        if subsecond % 2 == 1 {
            slcd::display_string("  ", 0);
            return;
        }
        let f = &FUNCTIONS[self.fn_index as usize];
        let mut buf = [0u8; 3];
        buf[0] = f.name[0];
        buf[1] = f.name[1];
        slcd::display_string(core::str::from_utf8(&buf[..2]).unwrap_or("  "), 0);
        match buf[0] {
            b'H' => slcd::set_pixel(1, 14),
            b'/' => slcd::set_pixel(1, 15),
            _ => {}
        }
    }

    fn show_stack_top(&self) {
        if self.stack_size > 0 {
            self.show_number(self.top());
        } else {
            slcd::display_string("  ------", 2);
            slcd::clear_colon();
        }
    }
}

impl WatchFace for RpnCalculatorAltFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.min = f64::NAN;
        self.max = f64::NAN;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => {
                self.change_mode(CALC_OPERATION);
                self.show_stack_top();
            }
            Event::Tick => {
                if self.mode == CALC_OPERATION {
                    self.show_fn(0);
                }
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                if self.mode == CALC_NUMBER {
                    self.adjust_number(-1);
                    self.show_stack_top();
                } else {
                    movement::move_to_next_face();
                    return;
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                let f = &FUNCTIONS[self.fn_index as usize];
                let proposed = self.stack_size as i32 - f.input as i32;
                if self.mode == CALC_NUMBER {
                    self.change_mode(CALC_OPERATION);
                } else if proposed < 0 || proposed + f.output as i32 > CALC_MAX_STACK_SIZE as i32 {
                    movement::play_signal();
                } else {
                    self.run_fn(f.id);
                    self.show_stack_top();
                    self.fn_index = 0;
                    self.show_fn(0);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                if self.mode == CALC_NUMBER {
                    self.adjust_number(1);
                    self.show_stack_top();
                } else {
                    self.fn_index = (self.fn_index + 1) % FUNCTIONS_LEN;
                    self.show_fn(0);
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.mode == CALC_OPERATION {
                    if self.fn_index == 0 {
                        self.fn_index = SECONDARY_FN_INDEX;
                    } else {
                        self.fn_index = 0;
                    }
                    self.show_fn(0);
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
