//! Developer-only minimal firmware boundary for USB feasibility work.
//!
//! This is a Developer-only USB enumeration feasibility image. It retains the
//! bootloader application boundary, reset/fault handling, clock startup, WDT,
//! and device identity, but omits movement, watch faces, and optional drivers.
//! USB transfer support remains fail closed because the PAC USB SRAM contract is
//! incomplete; this image must not be described as a USB shell.

#![cfg_attr(target_arch = "arm", no_std)]
#![cfg_attr(target_arch = "arm", no_main)]
#![allow(dead_code)]

#[cfg(all(
    feature = "minimal-usb",
    any(
        feature = "optical",
        feature = "pro-irda-rx",
        feature = "shell-auth",
        feature = "usb-cdc",
        feature = "defmt-log"
    )
))]
compile_error!(
    "minimal-usb is exclusive: do not combine it with production or optional firmware features"
);

#[cfg(target_arch = "arm")]
use cortex_m_rt::entry;

#[cfg(target_arch = "arm")]
mod minimal {
    use atsaml22j::mclk::RegisterBlock as Mclk;
    use atsaml22j::osc32kctrl::RegisterBlock as Osc32kctrl;
    use atsaml22j::osc32kctrl::rtcctrl::Rtcselselect;
    use atsaml22j::wdt::RegisterBlock as Wdt;
    use atsaml22j::wdt::config::Perselect;

    pub const PROOF_OF_LIFE: &[u8] = b"SENSOR-WATCH MINIMAL USB FEASIBILITY\0";
    pub const APP_START: usize = 0x0000_2000;
    pub const APP_END: usize = 0x0003_C000;

    static mut LAST_FAULT: u8 = 0;
    static mut RESET_REASON: u8 = 0;

    fn wait(mut condition: impl FnMut() -> bool) -> bool {
        for _ in 0..100_000 {
            if condition() {
                return true;
            }
        }
        false
    }

    fn wdt() -> &'static Wdt {
        unsafe { &*atsaml22j::Wdt::PTR }
    }
    fn clock() -> &'static Osc32kctrl {
        unsafe { &*atsaml22j::Osc32kctrl::PTR }
    }
    fn mclk() -> &'static Mclk {
        unsafe { &*atsaml22j::Mclk::PTR }
    }

    pub fn init_wdt() {
        if wdt().ctrla().read().enable().bit_is_set() {
            return;
        }
        wdt()
            .config()
            .modify(|_, w| w.per().variant(Perselect::Cyc2048));
        let _ = wait(|| wdt().syncbusy().read().bits() == 0);
        wdt().ctrla().modify(|_, w| {
            w.enable().set_bit();
            w.alwayson().set_bit()
        });
        let _ = wait(|| wdt().syncbusy().read().bits() == 0);
    }

    pub fn kick_wdt() {
        unsafe {
            wdt().clear().write(|w| w.clear().bits(0xA5));
        }
    }

    pub fn init_clock() {
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
    }

    pub fn record_reset_and_fault() {
        let rcause = unsafe { &*atsaml22j::Rstc::PTR }.rcause().read();
        unsafe {
            RESET_REASON = if rcause.wdt().bit_is_set() {
                1
            } else if rcause.por().bit_is_set() {
                0
            } else {
                3
            };
        }
        if rcause.wdt().bit_is_set() {
            unsafe {
                LAST_FAULT = 1;
            }
        }
    }

    // Fixed-size collection helper avoids allocation in the minimal image.
    type Vec16 = [u8; 16];
    pub fn identity() -> Vec16 {
        let base = 0x0080_A00C as *const u32;
        let words = unsafe {
            [
                core::ptr::read_volatile(base),
                core::ptr::read_volatile(base.add(1)),
                core::ptr::read_volatile(base.add(2)),
                core::ptr::read_volatile(base.add(3)),
            ]
        };
        let mut uid = [0; 16];
        for (i, word) in words.into_iter().enumerate() {
            uid[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        uid
    }

    pub fn run() -> ! {
        init_wdt();
        record_reset_and_fault();
        init_clock();
        let uid = identity();

        core::hint::black_box(PROOF_OF_LIFE.as_ptr());
        core::hint::black_box(uid);
        loop {
            kick_wdt();
            core::hint::black_box(PROOF_OF_LIFE);
        }
    }

    pub fn panic() -> ! {
        unsafe {
            LAST_FAULT = 2;
            RESET_REASON = 2;
        }
        cortex_m::peripheral::SCB::sys_reset()
    }
}

#[cfg(target_arch = "arm")]
#[entry]
fn main() -> ! {
    minimal::run()
}

#[cfg(target_arch = "arm")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    minimal::panic()
}

#[cfg(not(target_arch = "arm"))]
fn main() {}
