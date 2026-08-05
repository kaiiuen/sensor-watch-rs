//! Morsecalc watch face.
//!
//! Port of the C `morsecalc_face.c` and the morsecalc library. A Morse-code
//! based RPN calculator. It is a pure state machine: it reacts to a single
//! event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::slcd;

const MORSECALC_TOKEN_LEN: usize = 32;
const MORSECODE_LEN: u32 = 5;
const N_STACK: usize = 10;

/// The International Morse code binary tree.
const MORSECODE_TREE: &[u8] = b" etianmsurwdkgohvf\0l\0pjbxcyzq\0C\x35\x34V\x33\0R\0\x32W\0+\0\0\0\0\x31\x36=/\0\0S(\0\x37\0\0\0\x38\0\x39\x30\0\0\0\0\0E\0\0\0\0\0\0?_\0\0\0\0\"\0\0.\0\0\0\0@\0\0\0'\0\0-\0\0\0\0\0\0\0\0;!\0)\0\0\0\0\0,\0\0\0\0:\0\0\0\0\0\0";

/// The calculator state.
struct CalcState {
    stack: [f64; N_STACK],
    mem: f64,
    s: u8,
}

impl CalcState {
    const fn new() -> Self {
        CalcState {
            stack: [f64::NAN; N_STACK],
            mem: 0.0,
            s: 0,
        }
    }

    fn init(&mut self) {
        self.stack = [f64::NAN; N_STACK];
        self.s = 0;
        self.mem = 0.0;
    }

    fn add(&mut self) -> i32 {
        if self.s < 2 {
            return -2;
        }
        let a = self.stack[self.s as usize - 2];
        let b = self.stack[self.s as usize - 1];
        self.stack[self.s as usize - 2] = a + b;
        self.s -= 1;
        0
    }

    fn subtract(&mut self) -> i32 {
        if self.s < 2 {
            return -2;
        }
        let a = self.stack[self.s as usize - 2];
        let b = self.stack[self.s as usize - 1];
        self.stack[self.s as usize - 2] = a - b;
        self.s -= 1;
        0
    }

    fn multiply(&mut self) -> i32 {
        if self.s < 2 {
            return -2;
        }
        let a = self.stack[self.s as usize - 2];
        let b = self.stack[self.s as usize - 1];
        self.stack[self.s as usize - 2] = a * b;
        self.s -= 1;
        0
    }

    fn divide(&mut self) -> i32 {
        if self.s < 2 {
            return -2;
        }
        let a = self.stack[self.s as usize - 2];
        let b = self.stack[self.s as usize - 1];
        self.stack[self.s as usize - 2] = a / b;
        self.s -= 1;
        0
    }

    fn negate(&mut self) -> i32 {
        if self.s < 1 {
            return -2;
        }
        self.stack[self.s as usize - 1] = -self.stack[self.s as usize - 1];
        0
    }

    fn invert(&mut self) -> i32 {
        if self.s < 1 {
            return -2;
        }
        self.stack[self.s as usize - 1] = 1.0 / self.stack[self.s as usize - 1];
        0
    }

    fn delete(&mut self) -> i32 {
        if self.s < 1 {
            return -2;
        }
        self.s -= 1;
        0
    }

    fn clear_stack(&mut self) -> i32 {
        self.stack = [f64::NAN; N_STACK];
        self.s = 0;
        0
    }

    fn flip(&mut self) -> i32 {
        if self.s < 2 {
            return -2;
        }
        self.stack.swap(self.s as usize - 2, self.s as usize - 1);
        0
    }

    fn mem_clear(&mut self) -> i32 {
        self.mem = 0.0;
        0
    }

    fn mem_recall(&mut self) -> i32 {
        if self.s >= N_STACK as u8 {
            return -2;
        }
        self.stack[self.s as usize] = self.mem;
        self.s += 1;
        0
    }

    fn mem_add(&mut self) -> i32 {
        if self.s < 1 {
            return -2;
        }
        self.mem += self.stack[self.s as usize - 1];
        self.s -= 1;
        0
    }

    fn mem_subtract(&mut self) -> i32 {
        if self.s < 1 {
            return -2;
        }
        self.mem -= self.stack[self.s as usize - 1];
        self.s -= 1;
        0
    }

    fn push(&mut self, v: f64) -> i32 {
        if self.s >= N_STACK as u8 {
            return -2;
        }
        self.stack[self.s as usize] = v;
        self.s += 1;
        0
    }

    fn e(&mut self) -> i32 {
        self.push(core::f64::consts::E)
    }

    fn pi(&mut self) -> i32 {
        self.push(core::f64::consts::PI)
    }

    fn unary(&mut self, f: fn(f64) -> f64) -> i32 {
        if self.s < 1 {
            return -2;
        }
        self.stack[self.s as usize - 1] = f(self.stack[self.s as usize - 1]);
        0
    }

    fn binary(&mut self, f: fn(f64, f64) -> f64) -> i32 {
        if self.s < 2 {
            return -2;
        }
        let a = self.stack[self.s as usize - 2];
        let b = self.stack[self.s as usize - 1];
        self.stack[self.s as usize - 2] = f(a, b);
        self.s -= 1;
        0
    }

    fn input_function(&mut self, token: &str) -> i32 {
        match token {
            "x" => self.delete(),
            "xx" => self.clear_stack(),
            "xxx" => {
                self.init();
                0
            }
            "f" => self.flip(),
            "mc" => self.mem_clear(),
            "mr" => self.mem_recall(),
            "ma" => self.mem_add(),
            "ms" => self.mem_subtract(),
            "a" => self.add(),
            "s" => self.subtract(),
            "n" => self.negate(),
            "m" => self.multiply(),
            "d" => self.divide(),
            "i" => self.invert(),
            "e" => self.e(),
            "pi" => self.pi(),
            "exp" => self.unary(libm::exp),
            "pow" => self.binary(libm::pow),
            "ln" => self.unary(libm::log),
            "log" => self.unary(libm::log10),
            "sqrt" => self.unary(libm::sqrt),
            "sin" | "sn" => self.unary(libm::sin),
            "cos" => self.unary(libm::cos),
            "tan" => self.unary(libm::tan),
            "asin" => self.unary(libm::asin),
            "acos" => self.unary(libm::acos),
            "atan" => self.unary(libm::atan),
            "atan2" => self.binary(libm::atan2),
            "sind" => self.unary(|x| libm::sin(x * core::f64::consts::PI / 180.0)),
            "cosd" => self.unary(|x| libm::cos(x * core::f64::consts::PI / 180.0)),
            "tand" => self.unary(|x| libm::tan(x * core::f64::consts::PI / 180.0)),
            "asind" => self.unary(|x| libm::asin(x) * 180.0 / core::f64::consts::PI),
            "acosd" => self.unary(|x| libm::acos(x) * 180.0 / core::f64::consts::PI),
            "atand" => self.unary(|x| libm::atan(x) * 180.0 / core::f64::consts::PI),
            "atan2d" => self.binary(|a, b| libm::atan2(a, b) * 180.0 / core::f64::consts::PI),
            "tor" => self.unary(|x| x * core::f64::consts::PI / 180.0),
            "tod" => self.unary(|x| x * 180.0 / core::f64::consts::PI),
            _ => -1,
        }
    }

    fn input_float(&mut self, token: &str) -> i32 {
        let mut chars = [0u8; MORSECALC_TOKEN_LEN];
        let n = token.len().min(MORSECALC_TOKEN_LEN);
        for i in 0..n {
            chars[i] = match token.as_bytes()[i] {
                b'e' => b'0',
                b't' => b'1',
                b'n' => b'2',
                b'm' => b'3',
                b'd' => b'4',
                b'k' => b'5',
                b'g' => b'6',
                b'o' => b'7',
                b'b' => b'8',
                b'x' => b'9',
                b'h' => b'.',
                b'C' => b'-',
                b'p' => b'E',
                other => other,
            };
        }
        let s = core::str::from_utf8(&chars[..n]).unwrap_or("");
        let d = parse_float(s);
        if d.is_nan() {
            return -1;
        }
        self.push(d)
    }

    fn input(&mut self, token: &str) -> i32 {
        let retval = self.input_function(token);
        if retval == -1 {
            self.input_float(token)
        } else {
            retval
        }
    }
}

/// A minimal float parser.
fn parse_float(s: &str) -> f64 {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return f64::NAN;
    }
    let mut i = 0;
    let mut neg = false;
    if bytes[0] == b'-' {
        neg = true;
        i += 1;
    }
    let mut mantissa = 0.0f64;
    let mut frac = 0.0f64;
    let mut frac_div = 1.0f64;
    let mut has_digits = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        mantissa = mantissa * 10.0 + (bytes[i] - b'0') as f64;
        has_digits = true;
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            frac = frac * 10.0 + (bytes[i] - b'0') as f64;
            frac_div *= 10.0;
            has_digits = true;
            i += 1;
        }
    }
    if !has_digits {
        return f64::NAN;
    }
    let mut value = mantissa + frac / frac_div;
    if i < bytes.len() && (bytes[i] == b'E' || bytes[i] == b'e') {
        i += 1;
        let mut eneg = false;
        if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
            eneg = bytes[i] == b'-';
            i += 1;
        }
        let mut exp = 0i32;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            exp = exp * 10 + (bytes[i] - b'0') as i32;
            i += 1;
        }
        if eneg {
            exp = -exp;
        }
        value *= libm::pow(10.0, exp as f64);
    }
    if i != bytes.len() {
        return f64::NAN;
    }
    if neg {
        value = -value;
    }
    value
}

/// The morsecalc face state.
pub struct MorsecalcFace {
    cs: CalcState,
    mc: u32,
    token: [u8; MORSECALC_TOKEN_LEN],
    idxt: u8,
    led_is_on: bool,
}

impl MorsecalcFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        MorsecalcFace {
            cs: CalcState::new(),
            mc: 0,
            token: [0; MORSECALC_TOKEN_LEN],
            idxt: 0,
            led_is_on: false,
        }
    }

    pub fn new() -> Self {
        MorsecalcFace::new_static()
    }

    fn reset_token(&mut self) {
        self.token = [0; MORSECALC_TOKEN_LEN];
        self.idxt = 0;
    }

    fn morsecode_input(&mut self, input: u8) {
        if self.mc >= (1u32 << MORSECODE_LEN) - 1 {
            self.mc = 0;
        } else if input == 0 || input == 1 {
            self.mc = self.mc * 2 + input as u32 + 1;
        }
    }

    fn display_float(&self, d: f64) {
        let mut buf = [0u8; 11];
        if d == 0.0 {
            buf[0] = b' ';
            buf[1] = b' ';
            buf[2] = b' ';
            buf[3] = b' ';
            buf[4] = b' ';
            buf[5] = b'0';
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
            return;
        }
        if d.is_nan() {
            buf[0] = b' ';
            buf[1] = b' ';
            buf[2] = b' ';
            buf[3] = b'n';
            buf[4] = b'a';
            buf[5] = b'n';
            slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 4);
            return;
        }
        let mut d = d;
        let is_negative = d < 0.0;
        if is_negative {
            d = -d;
        }
        let mut om = libm::floor(libm::log(d) / libm::log(10.0)) as i32;
        let om_is_negative = om < 0;
        let mut digits = libm::round(d * libm::pow(10.0, 3.0 - om as f64)) as i32;
        if digits > 9999 {
            digits = 1000;
            om += 1;
        }
        if is_negative {
            slcd::set_pixel(0, 11);
            slcd::set_pixel(2, 12);
            slcd::set_pixel(2, 11);
        } else {
            slcd::display_character(b' ', 1);
        }
        if om_is_negative {
            slcd::set_pixel(1, 9);
        } else {
            slcd::display_character(b' ', 2);
        }
        slcd::display_character(b'0' + ((digits / 1000) % 10) as u8, 4);
        slcd::display_character(b'0' + ((digits / 100) % 10) as u8, 5);
        slcd::display_character(b'0' + ((digits / 10) % 10) as u8, 6);
        slcd::display_character(b'0' + (digits % 10) as u8, 7);
        let om = if om_is_negative { -om } else { om };
        if om <= 99 {
            slcd::display_character(b'0' + ((om / 10) % 10) as u8, 8);
            slcd::display_character(b'0' + (om % 10) as u8, 9);
        }
    }

    fn display_token(&self) {
        slcd::display_string("          ", 0);
        let mut c = MORSECODE_TREE[self.mc as usize];
        if c == 0 {
            c = b' ';
        }
        slcd::display_character(c, 0);
        let mut v = self.mc + 1;
        let mut bidx = 0u8;
        while v > 1 {
            v >>= 1;
            bidx += 1;
        }
        slcd::display_character(b'0' + bidx, 3);
        let mut nlen = 0usize;
        while nlen < self.idxt as usize && self.token[nlen] != 0 {
            nlen += 1;
        }
        let nprint = nlen.min(6);
        let start = nlen - nprint;
        for i in 0..nprint {
            slcd::display_character(self.token[start + i], 10 - nprint as u8 + i as u8);
        }
    }

    fn display_stack(&self) {
        slcd::display_string("          ", 0);
        let c = MORSECODE_TREE[self.mc as usize];
        if c == b'm' {
            self.display_float(self.cs.mem);
            slcd::display_character(c, 0);
        } else {
            let mut idx = 0u8;
            if c >= b'0' && c <= b'9' {
                idx = c - b'0';
            }
            if idx >= self.cs.s {
                slcd::display_string(" empty", 4);
            } else {
                self.display_float(self.cs.stack[self.cs.s as usize - 1 - idx as usize]);
            }
            slcd::display_character(b'0' + idx, 0);
        }
        slcd::display_character(b'0' + self.cs.s, 3);
    }

    fn input(&mut self) {
        let mut status = 0;
        let dec = MORSECODE_TREE[self.mc as usize];
        self.mc = 0;
        match dec {
            0 => {
                self.display_token();
            }
            b' ' => {
                if self.idxt > 0 {
                    let token = core::str::from_utf8(&self.token[..self.idxt as usize])
                        .unwrap_or("")
                        .trim_end_matches('\0');
                    status = self.cs.input(token);
                    self.reset_token();
                }
                self.display_stack();
            }
            b'(' => {
                if self.idxt > 0 {
                    self.idxt -= 1;
                    self.token[self.idxt as usize] = 0;
                }
                self.display_token();
            }
            b'S' => {
                self.reset_token();
                self.display_stack();
            }
            _ => {
                if self.idxt < MORSECALC_TOKEN_LEN as u8 - 1 {
                    self.token[self.idxt as usize] = dec;
                    self.idxt = (self.idxt + 1).min(MORSECALC_TOKEN_LEN as u8);
                    self.display_token();
                } else {
                    slcd::display_string("  full", 4);
                }
            }
        }
        match status {
            0 => {}
            -1 => slcd::display_string("cmderr", 4),
            -2 => slcd::display_string("stkerr", 4),
            _ => slcd::display_string("   err", 4),
        }
    }
}

impl WatchFace for MorsecalcFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.mc = 0;
        self.display_stack();
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        match event {
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.morsecode_input(0);
                self.display_token();
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                self.morsecode_input(1);
                self.display_token();
            }
            Event::Button(Button::Mode, ButtonEvent::Up) => {
                if self.mc != 0 || self.idxt != 0 {
                    self.input();
                } else {
                    movement::move_to_next_face();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                self.display_stack();
                self.mc = 0;
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.led_is_on = !self.led_is_on;
                if self.led_is_on {
                    movement::illuminate_led();
                } else {
                    watch::led::set_led_off();
                }
            }
            Event::Button(Button::Mode, ButtonEvent::LongPress) => movement::move_to_next_face(),
            Event::Tick => {
                if self.led_is_on {
                    movement::illuminate_led();
                }
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        self.led_is_on = false;
        watch::led::set_led_off();
    }
}
