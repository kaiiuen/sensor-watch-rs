//! Clock initialization.
//!
//! Port of the clock setup from the Sensor-Watch reference (`hpl_init.c`,
//! `hpl_osc32kctrl.c`, `hpl_mclk.c`). This enables the 32 kHz external crystal
//! oscillator (XOSC32K) and routes its 1 kHz output to the RTC, then enables
//! the RTC's APB clock. The RTC depends on this before it can run.

use atsaml22j::mclk::RegisterBlock as Mclk;
use atsaml22j::osc32kctrl::RegisterBlock as Osc32kctrl;
use atsaml22j::osc32kctrl::rtcctrl::Rtcselselect;

/// Returns a reference to the OSC32KCTRL peripheral register block.
fn osc32kctrl() -> &'static Osc32kctrl {
    // SAFETY: the OSC32KCTRL register block lives at a fixed address for the
    // whole program; this is the standard svd2rust `PTR` access pattern.
    unsafe { &*atsaml22j::Osc32kctrl::PTR }
}

/// Returns a reference to the MCLK peripheral register block.
fn mclk() -> &'static Mclk {
    // SAFETY: the MCLK register block lives at a fixed address for the whole
    // program; this is the standard svd2rust `PTR` access pattern.
    unsafe { &*atsaml22j::Mclk::PTR }
}

/// Enables the 32 kHz external crystal oscillator (XOSC32K).
///
/// Mirrors the reference `_osc32kctrl_init_sources()` with the default config:
/// - XOSC32K enabled, crystal connected (XTALEN)
/// - 1 kHz and 32 kHz outputs enabled
/// - Run in standby
/// - Start-up time 0x3 (1000092 us)
/// - On-demand disabled
fn init_xosc32k() {
    osc32kctrl().xosc32k().modify(|_, w| {
        w.enable().set_bit();
        w.xtalen().set_bit();
        w.en32k().set_bit();
        w.en1k().set_bit();
        w.runstdby().set_bit();
        w.ondemand().clear_bit();
        // SAFETY: 0x3 is a valid STARTUP field value.
        unsafe { w.startup().bits(0x3) }
    });

    // Wait for the oscillator to become ready.
    while !osc32kctrl().status().read().xosc32krdy().bit_is_set() {}
}

/// Routes the XOSC32K 1 kHz output to the RTC and selects the SLCD source.
///
/// Mirrors the reference `hri_osc32kctrl_write_RTCCTRL_reg` and
/// `hri_osc32kctrl_write_SLCDCTRL_SLCDSEL_bit`.
fn init_rtc_source() {
    // RTCSEL = XOSC1K: use the 1 kHz output of the external crystal.
    osc32kctrl()
        .rtcctrl()
        .modify(|_, w| w.rtcsel().variant(Rtcselselect::Xosc1k));
    // SLCD source = 0 (OSCULP32K), matching the reference default.
    osc32kctrl()
        .slcdctrl()
        .modify(|_, w| w.slcdsel().clear_bit());
}

/// Enables the RTC's APB clock in MCLK.
fn enable_rtc_apb() {
    mclk().apbamask().modify(|_, w| w.rtc_().set_bit());
}

/// Enables the Clock Failure Detector (CFD).
///
/// If the 32 kHz external crystal stops oscillating (a mechanical crack or
/// thermal shock), the CFD detects the failure and switches the RTC time base
/// to the internal OSCULP32K so the watch keeps running (slightly less
/// accurately) instead of freezing. `swback` enables the automatic switchover.
pub fn init_cfd() {
    // Enable the CFD and automatic switch-back to the internal oscillator.
    osc32kctrl().cfdctrl().modify(|_, w| {
        w.cfden().set_bit();
        w.swback().set_bit()
    });
}

/// Returns true if the clock failure detector has fired (crystal lost).
pub fn cfd_fired() -> bool {
    // The CFD status is reflected in the OSC32KCTRL STATUS register's
    // CLKFAIL bit. If set, the crystal has failed and the RTC is running
    // on the internal oscillator.
    osc32kctrl().status().read().clkfail().bit_is_set()
}

/// Initializes the clocks required by the RTC.
///
/// This is called from `watch::rtc::init()` before the RTC is configured.
pub fn init() {
    init_xosc32k();
    init_rtc_source();
    enable_rtc_apb();
    // Enable the clock failure detector so a broken crystal doesn't freeze
    // the watch.
    init_cfd();
}
