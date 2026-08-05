//! Simple calculator watch face.
//!
//! Port of the C `simple_calculator_face.c`. A basic calculator with add,
//! subtract, multiply, divide, root, and power operations. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::slcd;

const PLACEHOLDER_HUNDREDTHS: u8 = 0;
const PLACEHOLDER_TENTHS: u8 = 1;
const PLACEHOLDER_ONES: u8 = 2;
const PLACEHOLDER_TENS: u8 = 3;
const PLACEHOLDER_HUNDREDS: u8 = 4;
const PLACEHOLDER_THOUSANDS: u8 = 5;
const MAX_PLACEHOLDERS: u8 = 6;

const MODE_ENTERING_FIRST_NUM: u8 = 0;
const MODE_CHOOSING: u8 = 1;
const MODE_ENTERING_SECOND_NUM: u8 = 2;
const MODE_VIEW_RESULTS: u8 = 3;
const MODE_ERROR: u8 = 4;

const OP_ADD: u8 = 0;
const OP_SUB: u8 = 1;
const OP_MULT: u8 = 2;
const OP_DIV: u8 = 3;
const OP_ROOT: u8 = 4;
const OP_POWER: u8 = 5;
const OPERATIONS_COUNT: u8 = 6;

/// Increments the digit at the given placeholder in a number.
fn increment_digit(number: &mut CalculatorNumber, placeholder: u8) {
    let digit = match placeholder {
        PLACEHOLDER_HUNDREDTHS => &mut number.hundredths,
        PLACEHOLDER_TENTHS => &mut number.tenths,
        PLACEHOLDER_ONES => &mut number.ones,
        PLACEHOLDER_TENS => &mut number.tens,
        PLACEHOLDER_HUNDREDS => &mut number.hundreds,
        _ => &mut number.thousands,
    };
    *digit = (*digit + 1) % 10;
}

/// A calculator number stored digit-wise.
#[derive(Clone, Copy)]
struct CalculatorNumber {
    negative: bool,
    hundredths: u8,
    tenths: u8,
    ones: u8,
    tens: u8,
    hundreds: u8,
    thousands: u8,
}

impl CalculatorNumber {
    const fn zero() -> Self {
        CalculatorNumber {
            negative: false,
            hundredths: 0,
            tenths: 0,
            ones: 0,
            tens: 0,
            hundreds: 0,
            thousands: 0,
        }
    }

    fn reset(&mut self) {
        *self = CalculatorNumber::zero();
    }

    fn is_zero(&self) -> bool {
        self.hundredths == 0
            && self.tenths == 0
            && self.ones == 0
            && self.tens == 0
            && self.hundreds == 0
            && self.thousands == 0
    }
}

/// The simple calculator face state.
pub struct SimpleCalculatorFace {
    placeholder: u8,
    mode: u8,
    operation: u8,
    first_num: CalculatorNumber,
    second_num: CalculatorNumber,
    result: CalculatorNumber,
}

impl SimpleCalculatorFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        SimpleCalculatorFace {
            placeholder: PLACEHOLDER_ONES,
            mode: MODE_ENTERING_FIRST_NUM,
            operation: OP_ADD,
            first_num: CalculatorNumber::zero(),
            second_num: CalculatorNumber::zero(),
            result: CalculatorNumber::zero(),
        }
    }

    pub fn new() -> Self {
        SimpleCalculatorFace::new_static()
    }

    fn increment_placeholder(&self, number: &mut CalculatorNumber) {
        let placeholder = self.placeholder;
        increment_digit(number, placeholder);
    }

    fn convert_to_float(&self, number: CalculatorNumber) -> f32 {
        let mut result = 0.0f32;
        result += number.thousands as f32 * 1000.0;
        result += number.hundreds as f32 * 100.0;
        result += number.tens as f32 * 10.0;
        result += number.ones as f32 * 1.0;
        result += number.tenths as f32 * 0.1;
        result += number.hundredths as f32 * 0.01;
        result = libm::roundf(result * 100.0) / 100.0;
        if number.negative {
            result = -result;
        }
        result
    }

    fn update_display_number(&self, number: &CalculatorNumber, which_num: u8) {
        let mut buf = [0u8; 11];
        buf[0] = b'C';
        buf[1] = b'A';
        buf[2] = b'0' + which_num;
        buf[3] = if number.negative { b'-' } else { b' ' };
        buf[4] = b'0' + number.thousands;
        buf[5] = b'0' + number.hundreds;
        buf[6] = b'0' + number.tens;
        buf[7] = b'0' + number.ones;
        buf[8] = b'0' + number.tenths;
        buf[9] = b'0' + number.hundredths;
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn set_operation(&self) {
        match self.operation {
            OP_ADD => slcd::display_string("       Add", 0),
            OP_SUB => slcd::display_string("       sub", 0),
            OP_MULT => slcd::display_string("      n&ul", 0),
            OP_DIV => slcd::display_string("       div", 0),
            OP_ROOT => slcd::display_string("      root", 0),
            _ => slcd::display_string("       pow", 0),
        }
    }

    fn cycle_operation(&mut self) {
        self.operation = (self.operation + 1) % OPERATIONS_COUNT;
    }

    fn convert_to_string(&self, number: f32) -> CalculatorNumber {
        let mut result = CalculatorNumber::zero();
        let mut number = number;
        if number < 0.0 {
            number = -number;
            result.negative = true;
        }
        let int_part = number as i32;
        let decimal_part_float = (number - int_part as f32) * 100.0;
        let decimal_part = libm::roundf(decimal_part_float) as i32;
        result.thousands = ((int_part / 1000) % 10) as u8;
        result.hundreds = ((int_part / 100) % 10) as u8;
        result.tens = ((int_part / 10) % 10) as u8;
        result.ones = (int_part % 10) as u8;
        result.tenths = ((decimal_part / 10) % 10) as u8;
        result.hundredths = (decimal_part % 10) as u8;
        result
    }

    fn set_number(&self, number: &CalculatorNumber, which_num: u8, subsecond: u8) {
        let mut buf = [0u8; 11];
        buf[0] = b'C';
        buf[1] = b'A';
        buf[2] = b'0' + which_num;
        buf[3] = if number.negative { b'-' } else { b' ' };
        buf[4] = b'0' + number.thousands;
        buf[5] = b'0' + number.hundreds;
        buf[6] = b'0' + number.tens;
        buf[7] = b'0' + number.ones;
        buf[8] = b'0' + number.tenths;
        buf[9] = b'0' + number.hundredths;
        let display_index = 9 - self.placeholder;
        if subsecond % 2 == 0 {
            buf[display_index as usize] = b' ';
        }
        slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
    }

    fn view_results(&mut self) {
        let first = self.convert_to_float(self.first_num);
        let second = self.convert_to_float(self.second_num);
        let mut result_float = 0.0f32;
        match self.operation {
            OP_ADD => result_float = first + second,
            OP_SUB => result_float = first - second,
            OP_MULT => result_float = first * second,
            OP_DIV => {
                if second != 0.0 {
                    result_float = first / second;
                } else {
                    self.mode = MODE_ERROR;
                    return;
                }
            }
            OP_ROOT => {
                if first >= 0.0 {
                    result_float = libm::sqrtf(first);
                } else {
                    self.mode = MODE_ERROR;
                    return;
                }
            }
            OP_POWER => result_float = libm::powf(first, second),
            _ => result_float = 0.0,
        }
        if result_float > 9999.99 || result_float < -9999.99 {
            self.mode = MODE_ERROR;
            return;
        }
        result_float = libm::roundf(result_float * 100.0) / 100.0;
        self.result = self.convert_to_string(result_float);
        self.update_display_number(&self.result, 3);
    }

    fn reset_all(&mut self) {
        self.first_num.reset();
        self.second_num.reset();
        self.mode = MODE_ENTERING_FIRST_NUM;
        self.operation = OP_ADD;
        self.placeholder = PLACEHOLDER_ONES;
    }
}

impl WatchFace for SimpleCalculatorFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.placeholder = PLACEHOLDER_ONES;
        self.mode = MODE_ENTERING_FIRST_NUM;
        self.second_num.reset();
        self.result.reset();
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate | Event::Tick => match self.mode {
                MODE_ENTERING_FIRST_NUM => {
                    self.set_number(&self.first_num, 1, 0);
                }
                MODE_CHOOSING => self.set_operation(),
                MODE_ENTERING_SECOND_NUM => {
                    if self.operation == OP_ROOT {
                        self.mode = MODE_VIEW_RESULTS;
                    } else {
                        self.set_number(&self.second_num, 2, 0);
                    }
                }
                MODE_VIEW_RESULTS => self.view_results(),
                MODE_ERROR => slcd::display_string("CA  Error ", 0),
                _ => {}
            },
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            Event::Button(Button::Light, ButtonEvent::Up) => match self.mode {
                MODE_ENTERING_FIRST_NUM | MODE_ENTERING_SECOND_NUM => {
                    self.placeholder = (self.placeholder + 1) % MAX_PLACEHOLDERS;
                }
                MODE_CHOOSING => self.cycle_operation(),
                MODE_ERROR => self.reset_all(),
                _ => {}
            },
            Event::Button(Button::Light, ButtonEvent::LongPress) => match self.mode {
                MODE_ENTERING_FIRST_NUM => self.first_num.negative = !self.first_num.negative,
                MODE_ENTERING_SECOND_NUM => self.second_num.negative = !self.second_num.negative,
                MODE_ERROR => self.reset_all(),
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::Up) => match self.mode {
                MODE_ENTERING_FIRST_NUM => {
                    increment_digit(&mut self.first_num, self.placeholder);
                    self.update_display_number(&self.first_num, 1);
                }
                MODE_CHOOSING => {
                    self.mode = MODE_ENTERING_SECOND_NUM;
                }
                MODE_ENTERING_SECOND_NUM => {
                    increment_digit(&mut self.second_num, self.placeholder);
                    self.update_display_number(&self.second_num, 2);
                }
                MODE_ERROR => self.reset_all(),
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => match self.mode {
                MODE_ENTERING_FIRST_NUM => self.first_num.reset(),
                MODE_ENTERING_SECOND_NUM => self.second_num.reset(),
                MODE_ERROR => self.reset_all(),
                _ => {}
            },
            Event::Button(Button::Mode, ButtonEvent::Down) => {}
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                if self.mode == MODE_ERROR {
                    self.reset_all();
                } else if self.mode == MODE_ENTERING_FIRST_NUM && self.first_num.is_zero() {
                    movement::move_to_next_face();
                } else {
                    self.placeholder = PLACEHOLDER_ONES;
                    self.mode = (self.mode + 1) % 4;
                    if self.mode == MODE_ENTERING_FIRST_NUM {
                        self.first_num = self.result;
                        self.second_num.reset();
                        self.result.reset();
                    }
                }
            }
            Event::Button(Button::Mode, ButtonEvent::LongPress) => {
                if self.first_num.is_zero() {
                    movement::move_to_face(0);
                } else {
                    self.reset_all();
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
