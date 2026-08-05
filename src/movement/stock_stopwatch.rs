//! Stock stopwatch watch face.
//!
//! Port of the C `stock_stopwatch_face.c`. Implements the original F-91W
//! stopwatch functionality including hundredths of seconds and lap timing,
//! driven by a 128 Hz TC2 hardware counter. It is a pure state machine: it
//! reacts to a single event and returns; it never keeps the CPU awake.

use crate::movement;
use crate::movement::types::{Button, ButtonEvent, Event, Settings, WatchFace};
use crate::watch;
use crate::watch::buzzer::Note;
use crate::watch::rtc::DateTime;
use crate::watch::slcd::Indicator;

/// A distant-future date used to keep the watch awake while running.
const DISTANT_FUTURE: DateTime = DateTime {
    second: 0,
    minute: 0,
    hour: 0,
    day: 1,
    month: 1,
    year: 63,
};

/// The 128 Hz tick counter (incremented by the TC2 interrupt).
static mut TICKS: u32 = 0;
static mut IS_RUNNING: bool = false;

/// Returns a reference to the TC2 COUNT8 register block.
fn tc2() -> &'static atsaml22j::tc0::count8::Count8 {
    // SAFETY: the TC2 register block lives at a fixed address for the whole
    // program.
    unsafe { (*atsaml22j::Tc2::PTR).count8() }
}

fn gclk() -> &'static atsaml22j::gclk::RegisterBlock {
    // SAFETY: the GCLK register block lives at a fixed address.
    unsafe { &*atsaml22j::Gclk::PTR }
}

fn mclk() -> &'static atsaml22j::mclk::RegisterBlock {
    // SAFETY: the MCLK register block lives at a fixed address.
    unsafe { &*atsaml22j::Mclk::PTR }
}

fn tc2_sync() {
    while tc2().syncbusy().read().bits() != 0 {}
}

fn cb_start() {
    // SAFETY: enabling a valid timer peripheral.
    unsafe {
        tc2().ctrla().modify(|_, w| w.enable().set_bit());
        IS_RUNNING = true;
    }
}

fn cb_stop() {
    // SAFETY: disabling a valid timer peripheral.
    unsafe {
        tc2().ctrla().modify(|_, w| w.enable().clear_bit());
        IS_RUNNING = false;
    }
}

fn cb_initialize() {
    // SAFETY: configuring a valid timer peripheral.
    unsafe {
        mclk().apbcmask().modify(|_, w| w.tc2_().set_bit());
        gclk()
            .pchctrl(24)
            .write(|w| w.r#gen().gclk3().chen().set_bit());
        cb_stop();
        tc2().ctrla().write(|w| w.swrst().set_bit());
        tc2_sync();
        tc2().ctrla().modify(|_, w| {
            w.prescaler()
                .variant(atsaml22j::tc0::count8::ctrla::Prescalerselect::Div64);
            w.mode()
                .variant(atsaml22j::tc0::count8::ctrla::Modeselect::Count8);
            w.runstdby().set_bit()
        });
        // 32 kHz / 64 / 4 = 128 Hz.
        tc2().per().write(|w| w.bits(3));
        tc2().intenset().modify(|_, w| w.ovf().set_bit());
        cortex_m::peripheral::NVIC::unpend(atsaml22j::Interrupt::TC2);
        cortex_m::peripheral::NVIC::unmask(atsaml22j::Interrupt::TC2);
    }
}

/// The TC2 interrupt handler (128 Hz).
#[unsafe(no_mangle)]
pub extern "C" fn TC2() {
    unsafe {
        TICKS += 1;
    }
    tc2().intflag().write(|w| w.ovf().set_bit());
}

/// The stock stopwatch face state.
pub struct StockStopwatchFace {
    lap_ticks: u32,
    blink_ticks: u8,
    old_seconds: u32,
    old_minutes: u8,
    hours: u8,
    colon: bool,
    light_on_button: bool,
}

impl StockStopwatchFace {
    /// A const constructor for use in a static initializer.
    pub const fn new_static() -> Self {
        StockStopwatchFace {
            lap_ticks: 0,
            blink_ticks: 0,
            old_seconds: 0,
            old_minutes: 0,
            hours: 0,
            colon: false,
            light_on_button: true,
        }
    }

    pub fn new() -> Self {
        StockStopwatchFace::new_static()
    }

    fn button_beep(&self, settings: &Settings) {
        if settings.button_should_sound() {
            crate::movement::play_alarm_beeps(1, Note::C7);
        }
    }

    fn display_ticks(&self, ticks: u32) {
        let mut buf = [0u8; 11];
        let sec_100 = ((ticks & 0x7F) * 100 / 128) as u8;
        let seconds = ticks >> 7;
        let minutes = seconds / 60;
        if self.hours != 0 {
            buf[0] = b'0' + self.hours / 10;
            buf[1] = b'0' + self.hours % 10;
            buf[2] = b'0' + (minutes / 10) as u8;
            buf[3] = b'0' + (minutes % 10) as u8;
            buf[4] = b'0' + ((seconds % 60) / 10) as u8;
            buf[5] = b'0' + ((seconds % 60) % 10) as u8;
            buf[6] = b'0' + sec_100 / 10;
            buf[7] = b'0' + sec_100 % 10;
        } else {
            buf[0] = b' ';
            buf[1] = b' ';
            buf[2] = b'0' + (minutes / 10) as u8;
            buf[3] = b'0' + (minutes % 10) as u8;
            buf[4] = b'0' + ((seconds % 60) / 10) as u8;
            buf[5] = b'0' + ((seconds % 60) % 10) as u8;
            buf[6] = b'0' + sec_100 / 10;
            buf[7] = b'0' + sec_100 % 10;
        }
        watch::slcd::display_string(core::str::from_utf8(&buf[..]).unwrap_or(""), 2);
    }

    fn draw(&mut self) {
        let ticks = unsafe { TICKS };
        if self.lap_ticks == 0 {
            let sec_100 = ((ticks & 0x7F) * 100 / 128) as u8;
            if unsafe { IS_RUNNING } {
                let seconds = ticks >> 7;
                if seconds != self.old_seconds {
                    self.old_seconds = seconds;
                    let mut minutes = seconds / 60;
                    let seconds = seconds % 60;
                    if minutes != self.old_minutes as u32 {
                        self.old_minutes = (minutes % 60) as u8;
                        minutes %= 60;
                        let mut buf = [0u8; 11];
                        if self.hours != 0 {
                            buf[0] = b'0' + self.hours / 10;
                            buf[1] = b'0' + self.hours % 10;
                            buf[2] = b'0' + (minutes / 10) as u8;
                            buf[3] = b'0' + (minutes % 10) as u8;
                            buf[4] = b'0' + (seconds / 10) as u8;
                            buf[5] = b'0' + (seconds % 10) as u8;
                            buf[6] = b'0' + sec_100 / 10;
                            buf[7] = b'0' + sec_100 % 10;
                        } else {
                            buf[0] = b' ';
                            buf[1] = b' ';
                            buf[2] = b'0' + (minutes / 10) as u8;
                            buf[3] = b'0' + (minutes % 10) as u8;
                            buf[4] = b'0' + (seconds / 10) as u8;
                            buf[5] = b'0' + (seconds % 10) as u8;
                            buf[6] = b'0' + sec_100 / 10;
                            buf[7] = b'0' + sec_100 % 10;
                        }
                        watch::slcd::display_string(
                            core::str::from_utf8(&buf[..]).unwrap_or(""),
                            2,
                        );
                    } else {
                        let mut buf = [0u8; 5];
                        buf[0] = b'0' + (seconds / 10) as u8;
                        buf[1] = b'0' + (seconds % 10) as u8;
                        buf[2] = b'0' + sec_100 / 10;
                        buf[3] = b'0' + sec_100 % 10;
                        watch::slcd::display_string(
                            core::str::from_utf8(&buf[..4]).unwrap_or(""),
                            6,
                        );
                    }
                } else {
                    let mut buf = [0u8; 3];
                    buf[0] = b'0' + sec_100 / 10;
                    buf[1] = b'0' + sec_100 % 10;
                    watch::slcd::display_string(core::str::from_utf8(&buf[..2]).unwrap_or("  "), 8);
                }
            } else {
                self.display_ticks(ticks);
            }
        }
        if unsafe { IS_RUNNING } {
            let blink_ticks = ((ticks >> 6) & 1) as u8;
            if blink_ticks != self.blink_ticks {
                self.blink_ticks = blink_ticks;
                self.colon = !self.colon;
                if self.colon {
                    watch::slcd::set_colon();
                } else {
                    watch::slcd::clear_colon();
                }
            }
        }
    }

    fn update_lap_indicator(&self) {
        if self.lap_ticks != 0 {
            watch::slcd::set_indicator(Indicator::Lap);
        } else {
            watch::slcd::clear_indicator(Indicator::Lap);
        }
    }

    fn set_colon(&mut self) {
        watch::slcd::set_colon();
        self.colon = true;
    }
}

impl WatchFace for StockStopwatchFace {
    fn setup(&mut self, _settings: &Settings, _watch_face_index: usize) {
        unsafe {
            TICKS = 0;
        }
        self.lap_ticks = 0;
        self.blink_ticks = 0;
        self.old_minutes = 0;
        self.old_seconds = 0;
        self.hours = 0;
        self.colon = false;
        if !unsafe { IS_RUNNING } {
            cb_initialize();
        }
    }

    fn activate(&mut self, _settings: &Settings) {
        if unsafe { IS_RUNNING } {
            movement::schedule_background_task(DISTANT_FUTURE);
        }
    }

    fn loop_(&mut self, event: Event, settings: &mut Settings) {
        unsafe {
            while TICKS >= (128 * 60 * 60) {
                TICKS -= 128 * 60 * 60;
                self.hours += 1;
                if self.hours >= 24 {
                    self.hours -= 24;
                }
                self.old_minutes = 59;
            }
        }
        match event {
            Event::Activate => {
                self.set_colon();
                watch::slcd::display_string("ST  ", 0);
                self.update_lap_indicator();
                self.display_ticks(if self.lap_ticks != 0 {
                    self.lap_ticks
                } else {
                    unsafe { TICKS }
                });
            }
            Event::Tick => self.draw(),
            Event::Button(Button::Light, ButtonEvent::LongPress) => {
                self.light_on_button = !self.light_on_button;
                if self.light_on_button {
                    movement::illuminate_led();
                } else {
                    watch::led::set_led_off();
                }
            }
            Event::Button(Button::Alarm, ButtonEvent::Down) => {
                unsafe { IS_RUNNING = !IS_RUNNING };
                if unsafe { IS_RUNNING } {
                    cb_start();
                    movement::schedule_background_task(DISTANT_FUTURE);
                } else {
                    cb_stop();
                    self.set_colon();
                    movement::cancel_background_task();
                }
                self.draw();
                self.button_beep(settings);
            }
            Event::Button(Button::Light, ButtonEvent::Down) => {
                if self.light_on_button {
                    movement::illuminate_led();
                }
                if unsafe { IS_RUNNING } {
                    if self.lap_ticks != 0 {
                        self.lap_ticks = 0;
                    } else {
                        self.lap_ticks = unsafe { TICKS };
                        self.set_colon();
                    }
                } else if self.lap_ticks != 0 {
                    self.lap_ticks = 0;
                } else if unsafe { TICKS } != 0 {
                    unsafe {
                        TICKS = 0;
                    }
                    self.lap_ticks = 0;
                    self.blink_ticks = 0;
                    self.old_minutes = 0;
                    self.old_seconds = 0;
                    self.hours = 0;
                    self.button_beep(settings);
                }
                self.display_ticks(unsafe { TICKS });
                self.update_lap_indicator();
            }
            _ => movement::default_loop_handler(event, settings),
        }
    }

    fn resign(&mut self, _settings: &mut Settings) {
        movement::cancel_background_task();
    }
}
