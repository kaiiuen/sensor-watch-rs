//! Lightmeter watch face.
//!
//! Port of the C `lightmeter_face.c`. Uses the OPT3001 light sensor (requires
//! the optional sensor) to compute exposure values. It is a pure state
//! machine: it reacts to a single event and returns; it never keeps the CPU
//! awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::opt3001::Opt3001;
use crate::watch::slcd::Indicator;

const LIGHTMETER_CALIBRATION: f32 = 0.0;
const LIGHTMETER_ISO_100: u8 = 0;
const LIGHTMETER_AP_4P0: u8 = 0;

const ISOS: [(&str, f32); 6] = [
    ("ISO100", 0.0),
    ("ISO200", 1.0),
    ("ISO400", 2.0),
    ("ISO800", 3.0),
    ("ISO1600", 4.0),
    ("ISO3200", 5.0),
];

const APS: [(&str, f32); 8] = [
    ("f/1.4", 0.0),
    ("f/2.0", 1.0),
    ("f/2.8", 2.0),
    ("f/4.0", 3.0),
    ("f/5.6", 4.0),
    ("f/8.0", 5.0),
    ("f/11", 6.0),
    ("f/16", 7.0),
];

const SHS: [(&str, f32); 12] = [
    ("1/8000", -13.0),
    ("1/4000", -12.0),
    ("1/2000", -11.0),
    ("1/1000", -10.0),
    ("1/500", -9.0),
    ("1/250", -8.0),
    ("1/125", -7.0),
    ("1/60", -6.0),
    ("1/30", -5.0),
    ("1/15", -4.0),
    ("1/8", -3.0),
    ("1/4", -2.0),
];

/// The lightmeter face state.
pub struct LightmeterFace {
    waiting_for_conversion: u8,
    lux: f32,
    sensor: Opt3001,
    sensor_available: bool,
    mode: u8,
    iso: u8,
    ap: u8,
}

impl LightmeterFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        LightmeterFace {
            waiting_for_conversion: 0,
            lux: 0.0,
            sensor: Opt3001::new(),
            sensor_available: false,
            mode: 0,
            iso: LIGHTMETER_ISO_100,
            ap: LIGHTMETER_AP_4P0,
        }
    }

    pub fn new() -> Self {
        LightmeterFace::new_static()
    }

    fn mod_u16(m: u16, n: u16) -> u16 {
        (m % n + n) % n
    }

    fn show_ev(&self) {
        if !self.sensor_available {
            watch::slcd::display_string("NO LS", 0);
            return;
        }
        let ev = (libm::log2f(self.lux) + ISOS[self.iso as usize].1 + LIGHTMETER_CALIBRATION)
            .clamp(-9.0, 99.0);
        let evt = libm::roundf(2.0 * ev) as i32;
        let mut buf = [0u8; 11];
        buf[0] = b'E';
        buf[1] = b'V';
        buf[2] = b' ';
        buf[3] = b' ';
        buf[4] = b' ';
        buf[5] = b' ';
        buf[6] = b' ';
        buf[7] = b' ';
        buf[8] = b' ';
        buf[9] = b' ';
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 0);
        let whole = (evt / 2).unsigned_abs();
        let mut sb = [0u8; 3];
        sb[0] = b'0' + (whole / 10) as u8;
        sb[1] = b'0' + (whole % 10) as u8;
        watch::slcd::display_string(core::str::from_utf8(&sb[..2]).unwrap_or("  "), 2);
        if evt % 2 != 0 {
            watch::slcd::set_indicator(Indicator::Lap);
        }
        if ev < 0.0 {
            watch::slcd::set_pixel(1, 9);
        }
        if self.mode == 1 {
            let lux = self.lux.min(999999.0);
            let mut lb = [0u8; 7];
            write_num(&mut lb, lux as u32, 0, 6);
            watch::slcd::display_string(core::str::from_utf8(&lb[..]).unwrap_or(""), 4);
            return;
        }
        let comp_ev = ev + APS[self.ap as usize].1;
        let mut bestsh = 0usize;
        let mut besterr = f32::INFINITY;
        for (ind, sh) in SHS.iter().enumerate() {
            let errbuf = comp_ev + sh.1;
            if errbuf.abs() < besterr.abs() {
                besterr = errbuf;
                bestsh = ind;
            }
        }
        if besterr >= 0.5 {
            watch::slcd::display_string(SHS[0].0, 4);
        } else if besterr <= -0.5 {
            watch::slcd::display_string(SHS[11].0, 4);
        } else {
            watch::slcd::display_string(SHS[bestsh].0, 4);
        }
        watch::slcd::display_string(APS[self.ap as usize].0, 7);
    }
}

/// Writes a number right-aligned into the buffer at the given offset.
fn write_num(buf: &mut [u8; 7], value: u32, offset: usize, width: usize) {
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

impl WatchFace for LightmeterFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {}

    fn activate(&mut self, _settings: &Settings) {
        self.waiting_for_conversion = 0;
        self.sensor_available = self.sensor.begin().is_ok();
        self.show_ev();
    }

    fn loop_(&mut self, event: Event, _settings: &mut Settings) {
        match event {
            Event::Tick => {
                if self.waiting_for_conversion != 0 {
                    self.sensor.tick();
                    self.waiting_for_conversion = 0;
                    match self.sensor.read_lux() {
                        Ok(lux) => {
                            self.lux = lux;
                            self.show_ev();
                        }
                        Err(_) => {
                            self.sensor_available = false;
                            self.show_ev();
                        }
                    }
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Up) => {
                self.ap = Self::mod_u16(self.ap as u16 + 1, APS.len() as u16) as u8;
                self.show_ev();
            }
            Event::Button(Button::Light, ButtonEvent::Up) => {
                if self.ap == 0 {
                    self.ap = APS.len() as u8 - 1;
                } else {
                    self.ap = Self::mod_u16(self.ap as u16 - 1, APS.len() as u16) as u8;
                }
                self.show_ev();
            }
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.iso = Self::mod_u16(self.iso as u16 + 1, ISOS.len() as u16) as u8;
                watch::slcd::display_string("EV  ", 0);
                watch::slcd::display_string(ISOS[self.iso as usize].0, 4);
            }
            Event::Button(Button::Alarm, ButtonEvent::LongPress) => {
                if self.sensor.start_conversion().is_err() {
                    self.sensor_available = false;
                    self.show_ev();
                    return;
                }
                self.waiting_for_conversion = 1;
                watch::slcd::display_string("EV  ", 0);
                watch::slcd::display_string(ISOS[self.iso as usize].0, 4);
                watch::slcd::set_indicator(Indicator::Signal);
            }
            Event::Button(Button::Mode, ButtonEvent::LongPress) => {
                self.mode = if self.mode == 0 { 1 } else { 0 };
                self.show_ev();
            }
            _ => movement::default_loop_handler(event, _settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {}
}
