//! Randonaut watch face.
//!
//! Port of the C `randonaut_face.c`. Generates a random "blindspot" point near
//! the current location using the Randonautica algorithm. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::slcd::Indicator;

const R: f64 = 6371000.0;
const PI: f64 = core::f64::consts::PI;

/// A generated point.
struct Point {
    latitude: i32,
    longitude: i32,
    distance: u32,
    bearing: u16,
}

/// The randonaut face state.
pub struct RandonautFace {
    mode: u8,
    location_format: u8,
    radius: u16,
    rng: u8,
    chance: bool,
    quantum: bool,
    entropy: u32,
    point: Point,
    location: (i32, i32),
}

impl RandonautFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        RandonautFace {
            mode: 0,
            location_format: 0,
            radius: 1000,
            rng: 0,
            chance: true,
            quantum: false,
            entropy: 0,
            point: Point {
                latitude: 0,
                longitude: 0,
                distance: 0,
                bearing: 0,
            },
            location: (0, 0),
        }
    }

    pub fn new() -> Self {
        RandonautFace::new_static()
    }

    fn get_pseudo_entropy(&self, max: u32) -> u32 {
        let now = crate::watch::rtc::get_date_time();
        let mut x = now.to_reg();
        if x == 0 {
            x = 0x1234_5678;
        }
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x % max
    }

    fn get_entropy(&mut self) {
        if self.chance {
            self.quantum = self.entropy % 2 != 0;
        }
        loop {
            if !self.quantum {
                self.entropy = self.get_pseudo_entropy(i32::MAX as u32);
            } else {
                self.entropy = self.get_pseudo_entropy(i32::MAX as u32);
            }
            if self.entropy < i32::MAX as u32 && self.entropy > 0 {
                break;
            }
        }
        self.entropy %= i32::MAX as u32;
    }

    fn generate_blindspot(&mut self) {
        self.get_entropy();
        let lat = self.location.0 as f64 / 100000.0;
        let lon = self.location.1 as f64 / 100000.0;
        let radius = self.radius as f64;

        let random_distance = radius * libm::sqrt(self.entropy as f64 / i32::MAX as f64) / 1000.0;
        let random_bearing = 2.0 * PI * self.entropy as f64 / i32::MAX as f64;

        let phi = lat * PI / 180.0;
        let lambda = lon * PI / 180.0;
        let alpha = random_distance / R;

        let new_lat = libm::asin(
            libm::sin(phi) * libm::cos(alpha)
                + libm::cos(phi) * libm::sin(alpha) * libm::cos(random_bearing),
        );
        let new_lon = lambda
            + libm::atan2(
                libm::sin(random_bearing) * libm::sin(alpha) * libm::cos(phi),
                libm::cos(alpha) - libm::sin(phi) * libm::sin(new_lat),
            );

        self.point.latitude = libm::round(new_lat * (180.0 / PI) * 100000.0) as i32;
        self.point.longitude = libm::round(new_lon * (180.0 / PI) * 100000.0) as i32;
        self.point.distance = (random_distance * 1000.0) as u32;
        let bearing = random_bearing * (180.0 / PI);
        self.point.bearing = if bearing < 0.0 {
            libm::round(bearing + 360.0) as u16
        } else {
            libm::round(bearing) as u16
        };
    }

    fn display(&mut self) {
        let mut buf = [0u8; 11];
        watch::slcd::clear_colon();
        match self.mode {
            0 => {
                buf[0] = b'R';
                buf[1] = b'A';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b'R';
                buf[5] = b'a';
                buf[6] = b'n';
                buf[7] = b'd';
                buf[8] = b'o';
                buf[9] = b' ';
            }
            1 => {
                self.generate_blindspot();
                watch::slcd::clear_display();
                self.mode = 2;
                self.location_format = 1;
                buf[0] = b'R';
                buf[1] = b'A';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b'F';
                buf[5] = b'o';
                buf[6] = b'u';
                buf[7] = b'n';
                buf[8] = b'd';
                buf[9] = b' ';
            }
            2 => match self.location_format {
                0 => {
                    buf[0] = b'R';
                    buf[1] = b'A';
                    buf[2] = b' ';
                    buf[3] = b' ';
                    buf[4] = b'P';
                    buf[5] = b'o';
                    buf[6] = b'i';
                    buf[7] = b'n';
                    buf[8] = b't';
                    buf[9] = b' ';
                }
                1 => {
                    watch::slcd::clear_display();
                    buf[0] = b'D';
                    buf[1] = b'I';
                    buf[2] = b' ';
                    buf[3] = b'm';
                    buf[4] = b' ';
                    write_num(&mut buf, self.point.distance, 5, 5);
                }
                2 => {
                    watch::slcd::clear_display();
                    buf[0] = b'B';
                    buf[1] = b'E';
                    buf[2] = b' ';
                    buf[3] = b'#';
                    buf[4] = b' ';
                    write_num(&mut buf, self.point.bearing as u32, 5, 5);
                }
                3 => {
                    let lat = self.point.latitude.unsigned_abs();
                    buf[0] = b'L';
                    buf[1] = b'A';
                    buf[2] = b' ';
                    buf[3] = b'#';
                    buf[4] = if self.point.latitude < 0 { b'-' } else { b'+' };
                    buf[5] = b'0' + ((lat / 100000) % 10) as u8;
                    buf[6] = b'0' + ((lat / 10000) % 10) as u8;
                    buf[7] = b' ';
                    buf[8] = b' ';
                    buf[9] = b' ';
                }
                4 => {
                    let lat = self.point.latitude.unsigned_abs();
                    buf[0] = b'L';
                    buf[1] = b'A';
                    buf[2] = b' ';
                    buf[3] = b',';
                    buf[4] = b' ';
                    buf[5] = b'0' + ((lat / 1000) % 10) as u8;
                    buf[6] = b'0' + ((lat / 100) % 10) as u8;
                    buf[7] = b'0' + ((lat / 10) % 10) as u8;
                    buf[8] = b'0' + (lat % 10) as u8;
                    buf[9] = b' ';
                }
                5 => {
                    let lon = self.point.longitude.unsigned_abs();
                    buf[0] = b'L';
                    buf[1] = b'O';
                    buf[2] = b' ';
                    buf[3] = b'#';
                    buf[4] = if self.point.longitude < 0 { b'-' } else { b'+' };
                    buf[5] = b'0' + ((lon / 100000) % 10) as u8;
                    buf[6] = b'0' + ((lon / 10000) % 10) as u8;
                    buf[7] = b'0' + ((lon / 1000) % 10) as u8;
                    buf[8] = b' ';
                    buf[9] = b' ';
                }
                _ => {
                    let lon = self.point.longitude.unsigned_abs();
                    buf[0] = b'L';
                    buf[1] = b'O';
                    buf[2] = b' ';
                    buf[3] = b',';
                    buf[4] = b' ';
                    buf[5] = b'0' + ((lon / 100) % 10) as u8;
                    buf[6] = b'0' + ((lon / 10) % 10) as u8;
                    buf[7] = b'0' + (lon % 10) as u8;
                    buf[8] = b' ';
                    buf[9] = b' ';
                }
            },
            3 => {
                watch::slcd::set_colon();
                buf[0] = b'R';
                buf[1] = b'A';
                buf[2] = b' ';
                buf[3] = b'm';
                buf[4] = b' ';
                write_num(&mut buf, self.radius as u32, 5, 5);
            }
            4 => {
                buf[0] = b'R';
                buf[1] = b'N';
                buf[2] = b' ';
                buf[3] = b'G';
                buf[4] = b' ';
                if self.chance {
                    buf[5] = b'C';
                    buf[6] = b'h';
                    buf[7] = b'n';
                    buf[8] = b'c';
                    buf[9] = b'e';
                } else if self.quantum {
                    buf[5] = b'T';
                    buf[6] = b'r';
                    buf[7] = b'u';
                    buf[8] = b'e';
                    buf[9] = b' ';
                } else {
                    buf[5] = b'P';
                    buf[6] = b's';
                    buf[7] = b'u';
                    buf[8] = b'd';
                    buf[9] = b'o';
                }
            }
            _ => {
                buf[0] = b'W';
                buf[1] = b'R';
                buf[2] = b' ';
                buf[3] = b' ';
                buf[4] = b'F';
                buf[5] = b'i';
                buf[6] = b'l';
                buf[7] = b'e';
                buf[8] = b' ';
                buf[9] = b' ';
            }
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
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

impl WatchFace for RandonautFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.mode = 0;
        self.radius = 1000;
        self.get_entropy();
        self.chance = true;
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Activate => {}
            Event::Tick => {}
            Event::Button(Button::Light, ButtonEvent::Down) => {}
            Event::Button(Button::Light, ButtonEvent::Up) => match self.mode {
                0 => {
                    self.mode = 2;
                    self.location_format = 0;
                }
                1 => self.mode = 0,
                2 => self.mode = 0,
                3 => self.mode = 4,
                4 => self.mode = 3,
                _ => {}
            },
            Event::Button(Button::Light, ButtonEvent::LongPress) => match self.mode {
                3 | 4 => self.mode = 0,
                _ => {
                    self.mode = 3;
                    watch::slcd::clear_display();
                }
            },
            Event::Button(Button::Alarm, ButtonEvent::Up) => match self.mode {
                0 => self.mode = 1,
                2 => {
                    self.location_format = (self.location_format + 1) % 7;
                    if self.location_format == 0 {
                        self.location_format += 1;
                    }
                }
                3 => {
                    self.radius += 500;
                    if self.radius > 10000 {
                        self.radius = 1000;
                    }
                }
                4 => {
                    self.rng = (self.rng + 1) % 3;
                    match self.rng {
                        0 => self.chance = true,
                        1 => {
                            self.chance = false;
                            self.quantum = true;
                        }
                        _ => {
                            self.chance = false;
                            self.quantum = false;
                        }
                    }
                }
                5 => {
                    watch::slcd::set_indicator(Indicator::Signal);
                }
                _ => {}
            },
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.mode == 5 {
                    self.mode = 0;
                } else {
                    self.mode = 5;
                }
            }
            _ => movement::default_loop_handler(event, _settings),
        }
        self.display();
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
