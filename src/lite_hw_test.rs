//! Opt-in, headless Sensor Watch Lite/Red hardware-test image.
//!
//! This binary is intentionally separate from `sensor-watch`: no LCD, buttons,
//! watch-face, movement, optical, optional-sensor, or production application
//! modules are linked. USB bulk transfers remain fail-closed until proven on a
//! SAM L22 with endpoint/SRAM instrumentation.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]
#![allow(dead_code)]

#[cfg(not(target_arch = "arm"))]
fn main() {}

#[cfg(target_arch = "arm")]
use atsaml22j as _;

#[cfg(target_arch = "arm")]
#[path = "watch/usb.rs"]
mod usb;

#[cfg(target_arch = "arm")]
mod hw {
    use atsaml22j::mclk::RegisterBlock as Mclk;
    use atsaml22j::osc32kctrl::RegisterBlock as Osc32kctrl;
    use atsaml22j::osc32kctrl::rtcctrl::Rtcselselect;
    use atsaml22j::rtc::Mode2;
    use atsaml22j::rtc::mode2::ctrla::{Modeselect, Prescalerselect};
    use atsaml22j::wdt::RegisterBlock as Wdt;
    use atsaml22j::wdt::config::Perselect;
    use core::ptr::{read_volatile, write_volatile};
    use sensor_watch_core::lite_test::{Hardware, Status};

    // Explicitly the only supported image tuple: Red/Lite OSO-SWAT-A1-02.
    pub const BOARD: &str = "Red/Lite";
    pub const REVISION: &str = "OSO-SWAT-A1-02";
    pub const STORAGE_SCRATCH_HARDWARE_PROVEN: bool = false;

    const UID: usize = 0x0080_A00C;
    const PORTA: usize = 0x4100_4400;
    const RED_MASK: u32 = 1 << 20;
    const GREEN_MASK: u32 = 1 << 21;
    const LED_MASK: u32 = RED_MASK | GREEN_MASK;
    const PORT_DIRSET: usize = 0x08;
    const PORT_OUTCLR: usize = 0x14;
    const PORT_OUTSET: usize = 0x18;
    const PORT_PINCFG: usize = 0x40;

    fn wdt() -> &'static Wdt {
        unsafe { &*atsaml22j::Wdt::PTR }
    }
    fn clock() -> &'static Osc32kctrl {
        unsafe { &*atsaml22j::Osc32kctrl::PTR }
    }
    fn rtc() -> &'static Mode2 {
        unsafe { &*atsaml22j::Rtc::PTR }.mode2()
    }
    fn mclk() -> &'static Mclk {
        unsafe { &*atsaml22j::Mclk::PTR }
    }
    fn wait(mut condition: impl FnMut() -> bool) -> bool {
        for _ in 0..100_000 {
            if condition() {
                return true;
            }
        }
        false
    }

    pub fn startup() {
        // Keep the watchdog alive while the crystal starts. Every LED register
        // write uses the SAM L22 SET/CLR offsets, and outputs are made safe
        // before direction is enabled (active-low LEDs therefore start off).
        wdt()
            .config()
            .modify(|_, w| w.per().variant(Perselect::Cyc2048));
        let _ = wait(|| wdt().syncbusy().read().bits() == 0);
        wdt().ctrla().modify(|_, w| {
            w.enable().set_bit();
            w.alwayson().set_bit()
        });
        let _ = wait(|| wdt().syncbusy().read().bits() == 0);
        unsafe {
            write_volatile((PORTA + PORT_OUTSET) as *mut u32, LED_MASK);
            write_volatile((PORTA + PORT_DIRSET) as *mut u32, LED_MASK);
            write_volatile((PORTA + PORT_PINCFG + 20) as *mut u8, 0);
            write_volatile((PORTA + PORT_PINCFG + 21) as *mut u8, 0);
        }
        clock().xosc32k().modify(|_, w| {
            w.enable().set_bit();
            w.xtalen().set_bit();
            w.en32k().set_bit();
            w.en1k().set_bit();
            w.runstdby().set_bit();
            w.ondemand().clear_bit();
            unsafe { w.startup().bits(0x3) }
        });
        let _ = wait(|| clock().status().read().xosc32krdy().bit_is_set());
        clock()
            .rtcctrl()
            .modify(|_, w| w.rtcsel().variant(Rtcselselect::Xosc1k));
        mclk().apbamask().modify(|_, w| w.rtc_().set_bit());
        rtc().ctrla().modify(|_, w| {
            w.mode().variant(Modeselect::Clock);
            w.prescaler().variant(Prescalerselect::Div1024);
            w.clocksync().set_bit();
            w.enable().set_bit()
        });
        let _ = wait(|| rtc().syncbusy().read().bits() == 0);
    }

    pub fn kick_wdt() {
        unsafe {
            wdt().clear().write(|w| w.clear().bits(0xA5));
        }
    }

    pub fn rtc_seconds() -> u8 {
        rtc().clock().read().bits() as u8 & 0x3f
    }

    pub fn heartbeat() {
        unsafe {
            let value = read_volatile((PORTA + 0x10) as *const u32);
            write_volatile(
                (PORTA
                    + if value & GREEN_MASK != 0 {
                        PORT_OUTCLR
                    } else {
                        PORT_OUTSET
                    }) as *mut u32,
                GREEN_MASK,
            );
        }
    }

    pub fn red_activity(error: bool) {
        unsafe {
            write_volatile(
                (PORTA + if error { PORT_OUTCLR } else { PORT_OUTSET }) as *mut u32,
                RED_MASK,
            );
        }
    }

    pub struct LiteHardware;
    impl Hardware for LiteHardware {
        fn identity_present(&self) -> bool {
            let mut nonzero = false;
            for index in 0..4 {
                nonzero |= unsafe { read_volatile((UID + index * 4) as *const u32) != 0 };
            }
            nonzero
        }
        fn rtc_advances(&mut self) -> bool {
            let first = rtc_seconds();
            let mut second = first;
            // A bounded observation window is honest: no change is reported as
            // failure rather than being treated as a working RTC.
            for _ in 0..1_000_000 {
                second = rtc_seconds();
                if second != first {
                    break;
                }
                core::hint::spin_loop();
            }
            first != second
        }
        fn storage_scratch_read(&mut self, _out: &mut [u8; 8]) -> bool {
            // Do not touch flash until the reserved row/offset is confirmed in
            // the production data layout. The host mock proves the transaction.
            STORAGE_SCRATCH_HARDWARE_PROVEN
        }
        fn storage_scratch_write(&mut self, _value: &[u8; 8]) -> bool {
            STORAGE_SCRATCH_HARDWARE_PROVEN
        }
        fn storage_scratch_restore(&mut self, _value: &[u8; 8]) -> bool {
            STORAGE_SCRATCH_HARDWARE_PROVEN
        }
        fn led_cycle(&mut self) -> bool {
            red_activity(true);
            for _ in 0..10_000 {
                core::hint::spin_loop();
            }
            red_activity(false);
            true
        }
    }

    pub fn status() -> Status {
        Status::fresh()
    }
}

#[cfg(target_arch = "arm")]
#[cortex_m_rt::entry]
fn main() -> ! {
    use sensor_watch_core::lite_test::{Command, Status, format_response, run_test};
    hw::startup();
    let mut device = hw::LiteHardware;
    let mut status = Status::fresh();
    // This is the one session transport for the Lite controller. The USB layer
    // synchronizes its configured state and only admits bulk traffic when the
    // explicit hardware proof gate is enabled.
    let mut transport = usb::CdcTransport::new();
    let mut response = [0u8; sensor_watch_core::lite_test::MAX_RESPONSE];
    let mut last_second = hw::rtc_seconds();

    let _ = usb::init();
    loop {
        hw::kick_wdt();
        let _ = usb::poll_transport(&mut transport);
        status.session_active = transport.state() == usb::UsbState::Configured;
        while status.session_active {
            match transport.next_command() {
                Ok(Some(command)) => {
                    let command = match command {
                        usb::ReadOnlyCommand::TestAll => Command::All,
                        usb::ReadOnlyCommand::TestIdentity => Command::Identity,
                        usb::ReadOnlyCommand::TestRtc => Command::Rtc,
                        usb::ReadOnlyCommand::TestStorage => Command::Storage,
                        usb::ReadOnlyCommand::TestLed => Command::Led,
                        usb::ReadOnlyCommand::Status => Command::Status,
                        usb::ReadOnlyCommand::Ping => Command::Ping,
                        usb::ReadOnlyCommand::Help => Command::Help,
                    };
                    hw::red_activity(true);
                    run_test(&mut device, &mut status, command);
                    let length = format_response(command, status, &mut response);
                    let _ = transport.write(&response[..length]);
                    hw::red_activity(false);
                }
                Ok(None) => break,
                Err(_) => {
                    let _ = transport.write(b"ERR unsupported\r\n");
                    hw::red_activity(true);
                    hw::red_activity(false);
                    break;
                }
            }
        }
        if status.session_active {
            let second = hw::rtc_seconds();
            if second != last_second {
                last_second = second;
                hw::heartbeat();
            }
        }
    }
}

#[cfg(target_arch = "arm")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe {
        core::ptr::write_volatile(0x4100_4400 as *mut u32, 1 << 4);
    }
    loop {
        cortex_m::asm::nop();
    }
}
