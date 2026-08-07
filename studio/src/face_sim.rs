//! Watch-face simulation engine.
//!
//! Reimplements the behavior of the main firmware watch faces so they can be
//! fully used in the Studio simulator. Each face keeps its own state, reacts to
//! button presses, and ticks with the simulated clock. The output is a
//! 10-character LCD buffer (plus indicator flags) rendered using the firmware's
//! real `CHARACTER_SET`, so text displays correctly on the 7-segment LCD.

/// The firmware's 7-segment character set (ASCII - 0x20), bit 0 = segment A ...
/// bit 6 = segment G, bit 7 = DP. Mirrors `src/watch/slcd.rs`.
pub const CHARACTER_SET: [u8; 95] = [
    0b00000000, 0b01100000, 0b00100010, 0b01100011, 0b00101101, 0b00000000, 0b01000100, 0b00100000,
    0b00111001, 0b00001111, 0b11000000, 0b01110000, 0b00000100, 0b01000000, 0b01000000, 0b00010010,
    0b00111111, 0b00000110, 0b01011011, 0b01001111, 0b01100110, 0b01101101, 0b01111101, 0b00000111,
    0b01111111, 0b01101111, 0b00000000, 0b00000000, 0b01011000, 0b01001000, 0b01001100, 0b01010011,
    0b11111111, 0b01110111, 0b01111111, 0b00111001, 0b00111111, 0b01111001, 0b01110001, 0b00111101,
    0b01110110, 0b10001001, 0b00001110, 0b01110101, 0b00111000, 0b10110111, 0b00110111, 0b00111111,
    0b01110011, 0b01100111, 0b11110111, 0b01101101, 0b10000001, 0b00111110, 0b00111110, 0b10111110,
    0b01111110, 0b01101110, 0b00011011, 0b00111001, 0b00100100, 0b00001111, 0b00100011, 0b00001000,
    0b00000010, 0b01011111, 0b01111100, 0b01011000, 0b01011110, 0b01111011, 0b01110001, 0b01101111,
    0b01110100, 0b00010000, 0b01000010, 0b01110101, 0b00110000, 0b10110111, 0b01010100, 0b01011100,
    0b01110011, 0b01100111, 0b01010000, 0b01101101, 0b01111000, 0b01100010, 0b00011100, 0b10111110,
    0b01111110, 0b01101110, 0b00011011, 0b00010110, 0b00110110, 0b00110100, 0b00000001,
];

/// The LCD display state: 10 characters plus indicator flags.
#[derive(Clone, Copy, Debug, Default)]
pub struct FaceDisplay {
    /// The 10 characters to show (position 0..10).
    pub chars: [char; 10],
    /// Whether the colon is on.
    pub colon: bool,
    /// Indicator flags: signal, bell, pm, h24, lap.
    pub signal: bool,
    pub bell: bool,
    pub pm: bool,
    pub h24: bool,
    pub lap: bool,
}

impl FaceDisplay {
    /// Sets the characters from a string, starting at `pos`, blanking the rest.
    pub fn set_string(&mut self, s: &str, pos: usize) {
        self.chars = [' '; 10];
        for (i, c) in s.chars().take(10 - pos).enumerate() {
            self.chars[pos + i] = c;
        }
    }
}

/// The simulated time.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct SimTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub weekday: u32, // 0=Sun
}

/// A button press event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceButton {
    Light,
    Alarm,
}

/// The face engine: holds the current face and its state, and produces the
/// LCD display for the given time.
pub struct FaceEngine {
    /// The current face name.
    pub face_name: String,
    /// Stopwatch state.
    pub sw_running: bool,
    pub sw_seconds: u32,
    /// Timer state (seconds remaining).
    pub timer_seconds: u32,
    pub timer_running: bool,
    /// Counter state.
    pub counter: u32,
    /// Alarm time (seconds since midnight).
    pub alarm_seconds: u32,
    pub alarm_enabled: bool,
    /// World clock offset (minutes from UTC).
    pub world_offset: i32,
    /// Diagnostics face state.
    pub diag_cursor: u8,
    pub diag_screen: u8,
    pub diag_subrow: u8,
    pub diag_prev_screen: u8,
    pub diag_test_active: bool,
    /// Whether the clock shows 24-hour time (false = 12-hour).
    pub time_mode_24: bool,
    /// Power-on time in seconds (uptime), for the diagnostics stats.
    pub power_on_seconds: u32,
    /// Whether the coin-flip face shows heads (true) or tails (false).
    pub coin_heads: bool,
}

impl FaceEngine {
    /// Creates a new engine for the given face.
    pub fn new(face_name: &str) -> Self {
        FaceEngine {
            face_name: face_name.to_string(),
            sw_running: false,
            sw_seconds: 0,
            timer_seconds: 120,
            timer_running: false,
            counter: 0,
            alarm_seconds: 7 * 3600,
            alarm_enabled: false,
            world_offset: 0,
            diag_cursor: 0,
            diag_screen: 10,
            diag_subrow: 0,
            diag_prev_screen: 10,
            diag_test_active: false,
            time_mode_24: true,
            power_on_seconds: 0,
            coin_heads: true,
        }
    }

    /// Handles a button press for the current face.
    pub fn press(&mut self, button: FaceButton) {
        let upper = self.face_name.to_uppercase();
        if upper.contains("STOPWATCH") {
            match button {
                FaceButton::Alarm => self.sw_running = !self.sw_running,
                FaceButton::Light => {
                    self.sw_running = false;
                    self.sw_seconds = 0;
                }
            }
        } else if upper.contains("TIMER") || upper.contains("COUNTDOWN") {
            match button {
                FaceButton::Alarm => {
                    if self.timer_running {
                        self.timer_running = false;
                    } else {
                        self.timer_running = true;
                    }
                }
                FaceButton::Light => {
                    self.timer_running = false;
                    self.timer_seconds = 120;
                }
            }
        } else if upper.contains("COUNTER") {
            match button {
                FaceButton::Alarm => self.counter = self.counter.wrapping_add(1),
                FaceButton::Light => self.counter = 0,
            }
        } else if upper.contains("ALARM") {
            match button {
                FaceButton::Alarm => self.alarm_enabled = !self.alarm_enabled,
                FaceButton::Light => {}
            }
        } else if upper.contains("SIMPLE_COIN_FLIP") || upper.contains("TOSS_UP") {
            // A press flips the coin.
            if button == FaceButton::Alarm {
                self.coin_heads = !self.coin_heads;
            }
        } else if upper.contains("DIAGNOSTICS") || upper.contains("SETTINGS") {
            self.diag_press(button);
        }
    }

    /// Handles a button press for the diagnostics/settings face.
    fn diag_press(&mut self, button: FaceButton) {
        // If a button test is active, show which button was pressed.
        if self.diag_test_active {
            self.diag_test_active = false;
            return;
        }
        match button {
            // Top-left (Light): scroll down one row / move the cursor.
            FaceButton::Light => {
                if self.diag_screen == 10 {
                    self.diag_cursor = (self.diag_cursor + 1) % 10;
                } else if matches!(self.diag_screen, 6 | 7 | 8 | 9) {
                    let max = if self.diag_screen == 9 {
                        8
                    } else if self.diag_screen == 7 {
                        4
                    } else {
                        3
                    };
                    self.diag_subrow = (self.diag_subrow + 1) % max;
                }
            }
            // Bottom-right (Alarm): select / enter / exit / toggle.
            FaceButton::Alarm => {
                if self.diag_screen == 10 {
                    self.diag_prev_screen = self.diag_screen;
                    self.diag_screen = self.diag_cursor;
                    self.diag_subrow = 0;
                } else if self.diag_screen == 9 {
                    // Run the selected test.
                    self.diag_test_active = true;
                } else {
                    // Exit back to the main menu.
                    self.diag_screen = 10;
                }
            }
        }
    }

    /// Advances the face state by one second (called on each tick).
    pub fn tick(&mut self) {
        self.power_on_seconds = self.power_on_seconds.wrapping_add(1);
        let upper = self.face_name.to_uppercase();
        if upper.contains("STOPWATCH") && self.sw_running {
            self.sw_seconds = (self.sw_seconds + 1) % 3_600_000;
        } else if (upper.contains("TIMER") || upper.contains("COUNTDOWN")) && self.timer_running {
            if self.timer_seconds > 0 {
                self.timer_seconds -= 1;
            } else {
                self.timer_running = false;
            }
        }
    }

    /// Renders the current face to an LCD display for the given time.
    pub fn render(&self, time: &SimTime) -> FaceDisplay {
        let upper = self.face_name.to_uppercase();
        let mut d = FaceDisplay::default();

        if upper.contains("SIMPLE_CLOCK") || upper.contains("CLOCK") {
            render_clock(&mut d, time, self.time_mode_24);
        } else if upper.contains("STOPWATCH") {
            render_stopwatch(&mut d, self.sw_seconds);
        } else if upper.contains("TIMER") {
            render_timer(&mut d, self.timer_seconds);
        } else if upper.contains("COUNTDOWN") {
            render_countdown(&mut d, self.timer_seconds);
        } else if upper.contains("ALARM") {
            render_alarm(&mut d, self.alarm_seconds, self.alarm_enabled);
        } else if upper.contains("WORLD_CLOCK") {
            render_world_clock(&mut d, time, self.world_offset);
        } else if upper.contains("COUNTER") {
            render_counter(&mut d, self.counter);
        } else if upper.contains("FLASHLIGHT") {
            // Flashlight: show LIGHT in the main display.
            d.set_string("LIGHT", 2);
        } else if upper.contains("SIMPLE_COIN_FLIP") || upper.contains("TOSS_UP") {
            render_coin_flip(&mut d, self.coin_heads);
        } else if upper.contains("MOON_PHASE") {
            d.set_string("MOON", 3);
        } else if upper.contains("DIAGNOSTICS") || upper.contains("SETTINGS") {
            self.render_diag(&mut d, time);
        } else {
            let short: String = self.face_name.chars().take(10).collect();
            d.set_string(&short, 0);
        }
        d
    }

    /// Renders the diagnostics/settings face.
    fn render_diag(&self, d: &mut FaceDisplay, time: &SimTime) {
        const MENU_ITEMS: [&str; 10] = [
            "CPU   ", "MEMORY", "STORAG", "HARDWR", "SOFTWR", "SYSTEM", "SETTNG", "STATS ",
            "BATTER", "TEST  ",
        ];
        const TEST_ROWS: [&str; 8] = [
            "BTN   ", "LED   ", "BUZZER", "ACCEL ", "CPU   ", "RAM   ", "STORAG", "BENCH ",
        ];

        // Blank the display first so unused positions are spaces, not NUL.
        d.chars = [' '; 10];

        // Breadcrumb: positions 0-1 = depth, 2-3 = face index.
        let depth = if self.diag_screen == 10 {
            0
        } else if matches!(self.diag_screen, 6 | 7 | 8 | 9) {
            2
        } else {
            1
        };
        d.chars[0] = (b'0' + depth / 10) as char;
        d.chars[1] = (b'0' + depth % 10) as char;

        match self.diag_screen {
            10 => {
                // Main menu: show the selected category with a cursor.
                let item = MENU_ITEMS[(self.diag_cursor as usize).min(9)];
                d.chars[3] = '>';
                for (i, c) in item.chars().take(6).enumerate() {
                    d.chars[4 + i] = c;
                }
            }
            0 => {
                let s = "CPU  CTXM0";
                d.set_string(s, 0);
            }
            1 => {
                let s = "MEMORY32KB";
                d.set_string(s, 0);
            }
            2 => {
                let s = "STORAG8KB";
                d.set_string(s, 0);
            }
            3 => {
                // Hardware: use the short form for legibility on 7-segment.
                let s = "HW  SAML22";
                d.set_string(s, 0);
            }
            4 => {
                // Software: use the short form for legibility on 7-segment.
                let s = "SW  RUST";
                d.set_string(s, 0);
            }
            5 => {
                // System: show the current time.
                let (h2, h1) = two_digits(time.hour);
                let (m2, m1) = two_digits(time.minute);
                d.chars[0] = 'S';
                d.chars[1] = 'Y';
                d.chars[2] = 'S';
                d.chars[3] = 'T';
                d.chars[4] = 'E';
                d.chars[5] = 'M';
                d.chars[6] = h2;
                d.chars[7] = h1;
                d.chars[8] = m2;
                d.chars[9] = m1;
            }
            6 => {
                // Settings: LED color / buzzer voltage / power.
                match self.diag_subrow {
                    0 => d.set_string("LED   GREEN", 0),
                    1 => d.set_string("BUZZER 9.0V", 0),
                    _ => d.set_string("POWER  OFF", 0),
                }
            }
            7 => {
                // Stats.
                match self.diag_subrow {
                    0 => d.set_string("LIGHT  0000", 0),
                    1 => d.set_string("MODE   0000", 0),
                    2 => d.set_string("BUZZER 0000", 0),
                    _ => {
                        // Power-on time (uptime) as HHMMSS.
                        let s = self.power_on_seconds;
                        let h = s / 3600;
                        let m = (s % 3600) / 60;
                        let sec = s % 60;
                        let (h2, h1) = two_digits(h % 100);
                        let (m2, m1) = two_digits(m);
                        let (s2, s1) = two_digits(sec);
                        d.chars = ['P', 'W', 'R', ' ', h2, h1, m2, m1, s2, s1];
                    }
                }
            }
            8 => {
                // Battery.
                match self.diag_subrow {
                    0 => d.set_string("TYPE  CR2016", 0),
                    1 => d.set_string("CHARGE 100%", 0),
                    _ => d.set_string("DAYS   1000", 0),
                }
            }
            9 => {
                // Test submenu.
                let row = TEST_ROWS[(self.diag_subrow as usize).min(7)];
                d.set_string(row, 0);
                if self.diag_test_active {
                    d.set_string("RUNNING", 4);
                }
            }
            _ => {}
        }
    }
}

fn two_digits(v: u32) -> (char, char) {
    (
        (b'0' + (v / 10) as u8) as char,
        (b'0' + (v % 10) as u8) as char,
    )
}

fn render_clock(d: &mut FaceDisplay, time: &SimTime, time_mode_24: bool) {
    let wd = ["SU", "MO", "TU", "WE", "TH", "FR", "SA"][(time.weekday % 7) as usize];
    let (d2, d1) = two_digits(time.day);
    // Apply 12-hour conversion if needed.
    let mut hour = time.hour;
    let mut pm = false;
    if !time_mode_24 {
        pm = hour >= 12;
        hour %= 12;
        if hour == 0 {
            hour = 12;
        }
    }
    let (h2, h1) = two_digits(hour);
    let (m2, m1) = two_digits(time.minute);
    let (s2, s1) = two_digits(time.second);
    d.chars = [
        wd.chars().nth(0).unwrap_or(' '),
        wd.chars().nth(1).unwrap_or(' '),
        d2,
        d1,
        h2,
        h1,
        m2,
        m1,
        s2,
        s1,
    ];
    d.colon = true;
    d.h24 = time_mode_24;
    d.pm = pm;
}

fn render_stopwatch(d: &mut FaceDisplay, seconds: u32) {
    let (m2, m1) = two_digits((seconds / 60) % 100);
    let (s2, s1) = two_digits(seconds % 60);
    d.chars = ['S', 'T', ' ', ' ', m2, m1, s2, s1, ' ', ' '];
    d.colon = true;
}

fn render_timer(d: &mut FaceDisplay, seconds: u32) {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    let (h2, h1) = two_digits(h);
    let (m2, m1) = two_digits(m);
    let (s2, s1) = two_digits(s);
    d.chars = ['1', ' ', h2, h1, m2, m1, s2, s1, ' ', ' '];
    d.colon = true;
}

fn render_countdown(d: &mut FaceDisplay, seconds: u32) {
    let m = seconds / 60;
    let s = seconds % 60;
    let (m2, m1) = two_digits(m);
    let (s2, s1) = two_digits(s);
    d.chars = ['C', 'D', ' ', ' ', m2, m1, s2, s1, ' ', ' '];
    d.colon = true;
}

fn render_alarm(d: &mut FaceDisplay, alarm_seconds: u32, enabled: bool) {
    let h = alarm_seconds / 3600;
    let m = (alarm_seconds % 3600) / 60;
    let (h2, h1) = two_digits(h);
    let (m2, m1) = two_digits(m);
    d.chars = ['A', 'L', ' ', ' ', h2, h1, m2, m1, ' ', ' '];
    d.colon = true;
    d.bell = enabled;
}

fn render_world_clock(d: &mut FaceDisplay, time: &SimTime, offset: i32) {
    let mut h = time.hour as i32 + offset / 60;
    let mut m = time.minute as i32 + offset % 60;
    if m < 0 {
        m += 60;
        h -= 1;
    }
    if m >= 60 {
        m -= 60;
        h += 1;
    }
    h = h.rem_euclid(24);
    let (h2, h1) = two_digits(h as u32);
    let (m2, m1) = two_digits(m as u32);
    d.chars = ['W', 'C', ' ', ' ', h2, h1, m2, m1, ' ', ' '];
    d.colon = true;
}

fn render_counter(d: &mut FaceDisplay, counter: u32) {
    let c = counter.min(999_999);
    let (a, b, c2, d2, e, f) = (
        (b'0' + (c / 100_000) as u8) as char,
        (b'0' + ((c / 10_000) % 10) as u8) as char,
        (b'0' + ((c / 1000) % 10) as u8) as char,
        (b'0' + ((c / 100) % 10) as u8) as char,
        (b'0' + ((c / 10) % 10) as u8) as char,
        (b'0' + (c % 10) as u8) as char,
    );
    d.chars = ['C', 'T', ' ', a, b, c2, d2, e, f, ' '];
}

fn render_coin_flip(d: &mut FaceDisplay, heads: bool) {
    if heads {
        d.set_string("HEADS", 2);
    } else {
        d.set_string("TAILS", 2);
    }
}
