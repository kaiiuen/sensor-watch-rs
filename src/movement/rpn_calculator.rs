//! RPN calculator watch face.
//!
//! Port of the C `rpn_calculator_face.c`. A reverse-polish-notation calculator
//! with a small stack. It is a pure state machine: it reacts to a single event
//! and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch::slcd;

const RPN_CALCULATOR_STACK_SIZE: usize = 4;
const RPN_CALCULATOR_MAX_OPS: u8 = 7;

const RPN_CALCULATOR_OP_ADD: u8 = 0;
const RPN_CALCULATOR_OP_SUB: u8 = 1;
const RPN_CALCULATOR_OP_MUL: u8 = 2;
const RPN_CALCULATOR_OP_DIV: u8 = 3;
const RPN_CALCULATOR_OP_POW: u8 = 4;
const RPN_CALCULATOR_OP_SQRT: u8 = 5;
const RPN_CALCULATOR_OP_PI: u8 = 6;

const RPN_CALCULATOR_ERR: u8 = 0;
const RPN_CALCULATOR_NUMBER: u8 = 1;
const RPN_CALCULATOR_WAITING: u8 = 2;
const RPN_CALCULATOR_OP: u8 = 3;

/// The RPN calculator face state.
pub struct RpnCalculatorFace {
    stack: [f32; RPN_CALCULATOR_STACK_SIZE],
    top: i8,
    op: u8,
    mode: u8,
    selection: u8,
}

impl RpnCalculatorFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        RpnCalculatorFace {
            stack: [0.0; RPN_CALCULATOR_STACK_SIZE],
            top: -1,
            op: 0,
            mode: RPN_CALCULATOR_WAITING,
            selection: 2,
        }
    }

    pub fn new() -> Self {
        RpnCalculatorFace::new_static()
    }

    fn draw_number(&self, buf: &mut [u8; 11], num: f32) {
        let f = libm::fmodf(num, 1.0) * 100.0;
        let int_part = ((num as i32) % 10000).unsigned_abs();
        let frac = (f as i32).unsigned_abs();
        buf[0] = b'C';
        buf[1] = b'A';
        buf[2] = b' ';
        buf[3] = b' ';
        buf[4] = b'0' + ((int_part / 1000) % 10) as u8;
        buf[5] = b'0' + ((int_part / 100) % 10) as u8;
        buf[6] = b'0' + ((int_part / 10) % 10) as u8;
        buf[7] = b'0' + (int_part % 10) as u8;
        buf[8] = b'0' + ((frac / 10) % 10) as u8;
        buf[9] = b'0' + (frac % 10) as u8;
    }

    fn draw_op(&self, buf: &mut [u8; 11], op: u8) {
        buf[0] = b'C';
        buf[1] = b'A';
        buf[2] = b' ';
        buf[3] = b' ';
        buf[4] = b' ';
        buf[5] = b' ';
        buf[6] = b' ';
        buf[7] = b' ';
        match op {
            RPN_CALCULATOR_OP_ADD => {
                buf[8] = b'A';
                buf[9] = b'd';
            }
            RPN_CALCULATOR_OP_SUB => {
                buf[8] = b's';
                buf[9] = b'u';
            }
            RPN_CALCULATOR_OP_MUL => {
                buf[7] = b'n';
                buf[8] = b'&';
                buf[9] = b'u';
            }
            RPN_CALCULATOR_OP_DIV => {
                buf[8] = b'd';
                buf[9] = b'i';
            }
            RPN_CALCULATOR_OP_POW => {
                buf[8] = b'p';
                buf[9] = b'o';
            }
            RPN_CALCULATOR_OP_SQRT => {
                buf[7] = b's';
                buf[8] = b'q';
                buf[9] = b'r';
            }
            _ => {
                buf[8] = b'p';
                buf[9] = b'i';
            }
        }
    }

    fn next_op(&mut self) {
        self.op += 1;
        self.op %= RPN_CALCULATOR_MAX_OPS;
    }

    fn inc_digit(&self, num: f32, position: u8) -> f32 {
        if position > 5 {
            return 0.0;
        }
        let position = 5 - position;
        let f = libm::fmodf(num, 1.0) * 100.0;
        let int_part = ((num as i32) % 10000).unsigned_abs();
        let frac = (f as i32).unsigned_abs();
        let mut buf = [0u8; 7];
        buf[0] = b'0' + ((int_part / 1000) % 10) as u8;
        buf[1] = b'0' + ((int_part / 100) % 10) as u8;
        buf[2] = b'0' + ((int_part / 10) % 10) as u8;
        buf[3] = b'0' + (int_part % 10) as u8;
        buf[4] = b'0' + ((frac / 10) % 10) as u8;
        buf[5] = b'0' + (frac % 10) as u8;
        let mut digit = buf[position as usize] - b'0';
        digit = (digit + 1) % 10;
        buf[position as usize] = digit + b'0';
        let mut value = 0.0f32;
        for i in 0..6 {
            value = value * 10.0 + (buf[i] - b'0') as f32;
        }
        value / 100.0
    }

    fn stack_push(&mut self, f: f32) {
        self.top += 1;
        if self.top as usize >= RPN_CALCULATOR_STACK_SIZE {
            for i in 0..RPN_CALCULATOR_STACK_SIZE - 1 {
                self.stack[i] = self.stack[i + 1];
            }
        }
        self.stack[self.top as usize] = f;
    }

    fn stack_peek(&self) -> f32 {
        if self.top > -1 {
            self.stack[self.top as usize]
        } else {
            0.0
        }
    }

    fn stack_pop(&mut self) -> f32 {
        let f = self.stack_peek();
        if self.top > -1 {
            self.stack[self.top as usize] = 0.0;
            self.top -= 1;
        } else {
            self.top = -1;
        }
        f
    }

    fn run_op(&mut self) {
        let mut op_found = false;
        match self.op {
            RPN_CALCULATOR_OP_PI => {
                self.stack_push(core::f32::consts::PI);
                op_found = true;
            }
            _ => {}
        }
        if op_found {
            self.mode = RPN_CALCULATOR_WAITING;
            return;
        }
        if self.top < 0 {
            self.mode = RPN_CALCULATOR_ERR;
            return;
        }
        let right = self.stack_pop();
        match self.op {
            RPN_CALCULATOR_OP_SQRT => {
                self.stack_push(libm::sqrtf(right));
                op_found = true;
            }
            _ => {}
        }
        if op_found {
            self.mode = RPN_CALCULATOR_WAITING;
            return;
        }
        if self.top < 0 {
            self.mode = RPN_CALCULATOR_ERR;
            return;
        }
        let left = self.stack_pop();
        match self.op {
            RPN_CALCULATOR_OP_ADD => {
                self.stack_push(left + right);
                op_found = true;
            }
            RPN_CALCULATOR_OP_SUB => {
                self.stack_push(left - right);
                op_found = true;
            }
            RPN_CALCULATOR_OP_MUL => {
                self.stack_push(left * right);
                op_found = true;
            }
            RPN_CALCULATOR_OP_DIV => {
                self.stack_push(left / right);
                op_found = true;
            }
            RPN_CALCULATOR_OP_POW => {
                self.stack_push(libm::powf(left, right));
                op_found = true;
            }
            _ => {}
        }
        if op_found {
            self.mode = RPN_CALCULATOR_WAITING;
            return;
        }
        self.mode = RPN_CALCULATOR_ERR;
    }

    fn draw(&self, subsecond: u8) {
        let mut buf = [0u8; 11];
        match self.mode {
            RPN_CALCULATOR_ERR => {
                buf[0] = b'C';
                buf[1] = b'A';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b'e';
                buf[5] = b'r';
                buf[6] = b'r';
                buf[7] = b' ';
                buf[8] = b' ';
                buf[9] = b' ';
            }
            RPN_CALCULATOR_NUMBER => {
                self.draw_number(&mut buf, self.stack_peek());
                let i = 4 + (5 - self.selection);
                if buf[i as usize] == b' ' {
                    buf[i as usize] = b'0';
                }
                if subsecond % 2 == 1 {
                    buf[i as usize] = b' ';
                }
            }
            RPN_CALCULATOR_WAITING => {
                self.draw_number(&mut buf, self.stack_peek());
            }
            RPN_CALCULATOR_OP => {
                self.draw_op(&mut buf, self.op);
            }
            _ => {}
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }
}

impl WatchFace for RpnCalculatorFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {}

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => self.draw(0),
            Event::Tick => {
                if self.mode == RPN_CALCULATOR_NUMBER {
                    self.draw(0);
                }
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                if self.mode == RPN_CALCULATOR_NUMBER {
                    self.mode = RPN_CALCULATOR_WAITING;
                    self.draw(0);
                } else {
                    self.mode = RPN_CALCULATOR_WAITING;
                    movement::move_to_next_face();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => match self.mode {
                RPN_CALCULATOR_WAITING => {
                    self.mode = RPN_CALCULATOR_OP;
                    self.draw(0);
                }
                RPN_CALCULATOR_NUMBER => {
                    self.selection = (self.selection + 1) % 6;
                    self.draw(0);
                }
                RPN_CALCULATOR_OP => {
                    self.next_op();
                    self.draw(0);
                }
                _ => movement::illuminate_led(),
            },
            Event::Button(Button::Alarm, ButtonEvent::Up) => match self.mode {
                RPN_CALCULATOR_WAITING => {
                    self.mode = RPN_CALCULATOR_NUMBER;
                    self.selection = 2;
                    self.stack_push(0.0);
                    self.draw(0);
                }
                RPN_CALCULATOR_NUMBER => {
                    let v = self.inc_digit(self.stack[self.top as usize], self.selection);
                    self.stack[self.top as usize] = v;
                    self.draw(0);
                }
                RPN_CALCULATOR_ERR => {
                    self.mode = RPN_CALCULATOR_WAITING;
                    self.draw(0);
                }
                RPN_CALCULATOR_OP => {
                    self.run_op();
                    self.draw(0);
                }
                _ => {}
            },
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
